use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;
use std::collections::HashSet;

pub struct MD032;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListMarker {
    Asterisk,
    Plus,
    Dash,
    Ordered,
}

const MISSING_BEFORE: &str = "List should be surrounded by blank lines (missing before)";
const MISSING_AFTER: &str = "List should be surrounded by blank lines (missing after)";
const BROKEN_ORDERED: &str = "Line breaks ordered list continuation; subsequent numbered items \
                              are parsed as text, not list items";

impl Rule for MD032 {
    fn name(&self) -> &'static str {
        "MD032"
    }

    fn description(&self) -> &'static str {
        "Lists should be surrounded by blank lines"
    }

    fn tags(&self) -> &[&str] {
        &["bullet", "ul", "ol", "blank_lines"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        ListScanner::new(parser.lines()).scan(parser.get_code_block_line_numbers())
    }

    fn fixable(&self) -> bool {
        false
    }
}

/// Line-by-line scanner tracking which list, if any, the current line belongs to.
///
/// Line numbers are 0-indexed while scanning and converted to the 1-indexed form violations
/// use at the point they are reported.
struct ListScanner<'a> {
    lines: &'a [&'a str],
    violations: Vec<Violation>,
    in_list: bool,
    current_marker: Option<ListMarker>,
    last_list_line: usize,
}

impl<'a> ListScanner<'a> {
    fn new(lines: &'a [&'a str]) -> Self {
        Self {
            lines,
            violations: Vec::new(),
            in_list: false,
            current_marker: None,
            last_list_line: 0,
        }
    }

    fn scan(mut self, code_block_lines: &HashSet<usize>) -> Vec<Violation> {
        for (line_num, line) in self.lines.iter().enumerate() {
            if !code_block_lines.contains(&(line_num + 1)) {
                self.visit_line(line_num, line);
            }
        }
        self.violations
    }

    fn visit_line(&mut self, line_num: usize, line: &str) {
        let trimmed = line.trim_start();
        let is_indented = line.starts_with(char::is_whitespace);
        let is_blank = trimmed.is_empty();

        match get_list_marker(trimmed) {
            // Indented list-marker line while already inside a list: this is a nested
            // sub-list, not a sibling list at the same level. CommonMark and markdownlint
            // don't require blank lines around nested lists — only around the outermost
            // list — so treat it like any other indented continuation line and leave the
            // enclosing list's marker/state untouched.
            Some(_) if self.in_list && is_indented => {}
            Some(marker) if !self.in_list => self.start_list(line_num, marker, trimmed),
            Some(marker) if Some(marker) != self.current_marker => {
                self.switch_list(line_num, marker);
            }
            // Same marker, continue in list.
            Some(_) => self.last_list_line = line_num,
            // Indented non-list line: a continuation of the current list item.
            None if !self.in_list || (is_indented && !is_blank) => {}
            None if is_blank => self.end_list_on_blank(line_num),
            None => self.end_list(line_num),
        }
    }

    fn start_list(&mut self, line_num: usize, marker: ListMarker, trimmed: &str) {
        self.in_list = true;
        self.current_marker = Some(marker);
        self.last_list_line = line_num;

        // A list opening the file, or preceded by a blank line, is correctly surrounded.
        if line_num == 0 || self.lines[line_num - 1].trim().is_empty() {
            return;
        }

        // Detect broken ordered list continuation: a line that looks like an ordered list
        // item (e.g. "6.") following non-list text won't be parsed as a list item because
        // only "1." can interrupt a paragraph (CommonMark §5.2). Report on the interrupting
        // line rather than the list-like line — that's where the break is.
        if marker == ListMarker::Ordered && !starts_with_one(trimmed) {
            self.report(line_num, BROKEN_ORDERED);
        } else {
            self.report(line_num + 1, MISSING_BEFORE);
        }
    }

    /// A different marker starts a new list: the previous one is missing a blank line after
    /// it, and the new one is missing a blank line before it.
    fn switch_list(&mut self, line_num: usize, marker: ListMarker) {
        self.report(self.last_list_line + 1, MISSING_AFTER);
        self.report(line_num + 1, MISSING_BEFORE);
        self.current_marker = Some(marker);
        self.last_list_line = line_num;
    }

    /// A non-blank, non-indented, non-list line ends the list without the blank line that
    /// should have separated them.
    fn end_list(&mut self, line_num: usize) {
        self.in_list = false;
        self.current_marker = None;
        self.report(line_num + 1, MISSING_AFTER);
    }

    /// A blank line ends the list unless the same list resumes after it.
    fn end_list_on_blank(&mut self, line_num: usize) {
        if !self.list_continues_after(line_num) {
            self.in_list = false;
            self.current_marker = None;
        }
    }

    fn list_continues_after(&self, blank_line: usize) -> bool {
        self.lines
            .iter()
            .skip(blank_line + 1)
            .find(|line| !line.trim().is_empty())
            .and_then(|line| get_list_marker(line.trim_start()))
            .is_some_and(|marker| Some(marker) == self.current_marker)
    }

    fn report(&mut self, line: usize, message: &str) {
        self.violations.push(Violation {
            line,
            column: Some(1),
            rule: "MD032".to_owned(),
            message: message.to_owned(),
            fix: None,
        });
    }
}

