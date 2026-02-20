use thiserror::Error;

use crate::api::{ApiError, ClaudeClient, CreateWalkthroughResponse, WalkthroughStepResponse};
use crate::diff::{DiffParseError, ParsedDiff};
use crate::model::{Hunk, Message, Priority, Step, Walkthrough};

#[derive(Debug, Error)]
pub enum GenerationError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),

    #[error("diff parsing error: {0}")]
    DiffParse(#[from] DiffParseError),

    #[error("hunk index {0} out of bounds (max: {1})")]
    HunkIndexOutOfBounds(usize, usize),
}

pub struct WalkthroughGenerator {
    parsed_diff: ParsedDiff,
    client: ClaudeClient,
}

impl WalkthroughGenerator {
    pub fn new(diff_text: &str) -> Result<Self, GenerationError> {
        let parsed_diff = ParsedDiff::parse(diff_text)?;
        let client = ClaudeClient::from_env()?;
        Ok(Self {
            parsed_diff,
            client,
        })
    }

    /// Generate a walkthrough from the diff.
    pub async fn generate(&self) -> Result<Walkthrough, GenerationError> {
        let prompt = self.build_prompt();
        let response = self.client.generate_walkthrough(&prompt).await?;
        self.correlate_response(response)
    }

    fn build_prompt(&self) -> String {
        format!(
            "Please analyze this diff and create a code review walkthrough.\n\n\
             The diff contains {} hunks, numbered below:\n\n{}",
            self.parsed_diff.hunks.len(),
            self.parsed_diff.format_for_prompt()
        )
    }

    fn correlate_response(
        &self,
        response: CreateWalkthroughResponse,
    ) -> Result<Walkthrough, GenerationError> {
        let max_index = self.parsed_diff.hunks.len();
        let mut steps = Vec::new();

        for (i, step_response) in response.steps.into_iter().enumerate() {
            let step = self.correlate_step(step_response, i, max_index)?;
            steps.push(step);
        }

        Ok(Walkthrough { steps })
    }

    fn correlate_step(
        &self,
        response: WalkthroughStepResponse,
        step_index: usize,
        max_hunk_index: usize,
    ) -> Result<Step, GenerationError> {
        let mut hunks = Vec::new();

        for &idx in &response.hunk_indices {
            if idx == 0 || idx > max_hunk_index {
                return Err(GenerationError::HunkIndexOutOfBounds(idx, max_hunk_index));
            }

            if let Some(parsed_hunk) = self.parsed_diff.get_hunk(idx) {
                hunks.push(Hunk {
                    file_path: parsed_hunk.file_path.clone(),
                    start_line: parsed_hunk.start_line,
                    end_line: parsed_hunk.end_line,
                    content: parsed_hunk.content.clone(),
                });
            }
        }

        let priority = match response.priority.to_lowercase().as_str() {
            "critical" => Priority::Critical,
            "minor" => Priority::Minor,
            _ => Priority::Normal,
        };

        Ok(Step {
            id: format!("{}", step_index + 1),
            title: response.title.clone(),
            summary: response.summary.clone(),
            priority,
            hunks,
            messages: vec![Message::assistant(&response.summary)],
        })
    }
}
