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

        // Front matter is not markdown content: exclude it from every rule (MD025
        // instead treats its `title` field as the document's first heading).
        // Comments are excluded from every rule except MD013, since line length
        // is a mechanical property of the raw line, not a markdown-semantics one.
        violations.retain(|v| {
            !parser.front_matter_lines().contains(&v.line)
                && (v.rule == "MD013" || !parser.comment_lines().contains(&v.line))
        });

        if !self.config.no_inline_config {
            let all_rule_names: Vec<String> = self
                .registry
                .all_rules()
                .map(|rule| rule.name().to_owned())
                .collect();
            let suppressed = parse_inline_config(content, &all_rule_names);
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
/// Both `mdlint-` and `markdownlint-` prefixes are accepted as equivalent aliases.
/// Supports:
/// - `<!-- mdlint-disable -->` / `<!-- mdlint-disable MD001 MD003 -->`
/// - `<!-- mdlint-enable -->` / `<!-- mdlint-enable MD001 -->`
/// - `<!-- mdlint-disable-line -->` / `<!-- mdlint-disable-line MD001 -->`
/// - `<!-- mdlint-disable-next-line -->` / `<!-- mdlint-disable-next-line MD001 -->`
/// - `<!-- mdlint-capture -->` / `<!-- mdlint-restore -->` — push/pop the current
///   disable/enable state, so a block of directives can be undone as a unit.
///
/// Returns a map from rule name to the set of suppressed line numbers.
fn parse_inline_config(
    content: &str,
    all_rule_names: &[String],
) -> HashMap<String, HashSet<usize>> {
    // `default_disabled` is the state set by a bare disable/enable (applies to
    // every rule); `overrides` holds rule-specific exceptions layered on top of
    // it, so `disable` followed by `enable MD013` means "all rules except MD013".
    let mut default_disabled = false;
    let mut overrides: HashMap<String, bool> = HashMap::new();
    let mut capture_stack: Vec<(bool, HashMap<String, bool>)> = Vec::new();
    let mut pending_next_line: Option<Vec<String>> = None;

    let mut suppressed: HashMap<String, HashSet<usize>> = HashMap::new();

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let mut disable_line_rules: Vec<String> = Vec::new();

        // A disable-next-line directive on the *previous* line applies here.
        if let Some(rules) = pending_next_line.take() {
            for rule in rules {
                suppressed.entry(rule).or_default().insert(line_num);
            }
        }

        for (kind, rule_names) in extract_directives(line) {
            match kind {
                DirectiveKind::Disable => {
                    if rule_names.is_empty() {
                        default_disabled = true;
                        overrides.clear();
                    } else {
                        overrides.extend(rule_names.into_iter().map(|r| (r, true)));
                    }
                }
                DirectiveKind::Enable => {
                    if rule_names.is_empty() {
                        default_disabled = false;
                        overrides.clear();
                    } else {
                        overrides.extend(rule_names.into_iter().map(|r| (r, false)));
                    }
                }
                DirectiveKind::DisableLine => {
                    disable_line_rules.extend(rules_or_all(rule_names, all_rule_names));
                }
                DirectiveKind::DisableNextLine => {
                    pending_next_line = Some(rules_or_all(rule_names, all_rule_names));
                }
                DirectiveKind::Capture => {
                    capture_stack.push((default_disabled, overrides.clone()));
                }
                DirectiveKind::Restore => {
                    if let Some((d, o)) = capture_stack.pop() {
                        default_disabled = d;
                        overrides = o;
                    }
                }
            }
        }

        // Block-level suppression: the state as of *this* line (i.e. including any
        // disable/enable directive on this line) applies to this line, matching
        // markdownlint semantics where a disable comment suppresses its own line
        // and a matching enable comment does not.
        if default_disabled || !overrides.is_empty() {
            for rule in all_rule_names {
                let disabled = *overrides.get(rule).unwrap_or(&default_disabled);
                if disabled {
                    suppressed.entry(rule.clone()).or_default().insert(line_num);
                }
            }
        }

        for rule in disable_line_rules {
            suppressed.entry(rule).or_default().insert(line_num);
        }
    }

    suppressed
}

enum DirectiveKind {
    Disable,
    Enable,
    DisableLine,
    DisableNextLine,
    Capture,
    Restore,
}

