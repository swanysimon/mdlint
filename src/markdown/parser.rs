use crate::markdown::front_matter::{detect_front_matter, extract_title};
use pulldown_cmark::{BrokenLink, CowStr, Event, Options, Parser, Tag, TagEnd};
use std::collections::{HashMap, HashSet};
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
    /// Lines (1-indexed) that are part of a link reference definition (`[label]: url`).
    ref_def_lines: HashSet<usize>,
    /// Map from normalised (lowercase) label to its 1-indexed line number.
    ref_defs: HashMap<String, usize>,
    /// Lines (1-indexed) that are part of the YAML/TOML front matter block.
    front_matter_lines: HashSet<usize>,
    /// The front matter `title` field's value and 1-indexed line, if present.
    front_matter_title: Option<(String, usize)>,
    /// Lines (1-indexed) that fall entirely inside an HTML comment (`<!-- ... -->`).
    comment_lines: HashSet<usize>,
}

impl<'a> MarkdownParser<'a> {
    #[must_use]
    pub fn new(content: &'a str) -> Self {
        let lines: Vec<&'a str> = content.lines().collect();
        let line_offsets = build_line_offsets(content);
        let (code_block_lines, code_lines, code_ranges) = build_code_info(content, &line_offsets);
        let (ref_def_lines, ref_defs) = build_ref_def_info(content, &line_offsets);
        let front_matter = detect_front_matter(content);
        let front_matter_lines = front_matter
            .as_ref()
            .map(|fm| (1..=fm.end_line).collect())
            .unwrap_or_default();
        let front_matter_title = front_matter.as_ref().and_then(extract_title);
        let comment_lines = build_comment_lines(content, &code_block_lines);
        Self {
            content,
            lines,
            line_offsets,
            code_block_lines,
            code_lines,
            code_ranges,
            ref_def_lines,
            ref_defs,
            front_matter_lines,
            front_matter_title,
            comment_lines,
        }
    }

