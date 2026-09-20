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
        if !self.config.rule_enabled(rule.name()) {
            return Vec::new();
        }
        let config_value = match self.config.config().get(rule.name()) {
            Some(RuleConfig::Config(cfg)) => {
                // Convert TOML config to JSON for rule consumption
                let mut table = toml::map::Map::new();
                table.extend(cfg.clone());
                Some(toml_to_json(toml::Value::Table(table)))
            }
            _ => None,
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
pub(crate) fn parse_inline_config(content: &str) -> HashMap<String, HashSet<usize>> {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Active disable ranges awaiting a matching enable: rule -> start line
    let mut active: HashMap<String, usize> = HashMap::new();
    // Completed ranges: rule -> [(start, end)]
    let mut ranges: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

    // Fenced code blocks hold *examples* of directives, not directives.  Reading
    // them lets documentation reach out and change the document that contains it:
    // this project's own README silently suppressed MD013 from the middle of the
    // file to the end because an example opened a disable it never closed.
    let mut fence: Option<(char, usize)> = None;

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim_start();

        if let Some((delimiter, opened_len)) = fence {
            let run = trimmed.chars().take_while(|&ch| ch == delimiter).count();
            if run >= opened_len && trimmed[run..].trim().is_empty() {
                fence = None;
            }
            continue;
        }
        let backticks = trimmed.chars().take_while(|&ch| ch == '`').count();
        let tildes = trimmed.chars().take_while(|&ch| ch == '~').count();
        if backticks >= 3 {
            fence = Some(('`', backticks));
            continue;
        }
        if tildes >= 3 {
            fence = Some(('~', tildes));
            continue;
        }

        let Some((kind, rule_names)) = extract_directive(line) else {
            continue;
        };
        match kind {
            DirectiveKind::DisableNextLine => {
                // Attach to the next line with content, not to whitespace.  The
                // formatter inserts a blank line after the comment when it sits
                // against a block, and the directive has to survive that or it
                // would protect the blank line and release the text.
                let next = (line_num..total_lines)
                    .find(|&index| !lines[index].trim().is_empty())
                    .map_or(line_num + 1, |index| index + 1);
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
    // The comment must own the line.  Matching one anywhere lets prose *about*
    // directives act as one: this project's README disabled MD013 from the middle
    // of the file to the end because a sentence quoted a directive in a code span.
    let trimmed = line.trim();
    if !trimmed.starts_with("<!--") || !trimmed.ends_with("-->") {
        return None;
    }
    let body = trimmed[4..trimmed.len() - 3].trim();

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

    #[test]
    fn test_disable_next_line_skips_blank_lines() {
        // `mdlint format` puts a blank line between an HTML comment and the block
        // that follows it, so a directive that counted lines literally would
        // protect the blank and release the content on the next pass.
        let content = indoc! {"
            <!-- mdlint-disable-next-line MD018 -->

            #Heading
        "};
        let suppressed = parse_inline_config(content);
        assert_eq!(
            suppressed.get("MD018").map(|lines| lines.contains(&3)),
            Some(true),
            "directive should reach past the blank line to line 3"
        );
    }

    #[test]
    fn test_directives_inside_code_fences_are_examples_not_directives() {
        // Documentation must not be able to change the document containing it.
        let content = indoc! {"
            ```markdown
            <!-- mdlint-disable MD013 -->
            ```

            a line that should still be checked
        "};
        assert!(
            parse_inline_config(content).is_empty(),
            "a directive inside a fence must have no effect"
        );
    }

    #[test]
    fn test_directives_after_a_closed_fence_still_apply() {
        let content = indoc! {"
            ```text
            not a directive
            ```

            <!-- mdlint-disable MD013 -->
            suppressed
        "};
        let suppressed = parse_inline_config(content);
        assert_eq!(
            suppressed.get("MD013").map(|lines| lines.contains(&6)),
            Some(true),
            "a real directive after the fence must still work"
        );
    }

    #[test]
    fn test_directive_quoted_in_prose_is_not_a_directive() {
        // A sentence describing a directive must not apply it.
        let content = indoc! {"
            Use `<!-- mdlint-disable MD013 -->` to switch the rule off.

            a line that should still be checked
        "};
        assert!(
            parse_inline_config(content).is_empty(),
            "a directive quoted mid-line must have no effect"
        );
    }

    #[test]
    fn test_directive_in_a_table_cell_is_not_a_directive() {
        let content = indoc! {"
            | Comment | Effect |
            | --- | --- |
            | `<!-- mdlint-disable -->` | Disable all rules |
        "};
        assert!(
            parse_inline_config(content).is_empty(),
            "a directive inside a table cell must have no effect"
        );
    }
}