/// Extract every mdlint/markdownlint directive from a line. Usually a line has at
/// most one, but each `<!-- ... -->` comment on the line is scanned independently.
fn extract_directives(line: &str) -> Vec<(DirectiveKind, Vec<String>)> {
    let mut directives = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("<!--") {
        let Some(end) = rest[start..].find("-->") else {
            break;
        };
        let body = rest[start + 4..start + end].trim();
        if let Some(directive) = parse_directive_body(body) {
            directives.push(directive);
        }
        rest = &rest[start + end + 3..];
    }
    directives
}

/// Parse a single comment body (already stripped of `<!--`/`-->`) into a directive
/// kind and its rule-name arguments (empty = apply to all rules). Returns `None`
/// if the body isn't a recognized directive.
fn parse_directive_body(body: &str) -> Option<(DirectiveKind, Vec<String>)> {
    let rest = body
        .strip_prefix("markdownlint-")
        .or_else(|| body.strip_prefix("mdlint-"))?;

    // Longest-prefix-first so "disable-next-line"/"disable-line" aren't shadowed
    // by the shorter "disable" prefix.
    let (kind, rest) = if let Some(r) = rest.strip_prefix("disable-next-line") {
        (DirectiveKind::DisableNextLine, r)
    } else if let Some(r) = rest.strip_prefix("disable-line") {
        (DirectiveKind::DisableLine, r)
    } else if let Some(r) = rest.strip_prefix("disable") {
        (DirectiveKind::Disable, r)
    } else if let Some(r) = rest.strip_prefix("enable") {
        (DirectiveKind::Enable, r)
    } else if let Some(r) = rest.strip_prefix("capture") {
        (DirectiveKind::Capture, r)
    } else if let Some(r) = rest.strip_prefix("restore") {
        (DirectiveKind::Restore, r)
    } else {
        return None;
    };

    Some((kind, parse_rule_names(rest)))
}

