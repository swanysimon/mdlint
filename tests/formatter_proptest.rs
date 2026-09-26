use mdlint::config::{Config, RuleConfig};
use mdlint::formatter;
use mdlint::formatter::{FormatOptions, format_with};
use mdlint::lint::LintEngine;
use proptest::prelude::*;
use std::collections::HashMap;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    /// The formatter must never panic on any input string.
    #[test]
    fn formatter_never_panics(s in ".*") {
        let _ = formatter::format(&s);
    }

    /// Formatting is idempotent: format(format(x)) == format(x).
    #[test]
    fn formatter_is_idempotent(s in ".*") {
        let once = formatter::format(&s);
        let twice = formatter::format(&once);
        prop_assert_eq!(once, twice, "formatter not idempotent on input: {:?}", s);
    }

    /// The formatted output must end with exactly one newline (or be empty).
    #[test]
    fn formatter_trailing_newline(s in ".+") {
        let out = formatter::format(&s);
        if !out.is_empty() {
            prop_assert!(out.ends_with('\n'), "output should end with newline: {out:?}");
            prop_assert!(
                !out.ends_with("\n\n"),
                "output should not have double trailing newline: {out:?}"
            );
        }
    }

    /// Lines in formatted output must not have trailing whitespace.
    #[test]
    fn formatter_no_trailing_whitespace(s in ".*") {
        let out = formatter::format(&s);
        for line in out.lines() {
            prop_assert!(
                !line.ends_with(' ') && !line.ends_with('\t'),
                "line has trailing whitespace: {line:?}"
            );
        }
    }
}

/// A word that is either ordinary text or one of the characters that changes
/// meaning when reflow joins two previously separate lines, or when it lands at
/// the start of a line. `on_text` escapes `\`, backtick, `<`, `_` and `~`, but
/// not `*` or `[`/`]`, so those adjacencies are only reachable this way.
fn hazard_word() -> impl Strategy<Value = String> {
    prop_oneof![
        6 => "[a-z]{1,8}",
        1 => prop::sample::select(vec![
            "*", "**", "[", "]", "_", "`", "<", "|", "#", "-", "+", ">",
            "1.", "---", "===", "~~", "!", "\\", "<div>", "](x)", "```",
        ])
        .prop_map(str::to_owned),
    ]
}

/// A word whose length varies widely, including well past any width this
/// module tests with -- the only way to exercise the "a token longer than the
/// fill column is never broken" exception, since a word that can never exceed
/// the width never forces the formatter down that path. Mixes in Unicode
/// (multi-byte, char-counted) text so the width comparison itself -- `chars().count()`,
/// not byte length -- is exercised too.
fn variable_length_word() -> impl Strategy<Value = String> {
    prop_oneof![
        6 => "[a-z]{1,8}",
        2 => "[a-z]{1,60}",
        1 => "[α-ωа-я]{1,40}",
    ]
    .prop_map(String::from)
}

/// A multi-line paragraph, possibly containing block structure. Random line
/// breaks in the source are what reflow has to discard, and `.*` almost never
/// generates them next to hazard characters.
///
/// A hazard may open a source line, which turns that part of the input into a
/// list, heading, blockquote or HTML block rather than plain prose. That
/// exercises container handling as well as reflow, which is where this generator
/// has found most of its bugs. Hazards also land at the start of *output* lines,
/// because the wrapper is free to break in front of one. `>` is the hazard that
/// nests a *different kind* of container inside a list and back again, which is
/// what exercises every block opener's container prefix.
fn paragraph(word: impl Strategy<Value = String>) -> impl Strategy<Value = String> {
    prop::collection::vec((word, prop::bool::ANY), 20..60).prop_map(|parts| {
        let mut text = String::new();
        for (index, (word, newline)) in parts.iter().enumerate() {
            if index > 0 {
                text.push(if *newline { '\n' } else { ' ' });
            }
            text.push_str(word);
        }
        text.push('\n');
        text
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    /// Reflow must not break idempotency at any fill column.
    #[test]
    fn reflow_is_idempotent(
        text in paragraph(hazard_word()),
        width in 8usize..80,
    ) {
        let options = FormatOptions {
        width,
        ..Default::default()
    };
        let once = format_with(&text, &options);
        let twice = format_with(&once, &options);
        prop_assert_eq!(
            &once,
            &twice,
            "not idempotent at width {} on input: {:?}",
            width,
            text
        );
    }

    /// Every line must fit the fill column, *unless* it is a single token the
    /// formatter has nowhere to break -- in which case it may run over, but
    /// only because it truly has no breakable space. `variable_length_word`
    /// includes tokens well past the widths tested here, so this actually
    /// exercises the exception instead of it holding vacuously.
    #[test]
    fn reflow_respects_the_fill_column(
        text in paragraph(variable_length_word()),
        width in 4usize..80,
    ) {
        let options = FormatOptions {
        width,
        ..Default::default()
    };
        let out = format_with(&text, &options);
        for line in out.lines() {
            let len = line.chars().count();
            if len > width {
                prop_assert!(
                    !line.trim().contains(' '),
                    "line of {len} chars exceeds width {width} but still has a breakable space: {line:?}"
                );
            }
        }
    }

    /// `mdlint format` must never leave behind an over-width line that
    /// `mdlint check`'s MD013 then flags -- the documented guarantee that the
    /// format/check cycle can't go permanently red on the formatter's own
    /// output. Headings, table rows and code blocks are excluded: the formatter
    /// never wraps them, so those checks are a deliberate, separate exception
    /// (see README).
    ///
    /// This includes lines that need a block-hazard escape (`1.`, `#`, `---`,
    /// ...): `wrap_segment` reserves a column for the backslash whenever a
    /// line's first token would need one, so a greedy fill can't land the
    /// escaped line one character past the budget.
    #[test]
    fn formatted_output_never_flags_md013(
        text in paragraph(hazard_word()),
        width in 8usize..80,
    ) {
        let options = FormatOptions {
        width,
        ..Default::default()
    };
        let out = format_with(&text, &options);

        let mut params = HashMap::new();
        params.insert(
            "line_length".to_owned(),
            toml::Value::Integer(i64::try_from(width).unwrap()),
        );
        params.insert("heading_line_length".to_owned(), toml::Value::Integer(1_000_000));
        params.insert("tables".to_owned(), toml::Value::Boolean(false));
        params.insert("code_blocks".to_owned(), toml::Value::Boolean(false));
        let mut rules = HashMap::new();
        rules.insert("MD013".to_owned(), RuleConfig::Config(params));
        let config = Config {
            default_enabled: false,
            rules,
            ..Config::default()
        };

        let violations = LintEngine::new(config).lint_content(&out).unwrap();
        prop_assert!(
            violations.iter().all(|v| v.rule != "MD013"),
            "format() at width {width} left an MD013 violation `check` cannot fix: {violations:?}\noutput:\n{out}"
        );
    }
}
