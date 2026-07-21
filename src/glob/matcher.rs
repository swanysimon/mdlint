use crate::error::{MarkdownlintError, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::Path;

fn normalize_exclude_pattern(pattern: &str) -> String {
    if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
        pattern.to_string()
    } else {
        format!("**/{pattern}/**")
    }
}

pub struct GlobMatcher {
    includes: GlobSet,
    excludes: GlobSet,
}

impl GlobMatcher {
    #[allow(clippy::missing_errors_doc)]
    pub fn new(patterns: &[String]) -> Result<Self> {
        let mut include_builder = GlobSetBuilder::new();
        let mut exclude_builder = GlobSetBuilder::new();

        for pattern in patterns {
            if let Some(exclude_pattern) = pattern.strip_prefix('#') {
                let normalized = normalize_exclude_pattern(exclude_pattern);
                let glob = Glob::new(&normalized).map_err(|e| {
                    MarkdownlintError::InvalidGlob(format!(
                        "Invalid exclude pattern '{exclude_pattern}': {e}"
                    ))
                })?;
                exclude_builder.add(glob);
            } else {
                let glob = Glob::new(pattern).map_err(|e| {
                    MarkdownlintError::InvalidGlob(format!("Invalid pattern '{pattern}': {e}"))
                })?;
                include_builder.add(glob);
            }
        }

        let includes = include_builder.build().map_err(|e| {
            MarkdownlintError::InvalidGlob(format!("Failed to build include glob set: {e}"))
        })?;

        let excludes = exclude_builder.build().map_err(|e| {
            MarkdownlintError::InvalidGlob(format!("Failed to build exclude glob set: {e}"))
        })?;

        Ok(Self { includes, excludes })
    }

    #[must_use]
    pub fn matches(&self, path: &Path) -> bool {
        if self.excludes.is_match(path) {
            return false;
        }

        if self.includes.is_empty() {
            return true;
        }

        self.includes.is_match(path)
    }

    #[must_use]
    pub fn has_patterns(&self) -> bool {
        !self.includes.is_empty() || !self.excludes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_include_pattern() {
        let matcher = GlobMatcher::new(&["*.md".to_string()]).unwrap();

        assert!(matcher.matches(Path::new("README.md")));
        assert!(matcher.matches(Path::new("docs/guide.md")));
        assert!(!matcher.matches(Path::new("README.txt")));
    }

    #[test]
    fn test_exclude_pattern() {
        let matcher = GlobMatcher::new(&["*.md".to_string(), "#node_modules".to_string()]).unwrap();

        assert!(matcher.matches(Path::new("README.md")));
        assert!(!matcher.matches(Path::new("node_modules/README.md")));
        assert!(!matcher.matches(Path::new("node_modules/package/file.md")));
    }

    #[test]
    fn test_recursive_pattern() {
        let matcher = GlobMatcher::new(&["**/*.md".to_string()]).unwrap();

        assert!(matcher.matches(Path::new("README.md")));
        assert!(matcher.matches(Path::new("docs/guide.md")));
        assert!(matcher.matches(Path::new("docs/api/reference.md")));
        assert!(!matcher.matches(Path::new("README.txt")));
    }

    #[test]
    fn test_multiple_excludes() {
        let matcher = GlobMatcher::new(&[
            "**/*.md".to_string(),
            "#node_modules".to_string(),
            "#target".to_string(),
        ])
        .unwrap();

        assert!(matcher.matches(Path::new("README.md")));
        assert!(matcher.matches(Path::new("docs/guide.md")));
        assert!(!matcher.matches(Path::new("node_modules/file.md")));
        assert!(!matcher.matches(Path::new("target/debug/file.md")));
    }

    #[test]
    fn test_empty_patterns() {
        let matcher = GlobMatcher::new(&[]).unwrap();

        assert!(matcher.matches(Path::new("README.md")));
        assert!(matcher.matches(Path::new("any/file.txt")));
    }

    #[test]
    fn test_has_patterns() {
        let empty_matcher = GlobMatcher::new(&[]).unwrap();
        assert!(!empty_matcher.has_patterns());

        let include_matcher = GlobMatcher::new(&["*.md".to_string()]).unwrap();
        assert!(include_matcher.has_patterns());

        let exclude_matcher = GlobMatcher::new(&["#node_modules".to_string()]).unwrap();
        assert!(exclude_matcher.has_patterns());
    }
}
