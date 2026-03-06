use futures::StreamExt;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::sync::mpsc;

use crate::api::types::{
    ApiError, RECHUNK_STEP_TOOL, RechunkResponse, TokenUsage, WalkthroughStepResponse,
    chat_system_prompt, rechunk_system_prompt, walkthrough_system_prompt,
};
use crate::model::{Message, MessageRole, ReviewMode, Walkthrough};

pub enum ClientStreamEvent {
    StepComplete(WalkthroughStepResponse),
}

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

    async fn tool_use_request<T: DeserializeOwned>(
        &self,
        tool_schema: &str,
        tool_name: &str,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<(T, TokenUsage), ApiError> {
        let tool: serde_json::Value = serde_json::from_str(tool_schema)
            .map_err(|e| ApiError::Parse(format!("invalid tool schema: {}", e)))?;

        let request_body = json!({
            "model": MODEL,
            "max_tokens": 4096,
            "system": system_prompt,
            "tools": [tool],
            "tool_choice": {"type": "tool", "name": tool_name},
            "messages": [
                {
                    "role": "user",
                    "content": user_prompt
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

        let usage = api_response.usage.map(|u| TokenUsage {
            input_tokens:  u.input_tokens,
            output_tokens: u.output_tokens,
        }).unwrap_or_default();

        for content in api_response.content {
            if content.content_type == "tool_use"
                && content.name.as_deref() == Some(tool_name)
                && let Some(input) = content.input
            {
                let parsed = serde_json::from_value(input)
                    .map_err(|e| ApiError::Parse(format!("failed to parse tool input: {}", e)))?;
                return Ok((parsed, usage));
            }
        }

        Err(ApiError::Parse("no tool_use block found in response".to_string()))
    }

    pub async fn rechunk_step(
        &self,
        prompt: &str,
        mode: ReviewMode,
    ) -> Result<(RechunkResponse, TokenUsage), ApiError> {
        self.tool_use_request(
            RECHUNK_STEP_TOOL,
            "rechunk_step",
            rechunk_system_prompt(mode),
            prompt,
        )
        .await
    }

    /// Stream the walkthrough generation, sending complete steps as they're detected.
    /// Uses text mode with assistant prefill for true token-by-token streaming
    /// (tool_use streaming batches the entire response before streaming tokens).
    pub async fn generate_walkthrough_streaming(
        &self,
        diff_prompt: &str,
        mode: ReviewMode,
        event_tx: mpsc::Sender<ClientStreamEvent>,
    ) -> Result<TokenUsage, ApiError> {
        let prefill = r#"{"steps": ["#;

        let request_body = json!({
            "model": MODEL,
            "max_tokens": 4096,
            "system": walkthrough_system_prompt(mode),
            "stream": true,
            "messages": [
                {
                    "role": "user",
                    "content": diff_prompt
                },
                {
                    "role": "assistant",
                    "content": prefill
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

        let mut stream = response.bytes_stream();
        let mut sse_buffer = String::new();
        let mut json_buffer = prefill.to_string();
        let mut step_extractor = StepExtractor::new();
        let mut usage = TokenUsage::default();

        let debug = std::env::var("DOCENT_DEBUG").is_ok();
        let mut debug_log = if debug {
            std::fs::File::create("/tmp/docent-stream.log").ok()
        } else {
            None
        };
        let start = std::time::Instant::now();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            let chunk_str = String::from_utf8_lossy(&chunk);
            if let Some(ref mut f) = debug_log {
                use std::io::Write;
                let _ = writeln!(
                    f,
                    "[{:.1}s] CHUNK len={} sse_buf_before={}",
                    start.elapsed().as_secs_f64(),
                    chunk_str.len(),
                    sse_buffer.len(),
                );
            }
            sse_buffer.push_str(&chunk_str);

            while let Some(event_end) = sse_buffer.find("\n\n") {
                let event_data = sse_buffer[..event_end].to_string();
                sse_buffer = sse_buffer[event_end + 2..].to_string();

                if let Some((input_delta, output_delta)) = parse_sse_usage(&event_data) {
                    usage.input_tokens += input_delta;
                    usage.output_tokens += output_delta;
                }

                if let Some(text) = parse_sse_text_delta(&event_data) {
                    json_buffer.push_str(&text);
                    let steps = step_extractor.feed(&json_buffer);
                    if let Some(ref mut f) = debug_log {
                        use std::io::Write;
                        let _ = writeln!(
                            f,
                            "[{:.1}s] FEED buf={} scan={} state={} found={}",
                            start.elapsed().as_secs_f64(),
                            json_buffer.len(),
                            step_extractor.scan_pos,
                            step_extractor.state_name(),
                            steps.len(),
                        );
                    }
                    for step in steps {
                        if let Some(ref mut f) = debug_log {
                            use std::io::Write;
                            let _ = writeln!(
                                f,
                                "[{:.1}s] EMIT step: {}",
                                start.elapsed().as_secs_f64(),
                                step.title,
                            );
                        }
                        let _ = event_tx.send(ClientStreamEvent::StepComplete(step)).await;
                    }
                }
            }
        }

        if let Some(ref mut f) = debug_log {
            use std::io::Write;
            let _ = writeln!(
                f,
                "[{:.1}s] DONE: buf={} scan={} state={}",
                start.elapsed().as_secs_f64(),
                json_buffer.len(),
                step_extractor.scan_pos,
                step_extractor.state_name(),
            );
        }

        Ok(usage)
    }

    /// Chat about a specific step in the walkthrough with streaming.
    /// Sends text chunks through the provided sender as they arrive.
    /// Returns Ok(()) on success, or an error.
    pub async fn chat_streaming(
        &self,
        walkthrough: &Walkthrough,
        step_index: usize,
        messages: &[Message],
        mode: ReviewMode,
        chunk_tx: mpsc::Sender<String>,
    ) -> Result<TokenUsage, ApiError> {
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
            "system": chat_system_prompt(mode),
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
        let mut usage = TokenUsage::default();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete SSE events in the buffer
            while let Some(event_end) = buffer.find("\n\n") {
                let event_data = buffer[..event_end].to_string();
                buffer = buffer[event_end + 2..].to_string();

                if let Some((input_delta, output_delta)) = parse_sse_usage(&event_data) {
                    usage.input_tokens += input_delta;
                    usage.output_tokens += output_delta;
                }

                if let Some(text) = parse_sse_text_delta(&event_data) {
                    let _ = chunk_tx.send(text).await;
                }
            }
        }

        Ok(usage)
    }
}

/// Parse SSE event data to extract token usage deltas.
/// Returns `(input_delta, output_delta)`.
/// Handles both `message_start` (input + output) and `message_delta` (output only).
fn parse_sse_usage(event: &str) -> Option<(u32, u32)> {
    for line in event.lines() {
        let data = line.strip_prefix("data: ")?;
        let json: serde_json::Value = serde_json::from_str(data).ok()?;
        match json.get("type")?.as_str()? {
            "message_start" => {
                let usage = json.get("message")?.get("usage")?;
                let input = usage.get("input_tokens")?.as_u64()? as u32;
                let output = usage.get("output_tokens")?.as_u64()? as u32;
                return Some((input, output));
            }
            "message_delta" => {
                let output = json.get("usage")?.get("output_tokens")?.as_u64()? as u32;
                return Some((0, output));
            }
            _ => {}
        }
    }
    None
}

/// Parse SSE event data to extract text delta content
fn parse_sse_text_delta(event: &str) -> Option<String> {
    for line in event.lines() {
        if let Some(data) = line.strip_prefix("data: ")
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(data)
            && json.get("type")?.as_str()? == "content_block_delta"
        {
            let delta = json.get("delta")?;
            if delta.get("type")?.as_str()? == "text_delta" {
                return delta.get("text")?.as_str().map(String::from);
            }
        }
    }
    None
}

#[allow(dead_code)]
/// Parse SSE event data to extract input_json_delta content (for streaming tool_use)
fn parse_sse_input_json_delta(event: &str) -> Option<String> {
    for line in event.lines() {
        if let Some(data) = line.strip_prefix("data: ")
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(data)
            && json.get("type")?.as_str()? == "content_block_delta"
        {
            let delta = json.get("delta")?;
            if delta.get("type")?.as_str()? == "input_json_delta" {
                return delta.get("partial_json")?.as_str().map(String::from);
            }
        }
    }
    None
}

#[allow(dead_code)]
struct TitleExtractor {
    scan_pos:     usize,
    in_string:    bool,
    escape_next:  bool,
    titles_found: usize,
}

#[allow(dead_code)]
impl TitleExtractor {
    fn new() -> Self {
        Self {
            scan_pos:     0,
            in_string:    false,
            escape_next:  false,
            titles_found: 0,
        }
    }

    fn feed(&mut self, buffer: &str) -> Vec<String> {
        let mut titles = Vec::new();
        let bytes = buffer.as_bytes();

        while self.scan_pos < bytes.len() {
            if self.escape_next {
                self.escape_next = false;
                self.scan_pos += 1;
                continue;
            }

            let ch = bytes[self.scan_pos];

            if self.in_string {
                if ch == b'\\' {
                    self.escape_next = true;
                } else if ch == b'"' {
                    self.in_string = false;
                }
                self.scan_pos += 1;
                continue;
            }

            if ch == b'"' {
                // Check if this starts a "title" key
                if buffer[self.scan_pos..].starts_with("\"title\"") {
                    let after_key = self.scan_pos + 7;
                    if let Some((value, end_pos)) = extract_json_string_value(buffer, after_key) {
                        titles.push(value);
                        self.titles_found += 1;
                        self.scan_pos = end_pos;
                        continue;
                    }
                    // Value not complete yet — stop and retry on next feed
                    break;
                }
                // Some other string — advance past it
                self.in_string = true;
                self.scan_pos += 1;
                continue;
            }

            self.scan_pos += 1;
        }

        titles
    }
}

#[allow(dead_code)]
fn extract_json_string_value(buffer: &str, from: usize) -> Option<(String, usize)> {
    let bytes = buffer.as_bytes();
    let mut i = from;

    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    i += 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'"' {
        return None;
    }
    i += 1;

    let string_start = i;
    let mut escape = false;
    while i < bytes.len() {
        if escape {
            escape = false;
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            escape = true;
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            let raw = &buffer[string_start..i];
            let json_str = format!("\"{}\"", raw);
            let value = serde_json::from_str::<String>(&json_str).ok()?;
            return Some((value, i + 1));
        }
        i += 1;
    }

    None
}

/// Detects complete step JSON objects within the streaming `steps` array.
/// Tracks brace depth to find complete `{...}` objects, respecting string boundaries.
struct StepExtractor {
    scan_pos:    usize,
    state:       StepExtractorState,
    brace_depth: i32,
    in_string:   bool,
    escape_next: bool,
    obj_start:   Option<usize>,
}

enum StepExtractorState {
    Initial,
    FoundStepsKey,
    InStepsArray,
    Done,
}

impl StepExtractor {
    fn new() -> Self {
        Self {
            scan_pos:    0,
            state:       StepExtractorState::Initial,
            brace_depth: 0,
            in_string:   false,
            escape_next: false,
            obj_start:   None,
        }
    }

    fn state_name(&self) -> &'static str {
        match self.state {
            StepExtractorState::Initial => "Initial",
            StepExtractorState::FoundStepsKey => "FoundStepsKey",
            StepExtractorState::InStepsArray => "InStepsArray",
            StepExtractorState::Done => "Done",
        }
    }

    fn feed(&mut self, buffer: &str) -> Vec<WalkthroughStepResponse> {
        let mut results = Vec::new();
        let bytes = buffer.as_bytes();

        while self.scan_pos < bytes.len() {
            let ch = bytes[self.scan_pos];

            match self.state {
                StepExtractorState::Initial => {
                    if self.escape_next {
                        self.escape_next = false;
                        self.scan_pos += 1;
                        continue;
                    }
                    if self.in_string {
                        if ch == b'\\' {
                            self.escape_next = true;
                        } else if ch == b'"' {
                            self.in_string = false;
                        }
                        self.scan_pos += 1;
                        continue;
                    }
                    if ch == b'"' {
                        let remaining = &buffer[self.scan_pos..];
                        if remaining.starts_with("\"steps\"") {
                            self.state = StepExtractorState::FoundStepsKey;
                            self.scan_pos += 7;
                            continue;
                        }
                        if remaining.len() < 7 {
                            // Not enough data to determine if this is "steps" — wait
                            break;
                        }
                        self.in_string = true;
                    }
                    self.scan_pos += 1;
                }
                StepExtractorState::FoundStepsKey => {
                    if ch == b'[' {
                        self.state = StepExtractorState::InStepsArray;
                        self.brace_depth = 0;
                        self.in_string = false;
                        self.escape_next = false;
                        self.obj_start = None;
                        self.scan_pos += 1;
                        continue;
                    }
                    if !ch.is_ascii_whitespace() && ch != b':' {
                        // Not what we expected; go back to scanning
                        self.state = StepExtractorState::Initial;
                        continue;
                    }
                    self.scan_pos += 1;
                }
                StepExtractorState::InStepsArray => {
                    if self.escape_next {
                        self.escape_next = false;
                        self.scan_pos += 1;
                        continue;
                    }
                    if self.in_string {
                        if ch == b'\\' {
                            self.escape_next = true;
                        } else if ch == b'"' {
                            self.in_string = false;
                        }
                        self.scan_pos += 1;
                        continue;
                    }

                    match ch {
                        b'"' => {
                            self.in_string = true;
                            self.scan_pos += 1;
                        }
                        b'{' => {
                            if self.brace_depth == 0 {
                                self.obj_start = Some(self.scan_pos);
                            }
                            self.brace_depth += 1;
                            self.scan_pos += 1;
                        }
                        b'}' => {
                            self.brace_depth -= 1;
                            self.scan_pos += 1;
                            if self.brace_depth == 0
                                && let Some(start) = self.obj_start.take()
                            {
                                let obj_str = &buffer[start..self.scan_pos];
                                if let Ok(step) =
                                    serde_json::from_str::<WalkthroughStepResponse>(obj_str)
                                {
                                    results.push(step);
                                }
                            }
                        }
                        b']' => {
                            if self.brace_depth == 0 {
                                self.state = StepExtractorState::Done;
                            }
                            self.scan_pos += 1;
                        }
                        _ => {
                            self.scan_pos += 1;
                        }
                    }
                }
                StepExtractorState::Done => {
                    break;
                }
            }
        }

        results
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
    usage:   Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    input_tokens:  u32,
    output_tokens: u32,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_extractor_finds_titles() {
        let mut extractor = TitleExtractor::new();

        let json = r#"{"steps": [{"title": "Add model", "summary": "s""#;
        let titles = extractor.feed(json);
        assert_eq!(titles, vec!["Add model"]);

        let json2 = format!(
            r#"{}, "priority": "normal", "hunk_indices": [1]}}, {{"title": "Update tests", "summary": "t"}}]}}"#,
            json
        );
        let titles2 = extractor.feed(&json2);
        assert_eq!(titles2, vec!["Update tests"]);
        assert_eq!(extractor.titles_found, 2);
    }

    #[test]
    fn title_extractor_ignores_title_in_strings() {
        let mut extractor = TitleExtractor::new();
        // "title" appears inside the summary value — should NOT be extracted
        let json = r#"{"steps": [{"title": "Real title", "summary": "the \"title\" field matters"}]}"#;
        let titles = extractor.feed(json);
        assert_eq!(titles, vec!["Real title"]);
    }

    #[test]
    fn title_extractor_handles_escaped_quotes() {
        let mut extractor = TitleExtractor::new();
        let json = r#"{"steps": [{"title": "Fix \"quote\" issue", "summary": "s"}]}"#;
        let titles = extractor.feed(json);
        assert_eq!(titles, vec![r#"Fix "quote" issue"#]);
    }

    #[test]
    fn title_extractor_incremental_feed() {
        let mut extractor = TitleExtractor::new();

        // Title value not yet complete
        let partial = r#"{"steps": [{"title": "First st"#;
        let titles = extractor.feed(partial);
        assert!(titles.is_empty());

        // Now the title string closes
        let full = format!(r#"{}ep", "summary": "desc""#, partial);
        let titles = extractor.feed(&full);
        assert_eq!(titles, vec!["First step"]);
    }

    #[test]
    fn parse_sse_input_json_delta_extracts_partial_json() {
        let event = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"steps\\\"\"}}";
        let result = parse_sse_input_json_delta(event);
        assert_eq!(result, Some(r#"{"steps""#.to_string()));
    }

    #[test]
    fn parse_sse_input_json_delta_ignores_text_delta() {
        let event = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}";
        assert!(parse_sse_input_json_delta(event).is_none());
    }

    #[test]
    fn step_extractor_finds_complete_steps() {
        let mut extractor = StepExtractor::new();
        let json = r#"{"steps": [{"title": "Add model", "summary": "s", "priority": "normal", "hunk_indices": [1]}]}"#;
        let steps = extractor.feed(json);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].title, "Add model");
        assert_eq!(steps[0].hunk_indices, vec![1]);
    }

    #[test]
    fn step_extractor_incremental() {
        let mut extractor = StepExtractor::new();

        // First step complete, second incomplete
        let partial = r#"{"steps": [{"title": "First", "summary": "s1", "priority": "normal", "hunk_indices": [1]}, {"title": "Sec"#;
        let steps = extractor.feed(partial);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].title, "First");

        // Complete the second step
        let full = format!(
            r#"{}ond", "summary": "s2", "priority": "critical", "hunk_indices": [2, 3]}}]}}"#,
            partial
        );
        let steps2 = extractor.feed(&full);
        assert_eq!(steps2.len(), 1);
        assert_eq!(steps2[0].title, "Second");
        assert_eq!(steps2[0].priority, "critical");
    }

    #[test]
    fn step_extractor_handles_strings_with_braces() {
        let mut extractor = StepExtractor::new();
        let json = r#"{"steps": [{"title": "Handle {braces}", "summary": "has {nested} braces", "priority": "normal", "hunk_indices": [1]}]}"#;
        let steps = extractor.feed(json);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].title, "Handle {braces}");
    }

    #[test]
    fn step_extractor_ignores_steps_key_in_strings() {
        let mut extractor = StepExtractor::new();
        let json = r#"{"note": "the steps key", "steps": [{"title": "Real", "summary": "s", "priority": "normal", "hunk_indices": [1]}]}"#;
        let steps = extractor.feed(json);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].title, "Real");
    }

    #[test]
    fn step_extractor_split_steps_key() {
        let mut extractor = StepExtractor::new();

        // "steps" key split across feeds
        let partial = r#"{"ste"#;
        let steps = extractor.feed(partial);
        assert!(steps.is_empty());

        let full = r#"{"steps": [{"title": "Found", "summary": "s", "priority": "normal", "hunk_indices": [1]}]}"#;
        let steps = extractor.feed(full);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].title, "Found");
    }
}
