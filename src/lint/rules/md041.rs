use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, HeadingLevel, Tag};
use serde_json::Value;

pub struct MD041;

impl Rule for MD041 {
    fn name(&self) -> &'static str {
        "MD041"
    }

    fn description(&self) -> &'static str {
        "First line in file should be a top-level heading"
    }

    fn tags(&self) -> &[&str] {
        &["headings"]
    }

    #[allow(clippy::cast_possible_truncation)] // serde_json gives u64; heading level is always ≤ 6
    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let level = config
            .and_then(|c| c.get("level"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1) as usize;

        let expected_level = match level {
            2 => HeadingLevel::H2,
            3 => HeadingLevel::H3,
            4 => HeadingLevel::H4,
            5 => HeadingLevel::H5,
            6 => HeadingLevel::H6,
            _ => HeadingLevel::H1,
        };

        // Check if first non-blank line is the expected heading level
        let found_first_heading = false;

        for (event, range) in parser.parse_with_offsets() {
            // Skip blank/empty events at start
            match event {
                Event::Start(Tag::Heading { level, .. }) if !found_first_heading => {
                    let heading_line = parser.offset_to_line(range.start);

                    if level != expected_level {
                        violations.push(Violation {
                            line: heading_line,
                            column: Some(1),
                            rule: self.name().to_owned(),
                            message: format!(
                                "First line in file should be a level {} heading",
                                match expected_level {
                                    HeadingLevel::H1 => 1u8,
                                    HeadingLevel::H2 => 2u8,
                                    HeadingLevel::H3 => 3u8,
                                    HeadingLevel::H4 => 4u8,
                                    HeadingLevel::H5 => 5u8,
                                    HeadingLevel::H6 => 6u8,
                                }
                            ),
                            fix: None,
                        });
                    }
                    break;
                }
                Event::Text(_) | Event::Code(_) | Event::Start(Tag::Paragraph)
                    if !found_first_heading =>
                {
                    // Non-heading content found first
                    violations.push(Violation {
                        line: 1,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "First line in file should be a top-level heading".to_owned(),
                        fix: None,
                    });
                    break;
                }
                _ => {}
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starts_with_h1() {
        let content = "# Heading\n\nContent";
        let parser = MarkdownParser::new(content);
        let rule = MD041;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_starts_with_text() {
        let content = "Some text\n\n# Heading";
        let parser = MarkdownParser::new(content);
        let rule = MD041;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_starts_with_h2() {
        let content = "## Heading\n\nContent";
        let parser = MarkdownParser::new(content);
        let rule = MD041;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1); // Should be H1
    }

    #[test]
    fn test_blank_lines_before_heading() {
        let content = "\n\n# Heading\n\nContent";
        let parser = MarkdownParser::new(content);
        let rule = MD041;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Blank lines are OK
    }
}
