use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;

pub struct MD058;

impl Rule for MD058 {
    fn name(&self) -> &'static str {
        "MD058"
    }

    fn description(&self) -> &'static str {
        "Tables should be surrounded by blank lines"
    }

    fn tags(&self) -> &[&str] {
        &["table", "blank_lines"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let lines = parser.lines();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Check if this looks like a table start
            if line.contains('|') && !is_separator_line(line) {
                // Check if next line is separator (confirming this is a table)
                if i + 1 < lines.len() && is_separator_line(lines[i + 1].trim()) {
                    // Found start of table, check for blank line before
                    if i > 0 && !lines[i - 1].trim().is_empty() {
                        violations.push(Violation {
                            line: i + 1,
                            column: Some(1),
                            rule: self.name().to_string(),
                            message: "Table should be surrounded by blank lines".to_string(),
                            fix: None,
                        });
                    }

                    // Skip to end of table (but don't check for blank line after,
                    // since Markdown spec treats following text as part of the table)
                    i += 2; // Skip header and separator
                    while i < lines.len() {
                        let current = lines[i].trim();
                        if !current.contains('|') {
                            break;
                        }
                        i += 1;
                    }

                    continue;
                }
            }

            i += 1;
        }

        violations
    }

    fn fixable(&self) -> bool {
        false
    }
}

/// Check if a line is a table separator (contains ---)
fn is_separator_line(line: &str) -> bool {
    let trimmed = line.trim();
    (trimmed.contains("---") || trimmed.contains(":--") || trimmed.contains("--:"))
        && trimmed.contains('|')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_with_blank_lines() {
        let content = "Text before\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD058;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_table_without_blank_before() {
        let content = "Text before\n| A | B |\n|---|---|\n| 1 | 2 |\n\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD058;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_table_without_blank_after() {
        let content = "Text before\n\n| A | B |\n|---|---|\n| 1 | 2 |\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD058;
        let violations = rule.check(&parser, None);

        // Markdown spec treats text after table as part of the table, so no violation
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_table_without_any_blank_lines() {
        let content = "Text before\n| A | B |\n|---|---|\n| 1 | 2 |\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD058;
        let violations = rule.check(&parser, None);

        // Only violation is missing blank line before table
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_table_at_start() {
        let content = "| A | B |\n|---|---|\n| 1 | 2 |\n\nText after";
        let parser = MarkdownParser::new(content);
        let rule = MD058;
        let violations = rule.check(&parser, None);

        // No blank line before is okay at start of document
        assert_eq!(violations.len(), 0);
    }
}
