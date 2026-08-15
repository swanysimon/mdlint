use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;
use std::collections::HashMap;

pub struct MD005;

impl Rule for MD005 {
    fn name(&self) -> &'static str {
        "MD005"
    }

    fn description(&self) -> &'static str {
        "Inconsistent indentation for list items at the same level"
    }

    fn tags(&self) -> &[&str] {
        &["bullet", "ul", "indentation"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let mut level_indents: HashMap<usize, usize> = HashMap::new();
        let mut prev_indent = 0;
        let mut current_level = 0;

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            // Check if this is a list item
            let is_list_item = line.trim_start().starts_with("* ")
                || line.trim_start().starts_with("+ ")
                || line.trim_start().starts_with("- ")
                || line
                    .trim_start()
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit());

            if !is_list_item {
                continue;
            }

            // Calculate indentation
            let indent = line.len() - line.trim_start().len();

            // Determine list level based on indentation
            // Only consider it a new level if indented by at least 2 spaces more
            if indent >= prev_indent + 2 {
                current_level += 1;
            } else if indent < prev_indent {
                // Find the level for this indentation
                current_level = level_indents
                    .iter()
                    .filter(|(_, i)| **i == indent)
                    .map(|(l, _)| *l)
                    .next()
                    .unwrap_or(0);
            }
            // If indent is between prev_indent and prev_indent + 2, stay at same level

            // Check if this level has a recorded indentation
            if let Some(&expected_indent) = level_indents.get(&current_level) {
                if indent != expected_indent {
                    violations.push(Violation {
                        line: line_number,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: format!(
                            "List item indentation mismatch: expected {expected_indent} spaces, found {indent}"
                        ),
                        fix: None,
                    });
                }
            } else {
                // Record this level's indentation
                level_indents.insert(current_level, indent);
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
    use indoc::indoc;

    #[test]
    fn test_consistent_indentation() {
        let content = indoc! {"
            * Item 1
            * Item 2
              * Nested 1
              * Nested 2
            * Item 3"};
        let parser = MarkdownParser::new(content);
        let rule = MD005;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inconsistent_indentation() {
        let content = indoc! {"
            * Item 1
             * Item 2 - wrong indent
            * Item 3"};
        let parser = MarkdownParser::new(content);
        let rule = MD005;
        let violations = rule.check(&parser, None);

        assert!(!violations.is_empty());
    }

    #[test]
    fn test_ordered_list() {
        let content = indoc! {"
            1. Item 1
            2. Item 2
            3. Item 3"};
        let parser = MarkdownParser::new(content);
        let rule = MD005;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }
}