/// Returns true if the line starts with `1.` or `1)` (the only ordered marker
/// that can interrupt a paragraph in `CommonMark`).
fn starts_with_one(trimmed: &str) -> bool {
    let check = trimmed.strip_prefix('\\').unwrap_or(trimmed);
    check.starts_with("1. ") || check.starts_with("1) ")
}

fn get_list_marker(trimmed: &str) -> Option<ListMarker> {
    // Check for unordered list markers
    if trimmed.starts_with("* ") {
        return Some(ListMarker::Asterisk);
    }
    if trimmed.starts_with("+ ") {
        return Some(ListMarker::Plus);
    }
    if trimmed.starts_with("- ") {
        return Some(ListMarker::Dash);
    }

    // Check for ordered list markers (also detect escaped markers like \6.)
    let check = if let Some(stripped) = trimmed.strip_prefix('\\') {
        stripped
    } else {
        trimmed
    };
    if let Some(dot_pos) = check.find(". ") {
        let prefix = &check[..dot_pos];
        if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
            return Some(ListMarker::Ordered);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::rendered;
    use indoc::indoc;

    #[test]
    fn test_properly_surrounded() {
        let content = indoc! {"
            Text before

            * Item 1
            * Item 2

            Text after"};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_missing_blank_before() {
        let content = indoc! {"
            Text before
            * Item 1
            * Item 2

            Text after"};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:2:1: MD032 List should be surrounded by blank lines (missing before)"]
        );
    }

    #[test]
    fn test_missing_blank_after() {
        let content = indoc! {"
            Text before

            * Item 1
            * Item 2
            Text after"};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:5:1: MD032 List should be surrounded by blank lines (missing after)"]
        );
    }

    #[test]
    fn test_first_line() {
        let content = indoc! {"
            * Item 1
            * Item 2

            Text after"};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // First line is OK
    }

    #[test]
    fn test_wrapped_list_item() {
        // List items that wrap to multiple lines should not be treated as list ending
        let content = indoc! {"
            Text before

            * This is a long list item
              that wraps to the next line
            * Item 2

            Text after"};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        // Should have 0 violations - the wrapped line is a continuation, not a new paragraph
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_multiple_wrapped_lines() {
        // Multiple continuation lines in a single list item
        let content = indoc! {"
            Text

            * Item with multiple
              lines of text
              spanning across
              multiple lines
            * Item 2

            Text after"};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        // Should have 0 violations
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_wrapped_with_nested_list() {
        // Wrapped items with nested list
        let content = indoc! {"
            Text

            * Item 1 that
              wraps across lines
              * Nested item
            * Item 2

            Text after"};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        // Should have 0 violations
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_list_in_code_block_not_flagged() {
        let content = indoc! {"
            Text before

            ```markdown
            - item 1
            - item 2
            ```

            Text after"};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_mixed_markers_are_separate_lists() {
        // Different list markers are treated as separate lists
        let content = indoc! {"
            Text

            * Item asterisk
            + Item plus
            - Item dash

            Text after"};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        // Each marker change starts a new list, so every boundary reports both
        // "previous list needs a blank after" and "new list needs a blank
        // before".
        assert_eq!(
            rendered(&violations),
            [
                "test.md:3:1: MD032 List should be surrounded by blank lines (missing after)",
                "test.md:4:1: MD032 List should be surrounded by blank lines (missing before)",
                "test.md:4:1: MD032 List should be surrounded by blank lines (missing after)",
                "test.md:5:1: MD032 List should be surrounded by blank lines (missing before)",
            ]
        );
    }

    #[test]
    fn test_ordered_list_interrupted_by_paragraph() {
        // Only "1." can interrupt a paragraph (CommonMark §5.2), so "6." after text is
        // parsed as plain text. The violation is reported on the interrupting line.
        let content = indoc! {"
            Some paragraph text
            6. Six
            7. Seven
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:1: MD032 Line breaks ordered list continuation; subsequent numbered \
              items are parsed as text, not list items"
            ]
        );
    }

    #[test]
    fn test_nested_list_different_marker_tight_not_flagged() {
        // Regression test for issue #67: a nested ordered list directly under a
        // bullet item, with no blank lines separating it from the parent item's
        // text or the next sibling item, is not a set of separate top-level lists
        // and must not be flagged. This is exactly the output `mdlint format`
        // produces for this construct.
        let content = indoc! {"
            # Example

            - First item:
              1. One
              2. Two
            - Second item
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_nested_list_different_marker_loose_not_flagged() {
        // Same construct as above, but with the blank lines that make the outer
        // list loose. Both forms are valid CommonMark and neither should be
        // flagged by MD032.
        let content = indoc! {"
            # Example

            - First item:

              1. One
              2. Two

            - Second item
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }
}
