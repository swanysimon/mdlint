use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag, TagEnd};
use regex::Regex;
use serde_json::Value;

pub struct MD034;

impl Rule for MD034 {
    fn name(&self) -> &'static str {
        "MD034"
    }

    fn description(&self) -> &'static str {
        "Bare URL used"
    }

    fn tags(&self) -> &[&str] {
        &["links", "url"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        // A URL is bare only if it appears as plain text. Parsing the event
        // stream (rather than scanning lines) makes that precise: text inside a
        // link/image (`[text](url)`, `<url>` autolinks), inline code (`Event::Code`),
        // raw HTML (`Event::Html`), and reference-definition URLs are never emitted
        // as `Text` events, so they can't be mistaken for bare URLs — and a URL in
        // link display text is no longer wrongly flagged.
        let url_regex = Regex::new(r"(https?|ftp)://[^\s)\]>]+").expect("valid regex");

        let content = parser.content();
        let mut link_depth = 0u32;
        let mut in_code_block = false;

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::Link { .. } | Tag::Image { .. }) => link_depth += 1,
                Event::End(TagEnd::Link | TagEnd::Image) => {
                    link_depth = link_depth.saturating_sub(1);
                }
                Event::Start(Tag::CodeBlock(_)) => in_code_block = true,
                Event::End(TagEnd::CodeBlock) => in_code_block = false,
                Event::Text(_) if link_depth == 0 && !in_code_block => {
                    let text = &content[range.clone()];
                    for m in url_regex.find_iter(text) {
                        let (line, column) = parser.offset_to_position(range.start + m.start());
                        violations.push(Violation {
                            line,
                            column: Some(column),
                            rule: self.name().to_owned(),
                            message: format!("Bare URL used: {}", m.as_str()),
                            fix: None,
                        });
                    }
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
    use indoc::indoc;

    #[test]
    fn test_no_bare_url() {
        let content = "Check out [my site](https://example.com)";
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_bare_url() {
        let content = "Check out https://example.com for more info";
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("https://example.com"));
    }

    #[test]
    fn test_angle_bracket_url() {
        let content = "Check out <https://example.com> for info";
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Angle brackets are OK
    }

    #[test]
    fn test_multiple_urls() {
        let content = "Visit https://example.com and https://test.com";
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_url_in_code_block() {
        let content = indoc! {"
            ```shell
            curl -LO https://example.com/file.tar.gz
            ```"};
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0, "URLs in code blocks should be ignored");
    }

    #[test]
    fn test_url_in_inline_code() {
        let content = "Run `curl https://example.com` to download";
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0, "URLs in inline code should be ignored");
    }

    #[test]
    fn test_url_alone_in_backticks() {
        let content = "`https://example.com`";
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(
            violations.len(),
            0,
            "URL alone in a code span should not be flagged"
        );
    }

    #[test]
    fn test_url_alone_in_angle_brackets() {
        let content = "<https://example.com>";
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(
            violations.len(),
            0,
            "URL alone in angle brackets (autolink) should not be flagged"
        );
    }

    #[test]
    fn test_url_in_link_display_text() {
        // A URL inside link display text is part of the link, not bare.
        let content = "See [visit https://inner.example.com here](https://dest.example.com).";
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(
            violations.len(),
            0,
            "URL in link display text should not be flagged as bare"
        );
    }

    #[test]
    fn test_parenthesized_bare_url() {
        // A bare URL wrapped in prose parentheses is still bare.
        let content = "A wrapped (https://paren.example.com) URL.";
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("https://paren.example.com"));
    }

    #[test]
    fn test_bare_url_column() {
        let content = "Visit https://example.com now";
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        // "Visit " is 6 chars, URL starts at column 7 (1-indexed).
        assert_eq!(violations[0].column, Some(7));
    }

    #[test]
    fn test_url_in_reference_definition() {
        // Regression test for https://github.com/swanysimon/mdlint/issues/53
        let content = indoc! {"
            Here a [reference] is used.

            [reference]: https://example.com/"};
        let parser = MarkdownParser::new(content);
        let rule = MD034;
        let violations = rule.check(&parser, None);

        assert_eq!(
            violations.len(),
            0,
            "URL in a link reference definition should not be flagged as bare"
        );
    }
}
