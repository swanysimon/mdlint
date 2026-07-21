use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD021;

impl Rule for MD021 {
    fn name(&self) -> &'static str {
        "MD021"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces inside hashes on closed atx style heading"
    }

    fn tags(&self) -> &[&str] {
        &["headings", "atx_closed", "spaces"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;
            let trimmed = line.trim();

            // Check if this is a closed ATX heading
            if trimmed.starts_with('#') && trimmed.ends_with('#') {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 && parts.last().unwrap().chars().all(|c| c == '#') {
                    let closing_hashes = parts.last().unwrap();

                    // Find position of closing hashes
                    if let Some(pos) = trimmed.rfind(closing_hashes) {
                        // Count spaces before closing hashes
                        let mut space_count = 0;
                        let mut check_pos = pos;
                        while check_pos > 0 {
                            check_pos -= 1;
                            if trimmed.chars().nth(check_pos) == Some(' ') {
                                space_count += 1;
                            } else {
                                break;
                            }
                        }

                        if space_count > 1 {
                            // Replace multiple spaces with single space
                            let before_spaces = &trimmed[..=check_pos];
                            let after_spaces = &trimmed[pos..];
                            let replacement = format!("{before_spaces} {after_spaces}");

                            violations.push(Violation {
                                line: line_number,
                                column: Some(1),
                                rule: self.name().to_string(),
                                message:
                                    "Multiple spaces inside hashes on closed atx style heading"
                                        .to_string(),
                                fix: Some(Fix {
                                    line_start: line_number,
                                    line_end: line_number,
                                    column_start: None,
                                    column_end: None,
                                    replacement,
                                    description: "Replace multiple spaces with single space"
                                        .to_string(),
                                }),
                            });
                        }
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
    fn test_single_space() {
        let content = "# Heading #";
        let parser = MarkdownParser::new(content);
        let rule = MD021;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Single space is OK for MD021
    }

    #[test]
    fn test_multiple_spaces() {
        let content = "# Heading  ##";
        let parser = MarkdownParser::new(content);
        let rule = MD021;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_no_space() {
        let content = "# Heading#";
        let parser = MarkdownParser::new(content);
        let rule = MD021;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_regular_heading() {
        let content = "# Heading";
        let parser = MarkdownParser::new(content);
        let rule = MD021;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Not closed
    }

    #[test]
    fn test_fix_collapses_spaces_before_closing_hashes() {
        let content = "# Heading  ##\n";
        let parser = MarkdownParser::new(content);
        let rule = MD021;
        let violations = rule.check(&parser, None);
        assert_eq!(violations.len(), 1);
        let fixed = apply_fixes(content, &violations);
        assert_eq!(fixed, "# Heading ##\n");
    }
}
