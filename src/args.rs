use crate::config::loader::ConfigLoader;
use crate::logger::log_level::LogLevel;
use clap::builder::Styles;
use clap::builder::styling::{AnsiColor, Effects};
use clap::{Parser, ValueEnum, arg};
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
    about = "A fast, flexible, configuration-based command-line interface for linting Markdown files",
    after_help = "For help with a specific command, see: `mdlint help <command>`",
    styles = STYLES,
)]
pub struct Cli {
    // file selection
    #[clap(
        value_name = "FILES",
        help = "List of files or directories to check, or `-` to read from stdin"
    )]
    pub files: Vec<PathBuf>,
    #[arg(
        long,
        help = "List of files and/or directories to omit from analysis, including those passed directly to mdlint",
        help_heading = "File selection"
    )]
    pub exclude: Vec<PathBuf>,
    #[arg(
        long,
        default_value_t = true,
        help = "Respect file exclusions via ignore files like `.gitignore`. Use `--no-respect-ignore` to disable",
        help_heading = "File selection",
        conflicts_with = "no_respect_ignore"
    )]
    pub respect_ignore: bool,
    #[arg(long, hide = true, conflicts_with = "respect_ignore")]
    no_respect_ignore: bool,

    // to fix or not to fix
    #[arg(
        long,
        default_value_t = true,
        help = "Apply fixes to resolve violations. Use `--no-fix` to disable",
        conflicts_with = "no_fix"
    )]
    pub fix: bool,
    #[arg(long, hide = true, conflicts_with = "fix")]
    pub no_fix: bool,

    // rule selection
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        help = "Comma-separated list of rule codes to enable (or ALL, to enable all rules)",
        help_heading = "Rule selection"
    )]
    pub select: Vec<String>,
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        help = "Comma-separated list of rule codes to disable",
        help_heading = "Rule selection"
    )]
    pub ignore: Vec<String>,

    // config file overrides
    #[arg(
        long,
        help = "Path to TOML configuration file (`mdlint.toml`)",
        help_heading = "Configuration options",
        overrides_with = "no_config",
        global = true
    )]
    pub config: Option<PathBuf>,
    #[arg(
        long,
        help = "Ignore all configuration files",
        help_heading = "Configuration options",
        overrides_with = "config",
        global = true
    )]
    pub no_config: bool,

    // verbosity
    #[arg(
        short,
        long,
        help = "Enable verbose logging",
        help_heading = "Log levels",
        conflicts_with_all = ["quiet", "silent"],
        global = true
    )]
    pub verbose: bool,
    #[arg(
        short,
        long,
        help = "Print diagnostics, nothing else",
        help_heading = "Log levels",
        conflicts_with_all = ["verbose", "silent"],
        global = true
    )]
    pub quiet: bool,
    #[arg(
        short,
        long,
        help = "Disable all logging. Exit code will still be 1 when detecting diagnostics or 2 when other issues occur",
        help_heading = "Log levels",
        conflicts_with_all = ["verbose", "quiet"],
        global = true
    )]
    pub silent: bool,

    // color or not
    #[arg(
        long,
        default_value_t = TerminalColor::Auto,
        hide_default_value = true,
        help = "Whether to use terminal colors or not",
    )]
    pub color: TerminalColor,
}

impl Cli {
    /// Get the list of files to process. Returns ["."] if no files specified.
    pub fn files(&self) -> Vec<PathBuf> {
        if self.files.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            self.files.clone()
        }
    }

    pub fn should_fix(&self) -> bool {
        !self.no_fix
    }

    pub fn should_respect_ignore(&self) -> bool {
        !self.no_respect_ignore
    }

    pub fn log_level(&self) -> LogLevel {
        LogLevel::from(self)
    }

    pub fn configuration(&self) -> ConfigLoader {
        ConfigLoader::from(self)
    }
}

impl From<&Cli> for ConfigLoader {
    fn from(args: &Cli) -> Self {
        if args.no_config {
            Self::None
        } else if let Some(config_file) = &args.config {
            Self::File(PathBuf::from(config_file))
        } else {
            Self::Detect
        }
    }
}

impl From<&Cli> for LogLevel {
    fn from(args: &Cli) -> Self {
        if args.silent {
            Self::Silent
        } else if args.quiet {
            Self::Quiet
        } else if args.verbose {
            Self::Verbose
        } else {
            Self::Default
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
        let str = match self {
            TerminalColor::Auto => "auto".to_string(),
            TerminalColor::Always => "always".to_string(),
            TerminalColor::Never => "never".to_string(),
        };
        write!(f, "{}", str)
    }
}
