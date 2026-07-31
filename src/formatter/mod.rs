use std::fmt::Write as _;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Format a Markdown document to canonical style.
///
/// Returns the formatted document as a String. The output:
/// - Always ends with exactly one trailing newline (or is empty for empty input)
/// - Has exactly one blank line between top-level block elements
/// - Uses ATX-style headings
/// - Uses `-` for unordered list markers
/// - Uses backtick fences for code blocks
#[must_use]
pub fn format(input: &str) -> String {
    if input.trim().is_empty() {
        return String::new();
    }

    let mut state = FormatterState::new();
    let events: Vec<Event<'_>> = Parser::new_ext(input, mk_options()).collect();

    // Precompute per-event lookahead: is the *next* event Start(List(None))?
    let lookahead: Vec<bool> = (0..events.len())
        .map(|i| matches!(events.get(i + 1), Some(Event::Start(Tag::List(None)))))
        .collect();

    // Precompute the first character of the immediately following Text event, if
    // any.  pulldown-cmark splits a run like `Ⓐ~A` into three Text events; the
    // `_`/`~` flanking check in on_text needs the char *after* the current event
    // to match its cross-event handling of the char *before* (via self.inline).
    // Only an adjacent Text event contributes an alphanumeric neighbour; any other
    // event (emphasis marker, code, break, block end) is a non-alphanumeric
    // boundary, represented as None.
    let next_text_char: Vec<Option<char>> = (0..events.len())
        .map(|i| match events.get(i + 1) {
            Some(Event::Text(t)) => t.chars().next(),
            _ => None,
        })
        .collect();

    for ((event, next_is_ul), next_char) in events.into_iter().zip(lookahead).zip(next_text_char) {
        state.next_is_unordered_list = next_is_ul;
        state.next_text_char = next_char;
        state.process(event);
    }

    state.finish()
}

fn mk_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
}

#[allow(clippy::struct_excessive_bools)] // each bool is a distinct formatting phase flag
struct FormatterState {
    out: String,
    /// Whether the next block element should be preceded by a blank line.
    needs_blank: bool,

    // List state
    list_depth: usize,
    /// Start number for ordered list at each depth; None = unordered.
    list_starts: Vec<Option<u64>>,
    /// True when a list item was just opened but no Paragraph started yet (tight list).
    in_tight_item: bool,

    // Blockquote state
    bq_depth: usize,

    // Inline content buffer, flushed when a block element closes.
    inline: String,

    // Code block state
    in_code_block: bool,
    code_block_indent: String,

    // Per-depth item marker widths (e.g. 3 for "1. ", 2 for "- "), used to
    // compute the continuation indent for code blocks inside list items.
    list_item_widths: Vec<usize>,

    // Link/image stack: stores (dest_url, title) from Start until End.
    link_stack: Vec<(String, String)>,

    // Set by the outer format() loop before each event: true when the
    // immediately following event is Start(List(None)).  Used to detect
    // two adjacent unordered lists so we can insert a separator.
    next_is_unordered_list: bool,

    // First char of the next Text event, or None if the next event is not Text.
    // Supplies cross-event right-flank context for the `_`/`~` escape check.
    next_text_char: Option<char>,

    // Table state
    table_alignments: Vec<Alignment>,
    table_head_cells: Vec<String>,
    table_data_rows: Vec<Vec<String>>,
    current_row_cells: Vec<String>,
    in_table_head: bool,
}

impl FormatterState {
    fn new() -> Self {
        Self {
            out: String::new(),
            needs_blank: false,
            list_depth: 0,
            list_starts: Vec::new(),
            in_tight_item: false,
            bq_depth: 0,
            inline: String::new(),
            in_code_block: false,
            code_block_indent: String::new(),
            list_item_widths: Vec::new(),
            link_stack: Vec::new(),
            next_is_unordered_list: false,
            next_text_char: None,
            table_alignments: Vec::new(),
            table_head_cells: Vec::new(),
            table_data_rows: Vec::new(),
            current_row_cells: Vec::new(),
            in_table_head: false,
        }
    }

