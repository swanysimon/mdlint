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

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        // As in markdownlint, a bare `[label]` is not checked by default:
        // bracketed prose like `[sic]` or `[WIP]` is common and renders as the
        // literal text it is.
        let shortcut_syntax = config
            .and_then(|c| c.get("shortcut_syntax"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
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

            let undefined = match link_type {
                LinkType::ReferenceUnknown | LinkType::CollapsedUnknown => true,
                LinkType::ShortcutUnknown => shortcut_syntax,
                _ => false,
            };
            if undefined {
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
    use crate::lint::rules::rendered;
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

        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD052 Reference link label 'undefined' is not defined"]
        );
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
    fn test_shortcut_reference_undefined_not_checked_by_default() {
        // Regression test for https://github.com/swanysimon/mdlint/issues/100
        let content = "- Item one [Directional]";
        let parser = MarkdownParser::new(content);
        assert_eq!(MD052.check(&parser, None).len(), 0);
    }

    #[test]
    fn test_shortcut_reference_undefined() {
        // Regression test for https://github.com/swanysimon/mdlint/issues/53
        let content = "Here a [reference] is unused.";
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let config = serde_json::json!({ "shortcut_syntax": true });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD052 Reference link label 'reference' is not defined"]
        );
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
    fn test_gfm_alert_not_a_reference_label() {
        // Regression test for https://github.com/swanysimon/mdlint/issues/103
        let content = indoc! {"
            > [!NOTE]
            > A nice note and not a link label."};
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_gfm_alert_lookalike_with_trailing_text_still_flagged() {
        // Text after "[!NOTE]" on the same line disqualifies the GFM alert
        // syntax (it must be alone on its line), so this is genuinely an
        // undefined shortcut reference and MD052 must still catch it.
        let content = "> [!NOTE] inline text on the same line.";
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let config = serde_json::json!({ "shortcut_syntax": true });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD052 Reference link label '!NOTE' is not defined"]
        );
    }

    #[test]
    fn test_collapsed_reference_undefined() {
        let content = "[Link][]";
        let parser = MarkdownParser::new(content);
        let rule = MD052;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD052 Reference link label 'Link' is not defined"]
        );
    }
}
