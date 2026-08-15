use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag, TagEnd};
use serde_json::Value;

pub struct MD045;

impl Rule for MD045 {
    fn name(&self) -> &'static str {
        "MD045"
    }

    fn description(&self) -> &'static str {
        "Images should have alternate text (alt text)"
    }

    fn tags(&self) -> &[&str] {
        &["accessibility", "images"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let mut in_image = false;
        let mut image_start_line = 0;
        let mut alt_text = String::new();

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::Image { .. }) => {
                    in_image = true;
                    image_start_line = parser.offset_to_line(range.start);
                    alt_text.clear();
                }
                Event::Text(text) if in_image => {
                    alt_text.push_str(&text);
                }
                Event::End(TagEnd::Image) if in_image => {
                    // Only report if alt_text is completely empty (not just whitespace)
                    if alt_text.is_empty() {
                        violations.push(Violation {
                            line: image_start_line,
                            column: Some(1),
                            rule: self.name().to_owned(),
                            message: "Images should have alternate text (alt text)".to_owned(),
                            fix: None,
                        });
                    }
                    in_image = false;
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
    use crate::lint::rules::rendered;

    #[test]
    fn test_image_with_alt() {
        let content = "![Alt text](image.png)";
        let parser = MarkdownParser::new(content);
        let rule = MD045;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_image_without_alt() {
        let content = "![](image.png)";
        let parser = MarkdownParser::new(content);
        let rule = MD045;
        let violations = rule.check(&parser, None);

        // AIDEV: the alt-less image starts at column 23, but MD045 reports every
        // violation at column 1, so the column cannot locate it on the line.
        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD045 Images should have alternate text (alt text)"]
        );
    }

    #[test]
    fn test_image_with_whitespace_alt() {
        let content = "![  ](image.png)";
        let parser = MarkdownParser::new(content);
        let rule = MD045;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Whitespace-only is considered valid
    }

    #[test]
    fn test_multiple_images() {
        let content = "![Good](img1.png) and ![](img2.png)";
        let parser = MarkdownParser::new(content);
        let rule = MD045;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD045 Images should have alternate text (alt text)"]
        );
    }
}
