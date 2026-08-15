use crate::config::{Config, RuleConfig};
use crate::error::Result;
use crate::lint::{Rule, RuleRegistry};
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct LintEngine {
    config: Config,
    registry: RuleRegistry,
}

impl LintEngine {
    #[must_use]
    pub fn new(config: Config) -> Self {
        let registry = crate::lint::rules::create_default_registry();
        Self { config, registry }
    }

    pub fn lint_content(&self, content: &str) -> Result<Vec<Violation>> {
        let parser = MarkdownParser::new(content);
        let mut violations: Vec<Violation> = self
            .registry
            .all_rules()
            .flat_map(|rule| self.violations(&parser, rule))
            .collect();

        if !self.config.no_inline_config {
            let suppressed = parse_inline_config(content);
            if !suppressed.is_empty() {
                violations.retain(|v| {
                    let line = v.line;
                    let all = suppressed.get("*").is_some_and(|s| s.contains(&line));
                    let specific = suppressed
                        .get(v.rule.as_str())
                        .is_some_and(|s| s.contains(&line));
                    !all && !specific
                });
            }
        }

        Ok(violations)
    }

    fn violations(&self, parser: &MarkdownParser, rule: &dyn Rule) -> Vec<Violation> {
        let rule_config = self.config.config().get(rule.name());
        let config_value = match rule_config {
            Some(RuleConfig::Enabled(false)) => return Vec::new(),
            Some(RuleConfig::Enabled(true)) => None,
            Some(RuleConfig::Config(cfg)) => {
                // Convert TOML config to JSON for rule consumption
                let mut table = toml::map::Map::new();
                table.extend(cfg.clone());
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

/// Parse inline configuration comments from document content.
///
/// Supports:
/// - `<!-- mdlint-disable -->` / `<!-- mdlint-disable MD001 MD003 -->`
/// - `<!-- mdlint-enable -->` / `<!-- mdlint-enable MD001 -->`
/// - `<!-- mdlint-disable-next-line -->` / `<!-- mdlint-disable-next-line MD001 -->`
///
/// Returns a map from rule name (or `"*"` for all rules) to the set of suppressed line numbers.
fn parse_inline_config(content: &str) -> HashMap<String, HashSet<usize>> {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Active disable ranges awaiting a matching enable: rule -> start line
    let mut active: HashMap<String, usize> = HashMap::new();
    // Completed ranges: rule -> [(start, end)]
    let mut ranges: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let Some((kind, rule_names)) = extract_directive(line) else {
            continue;
        };
        match kind {
            DirectiveKind::DisableNextLine => {
                let next = line_num + 1;
                for rule in rules_or_all(rule_names) {
                    ranges.entry(rule).or_default().push((next, next));
                }
            }
            DirectiveKind::Disable => {
                for rule in rules_or_all(rule_names) {
                    active.entry(rule).or_insert(line_num);
                }
            }
            DirectiveKind::Enable => {
                let to_enable = rules_or_all(rule_names);
                if to_enable.contains(&"*".to_owned()) {
                    for (rule, start) in active.drain() {
                        ranges.entry(rule).or_default().push((start, line_num - 1));
                    }
                } else {
                    for rule in to_enable {
                        if let Some(start) = active.remove(&rule) {
                            ranges.entry(rule).or_default().push((start, line_num - 1));
                        }
                    }
                }
            }
        }
    }

    // Close any remaining open disables at end of document
    for (rule, start) in active {
        ranges.entry(rule).or_default().push((start, total_lines));
    }

    // Expand ranges into per-line sets
    let mut suppressed: HashMap<String, HashSet<usize>> = HashMap::new();
    for (rule, rule_ranges) in ranges {
        let entry = suppressed.entry(rule).or_default();
        for (start, end) in rule_ranges {
            entry.extend(start..=end);
        }
    }
    suppressed
}

enum DirectiveKind {
    Disable,
    Enable,
    DisableNextLine,
}

/// Extract an mdlint directive from a line, returning the kind and the list of rule names
/// (empty = apply to all rules). Returns `None` if the line contains no directive.
fn extract_directive(line: &str) -> Option<(DirectiveKind, Vec<String>)> {
    let start = line.find("<!--")?;
    let end = line[start..].find("-->")?;
    let body = line[start + 4..start + end].trim();

    if let Some(rest) = body.strip_prefix("mdlint-disable-next-line") {
        Some((DirectiveKind::DisableNextLine, parse_rule_names(rest)))
    } else if let Some(rest) = body.strip_prefix("mdlint-disable") {
        Some((DirectiveKind::Disable, parse_rule_names(rest)))
    } else {
        body.strip_prefix("mdlint-enable")
            .map(|rest| (DirectiveKind::Enable, parse_rule_names(rest)))
    }
}

fn parse_rule_names(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn rules_or_all(rules: Vec<String>) -> Vec<String> {
    if rules.is_empty() {
        vec!["*".to_owned()]
    } else {
        rules
    }
}

/// Convert a TOML value to a JSON value
fn toml_to_json(toml_val: toml::Value) -> Value {
    match toml_val {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Number(i.into()),
        toml::Value::Float(f) => {
            Value::Number(serde_json::Number::from_f64(f).unwrap_or_else(|| 0i32.into()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use indoc::indoc;

    fn engine_all_rules() -> LintEngine {
        LintEngine::new(Config {
            default_enabled: true,
            ..Config::default()
        })
    }

    #[test]
    fn test_disable_next_line_specific_rule() {
        // MD018: no space after hash. Line 2 has `#Heading` — suppressed by disable-next-line on line 1.
        let content = indoc! {"
            <!-- mdlint-disable-next-line MD018 -->
            #Heading without space
        "};
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        assert!(
            violations.iter().all(|v| v.rule != "MD018"),
            "MD018 should be suppressed on line 2: {violations:?}"
        );
    }

    #[test]
    fn test_disable_next_line_does_not_suppress_two_lines_ahead() {
        // The disable-next-line on line 1 suppresses line 2, NOT line 3
        let content = indoc! {"
            <!-- mdlint-disable-next-line MD018 -->
            # Good heading
            #Bad heading
        "};
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        // MD018 on line 3 should still fire
        assert!(
            violations.iter().any(|v| v.rule == "MD018" && v.line == 3),
            "MD018 on line 3 should not be suppressed: {violations:?}"
        );
    }

    #[test]
    fn test_disable_enable_specific_rule() {
        // Disable MD041, then re-enable it; violations between should be suppressed
        let content = indoc! {"
            <!-- mdlint-disable MD041 -->
            No heading here
            <!-- mdlint-enable MD041 -->
        "};
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        assert!(
            violations.iter().all(|v| v.rule != "MD041"),
            "MD041 should be suppressed in disabled range: {violations:?}"
        );
    }

    #[test]
    fn test_disable_all_rules() {
        let content = indoc! {"
            <!-- mdlint-disable -->
            No heading here
            <!-- mdlint-enable -->
        "};
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        // All rules suppressed from line 1 to line 2 (enable on line 3)
        let lines_12: Vec<_> = violations.iter().filter(|v| v.line <= 2).collect();
        assert!(
            lines_12.is_empty(),
            "Lines 1-2 should have no violations: {violations:?}"
        );
    }

    #[test]
    fn test_no_inline_config_flag_disables_parsing() {
        let content = indoc! {"
            <!-- mdlint-disable MD041 -->
            No heading here
        "};
        let engine = LintEngine::new(Config {
            default_enabled: true,
            no_inline_config: true,
            ..Config::default()
        });
        let violations = engine.lint_content(content).unwrap();
        // With no_inline_config, the directive is ignored — MD041 should still fire
        assert!(
            violations.iter().any(|v| v.rule == "MD041"),
            "MD041 should NOT be suppressed when no_inline_config=true: {violations:?}"
        );
    }

    #[test]
    fn test_disable_without_enable_suppresses_to_end() {
        let content = indoc! {"
            # Heading

            <!-- mdlint-disable MD013 -->
            A very long line that goes on and on and on and on and on and on and on and on and on and on and on and on and on
        "};
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        assert!(
            violations.iter().all(|v| v.rule != "MD013"),
            "MD013 should be suppressed to end of file: {violations:?}"
        );
    }
}
