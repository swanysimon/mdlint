use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashSet;
use std::ops::Range;

pub struct MarkdownParser<'a> {
    content: &'a str,
    lines: Vec<&'a str>,
    /// Byte offset of the start of each line (0-indexed).
    /// Enables O(log n) offset → (line, column) lookup via binary search.
    line_offsets: Vec<usize>,
    /// Lines (1-indexed) that fall inside a fenced/indented code block.
    code_block_lines: HashSet<usize>,
    /// Lines (1-indexed) inside any code (blocks + inline spans).
    code_lines: HashSet<usize>,
    /// Byte ranges of all code blocks and inline code spans.
    code_ranges: Vec<Range<usize>>,
}

impl<'a> MarkdownParser<'a> {
    pub fn new(content: &'a str) -> Self {
        let lines: Vec<&'a str> = content.lines().collect();
        let line_offsets = build_line_offsets(content);
        let (code_block_lines, code_lines, code_ranges) = build_code_info(content, &line_offsets);
        Self {
            content,
            lines,
            line_offsets,
            code_block_lines,
            code_lines,
            code_ranges,
        }
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
        Parser::new_ext(self.content, mk_options())
    }

    pub fn parse_with_offsets(&self) -> impl Iterator<Item = (Event<'a>, Range<usize>)> {
        Parser::new_ext(self.content, mk_options()).into_offset_iter()
    }

    pub fn offset_to_line(&self, offset: usize) -> usize {
        self.offset_to_position(offset).0
    }

    pub fn offset_to_position(&self, offset: usize) -> (usize, usize) {
        // partition_point returns the count of elements for which the predicate holds —
        // i.e. the index of the first line whose start offset exceeds `offset`.
        let i = self.line_offsets.partition_point(|&start| start <= offset);
        if i == 0 {
            return (1, 1);
        }
        let line_idx = i - 1; // 0-indexed
        let column = offset - self.line_offsets[line_idx] + 1;
        (line_idx + 1, column) // 1-indexed
    }

    /// Returns the 1-indexed line numbers inside code blocks or inline code.
    /// Result is precomputed in `new()` — O(1) to access.
    pub fn get_code_line_numbers(&self) -> &HashSet<usize> {
        &self.code_lines
    }

    /// Returns the 1-indexed line numbers inside code blocks only (not inline spans).
    /// Result is precomputed in `new()` — O(1) to access.
    pub fn get_code_block_line_numbers(&self) -> &HashSet<usize> {
        &self.code_block_lines
    }

    /// Returns byte ranges (into the original content) for all code blocks and
    /// inline code spans. Result is precomputed in `new()` — O(1) to access.
    pub fn get_code_ranges(&self) -> &[Range<usize>] {
        &self.code_ranges
    }

    /// Converts a (1-indexed) line number and 0-indexed byte offset within that
    /// line to an absolute byte offset in the content.
    pub fn line_offset_to_absolute(&self, line_num: usize, byte_offset_in_line: usize) -> usize {
        if line_num == 0 || line_num > self.line_offsets.len() {
            return self.content.len();
        }
        self.line_offsets[line_num - 1] + byte_offset_in_line
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

fn mk_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options
}

/// Builds a table of byte offsets for the start of each line (entry `i` = byte
/// offset where line `i+1` begins).  Handles both LF and CRLF correctly because
/// it scans the raw bytes rather than relying on `str::lines` lengths.
fn build_line_offsets(content: &str) -> Vec<usize> {
    let mut offsets = vec![0usize];
    for (i, byte) in content.bytes().enumerate() {
        if byte == b'\n' {
            let next = i + 1;
            if next < content.len() {
                offsets.push(next);
            }
        }
    }
    offsets
}

/// Map a byte offset to a 1-indexed line number using the precomputed offset
/// table.  O(log n) via binary search.
fn line_from_offset(offset: usize, line_offsets: &[usize]) -> usize {
    let i = line_offsets.partition_point(|&start| start <= offset);
    i.max(1)
}

/// Single parse pass that builds all three code-location caches simultaneously.
/// Called once in `MarkdownParser::new()`.
fn build_code_info(
    content: &str,
    line_offsets: &[usize],
) -> (HashSet<usize>, HashSet<usize>, Vec<Range<usize>>) {
    let mut code_block_lines: HashSet<usize> = HashSet::new();
    let mut code_lines: HashSet<usize> = HashSet::new();
    let mut code_ranges: Vec<Range<usize>> = Vec::new();

    let mut in_code_block = false;
    let mut code_block_start = 0usize;

    for (event, range) in Parser::new_ext(content, mk_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                code_block_start = range.start;
                let start_line = line_from_offset(range.start, line_offsets);
                let end_line = line_from_offset(range.end, line_offsets);
                for line in start_line..=end_line {
                    code_block_lines.insert(line);
                    code_lines.insert(line);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if in_code_block {
                    code_ranges.push(code_block_start..range.end);
                    in_code_block = false;
                }
            }
            Event::Code(_) => {
                // Inline code span
                code_ranges.push(range.clone());
                let start_line = line_from_offset(range.start, line_offsets);
                let end_line = line_from_offset(range.end, line_offsets);
                for line in start_line..=end_line {
                    code_lines.insert(line);
                }
            }
            _ => {
                if in_code_block {
                    let start_line = line_from_offset(range.start, line_offsets);
                    let end_line = line_from_offset(range.end, line_offsets);
                    for line in start_line..=end_line {
                        code_block_lines.insert(line);
                        code_lines.insert(line);
                    }
                }
            }
        }
    }

    (code_block_lines, code_lines, code_ranges)
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

    #[test]
    fn test_build_line_offsets() {
        // LF line endings
        let offsets = build_line_offsets("abc\ndef\nghi");
        assert_eq!(offsets, vec![0, 4, 8]);

        // CRLF line endings
        let offsets = build_line_offsets("abc\r\ndef\r\nghi");
        assert_eq!(offsets, vec![0, 5, 10]);

        // Single line (no newline)
        let offsets = build_line_offsets("abc");
        assert_eq!(offsets, vec![0]);

        // Empty content
        let offsets = build_line_offsets("");
        assert_eq!(offsets, vec![0]);

        // Trailing newline does not add a spurious extra entry
        let offsets = build_line_offsets("abc\n");
        assert_eq!(offsets, vec![0]);
    }

    #[test]
    fn test_offset_to_position_crlf() {
        // CRLF: "abc\r\ndef" — 'a'=0,'b'=1,'c'=2,'\r'=3,'\n'=4,'d'=5,'e'=6,'f'=7
        let content = "abc\r\ndef";
        let parser = MarkdownParser::new(content);
        assert_eq!(parser.offset_to_position(0), (1, 1));
        assert_eq!(parser.offset_to_position(2), (1, 3));
        assert_eq!(parser.offset_to_position(5), (2, 1));
        assert_eq!(parser.offset_to_position(7), (2, 3));
    }
}
