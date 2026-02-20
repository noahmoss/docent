use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use crate::api::types::{
    ApiError, CreateWalkthroughResponse, CHAT_SYSTEM_PROMPT, CREATE_WALKTHROUGH_TOOL,
    WALKTHROUGH_SYSTEM_PROMPT,
};
use crate::model::{Message, MessageRole, Walkthrough};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-sonnet-4-20250514";

pub struct ClaudeClient {
    api_key: String,
    client: reqwest::Client,
}

impl ClaudeClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    pub fn from_env() -> Result<Self, ApiError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| ApiError::MissingApiKey)?;
        Ok(Self::new(api_key))
    }

    /// Generate a walkthrough from a diff prompt.
    /// Returns the parsed tool call response.
    pub async fn generate_walkthrough(
        &self,
        diff_prompt: &str,
    ) -> Result<CreateWalkthroughResponse, ApiError> {
        let tool: serde_json::Value = serde_json::from_str(CREATE_WALKTHROUGH_TOOL)
            .map_err(|e| ApiError::Parse(format!("invalid tool schema: {}", e)))?;

        let request_body = json!({
            "model": MODEL,
            "max_tokens": 4096,
            "system": WALKTHROUGH_SYSTEM_PROMPT,
            "tools": [tool],
            "tool_choice": {"type": "tool", "name": "create_walkthrough"},
            "messages": [
                {
                    "role": "user",
                    "content": diff_prompt
                }
            ]
        });

        let response = self
            .client
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::ApiResponse {
                status: status.as_u16(),
                message: body,
            });
        }

        let api_response: ApiResponse = response
            .json()
            .await
            .map_err(|e| ApiError::Parse(format!("failed to parse response: {}", e)))?;

        // Find the tool_use content block
        for content in api_response.content {
            if content.content_type == "tool_use"
                && content.name.as_deref() == Some("create_walkthrough")
                && let Some(input) = content.input
            {
                let walkthrough: CreateWalkthroughResponse = serde_json::from_value(input)
                    .map_err(|e| ApiError::Parse(format!("failed to parse tool input: {}", e)))?;
                return Ok(walkthrough);
            }
        }

        Err(ApiError::Parse("no tool_use block found in response".to_string()))
    }

    /// Chat about a specific step in the walkthrough with streaming.
    /// Sends text chunks through the provided sender as they arrive.
    /// Returns Ok(()) on success, or an error.
    pub async fn chat_streaming(
        &self,
        walkthrough: &Walkthrough,
        step_index: usize,
        messages: &[Message],
        chunk_tx: mpsc::Sender<String>,
    ) -> Result<(), ApiError> {
        let step = walkthrough
            .get_step(step_index)
            .ok_or_else(|| ApiError::Parse("invalid step index".to_string()))?;

        // Build walkthrough overview
        let overview: String = walkthrough
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let marker = if i == step_index { " ← current" } else { "" };
                format!("{}. {}{}", i + 1, s.title, marker)
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Build context about the current step
        let step_context = format!(
            "## Walkthrough Overview\n{}\n\n## Current Step: {}\n\n{}\n\n## Code Changes\n\n{}",
            overview,
            step.title,
            step.summary,
            step.hunks
                .iter()
                .map(|h| format!("### {}\n```\n{}\n```", h.file_path, h.content))
                .collect::<Vec<_>>()
                .join("\n\n")
        );

        let full_context = format!("Here is the code change I'm reviewing:\n\n{}", step_context);

        // Convert messages to API format
        let api_messages: Vec<serde_json::Value> = std::iter::once(json!({
            "role": "user",
            "content": full_context
        }))
        .chain(messages.iter().map(|m| {
            json!({
                "role": match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                },
                "content": m.content.clone()
            })
        }))
        .collect();

        let request_body = json!({
            "model": MODEL,
            "max_tokens": 1024,
            "system": CHAT_SYSTEM_PROMPT,
            "stream": true,
            "messages": api_messages
        });

        let response = self
            .client
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::ApiResponse {
                status: status.as_u16(),
                message: body,
            });
        }

        // Process SSE stream
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete SSE events in the buffer
            while let Some(event_end) = buffer.find("\n\n") {
                let event_data = buffer[..event_end].to_string();
                buffer = buffer[event_end + 2..].to_string();

                // Parse SSE event
                if let Some(text) = parse_sse_text_delta(&event_data) {
                    // Send chunk, ignore errors (receiver may have closed)
                    let _ = chunk_tx.send(text).await;
                }
            }
        }

        Ok(())
    }
}

/// Parse SSE event data to extract text delta content
fn parse_sse_text_delta(event: &str) -> Option<String> {
    // Look for data line
    for line in event.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            // Parse JSON
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                // Check for content_block_delta with text_delta
                if json.get("type")?.as_str()? == "content_block_delta" {
                    let delta = json.get("delta")?;
                    if delta.get("type")?.as_str()? == "text_delta" {
                        return delta.get("text")?.as_str().map(String::from);
                    }
                }
            }
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    name: Option<String>,
    input: Option<serde_json::Value>,
    #[allow(dead_code)]
    text: Option<String>,
}
