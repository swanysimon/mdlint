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
fn list_item_continuation_indented() {
    // A soft-wrapped list item is reflowed: the source's manual line break is
    // just a soft break, so it gets rejoined onto one line (well under the
    // default reflow width).
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
fn list_item_continuation_wraps_when_over_line_length() {
    // A soft-wrapped list item whose joined text exceeds the default line
    // length is rewrapped, keeping the continuation indented so the linter
    // does not mistake it for a paragraph outside the list.
    let long_word = "x".repeat(115);
    let input = format!("- First line\n  {long_word}\n");
    let expected = format!("- First line\n  {long_word}\n");
    assert_formats_to(&input, &expected);
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

#[test]
fn format_reflows_soft_wrapped_paragraph_by_default() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("doc.md");
    fs::write(
        &file,
        indoc! {"
            First line
            continuation here.
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
    assert_eq!(result, "First line continuation here.\n");
}

#[test]
fn format_no_reflow_flag_preserves_source_line_breaks() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("doc.md");
    let original = indoc! {"
        First line
        continuation here.
    "};
    fs::write(&file, original).unwrap();

    let status = Command::new(mdlint_bin())
        .args(["format", "--no-reflow", file.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(status.success());
    let result = fs::read_to_string(&file).unwrap();
    assert_eq!(
        result, original,
        "--no-reflow should leave the source's line breaks untouched"
    );
}

#[test]
fn format_config_reflow_false_preserves_source_line_breaks() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("mdlint.toml");
    fs::write(&config, "reflow = false\n").unwrap();
    let file = dir.path().join("doc.md");
    let original = indoc! {"
        First line
        continuation here.
    "};
    fs::write(&file, original).unwrap();

    let status = Command::new(mdlint_bin())
        .args([
            "format",
            "--config",
            config.to_str().unwrap(),
            file.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(status.success());
    let result = fs::read_to_string(&file).unwrap();
    assert_eq!(
        result, original,
        "reflow = false in config should leave the source's line breaks untouched"
    );
}

#[test]
fn format_cli_flag_overrides_config_reflow_true() {
    // --no-reflow always wins even when the config file leaves reflow enabled.
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("mdlint.toml");
    fs::write(&config, "reflow = true\n").unwrap();
    let file = dir.path().join("doc.md");
    let original = indoc! {"
        First line
        continuation here.
    "};
    fs::write(&file, original).unwrap();

    let status = Command::new(mdlint_bin())
        .args([
            "format",
            "--no-reflow",
            "--config",
            config.to_str().unwrap(),
            file.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(status.success());
    let result = fs::read_to_string(&file).unwrap();
    assert_eq!(result, original);
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
