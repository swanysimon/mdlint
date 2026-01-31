use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Rule configuration: rule name -> config
    #[serde(default)]
    pub rules: HashMap<String, RuleConfig>,

    /// Enable all rules by default
    #[serde(default)]
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
}

fn default_gitignore() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rules: HashMap::new(),
            default_enabled: false,
            custom_rules: Vec::new(),
            gitignore: default_gitignore(),
            front_matter: None,
            no_inline_config: false,
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
