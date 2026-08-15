use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::Event;
use serde_json::Value;

pub struct MD033;

impl Rule for MD033 {
    fn name(&self) -> &'static str {
        "MD033"
    }

    fn description(&self) -> &'static str {
        "Inline HTML"
    }

    fn tags(&self) -> &[&str] {
        &["html"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let allowed_elements: Vec<String> = config
            .and_then(|c| c.get("allowed_elements"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_lowercase))
                    .collect()
            })
            .unwrap_or_default();

        let mut violations = Vec::new();

        for (event, range) in parser.parse_with_offsets() {
            // Check both Html (block) and InlineHtml events
            let html_str = match event {
                Event::Html(html) | Event::InlineHtml(html) => html.to_string(),
                _ => continue,
            };

            let line = parser.offset_to_line(range.start);

            // Skip closing tags - only report opening tags
            if html_str.trim().starts_with("</") {
                continue;
            }

            // Extract tag name from HTML
            if let Some(tag_name) = extract_tag_name(&html_str) {
                let is_disallowed = allowed_elements.is_empty()
                    || !allowed_elements.contains(&tag_name.to_lowercase());

                if is_disallowed {
                    violations.push(Violation {
                        line,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: format!("Inline HTML element: <{tag_name}>"),
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

fn extract_tag_name(html: &str) -> Option<String> {
    let trimmed = html.trim();
    if trimmed.starts_with('<') {
        // Handle opening tags, closing tags, and self-closing tags
        let inner = trimmed.trim_start_matches('<').trim_start_matches('/');
        // Skip comments (<!--), DOCTYPE (<!DOCTYPE), and other `<!` declarations
        if inner.starts_with('!') {
            return None;
        }
        inner
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .map(|end_pos| inner[..end_pos].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_no_html() {
        let content = indoc! {"
            # Heading

            Normal **markdown** text."};
        let parser = MarkdownParser::new(content);
        let rule = MD033;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inline_html() {
        let content = "Text with <br> tag";
        let parser = MarkdownParser::new(content);
        let rule = MD033;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("<br>"));
    }

    #[test]
    fn test_allowed_elements() {
        let content = "Text with <br> tag and <div>content</div>";
        let parser = MarkdownParser::new(content);
        let rule = MD033;
        let config = serde_json::json!({ "allowed_elements": ["br"] });
        let violations = rule.check(&parser, Some(&config));

        // Only <div> should be flagged, <br> is allowed
        assert!(!violations.is_empty());
        assert!(violations.iter().any(|v| v.message.contains("<div>")));
    }

    #[test]
    fn test_block_html() {
        let content = indoc! {"
            <div>
            Content
            </div>"};
        let parser = MarkdownParser::new(content);
        let rule = MD033;
        let violations = rule.check(&parser, None);

        assert!(!violations.is_empty());
    }
}