    fn process(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.on_start(tag),
            Event::End(tag) => self.on_end(tag),
            Event::Text(t) => self.on_text(&t),
            Event::Code(c) => self.emit_inline_code(&c),
            Event::Html(h) => {
                self.out.push_str(&h);
            }
            Event::InlineHtml(h) => {
                self.inline.push_str(&h);
            }
            Event::SoftBreak => {
                self.inline.push('\n');
            }
            Event::HardBreak => {
                // Backslash + newline = hard line break in CommonMark.
                // Using backslash style avoids trailing-whitespace stripping.
                self.inline.push_str("\\\n");
            }
            Event::Rule => {
                self.emit_blank_if_needed();
                self.write_bq_prefix();
                self.out.push_str("---\n");
                self.needs_blank = true;
            }
            Event::FootnoteReference(label) => {
                write!(self.inline, "[^{label}]").expect("writing to String is infallible");
            }
            Event::TaskListMarker(checked) => {
                if checked {
                    self.inline.push_str("[x] ");
                } else {
                    self.inline.push_str("[ ] ");
                }
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_lines)] // exhaustive match over pulldown-cmark Tag variants
    fn on_start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                // Inside a list, don't emit a blank before the paragraph—
                // the item marker was already written.
                if self.list_depth == 0 {
                    self.emit_blank_if_needed();
                }
                self.in_tight_item = false;
            }
            Tag::Heading { .. } => {
                self.emit_blank_if_needed();
                // The prefix (hashes) is written at End, when we have the level.
            }
            Tag::CodeBlock(kind) => {
                self.emit_blank_if_needed();
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.into_string().replace('\\', "\\\\"),
                    CodeBlockKind::Indented => String::new(),
                };
                let fence_indent = self.list_continuation_prefix();
                // When the fence lands on the same line as the list marker (tight item),
                // the effective list margin becomes marker_width + fence_indent_width.
                // Content and closing fence must use this combined width to stay inside the item.
                let content_indent = if self.in_tight_item {
                    let marker_width = self.list_item_widths.last().copied().unwrap_or(0);
                    " ".repeat(marker_width + fence_indent.len())
                } else {
                    fence_indent.clone()
                };
                let was_tight = self.in_tight_item;
                self.in_tight_item = false;
                self.code_block_indent = content_indent;
                // When the fence is on the same line as the list marker (tight
                // item), the blockquote prefix was already written by Tag::Item.
                // Writing it again would insert an extra `>` that the re-parser
                // interprets as a nested blockquote, breaking idempotency.
                if !was_tight {
                    self.write_bq_prefix();
                }
                self.out.push_str(&fence_indent);
                self.out.push_str("```");
                self.out.push_str(&lang);
                self.out.push('\n');
                self.in_code_block = true;
            }
            Tag::List(start) => {
                self.list_item_widths.push(0);
                if self.list_depth == 0 {
                    self.emit_blank_if_needed();
                } else {
                    // Nested list: suppress any pending blank line.
                    // A sublist follows its parent item text without a blank line.
                    self.needs_blank = false;
                    // Flush any tight-item inline content that preceded this sublist
                    // (e.g. `Text("Item 1")` in `- Item 1\n  - Nested`).
                    if self.in_tight_item && !self.inline.is_empty() {
                        let text = std::mem::take(&mut self.inline);
                        let prefix = "  ".repeat(self.list_depth);
                        self.flush_inline_text(&text, &prefix);
                        self.in_tight_item = false;
                    } else if self.in_tight_item {
                        // Outer tight item has no inline content before this nested list.
                        // Terminate the outer marker with a newline so inner markers are
                        // on their own lines, preventing markers from merging on re-parse.
                        self.out.push('\n');
                        self.in_tight_item = false;
                    }
                }
                self.list_depth += 1;
                // Ordered lists always start at 1 in canonical form (MD029).
                self.list_starts.push(start.map(|_| 1u64));
            }
            Tag::Item => {
                // For loose lists, End(Paragraph) sets needs_blank = true.
                // Emit that blank before the next item marker.
                if self.list_depth > 0 {
                    self.emit_blank_if_needed();
                }
                self.in_tight_item = true;
                let indent = "  ".repeat(self.list_depth.saturating_sub(1));
                let marker = match self.list_starts.last_mut() {
                    Some(Some(n)) => {
                        let s = format!("{indent}{n}. ");
                        *n += 1;
                        s
                    }
                    _ => format!("{indent}- "),
                };
                if let Some(w) = self.list_item_widths.last_mut() {
                    *w = marker.len();
                }
                self.write_bq_prefix();
                self.out.push_str(&marker);
            }
            Tag::Emphasis => self.inline.push('*'),
            Tag::Strong => self.inline.push_str("**"),
            Tag::Strikethrough => self.inline.push_str("~~"),
            Tag::Link {
                dest_url, title, ..
            } => {
                self.link_stack
                    .push((dest_url.into_string(), title.into_string()));
                self.inline.push('[');
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                self.link_stack
                    .push((dest_url.into_string(), title.into_string()));
                self.inline.push_str("![");
            }
            Tag::HtmlBlock => {
                self.emit_blank_if_needed();
            }
            Tag::BlockQuote(_) => {
                self.emit_blank_if_needed();
                self.bq_depth += 1;
            }
            Tag::FootnoteDefinition(label) => {
                self.emit_blank_if_needed();
                // Write the label prefix; body will be flushed inline.
                self.write_bq_prefix();
                write!(self.out, "[^{label}]: ").expect("writing to String is infallible");
            }
            Tag::Table(alignments) => {
                self.emit_blank_if_needed();
                self.table_alignments.clone_from(&alignments);
                self.table_head_cells = Vec::new();
                self.table_data_rows = Vec::new();
                self.current_row_cells = Vec::new();
                self.in_table_head = false;
            }
            Tag::TableHead => {
                self.in_table_head = true;
            }
            Tag::TableRow => {
                self.current_row_cells = Vec::new();
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_lines)] // exhaustive match over pulldown-cmark TagEnd variants
    fn on_end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                let text = std::mem::take(&mut self.inline);
                // pulldown-cmark may emit a paragraph containing only Unicode
                // whitespace (e.g. NEL U+0085) that is not a CommonMark line
                // ending — finish() strips it via trim_end(), leaving an empty
                // line that turns a blockquote into an empty one on re-parse.
                // Skip the emission entirely; invisible content is no content.
                if !text.trim().is_empty() {
                    if self.list_depth == 0 {
                        self.write_bq_prefix();
                    }
                    let prefix = "  ".repeat(self.list_depth);
                    self.flush_inline_text(&text, &prefix);
                    self.needs_blank = true;
                }
                self.in_tight_item = false;
            }
            TagEnd::Heading(level) => {
                let text = std::mem::take(&mut self.inline);
                let hashes = "#".repeat(level as usize);
                self.write_bq_prefix();
                // Collapse hard and soft breaks to spaces, then trim.  Trim must
                // come after: a leading break produces a leading space that trim()
                // removes; trimming first would strip a hard-break marker's `\`,
                // leaving it unescaped and breaking idempotency on re-parse.
                let heading_raw = collapse_heading_breaks(&text);
                let heading_text = heading_raw.trim();
                writeln!(self.out, "{hashes} {heading_text}").expect("writing to String is infallible");
                self.needs_blank = true;
            }
            TagEnd::CodeBlock => {
                // Ensure code block content ends with a newline so the closing
                // fence is never appended to the last content line.
                if !self.out.ends_with('\n') {
                    self.out.push('\n');
                }
                self.write_bq_prefix();
                self.out.push_str(&self.code_block_indent.clone());
                self.out.push_str("```\n");
                self.in_code_block = false;
                self.code_block_indent = String::new();
                self.needs_blank = true;
            }
            TagEnd::List(_) => {
                self.list_depth -= 1;
                self.list_starts.pop();
                self.list_item_widths.pop();
                if self.list_depth == 0 {
                    if self.next_is_unordered_list {
                        // Two adjacent unordered lists would merge into one on
                        // re-parse (both normalise to `-`). Insert an invisible
                        // HTML comment to keep them separate.
                        self.needs_blank = false;
                        self.out.push_str("\n<!---->\n");
                        self.needs_blank = true;
                    } else {
                        self.needs_blank = true;
                    }
                }
            }
            TagEnd::Item
                // Tight list item: the content was never wrapped in Paragraph.
                if self.in_tight_item => {
                    let text = std::mem::take(&mut self.inline);
                    if text.is_empty() {
                        // Empty tight item: the marker was already written; just terminate the line.
                        self.out.push('\n');
                    } else {
                        let prefix = "  ".repeat(self.list_depth);
                        self.flush_inline_text(&text, &prefix);
                    }
                    self.in_tight_item = false;
                }
            TagEnd::Emphasis => self.inline.push('*'),
            TagEnd::Strong => self.inline.push_str("**"),
            TagEnd::Strikethrough => self.inline.push_str("~~"),
            TagEnd::Link | TagEnd::Image => {
                if let Some((dest, title)) = self.link_stack.pop() {
                    if title.is_empty() {
                        write!(self.inline, "]({dest})").expect("writing to String is infallible");
                    } else {
                        write!(self.inline, "]({dest} \"{title}\")").expect("writing to String is infallible");
                    }
                }
            }
            TagEnd::HtmlBlock => {
                if !self.out.ends_with('\n') {
                    self.out.push('\n');
                }
                self.needs_blank = true;
            }
            TagEnd::BlockQuote(_) => {
                self.bq_depth -= 1;
                self.needs_blank = true;
            }
            TagEnd::FootnoteDefinition => {
                let text = std::mem::take(&mut self.inline);
                self.flush_inline_text(&text, "");
                self.needs_blank = true;
            }
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.inline);
                self.current_row_cells.push(cell);
            }
            TagEnd::TableHead => {
                // Cells may have been collected either via End(TableRow) inside the head
                // or directly (if no TableRow wrapper was emitted).
                if self.table_head_cells.is_empty() {
                    self.table_head_cells = std::mem::take(&mut self.current_row_cells);
                }
                self.in_table_head = false;
            }
            TagEnd::TableRow => {
                let row = std::mem::take(&mut self.current_row_cells);
                if self.in_table_head {
                    self.table_head_cells = row;
                } else {
                    self.table_data_rows.push(row);
                }
            }
            TagEnd::Table => {
                let head = std::mem::take(&mut self.table_head_cells);
                let rows = std::mem::take(&mut self.table_data_rows);
                let aligns = std::mem::take(&mut self.table_alignments);

                // Header row
                self.write_bq_prefix();
                self.out.push_str("| ");
                self.out.push_str(&head.join(" | "));
                self.out.push_str(" |\n");

                // Separator row
                self.write_bq_prefix();
                self.out.push_str("| ");
                let seps: Vec<&str> = aligns
                    .iter()
                    .map(|a| match a {
                        Alignment::Left => ":---",
                        Alignment::Right => "---:",
                        Alignment::Center => ":---:",
                        Alignment::None => "---",
                    })
                    .collect();
                self.out.push_str(&seps.join(" | "));
                self.out.push_str(" |\n");

                // Data rows
                for row in rows {
                    self.write_bq_prefix();
                    self.out.push_str("| ");
                    self.out.push_str(&row.join(" | "));
                    self.out.push_str(" |\n");
                }

                self.needs_blank = true;
            }
            _ => {}
        }
    }

    fn on_text(&mut self, text: &str) {
        if self.in_code_block {
            // Code block content goes directly to output, with list
            // continuation indent re-added (pulldown-cmark strips it).
            // When inside a blockquote, each content line also needs the
            // `> ` prefix so that the re-parser keeps the content inside
            // the blockquote (fence lines already get the prefix via
            // write_bq_prefix, but content lines arrive here as Text events).
            let bq = "> ".repeat(self.bq_depth);
            if bq.is_empty() && self.code_block_indent.is_empty() {
                self.out.push_str(text);
            } else {
                for line in text.split_inclusive('\n') {
                    self.out.push_str(&bq);
                    self.out.push_str(&self.code_block_indent);
                    self.out.push_str(line);
                }
            }
        } else {
            // `\\` and `` ` `` are resolved unconditionally by pulldown-cmark regardless
            // of context, so always re-escape them.
            //
            // For `_` and `~`, escape only at positions where the character is NOT between
            // two Unicode alphanumeric characters.  Intra-word delimiters (e.g. `x86_64`)
            // can never open or close emphasis per CommonMark Rule 10/17; everything else
            // must be escaped or it may form emphasis/strikethrough on the next parse.
            //
            // pulldown-cmark may split a single logical run into multiple Text events (e.g.
            // `\_0_` → `Text("_0")` + `Text("_")`).  The combined output `_0_` would form
            // emphasis on re-parse.  To catch this, we use the last char already written to
            // `self.inline` as the "preceding character" for the first char of the event.
            //
            // pulldown-cmark occasionally emits bare `\r` characters inside text events
            // (e.g. from heading content that contains `\r` without a following `\n`).
            // Emitting a raw `\r` into output causes it to be treated as a line ending on
            // re-parse (CommonMark spec §2.3), breaking the heading/paragraph structure and
            // therefore idempotency.  Normalise before processing.
            let text = &*text.replace("\r\n", "\n").replace('\r', "\n");
            let prev_inline_char = self.inline.chars().next_back();
            let chars: Vec<char> = text.chars().collect();
            let mut s = String::with_capacity(text.len() + 4);
            for (i, &ch) in chars.iter().enumerate() {
                match ch {
                    '\\' => s.push_str("\\\\"),
                    '`' => s.push_str("\\`"),
                    // A literal `<` in a Text event (pulldown only emits `<` as text
                    // when it does NOT already open a tag).  Left bare, adjacent text
                    // can reconstruct an autolink or HTML tag on re-parse (e.g.
                    // `<#@a>` → email autolink), changing meaning.  `\<` renders as
                    // `<` and can never start a tag, so escape unconditionally.
                    '<' => s.push_str("\\<"),
                    '_' | '~' => {
                        let prev = if i > 0 {
                            chars.get(i - 1).copied()
                        } else {
                            prev_inline_char
                        };
                        // For the last char of the event, the right neighbour is
                        // the first char of the next Text event (if any) so that a
                        // run split across events (e.g. `Ⓐ~A` → three events) is
                        // judged the same as the merged form on re-parse.
                        let next = chars.get(i + 1).copied().or(if i + 1 == chars.len() {
                            self.next_text_char
                        } else {
                            None
                        });
                        // Only leave bare when flanked by alphanumeric on BOTH sides.
                        if prev.is_some_and(char::is_alphanumeric)
                            && next.is_some_and(char::is_alphanumeric)
                        {
                            s.push(ch);
                        } else {
                            s.push('\\');
                            s.push(ch);
                        }
                    }
                    _ => s.push(ch),
                }
            }
            self.inline.push_str(&s);
        }
    }

    fn emit_inline_code(&mut self, code: &str) {
        // Choose a delimiter longer than any backtick run in the content.
        let max_run = code.chars().fold((0usize, 0usize), |(max, cur), ch| {
            if ch == '`' {
                (max.max(cur + 1), cur + 1)
            } else {
                (max, 0)
            }
        });
        let delim = "`".repeat(max_run.0 + 1);
        let needs_space = code.starts_with('`') || code.ends_with('`');
        self.inline.push_str(&delim);
        if needs_space {
            self.inline.push(' ');
        }
        self.inline.push_str(code);
        if needs_space {
            self.inline.push(' ');
        }
        self.inline.push_str(&delim);
    }

    /// Returns the continuation indent for the current innermost list item —
    /// i.e. the number of spaces needed to keep a block element (like a code
    /// fence) inside that item.  Empty string when not inside a list.
    fn list_continuation_prefix(&self) -> String {
        " ".repeat(self.list_item_widths.last().copied().unwrap_or(0))
    }

    fn emit_blank_if_needed(&mut self) {
        if self.needs_blank && !self.out.is_empty() {
            if self.bq_depth > 0 {
                // Inside a blockquote, the separator line must carry the `>`
                // marker so the parser keeps both paragraphs in the same block.
                self.out.push_str(&">".repeat(self.bq_depth));
            }
            self.out.push('\n');
        }
        self.needs_blank = false;
    }

    fn write_bq_prefix(&mut self) {
        self.out.push_str(&"> ".repeat(self.bq_depth));
    }

    /// Flush inline text to output.
    /// Each line in `text` gets the blockquote prefix prepended (except the first,
    /// which follows whatever was already written on the current output line).
    fn flush_inline_text(&mut self, text: &str, continuation_prefix: &str) {
        // Strip trailing hard-break markers (`\\\n`) preceded by only whitespace.
        // A `\` before a line ending that is at the end of a block is re-parsed by
        // pulldown-cmark as a literal `\`, not a hard break — so emitting `\\\n` at
        // the end of a paragraph breaks idempotency (the formatter doubles the `\`
        // on the second pass).  A trailing hard break is always a no-op: there is
        // nothing on the "next line" for the break to separate.
        let text = {
            let s = text.trim_end_matches(|c: char| c != '\n' && c.is_whitespace());
            // Strip a trailing hard-break marker only when the backslash run before
            // `\n` is odd: even runs are content pairs (`\\` = literal `\`) and must
            // not be removed.  An odd run = zero or more content pairs + one marker.
            if let Some(stripped) = s.strip_suffix('\n') {
                let run = stripped.chars().rev().take_while(|&c| c == '\\').count();
                if run % 2 == 1 {
                    &stripped[..stripped.len() - 1]
                } else {
                    text
                }
            } else {
                text
            }
        };
        let bq = "> ".repeat(self.bq_depth);
        let mut lines = text.split('\n').peekable();

        if let Some(first) = lines.next() {
            if self.bq_depth > 0 && (self.out.ends_with('\n') || self.out.is_empty()) {
                self.out.push_str(&bq);
            }
            if needs_line_escape(first, false) {
                self.out.push_str(&escape_line(first));
            } else {
                self.out.push_str(first);
            }
            self.out.push('\n');
        }

        while let Some(line) = lines.next() {
            if lines.peek().is_none() && line.is_empty() {
                // Trailing empty string from split: don't emit an extra newline.
                break;
            }
            // Skip blank or whitespace-only continuation lines.  Inside a paragraph
            // a blank line is impossible in real Markdown (it ends the paragraph).
            // These arise from: (a) consecutive breaks (HardBreak + SoftBreak with
            // no text) whose combined `\n`s produce an empty slot when split; or (b)
            // lines consisting entirely of Unicode whitespace, which finish()'s
            // trim_end() reduces to blank anyway.  Both cases strand any preceding
            // hard-break marker as a literal `\` that on_text doubles on re-parse.
            if line.trim_end().is_empty() {
                continue;
            }
            self.out.push_str(continuation_prefix);
            self.out.push_str(&bq);
            if needs_line_escape(line, true) {
                self.out.push_str(&escape_line(line));
            } else {
                self.out.push_str(line);
            }
            self.out.push('\n');
        }
    }

    fn finish(mut self) -> String {
        let s = std::mem::take(&mut self.out);
        let mut result: Vec<&str> = Vec::new();
        let mut prev_blank = false;
        for line in s.lines() {
            let line = line.trim_end();
            if line.is_empty() {
                if !prev_blank {
                    result.push(line);
                }
                prev_blank = true;
            } else {
                result.push(line);
                prev_blank = false;
            }
        }
        // Strip leading blank lines (e.g. from Unicode-whitespace-only lines such
        // as NBSP that trim_end() reduces to empty but aren't caught by the
        // initial input.trim().is_empty() guard).
        let start = result
            .iter()
            .position(|l| !l.is_empty())
            .unwrap_or(result.len());
        let joined = result
            .get(start..)
            .expect("start bounded by result.len()")
            .join("\n");
        let trimmed = joined.trim_end_matches('\n');
        if trimmed.is_empty() {
            return String::new();
        }
        format!("{trimmed}\n")
    }
}

