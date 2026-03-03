use thiserror::Error;

use crate::api::{
    ApiError, ClaudeClient, CreateWalkthroughResponse, RechunkResponse, WalkthroughStepResponse,
};
use crate::diff::{DiffParseError, FileFilter, ParsedDiff};
use crate::model::{Hunk, Message, Priority, ReviewMode, Step, Walkthrough};

#[derive(Debug, Error)]
pub enum GenerationError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),

    #[error("diff parsing error: {0}")]
    DiffParse(#[from] DiffParseError),

    #[error("hunk index {0} out of bounds (max: {1})")]
    HunkIndexOutOfBounds(usize, usize),

    #[error("line range {start}..={end} out of bounds (hunk has {total} content lines)")]
    RangeOutOfBounds {
        start: usize,
        end: usize,
        total: usize,
    },
}

pub struct WalkthroughGenerator {
    parsed_diff: ParsedDiff,
    client: ClaudeClient,
    mode: ReviewMode,
}

impl WalkthroughGenerator {
    #[allow(dead_code)]
    pub fn new(diff_text: &str) -> Result<Self, GenerationError> {
        Self::with_filter(diff_text, &FileFilter::default(), ReviewMode::default())
    }

    pub fn with_filter(
        diff_text: &str,
        filter: &FileFilter,
        mode: ReviewMode,
    ) -> Result<Self, GenerationError> {
        let mut parsed_diff = ParsedDiff::parse(diff_text)?;
        parsed_diff.apply_filter(filter)?;

        let client = ClaudeClient::from_env()?;
        Ok(Self {
            parsed_diff,
            client,
            mode,
        })
    }

    pub async fn generate(&self) -> Result<Walkthrough, GenerationError> {
        let prompt = self.build_prompt();
        let response = self.client.generate_walkthrough(&prompt, self.mode).await?;
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

        let priority = Priority::parse(&response.priority);

        Ok(Step {
            id: format!("{}", step_index + 1),
            title: response.title.clone(),
            summary: response.summary.clone(),
            priority,
            hunks,
            messages: vec![Message::assistant(&response.summary)],
            depth: 0,
        })
    }
}

/// Format a step's hunks with numbered content lines for the rechunk prompt.
/// The @@ header is shown without a number; content lines are numbered 1-based per hunk.
pub fn format_step_for_rechunk(step: &Step) -> String {
    let mut output = String::new();
    for (i, hunk) in step.hunks.iter().enumerate() {
        let lines: Vec<&str> = hunk.content.lines().collect();
        let content_lines: Vec<&str> = if lines.first().is_some_and(|l| l.starts_with("@@")) {
            lines[1..].to_vec()
        } else {
            lines.clone()
        };

        output.push_str(&format!(
            "=== Hunk {} ({}, lines {}-{}) ===\n",
            i + 1,
            hunk.file_path,
            hunk.start_line,
            hunk.end_line
        ));

        if let Some(header) = lines.first()
            && header.starts_with("@@")
        {
            output.push_str(header);
            output.push('\n');
        }

        for (j, line) in content_lines.iter().enumerate() {
            output.push_str(&format!("{:>4} | {}\n", j + 1, line));
        }
        output.push('\n');
    }
    output
}

/// Parse a @@ header to extract old and new starting line numbers.
fn parse_hunk_header(header: &str) -> Option<(usize, usize)> {
    // Format: @@ -A,B +C,D @@ optional text
    let header = header.strip_prefix("@@ ")?;
    let parts: Vec<&str> = header.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return None;
    }

    let old_start = parts[0]
        .strip_prefix('-')?
        .split(',')
        .next()?
        .parse::<usize>()
        .ok()?;

    let new_start = parts[1]
        .strip_prefix('+')?
        .split(',')
        .next()?
        .parse::<usize>()
        .ok()?;

    Some((old_start, new_start))
}

