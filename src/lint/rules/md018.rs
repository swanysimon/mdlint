use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD018;

impl Rule for MD018 {
    fn name(&self) -> &'static str {
        "MD018"
    }

    fn description(&self) -> &'static str {
        "No space after hash on atx style heading"
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

            // Check for ATX heading without space after hash
            if trimmed.starts_with('#') {
                // Count leading hashes
                let hash_count = trimmed.chars().take_while(|&c| c == '#').count();

                // Valid heading should have 1-6 hashes
                if hash_count > 0 && hash_count <= 6 {
                    // Check character after the hashes
                    if let Some(next_char) = trimmed.chars().nth(hash_count)
                        && !next_char.is_whitespace()
                        && next_char != '#'
                    {
                        // Insert space after the hashes
                        let hashes = "#".repeat(hash_count);
                        let rest = &trimmed[hash_count..];
                        let replacement = format!("{hashes} {rest}");

                        violations.push(Violation {
                            line: line_number,
                            column: Some(hash_count + 1),
                            rule: self.name().to_owned(),
                            message: "No space after hash on atx style heading".to_owned(),
                            fix: Some(Fix {
                                line_start: line_number,
                                line_end: line_number,
                                column_start: None,
                                column_end: None,
                                replacement,
                                description: "Add space after hash".to_owned(),
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

    fn apply_fixes(content: &str, violations: &[Violation]) -> String {
        let fixes: Vec<_> = violations.iter().filter_map(|v| v.fix.clone()).collect();
        Fixer::new()
            .apply_fixes_to_content(content, &fixes)
            .unwrap()
    }

    #[test]
    fn test_correct_spacing() {
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let parser = MarkdownParser::new(content);
        let rule = MD018;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_no_space_after_hash() {
        let content = "#Heading without space\n## Correct heading";
        let parser = MarkdownParser::new(content);
        let rule = MD018;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 1);
    }

    #[test]
    fn test_multiple_violations() {
        let content = "#First\n##Second\n### Correct";
        let parser = MarkdownParser::new(content);
        let rule = MD018;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_closed_heading() {
        let content = "## Closed heading ##";
        let parser = MarkdownParser::new(content);
        let rule = MD018;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_heading_in_code_block_not_flagged() {
        let content = "# Real heading\n\n```\n#NotAHeading\n```\n";
        let parser = MarkdownParser::new(content);
        let rule = MD018;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_fix_inserts_space_after_hash() {
        let content = "#Heading\n\n##Another\n";
        let parser = MarkdownParser::new(content);
        let rule = MD018;
        let violations = rule.check(&parser, None);
        assert_eq!(violations.len(), 2);
        let fixed = apply_fixes(content, &violations);
        assert_eq!(fixed, "# Heading\n\n## Another\n");
    }
}
