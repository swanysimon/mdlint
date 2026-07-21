use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag, TagEnd};
use serde_json::Value;

pub struct MD026;

impl Rule for MD026 {
    fn name(&self) -> &'static str {
        "MD026"
    }

    fn description(&self) -> &'static str {
        "Trailing punctuation in heading"
    }

    fn tags(&self) -> &[&str] {
        &["headings"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let punctuation = config
            .and_then(|c| c.get("punctuation"))
            .and_then(|v| v.as_str())
            .unwrap_or(".,;:!");

        let mut violations = Vec::new();
        let mut in_heading = false;
        let mut current_heading_text = String::new();
        let mut current_heading_line = 0;

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::Heading { .. }) => {
                    in_heading = true;
                    current_heading_text.clear();
                    current_heading_line = parser.offset_to_line(range.start);
                }
                Event::Text(text) if in_heading => {
                    current_heading_text.push_str(&text);
                }
                Event::Code(text) if in_heading => {
                    current_heading_text.push_str(&text);
                }
                Event::End(TagEnd::Heading(_)) if in_heading => {
                    let trimmed = current_heading_text.trim();
                    if let Some(last_char) = trimmed.chars().last()
                        && punctuation.contains(last_char)
                    {
                        violations.push(Violation {
                            line: current_heading_line,
                            column: Some(1),
                            rule: self.name().to_string(),
                            message: format!("Trailing punctuation in heading: '{last_char}'"),
                            fix: None,
                        });
                    }
                    in_heading = false;
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
    fn test_no_trailing_punctuation() {
        let content = "# Heading\n## Another Heading";
        let parser = MarkdownParser::new(content);
        let rule = MD026;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_trailing_period() {
        let content = "# Heading.";
        let parser = MarkdownParser::new(content);
        let rule = MD026;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("'.'"));
    }

    #[test]
    fn test_trailing_question() {
        let content = "## What is this?";
        let parser = MarkdownParser::new(content);
        let rule = MD026;
        let violations = rule.check(&parser, None);

        // Question marks are not in the default punctuation set
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_custom_punctuation() {
        let content = "# Heading!";
        let parser = MarkdownParser::new(content);
        let rule = MD026;
        let config = serde_json::json!({ "punctuation": "." });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0); // ! not in custom punctuation list
    }
}
