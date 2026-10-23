use mdlint::formatter;
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

    /// Idempotency over documents built from block-level lines, with HTML
    /// comments and front matter mixed in. `.*` never produces a newline, so the
    /// tests above cannot reach block spacing or front matter.
    #[test]
    fn formatter_is_idempotent_on_block_lines(
        front_matter in proptest::bool::ANY,
        lines in proptest::collection::vec(
            // Tilde fences, blockquotes, tables, setext underlines, link reference
            // definitions, indented code, and indented continuations are left out:
            // they reach pre-existing idempotency bugs unrelated to comments and
            // front matter.
            proptest::sample::select(vec![
                "<!-- c -->", "<!-- mdlint-disable-next-line MD025 -->", "<!--", "-->", "",
                "text", "# H", "## Sub", "- item", "* star", "1. one", "```", "---",
                "***", "+++", "<div>", "</div>",
            ]),
            0..14,
        ),
    ) {
        let body = lines.join("\n");
        let input = if front_matter { format!("---\ntitle: T\n---\n{body}") } else { body };
        let once = formatter::format(&input);
        let twice = formatter::format(&once);
        prop_assert_eq!(once, twice, "formatter not idempotent on input: {:?}", input);
    }
}
