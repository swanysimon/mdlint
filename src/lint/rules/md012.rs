use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD012;

impl Rule for MD012 {
    fn name(&self) -> &str {
        "MD012"
    }

    fn description(&self) -> &str {
        "Multiple consecutive blank lines"
    }

    fn tags(&self) -> &[&str] {
        &["whitespace", "blank_lines"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let maximum = config
            .and_then(|c| c.get("maximum"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;

        let mut violations = Vec::new();
        let mut consecutive_blank = 0;
        let mut blank_start_line = 0;

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            if line.trim().is_empty() {
                if consecutive_blank == 0 {
                    blank_start_line = line_number;
                }
                consecutive_blank += 1;
            } else {
                if consecutive_blank > maximum {
                    // Report a violation for each excess blank line
                    for i in maximum..consecutive_blank {
                        violations.push(Violation {
                            line: blank_start_line + i,
                            column: Some(1),
                            rule: self.name().to_string(),
                            message: format!(
                                "{} [Expected: {}; Actual: {}]",
                                self.description().to_string(), 1, consecutive_blank
                            ),
                            fix: Some(Fix {
                                line_start: blank_start_line + i,
                                line_end: blank_start_line + i,
                                column_start: None,
                                column_end: None,
                                replacement: String::new(),
                                description: "Remove excess blank line".to_string(),
                            }),
                        });
                    }
                }
                consecutive_blank = 0;
            }
        }

        // Check if file ends with too many blank lines
        if consecutive_blank > maximum {
            // Report a violation for each excess blank line
            for i in maximum..consecutive_blank {
                violations.push(Violation {
                    line: blank_start_line + i,
                    column: Some(1),
                    rule: self.name().to_string(),
                    message: format!("Expected: {}; Actual: {}", 1, consecutive_blank),
                    fix: Some(Fix {
                        line_start: blank_start_line + i,
                        line_end: blank_start_line + i,
                        column_start: None,
                        column_end: None,
                        replacement: String::new(),
                        description: "Remove excess blank line".to_string(),
                    }),
                });
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

    fn apply_fixes(content: &str, violations: &[Violation]) -> String {
        let fixes: Vec<_> = violations.iter().filter_map(|v| v.fix.clone()).collect();
        Fixer::new()
            .apply_fixes_to_content(content, &fixes)
            .unwrap()
    }

    #[test]
    fn test_no_consecutive_blanks() {
        let content = "Line 1\n\nLine 2\n\nLine 3";
        let parser = MarkdownParser::new(content);
        let rule = MD012;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_multiple_consecutive_blanks() {
        let content = "Line 1\n\n\nLine 2";
        let parser = MarkdownParser::new(content);
        let rule = MD012;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 3); // Third line is the excess blank
    }

    #[test]
    fn test_custom_maximum() {
        let content = "Line 1\n\n\nLine 2";
        let parser = MarkdownParser::new(content);
        let rule = MD012;
        let config = serde_json::json!({ "maximum": 2 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0); // 2 blank lines allowed
    }

    #[test]
    fn test_trailing_blank_lines() {
        let content = "Line 1\n\n\n";
        let parser = MarkdownParser::new(content);
        let rule = MD012;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_fix_removes_excess_blank_line() {
        let content = "Line 1\n\n\nLine 2\n";
        let parser = MarkdownParser::new(content);
        let rule = MD012;
        let violations = rule.check(&parser, None);
        assert_eq!(violations.len(), 1);
        let fixed = apply_fixes(content, &violations);
        assert_eq!(fixed, "Line 1\n\nLine 2\n");
    }
}
