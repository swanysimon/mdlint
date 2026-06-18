mod cli2;
mod js;

use crate::config::{Config, RuleConfig};
use crate::error::{MarkdownlintError, Result};
use crate::migrate::MigrationResult;
use cli2::Cli2Source;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const CANDIDATE_FILE_NAMES: &[&str] = &[
    ".markdownlint-cli2.jsonc",
    ".markdownlint-cli2.json",
    ".markdownlint-cli2.yaml",
    ".markdownlint-cli2.yml",
    ".markdownlint-cli2.cjs",
    ".markdownlint-cli2.mjs",
    ".markdownlint.jsonc",
    ".markdownlint.json",
    ".markdownlint.yaml",
    ".markdownlint.yml",
];

pub fn detect_config_file() -> Result<PathBuf> {
    CANDIDATE_FILE_NAMES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
        .ok_or_else(|| {
            MarkdownlintError::Migrate(
                "No markdownlint-cli2 config found in the current directory. \
                 Pass a path explicitly: `mdlint migrate <path>`"
                    .to_string(),
            )
        })
}

pub fn migrate_file(path: &Path) -> Result<MigrationResult> {
    let content = fs::read_to_string(path)
        .map_err(|e| MarkdownlintError::Migrate(format!("Failed to read {:?}: {}", path, e)))?;

    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let (source, mut extra_warnings) = match extension.as_str() {
        "json" | "jsonc" => (cli2::parse_json(&content, path)?, Vec::new()),
        "yaml" | "yml" => (cli2::parse_yaml(&content, path)?, Vec::new()),
        "cjs" | "mjs" | "js" => js::parse_js(&content, path)?,
        other => {
            return Err(MarkdownlintError::Migrate(format!(
                "Unsupported config file extension {:?}",
                other
            )));
        }
    };

    let mut result = build_config(source);
    extra_warnings.append(&mut result.warnings);
    result.warnings = extra_warnings;
    Ok(result)
}

fn build_config(source: Cli2Source) -> MigrationResult {
    let mut warnings = Vec::new();
    let mut config = Config::default();

    if let Some(fix) = source.fix {
        config.fix = fix;
    }

    if let Some(ignores) = source.ignores {
        config.exclude = ignores;
    }

    if let Some(rule_config) = source.config {
        for (name, value) in rule_config {
            if name == "default" {
                if let Some(enabled) = value.as_bool() {
                    config.default_enabled = enabled;
                }
                continue;
            }

            let Some(code) = crate::migrate::rules::resolve_rule_code(&name) else {
                warnings.push(format!(
                    "Skipped rule {:?}: no mdlint implementation for this rule",
                    name
                ));
                continue;
            };

            config.rules.insert(code, translate_rule_value(&value));
        }
    }

    MigrationResult { config, warnings }
}

fn translate_rule_value(value: &Value) -> RuleConfig {
    match value {
        Value::Bool(enabled) => RuleConfig::Enabled(*enabled),
        Value::Object(map) => {
            let params: HashMap<String, toml::Value> = map
                .iter()
                .filter_map(|(key, value)| json_to_toml(value).map(|v| (key.clone(), v)))
                .collect();
            RuleConfig::Config(params)
        }
        _ => RuleConfig::Enabled(true),
    }
}

fn json_to_toml(value: &Value) -> Option<toml::Value> {
    match value {
        Value::Null => None,
        Value::Bool(b) => Some(toml::Value::Boolean(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(toml::Value::Integer(i))
            } else {
                n.as_f64().map(toml::Value::Float)
            }
        }
        Value::String(s) => Some(toml::Value::String(s.clone())),
        Value::Array(items) => Some(toml::Value::Array(
            items.iter().filter_map(json_to_toml).collect(),
        )),
        Value::Object(map) => Some(toml::Value::Table(
            map.iter()
                .filter_map(|(k, v)| json_to_toml(v).map(|v| (k.clone(), v)))
                .collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn migrates_json_cli2_config() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".markdownlint-cli2.jsonc");
        fs::write(
            &path,
            r#"{
                "config": {
                    "default": true,
                    "MD013": { "line_length": 100 },
                    "heading-style": { "style": "atx" }
                },
                "ignores": ["dist/**"],
                "fix": false
            }"#,
        )
        .unwrap();

        let result = migrate_file(&path).unwrap();
        assert!(result.config.default_enabled);
        assert!(!result.config.fix);
        assert_eq!(result.config.exclude, vec!["dist/**".to_string()]);
        assert!(result.config.rules.contains_key("MD013"));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn warns_on_unknown_rule() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".markdownlint.json");
        fs::write(&path, r#"{ "MD999": true }"#).unwrap();

        let result = migrate_file(&path).unwrap();
        assert_eq!(result.warnings.len(), 1);
        assert!(result.config.rules.is_empty());
    }
}
