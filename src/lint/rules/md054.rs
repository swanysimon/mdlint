use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, LinkType, Tag};
use serde_json::Value;

pub struct MD054;

impl Rule for MD054 {
    fn name(&self) -> &'static str {
        "MD054"
    }

    fn description(&self) -> &'static str {
        "Link and image style"
    }

    fn tags(&self) -> &[&str] {
        &["links"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        // MD054 only checks when a specific style is configured
        // Default behavior is to allow all styles (no checking)
        let style = config.and_then(|c| c.get("style")).and_then(|v| v.as_str());

        // If no style is configured, allow everything
        if style.is_none() {
            return Vec::new();
        }

        let style = style.unwrap();
        let mut violations = Vec::new();
        let mut first_style: Option<&str> = None;

        for (event, range) in parser.parse_with_offsets() {
            let (is_link_or_image, link_type) = match &event {
                Event::Start(
                    Tag::Link { link_type: lt, .. } | Tag::Image { link_type: lt, .. },
                ) => (true, Some(lt)),
                _ => (false, None),
            };

            if is_link_or_image && let Some(lt) = link_type {
                let current_style = match lt {
                    LinkType::Inline => "inline",
                    LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut => "reference",
                    _ => continue,
                };

                if style == "consistent" {
                    if let Some(first) = first_style {
                        if current_style != first {
                            violations.push(Violation {
                                    line: parser.offset_to_line(range.start),
                                    column: Some(1),
                                    rule: self.name().to_string(),
                                    message: format!(
                                        "Link/image style should be consistent: expected '{first}', found '{current_style}'"
                                    ),
                                    fix: None,
                                });
                        }
                    } else {
                        first_style = Some(current_style);
                    }
                } else if current_style != style {
                    violations.push(Violation {
                        line: parser.offset_to_line(range.start),
                        column: Some(1),
                        rule: self.name().to_string(),
                        message: format!(
                            "Link/image style should be '{style}', found '{current_style}'"
                        ),
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

    #[test]
    fn test_consistent_inline() {
        let content = "[Link](url1) and [Another](url2)";
        let parser = MarkdownParser::new(content);
        let rule = MD054;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_consistent_reference() {
        let content = "[link1]: url1\n[link2]: url2\n\n[Link][link1] and [Another][link2]";
        let parser = MarkdownParser::new(content);
        let rule = MD054;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inconsistent_style() {
        let content = "[link1]: url1\n\n[Link](url) and [Ref][link1]";
        let parser = MarkdownParser::new(content);
        let rule = MD054;
        let config = serde_json::json!({ "style": "consistent" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_enforced_inline() {
        let content = "[link]: url\n\n[Link][link]";
        let parser = MarkdownParser::new(content);
        let rule = MD054;
        let config = serde_json::json!({ "style": "inline" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_enforced_reference() {
        let content = "[Link](url)";
        let parser = MarkdownParser::new(content);
        let rule = MD054;
        let config = serde_json::json!({ "style": "reference" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1);
    }
}
