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
    "package.json",
];

pub fn detect_config_file() -> Result<PathBuf> {
    CANDIDATE_FILE_NAMES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists() && is_usable_candidate(path))
        .ok_or_else(|| {
            MarkdownlintError::Migrate(
                "No markdownlint-cli2 config found in the current directory. \
                 Pass a path explicitly: `mdlint migrate <path>`"
                    .to_string(),
            )
        })
}

/// `package.json` is only a usable candidate as a last resort, and only when it actually
/// has a `"markdownlint-cli2"` field.
fn is_usable_candidate(path: &Path) -> bool {
    if path.file_name().and_then(|n| n.to_str()) != Some("package.json") {
        return true;
    }

    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<Value>(&content).ok())
        .is_some_and(|value| value.get("markdownlint-cli2").is_some())
}

pub fn migrate_file(path: &Path) -> Result<MigrationResult> {
    let content = fs::read_to_string(path).map_err(|e| {
        MarkdownlintError::Migrate(format!("Failed to read {}: {e}", path.display()))
    })?;

    let is_package_json = path.file_name().and_then(|n| n.to_str()) == Some("package.json");
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let (source, mut extra_warnings) = match (is_package_json, extension.as_str()) {
        (true, _) => (cli2::parse_package_json(&content, path)?, Vec::new()),
        (false, "json" | "jsonc") => (cli2::parse_json(&content, path)?, Vec::new()),
        (false, "yaml" | "yml") => (cli2::parse_yaml(&content, path)?, Vec::new()),
        (false, "cjs" | "mjs" | "js") => js::parse_js(&content, path)?,
        (false, other) => {
            return Err(MarkdownlintError::Migrate(format!(
                "Unsupported config file extension {other:?}"
            )));
        }
    };

    let mut result = build_config(source);
    extra_warnings.append(&mut result.warnings);
    result.warnings = extra_warnings;
    Ok(result)
}

