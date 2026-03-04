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
    fn name(&self) -> &str {
        "MD004"
    }

    fn description(&self) -> &str {
        "Unordered list style should be consistent"
    }

    fn tags(&self) -> &[&str] {
        &["bullet", "ul"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let style_config = config.and_then(|c| c.get("style")).and_then(|v| v.as_str());

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
                // If config specifies a style, check against it
                if let Some(required) = style_config {
                    let required_marker = match required {
                        "asterisk" => ListMarker::Asterisk,
                        "plus" => ListMarker::Plus,
                        "dash" => ListMarker::Dash,
                        _ => continue,
                    };

                    if current_marker != required_marker {
                        let indent_len = line.len() - trimmed.len();
                        let replacement =
                            format!("{}- {}", &line[..indent_len], &trimmed[2..]);
                        violations.push(Violation {
                            line: line_number,
                            column: Some(indent_len + 1),
                            rule: self.name().to_string(),
                            message: format!("List marker style should be {:?}", required_marker),
                            fix: Some(Fix {
                                line_start: line_number,
                                line_end: line_number,
                                column_start: None,
                                column_end: None,
                                replacement,
                                description: "Replace list marker with dash".to_string(),
                            }),
                        });
                    }
                } else {
                    // No config: ensure consistency
                    if let Some(first) = first_marker {
                        if current_marker != first {
                            let indent_len = line.len() - trimmed.len();
                            let replacement =
                                format!("{}- {}", &line[..indent_len], &trimmed[2..]);
                            violations.push(Violation {
                                line: line_number,
                                column: Some(indent_len + 1),
                                rule: self.name().to_string(),
                                message: format!(
                                    "List marker style should be consistent (expected {:?}, found {:?})",
                                    first, current_marker
                                ),
                                fix: Some(Fix {
                                    line_start: line_number,
                                    line_end: line_number,
                                    column_start: None,
                                    column_end: None,
                                    replacement,
                                    description: "Replace list marker with dash".to_string(),
                                }),
                            });
                        }
                    } else {
                        first_marker = Some(current_marker);
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

    #[test]
    fn test_consistent_asterisk() {
        let content = "* Item 1\n* Item 2\n* Item 3";
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inconsistent_markers() {
        let content = "* Item 1\n+ Item 2\n- Item 3";
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2); // Second and third items differ from first
    }

    #[test]
    fn test_enforced_dash_style() {
        let content = "* Item 1\n- Item 2";
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let config = serde_json::json!({ "style": "dash" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1); // First item uses asterisk
    }

    #[test]
    fn test_nested_lists() {
        let content = "* Item 1\n  * Nested 1\n  * Nested 2\n* Item 2";
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // All use asterisk
    }

    #[test]
    fn test_list_markers_in_code_block_not_flagged() {
        // List markers inside fenced code blocks must not be checked.
        let content = "```\n* asterisk\n+ plus\n- dash\n```\n\n- real item\n";
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let config = serde_json::json!({ "style": "dash" });
        let violations = rule.check(&parser, Some(&config));

        // Only the real list item on the last line matters; the code block is ignored.
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_markdown_syntax_in_code_block() {
        let content = r#"# My Document

Here's a code block with markdown syntax:

```
- This looks like a list item
* This also looks like a list item
+ And this one too
```

* Real list item
"#;
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let violations = rule.check(&parser, None);

        // Should not flag list markers inside code blocks
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_indented_code_block() {
        let content = r#"Regular text

    - This is an indented code block
    * Not a real list
    + Just code

* Real list item
"#;
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let violations = rule.check(&parser, None);

        // Should not flag list markers in indented code blocks
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_dash_in_code_block_with_real_list() {
        let content = r#"* List item 1

```python
# Comment with -- dashes
value = 10 - 5  # subtraction
```

+ List item 2
"#;
        let parser = MarkdownParser::new(content);
        let rule = MD004;
        let violations = rule.check(&parser, None);

        // Should only flag the inconsistent list marker, not code content
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 8); // Line with "+ List item 2"
    }
}
