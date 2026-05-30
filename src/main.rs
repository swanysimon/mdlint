use clap::Parser;
use mdlint::args::{CheckArgs, Cli, Command, FormatArgs, OutputFormat, TerminalColor};
use mdlint::config::loader::{ConfigLoader, find_all_configs};
use mdlint::config::{Config, merge_many_configs};
use mdlint::error::Result;
use mdlint::fix::Fixer;
use mdlint::format::{DefaultFormatter, Formatter, JsonFormatter};
use mdlint::formatter;
use mdlint::glob::FileWalker;
use mdlint::lint::{LintEngine, LintResult};
use mdlint::types::Violation;
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
        Command::Check(args) => run_check(args, config, use_color, cli.verbose),
        Command::Format(args) => run_format(args, config),
        Command::Server(_) => mdlint::server::run_server().map(|()| false),
    }
}

fn run_check(args: &CheckArgs, config: Config, use_color: bool, verbose: bool) -> Result<bool> {
    let excludes = merge_excludes(&args.exclude, &config.exclude);
    let should_fix = args.should_fix().unwrap_or(config.fix);
    let files = find_files(&args.files(), &excludes, args.should_respect_ignore())?;

    if files.is_empty() {
        eprintln!("No markdown files found");
        return Ok(false);
    }

    let lint_result = if args.parallel && !args.no_parallel {
        lint_files_parallel(config, &files, verbose)?
    } else {
        lint_files(config, &files, verbose)?
    };

    if should_fix && lint_result.has_errors() {
        apply_fixes(&lint_result)?;
    }

    let output = match args.output_format {
        OutputFormat::Default => DefaultFormatter::new(use_color).format(&lint_result),
        OutputFormat::Json => JsonFormatter::new(false).format(&lint_result),
    };
    print!("{}", output);

    Ok(lint_result.has_errors())
}

fn run_format(args: &FormatArgs, config: Config) -> Result<bool> {
    let excludes = merge_excludes(&args.exclude, &config.exclude);
    let files = find_files(&args.files(), &excludes, args.should_respect_ignore())?;

    if files.is_empty() {
        eprintln!("No markdown files found");
        return Ok(false);
    }

    let mut any_changed = false;

    for path in &files {
        let original = fs::read_to_string(path)?;
        let formatted = formatter::format(&original);

        if formatted == original {
            continue;
        }

        any_changed = true;

        if args.check {
            eprintln!("Would reformat: {}", path.display());
        } else {
            fs::write(path, &formatted)?;
            eprintln!("Reformatted: {}", path.display());
        }
    }

    Ok(args.check && any_changed)
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

fn merge_excludes(cli_excludes: &[PathBuf], config_excludes: &[String]) -> Vec<PathBuf> {
    let mut excludes: Vec<PathBuf> = cli_excludes.to_vec();
    excludes.extend(config_excludes.iter().map(PathBuf::from));
    excludes
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

type FileOutcome = Result<(PathBuf, Vec<Violation>, Vec<String>)>;

fn lint_files_parallel(config: Config, files: &[PathBuf], verbose: bool) -> Result<LintResult> {
    use rayon::prelude::*;

    let engine = LintEngine::new(config);
    let outcomes: Vec<FileOutcome> = files
        .par_iter()
        .map(|file_path| {
            if verbose {
                eprintln!("Checking: {}", file_path.display());
            }
            let content = fs::read_to_string(file_path)?;
            let violations = engine.lint_content(&content)?;
            let source_lines = content.lines().map(str::to_string).collect();
            Ok((file_path.clone(), violations, source_lines))
        })
        .collect();

    let mut lint_result = LintResult::new();
    for outcome in outcomes {
        let (path, violations, source_lines) = outcome?;
        if violations.is_empty() {
            lint_result.record_clean_file();
        } else {
            lint_result.add_file_result(path, violations, source_lines);
        }
    }
    Ok(lint_result)
}

fn lint_files(config: Config, files: &[PathBuf], verbose: bool) -> Result<LintResult> {
    let engine = LintEngine::new(config);
    let mut lint_result = LintResult::new();

    for file_path in files {
        if verbose {
            eprintln!("Checking: {}", file_path.display());
        }
        let content = fs::read_to_string(file_path)?;
        let violations = engine.lint_content(&content)?;
        if violations.is_empty() {
            lint_result.record_clean_file();
        } else {
            let source_lines: Vec<String> = content.lines().map(str::to_string).collect();
            lint_result.add_file_result(file_path.clone(), violations, source_lines);
        }
    }

    Ok(lint_result)
}

fn should_use_color(color: &TerminalColor) -> bool {
    match color {
        TerminalColor::Always => true,
        TerminalColor::Never => false,
        TerminalColor::Auto => {
            // Respect the NO_COLOR convention (https://no-color.org/)
            if env::var_os("NO_COLOR").is_some() {
                return false;
            }
            io::stdout().is_terminal()
        }
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
