mod markdownlint_cli2;
mod rules;
mod write;

use crate::args::{MigrateArgs, MigrateFrom};
use crate::config::Config;
use crate::error::{MarkdownlintError, Result};
use std::fs;

pub struct MigrationResult {
    pub config: Config,
    pub warnings: Vec<String>,
}

pub fn run_migrate(args: &MigrateArgs) -> Result<bool> {
    let input = match &args.input {
        Some(path) => path.clone(),
        None => match args.from {
            MigrateFrom::MarkdownlintCli2 => markdownlint_cli2::detect_config_file()?,
        },
    };

    let result = match args.from {
        MigrateFrom::MarkdownlintCli2 => markdownlint_cli2::migrate_file(&input)?,
    };

    for warning in &result.warnings {
        eprintln!("warning: {warning}");
    }

    let rendered = write::render(&result.config);

    if args.dry_run {
        print!("{rendered}");
        return Ok(false);
    }

    if args.output.exists() && !args.force {
        return Err(MarkdownlintError::Migrate(format!(
            "{:?} already exists; pass --force to overwrite it",
            args.output
        )));
    }

    fs::write(&args.output, rendered)?;
    eprintln!(
        "Migrated {:?} -> {:?}",
        input.display().to_string(),
        args.output.display().to_string()
    );

    Ok(false)
}
