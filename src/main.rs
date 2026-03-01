use clap::Parser;
use markdownlint_rs::args::{CheckArgs, Cli, Command, FormatArgs, OutputFormat, TerminalColor};
use markdownlint_rs::config::loader::{ConfigLoader, find_all_configs};
use markdownlint_rs::config::{Config, merge_many_configs};
use markdownlint_rs::error::Result;
use markdownlint_rs::fix::Fixer;
use markdownlint_rs::format::{DefaultFormatter, Formatter, JsonFormatter};
use markdownlint_rs::glob::FileWalker;
use markdownlint_rs::lint::{LintEngine, LintResult};
use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process;

fn main() {
    process::exit(
        run()
            .map(|had_errors| if had_errors { 1 } else { 0 })
            .unwrap_or(2),
    );
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    let config = load_config(&cli)?;
    let use_color = should_use_color(&cli.color);

    match &cli.command {
        Command::Check(args) => run_check(args, config, use_color),
        Command::Format(args) => run_format(args, config),
    }
}

fn run_check(args: &CheckArgs, config: Config, use_color: bool) -> Result<bool> {
    let files = find_files(&args.files(), &args.exclude, args.should_respect_ignore())?;

    if files.is_empty() {
        eprintln!("No markdown files found");
        return Ok(false);
    }

    let lint_result = lint_files(config, &files)?;

    if args.fix && lint_result.has_errors() {
        apply_fixes(&lint_result)?;
    }

    let output = match args.output_format {
        OutputFormat::Default => DefaultFormatter::new(use_color).format(&lint_result),
        OutputFormat::Json => JsonFormatter::new(false).format(&lint_result),
    };
    print!("{}", output);

    Ok(lint_result.has_errors())
}

fn run_format(args: &FormatArgs, _config: Config) -> Result<bool> {
    let files = find_files(&args.files(), &args.exclude, args.should_respect_ignore())?;

    if files.is_empty() {
        eprintln!("No markdown files found");
        return Ok(false);
    }

    // TODO: implement formatter
    eprintln!(
        "mdlint format: not yet implemented ({} file(s) found)",
        files.len()
    );
    Ok(false)
}

fn load_config(cli: &Cli) -> Result<Config> {
    match ConfigLoader::from(cli) {
        ConfigLoader::Detect => {
            let configs = find_all_configs(&env::current_dir()?)?;
            if configs.is_empty() {
                return Ok(Config::default());
            }
            let config_list: Vec<Config> = configs.into_iter().map(|(_, cfg)| cfg).collect();
            Ok(merge_many_configs(config_list))
        }
        loader => loader.load(),
    }
}

fn find_files(
    paths: &[PathBuf],
    excludes: &[PathBuf],
    respect_ignore: bool,
) -> Result<Vec<PathBuf>> {
    let mut all_files = Vec::new();
    let mut add_to_file = |path: PathBuf| {
        if !all_files.contains(&path) && !is_excluded(&path, excludes) {
            all_files.push(path);
        }
    };

    for path in paths {
        if path.is_dir() {
            let walker = FileWalker::new(respect_ignore);
            walker
                .find_markdown_files(path)?
                .into_iter()
                .for_each(&mut add_to_file);
        } else if path.is_file() {
            add_to_file(path.clone());
        } else {
            eprintln!("Warning: Path not found: {}", path.display());
        }
    }

    Ok(all_files)
}

fn is_excluded(path: &PathBuf, excludes: &[PathBuf]) -> bool {
    excludes.iter().any(|exclude| {
        // Canonicalize the exclude path so relative paths (e.g. "FORMAT_SPEC.md")
        // match against the absolute paths returned by the file walker.
        if let Ok(canonical) = exclude.canonicalize() {
            path == &canonical || path.starts_with(&canonical)
        } else {
            path == exclude || path.starts_with(exclude)
        }
    })
}

fn lint_files(config: Config, files: &[PathBuf]) -> Result<LintResult> {
    let engine = LintEngine::new(config);
    let mut lint_result = LintResult::new();

    for file_path in files {
        let content = fs::read_to_string(file_path)?;
        let violations = engine.lint_content(&content)?;
        if !violations.is_empty() {
            lint_result.add_file_result(file_path.clone(), violations);
        }
    }

    Ok(lint_result)
}

fn should_use_color(color: &TerminalColor) -> bool {
    match color {
        TerminalColor::Always => true,
        TerminalColor::Never => false,
        TerminalColor::Auto => io::stdout().is_terminal(),
    }
}

fn apply_fixes(lint_result: &LintResult) -> Result<()> {
    let fixer = Fixer::new();

    for file_result in &lint_result.file_results {
        let fixes: Vec<_> = file_result
            .violations
            .iter()
            .filter_map(|v| v.fix.clone())
            .collect();

        if fixes.is_empty() {
            continue;
        }

        let content = fs::read_to_string(&file_result.path)?;
        match fixer.apply_fixes_to_content(&content, &fixes) {
            Ok(fixed_content) => {
                fs::write(&file_result.path, fixed_content)?;
                eprintln!("Fixed: {}", file_result.path.display());
            }
            Err(e) => {
                eprintln!(
                    "Failed to apply fixes to {}: {}",
                    file_result.path.display(),
                    e
                );
            }
        }
    }

    Ok(())
}