fn parse_rule_names(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn rules_or_all(rules: Vec<String>, all_rule_names: &[String]) -> Vec<String> {
    if rules.is_empty() {
        all_rule_names.to_vec()
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

    fn engine_all_rules() -> LintEngine {
        LintEngine::new(Config {
            default_enabled: true,
            ..Config::default()
        })
    }

    #[test]
    fn test_disable_next_line_specific_rule() {
        // MD018: no space after hash. Line 2 has `#Heading` — suppressed by disable-next-line on line 1.
        let content = "<!-- mdlint-disable-next-line MD018 -->\n#Heading without space\n";
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
        let content = "<!-- mdlint-disable-next-line MD018 -->\n# Good heading\n#Bad heading\n";
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
        let content =
            "<!-- mdlint-disable MD041 -->\nNo heading here\n<!-- mdlint-enable MD041 -->\n";
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        assert!(
            violations.iter().all(|v| v.rule != "MD041"),
            "MD041 should be suppressed in disabled range: {violations:?}"
        );
    }

    #[test]
    fn test_disable_all_rules() {
        let content = "<!-- mdlint-disable -->\nNo heading here\n<!-- mdlint-enable -->\n";
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
        let content = "<!-- mdlint-disable MD041 -->\nNo heading here\n";
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
        let content = "# Heading\n\n<!-- mdlint-disable MD013 -->\nA very long line that goes on and on and on and on and on and on and on and on and on and on and on and on and on and on and on\n";
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        assert!(
            violations.iter().all(|v| v.rule != "MD013"),
            "MD013 should be suppressed to end of file: {violations:?}"
        );
    }

    #[test]
    fn test_markdownlint_prefix_alias_disable_next_line() {
        let content = "<!-- markdownlint-disable-next-line MD018 -->\n#Heading without space\n";
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        assert!(
            violations.iter().all(|v| v.rule != "MD018"),
            "markdownlint- prefix should behave like mdlint-: {violations:?}"
        );
    }

    #[test]
    fn test_markdownlint_prefix_alias_disable_enable() {
        let content = "<!-- markdownlint-disable MD041 -->\nNo heading here\n<!-- markdownlint-enable MD041 -->\n";
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        assert!(
            violations.iter().all(|v| v.rule != "MD041"),
            "markdownlint-disable/enable should behave like mdlint-: {violations:?}"
        );
    }

    #[test]
    fn test_disable_line_suppresses_only_current_line() {
        let content = "#Bad heading <!-- mdlint-disable-line MD018 -->\n#Also bad heading\n";
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        assert!(
            violations
                .iter()
                .all(|v| !(v.rule == "MD018" && v.line == 1)),
            "MD018 should be suppressed on line 1: {violations:?}"
        );
        assert!(
            violations.iter().any(|v| v.rule == "MD018" && v.line == 2),
            "MD018 on line 2 should still fire: {violations:?}"
        );
    }

    #[test]
    fn test_disable_then_enable_specific_rule_means_all_except() {
        // "disable" (bare) then "enable MD013" should leave everything else
        // disabled but MD013 checked.
        let content = "<!-- mdlint-disable -->\n<!-- mdlint-enable MD013 -->\n#Bad heading\nA very long line that goes on and on and on and on and on and on and on and on and on and on and on and on and on and on and on\n";
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        assert!(
            violations.iter().all(|v| v.rule != "MD018"),
            "MD018 should stay suppressed under the blanket disable: {violations:?}"
        );
        assert!(
            violations.iter().any(|v| v.rule == "MD013"),
            "MD013 should be checked again after being explicitly enabled: {violations:?}"
        );
    }

    #[test]
    fn test_capture_restore_roundtrip() {
        // capture saves the (enabled) state; a disable inside the captured scope
        // is undone by restore, so MD013 fires again afterward.
        let content = "<!-- mdlint-capture -->\n<!-- mdlint-disable MD013 -->\n<!-- mdlint-restore -->\nA very long line that goes on and on and on and on and on and on and on and on and on and on and on and on and on and on and on\n";
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        assert!(
            violations.iter().any(|v| v.rule == "MD013"),
            "MD013 should fire again after restore undoes the nested disable: {violations:?}"
        );
    }

    #[test]
    fn test_disable_persists_through_restore_when_captured_while_disabled() {
        // capture saves the (disabled) state; a nested disable of the same rule
        // is a no-op, so MD013 stays suppressed after restore too.
        let content = "<!-- mdlint-disable MD013 -->\n<!-- mdlint-capture -->\n<!-- mdlint-disable MD013 -->\n<!-- mdlint-restore -->\nA very long line that goes on and on and on and on and on and on and on and on and on and on and on and on and on and on and on\n";
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();
        assert!(
            violations.iter().all(|v| v.rule != "MD013"),
            "MD013 should remain suppressed: {violations:?}"
        );
    }

    #[test]
    fn test_restore_without_capture_is_a_no_op() {
        let content = "<!-- mdlint-restore -->\n#Bad heading\n";
        let engine = engine_all_rules();
        // Should not panic and should behave as if no directive was present.
        let violations = engine.lint_content(content).unwrap();
        assert!(violations.iter().any(|v| v.rule == "MD018"));
    }

    /// Acceptance test for GitHub issue #51: front matter and HTML comments
    /// should be excluded from checking (except MD013, which still measures raw
    /// line length), a front matter `title` should count as the document's first
    /// top-level heading for MD025, and both `mdlint-` and `markdownlint-`
    /// prefixed inline directives should be recognized. This is the exact
    /// `test.md` from the issue; the exact violation set below is the issue's
    /// own "Expected output".
    #[test]
    fn test_issue_51_acceptance() {
        let content = "---\ntitle: Metadata should not be handled as markdown\ntags:\n    - a\n    - b\nreference: http://example.com/as/bare/url\n---\n<!--\nComments should be ignored.\n\nReferences: http://example.com/as/bare/url\n-->\n\n<!-- (a) ignore comments during checking and linting -->\n<!-- (b) MD025 should pay attention to the metadata (title) and should report \"Multiple top-level headings in the same document\" -->\n# Test Markdown File\n\nSupport for temporary enable and disable rules via:\n<!-- markdownlint-disable MD0XX MD0XY -->\nand maybe also\n<!-- mdlint-disable MD0XX MD0XY -->\nsimilar to <https://github.com/DavidAnson/markdownlint#configuration>.\nAn example would be:\n\n<!-- markdownlint-disable-next-line MD025 -->\n# Another but this time valid top-level header\n";
        let engine = engine_all_rules();
        let violations = engine.lint_content(content).unwrap();

        assert_eq!(
            violations.len(),
            2,
            "expected exactly MD013@15 and MD025@16: {violations:?}"
        );
        assert!(
            violations.iter().any(|v| v.rule == "MD013" && v.line == 15),
            "expected MD013 on line 15 (the over-long comment): {violations:?}"
        );
        assert!(
            violations.iter().any(|v| v.rule == "MD025"
                && v.line == 16
                && v.message.contains("first h1 at line 2")),
            "expected MD025 on line 16 referencing the front matter title at line 2: {violations:?}"
        );
    }
}
