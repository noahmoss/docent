use serde::{Deserialize, Serialize};

use crate::model::{ReviewMode, Step, Walkthrough};
use crate::session::SessionState;

// --- Client → Server ---

#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigateAction {
    Next,
    Prev,
    Goto,
}

#[derive(Debug, Deserialize)]
pub struct NavigateParams {
    pub action: NavigateAction,
    pub step: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageParams {
    pub content: String,
}

// --- Server → Client ---

#[derive(Debug, Serialize)]
pub struct Response {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(id: u64, result: impl Serialize) -> Self {
        Self {
            id,
            result: Some(serde_json::to_value(result).unwrap_or(serde_json::Value::Null)),
            error: None,
        }
    }

    pub fn error(id: u64, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(message.into()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Notification {
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl Notification {
    pub fn new(method: &str, params: impl Serialize) -> Self {
        Self {
            method: method.to_string(),
            params: Some(serde_json::to_value(params).unwrap_or(serde_json::Value::Null)),
        }
    }

    pub fn bare(method: &str) -> Self {
        Self {
            method: method.to_string(),
            params: None,
        }
    }

    pub fn state_changed(state: &SessionState) -> Self {
        Self::new("state_changed", serde_json::json!({ "state": state }))
    }

    pub fn step_changed(
        current_step: usize,
        step: &Step,
        reviewed: &[bool],
        walkthrough_complete: bool,
    ) -> Self {
        Self::new(
            "step_changed",
            serde_json::json!({
                "current_step": current_step,
                "step": step,
                "reviewed": reviewed,
                "walkthrough_complete": walkthrough_complete,
            }),
        )
    }

    pub fn walkthrough_loaded(walkthrough: &Walkthrough, reviewed: &[bool]) -> Self {
        Self::new(
            "walkthrough_loaded",
            serde_json::json!({
                "walkthrough": walkthrough,
                "reviewed": reviewed,
            }),
        )
    }

    pub fn chat_chunk(step_index: usize, chunk: &str) -> Self {
        Self::new(
            "chat_chunk",
            serde_json::json!({
                "step_index": step_index,
                "chunk": chunk,
            }),
        )
    }

    pub fn chat_complete(step_index: usize) -> Self {
        Self::new(
            "chat_complete",
            serde_json::json!({ "step_index": step_index }),
        )
    }

    pub fn rechunk_complete(steps: &[Step], current_step: usize, reviewed: &[bool]) -> Self {
        Self::new(
            "rechunk_complete",
            serde_json::json!({
                "steps": steps,
                "current_step": current_step,
                "reviewed": reviewed,
            }),
        )
    }

    pub fn error(message: &str) -> Self {
        Self::new("error", serde_json::json!({ "message": message }))
    }

    pub fn step_added(step: &Step, index: usize, reviewed: &[bool]) -> Self {
        Self::new(
            "step_added",
            serde_json::json!({
                "step": step,
                "index": index,
                "reviewed": reviewed,
            }),
        )
    }

    pub fn generation_complete() -> Self {
        Self::bare("generation_complete")
    }

    pub fn shutdown() -> Self {
        Self::bare("shutdown")
    }
}

#[derive(Debug, Serialize)]
pub struct StateSnapshot {
    pub state: SessionState,
    pub review_mode: ReviewMode,
    pub current_step: usize,
    pub walkthrough: Walkthrough,
    pub reviewed: Vec<bool>,
    pub walkthrough_complete: bool,
    pub generation_in_progress: bool,
    pub chat_pending: Option<usize>,
    pub rechunk_pending: bool,
}
