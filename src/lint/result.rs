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

    pub fn has_errors(&self) -> bool {
        self.total_errors > 0
    }
}
