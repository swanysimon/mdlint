use crate::types::{FileResult, Violation};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct LintResult {
    pub file_results: Vec<FileResult>,
    pub total_errors: usize,
    /// Total number of files that were linted (including those with no violations).
    pub total_files_checked: usize,
}

impl LintResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file_result(
        &mut self,
        path: PathBuf,
        violations: Vec<Violation>,
        source_lines: Vec<String>,
    ) {
        self.total_errors += violations.len();
        self.total_files_checked += 1;
        self.file_results.push(FileResult {
            path,
            violations,
            source_lines,
        });
    }

    /// Record that a file was checked but had no violations.
    pub fn record_clean_file(&mut self) {
        self.total_files_checked += 1;
    }

    pub fn sort_violations(&mut self) {
        for file_result in &mut self.file_results {
            file_result
                .violations
                .sort_by_key(|v| (v.line, v.column, v.rule.clone()));
        }
    }

    pub fn has_errors(&self) -> bool {
        self.total_errors > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_violations() {
        let mut result = LintResult::new();
        result.add_file_result(
            PathBuf::from("test.md"),
            vec![
                // violations in reverse order
                Violation {
                    line: 3,
                    column: Some(1),
                    rule: "MD047".to_string(),
                    message: "Files should end with a single newline character".to_string(),
                    fix: None,
                },
                Violation {
                    line: 3,
                    column: Some(1),
                    rule: "MD041".to_string(),
                    message: "First line in file should be a level 1 heading".to_string(),
                    fix: None,
                },
                Violation {
                    line: 2,
                    column: Some(1),
                    rule: "MD012".to_string(),
                    message: "Multiple consecutive blank lines".to_string(),
                    fix: None,
                },
            ],
            vec![],
        );

        result.sort_violations();

        assert_eq!(result.file_results.len(), 1);
        assert_eq!(result.file_results[0].violations.len(), 3);
        assert_eq!(result.file_results[0].violations[0].rule, "MD012");
        assert_eq!(result.file_results[0].violations[1].rule, "MD041");
        assert_eq!(result.file_results[0].violations[2].rule, "MD047");
    }
}
