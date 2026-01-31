use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashSet;
use std::ops::Range;

pub struct MarkdownParser<'a> {
    content: &'a str,
    lines: Vec<&'a str>,
}

impl<'a> MarkdownParser<'a> {
    pub fn new(content: &'a str) -> Self {
        let lines = content.lines().collect();
        Self { content, lines }
    }

    pub fn content(&self) -> &'a str {
        self.content
    }

    pub fn lines(&self) -> &[&'a str] {
        &self.lines
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn get_line(&self, line_num: usize) -> Option<&'a str> {
        if line_num > 0 && line_num <= self.lines.len() {
            Some(self.lines[line_num - 1])
        } else {
            None
        }
    }

    pub fn parse(&self) -> impl Iterator<Item = Event<'a>> + 'a {
        Parser::new_ext(self.content, Self::options())
    }

    pub fn parse_with_offsets(&self) -> impl Iterator<Item = (Event<'a>, Range<usize>)> {
        Parser::new_ext(self.content, Self::options()).into_offset_iter()
    }

    fn options() -> Options {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
        options
    }

    pub fn offset_to_line(&self, offset: usize) -> usize {
        self.offset_to_position(offset).0
    }

    pub fn offset_to_position(&self, offset: usize) -> (usize, usize) {
        let mut current_offset = 0;
        for (line_num, line) in self.lines.iter().enumerate() {
            let line_len = line.len() + 1;
            if offset < current_offset + line_len {
                let column = offset - current_offset + 1;
                return (line_num + 1, column);
            }
            current_offset += line_len;
        }
        (self.lines.len(), 1)
    }

    /// Returns a set of line numbers that are inside code blocks or inline code.
    /// This is useful for rules that should ignore code content.
    ///
    /// Note: For inline code, this marks the entire line as code. For more precise
    /// detection, use `get_code_ranges()` instead.
    pub fn get_code_line_numbers(&self) -> HashSet<usize> {
        let mut code_lines = HashSet::new();
        let mut in_code_block = false;

        for (event, range) in self.parse_with_offsets() {
            match event {
                Event::Start(Tag::CodeBlock(_)) => {
                    in_code_block = true;
                    // Add all lines in this code block
                    let start_line = self.offset_to_line(range.start);
                    let end_line = self.offset_to_line(range.end);
                    for line in start_line..=end_line {
                        code_lines.insert(line);
                    }
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                }
                Event::Code(_) => {
                    // For inline code, we mark the whole line as code
                    // This is conservative but simpler than tracking ranges
                    let start_line = self.offset_to_line(range.start);
                    let end_line = self.offset_to_line(range.end);
                    for line in start_line..=end_line {
                        code_lines.insert(line);
                    }
                }
                _ => {
                    // If we're in a code block, mark these lines too
                    if in_code_block {
                        let start_line = self.offset_to_line(range.start);
                        let end_line = self.offset_to_line(range.end);
                        for line in start_line..=end_line {
                            code_lines.insert(line);
                        }
                    }
                }
            }
        }

        code_lines
    }

    /// Returns a set of line numbers that are inside code BLOCKS only (not inline code).
    /// This is useful for rules that need to check list markers, URLs, etc. that might
    /// legitimately appear on lines with inline code.
    /// Lines are 1-indexed to match violation reporting.
    pub fn get_code_block_line_numbers(&self) -> HashSet<usize> {
        let mut code_lines = HashSet::new();
        let mut in_code_block = false;

        for (event, range) in self.parse_with_offsets() {
            match event {
                Event::Start(Tag::CodeBlock(_)) => {
                    in_code_block = true;
                    // Add all lines in this code block
                    let start_line = self.offset_to_line(range.start);
                    let end_line = self.offset_to_line(range.end);
                    for line in start_line..=end_line {
                        code_lines.insert(line);
                    }
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                }
                _ => {
                    // If we're in a code block, mark these lines too
                    if in_code_block {
                        let start_line = self.offset_to_line(range.start);
                        let end_line = self.offset_to_line(range.end);
                        for line in start_line..=end_line {
                            code_lines.insert(line);
                        }
                    }
                }
            }
        }

        code_lines
    }

    /// Returns a vector of byte ranges that are inside code (blocks or inline).
    /// This is more precise than `get_code_line_numbers()` for inline code.
    pub fn get_code_ranges(&self) -> Vec<Range<usize>> {
        let mut code_ranges = Vec::new();
        let mut in_code_block = false;
        let mut code_block_start = 0;

        for (event, range) in self.parse_with_offsets() {
            match event {
                Event::Start(Tag::CodeBlock(_)) => {
                    in_code_block = true;
                    code_block_start = range.start;
                }
                Event::End(TagEnd::CodeBlock) => {
                    if in_code_block {
                        code_ranges.push(code_block_start..range.end);
                        in_code_block = false;
                    }
                }
                Event::Code(_) => {
                    // Inline code span - add its byte range
                    code_ranges.push(range);
                }
                _ => {}
            }
        }

        code_ranges
    }

    /// Converts a byte offset within a line to an absolute byte offset in the content.
    /// line_num is 1-indexed, byte_offset_in_line is 0-indexed from start of line.
    pub fn line_offset_to_absolute(&self, line_num: usize, byte_offset_in_line: usize) -> usize {
        let mut current_offset = 0;
        for (i, line) in self.lines.iter().enumerate() {
            if i + 1 == line_num {
                return current_offset + byte_offset_in_line;
            }
            current_offset += line.len() + 1; // +1 for newline
        }
        current_offset
    }

    pub fn is_heading(&self, event: &Event) -> bool {
        matches!(event, Event::Start(Tag::Heading { .. }))
    }

    pub fn is_code_block(&self, event: &Event) -> bool {
        matches!(event, Event::Start(Tag::CodeBlock(_)))
    }

    pub fn is_list(&self, event: &Event) -> bool {
        matches!(event, Event::Start(Tag::List(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_parsing() {
        let content = "# Heading\n\nSome **bold** text.";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.content(), content);
        assert_eq!(parser.line_count(), 3);
    }

    #[test]
    fn test_get_line() {
        let content = "Line 1\nLine 2\nLine 3";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.get_line(1), Some("Line 1"));
        assert_eq!(parser.get_line(2), Some("Line 2"));
        assert_eq!(parser.get_line(3), Some("Line 3"));
        assert_eq!(parser.get_line(0), None);
        assert_eq!(parser.get_line(4), None);
    }

    #[test]
    fn test_offset_to_line() {
        let content = "Line 1\nLine 2\nLine 3";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.offset_to_line(0), 1);
        assert_eq!(parser.offset_to_line(3), 1);
        assert_eq!(parser.offset_to_line(7), 2);
        assert_eq!(parser.offset_to_line(14), 3);
    }

    #[test]
    fn test_offset_to_position() {
        let content = "Line 1\nLine 2\nLine 3";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.offset_to_position(0), (1, 1));
        assert_eq!(parser.offset_to_position(3), (1, 4));
        assert_eq!(parser.offset_to_position(7), (2, 1));
    }

    #[test]
    fn test_parse_events() {
        let content = "# Heading";
        let parser = MarkdownParser::new(content);

        let events: Vec<_> = parser.parse().collect();
        assert!(!events.is_empty());
        assert!(parser.is_heading(&events[0]));
    }

    #[test]
    fn test_parse_with_offsets() {
        let content = "# Heading\n\nParagraph";
        let parser = MarkdownParser::new(content);

        let events: Vec<_> = parser.parse_with_offsets().collect();
        assert!(!events.is_empty());
    }

    #[test]
    fn test_event_type_checks() {
        let content = "# Heading\n\n```rust\ncode\n```\n\n- item";
        let parser = MarkdownParser::new(content);

        let events: Vec<_> = parser.parse().collect();

        let has_heading = events.iter().any(|e| parser.is_heading(e));
        let has_code = events.iter().any(|e| parser.is_code_block(e));
        let has_list = events.iter().any(|e| parser.is_list(e));

        assert!(has_heading);
        assert!(has_code);
        assert!(has_list);
    }

    #[test]
    fn test_code_line_numbers_fenced() {
        let content = "Normal text\n\n```sql\nSELECT * FROM table_name\nWHERE user_id = 123\n```\n\nMore text";
        let parser = MarkdownParser::new(content);
        let code_lines = parser.get_code_line_numbers();

        // Lines 3-6 should be marked as code (the ``` markers and content)
        assert!(
            code_lines.contains(&3),
            "Line 3 (opening ```) should be code"
        );
        assert!(
            code_lines.contains(&4),
            "Line 4 (code content) should be code"
        );
        assert!(
            code_lines.contains(&5),
            "Line 5 (code content) should be code"
        );
        assert!(
            code_lines.contains(&6),
            "Line 6 (closing ```) should be code"
        );

        // Other lines should not be marked
        assert!(!code_lines.contains(&1), "Line 1 should not be code");
        assert!(!code_lines.contains(&2), "Line 2 should not be code");
        assert!(!code_lines.contains(&8), "Line 8 should not be code");
    }

    #[test]
    fn test_code_line_numbers_inline() {
        let content = "This is `inline_code_with_underscores` in text";
        let parser = MarkdownParser::new(content);
        let code_lines = parser.get_code_line_numbers();

        // Line 1 should be marked because it contains inline code
        assert!(
            code_lines.contains(&1),
            "Line with inline code should be marked"
        );
    }

    #[test]
    fn test_code_line_numbers_mixed() {
        let content =
            "Normal text\n\nText with `inline_code` here\n\n```\nCode block\n```\n\nFinal text";
        let parser = MarkdownParser::new(content);
        let code_lines = parser.get_code_line_numbers();

        // Line 3 has inline code
        assert!(
            code_lines.contains(&3),
            "Line with inline code should be marked"
        );

        // Lines 5-7 are in code block
        assert!(code_lines.contains(&5), "Code block line should be marked");
        assert!(code_lines.contains(&6), "Code block line should be marked");
        assert!(code_lines.contains(&7), "Code block line should be marked");

        // Lines 1, 2, 9 are normal text
        assert!(
            !code_lines.contains(&1),
            "Normal text line should not be marked"
        );
        assert!(!code_lines.contains(&2), "Empty line should not be marked");
        assert!(
            !code_lines.contains(&9),
            "Normal text line should not be marked"
        );
    }
}
