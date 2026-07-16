use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, LinkType, Tag};
use serde_json::Value;
use std::collections::HashSet;

pub struct MD053;

impl Rule for MD053 {
    fn name(&self) -> &str {
        "MD053"
    }

    fn description(&self) -> &str {
        "Link and image reference definitions should be needed"
    }

    fn tags(&self) -> &[&str] {
        &["links"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        let defined_labels = parser.get_ref_defs();

        // Find reference-style links and images actually used. Parses
        // events rather than raw text so shortcut (`[label]`) and collapsed
        // (`[label][]`) forms are counted alongside the full `[text][label]` form.
        let mut used_labels: HashSet<String> = HashSet::new();

        for event in parser.parse() {
            let (link_type, id) = match event {
                Event::Start(Tag::Link { link_type, id, .. }) => (link_type, id),
                Event::Start(Tag::Image { link_type, id, .. }) => (link_type, id),
                _ => continue,
            };
            if matches!(
                link_type,
                LinkType::Reference
                    | LinkType::ReferenceUnknown
                    | LinkType::Collapsed
                    | LinkType::CollapsedUnknown
                    | LinkType::Shortcut
                    | LinkType::ShortcutUnknown
            ) {
                used_labels.insert(id.to_lowercase());
            }
        }

        // Find unused definitions
        for (label, line_number) in defined_labels {
            if !used_labels.contains(label.as_str()) {
                violations.push(Violation {
                    line: *line_number,
                    column: Some(1),
                    rule: self.name().to_string(),
                    message: format!(
                        "Link reference definition '{}' is defined but not used",
                        label
                    ),
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
    fn test_used_definition() {
        let content = "[example]: https://example.com\n\n[Link][example]";
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_unused_definition() {
        let content = "[unused]: https://example.com\n\nSome text without links.";
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("unused"));
    }

    #[test]
    fn test_multiple_definitions() {
        let content = "[used]: https://example.com\n[unused]: https://other.com\n\n[Link][used]";
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("unused"));
    }

    #[test]
    fn test_image_reference() {
        let content = "[img]: image.png\n\n![Alt][img]";
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_all_used() {
        let content = "[link1]: url1\n[link2]: url2\n\n[A][link1] [B][link2]";
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_shortcut_reference_used() {
        let content = "Here a [reference] is used.\n\nAnd below it is defined:\n\n[reference]: https://example.com/";
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_collapsed_reference_used() {
        let content = "[link]: https://example.com\n\n[Link][]";
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }
}
