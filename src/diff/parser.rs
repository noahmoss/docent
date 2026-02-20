use thiserror::Error;
use unidiff::PatchSet;

#[derive(Debug, Error)]
pub enum DiffParseError {
    #[error("empty diff")]
    EmptyDiff,
    #[error("failed to parse diff: {0}")]
    ParseError(String),
}

/// A single hunk from a unified diff, with an assigned index for Claude reference.
#[derive(Debug, Clone)]
pub struct ParsedHunk {
    /// 1-based index for Claude to reference
    pub index: usize,
    /// Path to the file being modified
    pub file_path: String,
    /// Starting line number in the new file
    pub start_line: usize,
    /// Ending line number in the new file
    pub end_line: usize,
    /// Raw hunk content including the @@ header
    pub content: String,
}

/// A parsed unified diff containing indexed hunks.
#[derive(Debug, Clone)]
pub struct ParsedDiff {
    pub hunks: Vec<ParsedHunk>,
}

impl ParsedDiff {
    /// Parse a unified diff string into indexed hunks.
    pub fn parse(diff_text: &str) -> Result<Self, DiffParseError> {
        let diff_text = diff_text.trim();
        if diff_text.is_empty() {
            return Err(DiffParseError::EmptyDiff);
        }

        let mut patch_set = PatchSet::new();
        patch_set
            .parse(diff_text)
            .map_err(|e| DiffParseError::ParseError(e.to_string()))?;

        let mut hunks = Vec::new();
        let mut index = 1usize;

        for patched_file in patch_set {
            let file_path = patched_file.target_file.trim_start_matches("b/").to_string();

            for hunk in patched_file {
                let start_line = hunk.target_start as usize;
                let length = hunk.target_length as usize;
                let end_line = if length > 0 {
                    start_line + length - 1
                } else {
                    start_line
                };

                hunks.push(ParsedHunk {
                    index,
                    file_path: file_path.clone(),
                    start_line,
                    end_line,
                    content: hunk.to_string(),
                });
                index += 1;
            }
        }

        if hunks.is_empty() {
            return Err(DiffParseError::ParseError("no hunks found".to_string()));
        }

        Ok(Self { hunks })
    }

    /// Get a hunk by its 1-based index.
    pub fn get_hunk(&self, index: usize) -> Option<&ParsedHunk> {
        self.hunks.iter().find(|h| h.index == index)
    }

    /// Format hunks for inclusion in a Claude prompt.
    pub fn format_for_prompt(&self) -> String {
        self.hunks
            .iter()
            .map(|h| {
                format!(
                    "=== Hunk {} ({}, lines {}-{}) ===\n{}",
                    h.index, h.file_path, h.start_line, h.end_line, h.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_diff() {
        let diff = r#"diff --git a/src/main.rs b/src/main.rs
index abc123..def456 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,7 @@
 fn main() {
+    println!("Hello");
     let x = 1;
+    let y = 2;
     println!("{}", x);
 }
"#;
        let parsed = ParsedDiff::parse(diff).unwrap();
        assert_eq!(parsed.hunks.len(), 1);
        assert_eq!(parsed.hunks[0].index, 1);
        assert_eq!(parsed.hunks[0].file_path, "src/main.rs");
    }

    #[test]
    fn test_parse_multiple_files() {
        let diff = r#"diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,2 +1,3 @@
+// File A
 fn a() {}
 fn a2() {}
diff --git a/src/b.rs b/src/b.rs
--- a/src/b.rs
+++ b/src/b.rs
@@ -5,2 +5,3 @@
 fn b() {}
+fn b2() {}
 fn b3() {}
"#;
        let parsed = ParsedDiff::parse(diff).unwrap();
        assert_eq!(parsed.hunks.len(), 2);
        assert_eq!(parsed.hunks[0].file_path, "src/a.rs");
        assert_eq!(parsed.hunks[1].file_path, "src/b.rs");
    }

    #[test]
    fn test_empty_diff_error() {
        let result = ParsedDiff::parse("");
        assert!(matches!(result, Err(DiffParseError::EmptyDiff)));
    }

    #[test]
    fn test_format_for_prompt() {
        let diff = r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,2 +1,3 @@
+use std::io;
 fn main() {}
"#;
        let parsed = ParsedDiff::parse(diff).unwrap();
        let prompt = parsed.format_for_prompt();
        assert!(prompt.contains("=== Hunk 1 (src/main.rs"));
    }
}
