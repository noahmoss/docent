//! Search functionality for the diff viewer.

/// A match found in the diff content.
#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Line index (0-based) within the flattened diff content
    pub line: usize,
    /// Start column of the match
    pub start: usize,
    /// End column (exclusive) of the match
    pub end: usize,
}

/// Search state for the diff viewer.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    /// Current search query (None if not searching)
    pub query: Option<String>,
    /// Input buffer while typing search
    pub input: String,
    /// Whether we're in search input mode
    pub active: bool,
    /// All matches found
    pub matches: Vec<SearchMatch>,
    /// Index of the current match (for n/N navigation)
    pub current: usize,
}

impl SearchState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter search input mode
    pub fn start(&mut self) {
        self.active = true;
        self.input.clear();
    }

    /// Cancel search input mode without searching
    pub fn cancel(&mut self) {
        self.active = false;
        self.input.clear();
    }

    /// Execute search with the current input (finalizes and exits input mode)
    pub fn execute(&mut self, lines: &[String]) {
        self.active = false;
        if self.input.is_empty() {
            self.clear();
            return;
        }

        self.query = Some(self.input.clone());
        self.find_matches(lines);
        self.current = 0;
    }

    /// Execute incremental search while typing (stays in input mode)
    pub fn execute_incremental(&mut self, lines: &[String]) {
        if self.input.is_empty() {
            self.query = None;
            self.matches.clear();
            self.current = 0;
            return;
        }

        self.query = Some(self.input.clone());
        self.find_matches(lines);
        self.current = 0;
    }

    /// Clear search state entirely
    pub fn clear(&mut self) {
        self.query = None;
        self.input.clear();
        self.active = false;
        self.matches.clear();
        self.current = 0;
    }

    /// Find all matches for the current query in the given lines
    fn find_matches(&mut self, lines: &[String]) {
        self.matches.clear();

        let query = match &self.query {
            Some(q) => q.to_lowercase(),
            None => return,
        };

        for (line_idx, line) in lines.iter().enumerate() {
            let line_lower = line.to_lowercase();
            let mut search_start = 0;

            while let Some(pos) = line_lower.get(search_start..).and_then(|s| s.find(&query)) {
                let start = search_start + pos;
                let end = start + query.len();

                // Only add match if positions are valid for original line
                if end <= line.len() {
                    self.matches.push(SearchMatch {
                        line: line_idx,
                        start,
                        end,
                    });
                }
                search_start = end;
            }
        }
    }

    /// Move to the next match
    pub fn next_match(&mut self) {
        if !self.matches.is_empty() {
            self.current = (self.current + 1) % self.matches.len();
        }
    }

    /// Move to the previous match
    pub fn prev_match(&mut self) {
        if !self.matches.is_empty() {
            self.current = if self.current == 0 {
                self.matches.len() - 1
            } else {
                self.current - 1
            };
        }
    }

    /// Get the current match, if any
    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.matches.get(self.current)
    }

    /// Check if a given line/column falls within any match
    #[allow(dead_code)]
    pub fn match_at(&self, line: usize, col: usize) -> Option<(usize, bool)> {
        for (idx, m) in self.matches.iter().enumerate() {
            if m.line == line && col >= m.start && col < m.end {
                let is_current = idx == self.current;
                return Some((idx, is_current));
            }
        }
        None
    }

    /// Get match count display string
    pub fn match_count_display(&self) -> String {
        if self.matches.is_empty() {
            if self.query.is_some() {
                "No matches".to_string()
            } else {
                String::new()
            }
        } else {
            format!("{}/{}", self.current + 1, self.matches.len())
        }
    }

    /// Add a character to the search input
    pub fn push_char(&mut self, c: char) {
        self.input.push(c);
    }

    /// Remove the last character from the search input
    pub fn pop_char(&mut self) {
        self.input.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_matches() {
        let lines = vec![
            "fn main() {".to_string(),
            "    let foo = 1;".to_string(),
            "    let bar = foo + 1;".to_string(),
            "}".to_string(),
        ];

        let mut state = SearchState::new();
        state.input = "foo".to_string();
        state.execute(&lines);

        assert_eq!(state.matches.len(), 2);
        assert_eq!(state.matches[0].line, 1);
        assert_eq!(state.matches[1].line, 2);
    }

    #[test]
    fn test_case_insensitive() {
        let lines = vec!["FOO foo Foo".to_string()];

        let mut state = SearchState::new();
        state.input = "foo".to_string();
        state.execute(&lines);

        assert_eq!(state.matches.len(), 3);
    }

    #[test]
    fn test_navigation() {
        let lines = vec!["a a a".to_string()];

        let mut state = SearchState::new();
        state.input = "a".to_string();
        state.execute(&lines);

        assert_eq!(state.matches.len(), 3);
        assert_eq!(state.current, 0);

        state.next_match();
        assert_eq!(state.current, 1);

        state.next_match();
        assert_eq!(state.current, 2);

        state.next_match();
        assert_eq!(state.current, 0); // wraps

        state.prev_match();
        assert_eq!(state.current, 2); // wraps back
    }
}
