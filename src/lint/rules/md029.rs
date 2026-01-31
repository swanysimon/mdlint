use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
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
            .unwrap_or("one_or_ordered");

        let mut violations = Vec::new();
        let mut expected_num = 1;
        let mut in_ordered_list = false;
        let mut consecutive_blank_lines = 0;
        let mut seen_non_one = false; // Track if we've seen any number other than 1

        // Get code block lines to skip (not inline code, which can appear in list items)
        let code_lines = parser.get_code_block_line_numbers();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            // Skip if line is in a code block or inline code
            // Reset consecutive blank lines when encountering code blocks
            if code_lines.contains(&line_number) {
                consecutive_blank_lines = 0;
                continue;
            }

            let trimmed = line.trim_start();

            // Track blank lines (only count actual blank lines, not code blocks)
            if line.trim().is_empty() {
                consecutive_blank_lines += 1;
                // Only reset after 2+ consecutive blank lines
                if consecutive_blank_lines >= 2 {
                    in_ordered_list = false;
                    expected_num = 1;
                    seen_non_one = false;
                }
                continue;
            } else {
                consecutive_blank_lines = 0;
            }

            // Check if this is an ordered list item
            if let Some(dot_pos) = trimmed.find('.') {
                let prefix = &trimmed[..dot_pos];
                if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(num) = prefix.parse::<usize>() {
                        if !in_ordered_list {
                            in_ordered_list = true;
                            expected_num = 1;
                            seen_non_one = false;
                        }

                        // Track if we've seen a non-1 number in this list
                        if num != 1 {
                            seen_non_one = true;
                        }

                        let is_valid = match style {
                            "one" => num == 1,
                            "ordered" => num == expected_num,
                            _ => {
                                // "one_or_ordered": if we've seen non-1, must be sequential
                                // otherwise, allow either 1 or expected
                                if seen_non_one {
                                    num == expected_num
                                } else {
                                    num == 1 || num == expected_num
                                }
                            }
                        };

                        if !is_valid {
                            let should_be = match style {
                                "one" => 1,
                                _ => expected_num,
                            };
                            violations.push(Violation {
                                line: line_number,
                                column: Some(line.len() - trimmed.len() + 1),
                                rule: self.name().to_string(),
                                message: format!(
                                    "Ordered list item prefix: expected {}, found {}",
                                    should_be, num
                                ),
                                fix: None,
                            });
                        }

                        // Increment expected based on what we EXPECTED, not what we saw
                        // This ensures violations continue to be detected
                        expected_num += 1;
                    }
                } else {
                    // Line has a dot but isn't a list item
                    in_ordered_list = false;
                    expected_num = 1;
                    seen_non_one = false;
                }
            } else if !trimmed.starts_with("*")
                && !trimmed.starts_with("+")
                && !trimmed.starts_with("-")
            {
                // Non-list line that's not blank
                in_ordered_list = false;
                expected_num = 1;
                seen_non_one = false;
            }
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
    fn test_ordered_sequence() {
        let content = "1. First\n2. Second\n3. Third";
        let parser = MarkdownParser::new(content);
        let rule = MD029;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_all_ones() {
        let content = "1. First\n1. Second\n1. Third";
        let parser = MarkdownParser::new(content);
        let rule = MD029;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Default allows "one" style
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
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0, "List with backticks should be valid");
    }
}
