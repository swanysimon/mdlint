use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD023;

impl Rule for MD023 {
    fn name(&self) -> &'static str {
        "MD023"
    }

    fn description(&self) -> &'static str {
        "Headings must start at the beginning of the line"
    }

    fn tags(&self) -> &[&str] {
        &["headings", "headers", "spaces"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let code_block_lines = parser.get_code_block_line_numbers();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;
            if code_block_lines.contains(&line_number) {
                continue;
            }

            // Check if line starts with whitespace followed by hash
            if line.starts_with(' ') || line.starts_with('\t') {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') {
                    // Count leading hashes to verify it's a heading
                    let hash_count = trimmed.chars().take_while(|&c| c == '#').count();

                    // Valid heading should have 1-6 hashes
                    if hash_count > 0 && hash_count <= 6 {
                        let indent = line.len() - trimmed.len();
                        // Remove leading whitespace
                        violations.push(Violation {
                            line: line_number,
                            column: Some(1),
                            rule: self.name().to_owned(),
                            message: format!(
                                "Heading must start at the beginning of the line ({indent} space(s) before)"
                            ),
                            fix: Some(Fix {
                                line_start: line_number,
                                line_end: line_number,
                                column_start: None,
                                column_end: None,
                                replacement: trimmed.to_owned(),
                                description: "Remove leading whitespace".to_owned(),
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
    fn test_correct_headings() {
        let content = indoc! {"
            # Heading 1
            ## Heading 2
            ### Heading 3"};
        let parser = MarkdownParser::new(content);
        let rule = MD023;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_indented_heading() {
        let content = " # Heading with space\n## Correct heading";
        let parser = MarkdownParser::new(content);
        let rule = MD023;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:1: MD023 Heading must start at the beginning of the line (1 space(s) before)"
            ]
        );
    }

    #[test]
    fn test_tab_indented() {
        // A leading tab expands to 4 spaces in CommonMark, making this an indented
        // code block rather than a heading. MD023 must not flag it.
        let content = "\t# Heading with tab";
        let parser = MarkdownParser::new(content);
        let rule = MD023;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_four_spaces_is_code_not_flagged() {
        // Four leading spaces = indented code block in CommonMark; not a heading.
        let content = "    # Heading with 4 spaces";
        let parser = MarkdownParser::new(content);
        let rule = MD023;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_two_spaces_is_flagged() {
        // 1-3 leading spaces: CommonMark still parses this as an ATX heading.
        let content = "  # Heading with 2 spaces";
        let parser = MarkdownParser::new(content);
        let rule = MD023;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:1: MD023 Heading must start at the beginning of the line (2 space(s) before)"
            ]
        );
    }

    #[test]
    fn test_fix_removes_leading_whitespace() {
        let content = indoc! {"
             # Indented heading

            Paragraph.
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD023;
        let violations = rule.check(&parser, None);
        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:1: MD023 Heading must start at the beginning of the line (1 space(s) before)"
            ]
        );
        let fixed = apply_fixes(content, &violations);
        assert_eq!(
            fixed,
            indoc! {"
                # Indented heading

                Paragraph.
            "}
        );
    }
}
