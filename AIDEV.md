# AIDEV.md — Development Task Checklist

Tasks are phrased as prompts you might give an AI coding assistant. Keep this file up to date as work progresses. See
CLAUDE.md for architecture and FORMAT_SPEC.md for canonical style decisions.

---

## Paragraph Reflow in `mdlint format`

Goal: `mdlint format` discards the soft line breaks inside a paragraph and refills greedily up to
`rules.MD013.line_length` (default 120).

Decisions already made:

- Reflow is **on by default** in a minor release. No opt-in flag, no formatter mode.
- The fill column is **read from MD013**, not a new config key, so "formatted but MD013-failing" is unrepresentable by
  construction.

Resolved during implementation: unwrap-then-refill, not pure unwrap. Delivered and on by default.

---

### Phase 0: Specification

- [x] Update FORMAT_SPEC.md before writing any code — it is the source of truth and currently states the opposite of
  this feature. Add a "Paragraph Reflow" section covering unwrap + refill, the width source, and the exemption list.
  Amend the "No configuration" principle to "one configuration input: the fill column". Remove the paragraph bullet from
  "What the Formatter Does NOT Change". Add MD013 to "Relationship to Linting Rules" as formatter-fixed but
  `check --fix`-unfixable.

---

### Phase 1: Break opportunities

`FormatterState.inline` is a flat `String` with escapes already baked in, so it cannot be wrapped by splitting on
whitespace — spaces occur inside constructs that must stay atomic:

| Construct | Emitted at | Why breaking there is unsafe |
| --- | --- | --- |
| `[text](dest "title")` | `on_end`, `TagEnd::Link` | Destroys the link |
| Inline code spans | `emit_inline_code` | Re-parse turns the newline into a space; the formatter re-joins |
| Inline HTML | `Event::InlineHtml` | Rewrites raw HTML that the spec passes through verbatim |
| Hard break | `Event::HardBreak` | The trailing backslash must stay line-final |

- [x] Add a `break_offsets: Vec<usize>` field to `FormatterState` recording byte positions in `inline` where a break is
  permitted. Push to it in exactly two places. In `on_text`, record `inline_len_at_entry + s.len()` for each whitespace
  character written into `s` — note that escaping shifts byte offsets, so the offset must be computed while building
  `s`, not from the source text. Link text flows through `on_text`, so breaking inside link text falls out correctly for
  free while `](dest)` contributes nothing. Everything else (emphasis markers, code spans, inline HTML, footnote
  references, task-list markers) records nothing and is therefore atomic by construction.

- [x] Change `Event::SoftBreak` to push a space plus a break offset instead of pushing `'\n'`. This single line is the
  core of unwrapping: source line breaks stop being structural and become break candidates like any other space.

- [x] Rework `flush_inline_text` now that the only remaining `'\n'` in `inline` is a hard break (`\\\n`). Its
  `split('\n')` becomes "split into hard-break segments, wrap each independently", with each segment keeping the
  existing first-line versus continuation-line distinction.

---

### Phase 2: Choosing breaks without creating structure

Highest idempotency risk in the feature. Today continuation lines only come from source soft breaks, so their content
was already line-initial in the input. With reflow the formatter chooses what lands at column 0, and can promote
mid-sentence text into block structure: `1.`, `#`, `-`, `>`, `|`, `---`, `<!--`, or a setext underline.

