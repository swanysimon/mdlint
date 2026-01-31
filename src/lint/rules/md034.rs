use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use regex::Regex;
use serde_json::Value;

pub struct MD034;

impl Rule for MD034 {
    fn name(&self) -> &str {
        "MD034"
    }

    fn description(&self) -> &str {
        "Bare URL used"
    }

    fn tags(&self) -> &[&str] {
        &["links", "url"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        // Regex to match URLs that aren't already in markdown link syntax
        let url_regex = Regex::new(r"(?:^|[^(\[<`])((https?|ftp)://[^\s)\]>]+)").unwrap();

        // Get code lines to skip (both blocks and inline code can contain URLs)
        let code_lines = parser.get_code_line_numbers();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            // Skip if line is in a code block or inline code
            if code_lines.contains(&line_number) {
                continue;
            }

            // Skip lines that are inside markdown link syntax
            for cap in url_regex.captures_iter(line) {
                if let Some(url_match) = cap.get(1) {
                    let url = url_match.as_str();
                    violations.push(Violation {
                        line: line_number,
                        column: Some(url_match.start() + 1),
                        rule: self.name().to_string(),
                        message: format!("Bare URL used: {}", url),
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
        let content = "```shell\ncurl -LO https://example.com/file.tar.gz\n```";
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
}
