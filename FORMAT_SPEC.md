# mdlint Format Specification

This document defines the canonical style that `mdlint format` enforces. It is the north star for all
formatter implementation work. Any ambiguity about what the formatter should produce is resolved here.

---

## Guiding Principles

1. **One canonical form.** Every valid Markdown input has exactly one correct formatted output.
2. **Idempotency is a hard requirement.** Formatting an already-formatted file produces no changes.
3. **Semantic equivalence.** The formatter never changes meaning — only surface syntax.
4. **Almost no configuration.** The formatter is opinionated. It accepts exactly one input: the fill
   column used for paragraph reflow, read from `rules.MD013.line_length`. Every other choice is
   fixed. If you disagree with one, open an issue.

---

## Formatter Architecture Decision

The formatter uses **approach (a): emit canonical text directly from pulldown-cmark events**.

Rationale:

- pulldown-cmark's event stream is a faithful structural representation of CommonMark documents.
  Walking the event stream lets us re-emit each element in canonical form without building a
  separate IR.
- An IR approach would require modeling every CommonMark construct (including tricky ones like
  nested emphasis or lazy continuation lines), duplicating work the parser already did.
- Direct emission is simpler, easier to test, and keeps the formatter code minimal.

Trade-offs accepted:

- Some decisions (e.g., blank line insertion between block elements) require peeking at the next
  event or buffering output, since the event stream carries no blank-line information. The formatter
  maintains a small state machine to track the previous block element type and inserts blank lines
  accordingly before emitting each new block.
- Raw HTML blocks are passed through verbatim; the formatter does not attempt to reformat HTML.

---

## Canonical Style Rules

### Headings (MD003)

Always use ATX style. Never setext style. Never closed ATX style.

```markdown
# Heading 1
## Heading 2
### Heading 3
```

Not:

```markdown
Heading 1
=========

Heading 2
---------

# Heading 3 #
```

Exactly one space between the `#` markers and the heading text (MD018, MD019). No trailing `#`
characters. Heading text is not modified (content is preserved as-is).

### Blank Lines Around Headings (MD022)

Every heading is preceded by exactly one blank line and followed by exactly one blank line, except:

- A heading at the very start of the file has no preceding blank line.
- A heading immediately following front matter has no preceding blank line.

```markdown
# Document Title

Introductory paragraph.

## Section One

Content here.

## Section Two

More content.
```

### Unordered List Markers (MD004)

Always use `-` (dash). Never `*` or `+`.

```markdown
- First item
- Second item
  - Nested item
  - Another nested item
- Third item
```

### Ordered List Markers (MD029)

Always use sequential numbering starting from `1.`. Items are renumbered regardless of
what numbers appear in the source — non-contiguous or repeated numbers are corrected.

```markdown
1. First item
2. Second item
3. Third item
```

### List Indentation (MD007)

Nested list items are indented to their parent item's content column: two
spaces under a `-` marker, three under a `1.` marker. Less than that and the
nested list re-parses as a sibling list at the outer level.

```markdown
- Top level
  - Nested once
    - Nested twice

1. Top level
   - Nested once
     - Nested twice
```

### Blank Lines Around Lists (MD032)

Lists are preceded and followed by exactly one blank line (same rule as other block elements).

### Code Fences (MD048)

