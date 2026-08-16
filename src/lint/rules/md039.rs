use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag, TagEnd};
use serde_json::Value;
use std::ops::Range;

pub struct MD039;

impl MD039 {
    /// Reports the leading and the trailing space of `span` separately, each at
    /// its own column, so two violations on one line can be told apart.
    fn spaces_in(
        &self,
        content: &str,
        span: &Range<usize>,
        parser: &MarkdownParser,
    ) -> Vec<Violation> {
        let text = &content[span.clone()];
        let at = |offset: usize| {
            let (line, column) = parser.offset_to_position(offset);
            Violation {
                line,
                column: Some(column),
                rule: self.name().to_owned(),
                message: "Spaces inside link text".to_owned(),
                fix: None,
            }
        };

        let mut violations = Vec::new();
        if text.starts_with(' ') {
            violations.push(at(span.start));
        }
        if text.ends_with(' ') {
            violations.push(at(span.end.saturating_sub(1)));
        }
        violations
    }
}

impl Rule for MD039 {
    fn name(&self) -> &'static str {
        "MD039"
    }

    fn description(&self) -> &'static str {
        "Spaces inside link text"
    }

    fn tags(&self) -> &[&str] {
        &["whitespace", "links"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        // Only the event stream knows what is really link text: it covers every
        // link form (inline, reference, collapsed) rather than just `[text](url)`,
        // and code blocks, code spans and task list checkboxes are structurally
        // excluded because none of them produce a Link.
        let mut violations = Vec::new();
        let content = parser.content();
        let mut in_link = false;
        let mut text_span: Option<Range<usize>> = None;

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::Link { .. }) => {
                    in_link = true;
                    text_span = None;
                }
                Event::End(TagEnd::Link) if in_link => {
                    if let Some(span) = text_span.take() {
                        violations.extend(self.spaces_in(content, &span, parser));
                    }
                    in_link = false;
                }
                // Every child event of the link is bounded by its `[`/`]`, so the
                // first start and the last end bracket exactly the link text —
                // including a nested image or code span at either edge.
                _ if in_link => {
                    text_span = Some(text_span.take().map_or(range.clone(), |span| {
                        span.start.min(range.start)..span.end.max(range.end)
                    }));
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
    use crate::lint::rules::rendered;
    use indoc::indoc;

    #[test]
    fn test_correct_link() {
        let content = "[Link text](https://example.com)";
        let parser = MarkdownParser::new(content);
        let rule = MD039;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_leading_space() {
        let content = "[ Link text](https://example.com)";
        let parser = MarkdownParser::new(content);
        let rule = MD039;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:1:2: MD039 Spaces inside link text"]
        );
    }

    #[test]
    fn test_trailing_space() {
        let content = "[Link text ](https://example.com)";
        let parser = MarkdownParser::new(content);
        let rule = MD039;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:1:11: MD039 Spaces inside link text"]
        );
    }

    #[test]
    fn test_both_spaces() {
        let content = "[ Link text ](https://example.com)";
        let parser = MarkdownParser::new(content);
        let rule = MD039;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:2: MD039 Spaces inside link text",
                "test.md:1:12: MD039 Spaces inside link text",
            ]
        );
    }

    #[test]
    fn test_task_list_checkbox_not_flagged() {
        let content = "* [ ] Unchecked item with a [real link](https://example.com)";
        let parser = MarkdownParser::new(content);
        let rule = MD039;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_code_block_not_flagged() {
        let content = indoc! {"
            Text

            ```markdown
            [ spaced ](url)
            ```
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD039;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_code_span_not_flagged() {
        let content = "Write it as `[ spaced ](url)` to show the mistake.";
        let parser = MarkdownParser::new(content);
        let rule = MD039;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_reference_link_flagged() {
        let content = indoc! {"
            See [ spaced ][ref] and [ collapsed ][].

            [ref]: https://example.com
            [ collapsed ]: https://example.com
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD039;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:6: MD039 Spaces inside link text",
                "test.md:1:13: MD039 Spaces inside link text",
                "test.md:1:26: MD039 Spaces inside link text",
                "test.md:1:36: MD039 Spaces inside link text",
            ]
        );
    }

    #[test]
    fn test_destination_containing_parenthesis_flagged() {
        let content = "See [ spaced ](https://example.com/a(b)c).";
        let parser = MarkdownParser::new(content);
        let rule = MD039;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:6: MD039 Spaces inside link text",
                "test.md:1:13: MD039 Spaces inside link text",
            ]
        );
    }

    #[test]
    fn test_nested_image_edges_flagged() {
        let content = "[ ![alt](image.png) ](https://example.com)";
        let parser = MarkdownParser::new(content);
        let rule = MD039;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:2: MD039 Spaces inside link text",
                "test.md:1:20: MD039 Spaces inside link text",
            ]
        );
    }
}
