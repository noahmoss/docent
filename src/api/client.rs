use serde::Deserialize;
use serde_json::json;

use crate::api::types::{
    ApiError, CreateWalkthroughResponse, CREATE_WALKTHROUGH_TOOL, SYSTEM_PROMPT,
};

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
            "system": SYSTEM_PROMPT,
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
            if content.content_type == "tool_use" && content.name.as_deref() == Some("create_walkthrough") {
                if let Some(input) = content.input {
                    let walkthrough: CreateWalkthroughResponse = serde_json::from_value(input)
                        .map_err(|e| ApiError::Parse(format!("failed to parse tool input: {}", e)))?;
                    return Ok(walkthrough);
                }
            }
        }

        Err(ApiError::Parse("no tool_use block found in response".to_string()))
    }
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
}
