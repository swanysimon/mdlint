use indoc::indoc;
use mdlint::formatter;
use std::fs;
use std::process::{Command, Stdio};
use tempfile::TempDir;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Assert that formatting `input` produces `expected`, and that the expected
/// output is already idempotent (format(expected) == expected).
fn assert_formats_to(input: &str, expected: &str) {
    let got = formatter::format(input);
    assert_eq!(
        got, expected,
        "format(input) did not match expected.\nInput:\n{input}\nExpected:\n{expected}\nGot:\n{got}"
    );
    let twice = formatter::format(expected);
    assert_eq!(
        twice, expected,
        "format(expected) != expected — expected output is not idempotent.\nExpected:\n{expected}\nTwice:\n{twice}"
    );
}

/// Same contract as `assert_formats_to`, at an explicit fill column so that
/// wrapping cases stay short enough to verify by eye.
fn assert_formats_at_width(input: &str, expected: &str, width: usize) {
    let options = formatter::FormatOptions {
        width,
        ..Default::default()
    };
    let got = formatter::format_with(input, &options);
    assert_eq!(
        got, expected,
        "format_with(input, {width}) did not match expected.\nInput:\n{input}\nExpected:\n{expected}\nGot:\n{got}"
    );
    let twice = formatter::format_with(expected, &options);
    assert_eq!(
        twice, expected,
        "format_with(expected, {width}) != expected — not idempotent.\nExpected:\n{expected}\nTwice:\n{twice}"
    );
}

fn mdlint_bin() -> std::path::PathBuf {
    // Use the debug build so tests don't need a release build.
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // remove test binary name
    if p.ends_with("deps") {
        p.pop();
    }
    p.push("mdlint");
    p
}

// ── canonicalization ─────────────────────────────────────────────────────────