/// Collapse the break markers inside a heading's inline buffer to single spaces.
///
/// A heading cannot span lines, so both soft breaks (`\n`) and hard breaks
/// (`\` + `\n`, emitted by `on_start`/`HardBreak`) become spaces.  The subtlety
/// is telling a hard-break `\` apart from a literal backslash in the heading
/// text: `on_text` always doubles content backslashes, so a run of backslashes
/// originating from content is even-length.  A hard break adds exactly one more,
/// making the run before the newline odd.  We therefore keep floor(n/2) escaped
/// backslashes (the content) and drop the trailing odd one (the marker) before
/// replacing the newline with a space.
fn collapse_heading_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let mut run = 1usize;
            while chars.peek() == Some(&'\\') {
                chars.next();
                run += 1;
            }
            // An odd run immediately before a newline ends in a hard-break marker;
            // emit the content pairs and drop the marker backslash.
            let is_hard_break = run % 2 == 1 && chars.peek() == Some(&'\n');
            let content_backslashes = if is_hard_break { run - 1 } else { run };
            for _ in 0..content_backslashes {
                out.push('\\');
            }
        } else if ch == '\n' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Escape `line` so that it round-trips through pulldown-cmark as plain text.
///
/// All block-trigger patterns whose first character is ASCII punctuation are
/// escaped by prepending `\`.  The exception is ordered-list markers (`0.`,
/// `12.`): digits are not ASCII punctuation, so `\0` is not a valid `CommonMark`
/// escape and would be doubled on re-parse.  Instead we place the backslash
/// before the `.` or `)` — `0\.` — which is valid and renders identically.
fn escape_line(line: &str) -> String {
    let digits_len = line.chars().take_while(char::is_ascii_digit).count();
    if digits_len > 0 {
        format!("{}\\{}", &line[..digits_len], &line[digits_len..])
    } else {
        format!("\\{line}")
    }
}

