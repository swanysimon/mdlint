# AIDEV.md — Development Task Checklist

Tasks are phrased as prompts you might give an AI coding assistant. Keep this file up to date as
work progresses. See CONTRIBUTING.md for development setup.

---

## Completed Foundation

- [x] Initialize Cargo project with optimized release profile (LTO, strip, opt-level=3)
- [x] Implement TOML-based configuration system with hierarchical directory discovery
- [x] Implement file discovery using the `ignore` crate (gitignore-aware)
- [x] Implement markdown parsing wrapper around pulldown-cmark with position tracking
- [x] Implement linting rule framework: `Rule` trait, registry, and `Violation` type
- [x] Implement auto-fix framework (`Fix` type, `fixer.rs`)
- [x] Implement default and JSON output formatters
- [x] Implement CLI with `check` and `format` subcommands using clap
- [x] Set up GitHub Actions CI: test, clippy, fmt, dogfooding, and multi-platform build jobs
- [x] Set up multi-platform binary builds (Linux x86/ARM, macOS x86/ARM, Windows) and Docker images
- [x] Port the majority of the 54 markdownlint rules (see `src/lint/rules/`)
- [x] Write user and developer documentation (README, CONTRIBUTING)

---

## Priority 1: Pivot and Positioning

These tasks update framing to reflect that mdlint is an **opinionated formatter first, linter second** —
analogous to ruff or gofmt, not markdownlint-cli2.

- [x] Update README.md to lead with "opinionated Markdown formatter" messaging. Remove the "Differences
  from markdownlint-cli2" section. Reframe the rules table to emphasize that most violations are
  auto-fixable via `mdlint format`. Keep existing structure but make the formatter the hero.

- [x] Write FORMAT_SPEC.md that specifies exactly what canonical mdlint-formatted markdown looks like.
  Document every opinionated choice: ATX headings only, dash list markers, backtick code fences,
  asterisk emphasis, trailing newline, blank lines around block elements, etc. This spec is the north
  star for all formatter implementation work.

- [x] Delete IMPROVEMENTS.md — its content has been moved into AIDEV.md. Remove any references to it
  from other documentation.

- [x] Update CONTRIBUTING.md to include a section on adding formatter rules (not just linting rules).
  Explain the distinction: a formatting rule is enforced by `mdlint format`; a linting rule is
  reported by `mdlint check`. Many rules should be both.

---

## Priority 2: Formatter Core

The formatter is the centerpiece of mdlint. It reads markdown, parses it to an AST, and emits canonical
text. This is what makes mdlint a formatter rather than a linter with `--fix`.

- [x] Design the formatter architecture. The architectural decision is documented in FORMAT_SPEC.md:
  emit canonical text directly from pulldown-cmark events, with a small state machine to track the
  previous block element and insert blank lines before each new block. Key constraint: idempotency
  is a hard requirement — formatting an already-formatted file must produce no changes.

- [x] Implement `src/formatter/mod.rs` — a formatter that takes `&str` and returns a `String` in
  canonical form. Wire it into the existing `format` CLI command, which currently is a placeholder.

- [x] Implement heading canonicalization in the formatter: always emit ATX style (`# Heading`), never
  setext style (`Heading\n===`). MD003-equivalent.

- [x] Implement list marker canonicalization: always use dashes (`- item`), never asterisks or plus
  signs. MD004-equivalent.

