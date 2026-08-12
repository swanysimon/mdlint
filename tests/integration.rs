use mdlint::config::Config;
use mdlint::fix::Fixer;
use mdlint::formatter;
use mdlint::lint::LintEngine;
use std::fs;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn fixture(path: &str) -> String {
    let full = format!("tests/fixtures/{path}");
    fs::read_to_string(&full).unwrap_or_else(|e| panic!("cannot read fixture {full}: {e}"))
}

fn all_rules_engine() -> LintEngine {
    LintEngine::new(Config {
        default_enabled: true,
        ..Config::default()
    })
}

// ── Format workflow tests ─────────────────────────────────────────────────────

#[test]
fn format_produces_expected_output() {
    let input = fixture("format/input.md");
    let expected = fixture("format/expected.md");
    let got = formatter::format(&input);
    assert_eq!(got, expected, "formatter output did not match golden file");
}

#[test]
fn format_is_idempotent_on_expected() {
    let expected = fixture("format/expected.md");
    let twice = formatter::format(&expected);
    assert_eq!(
        expected, twice,
        "formatting the already-formatted file produced different output"
    );
}

#[test]
fn format_empty_string_is_empty() {
    assert_eq!(formatter::format(""), "");
}

#[test]
fn format_whitespace_only_is_empty() {
    assert_eq!(formatter::format("   \n\n  \t\n"), "");
}

// ── Check workflow tests ──────────────────────────────────────────────────────

#[test]
fn check_detects_heading_level_skip() {
    let content = fixture("check/violations.md");
    let engine = all_rules_engine();
    let violations = engine.lint_content(&content).unwrap();
    assert!(
        violations.iter().any(|v| v.rule == "MD001"),
        "MD001 (heading level skip) should fire; got: {violations:?}"
    );
}

#[test]
fn check_detects_trailing_spaces() {
    // Use inline content so trailing spaces aren't stripped by the editor
    let content = "# Heading\n\nSome text   \nMore text\n";
    let engine = all_rules_engine();
    let violations = engine.lint_content(content).unwrap();
    assert!(
        violations.iter().any(|v| v.rule == "MD009"),
        "MD009 (trailing spaces) should fire; got: {violations:?}"
    );
}

#[test]
fn check_fix_removes_trailing_spaces() {
    let content = "# Heading\n\nSome text   \nMore text\n";
    let engine = all_rules_engine();
    let violations = engine.lint_content(content).unwrap();
    let fixes: Vec<_> = violations.iter().filter_map(|v| v.fix.clone()).collect();
    assert!(!fixes.is_empty(), "MD009 should produce an inline fix");
    let result = Fixer::new()
        .apply_fixes_to_content(content, &fixes)
        .unwrap();
    assert_eq!(result, "# Heading\n\nSome text\nMore text\n");
}

#[test]
fn check_detects_hard_tabs() {
    let content = fixture("check/violations.md");
    let engine = all_rules_engine();
    let violations = engine.lint_content(&content).unwrap();
    assert!(
        violations.iter().any(|v| v.rule == "MD010"),
        "MD010 (hard tabs) should fire; got: {violations:?}"
    );
}

#[test]
fn check_fix_replaces_hard_tabs() {
    let content = "# Heading\n\n\tTabbed line\n";
    let engine = all_rules_engine();
    let violations = engine.lint_content(content).unwrap();
    let fixes: Vec<_> = violations.iter().filter_map(|v| v.fix.clone()).collect();
    assert!(!fixes.is_empty(), "MD010 should produce an inline fix");
    let result = Fixer::new()
        .apply_fixes_to_content(content, &fixes)
        .unwrap();
    assert!(!result.contains('\t'), "tabs should be replaced after fix");
}

#[test]
fn format_output_for_nested_list_passes_check() {
    // Regression test for issue #67: `mdlint format`'s output for a bullet item
    // containing a nested ordered sub-list must pass `mdlint check` cleanly.
    // The formatter used to strip the blank lines around the nested list
    // (intended, canonical behavior), but MD032 then misjudged the nested,
    // differently-marked sub-list as a set of separate top-level lists,
    // reporting spurious violations on the formatter's own output.
    let content = "# Example\n\n- First item:\n\n  1. One\n  2. Two\n\n- Second item\n";
    let formatted = formatter::format(content);
    let engine = all_rules_engine();
    let violations = engine.lint_content(&formatted).unwrap();
    assert!(
        violations.is_empty(),
        "mdlint check should report no violations on mdlint format's output; got: {violations:?}\nformatted:\n{formatted}"
    );
}

#[test]
fn check_clean_file_has_no_violations() {
    let content = fixture("format/expected.md");
    let engine = LintEngine::new(Config {
        default_enabled: true,
        rules: {
            // MD041 requires a top-level heading — our fixture has one, but MD013
            // line length and other cosmetic rules might fire; only disable none.
            std::collections::HashMap::new()
        },
        ..Config::default()
    });
    let violations = engine.lint_content(&content).unwrap();
    // Only allow violations that are expected (none for a well-formed file)
    // Filter out MD041 if the expected.md doesn't start with a top-level heading
    let non_trivial: Vec<_> = violations
        .iter()
        .filter(|v| !matches!(v.rule.as_str(), "MD013" | "MD043"))
        .collect();
    assert!(
        non_trivial.is_empty(),
        "formatted file should have no violations (except MD013/MD043): {non_trivial:?}"
    );
}