    #[must_use]
    pub fn content(&self) -> &'a str {
        self.content
    }

    #[must_use]
    pub fn lines(&self) -> &[&'a str] {
        &self.lines
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    #[must_use]
    pub fn get_line(&self, line_num: usize) -> Option<&'a str> {
        if line_num > 0 && line_num <= self.lines.len() {
            self.lines.get(line_num - 1).copied()
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

    /// Like `parse_with_offsets`, but resolves otherwise-broken reference links
    /// (undefined labels) by flagging their `LinkType` as the corresponding
    /// `*Unknown` variant instead of silently dropping the event as plain text.
    /// Used by rules that need to detect undefined reference links/images.
    pub fn parse_with_broken_links(&self) -> impl Iterator<Item = (Event<'a>, Range<usize>)> + 'a {
        Parser::new_with_broken_link_callback(
            self.content,
            mk_options(),
            Some(|_broken: BrokenLink| Some((CowStr::from(""), CowStr::from("")))),
        )
        .into_offset_iter()
    }

    #[must_use]
    pub fn offset_to_line(&self, offset: usize) -> usize {
        self.offset_to_position(offset).0
    }

    #[must_use]
    pub fn offset_to_position(&self, offset: usize) -> (usize, usize) {
        // partition_point returns the count of elements for which the predicate holds —
        // i.e. the index of the first line whose start offset exceeds `offset`.
        let i = self.line_offsets.partition_point(|&start| start <= offset);
        if i == 0 {
            return (1, 1);
        }
        let line_idx = i - 1; // 0-indexed
        let column = offset
            - self
                .line_offsets
                .get(line_idx)
                .expect("line_idx = i-1, i from partition_point so valid")
            + 1;
        (line_idx + 1, column) // 1-indexed
    }

    /// Returns the 1-indexed line numbers inside code blocks or inline code.
    /// Result is precomputed in `new()` — O(1) to access.
    #[must_use]
    pub fn get_code_line_numbers(&self) -> &HashSet<usize> {
        &self.code_lines
    }

    /// Returns the 1-indexed line numbers inside code blocks only (not inline spans).
    /// Result is precomputed in `new()` — O(1) to access.
    #[must_use]
    pub fn get_code_block_line_numbers(&self) -> &HashSet<usize> {
        &self.code_block_lines
    }

    /// Returns byte ranges (into the original content) for all code blocks and
    /// inline code spans. Result is precomputed in `new()` — O(1) to access.
    #[must_use]
    pub fn get_code_ranges(&self) -> &[Range<usize>] {
        &self.code_ranges
    }

    /// Returns the 1-indexed line numbers that form link reference definitions
    /// (`[label]: url`). Result is precomputed in `new()` — O(1) to access.
    #[must_use]
    pub fn get_ref_def_line_numbers(&self) -> &HashSet<usize> {
        &self.ref_def_lines
    }

    /// Returns a map of normalised (lowercase) label → 1-indexed line number for
    /// every link reference definition in the document.
    #[must_use]
    pub fn get_ref_defs(&self) -> &HashMap<String, usize> {
        &self.ref_defs
    }

    /// Returns the 1-indexed line numbers that form the YAML/TOML front matter
    /// block (including its `---`/`+++` delimiters). Result is precomputed in
    /// `new()` — O(1) to access.
    #[must_use]
    pub fn front_matter_lines(&self) -> &HashSet<usize> {
        &self.front_matter_lines
    }

    /// Returns the front matter `title` field's value, if present.
    #[must_use]
    pub fn front_matter_title(&self) -> Option<&str> {
        self.front_matter_title
            .as_ref()
            .map(|(title, _)| title.as_str())
    }

    /// Returns the 1-indexed line number the front matter `title` field appears
    /// on, if present.
    #[must_use]
    pub fn front_matter_title_line(&self) -> Option<usize> {
        self.front_matter_title.as_ref().map(|(_, line)| *line)
    }

    /// Returns the 1-indexed line numbers that fall entirely inside an HTML
    /// comment (`<!-- ... -->`). A line that has real content before the opening
    /// `<!--` or after the closing `-->` is not included. Result is precomputed
    /// in `new()` — O(1) to access.
    #[must_use]
    pub fn comment_lines(&self) -> &HashSet<usize> {
        &self.comment_lines
    }

    /// Returns `true` if `line_num` is blank, or is entirely inside an HTML
    /// comment or the front matter block — content that carries no linting
    /// significance of its own but should not be treated as "missing" content
    /// by rules that require blank-line separation (e.g. MD022, MD031, MD032).
    #[must_use]
    pub fn is_blank_line(&self, line_num: usize) -> bool {
        self.get_line(line_num).is_none_or(|l| l.trim().is_empty())
            || self.comment_lines.contains(&line_num)
            || self.front_matter_lines.contains(&line_num)
    }

    /// Converts a (1-indexed) line number and 0-indexed byte offset within that
    /// line to an absolute byte offset in the content.
    #[must_use]
    pub fn line_offset_to_absolute(&self, line_num: usize, byte_offset_in_line: usize) -> usize {
        if line_num == 0 || line_num > self.line_offsets.len() {
            return self.content.len();
        }
        self.line_offsets
            .get(line_num - 1)
            .expect("line_num <= line_offsets.len() checked")
            + byte_offset_in_line
    }

    #[must_use]
    pub fn is_heading(&self, event: &Event) -> bool {
        matches!(event, Event::Start(Tag::Heading { .. }))
    }

    #[must_use]
    pub fn is_code_block(&self, event: &Event) -> bool {
        matches!(event, Event::Start(Tag::CodeBlock(_)))
    }

    #[must_use]
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

/// Scans raw lines for HTML comments (`<!-- ... -->`, possibly spanning multiple
/// lines) and returns the 1-indexed line numbers that fall *entirely* inside one —
/// a line with real content before the opening `<!--` or after the closing `-->`
/// is not included, so mixed lines like `text <!-- note -->` keep normal linting.
/// Lines already inside a code block are skipped, since `<!--` inside a fence is
/// literal text, not a comment.
///
/// AIDEV: raw-line scan, not tokenizer-accurate — a `<!--` inside an inline code
/// span on a prose line is (incorrectly) treated as opening a comment. Upgrade to
/// a code-range-aware scan if this proves to bite in practice.
fn build_comment_lines(content: &str, code_block_lines: &HashSet<usize>) -> HashSet<usize> {
    let mut comment_lines = HashSet::new();
    let mut in_comment = false;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        if code_block_lines.contains(&line_num) {
            continue;
        }

        if in_comment {
            if let Some(end) = line.find("-->") {
                if line[end + 3..].trim().is_empty() {
                    comment_lines.insert(line_num);
                }
                in_comment = false;
            } else {
                comment_lines.insert(line_num);
            }
            continue;
        }

        if let Some(start) = line.find("<!--") {
            let before = line[..start].trim().is_empty();
            if let Some(end) = line[start..].find("-->") {
                if before && line[start + end + 3..].trim().is_empty() {
                    comment_lines.insert(line_num);
                }
            } else {
                if before {
                    comment_lines.insert(line_num);
                }
                in_comment = true;
            }
        }
    }

    comment_lines
}

/// Collects link reference definition metadata in one pass over the parser's
/// `reference_definitions()` map (populated before the first event is consumed).
/// Returns (line-number set, label→line map); both use 1-indexed line numbers and
/// normalised (lowercase) labels.
fn build_ref_def_info(
    content: &str,
    line_offsets: &[usize],
) -> (HashSet<usize>, HashMap<String, usize>) {
    let parser = Parser::new_ext(content, mk_options());
    let mut line_set = HashSet::new();
    let mut label_map = HashMap::new();
    for (label, link_def) in parser.reference_definitions().iter() {
        let start = line_from_offset(link_def.span.start, line_offsets);
        let end = line_from_offset(link_def.span.end.saturating_sub(1), line_offsets);
        for line in start..=end {
            line_set.insert(line);
        }
        label_map.insert(label.to_owned(), start);
    }
    (line_set, label_map)
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

    #[test]
    fn test_ref_def_line_numbers() {
        let content = "Text\n\n[foo]: https://example.com\n\nMore text";
        let parser = MarkdownParser::new(content);
        let ref_def_lines = parser.get_ref_def_line_numbers();

        assert!(ref_def_lines.contains(&3), "ref def line should be marked");
        assert!(!ref_def_lines.contains(&1), "prose should not be marked");
        assert!(!ref_def_lines.contains(&5), "prose should not be marked");
    }

    #[test]
    fn test_front_matter_lines() {
        let content = "---\ntitle: Test\n---\n# Heading\nContent";
        let parser = MarkdownParser::new(content);

        assert_eq!(
            parser.front_matter_lines(),
            &HashSet::from([1, 2, 3]),
            "front matter block (delimiters + content) should be marked"
        );
        assert!(!parser.front_matter_lines().contains(&4));
    }

    #[test]
    fn test_front_matter_title() {
        let content = "---\ntitle: My Title\n---\n# Heading";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.front_matter_title(), Some("My Title"));
        assert_eq!(parser.front_matter_title_line(), Some(2));
    }

    #[test]
    fn test_front_matter_title_absent() {
        let content = "# Heading\nContent";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.front_matter_title(), None);
        assert_eq!(parser.front_matter_title_line(), None);
    }

    #[test]
    fn test_comment_lines_single_line() {
        let content = "Text\n\n<!-- a comment -->\n\nMore text";
        let parser = MarkdownParser::new(content);

        assert!(parser.comment_lines().contains(&3));
        assert!(!parser.comment_lines().contains(&1));
        assert!(!parser.comment_lines().contains(&5));
    }

    #[test]
    fn test_comment_lines_multi_line() {
        let content = "Text\n<!--\nInside comment\nStill inside\n-->\nAfter";
        let parser = MarkdownParser::new(content);

        for line in 2..=5 {
            assert!(
                parser.comment_lines().contains(&line),
                "line {line} should be inside the comment"
            );
        }
        assert!(!parser.comment_lines().contains(&1));
        assert!(!parser.comment_lines().contains(&6));
    }

    #[test]
    fn test_comment_lines_mixed_content_not_marked() {
        let content = "text before <!-- note --> text after";
        let parser = MarkdownParser::new(content);

        assert!(
            !parser.comment_lines().contains(&1),
            "a line with content outside the comment should not be marked"
        );
    }

    #[test]
    fn test_comment_lines_ignore_code_block() {
        let content = "```\n<!-- not a comment, just text -->\n```";
        let parser = MarkdownParser::new(content);

        assert!(
            parser.comment_lines().is_empty(),
            "<!-- inside a fenced code block should not be treated as a comment"
        );
    }

    #[test]
    fn test_is_blank_line() {
        let content = "---\ntitle: t\n---\nText\n\n<!-- comment -->\nMore";
        let parser = MarkdownParser::new(content);

        assert!(parser.is_blank_line(1), "front matter line should be blank");
        assert!(parser.is_blank_line(5), "empty line should be blank");
        assert!(
            parser.is_blank_line(6),
            "comment-only line should count as blank"
        );
        assert!(!parser.is_blank_line(4), "prose line should not be blank");
    }
}