/// markdownlint-cli2 built-in defaults that differ from mdlint's defaults. Any enabled
/// rule not explicitly configured by the user must be pinned to these values so
/// behaviour is preserved after migration.
fn cli2_defaults() -> Vec<(&'static str, RuleConfig)> {
    use toml::Value as T;
    vec![
        (
            "MD003",
            RuleConfig::Config(HashMap::from([(
                "style".to_string(),
                T::String("consistent".to_string()),
            )])),
        ),
        (
            "MD004",
            RuleConfig::Config(HashMap::from([(
                "style".to_string(),
                T::String("consistent".to_string()),
            )])),
        ),
        (
            "MD013",
            RuleConfig::Config(HashMap::from([("line_length".to_string(), T::Integer(80))])),
        ),
        (
            "MD029",
            RuleConfig::Config(HashMap::from([(
                "style".to_string(),
                T::String("one_or_ordered".to_string()),
            )])),
        ),
        (
            "MD035",
            RuleConfig::Config(HashMap::from([(
                "style".to_string(),
                T::String("consistent".to_string()),
            )])),
        ),
        (
            "MD046",
            RuleConfig::Config(HashMap::from([(
                "style".to_string(),
                T::String("consistent".to_string()),
            )])),
        ),
        (
            "MD048",
            RuleConfig::Config(HashMap::from([(
                "style".to_string(),
                T::String("consistent".to_string()),
            )])),
        ),
        (
            "MD049",
            RuleConfig::Config(HashMap::from([(
                "style".to_string(),
                T::String("consistent".to_string()),
            )])),
        ),
        (
            "MD050",
            RuleConfig::Config(HashMap::from([(
                "style".to_string(),
                T::String("consistent".to_string()),
            )])),
        ),
        (
            "MD055",
            RuleConfig::Config(HashMap::from([(
                "style".to_string(),
                T::String("consistent".to_string()),
            )])),
        ),
        (
            "MD026",
            RuleConfig::Config(HashMap::from([(
                "punctuation".to_string(),
                T::String(".,;:!。，；：！".to_string()),
            )])),
        ),
        (
            "MD060",
            RuleConfig::Config(HashMap::from([(
                "style".to_string(),
                T::String("any".to_string()),
            )])),
        ),
    ]
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

    if let Some(gitignore) = source.gitignore {
        config.gitignore = gitignore;
    }

    if let Some(no_inline_config) = source.no_inline_config {
        config.no_inline_config = no_inline_config;
    }

    if let Some(front_matter) = source.front_matter {
        config.front_matter = Some(front_matter);
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
                    "Skipped rule {name:?}: no mdlint implementation for this rule"
                ));
                continue;
            };

            config.rules.insert(code, translate_rule_value(&value));
        }
    }

    // Pin cli2's built-in defaults for rules that the user left unconfigured.
    // Rules with explicit params (RuleConfig::Config) are left as-is; rules the
    // user simply enabled (RuleConfig::Enabled(true)) or that are on via
    // default_enabled get the cli2 default so behaviour is unchanged post-migration.
    for (code, default) in cli2_defaults() {
        match config.rules.get(code) {
            Some(RuleConfig::Enabled(true)) => {
                config.rules.insert(code.to_string(), default);
            }
            None if config.default_enabled => {
                config.rules.insert(code.to_string(), default);
            }
            Some(RuleConfig::Config(_) | RuleConfig::Enabled(false)) | None => {}
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
        assert!(!result.config.rules.contains_key("MD999"));
    }

    #[test]
    fn maps_gitignore_no_inline_config_and_front_matter() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".markdownlint-cli2.jsonc");
        fs::write(
            &path,
            r#"{
                "config": { "default": true },
                "gitignore": false,
                "noInlineConfig": true,
                "frontMatterPattern": "^-{3}\\s*\\n(?:.*?\\n)?-{3}\\s*\\n"
            }"#,
        )
        .unwrap();

        let result = migrate_file(&path).unwrap();
        assert!(!result.config.gitignore);
        assert!(result.config.no_inline_config);
        assert_eq!(
            result.config.front_matter,
            Some("^-{3}\\s*\\n(?:.*?\\n)?-{3}\\s*\\n".to_string())
        );
    }

    #[test]
    fn pins_cli2_defaults_for_unconfigured_rules() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".markdownlint.json");
        // User enabled all rules but configured nothing explicitly.
        fs::write(&path, r#"{ "default": true }"#).unwrap();

        let result = migrate_file(&path).unwrap();

        // MD013: cli2 default is 80; mdlint default is 120 — must be pinned to 80.
        let md013 = result.config.rules.get("MD013").unwrap();
        assert!(
            matches!(md013, RuleConfig::Config(p) if p.get("line_length") == Some(&toml::Value::Integer(80)))
        );

        // MD003/MD004/MD049/MD050: cli2 default is "consistent"; mdlint defaults are opinionated.
        for code in &["MD003", "MD004", "MD049", "MD050"] {
            let rule = result.config.rules.get(*code).unwrap();
            assert!(
                matches!(rule, RuleConfig::Config(p) if p.get("style") == Some(&toml::Value::String("consistent".to_string()))),
                "{code} should be pinned to consistent"
            );
        }

        // MD026: cli2 default omits ?; mdlint default includes it.
        let md026 = result.config.rules.get("MD026").unwrap();
        assert!(
            matches!(md026, RuleConfig::Config(p) if p.get("punctuation") == Some(&toml::Value::String(".,;:!。，；：！".to_string())))
        );

        // MD060: cli2 default is "any"; mdlint default is "consistent".
        let md060 = result.config.rules.get("MD060").unwrap();
        assert!(
            matches!(md060, RuleConfig::Config(p) if p.get("style") == Some(&toml::Value::String("any".to_string())))
        );
    }

    #[test]
    fn does_not_override_explicit_user_config() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".markdownlint-cli2.jsonc");
        // User explicitly set MD013 line_length to 100 and MD003 style to atx.
        fs::write(
            &path,
            r#"{
                "config": {
                    "default": true,
                    "MD013": { "line_length": 100 },
                    "MD003": { "style": "atx" }
                }
            }"#,
        )
        .unwrap();

        let result = migrate_file(&path).unwrap();

        let md013 = result.config.rules.get("MD013").unwrap();
        assert!(
            matches!(md013, RuleConfig::Config(p) if p.get("line_length") == Some(&toml::Value::Integer(100)))
        );
        let md003 = result.config.rules.get("MD003").unwrap();
        assert!(
            matches!(md003, RuleConfig::Config(p) if p.get("style") == Some(&toml::Value::String("atx".to_string())))
        );
    }

    #[test]
    fn does_not_pin_defaults_when_default_disabled() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".markdownlint.json");
        // User disabled all rules by default; cli2 defaults should not be emitted.
        fs::write(&path, r#"{ "default": false }"#).unwrap();

        let result = migrate_file(&path).unwrap();
        assert!(!result.config.default_enabled);
        assert!(!result.config.rules.contains_key("MD013"));
        assert!(!result.config.rules.contains_key("MD003"));
    }

    #[test]
    fn migrates_from_package_json_field() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("package.json");
        fs::write(
            &path,
            r#"{
                "name": "some-package",
                "markdownlint-cli2": {
                    "config": { "default": true, "MD013": { "line_length": 100 } },
                    "fix": false
                }
            }"#,
        )
        .unwrap();

        let result = migrate_file(&path).unwrap();
        assert!(!result.config.fix);
        assert!(result.config.rules.contains_key("MD013"));
    }
}
