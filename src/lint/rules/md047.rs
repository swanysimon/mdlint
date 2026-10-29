use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD047;

impl Rule for MD047 {
    fn name(&self) -> &'static str {
        "MD047"
    }

    fn description(&self) -> &'static str {
        "Files should end with a single newline character"
    }

    fn tags(&self) -> &[&str] {
        &["blank_lines"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let content = parser.content();

        if content.is_empty() {
            return violations;
        }

        let lines = parser.lines();

        // Check if file ends with a newline
        if !content.ends_with('\n') {
            // Missing newline at end
            let last_line = lines.last().unwrap_or(&"");
            violations.push(Violation {
                line: lines.len(),
                column: Some(1),
                rule: self.name().to_owned(),
                message: "Files should end with a single newline character".to_owned(),
                fix: Some(Fix {
                    line_start: lines.len(),
                    line_end: lines.len(),
                    column_start: None,
                    column_end: None,
                    replacement: format!("{last_line}\n"),
                    description: "Add newline at end of file".to_owned(),
                }),
            });
        } else if content.ends_with("\n\n")
            // Blank lines at the end of an unclosed code block are its content.
            && !parser.get_code_block_line_numbers().contains(&lines.len())
        {
            // Multiple trailing newlines - count them
            let trailing_newlines = content.chars().rev().take_while(|&c| c == '\n').count();

            if trailing_newlines > 1 {
                // Remove all but one newline. `lines()` yields one empty line per
                // newline after the first: "text\n\n" is ["text", ""].
                let last_content_line_idx = lines.len().saturating_sub(trailing_newlines - 1);
                let last_content_line = if last_content_line_idx > 0 {
                    lines.get(last_content_line_idx - 1).unwrap_or(&"")
                } else {
                    ""
                };

                violations.push(Violation {
                    line: lines.len(),
                    column: Some(1),
                    rule: self.name().to_owned(),
                    message: "Files should end with a single newline character".to_owned(),
                    fix: Some(Fix {
                        line_start: last_content_line_idx.max(1),
                        line_end: lines.len(),
                        column_start: None,
                        column_end: None,
                        replacement: if last_content_line_idx > 0 {
                            format!("{last_content_line}\n")
                        } else {
                            "\n".to_owned()
                        },
                        description: "Remove extra newlines at end of file".to_owned(),
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
    use crate::lint::rules::rendered;
    use indoc::indoc;

    fn apply_fixes(content: &str, violations: &[Violation]) -> String {
        let fixes: Vec<_> = violations.iter().filter_map(|v| v.fix.clone()).collect();
        Fixer::new()
            .apply_fixes_to_content(content, &fixes)
            .unwrap()
    }

    #[test]
    fn test_single_newline() {
        let content = indoc! {"
            # Heading

            Content
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD047;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_no_newline() {
        let content = indoc! {"
            # Heading

            Content"};
        let parser = MarkdownParser::new(content);
        let rule = MD047;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:3:1: MD047 Files should end with a single newline character"]
        );
    }

    #[test]
    fn test_multiple_newlines() {
        let content = indoc! {"
            # Heading

            Content

        "};
        let parser = MarkdownParser::new(content);
        let rule = MD047;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:4:1: MD047 Files should end with a single newline character"]
        );
    }

    #[test]
    fn test_empty_file() {
        let content = "";
        let parser = MarkdownParser::new(content);
        let rule = MD047;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Empty file is OK
    }

    #[test]
    fn test_fix_adds_trailing_newline() {
        let content = indoc! {"
            # Heading

            Content"};
        let parser = MarkdownParser::new(content);
        let rule = MD047;
        let violations = rule.check(&parser, None);
        assert_eq!(
            rendered(&violations),
            ["test.md:3:1: MD047 Files should end with a single newline character"]
        );
        let fixed = apply_fixes(content, &violations);
        assert_eq!(
            fixed,
            indoc! {"
                # Heading

                Content
            "}
        );
    }

    #[test]
    fn test_fix_removes_extra_trailing_newlines_and_keeps_content() {
        for (content, expected) in [
            ("# Heading\n\nContent\n\n", "# Heading\n\nContent\n"),
            ("# Heading\n\nContent\n\n\n\n", "# Heading\n\nContent\n"),
            ("Content\n\n", "Content\n"),
        ] {
            let parser = MarkdownParser::new(content);
            let violations = MD047.check(&parser, None);
            assert_eq!(apply_fixes(content, &violations), expected, "{content:?}");
        }
    }

    #[test]
    fn test_blank_lines_ending_an_unclosed_code_block_are_content() {
        let content = "# Heading\n\n```\ncode\n\n";
        let parser = MarkdownParser::new(content);

        assert_eq!(MD047.check(&parser, None).len(), 0);
    }
}
