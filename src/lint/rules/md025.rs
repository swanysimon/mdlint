use crate::lint::rule::Rule;
use crate::lint::rules::{front_matter_title_line, validate_front_matter_title};
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

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let mut first_h1_line = front_matter_title_line(parser, config);

        for (event, range) in parser.parse_with_offsets() {
            if let Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            }) = event
            {
                let line = parser.offset_to_line(range.start);

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

    fn validate_config(&self, config: &Value) -> Result<(), String> {
        validate_front_matter_title(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::rendered;
    use indoc::indoc;

    #[test]
    fn test_single_h1() {
        let content = indoc! {"
            # Title
            ## Section
            ### Subsection"};
        let parser = MarkdownParser::new(content);
        let rule = MD025;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_multiple_h1() {
        let content = indoc! {"
            # First Title
            ## Section
            # Second Title"};
        let parser = MarkdownParser::new(content);
        let rule = MD025;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:3:1: MD025 Multiple top-level headings (first h1 at line 1)"]
        );
    }

    #[test]
    fn test_three_h1() {
        let content = indoc! {"
            # First
            # Second
            # Third"};
        let parser = MarkdownParser::new(content);
        let rule = MD025;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:2:1: MD025 Multiple top-level headings (first h1 at line 1)",
                "test.md:3:1: MD025 Multiple top-level headings (first h1 at line 1)",
            ]
        );
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

    #[test]
    fn test_front_matter_title_matches_case_insensitively() {
        let content = "---\nTitle: My Title\n---\n\n# Body Heading\n";
        let parser = MarkdownParser::new(content);
        let violations = MD025.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:5:1: MD025 Multiple top-level headings (first h1 at line 2)"]
        );
    }

    #[test]
    fn test_front_matter_title_custom_pattern() {
        let content = "---\ntitle: x\nheadline: My Title\n---\n\n# Body Heading\n";
        let parser = MarkdownParser::new(content);
        let config = serde_json::json!({ "front_matter_title": "^headline:" });
        let violations = MD025.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            ["test.md:6:1: MD025 Multiple top-level headings (first h1 at line 3)"]
        );
    }

    #[test]
    fn test_front_matter_title_empty_pattern_disables() {
        let content = "---\ntitle: My Title\n---\n\n# Body Heading\n";
        let parser = MarkdownParser::new(content);
        let config = serde_json::json!({ "front_matter_title": "" });
        let violations = MD025.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_validate_config_rejects_invalid_front_matter_title() {
        let invalid = serde_json::json!({ "front_matter_title": "^(title" });
        let error = MD025.validate_config(&invalid).unwrap_err();
        assert!(
            error.starts_with("front_matter_title is not a valid regular expression"),
            "{error}"
        );

        let not_a_string = serde_json::json!({ "front_matter_title": 5 });
        assert_eq!(
            MD025.validate_config(&not_a_string).unwrap_err(),
            "front_matter_title must be a string, got 5"
        );

        for valid in [
            serde_json::json!({}),
            serde_json::json!({ "front_matter_title": "" }),
            serde_json::json!({ "front_matter_title": "^headline:" }),
        ] {
            assert_eq!(MD025.validate_config(&valid), Ok(()));
        }
    }
}
