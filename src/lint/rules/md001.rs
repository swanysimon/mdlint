use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, HeadingLevel, Tag};
use serde_json::Value;

pub struct MD001;

impl Rule for MD001 {
    fn name(&self) -> &'static str {
        "MD001"
    }

    fn description(&self) -> &'static str {
        "Heading levels should only increment by one level at a time"
    }

    fn tags(&self) -> &[&str] {
        &["headings", "headers"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let mut last_level: Option<u8> = None;

        for (event, range) in parser.parse_with_offsets() {
            if let Event::Start(Tag::Heading { level, .. }) = event {
                let current_level = heading_level_to_u8(level);
                let line = parser.offset_to_line(range.start);

                if let Some(prev_level) = last_level {
                    // Check if we skipped a level (jumped by more than 1)
                    if current_level > prev_level + 1 {
                        violations.push(Violation {
                            line,
                            column: Some(1),
                            rule: self.name().to_owned(),
                            message: format!(
                                "Heading level skipped from h{prev_level} to h{current_level}"
                            ),
                            fix: None,
                        });
                    }
                }

                last_level = Some(current_level);
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        false
    }
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_skipped_levels() {
        let content = "# Heading 1\n## Heading 2\n### Heading 3\n## Heading 2 again";
        let parser = MarkdownParser::new(content);
        let rule = MD001;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_skipped_level() {
        let content = "# Heading 1\n### Heading 3 - skipped h2";
        let parser = MarkdownParser::new(content);
        let rule = MD001;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 2);
        assert!(violations[0].message.contains("h1 to h3"));
    }

    #[test]
    fn test_multiple_skips() {
        let content = "# H1\n#### H4 - skipped 2 levels\n## H2\n##### H5 - skipped h3 and h4";
        let parser = MarkdownParser::new(content);
        let rule = MD001;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2);
        assert_eq!(violations[0].line, 2);
        assert_eq!(violations[1].line, 4);
    }

    #[test]
    fn test_decreasing_levels_ok() {
        let content = "# H1\n## H2\n### H3\n## H2 back\n# H1 back";
        let parser = MarkdownParser::new(content);
        let rule = MD001;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_start_with_h2() {
        // Starting with h2 is allowed (no previous heading to compare to)
        let content = "## H2 first\n### H3\n## H2 again";
        let parser = MarkdownParser::new(content);
        let rule = MD001;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }
}
