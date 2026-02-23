use globset::{Glob, GlobSet, GlobSetBuilder};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FilterError {
    #[error("invalid glob pattern '{0}': {1}")]
    InvalidPattern(String, String),
    #[error("no files match the specified filters")]
    NoMatches,
}

/// A compiled file filter with include and exclude glob patterns.
///
/// Patterns use glob syntax (e.g., "*.clj", "src/**/*.rs", "test_*.py").
/// - If include patterns are specified, a path must match at least one to pass
/// - If exclude patterns are specified, a path must not match any to pass
#[derive(Debug, Clone, Default)]
pub struct FileFilter {
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
}

impl FileFilter {
    /// Create a new filter from include and exclude pattern lists.
    ///
    /// Patterns are compiled immediately, so invalid patterns will return an error.
    pub fn new(include: &[String], exclude: &[String]) -> Result<Self, FilterError> {
        let include = Self::build_glob_set(include)?;
        let exclude = Self::build_glob_set(exclude)?;
        Ok(Self { include, exclude })
    }

    /// Returns true if no patterns are specified.
    pub fn is_empty(&self) -> bool {
        self.include.is_none() && self.exclude.is_none()
    }

    /// Check if a path passes the filter.
    ///
    /// - If include patterns exist, path must match at least one
    /// - If exclude patterns exist, path must not match any
    pub fn matches(&self, path: &str) -> bool {
        // If include patterns specified, path must match at least one
        if let Some(ref set) = self.include
            && !set.is_match(path)
        {
            return false;
        }

        // If exclude patterns specified, path must not match any
        if let Some(ref set) = self.exclude
            && set.is_match(path)
        {
            return false;
        }

        true
    }

    fn build_glob_set(patterns: &[String]) -> Result<Option<GlobSet>, FilterError> {
        if patterns.is_empty() {
            return Ok(None);
        }

        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let glob = Glob::new(pattern)
                .map_err(|e| FilterError::InvalidPattern(pattern.clone(), e.to_string()))?;
            builder.add(glob);
        }

        let set = builder
            .build()
            .map_err(|e| FilterError::InvalidPattern("(combined)".to_string(), e.to_string()))?;

        Ok(Some(set))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_filter_matches_all() {
        let filter = FileFilter::new(&[], &[]).unwrap();
        assert!(filter.is_empty());
        assert!(filter.matches("anything.rs"));
        assert!(filter.matches("src/main.clj"));
    }

    #[test]
    fn test_include_filter() {
        let filter = FileFilter::new(&["*.clj".to_string()], &[]).unwrap();
        assert!(filter.matches("core.clj"));
        assert!(filter.matches("src/main.clj"));
        assert!(!filter.matches("main.rs"));
    }

    #[test]
    fn test_exclude_filter() {
        let filter = FileFilter::new(&[], &["test/*".to_string()]).unwrap();
        assert!(filter.matches("src/main.rs"));
        assert!(!filter.matches("test/main_test.rs"));
    }

    #[test]
    fn test_combined_filter() {
        let filter = FileFilter::new(
            &["*.clj".to_string()],
            &["*_test.clj".to_string()],
        ).unwrap();
        assert!(filter.matches("core.clj"));
        assert!(!filter.matches("core_test.clj"));
        assert!(!filter.matches("main.rs"));
    }

    #[test]
    fn test_invalid_pattern_error() {
        let result = FileFilter::new(&["[invalid".to_string()], &[]);
        assert!(matches!(result, Err(FilterError::InvalidPattern(_, _))));
    }
}
