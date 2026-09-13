use crate::config::types::{Config, RuleConfig};
use std::collections::HashMap;

/// Merge `override_cfg` (the config closer to the working directory) onto `base`.
///
/// Scalars follow one rule: the closest config that sets a value wins, and a config that
/// omits an option leaves whatever an outer config set. Arrays (`exclude`, `custom_rules`)
/// accumulate across the whole chain. Rules merge per rule code — a closer `[rules.MD013]`
/// replaces an outer one outright rather than merging parameter by parameter.
#[must_use]
pub fn merge_configs(mut base: Config, override_cfg: Config) -> Config {
    base.custom_rules.extend(override_cfg.custom_rules);
    base.exclude.extend(override_cfg.exclude);

    base.default_enabled = override_cfg.default_enabled.or(base.default_enabled);
    base.fix = override_cfg.fix.or(base.fix);
    base.front_matter = override_cfg.front_matter.or(base.front_matter);
    base.gitignore = override_cfg.gitignore.or(base.gitignore);
    base.no_inline_config = override_cfg.no_inline_config.or(base.no_inline_config);

    base.rules.extend(override_cfg.rules);

    base
}

#[must_use]
#[allow(clippy::implicit_hasher)] // binary-only crate; no benefit generalizing over BuildHasher
pub fn merge_rule_configs(
    base: &HashMap<String, RuleConfig>,
    override_cfg: &HashMap<String, RuleConfig>,
) -> HashMap<String, RuleConfig> {
    let mut merged = base.clone();

    for (k, v) in override_cfg {
        merged.insert(k.clone(), v.clone());
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
            default_enabled: Some(false),
            ..Default::default()
        };

        let merged = merge_configs(base, override_cfg);
        assert!(!merged.default_enabled());
    }

    #[test]
    fn test_merge_configs_gitignore() {
        let base = Config {
            gitignore: Some(true),
            ..Default::default()
        };

        let override_cfg = Config {
            gitignore: Some(false),
            ..Default::default()
        };

        let merged = merge_configs(base, override_cfg);
        assert!(!merged.gitignore());
    }

    #[test]
    fn omitted_scalar_keeps_the_outer_value() {
        // A config that says nothing about an option must not reset it to the built-in
        // default: the outer config that did set it still wins.
        let base = Config {
            fix: Some(false),
            default_enabled: Some(false),
            gitignore: Some(false),
            no_inline_config: Some(true),
            front_matter: Some("---".to_owned()),
            ..Default::default()
        };

        let merged = merge_configs(base, Config::default());
        assert!(!merged.fix());
        assert!(!merged.default_enabled());
        assert!(!merged.gitignore());
        assert!(merged.no_inline_config());
        assert_eq!(merged.front_matter.as_deref(), Some("---"));
    }

    #[test]
    fn closer_config_can_restore_a_default() {
        // Overriding works in both directions: a closer config can turn an option back on
        // as well as off.
        let base = Config {
            gitignore: Some(false),
            no_inline_config: Some(true),
            ..Default::default()
        };

        let override_cfg = Config {
            gitignore: Some(true),
            no_inline_config: Some(false),
            ..Default::default()
        };

        let merged = merge_configs(base, override_cfg);
        assert!(merged.gitignore());
        assert!(!merged.no_inline_config());
    }

    #[test]
    fn arrays_accumulate_across_the_chain() {
        let base = Config {
            exclude: vec!["dist".to_owned()],
            custom_rules: vec!["a.rs".to_owned()],
            ..Default::default()
        };

        let override_cfg = Config {
            exclude: vec!["build".to_owned()],
            custom_rules: vec!["b.rs".to_owned()],
            ..Default::default()
        };

        let merged = merge_configs(base, override_cfg);
        assert_eq!(merged.exclude, ["dist", "build"]);
        assert_eq!(merged.custom_rules, ["a.rs", "b.rs"]);
    }

    #[test]
    fn test_merge_configs_rules() {
        let mut base = Config::default();
        base.rules
            .insert("MD001".to_owned(), RuleConfig::Enabled(true));

        let mut override_cfg = Config::default();
        override_cfg
            .rules
            .insert("MD002".to_owned(), RuleConfig::Enabled(false));

        let merged = merge_configs(base, override_cfg);
        assert_eq!(merged.rules.len(), 2);
    }

    #[test]
    fn test_merge_many_configs() {
        let mut config1 = Config::default();
        config1
            .rules
            .insert("MD001".to_owned(), RuleConfig::Enabled(true));

        let config2 = Config {
            default_enabled: Some(true),
            gitignore: Some(true), // Explicitly set to test merge
            ..Default::default()
        };

        let config3 = Config {
            no_inline_config: Some(true),
            gitignore: Some(true), // Keep gitignore enabled
            ..Default::default()
        };

        let merged = merge_many_configs(vec![config1, config2, config3]);
        assert!(merged.gitignore());
        assert!(merged.default_enabled());
        assert!(merged.no_inline_config());
        assert_eq!(merged.rules.len(), 1);
    }
}
