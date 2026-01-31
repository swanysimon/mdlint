use crate::config::{Config, RuleConfig};
use crate::error::Result;
use crate::lint::{Rule, RuleRegistry};
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;
use std::path::Path;

pub struct LintEngine {
    config: Config,
    registry: RuleRegistry,
}

impl LintEngine {
    pub fn new(config: Config) -> Self {
        let registry = crate::lint::rules::create_default_registry();
        Self { config, registry }
    }

    pub fn lint_content(&self, content: &str) -> Result<Vec<Violation>> {
        let parser = MarkdownParser::new(content);
        Ok(self
            .registry
            .all_rules()
            .flat_map(|rule| self.violations(&parser, rule))
            .collect())
    }

    fn violations(&self, parser: &MarkdownParser, rule: &dyn Rule) -> Vec<Violation> {
        let rule_config = self.config.config().get(rule.name());
        let config_value = match rule_config {
            Some(RuleConfig::Enabled(false)) => return Vec::new(),
            Some(RuleConfig::Enabled(true)) => None,
            Some(RuleConfig::Config(cfg)) => {
                // Convert TOML config to JSON for rule consumption
                let mut table = toml::map::Map::new();
                for (k, v) in cfg.clone() {
                    table.insert(k, v);
                }
                let toml_value = toml::Value::Table(table);
                let json_value: Value = toml_to_json(toml_value);

                if let Some(Value::Bool(false)) = json_value.get("enabled") {
                    return Vec::new();
                }
                Some(json_value)
            }
            None => {
                // If default_enabled is true and no specific config exists, enable the rule
                if self.config.default_enabled {
                    None
                } else {
                    return Vec::new();
                }
            }
        };

        rule.check(parser, config_value.as_ref())
    }

    pub fn lint_file(&self, path: &Path) -> Result<Vec<Violation>> {
        let content = std::fs::read_to_string(path)?;
        self.lint_content(&content)
    }
}

/// Convert a TOML value to a JSON value
fn toml_to_json(toml_val: toml::Value) -> Value {
    match toml_val {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Number(i.into()),
        toml::Value::Float(f) => {
            Value::Number(serde_json::Number::from_f64(f).unwrap_or_else(|| 0.into()))
        }
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Array(arr) => Value::Array(arr.into_iter().map(toml_to_json).collect()),
        toml::Value::Table(table) => Value::Object(
            table
                .into_iter()
                .map(|(k, v)| (k, toml_to_json(v)))
                .collect(),
        ),
        toml::Value::Datetime(dt) => Value::String(dt.to_string()),
    }
}
