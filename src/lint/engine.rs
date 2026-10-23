use crate::config::{Config, RuleConfig};
use crate::error::Result;
use crate::lint::aliases::aliases_for;
use crate::lint::{Rule, RuleRegistry};
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

pub struct LintEngine {
    config: Config,
    registry: RuleRegistry,
    directives: DirectiveNames,
}

impl LintEngine {
    #[must_use]
    pub fn new(config: Config) -> Self {
        let registry = crate::lint::rules::create_default_registry();
        let directives = DirectiveNames::new(&registry);
        Self {
            config,
            registry,
            directives,
        }
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
            !parser.is_front_matter_line(v.line)
                && (v.rule == "MD013" || !parser.comment_lines().contains(&v.line))
        });

        if !self.config.no_inline_config {
            let suppressed = parse_inline_config(&parser, &self.directives);
            if !suppressed.is_empty() {
                violations.retain(|v| {
                    suppressed
                        .get(v.rule.as_str())
                        .is_none_or(|lines| !lines.contains(&v.line))
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

/// Resolves the rule names a directive lists — rule codes, markdownlint
/// aliases, and tags, all case-insensitive as in markdownlint — to rule codes.
struct DirectiveNames {
    all: Vec<String>,
    by_name: HashMap<String, Vec<String>>,
}

impl DirectiveNames {
    fn new(registry: &RuleRegistry) -> Self {
        let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
        for rule in registry.all_rules() {
            let code = rule.name();
            let mut add = |name: &str| {
                by_name
                    .entry(name.to_ascii_uppercase())
                    .or_default()
                    .push(code.to_owned());
            };
            add(code);
            aliases_for(code).for_each(&mut add);
            rule.tags().iter().copied().for_each(add);
        }
        let all = registry
            .all_rules()
            .map(|rule| rule.name().to_owned())
            .collect();
        Self { all, by_name }
    }

    /// The rules a directive targets: every rule when it names none, otherwise
    /// the ones its names resolve to. Unknown names resolve to nothing rather
    /// than widening the directive to every rule.
    fn resolve(&self, names: &[String]) -> Vec<String> {
        if names.is_empty() {
            return self.all.clone();
        }
        names
            .iter()
            .filter_map(|name| self.by_name.get(&name.to_ascii_uppercase()))
            .flatten()
            .cloned()
            .collect()
    }
}

/// Parse inline configuration comments.
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
/// Only comments Markdown recognises count, so a directive shown in a code
/// block or code span is an example, not an instruction.
///
/// Returns a map from rule name to the set of suppressed line numbers.
fn parse_inline_config(
    parser: &MarkdownParser,
    names: &DirectiveNames,
) -> HashMap<String, HashSet<usize>> {
    let mut suppressed: HashMap<String, HashSet<usize>> = HashMap::new();
    let mut suppress = |rules: Vec<String>, line: usize| {
        for rule in rules {
            suppressed.entry(rule).or_default().insert(line);
        }
    };

    // Directives that change the running state, keyed by the line they start on.
    let mut stateful: BTreeMap<usize, Vec<(DirectiveKind, Vec<String>)>> = BTreeMap::new();
    for comment in parser.comments() {
        let Some((kind, rule_names)) = parser
            .content()
            .get(comment.clone())
            .and_then(parse_directive)
        else {
            continue;
        };
        let start_line = parser.offset_to_line(comment.start);
        match kind {
            DirectiveKind::DisableLine => suppress(names.resolve(&rule_names), start_line),
            DirectiveKind::DisableNextLine => {
                let end_line = parser.offset_to_line(comment.end.saturating_sub(1));
                suppress(names.resolve(&rule_names), end_line + 1);
            }
            _ => stateful
                .entry(start_line)
                .or_default()
                .push((kind, rule_names)),
        }
    }

    // `default_disabled` is the state set by a bare disable/enable (applies to
    // every rule); `overrides` holds rule-specific exceptions layered on top of
    // it, so `disable` followed by `enable MD013` means "all rules except MD013".
    let mut default_disabled = false;
    let mut overrides: HashMap<String, bool> = HashMap::new();
    let mut capture_stack: Vec<(bool, HashMap<String, bool>)> = Vec::new();

    for line_num in 1..=parser.line_count() {
        for (kind, rule_names) in stateful.remove(&line_num).unwrap_or_default() {
            match kind {
                DirectiveKind::Disable if rule_names.is_empty() => {
                    default_disabled = true;
                    overrides.clear();
                }
                DirectiveKind::Enable if rule_names.is_empty() => {
                    default_disabled = false;
                    overrides.clear();
                }
                DirectiveKind::Disable | DirectiveKind::Enable => {
                    let disabled = matches!(kind, DirectiveKind::Disable);
                    overrides.extend(
                        names
                            .resolve(&rule_names)
                            .into_iter()
                            .map(|r| (r, disabled)),
                    );
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
                DirectiveKind::DisableLine | DirectiveKind::DisableNextLine => {
                    unreachable!("applied when collected")
                }
            }
        }

        // The state as of *this* line (i.e. including any disable/enable
        // directive on this line) applies to this line, matching markdownlint
        // semantics where a disable comment suppresses its own line and a
        // matching enable comment does not.
        if default_disabled || !overrides.is_empty() {
            let disabled = names
                .all
                .iter()
                .filter(|rule| *overrides.get(*rule).unwrap_or(&default_disabled))
                .cloned()
                .collect();
            suppress(disabled, line_num);
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

/// Parse one comment (`<!-- ... -->`, the closer optional when unterminated)
/// into a directive kind and its rule-name arguments (empty = apply to all
/// rules). Returns `None` if the comment isn't a recognized directive.
fn parse_directive(comment: &str) -> Option<(DirectiveKind, Vec<String>)> {
    let body = comment.strip_prefix("<!--")?;
    let body = body.strip_suffix("-->").unwrap_or(body).trim();
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
    } else {
        let r = rest.strip_prefix("restore")?;
        (DirectiveKind::Restore, r)
    };

    Some((kind, parse_rule_names(rest)))
}

fn parse_rule_names(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
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
            A very long line that goes on and on and on and on and on and on and on and on and on and on and on and on and on and on and on
        "};
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

    const LONG_LINE: &str = "A very long line that goes on and on and on and on and on and on and on and on and on and on and on and on and on and on and on";

    #[test]
    fn test_directive_names_resolve_aliases_tags_and_case() {
        for name in ["line-length", "md013", "Line_Length"] {
            let content = format!("# T\n\n<!-- markdownlint-disable {name} -->\n{LONG_LINE}\n");
            let violations = engine_all_rules().lint_content(&content).unwrap();
            assert!(
                violations.iter().all(|v| v.rule != "MD013"),
                "`{name}` should name MD013: {violations:?}"
            );
        }
    }

    #[test]
    fn test_tag_directive_covers_every_rule_with_the_tag() {
        // MD018 and MD019 are both tagged `atx`.
        let content = "<!-- mdlint-disable atx -->\n#No space\n\n##  Two spaces\n";
        let violations = engine_all_rules().lint_content(content).unwrap();
        assert!(
            violations
                .iter()
                .all(|v| v.rule != "MD018" && v.rule != "MD019"),
            "{violations:?}"
        );
    }

    #[test]
    fn test_unknown_rule_names_disable_nothing() {
        let content = "# T\n\n<!-- mdlint-disable MD0XX -->\n#No space\n";
        let violations = engine_all_rules().lint_content(content).unwrap();
        assert!(
            violations.iter().any(|v| v.rule == "MD018"),
            "an unknown name must not widen to every rule: {violations:?}"
        );
    }

    #[test]
    fn test_directive_in_fenced_code_block_is_ignored() {
        let content = "# T\n\n```markdown\n<!-- mdlint-disable -->\n```\n\n#No space\n";
        let violations = engine_all_rules().lint_content(content).unwrap();
        assert!(
            violations.iter().any(|v| v.rule == "MD018" && v.line == 7),
            "a directive shown as an example must not apply: {violations:?}"
        );
    }

    #[test]
    fn test_directive_in_code_span_is_ignored() {
        let content = "# T\n\nWrite `<!-- mdlint-disable -->` to disable.\n\n#No space\n";
        let violations = engine_all_rules().lint_content(content).unwrap();
        assert!(
            violations.iter().any(|v| v.rule == "MD018" && v.line == 5),
            "{violations:?}"
        );
    }

    #[test]
    fn test_code_span_comment_opener_does_not_hide_later_lines() {
        let content = "# T\n\nUse `<!--` to open a comment.\n\n#No space\n";
        let violations = engine_all_rules().lint_content(content).unwrap();
        assert!(
            violations.iter().any(|v| v.rule == "MD018" && v.line == 5),
            "{violations:?}"
        );
    }

    #[test]
    fn test_two_disable_next_line_directives_on_one_line() {
        let content = format!(
            "# T\n\n<!-- mdlint-disable-next-line MD018 --> <!-- mdlint-disable-next-line MD013 -->\n#No space {LONG_LINE}\n"
        );
        let violations = engine_all_rules().lint_content(&content).unwrap();
        assert!(
            violations
                .iter()
                .all(|v| v.line != 4 || (v.rule != "MD018" && v.rule != "MD013")),
            "both directives should apply: {violations:?}"
        );
    }

    #[test]
    fn test_multi_line_disable_next_line_targets_line_after_comment() {
        let content = "# T\n\n<!-- mdlint-disable-next-line\n     MD018 -->\n#No space\n";
        let violations = engine_all_rules().lint_content(content).unwrap();
        assert!(
            violations.iter().all(|v| v.rule != "MD018"),
            "{violations:?}"
        );
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
