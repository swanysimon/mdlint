use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag, TagEnd};
use serde_json::Value;

pub struct MD043;

impl Rule for MD043 {
    fn name(&self) -> &'static str {
        "MD043"
    }

    fn description(&self) -> &'static str {
        "Required heading structure"
    }

    fn tags(&self) -> &[&str] {
        &["headings"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let headings = config
            .and_then(|c| c.get("headings"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                    .collect::<Vec<_>>()
            });

        // If no required structure is specified, skip check
        let required_headings = match headings {
            Some(h) if !h.is_empty() => h,
            _ => return Vec::new(),
        };

        let mut violations = Vec::new();
        let mut heading_index = 0;
        let mut in_heading = false;
        let mut current_heading_text = String::new();
        let mut current_heading_line = 0;

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::Heading { .. }) => {
                    in_heading = true;
                    current_heading_text.clear();
                    current_heading_line = parser.offset_to_line(range.start);
                }
                Event::Text(text) if in_heading => {
                    current_heading_text.push_str(&text);
                }
                Event::End(TagEnd::Heading(_)) if in_heading => {
                    let text = current_heading_text.trim();

                    if heading_index < required_headings.len() {
                        let expected = &required_headings[heading_index];
                        // Support wildcards (*)
                        if expected != "*" && text != expected {
                            violations.push(Violation {
                                line: current_heading_line,
                                column: Some(1),
                                rule: self.name().to_string(),
                                message: format!("Expected heading '{expected}', found '{text}'"),
                                fix: None,
                            });
                        }
                    } else {
                        // Extra heading not in structure
                        violations.push(Violation {
                            line: current_heading_line,
                            column: Some(1),
                            rule: self.name().to_string(),
                            message: format!("Unexpected heading: '{text}'"),
                            fix: None,
                        });
                    }

                    heading_index += 1;
                    in_heading = false;
                }
                _ => {}
            }
        }

        // Check if we have fewer headings than required
        if heading_index < required_headings.len() {
            violations.push(Violation {
                line: parser.lines().len(),
                column: Some(1),
                rule: self.name().to_string(),
                message: format!(
                    "Missing required headings (expected {}, found {})",
                    required_headings.len(),
                    heading_index
                ),
                fix: None,
            });
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
    fn test_no_config() {
        let content = "# Any Heading\n## Any Subheading";
        let parser = MarkdownParser::new(content);
        let rule = MD043;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // No required structure, no violations
    }

    #[test]
    fn test_correct_structure() {
        let content = "# Introduction\n## Background\n## Methods";
        let parser = MarkdownParser::new(content);
        let rule = MD043;
        let config = serde_json::json!({
            "headings": ["Introduction", "Background", "Methods"]
        });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_wrong_heading() {
        let content = "# Introduction\n## Wrong Heading";
        let parser = MarkdownParser::new(content);
        let rule = MD043;
        let config = serde_json::json!({
            "headings": ["Introduction", "Background"]
        });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("Wrong Heading"));
    }

    #[test]
    fn test_wildcard() {
        let content = "# Introduction\n## Any Text Here\n## Methods";
        let parser = MarkdownParser::new(content);
        let rule = MD043;
        let config = serde_json::json!({
            "headings": ["Introduction", "*", "Methods"]
        });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0); // Wildcard matches anything
    }
}
