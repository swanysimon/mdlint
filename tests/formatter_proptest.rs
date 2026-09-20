use mdlint::formatter;
use mdlint::formatter::{FormatOptions, format_with};
use proptest::prelude::*;

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
            "*", "**", "[", "]", "_", "`", "<", "|", "#", "-", "+",
            "1.", "---", "===", "~~", "!", "\\", "<div>", "](x)",
        ])
        .prop_map(str::to_owned),
    ]
}

/// A multi-line paragraph, possibly containing block structure. Random line
/// breaks in the source are what reflow has to discard, and `.*` almost never
/// generates them next to hazard characters.
///
/// A hazard may open a source line, which turns that part of the input into a
/// list, heading, blockquote or HTML block rather than plain prose. That
/// exercises container handling as well as reflow, which is where this generator
/// has found most of its bugs. Hazards also land at the start of *output* lines,
/// because the wrapper is free to break in front of one.
///
/// `>` is held out of the corpus. It is the one hazard that can nest a
/// *different kind* of container inside a list (or a list inside itself), and
/// block openers still write only the blockquote marker rather than the whole
/// enclosing prefix -- see the ignored
/// `test_blockquote_nested_in_list_opens_with_the_full_prefix`. Blockquote
/// behaviour is covered by the unit tests instead.
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

    /// With no token longer than the budget and nothing that needs escaping,
    /// every line must fit the fill column.
    #[test]
    fn reflow_respects_the_fill_column(
        text in paragraph("[a-z]{1,8}".prop_map(String::from)),
        width in 12usize..80,
    ) {
        let options = FormatOptions {
        width,
        ..Default::default()
    };
        let out = format_with(&text, &options);
        for line in out.lines() {
            prop_assert!(
                line.chars().count() <= width,
                "line of {} chars exceeds width {}: {:?}",
                line.chars().count(),
                width,
                line
            );
        }
    }
}
