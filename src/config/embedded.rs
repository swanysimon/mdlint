//! Configuration embedded in another tool's manifest: `[tool.mdlint]` in `pyproject.toml`
//! (the convention ruff, black and mypy use) and a top-level `"mdlint"` key in `package.json`
//! (the convention eslint and prettier use).
//!
//! A manifest without the mdlint section is not an mdlint config at all: parsing returns
//! `Ok(None)` so discovery keeps walking up the directory tree instead of stopping at a
//! `pyproject.toml` that happens to exist.

use crate::config::Config;
use crate::error::{MarkdownlintError, Result};
use std::path::Path;

/// Key holding mdlint settings in both manifests: `[tool.mdlint]` and `{"mdlint": {...}}`.
pub const SECTION: &str = "mdlint";

/// Parse the `[tool.mdlint]` table out of a `pyproject.toml`.
pub fn parse_pyproject(content: &str, path: &Path) -> Result<Option<Config>> {
    let manifest: toml::Value = toml::from_str(content).map_err(|e| config_error(path, &e))?;
    let Some(section) = manifest.get("tool").and_then(|tool| tool.get(SECTION)) else {
        return Ok(None);
    };
    if !section.is_table() {
        return Err(not_a_table(path, "[tool.mdlint] table"));
    }
    section
        .clone()
        .try_into()
        .map(Some)
        .map_err(|e| section_error(path, &e))
}

/// Parse the top-level `"mdlint"` object out of a `package.json`.
pub fn parse_package_json(content: &str, path: &Path) -> Result<Option<Config>> {
    let manifest: serde_json::Value =
        serde_json::from_str(content).map_err(|e| config_error(path, &e))?;
    let Some(section) = manifest.get(SECTION) else {
        return Ok(None);
    };
    if !section.is_object() {
        return Err(not_a_table(path, "\"mdlint\" object"));
    }
    serde_json::from_value(section.clone())
        .map(Some)
        .map_err(|e| section_error(path, &e))
}

fn not_a_table(path: &Path, expected: &str) -> MarkdownlintError {
    MarkdownlintError::Config(format!(
        "Invalid mdlint configuration in {}: expected a {expected}",
        path.display()
    ))
}

fn config_error(path: &Path, error: &dyn std::fmt::Display) -> MarkdownlintError {
    MarkdownlintError::Config(format!("Failed to parse {}: {error}", path.display()))
}

fn section_error(path: &Path, error: &dyn std::fmt::Display) -> MarkdownlintError {
    MarkdownlintError::Config(format!(
        "Failed to parse mdlint configuration in {}: {error}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RuleConfig;
    use indoc::indoc;

    fn path() -> &'static Path {
        Path::new("manifest")
    }

    #[test]
    fn pyproject_reads_tool_mdlint_table() {
        let content = indoc! {r#"
            [project]
            name = "example"

            [tool.ruff]
            line-length = 100

            [tool.mdlint]
            gitignore = false
            exclude = ["docs/generated"]

            [tool.mdlint.rules.MD013]
            line_length = 100
        "#};

        let config = parse_pyproject(content, path()).unwrap().unwrap();
        assert!(!config.gitignore());
        assert_eq!(config.exclude, vec!["docs/generated".to_owned()]);
        match config.rules.get("MD013") {
            Some(RuleConfig::Config(params)) => {
                assert_eq!(params.get("line_length"), Some(&toml::Value::Integer(100)));
            }
            other => panic!("expected MD013 parameters, got {other:?}"),
        }
    }

    #[test]
    fn pyproject_without_section_is_not_a_config() {
        let content = indoc! {r#"
            [project]
            name = "example"

            [tool.ruff]
            line-length = 100
        "#};

        assert!(parse_pyproject(content, path()).unwrap().is_none());
    }

    #[test]
    fn pyproject_defaults_apply_to_omitted_fields() {
        let config = parse_pyproject("[tool.mdlint]\n", path()).unwrap().unwrap();
        assert!(config.default_enabled());
        assert!(config.gitignore());
        assert!(config.fix());
        assert!(config.rules.is_empty());
    }

    #[test]
    fn pyproject_invalid_section_errors() {
        let error = parse_pyproject("[tool]\nmdlint = 5\n", path()).unwrap_err();
        assert!(error.to_string().contains("expected a [tool.mdlint] table"));

        let error = parse_pyproject("[tool.mdlint]\ngitignore = 5\n", path()).unwrap_err();
        assert!(error.to_string().contains("mdlint configuration"));
    }

    #[test]
    fn package_json_reads_mdlint_key() {
        let content = indoc! {r#"
            {
              "name": "example",
              "mdlint": {
                "default_enabled": false,
                "exclude": ["dist"],
                "rules": {
                  "MD013": { "line_length": 100 },
                  "MD033": false
                }
              }
            }
        "#};

        let config = parse_package_json(content, path()).unwrap().unwrap();
        assert!(!config.default_enabled());
        assert_eq!(config.exclude, vec!["dist".to_owned()]);
        assert!(matches!(
            config.rules.get("MD033"),
            Some(RuleConfig::Enabled(false))
        ));
        match config.rules.get("MD013") {
            Some(RuleConfig::Config(params)) => {
                assert_eq!(params.get("line_length"), Some(&toml::Value::Integer(100)));
            }
            other => panic!("expected MD013 parameters, got {other:?}"),
        }
    }

    #[test]
    fn package_json_without_key_is_not_a_config() {
        let content = r#"{"name": "example", "devDependencies": {}}"#;
        assert!(parse_package_json(content, path()).unwrap().is_none());
    }

    #[test]
    fn package_json_invalid_section_errors() {
        let error = parse_package_json(r#"{"mdlint": []}"#, path()).unwrap_err();
        assert!(error.to_string().contains(r#"expected a "mdlint" object"#));

        let error = parse_package_json(r#"{"mdlint": {"gitignore": 5}}"#, path()).unwrap_err();
        assert!(error.to_string().contains("mdlint configuration"));
    }

    #[test]
    fn malformed_manifest_errors() {
        assert!(parse_package_json("{not json", path()).is_err());
        assert!(parse_pyproject("[tool", path()).is_err());
    }
}
