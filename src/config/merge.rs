use crate::config::types::{Config, RuleConfig};
use std::collections::HashMap;

pub fn merge_configs(mut base: Config, override_cfg: Config) -> Config {
    // Extend custom rules
    if !override_cfg.custom_rules.is_empty() {
        base.custom_rules.extend(override_cfg.custom_rules);
    }

    // Extend excludes
    if !override_cfg.exclude.is_empty() {
        base.exclude.extend(override_cfg.exclude);
    }

    base.default_enabled = override_cfg.default_enabled;
    base.fix = override_cfg.fix;

    // Override front_matter if set
    if override_cfg.front_matter.is_some() {
        base.front_matter = override_cfg.front_matter;
    }

    // Override gitignore setting
    if !override_cfg.gitignore {
        base.gitignore = false;
    }

    // Override no_inline_config if set
    if override_cfg.no_inline_config {
        base.no_inline_config = true;
    }

    // Merge rule configurations
    for (rule_name, rule_config) in override_cfg.rules {
        base.rules.insert(rule_name, rule_config);
    }

    base
}

pub fn merge_rule_configs(
    base: &HashMap<String, RuleConfig>,
    override_cfg: &HashMap<String, RuleConfig>,
) -> HashMap<String, RuleConfig> {
    let mut merged = base.clone();

    for (rule_name, rule_config) in override_cfg {
        merged.insert(rule_name.clone(), rule_config.clone());
    }

    merged
}

pub fn merge_many_configs(configs: Vec<Config>) -> Config {
    configs.into_iter().fold(Config::default(), merge_configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{Config, RuleConfig};

    #[test]
    fn test_merge_configs_default_enabled() {
        // Child config explicitly opting out overrides the default true
        let base = Config::default();
        let override_cfg = Config {
            default_enabled: false,
            ..Default::default()
        };

        let merged = merge_configs(base, override_cfg);
        assert!(!merged.default_enabled);
    }

    #[test]
    fn test_merge_configs_gitignore() {
        let base = Config {
            gitignore: true,
            ..Default::default()
        };

        let override_cfg = Config {
            gitignore: false,
            ..Default::default()
        };

        let merged = merge_configs(base, override_cfg);
        assert!(!merged.gitignore);
    }

    #[test]
    fn test_merge_configs_rules() {
        let mut base = Config::default();
        base.rules
            .insert("MD001".to_string(), RuleConfig::Enabled(true));

        let mut override_cfg = Config::default();
        override_cfg
            .rules
            .insert("MD002".to_string(), RuleConfig::Enabled(false));

        let merged = merge_configs(base, override_cfg);
        assert_eq!(merged.rules.len(), 2);
    }

    #[test]
    fn test_merge_many_configs() {
        let mut config1 = Config::default();
        config1
            .rules
            .insert("MD001".to_string(), RuleConfig::Enabled(true));

        let config2 = Config {
            default_enabled: true,
            gitignore: true, // Explicitly set to test merge
            ..Default::default()
        };

        let config3 = Config {
            no_inline_config: true,
            gitignore: true, // Keep gitignore enabled
            ..Default::default()
        };

        let merged = merge_many_configs(vec![config1, config2, config3]);
        assert!(merged.gitignore);
        assert!(merged.default_enabled);
        assert!(merged.no_inline_config);
        assert_eq!(merged.rules.len(), 1);
    }
}
