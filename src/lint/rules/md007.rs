use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag, TagEnd};
use serde_json::Value;

pub struct MD007;

impl Rule for MD007 {
    fn name(&self) -> &'static str {
        "MD007"
    }

    fn description(&self) -> &'static str {
        "Unordered list indentation"
    }

    fn tags(&self) -> &[&str] {
        &["bullet", "ul", "indentation"]
    }

    #[allow(clippy::cast_possible_truncation)] // serde_json gives u64; values are small config counts
    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let indent_size = config
            .and_then(|c| c.get("indent"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(2) as usize;

        let mut violations = Vec::new();
        // Open lists and blockquotes, outermost first.
        let mut containers: Vec<Container> = Vec::new();

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::List(start)) => containers.push(if start.is_some() {
                    Container::OrderedList
                } else {
                    Container::UnorderedList
                }),
                Event::Start(Tag::BlockQuote(_)) => containers.push(Container::BlockQuote),
                Event::End(TagEnd::List(_) | TagEnd::BlockQuote(_)) => {
                    containers.pop();
                }
                Event::Start(Tag::Item) => {
                    let Some(depth) = unordered_depth(&containers) else {
                        continue;
                    };
                    let (line_number, column) = parser.offset_to_position(range.start);
                    let Some(line) = parser.get_line(line_number) else {
                        continue;
                    };
                    // The item's range can open with indent beyond its
                    // parent's content column.
                    let start = (column - 1).min(line.len());
                    let marker = start + line[start..].len() - line[start..].trim_start().len();
                    let indent = marker_indent(&line[..marker]);
                    let expected_indent = depth * indent_size;
                    if indent != expected_indent {
                        violations.push(Violation {
                            line: line_number,
                            column: Some(1),
                            rule: self.name().to_owned(),
                            message: format!(
                                "Unordered list indentation should be {expected_indent} spaces (found {indent})"
                            ),
                            fix: None,
                        });
                    }
                }
                _ => {}
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        false
    }
}

enum Container {
    OrderedList,
    UnorderedList,
    BlockQuote,
}

/// How many unordered lists enclose an item of the innermost one, or `None`
/// when the rule does not apply to it.
///
/// As in markdownlint, a list nested in an ordered one is not checked: its
/// indent is set by the width of the ordered marker (three spaces under `1.`,
/// four under `10.`), not by `indent`.  Nor is a list inside a blockquote that
/// is itself inside a list, whose indent this rule has no single way to count.
fn unordered_depth(containers: &[Container]) -> Option<usize> {
    let lists = containers
        .iter()
        .skip_while(|container| matches!(container, Container::BlockQuote));
    let mut depth = 0usize;
    for container in lists {
        match container {
            Container::UnorderedList => depth += 1,
            Container::OrderedList | Container::BlockQuote => return None,
        }
    }
    depth.checked_sub(1)
}

/// Width of the indent before a list marker, given the text before it on its
/// line: what is left once any blockquote markers are removed.
fn marker_indent(before_marker: &str) -> usize {
    let mut rest = before_marker;
    while let Some(after) = rest.trim_start().strip_prefix('>') {
        rest = after.strip_prefix(' ').unwrap_or(after);
    }
    rest.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::rendered;
    use indoc::indoc;

    #[test]
    fn test_correct_indentation() {
        let content = indoc! {"
            * Item 1
              * Nested 1
                * Double nested
              * Nested 2
            * Item 2"};
        let parser = MarkdownParser::new(content);
        let rule = MD007;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_incorrect_indentation() {
        let content = "* Item 1\n   * Nested wrong - 3 spaces instead of 2";
        let parser = MarkdownParser::new(content);
        let rule = MD007;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:2:1: MD007 Unordered list indentation should be 2 spaces (found 3)"]
        );
    }

    #[test]
    fn test_custom_indent_size() {
        let content = "* Item 1\n    * Nested with 4 spaces";
        let parser = MarkdownParser::new(content);
        let rule = MD007;
        let config = serde_json::json!({ "indent": 4 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_multiple_levels() {
        let content = indoc! {"
            * Level 1
              * Level 2
                * Level 3
                  * Level 4"};
        let parser = MarkdownParser::new(content);
        let rule = MD007;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_list_nested_in_ordered_item_not_checked() {
        // Regression test for https://github.com/swanysimon/mdlint/issues/100
        let content = indoc! {"
            1. Parent item:
               - child one
               - child two
            2. Next item.

            10. Wide marker:
                - child
        "};
        let parser = MarkdownParser::new(content);
        assert_eq!(MD007.check(&parser, None).len(), 0);
    }

    #[test]
    fn test_indentation_inside_blockquote() {
        let content = indoc! {"
            > - Item
            >   - Nested
            >    - Wrong
        "};
        let parser = MarkdownParser::new(content);
        assert_eq!(
            rendered(&MD007.check(&parser, None)),
            ["test.md:3:1: MD007 Unordered list indentation should be 2 spaces (found 3)"]
        );
    }

    #[test]
    fn test_indented_code_lookalike_is_not_a_list_item() {
        // Four spaces after a paragraph is a lazy continuation, not a list.
        let content = "Text\n    - not an item\n";
        let parser = MarkdownParser::new(content);
        assert_eq!(MD007.check(&parser, None).len(), 0);
    }
}
