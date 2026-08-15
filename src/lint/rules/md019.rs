use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD019;

impl Rule for MD019 {
    fn name(&self) -> &'static str {
        "MD019"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after hash on atx style heading"
    }

    fn tags(&self) -> &[&str] {
        &["headings", "headers", "atx", "spaces"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let code_block_lines = parser.get_code_block_line_numbers();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;
            if code_block_lines.contains(&line_number) {
                continue;
            }
            let trimmed = line.trim();

            // Check for ATX heading with multiple spaces after hash
            if trimmed.starts_with('#') {
                // Count leading hashes
                let hash_count = trimmed.chars().take_while(|&c| c == '#').count();

                // Valid heading should have 1-6 hashes
                if hash_count > 0 && hash_count <= 6 && trimmed.len() > hash_count {
                    let after_hashes = &trimmed[hash_count..];

                    // Count spaces after hashes
                    let space_count = after_hashes.chars().take_while(|&c| c == ' ').count();

                    if space_count > 1 {
                        // Replace multiple spaces with single space
                        let hashes = "#".repeat(hash_count);
                        let rest = after_hashes[space_count..].trim_start();
                        let replacement = format!("{hashes} {rest}");

                        violations.push(Violation {
                            line: line_number,
                            column: Some(hash_count + 2),
                            rule: self.name().to_owned(),
                            message: format!(
                                "Multiple spaces after hash on atx style heading ({space_count} spaces)"
                            ),
                            fix: Some(Fix {
                                line_start: line_number,
                                line_end: line_number,
                                column_start: None,
                                column_end: None,
                                replacement,
                                description: "Replace multiple spaces with single space".to_owned(),
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
    fn test_correct_single_space() {
        let content = indoc! {"
            # Heading 1
            ## Heading 2
            ### Heading 3"};
        let parser = MarkdownParser::new(content);
        let rule = MD019;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_multiple_spaces() {
        let content = "#  Heading with 2 spaces\n## Correct heading";
        let parser = MarkdownParser::new(content);
        let rule = MD019;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:1:3: MD019 Multiple spaces after hash on atx style heading (2 spaces)"]
        );
    }

    #[test]
    fn test_many_spaces() {
        let content = "###     Heading with 5 spaces";
        let parser = MarkdownParser::new(content);
        let rule = MD019;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:1:5: MD019 Multiple spaces after hash on atx style heading (5 spaces)"]
        );
    }

    #[test]
    fn test_heading_in_code_block_not_flagged() {
        let content = indoc! {"
            # Real heading

            ```
            ##  WouldBeViolation
            ```
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD019;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_fix_collapses_multiple_spaces() {
        let content = indoc! {"
            #  Too many spaces

            ###   Even more
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD019;
        let violations = rule.check(&parser, None);
        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:3: MD019 Multiple spaces after hash on atx style heading (2 spaces)",
                "test.md:3:5: MD019 Multiple spaces after hash on atx style heading (3 spaces)",
            ]
        );
        let fixed = apply_fixes(content, &violations);
        assert_eq!(
            fixed,
            indoc! {"
                # Too many spaces

                ### Even more
            "}
        );
    }
}
