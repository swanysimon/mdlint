use crate::markdown::front_matter::detect_front_matter;
use pulldown_cmark::{BrokenLink, CowStr, Event, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

pub struct MarkdownParser<'a> {
    content: &'a str,
    /// What pulldown-cmark parses: `content` with the front matter blanked to
    /// spaces. Front matter is not Markdown — left in, `title: x` above the
    /// closing `---` parses as a setext heading. Blanking byte-for-byte keeps
    /// every offset aligned with `content`.
    source: Cow<'a, str>,
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
    /// Last line (1-indexed) of the YAML/TOML front matter block, including its
    /// closing delimiter; 0 when there is none.
    front_matter_end: usize,
    /// Byte ranges of HTML comments, in document order.
    comments: Vec<Range<usize>>,
    /// Lines (1-indexed) inside HTML blocks, comment blocks included.
    html_block_lines: HashSet<usize>,
    /// Lines (1-indexed) that fall entirely inside HTML comments.
    comment_lines: HashSet<usize>,
    /// The subset of `comment_lines` belonging to comment blocks: HTML blocks
    /// opened by `<!--`, as opposed to comments inside a paragraph.
    comment_block_lines: HashSet<usize>,
}

impl<'a> MarkdownParser<'a> {
    #[must_use]
    pub fn new(content: &'a str) -> Self {
        let lines: Vec<&'a str> = content.lines().collect();
        let line_offsets = build_line_offsets(content);
        let front_matter_end = detect_front_matter(content).map_or(0, |fm| fm.end_line);
        let source = blank_front_matter(content, front_matter_end, &line_offsets);
        let (code_block_lines, code_lines, code_ranges) = build_code_info(&source, &line_offsets);
        let (ref_def_lines, ref_defs) = build_ref_def_info(&source, &line_offsets);
        let html_block_lines = build_html_block_lines(&source, &line_offsets);
        let (comments, block_comments) = build_comment_ranges(&source);
        let comment_lines = build_comment_lines(&source, &lines, &line_offsets, &comments);
        let comment_block_lines =
            build_comment_lines(&source, &lines, &line_offsets, &block_comments);
        Self {
            content,
            source,
            lines,
            line_offsets,
            code_block_lines,
            code_lines,
            code_ranges,
            ref_def_lines,
            ref_defs,
            front_matter_end,
            comments,
            html_block_lines,
            comment_lines,
            comment_block_lines,
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

    pub fn parse(&self) -> impl Iterator<Item = Event<'_>> {
        Parser::new_ext(&self.source, mk_options())
    }

    pub fn parse_with_offsets(&self) -> impl Iterator<Item = (Event<'_>, Range<usize>)> {
        Parser::new_ext(&self.source, mk_options()).into_offset_iter()
    }

    /// Like `parse_with_offsets`, but resolves otherwise-broken reference links
    /// (undefined labels) by flagging their `LinkType` as the corresponding
    /// `*Unknown` variant instead of silently dropping the event as plain text.
    /// Used by rules that need to detect undefined reference links/images.
    pub fn parse_with_broken_links(&self) -> impl Iterator<Item = (Event<'_>, Range<usize>)> {
        Parser::new_with_broken_link_callback(
            &self.source,
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

    /// Returns `true` if the 1-indexed `line_num` is part of the YAML/TOML front
    /// matter block, delimiters included.
    #[must_use]
    pub fn is_front_matter_line(&self, line_num: usize) -> bool {
        (1..=self.front_matter_end).contains(&line_num)
    }

    /// Returns the first front matter line (1-indexed, delimiters excluded)
    /// matching `pattern`. Used to find the `title` field that stands in for the
    /// document's top-level heading.
    #[must_use]
    pub fn front_matter_line_matching(&self, pattern: &Regex) -> Option<usize> {
        (2..self.front_matter_end)
            .find(|&line| self.get_line(line).is_some_and(|l| pattern.is_match(l)))
    }

    /// Returns the 1-indexed line numbers inside HTML blocks. Their text is raw
    /// HTML, so a line there that looks like Markdown (`- item`) is not.
    #[must_use]
    pub fn html_block_lines(&self) -> &HashSet<usize> {
        &self.html_block_lines
    }

    /// Returns the byte ranges of the HTML comments (`<!-- ... -->`) Markdown
    /// actually recognises, in document order: `<!--` inside code is not one.
    #[must_use]
    pub fn comments(&self) -> &[Range<usize>] {
        &self.comments
    }

    /// Returns the 1-indexed line numbers that fall entirely inside HTML
    /// comments. A line with real content before an opening `<!--` or after a
    /// closing `-->` is not included. Result is precomputed in `new()` — O(1) to
    /// access.
    #[must_use]
    pub fn comment_lines(&self) -> &HashSet<usize> {
        &self.comment_lines
    }

    /// Returns `true` if `line_num` is blank, belongs to an HTML comment
    /// block, or is part of the front matter — lines rules that require
    /// blank-line separation (MD022, MD031, MD032) treat as a separator. The
    /// formatter keeps a comment block's spacing as written, so both spacings
    /// must pass. A comment inside a paragraph does not count: its lines are
    /// paragraph content, and the formatter spaces the paragraph as usual.
    #[must_use]
    pub fn is_blank_line(&self, line_num: usize) -> bool {
        self.get_line(line_num).is_none_or(|l| l.trim().is_empty())
            || self.comment_block_lines.contains(&line_num)
            || self.is_front_matter_line(line_num)
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

/// Returns `content` with every byte of its first `front_matter_end` lines
/// blanked to a space, newlines kept, so the parser sees blank lines there and
/// offsets still line up. Borrows `content` unchanged when there is no front
/// matter.
fn blank_front_matter<'a>(
    content: &'a str,
    front_matter_end: usize,
    line_offsets: &[usize],
) -> Cow<'a, str> {
    if front_matter_end == 0 {
        return Cow::Borrowed(content);
    }
    let end = line_offsets
        .get(front_matter_end)
        .copied()
        .unwrap_or(content.len());
    let blanked: Vec<u8> = content
        .bytes()
        .take(end)
        .map(|b| if b == b'\n' || b == b'\r' { b } else { b' ' })
        .collect();
    let mut source = String::from_utf8(blanked).expect("only ASCII bytes remain");
    source.push_str(content.get(end..).expect("end is a line start"));
    Cow::Owned(source)
}

fn build_html_block_lines(source: &str, line_offsets: &[usize]) -> HashSet<usize> {
    Parser::new_ext(source, mk_options())
        .into_offset_iter()
        .filter(|(event, _)| matches!(event, Event::Start(Tag::HtmlBlock)))
        .flat_map(|(_, range)| {
            line_from_offset(range.start, line_offsets)
                ..=line_from_offset(range.end.saturating_sub(1), line_offsets)
        })
        .collect()
}

/// Collects the byte ranges of HTML comments from parser events, so a `<!--`
/// in a code span or code block is never mistaken for one. Inline comments
/// arrive as one `InlineHtml` event each. An HTML block holds no Markdown, so
/// its comments are found by scanning its text; an unclosed one runs to the end
/// of the block, as `CommonMark` specifies.
///
/// Returns every comment, and separately those in comment blocks (HTML blocks
/// opened by `<!--`), both in document order.
fn build_comment_ranges(source: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let mut comments = Vec::new();
    let mut block_comments = Vec::new();
    for (event, range) in Parser::new_ext(source, mk_options()).into_offset_iter() {
        match event {
            Event::InlineHtml(html) if html.starts_with("<!--") => comments.push(range),
            Event::Start(Tag::HtmlBlock) => {
                let first_block_comment = comments.len();
                let mut pos = range.start;
                while let Some(open) = source.get(pos..range.end).and_then(|b| b.find("<!--")) {
                    let start = pos + open;
                    let end = source
                        .get(start + 4..range.end)
                        .and_then(|rest| rest.find("-->"))
                        .map_or(range.end, |close| start + 4 + close + 3);
                    comments.push(start..end);
                    pos = end;
                }
                let is_comment_block = source
                    .get(range.clone())
                    .is_some_and(|block| block.trim_start().starts_with("<!--"));
                if is_comment_block {
                    block_comments.extend(
                        comments
                            .get(first_block_comment..)
                            .unwrap_or_default()
                            .iter()
                            .cloned(),
                    );
                }
            }
            _ => {}
        }
    }
    (comments, block_comments)
}

/// Returns the 1-indexed lines whose every non-whitespace byte lies inside a
/// comment. A blank line counts when a comment spans it. `comments` must be
/// sorted and non-overlapping, which document order guarantees.
fn build_comment_lines(
    source: &str,
    lines: &[&str],
    line_offsets: &[usize],
    comments: &[Range<usize>],
) -> HashSet<usize> {
    // The first comment ending after `pos` covers it iff it also starts at or
    // before it.
    let covering = |pos: usize| {
        comments
            .get(comments.partition_point(|c| c.end <= pos))
            .filter(|c| c.start <= pos)
    };
    let is_comment_line = |(idx, line): (usize, &&str)| {
        let line_start = *line_offsets.get(idx).expect("one offset per line");
        let line_end = line_start + line.trim_end().len();
        let mut pos = line_start + (line.len() - line.trim_start().len());
        if pos >= line_end {
            return covering(line_start).is_some_and(|c| c.start < line_start);
        }
        while pos < line_end {
            let Some(comment) = covering(pos) else {
                return false;
            };
            let rest = source.get(comment.end..line_end).unwrap_or_default();
            pos = line_end - rest.trim_start().len();
        }
        true
    };
    lines
        .iter()
        .enumerate()
        .filter(|&entry| is_comment_line(entry))
        .map(|(idx, _)| idx + 1)
        .collect()
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
    use indoc::indoc;

    #[test]
    fn test_basic_parsing() {
        let content = indoc! {"
            # Heading

            Some **bold** text."};
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.content(), content);
        assert_eq!(parser.line_count(), 3);
    }

    #[test]
    fn test_get_line() {
        let content = indoc! {"
            Line 1
            Line 2
            Line 3"};
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.get_line(1), Some("Line 1"));
        assert_eq!(parser.get_line(2), Some("Line 2"));
        assert_eq!(parser.get_line(3), Some("Line 3"));
        assert_eq!(parser.get_line(0), None);
        assert_eq!(parser.get_line(4), None);
    }

    #[test]
    fn test_offset_to_line() {
        let content = indoc! {"
            Line 1
            Line 2
            Line 3"};
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.offset_to_line(0), 1);
        assert_eq!(parser.offset_to_line(3), 1);
        assert_eq!(parser.offset_to_line(7), 2);
        assert_eq!(parser.offset_to_line(14), 3);
    }

    #[test]
    fn test_offset_to_position() {
        let content = indoc! {"
            Line 1
            Line 2
            Line 3"};
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
        let content = indoc! {"
            # Heading

            Paragraph"};
        let parser = MarkdownParser::new(content);

        let events: Vec<_> = parser.parse_with_offsets().collect();
        assert!(!events.is_empty());
    }

    #[test]
    fn test_event_type_checks() {
        let content = indoc! {"
            # Heading

            ```rust
            code
            ```

            - item"};
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
        let content = indoc! {"
            Normal text

            ```sql
            SELECT * FROM table_name
            WHERE user_id = 123
            ```

            More text"};
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
        let content = indoc! {"
            Normal text

            Text with `inline_code` here

            ```
            Code block
            ```

            Final text"};
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
        let offsets = build_line_offsets(indoc! {"
            abc
            def
            ghi"});
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
        let content = indoc! {"
            Text

            [foo]: https://example.com

            More text"};
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

        for line in 1..=3 {
            assert!(parser.is_front_matter_line(line), "line {line}");
        }
        assert!(!parser.is_front_matter_line(0));
        assert!(!parser.is_front_matter_line(4));
    }

    #[test]
    fn test_front_matter_is_not_parsed_as_markdown() {
        // Left in, `title: T` above the closing `---` is a setext heading.
        let content = "---\ntitle: T\n---\n\n#### Deep\n";
        let parser = MarkdownParser::new(content);

        let headings: Vec<usize> = parser
            .parse_with_offsets()
            .filter(|(event, _)| parser.is_heading(event))
            .map(|(_, range)| parser.offset_to_line(range.start))
            .collect();
        assert_eq!(headings, [5]);
        assert_eq!(parser.content(), content, "content() stays the raw text");
    }

    #[test]
    fn test_front_matter_blanking_keeps_offsets_with_multibyte_text() {
        let content = "---\ntitle: Ünïcödé\n---\n# Heading\n";
        let parser = MarkdownParser::new(content);

        let (_, range) = parser
            .parse_with_offsets()
            .find(|(event, _)| parser.is_heading(event))
            .unwrap();
        assert_eq!(&content[range], "# Heading\n");
    }

    #[test]
    fn test_front_matter_line_matching() {
        let content = "---\nauthor: A\ntitle: My Title\n---\ntitle: body\n";
        let parser = MarkdownParser::new(content);

        let title = Regex::new("^title:").unwrap();
        assert_eq!(parser.front_matter_line_matching(&title), Some(3));
        let delimiter = Regex::new("^---").unwrap();
        assert_eq!(parser.front_matter_line_matching(&delimiter), None);
    }

    #[test]
    fn test_front_matter_line_matching_without_front_matter() {
        let parser = MarkdownParser::new("title: x\n");

        assert_eq!(
            parser.front_matter_line_matching(&Regex::new("title").unwrap()),
            None
        );
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
    fn test_comment_opener_in_code_span_is_not_a_comment() {
        let content = "Use `<!--` to open a comment.\n\n#NoSpace\n";
        let parser = MarkdownParser::new(content);

        assert!(parser.comments().is_empty());
        assert!(parser.comment_lines().is_empty());
    }

    #[test]
    fn test_comment_lines_blank_line_inside_comment() {
        let content = "<!--\n\nstill inside\n-->\n\nAfter";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.comment_lines(), &HashSet::from([1, 2, 3, 4]));
    }

    #[test]
    fn test_comment_lines_two_comments_on_one_line() {
        let content = "<!-- a --> <!-- b -->\n<!-- a --> text\n";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.comment_lines(), &HashSet::from([1]));
    }

    #[test]
    fn test_html_block_lines() {
        let content = "text\n\n<div>\n- item\n</div>\n\n- real\n";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.html_block_lines(), &HashSet::from([3, 4, 5]));
    }

    #[test]
    fn test_comment_inside_html_block() {
        let content = "<div>\n<!-- note -->\n</div>\n";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.comment_lines(), &HashSet::from([2]));
    }

    #[test]
    fn test_inline_comment_spanning_lines() {
        // The Rust book's `[link][ref]<!--\nignore -->` idiom: neither line is
        // comment-only, but both belong to one comment.
        let content = "See [a][b]<!--\nignore --> more\n\n[b]: https://example.com\n";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.comments().len(), 1);
        assert_eq!(&content[parser.comments()[0].clone()], "<!--\nignore -->");
        assert!(parser.comment_lines().is_empty());
    }

    #[test]
    fn test_comment_inside_paragraph_is_not_a_separator() {
        // Lines 4-5 are inside a comment, but the comment is paragraph content.
        let content = "# T\n\ntext <!--\nnote\n-->\n## H\n";
        let parser = MarkdownParser::new(content);

        assert_eq!(parser.comment_lines(), &HashSet::from([4, 5]));
        assert!(!parser.is_blank_line(5));
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
