use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD020;

impl Rule for MD020 {
    fn name(&self) -> &'static str {
        "MD020"
    }

    fn description(&self) -> &'static str {
        "No space inside hashes on closed atx style heading"
    }

    fn tags(&self) -> &[&str] {
        &["headings", "atx_closed", "spaces"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;
            let trimmed = line.trim();

            // Check if this is a closed ATX heading (starts and ends with #)
            if trimmed.starts_with('#') && trimmed.ends_with('#') {
                // Count opening hashes
                let opening_hashes = trimmed.chars().take_while(|&c| c == '#').count();

                // Count closing hashes
                let closing_hashes = trimmed.chars().rev().take_while(|&c| c == '#').count();

                // Make sure there's content between opening and closing hashes
                if opening_hashes + closing_hashes < trimmed.len() {
                    // Get the character before the closing hashes
                    let chars: Vec<char> = trimmed.chars().collect();
                    let pos_before_closing = chars.len() - closing_hashes - 1;

                    if chars[pos_before_closing] != ' ' {
                        // Insert space before closing hashes
                        let before_closing: String = chars[..=pos_before_closing].iter().collect();
                        let closing: String = chars[(pos_before_closing + 1)..].iter().collect();
                        let replacement = format!("{before_closing} {closing}");

                        violations.push(Violation {
                            line: line_number,
                            column: Some(1),
                            rule: self.name().to_string(),
                            message: "No space inside hashes on closed atx style heading"
                                .to_string(),
                            fix: Some(Fix {
                                line_start: line_number,
                                line_end: line_number,
                                column_start: None,
                                column_end: None,
                                replacement,
                                description: "Add space before closing hashes".to_string(),
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
    fn test_correct_closed_heading() {
        let content = "# Heading #";
        let parser = MarkdownParser::new(content);
        let rule = MD020;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Has space, so it's correct
    }

    #[test]
    fn test_no_space_before_closing() {
        let content = "# Heading#";
        let parser = MarkdownParser::new(content);
        let rule = MD020;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1); // No space, violation
    }

    #[test]
    fn test_regular_heading() {
        let content = "# Heading";
        let parser = MarkdownParser::new(content);
        let rule = MD020;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Not a closed heading
    }

    #[test]
    fn test_multiple_levels() {
        let content = "## Heading##\n### Another###";
        let parser = MarkdownParser::new(content);
        let rule = MD020;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2); // Both missing spaces
    }

    #[test]
    fn test_fix_inserts_space_before_closing_hashes() {
        let content = "# Heading#\n";
        let parser = MarkdownParser::new(content);
        let rule = MD020;
        let violations = rule.check(&parser, None);
        assert_eq!(violations.len(), 1);
        let fixed = apply_fixes(content, &violations);
        assert_eq!(fixed, "# Heading #\n");
    }
}
