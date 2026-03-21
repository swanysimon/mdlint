use thiserror::Error;

#[derive(Error, Debug)]
pub enum MarkdownlintError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid glob pattern: {0}")]
    InvalidGlob(String),

    #[error("Invalid regex pattern: {0}")]
    InvalidRegex(#[from] regex::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Fix error: {0}")]
    Fix(String),

    #[error("LSP error: {0}")]
    Lsp(String),
}

pub type Result<T> = std::result::Result<T, MarkdownlintError>;