/// Slice a hunk by extracting content lines in the range `start_line..=end_line` (1-based).
/// Reconstructs a valid Hunk with correct @@ header line numbers.
pub fn slice_hunk(
    hunk: &Hunk,
    start_line: usize,
    end_line: usize,
) -> Result<Hunk, GenerationError> {
    let all_lines: Vec<&str> = hunk.content.lines().collect();

    // Separate @@ header from content lines
    let (header, content_lines) = if all_lines.first().is_some_and(|l| l.starts_with("@@")) {
        (Some(all_lines[0]), &all_lines[1..])
    } else {
        (None, all_lines.as_slice())
    };

    let total = content_lines.len();
    if start_line == 0 || end_line == 0 || start_line > end_line || end_line > total {
        return Err(GenerationError::RangeOutOfBounds {
            start: start_line,
            end: end_line,
            total,
        });
    }

    let selected = &content_lines[(start_line - 1)..end_line];

    // Count lines before the selection to compute new header offsets
    let (old_start, new_start) = header
        .and_then(parse_hunk_header)
        .unwrap_or((hunk.start_line, hunk.start_line));

    let prefix = &content_lines[..(start_line - 1)];
    let old_offset: usize = prefix.iter().filter(|l| !l.starts_with('+')).count();
    let new_offset: usize = prefix.iter().filter(|l| !l.starts_with('-')).count();

    let old_count: usize = selected.iter().filter(|l| !l.starts_with('+')).count();
    let new_count: usize = selected.iter().filter(|l| !l.starts_with('-')).count();

    let new_header = format!(
        "@@ -{},{} +{},{} @@",
        old_start + old_offset,
        old_count,
        new_start + new_offset,
        new_count
    );

    let mut content = new_header;
    for line in selected {
        content.push('\n');
        content.push_str(line);
    }

    Ok(Hunk {
        file_path: hunk.file_path.clone(),
        start_line: hunk.start_line + new_offset,
        end_line: hunk.start_line + new_offset + new_count.saturating_sub(1),
        content,
    })
}

