use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;
use std::collections::HashMap;

pub trait Rule: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn tags(&self) -> &[&str];

    /// Check the markdown content for violations
    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation>;

    /// Whether this rule can automatically fix violations
    fn fixable(&self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct RuleRegistry {
    rules: HashMap<String, Box<dyn Rule + Send + Sync>>,
}

impl RuleRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, rule: Box<dyn Rule + Send + Sync>) {
        self.rules.insert(rule.name().to_string(), rule);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Rule> {
        self.rules.get(name).map(|r| r.as_ref() as &dyn Rule)
    }

    pub fn all_rules(&self) -> impl Iterator<Item = &dyn Rule> {
        self.rules.values().map(|r| r.as_ref() as &dyn Rule)
    }
}
