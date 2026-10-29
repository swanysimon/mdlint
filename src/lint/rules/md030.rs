use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use pulldown_cmark::{Event, Tag};
use serde_json::Value;
use std::collections::BTreeSet;

pub struct MD030;

impl Rule for MD030 {
    fn name(&self) -> &'static str {
        "MD030"
    }

    fn description(&self) -> &'static str {
        "Spaces after list markers"
    }

    fn tags(&self) -> &[&str] {
        &["ol", "ul", "whitespace"]
    }

    #[allow(clippy::cast_possible_truncation)] // serde_json gives u64; values are small config counts
    #[allow(clippy::too_many_lines)] // rule logic requires checking multiple interacting config flags
    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let ul_single = config
            .and_then(|c| c.get("ul_single"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1) as usize;

        let _ul_multi = config
            .and_then(|c| c.get("ul_multi"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1) as usize;

        let ol_single = config
            .and_then(|c| c.get("ol_single"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1) as usize;

        let _ol_multi = config
            .and_then(|c| c.get("ol_multi"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1) as usize;

        let mut violations = Vec::new();

        // Only lines where the parser starts a list item. Text that merely
        // begins with a marker character (`-->`, `+1`, `*emphasis*`) is not a
        // list, and "fixing" it would turn it into one.
        let item_lines: BTreeSet<usize> = parser
            .parse_with_offsets()
            .filter(|(event, _)| matches!(event, Event::Start(Tag::Item)))
            .map(|(_, range)| parser.offset_to_line(range.start))
            .collect();

        for line_number in item_lines {
            let Some(line) = parser.get_line(line_number) else {
                continue;
            };
            let trimmed = line.trim_start();

            // Check unordered list markers
            if trimmed.starts_with('*') || trimmed.starts_with('+') || trimmed.starts_with('-') {
                let marker_char = trimmed
                    .chars()
                    .next()
                    .expect("non-empty, starts_with checked");
                let after_marker = &trimmed[1..];
                let space_count = after_marker.chars().take_while(|&c| c == ' ').count();

                // Only check if there's content after the marker (not just a marker alone)
                if !after_marker.trim().is_empty() {
                    // For now, assume single-line (could be enhanced to detect multi-line)
                    let expected = ul_single;

                    if space_count != expected {
                        // Fix the spacing after list marker
                        let leading_spaces = &line[..line.len() - trimmed.len()];
                        let content = after_marker[space_count..].trim_start();
                        let spaces = " ".repeat(expected);
                        let replacement = format!("{leading_spaces}{marker_char}{spaces}{content}");

                        violations.push(Violation {
                            line: line_number,
                            column: Some(line.len() - trimmed.len() + 2),
                            rule: self.name().to_owned(),
                            message: format!(
                                "Expected {expected} space(s) after list marker, found {space_count}"
                            ),
                            fix: Some(Fix {
                                line_start: line_number,
                                line_end: line_number,
                                column_start: None,
                                column_end: None,
                                replacement,
                                description: format!("Adjust spacing to {expected} space(s)"),
                            }),
                        });
                    }
                }
            }

            // Check ordered list markers
            if let Some(dot_pos) = trimmed.find('.') {
                let prefix = &trimmed[..dot_pos];
                if prefix.chars().all(|c| c.is_ascii_digit()) && !prefix.is_empty() {
                    let after_dot = &trimmed[dot_pos + 1..];

                    // Only check if there's content after the marker
                    if !after_dot.trim().is_empty() {
                        let space_count = after_dot.chars().take_while(|&c| c == ' ').count();

                        // For now, assume single-line
                        let expected = ol_single;

                        if space_count != expected {
                            // Fix the spacing after list marker
                            let leading_spaces = &line[..line.len() - trimmed.len()];
                            let marker = &trimmed[..=dot_pos];
                            let content = after_dot[space_count..].trim_start();
                            let spaces = " ".repeat(expected);
                            let replacement = format!("{leading_spaces}{marker}{spaces}{content}");

                            violations.push(Violation {
                                line: line_number,
                                column: Some(line.len() - trimmed.len() + dot_pos + 2),
                                rule: self.name().to_owned(),
                                message: format!(
                                    "Expected {expected} space(s) after list marker, found {space_count}"
                                ),
                                fix: Some(Fix {
                                    line_start: line_number,
                                    line_end: line_number,
                                    column_start: None,
                                    column_end: None,
                                    replacement,
                                    description: format!("Adjust spacing to {expected} space(s)"),
                                }),
                            });
                        }
                    }
                }
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::rendered;
    use indoc::indoc;

    #[test]
    fn test_correct_spacing() {
        let content = indoc! {"
            * Item 1
            + Item 2
            - Item 3
            1. Ordered"};
        let parser = MarkdownParser::new(content);
        let rule = MD030;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_marker_character_without_space_is_not_a_list() {
        // None of these is a list item, and the old "fix" (`* Item`, `- ->`)
        // would have turned each into one, which `mdlint format` never does.
        let content = indoc! {"
            *Item without space

            -->

            +++

            -5 degrees"};
        let parser = MarkdownParser::new(content);
        let violations = MD030.check(&parser, None);

        assert_eq!(violations.len(), 0, "{violations:?}");
    }

    #[test]
    fn test_fix_collapses_spaces_after_marker() {
        let content = "-  Item\n1.   Ordered\n";
        let parser = MarkdownParser::new(content);
        let found: Vec<_> = MD030
            .check(&parser, None)
            .into_iter()
            .filter_map(|v| v.fix)
            .collect();
        let result = crate::fix::Fixer::new()
            .apply_fixes_to_content(content, &found)
            .unwrap();

        assert_eq!(result, "- Item\n1. Ordered\n");
    }

    #[test]
    fn test_multiple_spaces() {
        let content = "*  Item with 2 spaces";
        let parser = MarkdownParser::new(content);
        let rule = MD030;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:1:2: MD030 Expected 1 space(s) after list marker, found 2"]
        );
    }

    #[test]
    fn test_custom_spacing() {
        let content = "*  Item with 2 spaces";
        let parser = MarkdownParser::new(content);
        let rule = MD030;
        let config = serde_json::json!({ "ul_single": 2 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0); // 2 spaces now expected
    }

    #[test]
    fn test_bold_not_list_marker() {
        // Bold/emphasis at start of line should not be treated as list marker
        let content = indoc! {"
            **Slice-specific schemas** → some text
            **Bold text** at start
            *Italic text* here
            __Also bold__ text"};
        let parser = MarkdownParser::new(content);
        let rule = MD030;
        let violations = rule.check(&parser, None);

        assert_eq!(
            violations.len(),
            0,
            "Bold/emphasis should not trigger MD030"
        );
    }

    #[test]
    fn test_actual_list_with_bold() {
        // Actual list items can contain bold text
        let content = indoc! {"
            * **Bold** item
            + *Italic* item
            - Normal item"};
        let parser = MarkdownParser::new(content);
        let rule = MD030;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_horizontal_rules_not_list_markers() {
        // Horizontal rules should not trigger MD030 violations
        let content = indoc! {"
            # Heading

            ---

            More content

            ***

            ___

            * * *

            - - -"};
        let parser = MarkdownParser::new(content);
        let rule = MD030;
        let violations = rule.check(&parser, None);

        assert_eq!(
            violations.len(),
            0,
            "Horizontal rules should not be treated as list markers"
        );
    }

    #[test]
    fn test_code_blocks_not_checked() {
        // Code blocks should not trigger MD030 violations
        let content = indoc! {"
            # Heading

            ```
            --config <CONFIG>
            --fix
            -h, --help
            ```

            Normal text with `-h` inline code."};
        let parser = MarkdownParser::new(content);
        let rule = MD030;
        let violations = rule.check(&parser, None);

        assert_eq!(
            violations.len(),
            0,
            "Code blocks and inline code should not be checked for list markers"
        );
    }

    #[test]
    fn test_real_list_after_code_block() {
        // Real list markers outside code blocks should still be checked
        let content = indoc! {"
            ```
            --config
            ```

            *  Item with 2 spaces"};
        let parser = MarkdownParser::new(content);
        let rule = MD030;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:5:2: MD030 Expected 1 space(s) after list marker, found 2"],
            "Real list markers outside code blocks should be checked"
        );
    }

    #[test]
    fn test_table_separator_not_list() {
        // Table separator lines should not trigger MD030 violations
        let content = indoc! {"
            Rule  | Description
            ------|------------
            MD001 | First rule
            MD002 | Second rule"};
        let parser = MarkdownParser::new(content);
        let rule = MD030;
        let violations = rule.check(&parser, None);

        assert_eq!(
            violations.len(),
            0,
            "Table separator lines should not be treated as list markers"
        );
    }
}
