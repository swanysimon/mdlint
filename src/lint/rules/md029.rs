use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use pulldown_cmark::{Event, Tag, TagEnd};
use serde_json::Value;

pub struct MD029;

impl Rule for MD029 {
    fn name(&self) -> &str {
        "MD029"
    }

    fn description(&self) -> &str {
        "Ordered list item prefix"
    }

    fn tags(&self) -> &[&str] {
        &["ol"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let style = config
            .and_then(|c| c.get("style"))
            .and_then(|v| v.as_str())
            .unwrap_or("ordered");

        let mut violations = Vec::new();
        // Stack: None = unordered list, Some((expected_next, seen_non_one)) = ordered list.
        // Using AST events rather than raw line scanning ensures that code blocks, headings,
        // and other block-level elements correctly break list continuity.
        let mut list_stack: Vec<Option<(usize, bool)>> = Vec::new();

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::List(start)) => {
                    if start.is_some() {
                        // Ordered list. The formatter canonicalises all ordered lists to
                        // start at 1, so we always expect the first item to be 1.
                        list_stack.push(Some((1, false)));
                    } else {
                        list_stack.push(None);
                    }
                }
                Event::End(TagEnd::List(_)) => {
                    list_stack.pop();
                }
                Event::Start(Tag::Item) => {
                    if let Some(Some((expected, seen_non_one))) = list_stack.last_mut() {
                        let line_num = parser.offset_to_line(range.start);
                        if let Some(line) = parser.get_line(line_num)
                            && let Some(num) = parse_item_number(line.trim_start())
                        {
                            if num != 1 {
                                *seen_non_one = true;
                            }

                            let is_valid = match style {
                                "one" => num == 1,
                                "ordered" => num == *expected,
                                _ => {
                                    // "one_or_ordered": if we've seen non-1, require sequential;
                                    // otherwise allow either all-ones or sequential.
                                    if *seen_non_one {
                                        num == *expected
                                    } else {
                                        num == 1 || num == *expected
                                    }
                                }
                            };

                            if !is_valid {
                                let should_be = if style == "one" { 1 } else { *expected };
                                let indent = line.len() - line.trim_start().len();
                                let digit_len = line
                                    .trim_start()
                                    .chars()
                                    .take_while(|c| c.is_ascii_digit())
                                    .count();
                                violations.push(Violation {
                                    line: line_num,
                                    column: Some(indent + 1),
                                    rule: self.name().to_string(),
                                    message: format!(
                                        "Ordered list item prefix: expected {}, found {}",
                                        should_be, num
                                    ),
                                    fix: Some(Fix {
                                        line_start: line_num,
                                        line_end: line_num,
                                        column_start: Some(indent + 1),
                                        column_end: Some(indent + digit_len),
                                        replacement: should_be.to_string(),
                                        description: format!(
                                            "Renumber ordered list item to {}",
                                            should_be
                                        ),
                                    }),
                                });
                            }

                            *expected += 1;
                        }
                    }
                }
                _ => {}
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        true
    }
}

/// Extract the leading integer from an ordered list item line (after stripping indentation).
/// Returns `Some(n)` for `"3. text"` or `"3) text"`, `None` otherwise.
fn parse_item_number(trimmed: &str) -> Option<usize> {
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = &trimmed[digits.len()..];
    if rest.starts_with(['.', ')']) {
        digits.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ordered_sequence() {
        let content = "1. First\n2. Second\n3. Third";
        let parser = MarkdownParser::new(content);
        let rule = MD029;
        let config = serde_json::json!({ "style": "ordered" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_all_ones() {
        let content = "1. First\n1. Second\n1. Third";
        let parser = MarkdownParser::new(content);
        let rule = MD029;
        let violations = rule.check(&parser, None);

        // Default "ordered": items 2 and 3 should be 2 and 3, not 1
        assert_eq!(violations.len(), 2);
        assert_eq!(violations[0].line, 2);
        assert_eq!(violations[1].line, 3);
    }

    #[test]
    fn test_wrong_sequence() {
        let content = "1. First\n3. Third - wrong\n4. Fourth";
        let parser = MarkdownParser::new(content);
        let rule = MD029;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2); // Lines 2 and 3 are wrong
        assert_eq!(violations[0].line, 2);
        assert_eq!(violations[1].line, 3);
    }

    #[test]
    fn test_enforced_ordered() {
        let content = "1. First\n1. Second - should be 2";
        let parser = MarkdownParser::new(content);
        let rule = MD029;
        let config = serde_json::json!({ "style": "ordered" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 2);
    }

    #[test]
    fn test_enforced_one() {
        let content = "1. First\n2. Second - should be 1";
        let parser = MarkdownParser::new(content);
        let rule = MD029;
        let config = serde_json::json!({ "style": "one" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 2);
    }

    #[test]
    fn test_list_with_backticks() {
        // Test numbered list where items contain backticks
        let content = "1. Command-line options (`--config`)\n\
                       2. Local directory config (`mdlint.toml` in current dir)\n\
                       3. Parent directory configs (walking up to root)\n\
                       4. Default configuration";
        let parser = MarkdownParser::new(content);
        let rule = MD029;
        let config = serde_json::json!({ "style": "ordered" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0, "List with backticks should be valid");
    }

    #[test]
    fn test_fix_populated_for_wrong_number() {
        let content = "1. First\n1. Second\n1. Third";
        let parser = MarkdownParser::new(content);
        let rule = MD029;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2);
        let fix0 = violations[0].fix.as_ref().expect("fix should be Some");
        assert_eq!(fix0.line_start, 2);
        assert_eq!(fix0.replacement, "2");
        let fix1 = violations[1].fix.as_ref().expect("fix should be Some");
        assert_eq!(fix1.line_start, 3);
        assert_eq!(fix1.replacement, "3");
    }

    #[test]
    fn test_fix_indented_list() {
        let content = "1. First\n\n   text\n\n1. Second";
        let parser = MarkdownParser::new(content);
        let rule = MD029;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        let fix = violations[0].fix.as_ref().expect("fix should be Some");
        assert_eq!(fix.replacement, "2");
        assert_eq!(fix.column_start, Some(1));
    }

    #[test]
    fn test_code_block_breaks_list() {
        // An unindented code block breaks CommonMark list continuity.
        // Each numbered item is its own single-item list starting at 1.
        let content = "1. First item\n\n```\ncode\n```\n\n1. Second item\n\n```\ncode\n```\n\n1. Third item\n";
        let parser = MarkdownParser::new(content);
        let rule = MD029;
        let violations = rule.check(&parser, None);

        // Each `1.` is the first item of a fresh list — no violations.
        assert_eq!(violations.len(), 0);
    }
}