- [x] Implement greedy fill to `width - prefix.len()`, then before committing a break, test the next line with
  `needs_line_escape(line, true)`. If it would parse as structural, retreat to the previous break offset — that yields
  clean output with no backslashes. If no earlier offset exists (a single long token), commit and fall back to
  `escape_line`, accepting that the inserted `\` may push the line one character over. Add no new pattern-matching code:
  `needs_line_escape` already uses pulldown-cmark itself as the oracle, so new CommonMark edge cases keep being handled
  automatically.

- [x] Never break an overlong token. A 200-character URL emits an over-width line. Confirm that MD013's existing
  `link_only_lines` exemption covers the reflow output so `format` and `check` do not disagree on those lines.

- [x] Exempt from reflow: headings (already collapsed by `TagEnd::Heading` and un-wrappable in ATX, so MD013's
  `heading_line_length` stays permanently unfixable), code blocks, HTML blocks, front matter, table cells (the
  `TagEnd::Table` emitter builds its own lines), and link reference definitions.

- [x] Fix the list continuation indent. `flush_inline_text` is called with `"  ".repeat(list_depth)`, which is correct
  for a dash marker (width 2) but wrong for an ordered `1.` marker (width 3). Nearly invisible today because
  continuation lines are rare; reflow generates them constantly. `list_item_widths` already holds the right number.
  Subtract the prefix width from the available width.

---

### Phase 3: Config plumbing

- [x] Add `formatter::FormatOptions { width: usize }` with `From<&Config>` reading `rules.MD013.line_length`. Keep
  `format(input)` as a thin wrapper over `format_with(input, &FormatOptions::default())` so `tests/formatter.rs`,
  `tests/formatter_proptest.rs`, and `tests/integration.rs` compile unchanged and the real diff stays confined to two
  call sites.

- [x] Update `src/main.rs` in `run_format`, which already has `config` in scope. One-line change.

- [x] Update `formatting()` in `src/server/handlers.rs`, which **does not load config at all**. `load_config(uri)`
  exists in the same file and is called by `did_open`, `did_change`, and `code_action`, but `formatting()` skips it.
  This is a pre-existing latent divergence that reflow turns into a visible bug: LSP format-on-save would wrap at 120
  while the CLI wraps at a configured 80.

---

### Phase 4: Language server

- [x] Replace `whole_doc_edit` in `src/server/convert.rs` with minimal line-range edits. It currently replaces the
  entire document in one `TextEdit`. Today most of a file is unchanged so editors cope; with reflow every paragraph
  changes, so every format-on-save becomes a full-document replace — cursors jump unpredictably, undo granularity
  collapses, folding state resets. A line-based common prefix/suffix trim gets most of the benefit in roughly fifteen
  lines and needs no diff crate.

- [x] Update the `tests/lsp.rs` formatting assertion. It asserts exactly one whole-doc edit whose `new_text` equals
  `formatter::format(content)`. It becomes "applying the returned edits to `content` yields
  `format_with(content, opts)`", which is the property actually wanted. The test uses `file:///tmp/test.md`, so
  `find_all_configs` walks up from `/tmp` and finds nothing, giving defaults — verify that still holds once
  `formatting()` loads config, or pin it with an explicit temp dir.

- [ ] Do **not** add `documentRangeFormattingProvider`. Reflow makes "format selection" more desirable, but range
  formatting over a reflowing formatter needs block-boundary snapping. YAGNI until someone asks.

- [ ] Known and unrelated: `did_change` calls `load_config` on every keystroke, which is a filesystem walk per edit.
  Adding one more call in `formatting()` (rare, on save) does not worsen it. Noted, not fixed.

---

### Phase 5: `--check` and rollout

`--check` needs **no code change**. `run_format` already compares `formatted == original` and returns
`args.check && any_changed`. Reflow flows through untouched and exit codes are unaffected. The work is blast radius.

- [x] Land the dogfooding churn as a separate commit after the feature commit. `prek.toml` runs `mdlint-format` and
  `mdlint-check` on every invocation, so the first `prek run -a` after this lands rewrites README.md, CLAUDE.md,
  FORMAT_SPEC.md, npm/README.md, and python/README.md. Keeping it separate is what makes the feature diff reviewable.
  The rules table in README.md is untouched (tables are exempt), so the churn is prose-only. The three-README sync rule
  holds: reflow is deterministic, so all three reflow consistently.

