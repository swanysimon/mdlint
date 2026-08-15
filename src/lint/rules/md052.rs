use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, LinkType, Tag};
use serde_json::Value;

pub struct MD052;

impl Rule for MD052 {
    fn name(&self) -> &'static str {
        "MD052"
    }

    fn description(&self) -> &'static str {
        "Reference links and images should use a label that is defined"
    }

    fn tags(&self) -> &[&str] {
        &["links"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        // A broken-link callback forces pulldown-cmark to still emit an event for
        // reference-style links/images with no matching definition, tagging their
        // `LinkType` as the `*Unknown` variant instead of silently treating them as
        // plain text. This lets shortcut (`[label]`) and collapsed (`[label][]`)
        // forms be checked alongside the full `[text][label]` form.
        for (event, range) in parser.parse_with_broken_links() {
            let (is_image, link_type, id) = match event {
                Event::Start(Tag::Link { link_type, id, .. }) => (false, link_type, id),
                Event::Start(Tag::Image { link_type, id, .. }) => (true, link_type, id),
                _ => continue,
            };

            if matches!(
                link_type,
                LinkType::ReferenceUnknown | LinkType::CollapsedUnknown | LinkType::ShortcutUnknown
            ) {
                let item_type = if is_image { "image" } else { "link" };
                violations.push(Violation {
                    line: parser.offset_to_line(range.start),
                    column: Some(1),
                    rule: self.name().to_owned(),
                    message: format!("Reference {item_type} label '{id}' is not defined"),
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
    fn test_defined_reference() {
        let content = indoc! {"
            [example]: https://example.com

            [Link][example]"};
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_undefined_reference() {
        let content = "[Link][undefined]";
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("undefined"));
    }

    #[test]
    fn test_image_reference() {
        let content = indoc! {"
            [img]: image.png

            ![Alt][img]"};
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inline_links_ignored() {
        let content = "[Link](https://example.com)";
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let violations = rule.check(&parser, None);

        // Inline links should not trigger violations
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_case_insensitive() {
        let content = indoc! {"
            [EXAMPLE]: https://example.com

            [Link][example]"};
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let violations = rule.check(&parser, None);

        // Labels should be case-insensitive
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_shortcut_reference_undefined() {
        // Regression test for https://github.com/swanysimon/mdlint/issues/53
        let content = "Here a [reference] is unused.";
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("reference"));
    }

    #[test]
    fn test_shortcut_reference_defined() {
        let content = indoc! {"
            Here a [reference] is used.

            [reference]: https://example.com/"};
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_collapsed_reference_undefined() {
        let content = "[Link][]";
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }
}
