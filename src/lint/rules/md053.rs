use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, LinkType, Tag};
use serde_json::Value;
use std::collections::HashSet;

pub struct MD053;

impl Rule for MD053 {
    fn name(&self) -> &'static str {
        "MD053"
    }

    fn description(&self) -> &'static str {
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
            let Event::Start(Tag::Link { link_type, id, .. } | Tag::Image { link_type, id, .. }) =
                event
            else {
                continue;
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
            if !used_labels.contains(label.to_lowercase().as_str()) {
                violations.push(Violation {
                    line: *line_number,
                    column: Some(1),
                    rule: self.name().to_owned(),
                    message: format!("Link reference definition '{label}' is defined but not used"),
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
    use indoc::indoc;

    #[test]
    fn test_used_definition() {
        let content = indoc! {"
            [example]: https://example.com

            [Link][example]"};
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_unused_definition() {
        let content = indoc! {"
            [unused]: https://example.com

            Some text without links."};
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("unused"));
    }

    #[test]
    fn test_multiple_definitions() {
        let content = indoc! {"
            [used]: https://example.com
            [unused]: https://other.com

            [Link][used]"};
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("unused"));
    }

    #[test]
    fn test_image_reference() {
        let content = indoc! {"
            [img]: image.png

            ![Alt][img]"};
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_all_used() {
        let content = indoc! {"
            [link1]: url1
            [link2]: url2

            [A][link1] [B][link2]"};
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_shortcut_reference_used() {
        let content = indoc! {"
            Here a [reference] is used.

            And below it is defined:

            [reference]: https://example.com/"};
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_collapsed_reference_used() {
        let content = indoc! {"
            [link]: https://example.com

            [Link][]"};
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_shortcut_reference_used_multiple_times() {
        // Regression: [Textual] used twice, definition at end — MD053 must not
        // flag it as unused.
        let content = indoc! {"
            # Title

            [Textual] is great.

            More text.

            [Textual] again.

            [Textual]: https://textual.textualize.io/"};
        let parser = MarkdownParser::new(content);
        let rule = MD053;
        let violations = rule.check(&parser, None);
        assert_eq!(
            violations.len(),
            0,
            "shortcut ref used multiple times should not be flagged"
        );
    }
}
