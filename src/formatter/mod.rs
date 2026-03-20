use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Format a Markdown document to canonical style.
///
/// Returns the formatted document as a String. The output:
/// - Always ends with exactly one trailing newline (or is empty for empty input)
/// - Has exactly one blank line between top-level block elements
/// - Uses ATX-style headings
/// - Uses `-` for unordered list markers
/// - Uses backtick fences for code blocks
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

    for (event, next_is_ul) in events.into_iter().zip(lookahead) {
        state.next_is_unordered_list = next_is_ul;
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
                self.emit_blank_if_needed();
                self.out.push_str(&h);
                // HTML blocks may or may not end with \n; normalise.
                if !self.out.ends_with('\n') {
                    self.out.push('\n');
                }
                self.needs_blank = true;
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
                self.inline.push_str(&format!("[^{}]", label));
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
                    CodeBlockKind::Fenced(lang) => lang.into_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                let indent = self.list_continuation_prefix();
                self.code_block_indent = indent.clone();
                self.write_bq_prefix();
                self.out.push_str(&indent);
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
                        let s = format!("{}{}. ", indent, n);
                        *n += 1;
                        s
                    }
                    _ => format!("{}- ", indent),
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
            Tag::BlockQuote(_) => {
                self.emit_blank_if_needed();
                self.bq_depth += 1;
            }
            Tag::FootnoteDefinition(label) => {
                self.emit_blank_if_needed();
                // Write the label prefix; body will be flushed inline.
                self.write_bq_prefix();
                self.out.push_str(&format!("[^{}]: ", label));
            }
            Tag::Table(alignments) => {
                self.emit_blank_if_needed();
                self.table_alignments = alignments.to_vec();
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
            Tag::TableCell => {
                // inline content accumulates in self.inline; flushed at End(TableCell)
            }
            _ => {}
        }
    }

    fn on_end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                let text = std::mem::take(&mut self.inline);
                if self.list_depth == 0 {
                    self.write_bq_prefix();
                }
                let prefix = "  ".repeat(self.list_depth);
                self.flush_inline_text(&text, &prefix);
                self.needs_blank = true;
                self.in_tight_item = false;
            }
            TagEnd::Heading(level) => {
                let text = std::mem::take(&mut self.inline);
                let hashes = "#".repeat(level as usize);
                self.write_bq_prefix();
                // Trim: pulldown-cmark strips leading/trailing whitespace (incl. VT U+000B)
                // on re-parse; collapse soft-break newlines to spaces for single-line ATX.
                let heading_text = text.trim().replace('\n', " ");
                self.out.push_str(&format!("{} {}\n", hashes, heading_text));
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
            TagEnd::Item => {
                // Tight list item: the content was never wrapped in Paragraph.
                if self.in_tight_item {
                    let text = std::mem::take(&mut self.inline);
                    if !text.is_empty() {
                        let prefix = "  ".repeat(self.list_depth);
                        self.flush_inline_text(&text, &prefix);
                    }
                    self.in_tight_item = false;
                }
            }
            TagEnd::Emphasis => self.inline.push('*'),
            TagEnd::Strong => self.inline.push_str("**"),
            TagEnd::Strikethrough => self.inline.push_str("~~"),
            TagEnd::Link => {
                if let Some((dest, title)) = self.link_stack.pop() {
                    if title.is_empty() {
                        self.inline.push_str(&format!("]({})", dest));
                    } else {
                        self.inline.push_str(&format!("]({} \"{}\")", dest, title));
                    }
                }
            }
            TagEnd::Image => {
                if let Some((dest, title)) = self.link_stack.pop() {
                    if title.is_empty() {
                        self.inline.push_str(&format!("]({})", dest));
                    } else {
                        self.inline.push_str(&format!("]({} \"{}\")", dest, title));
                    }
                }
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
            if self.code_block_indent.is_empty() {
                self.out.push_str(text);
            } else {
                for line in text.split_inclusive('\n') {
                    self.out.push_str(&self.code_block_indent);
                    self.out.push_str(line);
                }
            }
        } else {
            // pulldown-cmark resolves `\\` → `\`; re-double to survive the next parse.
            self.inline.push_str(&text.replace('\\', "\\\\"));
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
        let bq = "> ".repeat(self.bq_depth);
        let mut lines = text.split('\n').peekable();

        if let Some(first) = lines.next() {
            if self.bq_depth > 0 && (self.out.ends_with('\n') || self.out.is_empty()) {
                self.out.push_str(&bq);
            }
            if needs_line_escape(first, false) {
                self.out.push('\\');
            }
            self.out.push_str(first);
            self.out.push('\n');
        }

        while let Some(line) = lines.next() {
            if lines.peek().is_none() && line.is_empty() {
                // Trailing empty string from split: don't emit an extra newline.
                break;
            }
            self.out.push_str(continuation_prefix);
            self.out.push_str(&bq);
            if needs_line_escape(line, true) {
                self.out.push('\\');
            }
            self.out.push_str(line);
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
        let joined = result.join("\n");
        let trimmed = joined.trim_end_matches('\n');
        if trimmed.is_empty() {
            return String::new();
        }
        format!("{}\n", trimmed)
    }
}

/// Returns true if `line` starts with a sequence that would be re-interpreted
/// as a structural Markdown block element on re-parse, and therefore needs a
/// leading `\` escape.  This matters for first lines of paragraphs and for
/// soft-break continuation lines that are emitted as separate output lines.
///
/// When `is_continuation` is true, the line is a soft-break continuation
/// inside a paragraph.  In CommonMark only `1.` / `1)` can interrupt a
/// paragraph, so other ordered-list markers (2., 6., etc.) must NOT be
/// escaped — escaping them hides real formatting problems from the linter.
fn needs_line_escape(line: &str, is_continuation: bool) -> bool {
    if line.is_empty() {
        return false;
    }

    // Blockquote marker
    if line.starts_with('>') {
        return true;
    }

    // Unordered list marker: *, -, or + followed by space/tab or end of line.
    // Also catches thematic breaks that start with * or - (e.g. `* * *`, `- - -`).
    if let Some(rest) = line.strip_prefix(['*', '-', '+'])
        && (rest.is_empty() || rest.starts_with([' ', '\t']))
    {
        return true;
    }

    // Thematic break: three or more of the same char (-, *, _) with optional spaces.
    // Catches `---`, `___`, `* * *`, etc.  The * and - cases with trailing space are
    // already caught above; this covers `---` and `___` and variants without spaces.
    let first = line.chars().next().unwrap();
    if matches!(first, '-' | '*' | '_') {
        let all_valid = line.chars().all(|c| c == first || c == ' ' || c == '\t');
        let count = line.chars().filter(|&c| c == first).count();
        if all_valid && count >= 3 {
            return true;
        }
    }

    let after_hashes = line.trim_start_matches('#');
    if after_hashes.len() < line.len() && (after_hashes.is_empty() || after_hashes.starts_with(' '))
    {
        return true;
    }

    // Ordered list marker: one or more ASCII digits followed by . or ) and then space/tab/end.
    // On continuation lines only `1.` / `1)` can interrupt a paragraph (CommonMark spec §5.2),
    // so we must not escape other numbers — doing so hides broken-list errors from the linter.
    let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        let rest = &line[digits.len()..];
        if let Some(after_marker) = rest.strip_prefix(['.', ')'])
            && (after_marker.is_empty() || after_marker.starts_with([' ', '\t']))
            && (!is_continuation || digits == "1")
        {
            return true;
        }
    }

    // HTML block openers:
    // Type 2: <!--
    // Type 3: <?
    // Type 4: <! followed by an ASCII uppercase letter
    // Type 5: <![CDATA[
    if line.starts_with("<!--") || line.starts_with("<?") || line.starts_with("<![CDATA[") {
        return true;
    }
    if let Some(rest) = line.strip_prefix("<!")
        && rest.starts_with(|c: char| c.is_ascii_uppercase())
    {
        return true;
    }
    // Type 1: <script, <pre, <style, <textarea (case-insensitive) + whitespace / > / end
    let lower: String = line
        .chars()
        .take(12)
        .collect::<String>()
        .to_ascii_lowercase();
    for tag in &["<script", "<pre", "<style", "<textarea"] {
        if let Some(rest) = lower.strip_prefix(tag)
            && (rest.is_empty() || rest.starts_with([' ', '\t', '>']))
        {
            return true;
        }
    }

    false
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
}
