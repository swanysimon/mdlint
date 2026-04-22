use crate::config::loader::ConfigLoader;
use crate::logger::log_level::LogLevel;
use clap::builder::Styles;
use clap::builder::styling::{AnsiColor, Effects};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::fmt::Display;
use std::path::PathBuf;

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default());

#[derive(Parser, Debug)]
#[command(
    author,
    name = "mdlint",
    version,
    about = "An opinionated Markdown formatter and linter",
    after_help = "For help with a specific command, see: `mdlint help <command>`",
    styles = STYLES,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(
        long,
        global = true,
        help = "Path to TOML configuration file (`mdlint.toml`)",
        help_heading = "Configuration",
        overrides_with = "no_config"
    )]
    pub config: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        help = "Ignore all configuration files",
        help_heading = "Configuration",
        overrides_with = "config"
    )]
    pub no_config: bool,

    #[arg(
        short,
        long,
        global = true,
        help = "Enable verbose logging",
        help_heading = "Log levels",
        conflicts_with_all = ["quiet", "silent"]
    )]
    pub verbose: bool,

    #[arg(
        short,
        long,
        global = true,
        help = "Print diagnostics, nothing else",
        help_heading = "Log levels",
        conflicts_with_all = ["verbose", "silent"]
    )]
    pub quiet: bool,

    #[arg(
        short,
        long,
        global = true,
        help = "Disable all logging (exit code still reflects result)",
        help_heading = "Log levels",
        conflicts_with_all = ["verbose", "quiet"]
    )]
    pub silent: bool,

    #[arg(
        long,
        global = true,
        default_value_t = TerminalColor::Auto,
        hide_default_value = true,
        help = "Control colors in output"
    )]
    pub color: TerminalColor,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Lint Markdown files and report issues.
    Check(CheckArgs),
    /// Format Markdown files with opinionated style.
    Format(FormatArgs),
    /// Start an LSP server communicating over stdio.
    Server(ServerArgs),
}

#[derive(Args, Debug)]
pub struct ServerArgs {}

#[derive(Args, Debug)]
pub struct CheckArgs {
    #[arg(
        value_name = "FILES",
        help = "Files or directories to check (defaults to current directory)"
    )]
    pub files: Vec<PathBuf>,

    #[arg(
        long,
        help = "Files and directories to exclude from analysis",
        help_heading = "File selection"
    )]
    pub exclude: Vec<PathBuf>,

    #[arg(
        long,
        default_value_t = true,
        help = "Respect `.gitignore` and similar exclusion files. Use `--no-respect-ignore` to disable",
        help_heading = "File selection",
        conflicts_with = "no_respect_ignore"
    )]
    pub respect_ignore: bool,

    #[arg(long, hide = true, conflicts_with = "respect_ignore")]
    pub no_respect_ignore: bool,

    #[arg(long, help = "Apply auto-fixes where possible", overrides_with = "no_fix")]
    pub fix: bool,

    #[arg(long, hide = true, overrides_with = "fix")]
    pub no_fix: bool,

    #[arg(
        long,
        value_name = "FORMAT",
        default_value_t = OutputFormat::Default,
        help = "Output format"
    )]
    pub output_format: OutputFormat,

    #[arg(
        long,
        help = "Lint files in parallel (experimental)",
        help_heading = "Experimental",
        overrides_with = "no_parallel"
    )]
    pub parallel: bool,

    #[arg(long, hide = true, overrides_with = "parallel")]
    pub no_parallel: bool,

    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        help = "Comma-separated list of rules to enable (or `ALL`)",
        help_heading = "Rule selection"
    )]
    pub select: Vec<String>,

    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        help = "Comma-separated list of rules to disable",
        help_heading = "Rule selection"
    )]
    pub ignore: Vec<String>,
}

impl CheckArgs {
    pub fn files(&self) -> Vec<PathBuf> {
        if self.files.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            self.files.clone()
        }
    }

    pub fn should_respect_ignore(&self) -> bool {
        !self.no_respect_ignore
    }

    pub fn should_fix(&self) -> Option<bool> {
        match (self.fix, self.no_fix) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            (false, false) => None,
        }
    }
}

#[derive(Args, Debug)]
pub struct FormatArgs {
    #[arg(
        value_name = "FILES",
        help = "Files or directories to format (defaults to current directory)"
    )]
    pub files: Vec<PathBuf>,

    #[arg(
        long,
        help = "Files and directories to exclude from formatting",
        help_heading = "File selection"
    )]
    pub exclude: Vec<PathBuf>,

    #[arg(
        long,
        default_value_t = true,
        help = "Respect `.gitignore` and similar exclusion files. Use `--no-respect-ignore` to disable",
        help_heading = "File selection",
        conflicts_with = "no_respect_ignore"
    )]
    pub respect_ignore: bool,

    #[arg(long, hide = true, conflicts_with = "respect_ignore")]
    pub no_respect_ignore: bool,

    #[arg(
        long,
        help = "Check formatting without modifying files (exits with 1 if any file would change)"
    )]
    pub check: bool,
}

impl FormatArgs {
    pub fn files(&self) -> Vec<PathBuf> {
        if self.files.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            self.files.clone()
        }
    }

    pub fn should_respect_ignore(&self) -> bool {
        !self.no_respect_ignore
    }
}

impl From<&Cli> for ConfigLoader {
    fn from(cli: &Cli) -> Self {
        if cli.no_config {
            Self::None
        } else if let Some(config_file) = &cli.config {
            Self::File(config_file.clone())
        } else {
            Self::Detect
        }
    }
}

impl From<&Cli> for LogLevel {
    fn from(cli: &Cli) -> Self {
        if cli.silent {
            Self::Silent
        } else if cli.quiet {
            Self::Quiet
        } else if cli.verbose {
            Self::Verbose
        } else {
            Self::Default
        }
    }
}

#[derive(ValueEnum, Debug, Default, Clone)]
pub enum OutputFormat {
    #[default]
    Default,
    Json,
}

impl Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Default => write!(f, "default"),
            OutputFormat::Json => write!(f, "json"),
        }
    }
}

#[derive(ValueEnum, Debug, Default, Clone)]
pub enum TerminalColor {
    #[default]
    Auto,
    Always,
    Never,
}

impl Display for TerminalColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalColor::Auto => write!(f, "auto"),
            TerminalColor::Always => write!(f, "always"),
            TerminalColor::Never => write!(f, "never"),
        }
    }
}
