mod markdownlint_cli2;

use crate::config::Config;
use crate::error::Result;
use std::fmt::Display;
use std::path::{Path, PathBuf};

/// A linter/formatter mdlint knows how to migrate a config from.
///
/// Detection order matters: `detect_source` tries each source in declaration order, so
/// list more specific sources (whose config filenames are unlikely to collide) before
/// more general ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    MarkdownlintCli2,
}

impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::MarkdownlintCli2 => write!(f, "markdownlint-cli2"),
        }
    }
}

/// A config file belonging to a detected `Source`, along with any human-readable
/// warnings about settings that couldn't be migrated automatically.
pub struct Migration {
    pub source: Source,
    pub config_path: PathBuf,
    pub config: Config,
    pub warnings: Vec<String>,
}

/// Search `dir` for a config file belonging to a known source, without requiring the
/// caller to know which tool they're migrating from.
pub fn detect_source(dir: &Path) -> Option<(Source, PathBuf)> {
    markdownlint_cli2::detect(dir).map(|path| (Source::MarkdownlintCli2, path))
}

/// Convert a detected source's config file into an equivalent mdlint `Config`.
pub fn migrate(source: Source, config_path: &Path) -> Result<Migration> {
    let (config, warnings) = match source {
        Source::MarkdownlintCli2 => markdownlint_cli2::migrate(config_path)?,
    };

    Ok(Migration {
        source,
        config_path: config_path.to_path_buf(),
        config,
        warnings,
    })
}
