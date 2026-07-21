use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag, TagEnd};
use serde_json::Value;

pub struct MD059;

impl Rule for MD059 {
    fn name(&self) -> &'static str {
        "MD059"
    }

    fn description(&self) -> &'static str {
        "Link text should be descriptive"
    }

    fn tags(&self) -> &[&str] {
        &["links", "accessibility"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        // List of non-descriptive link texts
        let non_descriptive = [
            "click here",
            "here",
            "link",
            "read more",
            "more",
            "this",
            "this link",
            "click",
        ];

        let mut in_link = false;
        let mut link_text = String::new();
        let mut link_line = 0;

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::Link { .. }) => {
                    in_link = true;
                    link_text.clear();
                    link_line = parser.offset_to_line(range.start);
                }
                Event::Text(text) if in_link => {
                    link_text.push_str(&text);
                }
                Event::End(TagEnd::Link) if in_link => {
                    let text_lower = link_text.trim().to_lowercase();

                    // Check if link text is non-descriptive
                    if non_descriptive.contains(&text_lower.as_str()) {
                        violations.push(Violation {
                            line: link_line,
                            column: Some(1),
                            rule: self.name().to_string(),
                            message: format!(
                                "Link text '{}' is not descriptive; use meaningful text",
                                link_text.trim()
                            ),
                            fix: None,
                        });
                    }

                    in_link = false;
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
    fn test_descriptive_link() {
        let content = "See the [documentation](https://example.com) for details.";
        let parser = MarkdownParser::new(content);
        let rule = MD059;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_click_here() {
        let content = "[Click here](https://example.com) to continue.";
        let parser = MarkdownParser::new(content);
        let rule = MD059;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("Click here"));
    }

    #[test]
    fn test_here() {
        let content = "You can find it [here](https://example.com).";
        let parser = MarkdownParser::new(content);
        let rule = MD059;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_multiple_bad_links() {
        let content = "[Click here](url1) and [read more](url2).";
        let parser = MarkdownParser::new(content);
        let rule = MD059;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_case_insensitive() {
        let content = "[CLICK HERE](https://example.com) is bad.";
        let parser = MarkdownParser::new(content);
        let rule = MD059;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }
}
