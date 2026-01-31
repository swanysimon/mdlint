use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag, TagEnd};
use serde_json::Value;
use std::collections::HashSet;

pub struct MD013;

impl Rule for MD013 {
    fn name(&self) -> &str {
        "MD013"
    }

    fn description(&self) -> &str {
        "Line length"
    }

    fn tags(&self) -> &[&str] {
        &["line_length"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let line_length = config
            .and_then(|c| c.get("line_length"))
            .and_then(|v| v.as_u64())
            .unwrap_or(80) as usize;

        let heading_line_length = config
            .and_then(|c| c.get("heading_line_length"))
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let check_code_blocks = config
            .and_then(|c| c.get("code_blocks"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let check_tables = config
            .and_then(|c| c.get("tables"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let check_headings = config
            .and_then(|c| c.get("headings"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mut violations = Vec::new();

        // Track special lines (headings, code blocks, tables, links/images)
        let mut heading_lines = HashSet::new();
        let mut code_block_lines = HashSet::new();
        let mut table_lines = HashSet::new();
        let mut link_only_lines = HashSet::new();

        let mut in_code_block = false;
        let mut in_table = false;

        for (event, range) in parser.parse_with_offsets() {
            let line = parser.offset_to_line(range.start);

            match event {
                Event::Start(Tag::Heading { .. }) => {
                    heading_lines.insert(line);
                }
                Event::Start(Tag::CodeBlock(_)) => {
                    in_code_block = true;
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                }
                Event::Start(Tag::Table(_)) => {
                    in_table = true;
                }
                Event::End(TagEnd::Table) => {
                    in_table = false;
                }
                Event::Start(Tag::Link { .. }) | Event::Start(Tag::Image { .. }) => {
                    // Check if this link/image is the only content on the line
                    if let Some(line_text) = parser.lines().get(line - 1) {
                        let trimmed = line_text.trim();
                        // If the line starts with [ or !, it's likely a link/image only line
                        if trimmed.starts_with('[') || trimmed.starts_with("![") {
                            link_only_lines.insert(line);
                        }
                    }
                }
                Event::Text(_) if in_code_block => {
                    code_block_lines.insert(line);
                }
                Event::Text(_) if in_table => {
                    table_lines.insert(line);
                }
                _ => {}
            }
        }

        // Check each line
        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;
            let line_len = line.chars().count();

            let is_heading = heading_lines.contains(&line_number);
            let is_code_block = code_block_lines.contains(&line_number);
            let is_table = table_lines.contains(&line_number);
            let is_link_only = link_only_lines.contains(&line_number);

            // Skip lines that only contain links or images (can't be shortened)
            if is_link_only {
                continue;
            }

            // Skip if we shouldn't check this type of line
            if is_heading && !check_headings {
                continue;
            }
            if is_code_block && !check_code_blocks {
                continue;
            }
            if is_table && !check_tables {
                continue;
            }

            // Determine the limit for this line
            let limit = if is_heading {
                heading_line_length.unwrap_or(line_length)
            } else {
                line_length
            };

            if line_len > limit {
                violations.push(Violation {
                    line: line_number,
                    column: Some(limit + 1),
                    rule: self.name().to_string(),
                    message: format!("Line exceeds maximum length ({} > {})", line_len, limit),
                    fix: None,
                });
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
    fn test_short_lines() {
        let content = "Short line\nAnother short line\nStill short";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_long_line() {
        let content = "This is a very long line that definitely exceeds the default eighty character limit and should be flagged";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 1);
    }

    #[test]
    fn test_custom_line_length() {
        let content = "This line is exactly forty characters.";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 30 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_heading_exception() {
        let content =
            "# This is a very long heading that would normally exceed the line length limit";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "headings": false });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_code_block_check() {
        let content = "```\nThis is a very long line in a code block that exceeds the maximum allowed character count\n```";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "code_blocks": true });
        let violations = rule.check(&parser, Some(&config));

        assert!(!violations.is_empty());
    }

    #[test]
    fn test_code_block_ignore() {
        let content = "```\nThis is a very long line in a code block that exceeds the maximum allowed character count\n```";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "code_blocks": false });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_link_only_line_ignored() {
        // A line containing only a link should not trigger line length check
        let content = "[This is a very long link text](https://github.com/example/repository/with/a/very/long/url/path/that/exceeds/the/limit)";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 80 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            violations.len(),
            0,
            "Link-only lines should not trigger MD013"
        );
    }

    #[test]
    fn test_image_only_line_ignored() {
        // A line containing only an image should not trigger line length check
        let content = "![Alt text](https://github.com/example/repository/with/a/very/long/image/url/path/that/exceeds/the/maximum/character/limit)";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 80 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            violations.len(),
            0,
            "Image-only lines should not trigger MD013"
        );
    }

    #[test]
    fn test_badge_link_ignored() {
        // Badge links (image inside link) should not trigger line length check
        let content = "[![CI](https://github.com/user/repo/workflows/CI/badge.svg)](https://github.com/user/repo/actions/workflows/ci.yml?query=branch%3Amain)";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 120 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0, "Badge links should not trigger MD013");
    }

    #[test]
    fn test_text_with_link_still_checked() {
        // A line with text AND a link should still be checked
        let content = "Check out this link: [example](https://github.com/example/repository/with/a/very/long/url/path) for more information about the thing";
        let parser = MarkdownParser::new(content);
        let rule = MD013;
        let config = serde_json::json!({ "line_length": 80 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            violations.len(),
            1,
            "Lines with text and links should still be checked"
        );
    }
}
