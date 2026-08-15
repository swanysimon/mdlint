use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag, TagEnd};
use serde_json::Value;
use std::collections::HashMap;

pub struct MD051;

impl Rule for MD051 {
    fn name(&self) -> &'static str {
        "MD051"
    }

    fn description(&self) -> &'static str {
        "Link fragments should be valid"
    }

    fn tags(&self) -> &[&str] {
        &["links"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        // Build a set of valid heading fragments
        let mut heading_ids: HashMap<String, usize> = HashMap::new();
        let mut in_heading = false;
        let mut current_heading_text = String::new();

        // First pass: collect all headings
        for (event, _range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::Heading { .. }) => {
                    in_heading = true;
                    current_heading_text.clear();
                }
                Event::Text(text) if in_heading => {
                    current_heading_text.push_str(&text);
                }
                Event::End(TagEnd::Heading(_)) if in_heading => {
                    let heading_id = heading_to_id(&current_heading_text);
                    // Handle duplicate headings by tracking counts
                    let count = heading_ids.entry(heading_id.clone()).or_insert(0);
                    *count += 1;
                    in_heading = false;
                }
                _ => {}
            }
        }

        // Second pass: check link fragments
        let mut in_link = false;
        let mut link_url = String::new();
        let mut link_line = 0;

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::Link { dest_url, .. }) => {
                    in_link = true;
                    link_url = dest_url.to_string();
                    link_line = parser.offset_to_line(range.start);
                }
                Event::End(TagEnd::Link) if in_link => {
                    // Check if URL is a fragment-only link
                    if let Some(fragment) = link_url.strip_prefix('#') {
                        // Remove the '#'
                        let fragment_id = fragment.to_owned();

                        if !heading_ids.contains_key(&fragment_id) {
                            violations.push(Violation {
                                line: link_line,
                                column: Some(1),
                                rule: self.name().to_owned(),
                                message: format!(
                                    "Link fragment '{fragment}' does not match any heading"
                                ),
                                fix: None,
                            });
                        }
                    } else if let Some(pos) = link_url.find('#') {
                        // URL with fragment (e.g., "page.html#section")
                        // For now, skip external links (only check internal fragments)
                        if !link_url.starts_with("http://") && !link_url.starts_with("https://") {
                            let fragment = &link_url[pos + 1..];
                            let fragment_id = fragment.to_owned();

                            if !heading_ids.contains_key(&fragment_id) {
                                violations.push(Violation {
                                    line: link_line,
                                    column: Some(1),
                                    rule: self.name().to_owned(),
                                    message: format!(
                                        "Link fragment '{fragment}' does not match any heading"
                                    ),
                                    fix: None,
                                });
                            }
                        }
                    }

                    in_link = false;
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

/// Convert heading text to a GitHub-style heading ID
fn heading_to_id(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else if c.is_whitespace() {
                '-'
            } else {
                // Remove special characters
                '\0'
            }
        })
        .filter(|&c| c != '\0')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::rendered;
    use indoc::indoc;

    #[test]
    fn test_valid_fragment() {
        let content = indoc! {"
            # Introduction

            See [intro](#introduction) for more."};
        let parser = MarkdownParser::new(content);
        let rule = MD051;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_invalid_fragment() {
        let content = indoc! {"
            # Introduction

            See [wrong](#nonexistent) for more."};
        let parser = MarkdownParser::new(content);
        let rule = MD051;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:3:1: MD051 Link fragment 'nonexistent' does not match any heading"]
        );
    }

    #[test]
    fn test_multiple_headings() {
        let content = indoc! {"
            # One
            ## Two
            ### Three

            [Link](#two)"};
        let parser = MarkdownParser::new(content);
        let rule = MD051;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_heading_with_spaces() {
        let content = indoc! {"
            # Hello World

            [Link](#hello-world)"};
        let parser = MarkdownParser::new(content);
        let rule = MD051;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_external_links_ignored() {
        let content = indoc! {"
            # Section

            [External](https://example.com#anything)"};
        let parser = MarkdownParser::new(content);
        let rule = MD051;
        let violations = rule.check(&parser, None);

        // External links should be ignored
        assert_eq!(violations.len(), 0);
    }
}
