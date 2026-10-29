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
                                    replacement: dash_rule_replacement(parser, line_number, "---"),
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
                            replacement: dash_rule_replacement(parser, line_number, style),
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

/// The replacement for a rule on `line_number`, with a blank line above where
/// `mdlint format` puts one, for the same reasons:
/// - a dash rule directly under paragraph text would be a setext underline,
///   turning the paragraph into a heading;
/// - a `---` on line 1 with another `---` below it would open front matter.
fn dash_rule_replacement(parser: &MarkdownParser, line_number: usize, style: &str) -> String {
    let under_text = line_number > 1 && !parser.is_blank_line(line_number - 1);
    let opens_front_matter = line_number == 1
        && style == "---"
        && parser
            .lines()
            .iter()
            .skip(1)
            .any(|line| *line == "---" || is_horizontal_rule(line.trim()));
    if style.starts_with('-') && (under_text || opens_front_matter) {
        format!("\n{style}")
    } else {
        style.to_owned()
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

    #[test]
    fn test_fix_keeps_rule_under_paragraph_a_rule() {
        // `***` under text is a thematic break; `---` there would be a setext
        // underline, so the fix adds the blank line `mdlint format` adds.
        let content = "Paragraph\n***\nMore\n";
        let parser = MarkdownParser::new(content);
        let violations = MD035.check(&parser, None);

        assert_eq!(
            apply_fixes(content, &violations),
            "Paragraph\n\n---\nMore\n"
        );
        assert_eq!(
            crate::formatter::format(&apply_fixes(content, &violations)),
            crate::formatter::format(content)
        );
    }

    #[test]
    fn test_fix_never_opens_front_matter() {
        let content = "***\n\nIntro\n\n***\n";
        let parser = MarkdownParser::new(content);
        let fixed = apply_fixes(content, &MD035.check(&parser, None));

        assert_eq!(fixed, "\n---\n\nIntro\n\n---\n");
        assert_eq!(
            crate::formatter::format(&fixed),
            crate::formatter::format(content)
        );
    }
}
