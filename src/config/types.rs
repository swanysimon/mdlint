use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[allow(clippy::struct_excessive_bools)] // config struct mirrors TOML fields 1:1; bools are the right representation
pub struct Config {
    /// Rule configuration: rule name -> config
    #[serde(default)]
    pub rules: HashMap<String, RuleConfig>,

    /// Enable all rules by default
    #[serde(default = "default_default_enabled")]
    pub default_enabled: bool,

    /// Custom rule paths (for future extension)
    #[serde(default)]
    pub custom_rules: Vec<String>,

    /// Respect .gitignore files when discovering files
    #[serde(default = "default_gitignore")]
    pub gitignore: bool,

    /// Front matter pattern (YAML --- or TOML +++)
    #[serde(default)]
    pub front_matter: Option<String>,

    /// Disable inline configuration comments
    #[serde(default)]
    pub no_inline_config: bool,

    /// Paths and glob patterns to exclude from file discovery
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Apply auto-fixes automatically when running `mdlint check`
    #[serde(default = "default_fix")]
    pub fix: bool,

    /// `mdlint format` reflows paragraph/list-item/blockquote/footnote prose to
    /// the configured line length (see `[rules.MD013].line_length`)
    #[serde(default = "default_reflow")]
    pub reflow: bool,
}

fn default_default_enabled() -> bool {
    true
}

fn default_gitignore() -> bool {
    true
}

fn default_fix() -> bool {
    true
}

fn default_reflow() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rules: HashMap::new(),
            default_enabled: true,
            custom_rules: Vec::new(),
            gitignore: default_gitignore(),
            front_matter: None,
            no_inline_config: false,
            exclude: Vec::new(),
            fix: true,
            reflow: default_reflow(),
        }
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

    /// The line length `mdlint format` reflows prose to, taken from
    /// `[rules.MD013].line_length` (falling back to MD013's own default) so the
    /// formatter and the linter agree on a single maximum width.
    #[must_use]
    pub fn line_length(&self) -> usize {
        const DEFAULT: usize = 120;
        match self.rules.get("MD013") {
            Some(RuleConfig::Config(params)) => params
                .get("line_length")
                .and_then(toml::Value::as_integer)
                .and_then(|v| usize::try_from(v).ok())
                .unwrap_or(DEFAULT),
            _ => DEFAULT,
        }
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
            self.default_enabled = false;
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
    fn reflow_defaults_to_true() {
        assert!(Config::default().reflow);
    }

    #[test]
    fn line_length_defaults_to_120_without_md013_config() {
        assert_eq!(Config::default().line_length(), 120);
    }

    #[test]
    fn line_length_reads_from_md013_rule_config() {
        let mut config = Config::default();
        let mut params = HashMap::new();
        params.insert("line_length".to_owned(), toml::Value::Integer(100));
        config
            .rules
            .insert("MD013".to_owned(), RuleConfig::Config(params));
        assert_eq!(config.line_length(), 100);
    }

    #[test]
    fn line_length_falls_back_when_md013_has_no_line_length_param() {
        let mut config = Config::default();
        config
            .rules
            .insert("MD013".to_owned(), RuleConfig::Enabled(true));
        assert_eq!(config.line_length(), 120);
    }

    #[test]
    fn select_empty_is_noop() {
        let config = Config::default().apply_rule_filters(&[], &[]);
        assert!(config.default_enabled);
        assert!(config.rules.is_empty());
    }

    #[test]
    fn select_restricts_to_listed_rules() {
        let config = Config::default().apply_rule_filters(&["md001".to_owned()], &[]);
        assert!(!config.default_enabled);
        assert!(matches!(
            config.rules.get("MD001"),
            Some(RuleConfig::Enabled(true))
        ));
        assert_eq!(config.rules.len(), 1);
    }

    #[test]
    fn select_all_is_noop() {
        let config = Config::default().apply_rule_filters(&["ALL".to_owned()], &[]);
        assert!(config.default_enabled);
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
