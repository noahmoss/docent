use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API returned error: {status} - {message}")]
    ApiResponse { status: u16, message: String },

    #[error("failed to parse API response: {0}")]
    Parse(String),

    #[error("missing API key - set ANTHROPIC_API_KEY environment variable")]
    MissingApiKey,
}

/// Tool schema for Claude to create a walkthrough
pub const CREATE_WALKTHROUGH_TOOL: &str = r#"{
  "name": "create_walkthrough",
  "description": "Create a structured code review walkthrough organizing diff hunks into a narrative sequence of steps",
  "input_schema": {
    "type": "object",
    "properties": {
      "steps": {
        "type": "array",
        "description": "Ordered list of walkthrough steps",
        "items": {
          "type": "object",
          "properties": {
            "title": {
              "type": "string",
              "description": "Short title for this step (e.g., 'Add UserSession model')"
            },
            "summary": {
              "type": "string",
              "description": "Markdown explanation of what this step does and why it matters"
            },
            "priority": {
              "type": "string",
              "enum": ["critical", "normal", "minor"],
              "description": "How important this change is: critical for security/architecture, normal for features, minor for tests/docs"
            },
            "hunk_indices": {
              "type": "array",
              "items": { "type": "integer" },
              "description": "1-based indices of the hunks that belong to this step"
            }
          },
          "required": ["title", "summary", "priority", "hunk_indices"]
        }
      }
    },
    "required": ["steps"]
  }
}"#;

use crate::model::ReviewMode;

const WALKTHROUGH_SYSTEM_PROMPT: &str = r#"You are an expert code reviewer creating a narrative walkthrough of a code change.

Your goal is to organize the diff hunks into a logical sequence that tells a story - not necessarily in file order, but in the order that best helps a reviewer understand the changes.

Guidelines:
- Group related hunks together into steps (e.g., all hunks for a new feature)
- Order steps from foundational changes to dependent changes
- Mark security-critical or architecturally significant changes as "critical"
- Write summaries in markdown, highlighting key points with **bold**
- Each hunk should appear in exactly one step
- Aim for 5-10 steps for a typical PR, but use your judgment based on the complexity of the change
- Focus on describing what the code does and why, not on finding problems

Call the create_walkthrough tool with your structured analysis."#;

const REVIEW_SYSTEM_PROMPT: &str = r#"You are an expert code reviewer performing an opinionated review of a code change.

Your goal is to organize the diff hunks into a logical sequence, and for each step give a candid assessment — call out potential bugs, design issues, missing edge cases, security concerns, and anything that looks wrong or risky.

Guidelines:
- Group related hunks together into steps (e.g., all hunks for a new feature)
- Order steps from foundational changes to dependent changes
- Mark steps with real issues (bugs, security, correctness) as "critical"
- Mark steps that are fine but worth noting as "normal"
- Mark steps with no concerns as "minor"
- Write summaries in markdown, highlighting key points with **bold**
- Each hunk should appear in exactly one step
- Aim for 5-10 steps for a typical PR, but use your judgment based on the complexity of the change
- Be direct and opinionated: if something looks wrong, say so clearly
- Flag: bugs, race conditions, missing error handling, security concerns, behavioral changes that might be unintentional, questionable design choices
- Don't nitpick style or formatting — focus on things that could break or that the author should reconsider

Call the create_walkthrough tool with your structured analysis."#;

const WALKTHROUGH_CHAT_PROMPT: &str = r#"You are an expert code reviewer helping a developer understand a code change.

You are discussing a specific step in a code review walkthrough. The step contains:
- A title and summary explaining what the change does
- The actual code diff (hunks) being reviewed

Answer questions concisely and helpfully. Focus on:
- Explaining what the code does and why
- Providing context about how the change fits into the broader codebase
- Clarifying anything that might be confusing in the diff

Keep responses brief but informative. Only use **bold** and `inline code` for formatting - no headers, lists, or code blocks."#;

const REVIEW_CHAT_PROMPT: &str = r#"You are an expert code reviewer helping a developer with an opinionated code review.

You are discussing a specific step in a code review. The step contains:
- A title and summary with review findings
- The actual code diff (hunks) being reviewed

Answer questions concisely and helpfully. Focus on:
- Explaining potential issues and why they matter
- Flagging bugs, edge cases, or correctness issues visible in the diff
- Calling out things the author should verify (e.g., "does this handle the nil case?", "is this called from multiple threads?")
- Don't nitpick style, naming, or formatting — focus on substance

Keep responses brief but informative. Only use **bold** and `inline code` for formatting - no headers, lists, or code blocks."#;

pub fn walkthrough_system_prompt(mode: ReviewMode) -> &'static str {
    match mode {
        ReviewMode::Walkthrough => WALKTHROUGH_SYSTEM_PROMPT,
        ReviewMode::Review => REVIEW_SYSTEM_PROMPT,
    }
}

pub fn chat_system_prompt(mode: ReviewMode) -> &'static str {
    match mode {
        ReviewMode::Walkthrough => WALKTHROUGH_CHAT_PROMPT,
        ReviewMode::Review => REVIEW_CHAT_PROMPT,
    }
}

/// A step as returned by Claude's tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkthroughStepResponse {
    pub title: String,
    pub summary: String,
    pub priority: String,
    pub hunk_indices: Vec<usize>,
}

/// The full tool call response from Claude
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWalkthroughResponse {
    pub steps: Vec<WalkthroughStepResponse>,
}
