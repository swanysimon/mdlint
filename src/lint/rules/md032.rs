use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;

pub struct MD032;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListMarker {
    Asterisk,
    Plus,
    Dash,
    Ordered,
}

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
        let mut violations = Vec::new();
        let lines = parser.lines();
        let mut in_list = false;
        let mut current_marker: Option<ListMarker> = None;
        let mut last_list_line: usize = 0;
        let code_block_lines = parser.get_code_block_line_numbers();

        for (line_num, line) in lines.iter().enumerate() {
            if code_block_lines.contains(&(line_num + 1)) {
                continue;
            }
            let trimmed = line.trim_start();
            let list_marker = get_list_marker(trimmed);
            let is_indented = !line.is_empty()
                && line
                    .chars()
                    .next()
                    .expect("non-empty, checked above")
                    .is_whitespace();

            if list_marker.is_some() && in_list && is_indented {
                // Indented list-marker line while already inside a list: this is a
                // nested sub-list, not a sibling list at the same level. CommonMark
                // and markdownlint don't require blank lines around nested lists —
                // only around the outermost list — so treat it like any other
                // indented continuation line and leave the enclosing list's
                // marker/state untouched.
            } else if let Some(marker) = list_marker {
                if !in_list {
                    // Starting a new list
                    in_list = true;
                    current_marker = Some(marker);
                    last_list_line = line_num;

                    // Check if previous line is blank (unless it's the first line)
                    if line_num > 0 {
                        let prev_line = lines.get(line_num - 1).expect("line_num > 0");
                        if !prev_line.trim().is_empty() {
                            // Detect broken ordered list continuation: a line that
                            // looks like an ordered list item (e.g. "6.") following
                            // non-list text won't be parsed as a list item because
                            // only "1." can interrupt a paragraph (CommonMark §5.2).
                            if marker == ListMarker::Ordered && !starts_with_one(trimmed) {
                                // Report on the interrupting line (previous line),
                                // not the list-like line — that's where the break is.
                                violations.push(Violation {
                                    line: line_num, // previous line (0-indexed → 1-indexed)
                                    column: Some(1),
                                    rule: self.name().to_owned(),
                                    message: "Line breaks ordered list continuation; subsequent \
                                         numbered items are parsed as text, not list items"
                                        .to_owned(),
                                    fix: None,
                                });
                            } else {
                                violations.push(Violation {
                                    line: line_num + 1,
                                    column: Some(1),
                                    rule: self.name().to_owned(),
                                    message: "List should be surrounded by blank lines".to_owned(),
                                    fix: None,
                                });
                            }
                        }
                    }
                } else if Some(marker) != current_marker {
                    // Different list marker - this is a new list!
                    // The previous list needs a blank line after it (report at previous list line)
                    violations.push(Violation {
                        line: last_list_line + 1,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "List should be surrounded by blank lines".to_owned(),
                        fix: None,
                    });
                    // Also this new list needs a blank line before it (report at new list line)
                    violations.push(Violation {
                        line: line_num + 1,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "List should be surrounded by blank lines".to_owned(),
                        fix: None,
                    });
                    current_marker = Some(marker);
                    last_list_line = line_num;
                } else {
                    // Same marker, continue in list
                    last_list_line = line_num;
                }
            } else if in_list && is_indented && !line.trim().is_empty() {
                // Indented non-list line - this is a continuation of the list item
                // Do nothing, stay in list
            } else if in_list && !line.trim().is_empty() {
                // Ending a list (non-blank, non-indented, non-list line)
                in_list = false;
                current_marker = None;

                // Check if next line should have been blank
                violations.push(Violation {
                    line: line_num + 1, // The line after the list
                    column: Some(1),
                    rule: self.name().to_owned(),
                    message: "List should be surrounded by blank lines".to_owned(),
                    fix: None,
                });
            } else if in_list && line.trim().is_empty() {
                // Blank line during list - might be end
                // Look ahead to see if list continues with same marker
                let mut continues = false;
                for future_line in lines.iter().skip(line_num + 1) {
                    if let Some(future_marker) = get_list_marker(future_line.trim_start()) {
                        if Some(future_marker) == current_marker {
                            continues = true;
                        }
                        break;
                    } else if !future_line.trim().is_empty() {
                        break;
                    }
                }
                if !continues {
                    in_list = false;
                    current_marker = None;
                }
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        false
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

    #[test]
    fn test_properly_surrounded() {
        let content = "Text before\n\n* Item 1\n* Item 2\n\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_missing_blank_before() {
        let content = "Text before\n* Item 1\n* Item 2\n\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 2); // List starts on line 2
    }

    #[test]
    fn test_missing_blank_after() {
        let content = "Text before\n\n* Item 1\n* Item 2\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 5); // Text after list
    }

    #[test]
    fn test_first_line() {
        let content = "* Item 1\n* Item 2\n\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // First line is OK
    }

    #[test]
    fn test_wrapped_list_item() {
        // List items that wrap to multiple lines should not be treated as list ending
        let content = "Text before\n\n* This is a long list item\n  that wraps to the next line\n* Item 2\n\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        // Should have 0 violations - the wrapped line is a continuation, not a new paragraph
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_multiple_wrapped_lines() {
        // Multiple continuation lines in a single list item
        let content = "Text\n\n* Item with multiple\n  lines of text\n  spanning across\n  multiple lines\n* Item 2\n\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        // Should have 0 violations
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_wrapped_with_nested_list() {
        // Wrapped items with nested list
        let content =
            "Text\n\n* Item 1 that\n  wraps across lines\n  * Nested item\n* Item 2\n\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        // Should have 0 violations
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_list_in_code_block_not_flagged() {
        let content = "Text before\n\n```markdown\n- item 1\n- item 2\n```\n\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_mixed_markers_are_separate_lists() {
        // Different list markers are treated as separate lists
        let content = "Text\n\n* Item asterisk\n+ Item plus\n- Item dash\n\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        // Each marker change is a new list needing blank lines
        // + needs blank before/after (2 violations)
        // - needs blank before/after (2 violations)
        assert_eq!(violations.len(), 4);
    }

    #[test]
    fn test_nested_list_different_marker_tight_not_flagged() {
        // Regression test for issue #67: a nested ordered list directly under a
        // bullet item, with no blank lines separating it from the parent item's
        // text or the next sibling item, is not a set of separate top-level lists
        // and must not be flagged. This is exactly the output `mdlint format`
        // produces for this construct.
        let content = "# Example\n\n- First item:\n  1. One\n  2. Two\n- Second item\n";
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
        let content = "# Example\n\n- First item:\n\n  1. One\n  2. Two\n\n- Second item\n";
        let parser = MarkdownParser::new(content);
        let rule = MD032;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }
}