- [x] Leave `MD013::fixable()` as `false`. A per-violation `Fix` can express a multi-line replacement via
  `line_start`/`line_end` plus embedded newlines, so it is technically possible — but MD013 would have to reimplement
  paragraph reflow to build that fix, and rules receive `&MarkdownParser`, not formatter state. That duplicates wrapping
  logic in two places. Document the divergence in FORMAT_SPEC.md instead: `mdlint format` fixes line length,
  `mdlint check --fix` does not. If parity is wanted later, extract a shared `reflow_paragraph(text, width) -> String`
  that both call.

- [x] Write the release note for the minor bump: every Markdown file in a consuming repository will be rewritten on
  first run.

---

### Phase 6: Testing

- [x] Add formatter cases through `assert_formats_to`, which already checks format and idempotency in one call: a long
  paragraph, a paragraph with a mid-way hard break, a paragraph inside a nested ordered list, a paragraph inside a
  blockquote, a paragraph containing a long link, a break that would land `1.`/`#`/`---` at column 0, and a single token
  longer than the width.

- [x] Extend the proptest suite — this is where the feature lives or dies. `formatter_is_idempotent` runs ten thousand
  cases over `".*"`, which almost never produces paragraphs long enough to wrap. Add a targeted generator producing
  multi-line paragraphs of twenty to sixty short words drawn from a corpus salted with join hazards: `*`, `[`, `]`, `_`,
  backtick, `<`, `1.`, `#`, `-`, `>`, `|`, `---`, `===`. Note that `on_text` escapes `\`, backtick, `<`, `_`, and `~`
  but **not** `*` or `[`/`]`: joining two previously separate lines can put those into new adjacency, which is exactly
  what the generator needs to hunt.

- [x] Add a proptest property: wrapping never produces a line exceeding `width` unless that line is a single unbreakable
  token.

---

### Deliberately out of scope

- [ ] `mdlint format --diff` — the natural follow-up once `--check` starts listing every file. Add it only if the
  `--check` output becomes unusable.
- [ ] LSP range formatting (see Phase 4).
- [ ] Making MD013 auto-fixable (see Phase 5).

---

## Outcome

Delivered and green: `prek run -a` passes all twelve hooks, and the reflow properties hold over 30,000 proptest cases.

Two things were needed that the plan did not anticipate:

- **Runs of spaces collapse to one.** A break landing inside a run strands a space at a line edge that `finish` then
  trims, changing the text on every pass. Collapsing is required for idempotency and is invisible when rendered.
  Documented in FORMAT_SPEC.md.
- **No break immediately before inline HTML.** Escaping a tag that lands at column 0 would turn it into literal text, so
  the break opportunity in front of it is withdrawn instead.

One decision worth revisiting: `[rules.MD013] enabled = false` does **not** disable reflow. Only `line_length` is read.
Disabling a lint should not change canonical style, but a user who turned MD013 off may not expect their files to be
rewrapped.

### Pre-existing bugs surfaced by the new proptest generator

None of these were caused by reflow; all reproduce on the commit before it. The generator's dense structural characters
and multi-line paragraphs are simply better at reaching them than `.*` was. Fixed here, with regression tests in
`src/formatter/mod.rs`:

- Hard break inside a heading produced a doubled space.
- Heading text ending in `#` re-parsed as a closed ATX heading and lost a character every pass.
- A list nested under an ordered marker was indented two spaces, below that item's content column, so it re-parsed as a
  sibling list at the outer level.
- A blockquote inside a list item never got its `>` on the first line and was silently flattened into the item's
  paragraph — the quote disappeared entirely.
- `- ---` is itself a thematic break, so a rule inside a tight list item swallowed the list.
- `write_bq_prefix` wrote unconditionally, so `> - # x` gained a nesting level on every pass, without bound.

Still open, deliberately not fixed here:

- [ ] Raw HTML blocks (and empty blockquotes) inside a container lose the container prefix. `Event::Html` writes the
  block straight to the output with no `>` or list indent. Same family as the two container bugs fixed above, but the
  fix means prefixing every line of an opaque HTML block. See the ignored `test_html_block_in_blockquote_keeps_marker`.
  The proptest generator is scoped to single paragraphs to stay off this seam; re-widening it is the way to find the
  rest of the family.
