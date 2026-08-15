use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag};
use serde_json::Value;

pub struct MD042;

impl Rule for MD042 {
    fn name(&self) -> &'static str {
        "MD042"
    }

    fn description(&self) -> &'static str {
        "No empty links"
    }

    fn tags(&self) -> &[&str] {
        &["links"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        for (event, range) in parser.parse_with_offsets() {
            if let Event::Start(Tag::Link { dest_url, .. }) = event {
                // Check if the destination URL is empty or only contains "#"
                let url_str = dest_url.to_string();
                if url_str.is_empty() || url_str == "#" {
                    let line = parser.offset_to_line(range.start);
                    violations.push(Violation {
                        line,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "No empty links".to_owned(),
                        fix: None,
                    });
                }
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
    use crate::lint::rules::rendered;

    #[test]
    fn test_link_with_text() {
        let content = "[Link text](https://example.com)";
        let parser = MarkdownParser::new(content);
        let rule = MD042;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_empty_destination() {
        let content = "[Link text]()";
        let parser = MarkdownParser::new(content);
        let rule = MD042;
        let violations = rule.check(&parser, None);

        // AIDEV: the empty link starts at column 18, but MD042 reports every
        // violation at column 1, so the column cannot locate it on the line.
        assert_eq!(rendered(&violations), ["test.md:1:1: MD042 No empty links"]);
    }

    #[test]
    fn test_empty_text_non_empty_url() {
        let content = "[](https://example.com)";
        let parser = MarkdownParser::new(content);
        let rule = MD042;
        let violations = rule.check(&parser, None);

        // Empty link text is fine, MD042 is about empty destinations
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_empty_fragment() {
        let content = "[Link](#)";
        let parser = MarkdownParser::new(content);
        let rule = MD042;
        let violations = rule.check(&parser, None);

        // Empty fragment should trigger MD042
        assert_eq!(rendered(&violations), ["test.md:1:1: MD042 No empty links"]);
    }

    #[test]
    fn test_multiple_links() {
        let content = "[Good](url1) and [Bad]() and [Also good](url3)";
        let parser = MarkdownParser::new(content);
        let rule = MD042;
        let violations = rule.check(&parser, None);

        assert_eq!(rendered(&violations), ["test.md:1:1: MD042 No empty links"]);
    }
}