/// Create sub-steps from a rechunk API response by slicing the original step's hunks.
pub fn create_sub_steps(
    step: &Step,
    response: RechunkResponse,
    base_id: &str,
) -> Result<Vec<Step>, GenerationError> {
    let mut sub_steps = Vec::new();

    for (i, step_response) in response.steps.into_iter().enumerate() {
        let mut hunks = Vec::new();

        for range in &step_response.ranges {
            let hunk_idx = range.hunk_index;
            if hunk_idx == 0 || hunk_idx > step.hunks.len() {
                return Err(GenerationError::HunkIndexOutOfBounds(
                    hunk_idx,
                    step.hunks.len(),
                ));
            }
            let source_hunk = &step.hunks[hunk_idx - 1];
            let sliced = slice_hunk(source_hunk, range.start_line, range.end_line)?;
            hunks.push(sliced);
        }

        let priority = match step_response.priority.to_lowercase().as_str() {
            "critical" => Priority::Critical,
            "minor" => Priority::Minor,
            _ => Priority::Normal,
        };

        sub_steps.push(Step {
            id: format!("{}.{}", base_id, i + 1),
            title: step_response.title,
            summary: step_response.summary.clone(),
            priority,
            hunks,
            messages: vec![Message::assistant(&step_response.summary)],
            depth: 0,
        });
    }

    Ok(sub_steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hunk(content: &str) -> Hunk {
        Hunk {
            file_path: "src/foo.rs".to_string(),
            start_line: 1,
            end_line: 10,
            content: content.to_string(),
        }
    }

    #[test]
    fn test_parse_hunk_header() {
        assert_eq!(parse_hunk_header("@@ -0,0 +1,25 @@"), Some((0, 1)));
        assert_eq!(parse_hunk_header("@@ -15,6 +15,21 @@"), Some((15, 15)));
        assert_eq!(
            parse_hunk_header("@@ -10,3 +10,18 @@ fn something"),
            Some((10, 10))
        );
        assert_eq!(parse_hunk_header("not a header"), None);
    }

    #[test]
    fn test_slice_hunk_basic() {
        let hunk = make_hunk(
            "@@ -0,0 +1,5 @@\n\
             +line one\n\
             +line two\n\
             +line three\n\
             +line four\n\
             +line five",
        );

        let sliced = slice_hunk(&hunk, 1, 3).unwrap();
        assert!(sliced.content.contains("+line one"));
        assert!(sliced.content.contains("+line three"));
        assert!(!sliced.content.contains("+line four"));

        let sliced2 = slice_hunk(&hunk, 4, 5).unwrap();
        assert!(sliced2.content.contains("+line four"));
        assert!(sliced2.content.contains("+line five"));
        assert!(!sliced2.content.contains("+line one"));
    }

    #[test]
    fn test_slice_hunk_out_of_bounds() {
        let hunk = make_hunk("@@ -0,0 +1,3 @@\n+a\n+b\n+c");

        assert!(slice_hunk(&hunk, 0, 2).is_err());
        assert!(slice_hunk(&hunk, 2, 1).is_err());
        assert!(slice_hunk(&hunk, 1, 4).is_err());
    }

    #[test]
    fn test_slice_hunk_header_line_numbers() {
        let hunk = make_hunk(
            "@@ -0,0 +1,4 @@\n\
             +aaa\n\
             +bbb\n\
             +ccc\n\
             +ddd",
        );

        let sliced = slice_hunk(&hunk, 3, 4).unwrap();
        // The sliced hunk should start at +3 (2 lines skipped)
        assert!(sliced.content.starts_with("@@ -0,0 +3,2 @@"));
    }

    #[test]
    fn test_create_sub_steps() {
        let step = Step {
            id: "3".to_string(),
            title: "Big step".to_string(),
            summary: "A big step".to_string(),
            priority: Priority::Normal,
            hunks: vec![make_hunk("@@ -0,0 +1,4 @@\n+aaa\n+bbb\n+ccc\n+ddd")],
            messages: vec![],
            depth: 0,
        };

        let response = RechunkResponse {
            steps: vec![
                crate::api::RechunkStepResponse {
                    title: "First part".to_string(),
                    summary: "Lines 1-2".to_string(),
                    priority: "normal".to_string(),
                    ranges: vec![crate::api::HunkRange {
                        hunk_index: 1,
                        start_line: 1,
                        end_line: 2,
                    }],
                },
                crate::api::RechunkStepResponse {
                    title: "Second part".to_string(),
                    summary: "Lines 3-4".to_string(),
                    priority: "normal".to_string(),
                    ranges: vec![crate::api::HunkRange {
                        hunk_index: 1,
                        start_line: 3,
                        end_line: 4,
                    }],
                },
            ],
        };

        let sub_steps = create_sub_steps(&step, response, "3").unwrap();
        assert_eq!(sub_steps.len(), 2);
        assert_eq!(sub_steps[0].id, "3.1");
        assert_eq!(sub_steps[1].id, "3.2");
        assert_eq!(sub_steps[0].title, "First part");
        assert!(sub_steps[0].hunks[0].content.contains("+aaa"));
        assert!(!sub_steps[0].hunks[0].content.contains("+ccc"));
        assert!(sub_steps[1].hunks[0].content.contains("+ccc"));
    }

    #[test]
    fn test_format_step_for_rechunk() {
        let step = Step {
            id: "1".to_string(),
            title: "Test".to_string(),
            summary: "Test".to_string(),
            priority: Priority::Normal,
            hunks: vec![make_hunk("@@ -0,0 +1,2 @@\n+hello\n+world")],
            messages: vec![],
            depth: 0,
        };

        let output = format_step_for_rechunk(&step);
        assert!(output.contains("=== Hunk 1"));
        assert!(output.contains("1 | +hello"));
        assert!(output.contains("2 | +world"));
    }
}
