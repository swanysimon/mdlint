use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD004;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListMarker {
    Asterisk, // *
    Plus,     // +
    Dash,     // -
}

impl Rule for MD004 {
    fn name(&self) -> &'static str {
        "MD004"
    }

    fn description(&self) -> &'static str {
        "Unordered list style should be consistent"
    }

    fn tags(&self) -> &[&str] {
        &["bullet", "ul"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let style = config
            .and_then(|c| c.get("style"))
            .and_then(|v| v.as_str())
            .unwrap_or("dash");

        let mut violations = Vec::new();
        let mut first_marker: Option<ListMarker> = None;
        let code_block_lines = parser.get_code_block_line_numbers();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            // Skip code blocks
            if code_block_lines.contains(&line_number) {
                continue;
            }

            let trimmed = line.trim_start();

            // Detect unordered list marker
            let marker = if trimmed.starts_with("* ") {
                Some(ListMarker::Asterisk)
            } else if trimmed.starts_with("+ ") {
                Some(ListMarker::Plus)
            } else if trimmed.starts_with("- ") {
                Some(ListMarker::Dash)
            } else {
                None
            };

            if let Some(current_marker) = marker {
                if style == "consistent" {
                    if let Some(first) = first_marker {
                        if current_marker != first {
                            let indent_len = line.len() - trimmed.len();
                            let replacement = format!("{}- {}", &line[..indent_len], &trimmed[2..]);
                            violations.push(Violation {
                                line: line_number,
                                column: Some(indent_len + 1),
                                rule: self.name().to_owned(),
                                message: format!(
                                    "List marker style should be consistent (expected {first:?}, found {current_marker:?})"
                                ),
                                fix: Some(Fix {
                                    line_start: line_number,
                                    line_end: line_number,
                                    column_start: None,
                                    column_end: None,
                                    replacement,
                                    description: "Replace list marker with dash".to_owned(),
                                }),
                            });
                        }
                    } else {
                        first_marker = Some(current_marker);
                    }
                } else {
                    let required_marker = match style {
                        "asterisk" => ListMarker::Asterisk,
                        "plus" => ListMarker::Plus,
                        "dash" => ListMarker::Dash,
                        _ => continue,
                    };

                    if current_marker != required_marker {
                        let indent_len = line.len() - trimmed.len();
                        let replacement = format!(
                            "{}{} {}",
                            &line[..indent_len],
                            match required_marker {
                                ListMarker::Asterisk => "*",
                                ListMarker::Plus => "+",
                                ListMarker::Dash => "-",
                            },
                            &trimmed[2..]
                        );
                        violations.push(Violation {
                            line: line_number,
                            column: Some(indent_len + 1),
                            rule: self.name().to_owned(),
                            message: format!("List marker style should be {required_marker:?}"),
                            fix: Some(Fix {
                                line_start: line_number,
                                line_end: line_number,
                                column_start: None,
                                column_end: None,
                                replacement,
                                description: "Replace list marker with required style".to_owned(),
                            }),
                        });
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
    use crate::fix::Fixer;
    use crate::lint::rules::rendered;
    use indoc::indoc;

    fn apply_fixes(content: &str, violations: &[Violation]) -> String {
        let fixes: Vec<_> = violations.iter().filter_map(|v| v.fix.clone()).collect();
        Fixer::new()
            .apply_fixes_to_content(content, &fixes)
            .unwrap()
    }

    #[test]
    fn test_consistent_asterisk() {
        let content = indoc! {"
            * Item 1
            * Item 2
            * Item 3"};
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let config = serde_json::json!({ "style": "consistent" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inconsistent_markers() {
        let content = indoc! {"
            * Item 1
            + Item 2
            - Item 3"};
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let violations = rule.check(&parser, None);

        // mdlint enforces `dash` by default rather than first-marker-wins, so
        // the asterisk and plus are flagged and the dash is accepted.
        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:1: MD004 List marker style should be Dash",
                "test.md:2:1: MD004 List marker style should be Dash",
            ]
        );
    }

    #[test]
    fn test_enforced_dash_style() {
        let content = "* Item 1\n- Item 2";
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let config = serde_json::json!({ "style": "dash" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD004 List marker style should be Dash"]
        );
    }

    #[test]
    fn test_nested_lists() {
        let content = indoc! {"
            * Item 1
              * Nested 1
              * Nested 2
            * Item 2"};
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let config = serde_json::json!({ "style": "consistent" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0); // All use asterisk
    }

    #[test]
    fn test_list_markers_in_code_block_not_flagged() {
        // List markers inside fenced code blocks must not be checked.
        let content = indoc! {"
            ```
            * asterisk
            + plus
            - dash
            ```

            - real item
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let config = serde_json::json!({ "style": "dash" });
        let violations = rule.check(&parser, Some(&config));

        // Only the real list item on the last line matters; the code block is ignored.
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_markdown_syntax_in_code_block() {
        let content = indoc! {"
            # My Document

            Here's a code block with markdown syntax:

            ```
            - This looks like a list item
            * This also looks like a list item
            + And this one too
            ```

            - Real list item
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let violations = rule.check(&parser, None);

        // Should not flag list markers inside code blocks
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_indented_code_block() {
        let content = indoc! {"
            Regular text

                - This is an indented code block
                * Not a real list
                + Just code

            - Real list item
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let violations = rule.check(&parser, None);

        // Should not flag list markers in indented code blocks
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_fix_normalises_marker_to_dash() {
        let content = indoc! {"
            * Item 1
            * Item 2
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let config = serde_json::json!({ "style": "dash" });
        let violations = rule.check(&parser, Some(&config));
        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:1: MD004 List marker style should be Dash",
                "test.md:2:1: MD004 List marker style should be Dash",
            ]
        );
        let fixed = apply_fixes(content, &violations);
        assert_eq!(
            fixed,
            indoc! {"
                - Item 1
                - Item 2
            "}
        );
    }

    #[test]
    fn test_dash_in_code_block_with_real_list() {
        let content = indoc! {"
            * List item 1

            ```python
            # Comment with -- dashes
            value = 10 - 5  # subtraction
            ```

            + List item 2
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let violations = rule.check(&parser, None);

        // Both non-dash markers violate the default "dash" style
        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:1: MD004 List marker style should be Dash",
                "test.md:8:1: MD004 List marker style should be Dash",
            ]
        );
    }
}
