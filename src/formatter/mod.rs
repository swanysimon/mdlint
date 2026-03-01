use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

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

    for event in events {
        state.process(event);
    }

    state.finish()
}

fn mk_options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    opts
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

    // Link/image stack: stores (dest_url, title) from Start until End.
    link_stack: Vec<(String, String)>,
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
            link_stack: Vec::new(),
        }
    }

    fn process(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.on_start(tag),
            Event::End(tag) => self.on_end(tag),
            Event::Text(t) => self.on_text(&t),
            Event::Code(c) => {
                // Inline code: choose delimiter based on content
                let delim = if c.contains('`') { "``" } else { "`" };
                self.inline.push_str(delim);
                if c.starts_with('`') || c.ends_with('`') {
                    self.inline.push(' ');
                }
                self.inline.push_str(&c);
                if c.starts_with('`') || c.ends_with('`') {
                    self.inline.push(' ');
                }
                self.inline.push_str(delim);
            }
            Event::Html(h) => {
                // Raw HTML block: emit verbatim.
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
                // Two trailing spaces + newline = hard break in Markdown.
                self.inline.push_str("  \n");
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
                // Emitted just before the list item text.
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
                self.write_bq_prefix();
                self.out.push_str("```");
                self.out.push_str(&lang);
                self.out.push('\n');
                self.in_code_block = true;
            }
            Tag::List(start) => {
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
                        self.flush_inline_text(&text, false);
                        self.in_tight_item = false;
                    }
                }
                self.list_depth += 1;
                self.list_starts.push(start);
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
            // Tables: pass through as-is for now (TODO)
            Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
            _ => {}
        }
    }

    fn on_end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                let text = std::mem::take(&mut self.inline);
                self.flush_inline_text(&text, false);
                self.needs_blank = true;
                self.in_tight_item = false;
            }
            TagEnd::Heading(level) => {
                let text = std::mem::take(&mut self.inline);
                let hashes = "#".repeat(heading_to_u8(level) as usize);
                self.write_bq_prefix();
                self.out.push_str(&format!("{} {}\n", hashes, text));
                self.needs_blank = true;
            }
            TagEnd::CodeBlock => {
                self.write_bq_prefix();
                self.out.push_str("```\n");
                self.in_code_block = false;
                self.needs_blank = true;
            }
            TagEnd::List(_) => {
                self.list_depth -= 1;
                self.list_starts.pop();
                if self.list_depth == 0 {
                    self.needs_blank = true;
                }
            }
            TagEnd::Item => {
                // Tight list item: the content was never wrapped in Paragraph.
                if self.in_tight_item {
                    let text = std::mem::take(&mut self.inline);
                    if !text.is_empty() {
                        self.flush_inline_text(&text, false);
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
                self.flush_inline_text(&text, false);
                self.needs_blank = true;
            }
            // Tables: pass through (TODO)
            TagEnd::Table | TagEnd::TableHead | TagEnd::TableRow | TagEnd::TableCell => {}
            _ => {}
        }
    }

    fn on_text(&mut self, text: &str) {
        if self.in_code_block {
            // Code block content goes directly to output verbatim.
            self.out.push_str(text);
        } else {
            self.inline.push_str(text);
        }
    }

    fn emit_blank_if_needed(&mut self) {
        if self.needs_blank {
            self.out.push('\n');
            self.needs_blank = false;
        }
    }

    fn write_bq_prefix(&mut self) {
        for _ in 0..self.bq_depth {
            self.out.push_str("> ");
        }
    }

    /// Flush inline text to output.
    /// Each line in `text` gets the blockquote prefix prepended (except the first,
    /// which follows whatever was already written on the current output line).
    fn flush_inline_text(&mut self, text: &str, _continuation_indent: bool) {
        let bq = "> ".repeat(self.bq_depth);
        let mut lines = text.split('\n').peekable();

        if let Some(first) = lines.next() {
            self.out.push_str(first);
            self.out.push('\n');
        }

        while let Some(line) = lines.next() {
            if lines.peek().is_none() && line.is_empty() {
                // Trailing empty string from split: don't emit an extra newline.
                break;
            }
            self.out.push_str(&bq);
            self.out.push_str(line);
            self.out.push('\n');
        }
    }

    fn finish(mut self) -> String {
        // Normalise to exactly one trailing newline.
        let s = std::mem::take(&mut self.out);
        let trimmed = s.trim_end_matches('\n');
        if trimmed.is_empty() {
            return String::new();
        }
        format!("{}\n", trimmed)
    }
}

fn heading_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_horizontal_rule() {
        assert_eq!(format("---"), "---\n");
        assert_eq!(format("***"), "---\n");
        assert_eq!(format("___"), "---\n");
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
    fn test_trailing_newline_normalised() {
        // Input with multiple trailing newlines → exactly one in output
        assert_eq!(format("text\n\n\n"), "text\n");
        // Input with no trailing newline → one added
        assert_eq!(format("text"), "text\n");
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
}