- [x] Implement code fence canonicalization: always use backticks (```` ``` ````), never tildes (`~~~`).
  MD048-equivalent.

- [x] Implement emphasis marker canonicalization: always use asterisks (`*text*`, `**text**`), never
  underscores. MD049/MD050-equivalent.

- [x] Implement blank line normalization: exactly one blank line before and after block elements
  (headings, code blocks, lists, blockquotes); no multiple consecutive blank lines. MD012/MD022/MD031/
  MD032-equivalent.

- [x] Implement trailing whitespace removal: strip trailing spaces and tabs from every line.
  MD009-equivalent.

- [x] Implement trailing newline normalization: exactly one newline at end of file. MD047-equivalent.

- [x] Write tests verifying formatter idempotency: format a file, format it again, assert outputs are
  identical. Then add a property-based test using proptest that generates random CommonMark input and
  verifies idempotency.

- [x] Implement `mdlint format --check`: read each file, format in memory, exit 1 if any file would
  change. No files written. This is the CI-friendly verification mode.

- [x] Implement ordered list renumbering in the formatter: when a list uses sequential numbering
  (1. 2. 3.), canonicalise it so the numbers are correct and contiguous. Once implemented, mark
  MD029 as `fixable: true` and change its default style from `"one"` to `"ordered"`. The motivation
  is AI-agent readability: rendered Markdown renumbers automatically so `"one"` style is fine for
  human readers, but agents consume raw source where `1. / 1. / 1.` carries no positional information.
  Sequential numbers are semantically richer in raw form.

---

## Priority 3: Linting Rules

The `check` command is the secondary workflow. Rules report violations; violations enforceable by the
formatter should also be marked fixable.

- [ ] Audit all existing rule implementations against the markdownlint reference. For each of the 54
  rules, verify: (1) the rule file exists in `src/lint/rules/`, (2) it is registered in
  `create_default_registry()`, (3) it has at least one passing test. Fix any gaps found.

- [x] Mark all formatter-enforceable rules as fixable (return `true` from `fixable()`) and ensure their
  fix logic is consistent with what the formatter does — no divergence between `--fix` and
  `mdlint format`.

- [x] Fix the task list checkbox detection bug: `[ ]` in link position is being detected as a link.
  It should be recognized as a GFM task list checkbox and excluded from link-related rules (MD011, etc.).

- [x] Fix code block exclusion bugs: MD003, MD004, MD018, MD019, MD023, MD032, MD035 do not
  correctly skip content inside fenced code blocks. FORMAT_SPEC.md surfaces all of these —
  running `mdlint check FORMAT_SPEC.md` is a good regression test. MD003 uses raw line scanning
  with no code block check at all. The fix pattern is already established in MD004 (uses AST
  events to build `code_block_lines`) but MD004's detection also has a bug (detecting asterisk
  markers inside code examples as the "first" marker). Also: the auto-fixer applies MD018/MD019
  fixes inside fenced code blocks — the fixer must also check whether a line is inside a code
  block before applying a fix.

- [x] Implement inline configuration comments: parse `<!-- mdlint-disable MD001 -->`,
  `<!-- mdlint-enable MD001 -->`, and `<!-- mdlint-disable-next-line MD001 -->` HTML comments
  during the check pass to suppress violations on specific lines.

- [x] Implement any remaining rules from the 54-rule set that are missing: review `src/lint/rules/mod.rs`
  to confirm registration count against the full markdownlint rule list. Confirmed: MD057 is a deliberate
  gap in the official spec (like MD002, MD008). All other rules have implementations and tests.

---

## Priority 4: Testing

- [x] Write integration tests for the full check workflow: use test fixtures in `tests/fixtures/` to run
  discovery → lint → format output and compare against golden output files.

- [x] Write integration tests for the full format workflow: for each fixture, run the formatter and
  assert output matches a golden file. Assert that formatting the golden file again is a no-op
  (idempotency check).

- [x] Add property-based tests for the formatter using proptest: generate random strings and verify the
  formatter (1) never panics, (2) is idempotent, (3) produces output that parses as valid CommonMark.
  Proptest found two real bugs fixed in the process: empty list items had trailing whitespace; code
  block content without a trailing newline merged with the closing fence; hard breaks using
  trailing-space newline syntax were stripped by trailing-whitespace normalisation (fixed to use
  `\\\n`); backslash characters in
  text were not re-escaped (fixed in `on_text`).

- [x] Add regression tests for all known bugs as they are discovered and fixed. Each bug gets a fixture
  and a test.

- [ ] Consider running compatibility tests against reference markdown files from real open-source projects
  to verify the formatter produces consistent, expected output.

---

## Priority 5: CLI Polish

- [x] Add color support to check output. Colors already implemented via ANSI codes; added `NO_COLOR`
  env var respect (https://no-color.org/). `--color auto/always/never` flag already existed.

- [x] Add summary statistics to check output: "Found X error(s) in Y file(s) (Z checked)" shown at end
  of every run, including the total files checked (not just files with violations).

- [x] Add a `--verbose` flag that prints the name of each file as it is processed (`--verbose` already
  existed in args; wired it up to print the path to stderr before each file).

- [x] Improve error messages with structured context: show the offending source line and a caret
  indicator under the column position beneath each violation.

---

## Priority 6: Distribution

- [x] Publish first release to crates.io by setting the `CARGO_REGISTRY_TOKEN` GitHub Actions secret
  and running `cargo release patch --execute`.

- [ ] Write a Homebrew formula in a tap repository so macOS and Linux users can install with
  `brew install`.

- [ ] Add pre-commit hook configuration examples to the documentation so teams can run
  `mdlint format --check` as a git pre-commit hook.

- [ ] Consider an npm wrapper package so Node.js projects can add mdlint as a devDependency and use it
  without a separate Rust toolchain installation.
