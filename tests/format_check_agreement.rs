//! `mdlint format` and `mdlint check` must never disagree: formatted output
//! passes every rule the formatter owns, and applying `check`'s fixes first
//! leads `format` to the same result.

use mdlint::config::Config;
use mdlint::formatter;
use mdlint::lint::LintEngine;
use mdlint::lint::rules::create_default_registry;
use proptest::prelude::*;
use std::collections::HashSet;

/// Rules whose style `mdlint format` enforces: the ones that declare it
/// (`fixable`), plus `FORMAT_SPEC.md`'s Canonical Style Rules that have no fix.
fn formatter_owned_rules() -> HashSet<String> {
    let mut rules: HashSet<String> = create_default_registry()
        .all_rules()
        .filter(|rule| rule.fixable())
        .map(|rule| rule.name().to_owned())
        .collect();
    rules.extend(["MD003", "MD007", "MD028", "MD032", "MD048", "MD054"].map(str::to_owned));
    rules
}

fn engine() -> LintEngine {
    LintEngine::new(Config {
        default_enabled: true,
        ..Config::default()
    })
}

fn document() -> impl Strategy<Value = String> {
    (
        proptest::bool::ANY,
        proptest::collection::vec(
            // The idempotency proptest's lines, minus `---` (a setext underline
            // under multi-line text, where MD022's fix splits the heading) and
            // raw `<div>` blocks (line-scanning rules such as MD004 read their
            // contents as Markdown), and `* star` (MD004's fix merges it into an
            // adjacent `-` list). All are pre-existing check bugs unrelated to
            // comments and front matter.
            proptest::sample::select(vec![
                "<!-- c -->",
                "<!-- mdlint-disable-next-line MD025 -->",
                "<!--\nnote\n-->",
                "text <!--\nnote\n-->",
                "<!--",
                "-->",
                "",
                "text",
                "# H",
                "## Sub",
                "- item",
                "1. one",
                "```",
                "***",
                "+++",
            ]),
            0..14,
        ),
    )
        .prop_map(|(front_matter, lines)| {
            let body = lines.join("\n");
            if front_matter {
                format!("---\ntitle: T\n---\n{body}")
            } else {
                body
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]

    #[test]
    fn check_accepts_formatted_output(input in document()) {
        let owned = formatter_owned_rules();
        let formatted = formatter::format(&input);
        let violations: Vec<_> = engine()
            .lint_content(&formatted)
            .unwrap()
            .into_iter()
            .filter(|v| owned.contains(&v.rule))
            .collect();
        prop_assert!(
            violations.is_empty(),
            "check flags format's output.\nInput: {:?}\nFormatted: {:?}\n{:?}",
            input, formatted, violations
        );
    }

    #[test]
    fn check_fixes_lead_format_to_the_same_output(input in document()) {
        let fixed = engine().fix_content(&input).unwrap();
        prop_assert_eq!(
            formatter::format(&fixed),
            formatter::format(&input),
            "check --fix moved the document away from format's output.\nInput: {:?}\nFixed: {:?}",
            input, fixed
        );
    }
}
