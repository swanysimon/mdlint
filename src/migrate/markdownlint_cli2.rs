use crate::config::{Config, RuleConfig};
use crate::error::{MarkdownlintError, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Dedicated config filenames markdownlint-cli2 looks for, in the same priority order
/// as the upstream tool. `.cjs`/`.mjs` configs are executable JavaScript and can't be
/// parsed without a JS runtime, so we detect them only to give a clear error message.
const CONFIG_FILE_NAMES: &[&str] = &[
    ".markdownlint-cli2.jsonc",
    ".markdownlint-cli2.json",
    ".markdownlint-cli2.yaml",
    ".markdownlint-cli2.yml",
    ".markdownlint-cli2.cjs",
    ".markdownlint-cli2.mjs",
];

pub fn detect(dir: &Path) -> Option<PathBuf> {
    CONFIG_FILE_NAMES
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
        .or_else(|| {
            let package_json = dir.join("package.json");
            has_cli2_field(&package_json).then_some(package_json)
        })
}

fn has_cli2_field(package_json: &Path) -> bool {
    fs::read_to_string(package_json)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .is_some_and(|value| value.get("markdownlint-cli2").is_some())
}

pub fn migrate(config_path: &Path) -> Result<(Config, Vec<String>)> {
    match config_path.extension().and_then(|ext| ext.to_str()) {
        Some("jsonc") | Some("json") => migrate_json(config_path),
        Some("yaml") | Some("yml") => Err(unsupported_format(config_path, "YAML")),
        Some("cjs") | Some("mjs") => Err(unsupported_format(config_path, "JavaScript")),
        _ if config_path.file_name().and_then(|n| n.to_str()) == Some("package.json") => {
            migrate_json(config_path)
        }
        _ => Err(unsupported_format(config_path, "unknown")),
    }
}

fn unsupported_format(config_path: &Path, kind: &str) -> MarkdownlintError {
    MarkdownlintError::Migrate(format!(
        "{} is a {} markdownlint-cli2 config, which mdlint can't parse yet. \
         Convert it to JSON or JSONC and re-run `mdlint migrate`.",
        config_path.display(),
        kind
    ))
}

fn migrate_json(config_path: &Path) -> Result<(Config, Vec<String>)> {
    let content = fs::read_to_string(config_path)?;
    let stripped = strip_jsonc_comments(&content);
    let raw: serde_json::Value = serde_json::from_str(&stripped)
        .map_err(|e| MarkdownlintError::Migrate(format!("Failed to parse JSON: {}", e)))?;

    let cli2 = if config_path.file_name().and_then(|n| n.to_str()) == Some("package.json") {
        raw.get("markdownlint-cli2").cloned().ok_or_else(|| {
            MarkdownlintError::Migrate(
                "package.json has no \"markdownlint-cli2\" field".to_string(),
            )
        })?
    } else {
        raw
    };

    let mut config = Config::default();
    let mut warnings = Vec::new();

    if let Some(value) = cli2.get("gitignore").and_then(serde_json::Value::as_bool) {
        config.gitignore = value;
    }
    if let Some(value) = cli2.get("fix").and_then(serde_json::Value::as_bool) {
        config.fix = value;
    }
    if let Some(value) = cli2
        .get("noInlineConfig")
        .and_then(serde_json::Value::as_bool)
    {
        config.no_inline_config = value;
    }
    if let Some(value) = cli2.get("frontMatter").and_then(serde_json::Value::as_str) {
        config.front_matter = Some(value.to_string());
    }
    if let Some(values) = cli2.get("ignores").and_then(serde_json::Value::as_array) {
        config.exclude = string_array(values);
    }
    if let Some(values) = cli2
        .get("customRules")
        .and_then(serde_json::Value::as_array)
    {
        config.custom_rules = string_array(values);
    }
    if cli2.get("globs").is_some() {
        warnings.push(
            "\"globs\" has no mdlint equivalent; pass file paths to `mdlint check`/`format` \
             on the command line instead"
                .to_string(),
        );
    }

    if let Some(rule_config) = cli2.get("config").and_then(serde_json::Value::as_object) {
        migrate_rule_config(rule_config, &mut config, &mut warnings)?;
    }

    Ok((config, warnings))
}

fn string_array(values: &[serde_json::Value]) -> Vec<String> {
    values
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

fn migrate_rule_config(
    rule_config: &serde_json::Map<String, serde_json::Value>,
    config: &mut Config,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let rule_code = Regex::new(r"(?i)^MD\d+$").expect("static regex is valid");

    for (key, value) in rule_config {
        if key == "default" {
            if let Some(value) = value.as_bool() {
                config.default_enabled = value;
            }
            continue;
        }

        if !rule_code.is_match(key) {
            warnings.push(format!(
                "rule alias \"{}\" isn't supported yet; configure the rule by its MDxxx code \
                 instead",
                key
            ));
            continue;
        }

        let code = key.to_uppercase();
        let rule_value = match value {
            serde_json::Value::Bool(enabled) => RuleConfig::Enabled(*enabled),
            serde_json::Value::Object(params) => RuleConfig::Config(json_object_to_toml(params)?),
            _ => {
                warnings.push(format!(
                    "rule \"{}\" has an unsupported config value and was skipped",
                    code
                ));
                continue;
            }
        };
        config.rules.insert(code, rule_value);
    }

    Ok(())
}

fn json_object_to_toml(
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<HashMap<String, toml::Value>> {
    let mut table = HashMap::with_capacity(params.len());
    for (key, value) in params {
        if let Some(value) = json_to_toml(value)? {
            table.insert(key.clone(), value);
        }
    }
    Ok(table)
}

fn json_to_toml(value: &serde_json::Value) -> Result<Option<toml::Value>> {
    let converted = match value {
        serde_json::Value::Null => return Ok(None),
        serde_json::Value::Bool(b) => toml::Value::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                return Err(MarkdownlintError::Migrate(format!(
                    "Unsupported number value: {}",
                    n
                )));
            }
        }
        serde_json::Value::String(s) => toml::Value::String(s.clone()),
        serde_json::Value::Array(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                if let Some(value) = json_to_toml(item)? {
                    values.push(value);
                }
            }
            toml::Value::Array(values)
        }
        serde_json::Value::Object(map) => {
            let mut table = toml::map::Map::with_capacity(map.len());
            for (key, value) in map {
                if let Some(value) = json_to_toml(value)? {
                    table.insert(key.clone(), value);
                }
            }
            toml::Value::Table(table)
        }
    };
    Ok(Some(converted))
}

