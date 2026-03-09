use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    pub fn config(&self) -> &HashMap<String, RuleConfig> {
        &self.rules
    }
}