/// Returns true if `line`, when emitted as the start of a new output line,
/// would be re-interpreted as a structural block element on re-parse.
///
/// Uses pulldown-cmark itself as the oracle: if parsing `line` in isolation
/// does not produce `Start(Paragraph)` as its first event, the line will be
/// misread — escape it.  This delegates all structural detection to the same
/// parser that the formatter and linter use, so new `CommonMark` edge cases are
/// handled automatically without manual pattern maintenance.
///
/// The one exception kept as a manual check is the setext heading underline on
/// a continuation line (`===`, `--` etc.): these parse as plain paragraphs in
/// isolation but turn the *preceding* output line into a heading when emitted
/// together.  That context-sensitivity cannot be detected by a single-line parse.
///
/// On continuation lines (`is_continuation = true`), ordered-list markers other
/// than `1.`/`1)` do NOT interrupt a paragraph (`CommonMark` spec §5.2) and must
/// not be escaped — escaping them hides broken-list errors from the linter.
/// Since cmark parses `2. foo` in isolation as a list item, we suppress the
/// escape for those cases here.
fn needs_line_escape(line: &str, is_continuation: bool) -> bool {
    // finish() strips trailing Unicode whitespace; check the trimmed form so
    // structural patterns hidden behind trailing Unicode whitespace are caught.
    let line = line.trim_end();
    if line.is_empty() {
        return false;
    }

    // Setext heading underlines are context-sensitive: `===` / `--` alone parse
    // as paragraphs, but after a text line they become headings.
    if is_continuation {
        let trimmed = line.trim_end_matches([' ', '\t']);
        if !trimmed.is_empty()
            && (trimmed.chars().all(|c| c == '=') || trimmed.chars().all(|c| c == '-'))
        {
            return true;
        }
    }

    // On continuation lines, only `1.`/`1)` can interrupt a paragraph — don't
    // escape other ordered-list numbers even though cmark would flag them.
    if is_continuation {
        let digits_len = line.chars().take_while(char::is_ascii_digit).count();
        if digits_len > 0 {
            let rest = &line[digits_len..];
            if let Some(after) = rest.strip_prefix(['.', ')'])
                && (after.is_empty() || after.starts_with([' ', '\t']))
                && &line[..digits_len] != "1"
            {
                return false;
            }
        }
    }

    // Delegate all other structural detection to cmark: if the line does not
    // parse as a paragraph in isolation, it must be escaped.
    !matches!(
        Parser::new_ext(line, mk_options()).next(),
        Some(Event::Start(Tag::Paragraph))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assert that `input` formats to `expected` AND that `expected` is already
    /// canonical (formatting it again produces no change — the "not-fix" side).
    fn assert_formats_to(input: &str, expected: &str) {
        let got = format(input);
        assert_eq!(
            got, expected,
            "format(input) did not match expected.\nInput:\n{input}\nExpected:\n{expected}\nGot:\n{got}"
        );
        assert_eq!(
            format(expected),
            expected,
            "format(expected) != expected — already-canonical content must be unchanged.\nExpected:\n{expected}"
        );
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(format(""), "");
        assert_eq!(format("   "), "");
        assert_eq!(format("\n\n"), "");
    }

    #[test]
    fn test_simple_paragraph() {
        assert_eq!(format("Hello, world."), "Hello, world.\n");
    }

    #[test]
    fn test_atx_heading() {
        assert_eq!(format("# Heading 1"), "# Heading 1\n");
        assert_eq!(format("## Heading 2"), "## Heading 2\n");
        assert_eq!(format("###### Heading 6"), "###### Heading 6\n");
    }

    #[test]
    fn test_heading_and_paragraph() {
        let input = "# Title\n\nSome text.";
        let output = format(input);
        assert_eq!(output, "# Title\n\nSome text.\n");
    }

    #[test]
    fn test_multiple_paragraphs() {
        let input = "First paragraph.\n\nSecond paragraph.";
        let output = format(input);
        assert_eq!(output, "First paragraph.\n\nSecond paragraph.\n");
    }

    #[test]
    fn test_fenced_code_block() {
        let input = "```rust\nlet x = 1;\n```";
        let output = format(input);
        assert_eq!(output, "```rust\nlet x = 1;\n```\n");
    }

    #[test]
    fn test_code_block_no_lang() {
        let input = "```\ncode here\n```";
        let output = format(input);
        assert_eq!(output, "```\ncode here\n```\n");
    }

    #[test]
    fn test_unordered_list() {
        let input = "- Item 1\n- Item 2\n- Item 3";
        let output = format(input);
        assert_eq!(output, "- Item 1\n- Item 2\n- Item 3\n");
    }

    #[test]
    fn test_ordered_list() {
        let input = "1. First\n2. Second\n3. Third";
        let output = format(input);
        assert_eq!(output, "1. First\n2. Second\n3. Third\n");
    }

    #[test]
    fn test_ordered_list_all_ones_renumbered() {
        // "one" style (1. / 1. / 1.) is canonicalized to sequential.
        assert_formats_to(
            "1. First\n1. Second\n1. Third",
            "1. First\n2. Second\n3. Third\n",
        );
    }

    #[test]
    fn test_ordered_list_non_one_start_renumbered() {
        // Lists starting at a number other than 1 are renumbered from 1.
        assert_formats_to(
            "3. First\n5. Second\n9. Third",
            "1. First\n2. Second\n3. Third\n",
        );
    }

    #[test]
    fn test_bold_italic_inline() {
        assert_eq!(format("**bold** and *italic*"), "**bold** and *italic*\n");
    }

    #[test]
    fn test_inline_code() {
        assert_eq!(format("Use `foo()` here."), "Use `foo()` here.\n");
    }

    #[test]
    fn test_link() {
        let input = "[text](https://example.com)";
        let output = format(input);
        assert_eq!(output, "[text](https://example.com)\n");
    }

    #[test]
    fn test_image() {
        let input = "![alt text](image.png)";
        let output = format(input);
        assert_eq!(output, "![alt text](image.png)\n");
    }

    #[test]
    fn test_blank_line_between_heading_and_code() {
        let input = "# Heading\n\n```\ncode\n```";
        let output = format(input);
        assert_eq!(output, "# Heading\n\n```\ncode\n```\n");
    }

    #[test]
    fn test_blank_line_between_list_and_paragraph() {
        let input = "- item\n\nAfter list.";
        let output = format(input);
        assert_eq!(output, "- item\n\nAfter list.\n");
    }

    #[test]
    fn test_nested_list() {
        let input = "- Item 1\n  - Nested\n- Item 2";
        let output = format(input);
        assert_eq!(output, "- Item 1\n  - Nested\n- Item 2\n");
    }

    #[test]
    fn test_strikethrough() {
        assert_eq!(format("~~struck~~"), "~~struck~~\n");
    }

    // --- Canonicalization ---

    // Headings: setext → ATX (both levels)
    #[test]
    fn test_setext_headings_to_atx() {
        assert_formats_to("Heading 1\n=========", "# Heading 1\n");
        assert_formats_to("Heading 2\n---------", "## Heading 2\n");
    }

    // Heading with a hard line break in content: the `\` marker must not appear
    // unescaped in output (proptest regression: input "\\\r¡\r=").
    #[test]
    fn test_setext_heading_hard_break_not_leaked() {
        assert_formats_to("\\\r¡\r=", "# ¡\n");
    }

    // Headings: closed ATX → open ATX
    #[test]
    fn test_closed_atx_stripped() {
        assert_formats_to("## Heading ##", "## Heading\n");
        assert_formats_to("# Title #", "# Title\n");
    }

    // Headings: multiple spaces after `#` collapsed to one
    #[test]
    fn test_multiple_spaces_after_hash_collapsed() {
        assert_formats_to("#  Heading", "# Heading\n");
        assert_formats_to("##   Wide", "## Wide\n");
    }

    // Headings: a literal backslash in heading text stays escaped and idempotent.
    // Regression: `#\t0\\\ra` — the `\r` softens to a break, and the collapse of
    // break markers must not consume one of the doubled content backslashes.
    #[test]
    fn test_heading_literal_backslash_idempotent() {
        assert_formats_to("#\t0\\\ra", "# 0\\\\ a\n");
        assert_formats_to("# a\\b", "# a\\\\b\n");
    }

    // A genuine hard break inside a heading collapses to a single space with no
    // stray backslash left behind.
    #[test]
    fn test_heading_hard_break_collapses() {
        assert_formats_to("# a\\\nb", "# a\\\\\n\nb\n");
    }

    // `_`/`~` flanked by alphanumerics across a pulldown-cmark event split must
    // stay bare and idempotent.  Regression: `Ⓐ~A` splits into three Text events;
    // the lone `~` event has no in-event right neighbour, so without cross-event
    // lookahead it escaped on pass 1 then un-escaped on pass 2.
    #[test]
    fn test_intraword_tilde_across_event_split() {
        assert_formats_to("Ⓐ~A", "Ⓐ~A\n");
        // Not flanked on both sides → still escaped.
        assert_formats_to("Ⓐ~", "Ⓐ\\~\n");
        assert_formats_to("~A", "\\~A\n");
    }

    // A literal `<` in text must be escaped so adjacent characters can't
    // reconstruct an autolink or HTML tag on re-parse.  Regression: `<#\@a>`
    // dropped the backslash and re-parsed as an email autolink, changing meaning.
    // Genuine autolinks/HTML arrive as Link/Html events, not Text, so they are
    // unaffected.
    #[test]
    fn test_literal_angle_bracket_escaped() {
        assert_formats_to("<#\\@a>", "\\<#@a>\n");
        assert_formats_to("x<y", "x\\<y\n");
        // Real autolink and inline HTML are preserved, not escaped.
        assert_formats_to(
            "<https://example.com>",
            "[https://example.com](https://example.com)\n",
        );
        assert_formats_to("<div>hi</div>", "<div>hi</div>\n");
    }

    #[test]
    fn test_collapse_heading_breaks_unit() {
        // Soft break → space.
        assert_eq!(collapse_heading_breaks("a\nb"), "a b");
        // Hard-break marker (odd run before newline) dropped; newline → space.
        assert_eq!(collapse_heading_breaks("a\\\nb"), "a b");
        // Doubled content backslash (even run) before a soft break: keep both, then space.
        assert_eq!(collapse_heading_breaks("a\\\\\nb"), "a\\\\ b");
        // Literal backslash not before a newline is untouched.
        assert_eq!(collapse_heading_breaks("a\\\\b"), "a\\\\b");
    }

    // Blank lines: multiple consecutive blank lines collapsed to one
    #[test]
    fn test_multiple_blank_lines_collapsed() {
        assert_formats_to("First.\n\n\n\nSecond.", "First.\n\nSecond.\n");
    }

    // List markers: * and + → -
    #[test]
    fn test_list_markers_to_dash() {
        assert_formats_to("* Item 1\n* Item 2", "- Item 1\n- Item 2\n");
        assert_formats_to("+ Item 1\n+ Item 2", "- Item 1\n- Item 2\n");
    }

    // Emphasis: _ / __ → * / **
    #[test]
    fn test_emphasis_to_asterisk() {
        assert_formats_to("_italic_", "*italic*\n");
        assert_formats_to("__bold__", "**bold**\n");
    }

    // Code fences: ~~~ → ``` (with and without lang tag)
    #[test]
    fn test_tilde_fence_to_backtick() {
        assert_formats_to("~~~rust\ncode\n~~~", "```rust\ncode\n```\n");
        assert_formats_to("~~~\ncode\n~~~", "```\ncode\n```\n");
    }

    // Horizontal rules: all styles → ---
    #[test]
    fn test_all_hr_styles_to_dashes() {
        assert_formats_to("***", "---\n");
        assert_formats_to("___", "---\n");
        assert_formats_to("* * *", "---\n");
        assert_formats_to("- - -", "---\n");
        assert_formats_to("_ _ _", "---\n");
    }

    // Hard line breaks: trailing-space syntax → backslash continuation.
    // Two spaces before \n must become \\\n so trailing-whitespace stripping
    // doesn't silently drop the line break (CLAUDE.md lessons learned).
    #[test]
    fn test_hard_line_break_becomes_backslash() {
        assert_formats_to("foo  \nbar", "foo\\\nbar\n");
    }

    // Tables
    #[test]
    fn test_simple_table() {
        let input = "| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n";
        let output = format(input);
        assert_eq!(output, "| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n");
    }

    #[test]
    fn test_table_no_leading_pipes() {
        // GFM allows tables without leading/trailing pipes
        assert_formats_to(
            "A | B\n--- | ---\n1 | 2\n",
            "| A | B |\n| --- | --- |\n| 1 | 2 |\n",
        );
    }

    #[test]
    fn test_table_idempotent() {
        // Proptest uses random strings and is unlikely to generate valid table
        // syntax, so this structural idempotency check is worth keeping explicitly.
        let input = "| A | B |\n| --- | --- |\n| 1 | 2 |\n";
        let once = format(input);
        let twice = format(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn test_table_with_inline_formatting() {
        let input = "| **bold** | `code` |\n| --- | --- |\n| *em* | plain |\n";
        let output = format(input);
        assert_eq!(
            output,
            "| **bold** | `code` |\n| --- | --- |\n| *em* | plain |\n"
        );
    }

    #[test]
    fn test_table_followed_by_paragraph() {
        let input = "| A | B |\n| --- | --- |\n| 1 | 2 |\n\nSome text.\n";
        let output = format(input);
        assert_eq!(
            output,
            "| A | B |\n| --- | --- |\n| 1 | 2 |\n\nSome text.\n"
        );
    }

    // Structural escape: text that starts with a structural character must be
    // escaped so it is not re-interpreted on the next parse pass.
    #[test]
    fn test_escaped_list_marker_in_paragraph() {
        // \* in source resolves to literal *, which must not become a list item
        let once = format("\\*");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: escaped asterisk");
        // Similarly for - and +
        let once = format("\\-");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: escaped dash");
    }

    #[test]
    fn test_setext_heading_with_leading_vt() {
        // VT (U+000B) in setext heading body is preserved by pulldown-cmark, but
        // stripped from ATX heading content on re-parse — trim before emitting.
        let once = format("\u{b}¡\r=");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: setext heading with leading VT");
    }

    #[test]
    fn test_escaped_heading_in_paragraph() {
        // \# in source resolves to literal #, which must not become an ATX heading
        let once = format("\\# not a heading");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: escaped hash");
    }

    // Code blocks inside list items: fences and content must be indented to
    // keep the block inside the list item (3 spaces for `1. `, 2 for `- `).
    #[test]
    fn test_ordered_list_with_code_block() {
        let canonical = "1. **Enable rule:**\n\n   ```toml\n   enabled = false\n   ```\n\n2. **Another item:**\n\n   ```toml\n   line_length = 100\n   ```\n";
        // Starting with `1. / 1.` triggers MD029 renumbering in the formatter.
        assert_formats_to(
            "1. **Enable rule:**\n\n   ```toml\n   enabled = false\n   ```\n\n1. **Another item:**\n\n   ```toml\n   line_length = 100\n   ```\n",
            canonical,
        );
    }

    #[test]
    fn test_unordered_list_with_code_block() {
        let canonical = "- **Item:**\n\n  ```toml\n  enabled = false\n  ```\n";
        assert_formats_to(canonical, canonical);
    }

    #[test]
    fn test_tight_list_item_code_block_only() {
        // A list item whose sole content is a code block (no text paragraph).
        // The opening fence lands on the same line as the marker ("-   ```"),
        // making the effective list margin 4. Content and closing fence must
        // both use 4-space indent so the closing fence stays inside the item.
        let canonical = "-   ```\n    ¡\n    ```\n";
        assert_formats_to(canonical, canonical);
    }

    #[test]
    fn test_setext_underline_in_paragraph_continuation() {
        // "\t=" is stripped to "=" by pulldown-cmark; the bare "=" on a
        // continuation line must be escaped so "a\n=\n" is not re-parsed
        // as a setext h1 heading on the next format pass.
        let once = format("a\r\t=");
        let twice = format(&once);
        assert_eq!(
            once, twice,
            "idempotency: setext-underline-like continuation"
        );
        // Same for "--" which is a valid setext h2 underline.
        let once = format("a\r\t--");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: setext h2 continuation");
    }

    #[test]
    fn test_backtick_in_text_escaped() {
        // A lone backtick in paragraph text must be escaped so it cannot pair
        // with another backtick on re-parse and form an unintended code span.
        let once = format("\\`\r`");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: lone backticks in text");
    }

    #[test]
    fn test_empty_list_items_idempotent() {
        // Two consecutive empty tight items (from "*\r*\t" = two asterisk markers
        // with no content): the old code omitted the newline after each empty item's
        // marker, causing the markers to merge onto one line ("- -") which re-parsed
        // as a nested list on the next pass.
        let once = format("*\r*\t");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: empty tight list items");
    }

    #[test]
    fn test_html_block_with_cr_content_idempotent() {
        // pulldown-cmark splits "<?>\r\" into two Html events within the same
        // HtmlBlock: Html("<?>") and Html("\").  The old Html handler set
        // needs_blank = true after the first event, inserting a spurious blank
        // line before the second, so the second format pass saw "\" as a
        // separate paragraph and escaped it to "\\".
        let once = format("<?>\r\\");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: HTML block with CR content");
    }

    #[test]
    fn test_list_marker_with_trailing_unicode_whitespace_idempotent() {
        // "*\u{85}\u{b}" is paragraph text (NEL and VT are not CommonMark line
        // endings, so `*` has no space after it and is not a list item). But
        // finish() strips trailing Unicode whitespace via trim_end(), leaving
        // bare "*\n" which re-parses as an empty list item on the next pass.
        // The fix: needs_line_escape checks against the trimmed form of the line.
        assert_formats_to("*\u{85}\u{b}", "\\*\n");
    }

    #[test]
    fn test_blockquote_nel_idempotent() {
        // ">\u{85}": cmark emits BlockQuote > Paragraph > Text("\u{85}") — NEL is
        // Unicode whitespace that finish() strips, leaving ">" which re-parses as
        // an empty blockquote → second pass returns "".
        let once = format(">\u{85}");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: blockquote + NEL");
    }

    #[test]
    fn test_trailing_backslash_in_paragraph_not_doubled() {
        // Proptest regression: "¡\\\t\r\u{b}" — pulldown emits Text("¡\") + SoftBreak.
        // on_text doubles the \ to \\; SoftBreak appends \n → inline = "¡\\\n".
        // The trailing-hard-break strip must not fire on an even backslash run (\\
        // = one literal \), only on an odd run (the extra \ is the break marker).
        assert_formats_to("¡\\\t\r\x0B", "¡\\\\\n");
    }

    #[test]
    fn test_hard_break_followed_by_vt_in_paragraph() {
        // "\\\r\u{b}\r¡": cmark emits HardBreak + SoftBreak (VT stripped) + Text("¡").
        // The two consecutive breaks produce an empty continuation slot when split on
        // '\n', which emits a blank line that breaks the paragraph on re-parse — the
        // preceding `\` is then doubled by on_text on the second pass.
        let once = format("\\\r\u{b}\r¡");
        let twice = format(&once);
        assert_eq!(once, twice, "idempotency: hard-break + VT continuation");
    }

    #[test]
    fn test_code_fence_info_backslash_idempotent() {
        // pulldown-cmark returns the unescaped info string for fenced code blocks.
        // Emitting it verbatim means "\!" round-trips to "!" (pulldown-cmark
        // treats "\!" as a backslash escape of "!" on the next parse).
        // The fix escapes "\" to "\\" in the info string so the round-trip is stable.
        let once = format("```\\\r!");
        let twice = format(&once);
        assert_eq!(
            once, twice,
            "idempotency: code fence info string with backslash"
        );
    }
}
