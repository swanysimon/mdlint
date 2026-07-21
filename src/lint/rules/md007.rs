use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
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
        let mut list_depth = 0;
        let mut prev_indent = 0;
        let code_block_lines = parser.get_code_block_line_numbers();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            if code_block_lines.contains(&line_number) {
                continue;
            }

            let trimmed = line.trim_start();

            // Check if this is an unordered list item
            let is_ul_item =
                trimmed.starts_with("* ") || trimmed.starts_with("+ ") || trimmed.starts_with("- ");

            if !is_ul_item {
                if !line.trim().is_empty() && !trimmed.starts_with("  ") {
                    // Reset depth when we leave the list
                    list_depth = 0;
                    prev_indent = 0;
                }
                continue;
            }

            // Calculate indentation
            let indent = line.len() - trimmed.len();

            // Determine expected indentation based on depth
            if indent > prev_indent {
                // Going deeper
                list_depth += 1;
            } else if indent < prev_indent {
                // Going shallower
                list_depth = indent / indent_size;
            }

            let expected_indent = list_depth * indent_size;

            if indent != expected_indent {
                violations.push(Violation {
                    line: line_number,
                    column: Some(1),
                    rule: self.name().to_string(),
                    message: format!(
                        "Unordered list indentation should be {expected_indent} spaces (found {indent})"
                    ),
                    fix: None,
                });
            }

            prev_indent = indent;
        }

        violations
    }

    fn fixable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correct_indentation() {
        let content = "* Item 1\n  * Nested 1\n    * Double nested\n  * Nested 2\n* Item 2";
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

        assert!(!violations.is_empty());
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
        let content = "* Level 1\n  * Level 2\n    * Level 3\n      * Level 4";
        let parser = MarkdownParser::new(content);
        let rule = MD007;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }
}