/// Strip `//` and `/* */` comments from JSONC, respecting string literals so that
/// comment-like sequences inside strings are left untouched.
fn strip_jsonc_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            output.push(c);
            if c == '\\' {
                if let Some(escaped) = chars.next() {
                    output.push(escaped);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        match c {
            '"' => {
                in_string = true;
                output.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                while let Some(&next) = chars.peek() {
                    if next == '\n' {
                        break;
                    }
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                while let Some(next) = chars.next() {
                    if next == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => output.push(c),
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_strip_jsonc_comments() {
        let input = r#"{
            // a comment
            "a": 1, /* inline */ "b": "// not a comment"
        }"#;
        let stripped = strip_jsonc_comments(input);
        let value: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(value["a"], 1);
        assert_eq!(value["b"], "// not a comment");
    }

    #[test]
    fn test_migrate_json_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".markdownlint-cli2.jsonc");
        let mut file = fs::File::create(&config_path).unwrap();
        write!(
            file,
            r#"{{
                // disable MD013 entirely
                "config": {{
                    "default": true,
                    "MD013": false,
                    "MD003": {{ "style": "atx" }}
                }},
                "gitignore": false,
                "ignores": ["node_modules", "dist"]
            }}"#
        )
        .unwrap();

        let (config, warnings) = migrate(&config_path).unwrap();
        assert!(config.default_enabled);
        assert!(!config.gitignore);
        assert_eq!(config.exclude, vec!["node_modules", "dist"]);
        assert!(warnings.is_empty());

        match config.rules.get("MD013").unwrap() {
            RuleConfig::Enabled(enabled) => assert!(!enabled),
            RuleConfig::Config(_) => panic!("expected MD013 to be disabled"),
        }
        match config.rules.get("MD003").unwrap() {
            RuleConfig::Config(params) => {
                assert_eq!(params.get("style").unwrap().as_str(), Some("atx"));
            }
            RuleConfig::Enabled(_) => panic!("expected MD003 to carry params"),
        }
    }

    #[test]
    fn test_migrate_warns_on_unsupported_alias() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".markdownlint-cli2.json");
        let mut file = fs::File::create(&config_path).unwrap();
        write!(file, r#"{{ "config": {{ "heading-style": false }} }}"#).unwrap();

        let (_, warnings) = migrate(&config_path).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("heading-style"));
    }

    #[test]
    fn test_migrate_rejects_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".markdownlint-cli2.yaml");
        fs::write(&config_path, "config:\n  default: true\n").unwrap();

        let result = migrate(&config_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_prefers_dedicated_file_over_package_json() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join(".markdownlint-cli2.jsonc"), "{}").unwrap();
        fs::write(
            temp_dir.path().join("package.json"),
            r#"{ "markdownlint-cli2": {} }"#,
        )
        .unwrap();

        let detected = detect(temp_dir.path()).unwrap();
        assert_eq!(detected.file_name().unwrap(), ".markdownlint-cli2.jsonc");
    }

    #[test]
    fn test_detect_falls_back_to_package_json() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("package.json"),
            r#"{ "markdownlint-cli2": {} }"#,
        )
        .unwrap();

        let detected = detect(temp_dir.path()).unwrap();
        assert_eq!(detected.file_name().unwrap(), "package.json");
    }

    #[test]
    fn test_detect_none() {
        let temp_dir = TempDir::new().unwrap();
        assert!(detect(temp_dir.path()).is_none());
    }
}
