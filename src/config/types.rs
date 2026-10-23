use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    /// Rule configuration: rule name -> config
    #[serde(default)]
    pub rules: HashMap<String, RuleConfig>,

    /// Enable all rules by default. `None` means "not set by this config", which is what lets
    /// merging tell an omitted value apart from one the user explicitly set to the default;
    /// read it through [`Config::default_enabled`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_enabled: Option<bool>,

    /// Custom rule paths (for future extension)
    #[serde(default)]
    pub custom_rules: Vec<String>,

    /// Respect .gitignore files when discovering files; read it through [`Config::gitignore`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gitignore: Option<bool>,

    /// Front matter pattern (YAML --- or TOML +++)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front_matter: Option<String>,

    /// Disable inline configuration comments; read it through [`Config::no_inline_config`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_inline_config: Option<bool>,

    /// Paths and glob patterns to exclude from file discovery
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Apply auto-fixes automatically when running `mdlint check`; read it through
    /// [`Config::fix`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<bool>,
}

/// Built-in defaults, used for every option no config in the chain sets.
impl Config {
    #[must_use]
    pub fn default_enabled(&self) -> bool {
        self.default_enabled.unwrap_or(true)
    }

    #[must_use]
    pub fn gitignore(&self) -> bool {
        self.gitignore.unwrap_or(true)
    }

    #[must_use]
    pub fn no_inline_config(&self) -> bool {
        self.no_inline_config.unwrap_or_default()
    }

    #[must_use]
    pub fn fix(&self) -> bool {
        self.fix.unwrap_or(true)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RuleConfig {
    Enabled(bool),
    Config(HashMap<String, toml::Value>),
}

// Legacy field mappings for backward compatibility with old config structure
impl Config {
    /// Legacy accessor for config field (now called rules)
    #[must_use]
    pub fn config(&self) -> &HashMap<String, RuleConfig> {
        &self.rules
    }

    /// Apply `--select`/`--ignore` CLI overrides on top of the loaded config.
    ///
    /// `select` (if non-empty and not `ALL`, case-insensitive) restricts linting to just the
    /// listed rules: `default_enabled` is turned off and each rule gets a bare `Enabled(true)`
    /// entry, unless a more specific `RuleConfig::Config(..)` already exists for it (so
    /// config-file rule parameters, e.g. MD013's `line_length`, survive `--select`).
    ///
    /// `ignore` unconditionally force-disables the listed rules, overriding both the config
    /// file and `--select`.
    ///
    /// Empty `select`/`ignore` is a no-op.
    #[must_use]
    pub fn apply_rule_filters(mut self, select: &[String], ignore: &[String]) -> Self {
        let select_all = select.iter().any(|code| code.eq_ignore_ascii_case("all"));
        if !select.is_empty() && !select_all {
            self.default_enabled = Some(false);
            for code in select {
                self.rules
                    .entry(code.to_uppercase())
                    .or_insert(RuleConfig::Enabled(true));
            }
        }

        for code in ignore {
            self.rules
                .insert(code.to_uppercase(), RuleConfig::Enabled(false));
        }

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_empty_is_noop() {
        let config = Config::default().apply_rule_filters(&[], &[]);
        assert!(config.default_enabled());
        assert!(config.rules.is_empty());
    }

    #[test]
    fn select_restricts_to_listed_rules() {
        let config = Config::default().apply_rule_filters(&["md001".to_owned()], &[]);
        assert!(!config.default_enabled());
        assert!(matches!(
            config.rules.get("MD001"),
            Some(RuleConfig::Enabled(true))
        ));
        assert_eq!(config.rules.len(), 1);
    }

    #[test]
    fn select_all_is_noop() {
        let config = Config::default().apply_rule_filters(&["ALL".to_owned()], &[]);
        assert!(config.default_enabled());
        assert!(config.rules.is_empty());
    }

    #[test]
    fn select_preserves_existing_rule_config() {
        let mut base = Config::default();
        let mut params = HashMap::new();
        params.insert("line_length".to_owned(), toml::Value::Integer(100));
        base.rules
            .insert("MD013".to_owned(), RuleConfig::Config(params));

        let config = base.apply_rule_filters(&["MD013".to_owned()], &[]);
        match config.rules.get("MD013") {
            Some(RuleConfig::Config(params)) => {
                assert_eq!(params.get("line_length"), Some(&toml::Value::Integer(100)));
            }
            other => panic!("expected preserved MD013 config, got {other:?}"),
        }
    }

    #[test]
    fn ignore_force_disables_rule() {
        let mut base = Config::default();
        base.rules
            .insert("MD013".to_owned(), RuleConfig::Enabled(true));

        let config = base.apply_rule_filters(&[], &["md013".to_owned()]);
        assert!(matches!(
            config.rules.get("MD013"),
            Some(RuleConfig::Enabled(false))
        ));
    }

    #[test]
    fn ignore_wins_over_select() {
        let config = Config::default().apply_rule_filters(
            &["MD001".to_owned(), "MD013".to_owned()],
            &["MD013".to_owned()],
        );
        assert!(matches!(
            config.rules.get("MD001"),
            Some(RuleConfig::Enabled(true))
        ));
        assert!(matches!(
            config.rules.get("MD013"),
            Some(RuleConfig::Enabled(false))
        ));
    }
}
