# Reflow review TODO

Third pass, 2026-09-20, after a second review found three real gaps in the first fix pass. All three are now fixed and
verified. Baseline: branch `docs/reflow-plan`, HEAD after nine fix commits (`23499d8`…`cffd0f2`), rebased onto
`origin/main` (`c20827f`). Full `cargo test` suite (incl. `PROPTEST_CASES=10000`) and `prek run -a` (12 hooks, incl.
dogfood) pass.

## Done — verified across both review passes

- [x] **Character loss from stale break offsets.** Both `break_offsets` and `block_reflow_suppressed` reset at the start
  of every new block and on the whitespace-only-paragraph skip path.
- [x] **MD013 exemption aligned with real break opportunities.** `is_unbreakable` is event-driven, sharing the
  formatter's own parser config; code spans, inline HTML, link titles, and raw HTML blocks are all correctly exempt.
- [x] **Reflow suppression reset between blocks.** A suppressed code fence, table, or HTML block no longer leaks
  `width = usize::MAX` into the next paragraph.
- [x] **Directive examples in fenced and indented code ignored.** See below — the indented-code detection was rewritten
  again to fix a regression the second review found.
- [x] **LSP no longer silently formats with defaults on a config error.**
- [x] **Phase 6 property coverage**, including a `formatted_output_never_flags_md013` property that runs the real lint
  engine over formatter output — this is what caught two of the three gaps below.
- [x] **Configured reflow through both public entry points** (CLI and LSP).
- [x] **CLI/LSP configuration parity decided** and documented as an intentional divergence.
- [x] **Documentation reconciled** across AIDEV.md, CLAUDE.md, FORMAT_SPEC.md, and all three READMEs.
- [x] **Rebased onto origin/main.**
- [x] **Loose list items losing a second block's blank line and indent** (`505e1c2`). `on_start(Tag::Paragraph)` skipped
  both the blank separator and the item's continuation indent whenever `list_depth > 0`, correct only for an item's
  *first* block. A second (or later) paragraph in a loose item fell to column 0 and merged into the first block as a
  lazy continuation on the next pass — non-idempotent and meaning-destroying, e.g. `- first para\n\n  second para\n`
  collapsed to `- first para second para\n`. Confirmed pre-existing on `origin/main` (`c20827f`), predating the reflow
  work; missed by every existing fixture, unit test, and proptest generator because none produced a multi-block list
  item. Fixed using the same `in_tight_item` signal `Tag::CodeBlock` already used for this. Regression test added; every
  existing exact-output formatter test still passes unchanged.
- [x] **Escaped lines landing one character over budget** (`78d963e`). `wrap_segment` picked a line's word boundary by
  its *unescaped* length, so a line starting with a block hazard (`1.`, `#`, `---`, ...) that `retreat_past_structure`
  had no earlier break to avoid grew one character past `width` once `escape_line` added its backslash — permanently
  MD013-red, since the content has spaces and so isn't exempt. Fixed properly, not just tolerated: `wrap_segment` now
  reserves one column whenever a candidate line's first token would need escaping (decidable in isolation, since
  CommonMark block-start sniffing is prefix-only). Verified by tightening `formatted_output_never_flags_md013`'s filter
  from "over by more than one" back to "over by any amount" — 10,000 cases clean, and every existing exact-output test
  still passes, so the reservation isn't visibly shifting wrap points anywhere it wasn't already needed.
- [x] **Directives at a nested list item's content column misread as indented-code examples** (`cffd0f2`). The
  line-based indented-code heuristic (`non-blank, indented >= 4, previous line blank`) had no container awareness: a
  real directive at a nested list item's own content column (4 spaces, two dash-levels deep) is ordinary paragraph text
  for that item, not code, but the heuristic couldn't tell the difference and silently dropped the directive's
  protection. Replaced the whole hand-rolled fence/indent scan with `MarkdownParser::get_code_block_line_numbers()`,
  which already derives this correctly from real parse events (used elsewhere in the codebase for the same purpose).
  Fixes this while keeping every prior repro working (fenced/indented examples stay inert, lazy continuations still
  apply).

## Still open

- [ ] **Container prefixes on block openers, in the blockquote-mixed-nesting case.**
  `cargo test --lib test_blockquote_nested_in_list_opens_with_the_full_prefix -- --ignored` still fails
  (`-\n>   - alpha\n` vs `-\n\n> - alpha\n`). This is narrower than it first looked: the *plain-list* case (a paragraph,
  code fence, etc. as a later block in a list item with no blockquote involved) is fixed above. What remains is
  specifically a blockquote nested inside a list item, where a later block only gets `write_bq_prefix` (the blockquote
  markers) and not the full `container_prefix` (which would also carry the enclosing list's indent). Root cause is a
  genuine ambiguity between "this marker starts a fresh line" and "this marker continues an already-open line" that
  touches item markers, fences, tables, headings, and rules — a wider, riskier change than the fixes above. When fixing:
  audit every opener site, unignore the test, and restore `>` to the proptest `hazard_word` corpus (currently withheld
  for exactly this reason).
- [ ] **Confirm the minor-release rollout before publishing.** Procedural, unchanged: version still `0.3.24`; the
  first-run rewrite warning is in all three READMEs (that is the release note; no CHANGELOG in the repo); the release
  must be **minor** (`0.4.0`) — the README's documented `cargo release patch --execute` default would be wrong for a
  change that rewrites every consumer's files.

## Verification log

- All repros from both prior review passes re-run clean against the current HEAD binary.
- `cargo test` (all suites incl. proptests at `PROPTEST_CASES=10000`, lsp, migrate) exit 0; `prek run -a` exit 0.
- The ignored block-opener test still fails identically to before — confirmed unaffected by the paragraph fix, so the
  remaining gap is accurately scoped to the blockquote-mixed case only.