Always use backticks (`` ` ``). Always use exactly three backticks. Never tildes (`~~~`).

````markdown
```language
code here
```
````

Include the language identifier when known. The formatter preserves whatever language tag was
present in the source; it does not infer or remove language tags.

### Emphasis (MD049)

Always use `*` for emphasis (italic). Never `_`.

```markdown
This is *important*.
```

Exception: underscores inside words (snake_case identifiers) are not emphasis and are not modified.

### Strong Emphasis (MD050)

Always use `**` for strong (bold). Never `__`.

```markdown
This is **critical**.
```

### Paragraph Reflow (MD013)

Paragraph text is reflowed. Existing line breaks inside a paragraph carry no meaning in CommonMark —
they render as a single space — so the formatter discards them and refills the paragraph greedily up
to the fill column.

The fill column is `rules.MD013.line_length` (default 120), measured in characters, not bytes. Taking
it from MD013 rather than a separate key means a formatted file can never fail MD013 for a reason the
formatter was able to fix.

```markdown
Input, wrapped at whatever width the author's editor happened to use:

A paragraph that was
hard-wrapped at
some arbitrary column.

Output, refilled:

A paragraph that was hard-wrapped at some arbitrary column.
```

Reflow applies to paragraph text only. It does not apply to headings, code blocks, HTML blocks, front
matter, table cells, or link reference definitions.

Line breaks are only ever inserted at a space that appears in the source text or at a former soft
break. The formatter never breaks inside a link destination, a code span, an inline HTML tag, or
immediately after a hard-break marker, because a break in any of those positions changes meaning.

Two consequences follow:

- **A token longer than the fill column is never broken.** A long URL, path, or identifier emits an
  over-width line rather than being corrupted. MD013 exempts prose lines it cannot break -- those
  whose content past any list or blockquote marker holds no space -- so the formatter never produces
  output that the linter then complains about. Headings are the exception: they cannot be wrapped at
  all, so `heading_line_length` stays a genuinely unfixable check.
- **Hard breaks are preserved.** A paragraph containing hard breaks is split into segments at each
  hard break, and each segment is refilled independently.

Where a break would place text at the start of a line such that it re-parses as a block element (a
list marker, an ATX heading, a thematic rule, a setext underline), the formatter first tries to break
earlier. If no earlier break opportunity exists, it breaks anyway and escapes the line.

Reflow is suppressed wherever MD013 is switched off by an inline comment
(`<!-- mdlint-disable MD013 -->`, `<!-- mdlint-disable-next-line MD013 -->`, or a blanket
`<!-- mdlint-disable -->`). Protected paragraphs keep the author's line breaks and are not refilled,
which is how deliberately hand-wrapped prose is preserved. Because reflow rewrites a whole paragraph
or none of it, a directive covering any part of a paragraph protects the whole paragraph.
`no_inline_config = true` disables this along with every other inline directive.

### Trailing Whitespace (MD009)

No trailing spaces or tabs on any line. Hard line breaks (two trailing spaces before a newline) are
replaced with a `\` continuation character, then the trailing spaces are removed.

```markdown
Line one\
Line two
```

### Hard Tabs (MD010)

All hard tabs in non-code content are replaced with spaces. The number of spaces is determined by
expanding to the next 4-space tab stop.

Tabs inside fenced code blocks and indented code blocks are preserved verbatim.

### Multiple Consecutive Blank Lines (MD012)

At most one blank line between any two block elements. Multiple consecutive blank lines are
collapsed to one.

### Trailing Newline (MD047)

Every file ends with exactly one newline character (`\n`). No trailing blank lines. No missing
final newline.

### Horizontal Rules (MD035)

Always use `---` (three dashes). Never `***`, `___`, `- - -`, or other variants.

```markdown
---
```

Horizontal rules are preceded and followed by blank lines (same block-spacing rule as other
block elements).

### Link and Image Style (MD054)

The formatter does not rewrite link or image syntax between styles (inline vs. reference).
It does remove unnecessary angle brackets from URLs that do not require them per CommonMark
(MD034).

### Blockquotes (MD027, MD028)

Exactly one space after each `>` marker:

```markdown
> This is a blockquote.
>
> Second paragraph in the same blockquote.
```

No blank lines between consecutive blockquote lines that belong to the same block. One blank line
between a blockquote and surrounding content.

### ATX Heading Space (MD018, MD019)

Exactly one space between the opening `#` characters and the heading text. No extra spaces.

```markdown
# Correct
## Also Correct
```

Not: `` #No space `` or `` ##  Two spaces ``

### Headings Must Start at the Beginning of the Line (MD023)

Headings are never indented.

```markdown
# This is correct
```

Not: a heading preceded by spaces (e.g., two spaces then `# heading`)

### Front Matter

Front matter (YAML `---` blocks or TOML `+++` blocks) at the start of a file is passed through
verbatim. The formatter does not modify front matter content.

---

## What the Formatter Does NOT Change

- **Paragraph wording.** Reflow moves the line breaks between words (see "Paragraph Reflow"), but the
  words themselves, and their order, are never altered.
- **Code block contents.** The content inside fenced or indented code blocks is preserved
  character-for-character, including indentation, tabs, and blank lines.
- **Inline code.** The content inside backtick spans is not modified.
- **HTML blocks.** Raw HTML blocks are passed through verbatim.
- **Link/image URLs and titles.** Not reformatted.
- **Heading text content.** The text of headings is preserved exactly; only the surrounding
  syntax (ATX vs setext, spacing) is canonicalized.
- **Table content.** Cell content is preserved. Column alignment markers are preserved. Table
  formatting (column widths, pipe alignment) may be normalized in a future version but is not
  in scope for the initial implementation.

---

## Formatter Output Contract

Given any CommonMark-compliant input:

1. `format(input)` produces output that is semantically equivalent to `input`.
2. `format(format(input)) == format(input)` (idempotency).
3. `format(input)` ends with exactly one `\n`.
4. `format(input)` contains no trailing whitespace on any line.
5. `format(input)` parses as valid CommonMark.

---

## Relationship to Linting Rules

Every rule listed in the Canonical Style Rules section above corresponds to a `mdlint check`
violation that the formatter fixes. The formatter and the `--fix` flag in `mdlint check` must
produce identical output for all fixable rules. There must be no divergence between the two paths.

Rules that the formatter enforces but the linter cannot report (because they require whole-document
context beyond what a per-violation fix can express) are formatter-only behaviors documented above.

MD013 is the one deliberate exception to the "no divergence" rule, in the opposite direction:
`mdlint format` fixes over-long paragraph lines by reflowing them, but `mdlint check --fix` does not.
Reflow is a whole-paragraph rewrite, and expressing it as a per-violation `Fix` would mean
reimplementing the wrapping logic inside the rule, where only a `MarkdownParser` is available rather
than formatter state. Duplicating it in two places is worse than the divergence. Run `mdlint format`
to fix line length.
