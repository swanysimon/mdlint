use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, HeadingLevel, Tag};
use serde_json::Value;

pub struct MD025;

impl Rule for MD025 {
    fn name(&self) -> &'static str {
        "MD025"
    }

    fn description(&self) -> &'static str {
        "Multiple top-level headings in the same document"
    }

    fn tags(&self) -> &[&str] {
        &["headings", "headers"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        // A front matter `title` field stands in for the document's first
        // top-level heading, per markdownlint's `front_matter_title` convention.
        let mut first_h1_line: Option<usize> = parser.front_matter_title_line();

        for (event, range) in parser.parse_with_offsets() {
            if let Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            }) = event
            {
                let line = parser.offset_to_line(range.start);
                if parser.front_matter_lines().contains(&line) {
                    continue;
                }

                if let Some(first_line) = first_h1_line {
                    violations.push(Violation {
                        line,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: format!(
                            "Multiple top-level headings (first h1 at line {first_line})"
                        ),
                        fix: None,
                    });
                } else {
                    first_h1_line = Some(line);
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
    fn test_single_h1() {
        let content = "# Title\n## Section\n### Subsection";
        let parser = MarkdownParser::new(content);
        let rule = MD025;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_multiple_h1() {
        let content = "# First Title\n## Section\n# Second Title";
        let parser = MarkdownParser::new(content);
        let rule = MD025;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 3);
    }

    #[test]
    fn test_three_h1() {
        let content = "# First\n# Second\n# Third";
        let parser = MarkdownParser::new(content);
        let rule = MD025;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2); // Second and third are violations
    }

    #[test]
    fn test_no_h1() {
        let content = "## Section\n### Subsection";
        let parser = MarkdownParser::new(content);
        let rule = MD025;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_front_matter_title_counts_as_first_h1() {
        let content = "---\ntitle: My Title\n---\n\n# Body Heading\n";
        let parser = MarkdownParser::new(content);
        let rule = MD025;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 5);
        assert!(violations[0].message.contains("first h1 at line 2"));
    }

    #[test]
    fn test_front_matter_without_title_does_not_seed_first_h1() {
        let content = "---\nauthor: Someone\n---\n\n# Body Heading\n";
        let parser = MarkdownParser::new(content);
        let rule = MD025;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }
}
