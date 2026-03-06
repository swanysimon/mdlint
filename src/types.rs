use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct FileResult {
    pub path: PathBuf,
    pub violations: Vec<Violation>,
    /// Source lines (1-indexed by line number) used for snippet display.
    /// May be empty if source is not available.
    pub source_lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Violation {
    pub line: usize,
    pub column: Option<usize>,
    pub rule: String,
    pub message: String,
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone)]
pub struct Fix {
    pub line_start: usize,
    pub line_end: usize,
    pub column_start: Option<usize>,
    pub column_end: Option<usize>,
    pub replacement: String,
    pub description: String,
}
