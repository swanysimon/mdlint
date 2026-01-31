use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, HeadingLevel, Tag, TagEnd};
use serde_json::Value;
use std::collections::HashMap;

pub struct MD024;

impl Rule for MD024 {
    fn name(&self) -> &str {
        "MD024"
    }

    fn description(&self) -> &str {
        "Multiple headings with the same content"
    }

    fn tags(&self) -> &[&str] {
        &["headings"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let siblings_only = config
            .and_then(|c| c.get("siblings_only"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut violations = Vec::new();
        let mut heading_texts: HashMap<String, (usize, HeadingLevel)> = HashMap::new();
        let mut sibling_headings: HashMap<(HeadingLevel, String), usize> = HashMap::new();
        let mut last_heading_level: Option<HeadingLevel> = None;
        let mut in_heading = false;
        let mut current_heading_text = String::new();
        let mut current_heading_line = 0;
        let mut current_heading_level = HeadingLevel::H1;

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    in_heading = true;
                    current_heading_text.clear();
                    current_heading_line = parser.offset_to_line(range.start);
                    current_heading_level = level;
                }
                Event::Text(text) if in_heading => {
                    current_heading_text.push_str(&text);
                }
                Event::Code(code) if in_heading => {
                    // Include inline code in heading text
                    current_heading_text.push('`');
                    current_heading_text.push_str(&code);
                    current_heading_text.push('`');
                }
                Event::End(TagEnd::Heading(_)) if in_heading => {
                    let text = current_heading_text.trim().to_string();

                    if siblings_only {
                        // Check if same level heading with same text exists
                        if let Some(&prev_level) = last_heading_level.as_ref()
                            && prev_level != current_heading_level
                        {
                            // Different level, clear sibling tracking
                            sibling_headings.clear();
                        }

                        if let Some(&first_line) =
                            sibling_headings.get(&(current_heading_level, text.clone()))
                        {
                            violations.push(Violation {
                                line: current_heading_line,
                                column: Some(1),
                                rule: self.name().to_string(),
                                message: format!(
                                    "Multiple sibling headings with the same content: \"{}\" (first at line {})",
                                    text, first_line
                                ),
                                fix: None,
                            });
                        } else {
                            sibling_headings.insert(
                                (current_heading_level, text.clone()),
                                current_heading_line,
                            );
                        }
                    } else {
                        // Check globally
                        if let Some(&(first_line, _first_level)) = heading_texts.get(&text) {
                            violations.push(Violation {
                                line: current_heading_line,
                                column: Some(1),
                                rule: self.name().to_string(),
                                message: format!(
                                    "Multiple headings with the same content: \"{}\" (first at line {})",
                                    text, first_line
                                ),
                                fix: None,
                            });
                        } else {
                            heading_texts
                                .insert(text, (current_heading_line, current_heading_level));
                        }
                    }

                    last_heading_level = Some(current_heading_level);
                    in_heading = false;
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

    #[test]
    fn test_unique_headings() {
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let parser = MarkdownParser::new(content);
        let rule = MD024;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_duplicate_headings() {
        let content = "# Heading\n## Content\n# Heading";
        let parser = MarkdownParser::new(content);
        let rule = MD024;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("Heading"));
    }

    #[test]
    fn test_siblings_only_different_levels() {
        let content = "# Heading\n## Heading\n### Heading";
        let parser = MarkdownParser::new(content);
        let rule = MD024;
        let config = serde_json::json!({ "siblings_only": true });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0); // Different levels, so OK with siblings_only
    }

    #[test]
    fn test_siblings_only_same_level() {
        let content = "## Heading\n## Content\n## Heading";
        let parser = MarkdownParser::new(content);
        let rule = MD024;
        let config = serde_json::json!({ "siblings_only": true });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_headings_with_inline_code() {
        // Headings with different inline code should not be duplicates
        let content = "#### `mdlint check`\n\nSome text\n\n#### `mdlint format`";
        let parser = MarkdownParser::new(content);
        let rule = MD024;
        let violations = rule.check(&parser, None);

        assert_eq!(
            violations.len(),
            0,
            "Different code headings should not be duplicates"
        );
    }

    #[test]
    fn test_duplicate_code_headings() {
        // Headings with same inline code should be duplicates
        let content = "#### `mdlint check`\n\nSome text\n\n#### `mdlint check`";
        let parser = MarkdownParser::new(content);
        let rule = MD024;
        let violations = rule.check(&parser, None);

        assert_eq!(
            violations.len(),
            1,
            "Same code headings should be duplicates"
        );
        assert!(violations[0].message.contains("`mdlint check`"));
    }
}
