use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD035;

impl Rule for MD035 {
    fn name(&self) -> &'static str {
        "MD035"
    }

    fn description(&self) -> &'static str {
        "Horizontal rule style"
    }

    fn tags(&self) -> &[&str] {
        &["hr"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let style = config
            .and_then(|c| c.get("style"))
            .and_then(|v| v.as_str())
            .unwrap_or("---");

        let mut violations = Vec::new();
        let mut first_hr_style: Option<String> = None;
        let code_block_lines = parser.get_code_block_line_numbers();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;
            if code_block_lines.contains(&line_number) {
                continue;
            }
            let trimmed = line.trim();

            // Check if line is a horizontal rule (3+ of same char: - * _)
            if is_horizontal_rule(trimmed) {
                let current_style = get_hr_style(trimmed);

                if style == "consistent" {
                    // Track first style and ensure consistency
                    if let Some(first_style) = &first_hr_style {
                        if &current_style != first_style {
                            violations.push(Violation {
                                line: line_number,
                                column: Some(1),
                                rule: self.name().to_owned(),
                                message: format!(
                                    "Horizontal rule style should be consistent: expected {first_style}, found {current_style}"
                                ),
                                fix: Some(Fix {
                                    line_start: line_number,
                                    line_end: line_number,
                                    column_start: None,
                                    column_end: None,
                                    replacement: "---".to_owned(),
                                    description: "Replace with canonical horizontal rule".to_owned(),
                                }),
                            });
                        }
                    } else {
                        first_hr_style = Some(current_style);
                    }
                } else if current_style != style {
                    violations.push(Violation {
                        line: line_number,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: format!(
                            "Horizontal rule style should be '{style}', found '{current_style}'"
                        ),
                        fix: Some(Fix {
                            line_start: line_number,
                            line_end: line_number,
                            column_start: None,
                            column_end: None,
                            replacement: style.to_owned(),
                            description: "Replace with required horizontal rule style".to_owned(),
                        }),
                    });
                }
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        true
    }
}

fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }

    // Check for 3+ dashes, asterisks, or underscores (possibly with spaces)
    let chars: Vec<char> = trimmed.chars().filter(|&c| c != ' ').collect();
    if chars.len() < 3 {
        return false;
    }

    let first_char = chars.first().copied().expect("len >= 3 checked");
    if first_char != '-' && first_char != '*' && first_char != '_' {
        return false;
    }

    chars.iter().all(|&c| c == first_char)
}

fn get_hr_style(line: &str) -> String {
    line.trim().to_owned()
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
    fn test_consistent_style() {
        let content = indoc! {"
            ---

            Content

            ---"};
        let parser = MarkdownParser::new(content);
        let rule = MD035;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inconsistent_style() {
        let content = indoc! {"
            ---

            Content

            ***"};
        let parser = MarkdownParser::new(content);
        let rule = MD035;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:5:1: MD035 Horizontal rule style should be '---', found '***'"]
        );
    }

    #[test]
    fn test_enforced_style() {
        let content = indoc! {"
            ***

            Content"};
        let parser = MarkdownParser::new(content);
        let rule = MD035;
        let config = serde_json::json!({ "style": "---" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD035 Horizontal rule style should be '---', found '***'"]
        );
    }

    #[test]
    fn test_hr_in_code_block_not_flagged() {
        let content = indoc! {"
            ---

            ```markdown
            =========
            ```
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD035;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_with_spaces() {
        let content = indoc! {"
            * * *

            Content

            * * *"};
        let parser = MarkdownParser::new(content);
        let rule = MD035;
        let config = serde_json::json!({ "style": "consistent" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0); // Consistent
    }

    #[test]
    fn test_fix_normalises_hr_to_dashes() {
        let content = indoc! {"
            ***

            Content
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD035;
        let config = serde_json::json!({ "style": "---" });
        let violations = rule.check(&parser, Some(&config));
        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD035 Horizontal rule style should be '---', found '***'"]
        );
        let fixed = apply_fixes(content, &violations);
        assert_eq!(
            fixed,
            indoc! {"
                ---

                Content
            "}
        );
    }
}