#[test]
fn setext_headings_become_atx() {
    assert_formats_to(
        indoc! {"
            Title
            =====

            Section
            -------
        "},
        indoc! {"
            # Title

            ## Section
        "},
    );
}

#[test]
fn closed_atx_headings_stripped() {
    assert_formats_to(
        indoc! {"
            ## Heading ##

            ### Sub ###
        "},
        indoc! {"
            ## Heading

            ### Sub
        "},
    );
}

#[test]
fn extra_spaces_after_hash_collapsed() {
    assert_formats_to(
        indoc! {"
            #  Too many

            ###   Lots
        "},
        indoc! {"
            # Too many

            ### Lots
        "},
    );
}

#[test]
fn asterisk_and_plus_list_markers_become_dash() {
    // Two lists with different markers both normalise to `-`.  An invisible
    // HTML comment separator is inserted so they don't merge into a single
    // list (and become loose) on the second format pass.
    assert_formats_to(
        indoc! {"
            * Alpha
            * Beta

            + Gamma
            + Delta
        "},
        indoc! {"
            - Alpha
            - Beta

            <!---->

            - Gamma
            - Delta
        "},
    );
}

#[test]
fn tilde_code_fences_become_backtick() {
    assert_formats_to(
        indoc! {"
            ~~~rust
            fn main() {}
            ~~~
        "},
        indoc! {"
            ```rust
            fn main() {}
            ```
        "},
    );
    assert_formats_to(
        indoc! {"
            ~~~
            plain
            ~~~
        "},
        indoc! {"
            ```
            plain
            ```
        "},
    );
}

#[test]
fn underscore_emphasis_becomes_asterisk() {
    assert_formats_to("_italic_ and __bold__\n", "*italic* and **bold**\n");
}

#[test]
fn horizontal_rules_normalised_to_dashes() {
    assert_formats_to("***\n", "---\n");
    assert_formats_to("___\n", "---\n");
    assert_formats_to("* * *\n", "---\n");
    assert_formats_to("- - -\n", "---\n");
}

#[test]
fn multiple_blank_lines_collapsed() {
    assert_formats_to(
        indoc! {"
            First.



            Second.
        "},
        indoc! {"
            First.

            Second.
        "},
    );
}

#[test]
fn trailing_whitespace_removed() {
    // Lines with trailing spaces get stripped
    let input = "Text with trailing spaces.   \n\nMore text.  \n";
    let out = formatter::format(input);
    for line in out.lines() {
        assert_eq!(
            line,
            line.trim_end(),
            "line has trailing whitespace: {line:?}"
        );
    }
}

#[test]
fn trailing_newline_normalised() {
    assert!(formatter::format("text").ends_with('\n'));
    assert!(
        formatter::format(indoc! {"
            text


        "})
        .ends_with('\n')
    );
    assert_eq!(
        formatter::format(indoc! {"
            text


        "})
        .matches('\n')
        .count(),
        1
    );
}

#[test]
fn empty_input_produces_empty_output() {
    assert_eq!(formatter::format(""), "");
    assert_eq!(formatter::format("   \n\n  "), "");
}

// ── structure preservation ────────────────────────────────────────────────────

#[test]
fn nested_lists_preserved() {
    assert_formats_to(
        indoc! {"
            - Top
              - Nested
                - Deep
            - Back
        "},
        indoc! {"
            - Top
              - Nested
                - Deep
            - Back
        "},
    );
}

#[test]
fn nested_ordered_list_under_bullet_item_stays_tight() {
    // Regression test for issue #67: a nested ordered list under a bullet item
    // is formatted tight (no blank lines separating it from the parent item's
    // text or the next sibling item), whether or not the source had blank
    // lines around it. This canonical form must pass `mdlint check` (MD032)
    // cleanly — see the matching MD032 tests in src/lint/rules/md032.rs.
    assert_formats_to(
        indoc! {"
            # Example

            - First item:

              1. One
              2. Two

            - Second item
        "},
        indoc! {"
            # Example

            - First item:
              1. One
              2. Two
            - Second item
        "},
    );
}

#[test]
fn ordered_list_preserved() {
    assert_formats_to(
        indoc! {"
            1. First
            2. Second
            3. Third
        "},
        indoc! {"
            1. First
            2. Second
            3. Third
        "},
    );
}

#[test]
fn code_block_content_preserved_verbatim() {
    // Tabs and unusual indentation inside code blocks must survive unchanged.
    let input = indoc! {"
        ```
        \tindented with tab
            four spaces
        ```
    "};
    assert_formats_to(input, input);
}

#[test]
fn inline_code_content_preserved() {
    assert_formats_to(
        "Use `_underscores_` and `* asterisks` in code spans.\n",
        "Use `_underscores_` and `* asterisks` in code spans.\n",
    );
}

#[test]
fn link_and_image_preserved() {
    assert_formats_to(
        "[link](https://example.com) and ![img](pic.png)\n",
        "[link](https://example.com) and ![img](pic.png)\n",
    );
}

#[test]
fn blockquote_preserved() {
    assert_formats_to(
        indoc! {"
            > quoted
            >
            > second para
        "},
        indoc! {"
            > quoted
            >
            > second para
        "},
    );
}

#[test]
fn gfm_table_canonicalised() {
    // Input without leading/trailing pipes → output with them
    assert_formats_to(
        indoc! {"
            A | B
            --- | ---
            1 | 2
        "},
        indoc! {"
            | A | B |
            | --- | --- |
            | 1 | 2 |
        "},
    );
}

#[test]
fn gfm_table_already_canonical_unchanged() {
    let canonical = indoc! {"
        | A | B |
        | --- | --- |
        | 1 | 2 |
    "};
    assert_formats_to(canonical, canonical);
}

#[test]
fn list_item_soft_wrap_is_rejoined() {
    // The source break carries no meaning and the joined line fits the fill
    // column, so reflow discards it.
    assert_formats_to(
        indoc! {"
            - First line
              continuation here
        "},
        indoc! {"
            - First line continuation here
        "},
    );
}

#[test]
fn list_item_continuation_indented_to_content_column() {
    // A wrapped list item keeps its continuation at the item's content column,
    // so the linter does not mistake it for a paragraph outside the list. That
    // column is the marker width: two for `- `, three for `1. `.
    assert_formats_at_width(
        "- alpha bravo charlie delta\n",
        "- alpha bravo\n  charlie delta\n",
        16,
    );
    assert_formats_at_width(
        "1. alpha bravo charlie delta\n",
        "1. alpha bravo\n   charlie delta\n",
        16,
    );
}

// ── paragraph reflow ─────────────────────────────────────────────────────────

#[test]
fn paragraph_is_unwrapped_and_refilled() {
    assert_formats_at_width(
        "alpha bravo\ncharlie delta echo\nfoxtrot\n",
        "alpha bravo charlie\ndelta echo foxtrot\n",
        20,
    );
}

#[test]
fn reflow_never_breaks_inside_a_link_destination() {
    // A break inside `](...)` would destroy the link, so the whole destination
    // is atomic even though it pushes the line over the fill column.
    assert_formats_at_width(
        "alpha [text](https://example.com/a/long/path) bravo\n",
        "alpha\n[text](https://example.com/a/long/path)\nbravo\n",
        12,
    );
}

#[test]
fn reflow_never_breaks_inside_a_code_span() {
    assert_formats_at_width(
        "alpha `code with spaces` bravo\n",
        "alpha\n`code with spaces`\nbravo\n",
        12,
    );
}

#[test]
fn overlong_token_overflows_rather_than_splitting() {
    // Splitting would corrupt the URL; an over-width line is the lesser harm.
    assert_formats_at_width(
        "a https://example.com/very/long b\n",
        "a\nhttps://example.com/very/long\nb\n",
        10,
    );
}

#[test]
fn hard_breaks_survive_reflow() {
    // Each hard-break segment is refilled independently and the `\` is kept.
    assert_formats_at_width(
        "alpha bravo charlie\\\ndelta echo foxtrot\n",
        "alpha bravo\ncharlie\\\ndelta echo\nfoxtrot\n",
        14,
    );
}

#[test]
fn reflow_breaks_earlier_to_avoid_creating_a_list_item() {
    // Breaking after `bravobravo` would put `- delta` at column 0, where it
    // re-parses as a list. The wrapper retreats to the previous opportunity.
    assert_formats_at_width(
        "alpha bravobravo - delta echo\n",
        "alpha\nbravobravo -\ndelta echo\n",
        16,
    );
}

#[test]
fn reflow_escapes_when_no_earlier_break_exists() {
    // Nothing to retreat to, so the structural line is escaped instead.
    assert_formats_at_width(
        "alphaalphaalpha - delta\n",
        "alphaalphaalpha\n\\- delta\n",
        16,
    );
}

#[test]
fn runs_of_spaces_collapse() {
    // Required for idempotency: a break landing inside a run would strand a
    // space at a line edge for `finish` to trim, changing the text each pass.
    assert_formats_to("alpha    bravo\n", "alpha bravo\n");
}

#[test]
fn blockquote_reflow_keeps_the_marker_on_every_line() {
    assert_formats_at_width(
        "> alpha bravo charlie delta\n",
        "> alpha bravo\n> charlie delta\n",
        16,
    );
}

#[test]
fn headings_are_never_wrapped() {
    // ATX headings cannot span lines, so MD013 on a heading stays unfixable.
    assert_formats_at_width(
        "# alpha bravo charlie delta echo\n",
        "# alpha bravo charlie delta echo\n",
        16,
    );
}

// ── reflow honours inline directives ─────────────────────────────────────────
//
// Width 30 is chosen so the two outcomes differ: "alpha bravo charlie delta" is
// 25 characters, so an unprotected paragraph joins onto one line while a
// protected one keeps its two.

#[test]
fn disable_md013_protects_a_paragraph_from_reflow() {
    assert_formats_at_width(
        indoc! {"
            <!-- mdlint-disable MD013 -->

            alpha bravo
            charlie delta

            <!-- mdlint-enable MD013 -->

            echo foxtrot
            golf hotel
        "},
        indoc! {"
            <!-- mdlint-disable MD013 -->

            alpha bravo
            charlie delta

            <!-- mdlint-enable MD013 -->

            echo foxtrot golf hotel
        "},
        30,
    );
}

#[test]
fn disable_next_line_protects_the_following_paragraph() {
    // The formatter inserts a blank line after the comment, so the directive has
    // to reach past it or the protection would vanish on the second pass.
    assert_formats_at_width(
        indoc! {"
            <!-- mdlint-disable-next-line MD013 -->
            alpha bravo
            charlie delta
        "},
        indoc! {"
            <!-- mdlint-disable-next-line MD013 -->

            alpha bravo
            charlie delta
        "},
        30,
    );
}

#[test]
fn blanket_disable_also_protects_reflow() {
    assert_formats_at_width(
        indoc! {"
            <!-- mdlint-disable -->

            alpha bravo
            charlie delta
        "},
        indoc! {"
            <!-- mdlint-disable -->

            alpha bravo
            charlie delta
        "},
        30,
    );
}

#[test]
fn no_inline_config_reflows_a_protected_paragraph_anyway() {
    let input = indoc! {"
        <!-- mdlint-disable MD013 -->

        alpha bravo
        charlie delta
    "};
    let options = formatter::FormatOptions {
        width: 30,
        no_inline_config: true,
        ..Default::default()
    };
    assert_eq!(
        formatter::format_with(input, &options),
        indoc! {"
            <!-- mdlint-disable MD013 -->

            alpha bravo charlie delta
        "},
        "no_inline_config must ignore the directive and reflow"
    );
}

#[test]
fn disabling_md013_in_config_disables_reflow() {
    // The same switch as `<!-- mdlint-disable MD013 -->`, spelled in config.
    let input = "alpha bravo\ncharlie delta\n";
    let options = formatter::FormatOptions {
        width: 30,
        reflow: false,
        ..Default::default()
    };
    assert_eq!(formatter::format_with(input, &options), input);
    // ...but the rest of the canonical style is still applied.
    let options = formatter::FormatOptions {
        reflow: false,
        ..Default::default()
    };
    assert_eq!(
        formatter::format_with("Setext\n======\n", &options),
        "# Setext\n"
    );
}

// ── container prefixes on wrapped lines ──────────────────────────────────────

#[test]
fn list_inside_blockquote_continues_with_the_quote_outermost() {
    assert_formats_at_width(
        "> - alpha bravo charlie delta\n",
        "> - alpha bravo charlie\n>   delta\n",
        24,
    );
}

#[test]
fn blockquote_inside_list_continues_with_the_list_outermost() {
    // The mirror image of the case above: concatenating two fixed prefixes can
    // only ever get one of the two orders right.
    assert_formats_at_width(
        "- > echo foxtrot golf hotel\n",
        "- > echo foxtrot golf\n  > hotel\n",
        24,
    );
}

// ── idempotency on complex documents ─────────────────────────────────────────

#[test]
fn idempotent_on_mixed_document() {
    let input = indoc! {"
        # Title

        Intro paragraph.

        ## Section

        - Item one
        - Item two
          - Nested

        ```rust
        fn main() {}
        ```

        | Col A | Col B |
        | ----- | ----- |
        | val   | val   |

        > A blockquote

        Final paragraph.
    "};
    let once = formatter::format(input);
    let twice = formatter::format(&once);
    assert_eq!(once, twice, "formatter is not idempotent on mixed document");
}

// ── `mdlint format` CLI ──────────────────────────────────────────────────────

#[test]
fn format_check_does_not_modify_file() {
    // `format --check` must never write to disk even when changes are needed.
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("doc.md");
    let original = indoc! {"
        Heading
        =======

        * item
    "};
    fs::write(&file, original).unwrap();

    Command::new(mdlint_bin())
        .args(["format", "--check", file.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    let after = fs::read_to_string(&file).unwrap();
    assert_eq!(after, original, "format --check must not modify the file");
}

#[test]
fn format_check_exits_0_when_already_formatted() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("clean.md");
    fs::write(
        &file,
        indoc! {"
            # Heading

            Paragraph.
        "},
    )
    .unwrap();

    let status = Command::new(mdlint_bin())
        .args(["format", "--check", file.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(
        status.success(),
        "expected exit 0 for already-formatted file"
    );
}

#[test]
fn format_check_exits_1_when_file_needs_formatting() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("dirty.md");
    fs::write(
        &file,
        indoc! {"
            Heading
            =======

            Paragraph.
        "},
    )
    .unwrap();

    let status = Command::new(mdlint_bin())
        .args(["format", "--check", file.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert_eq!(
        status.code(),
        Some(1),
        "expected exit 1 when file needs formatting"
    );
}

#[test]
fn format_rewrites_file_in_place() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("doc.md");
    fs::write(
        &file,
        indoc! {"
            Heading
            =======

            * item
        "},
    )
    .unwrap();

    let status = Command::new(mdlint_bin())
        .args(["format", file.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(status.success());
    let result = fs::read_to_string(&file).unwrap();
    assert_eq!(
        result,
        indoc! {"
            # Heading

            - item
        "}
    );
}

// ── `mdlint check` CLI ───────────────────────────────────────────────────────

#[test]
fn check_without_fix_does_not_modify_file() {
    // `check` with `fix = false` must never write to disk.
    // We supply an explicit config because the default has `fix = true`.
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("mdlint.toml");
    fs::write(
        &config,
        indoc! {"
            default_enabled = true
            fix = false
        "},
    )
    .unwrap();
    let file = dir.path().join("doc.md");
    let content = "# Heading\n\nTrailing spaces.   \n";
    fs::write(&file, content).unwrap();

    Command::new(mdlint_bin())
        .args([
            "check",
            "--config",
            config.to_str().unwrap(),
            file.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    let after = fs::read_to_string(&file).unwrap();
    assert_eq!(
        after, content,
        "check with fix=false must not modify the file"
    );
}

#[test]
fn check_with_fix_corrects_violations_and_exits_1() {
    // `check --fix` applies inline fixes but still exits 1 because violations
    // were present (exit code reflects the pre-fix lint result).
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("doc.md");
    fs::write(&file, "# Heading\n\nTrailing spaces.   \n").unwrap();

    let status = Command::new(mdlint_bin())
        .args(["check", "--fix", file.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert_eq!(
        status.code(),
        Some(1),
        "check --fix should exit 1 when violations were found"
    );
    let after = fs::read_to_string(&file).unwrap();
    assert_eq!(
        after,
        indoc! {"
            # Heading

            Trailing spaces.
        "},
        "trailing spaces should be removed by --fix"
    );
}
