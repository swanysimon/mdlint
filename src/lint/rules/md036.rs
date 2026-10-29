use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag, TagEnd};
use serde_json::Value;

pub struct MD036;

impl Rule for MD036 {
    fn name(&self) -> &'static str {
        "MD036"
    }

    fn description(&self) -> &'static str {
        "Emphasis used instead of a heading"
    }

    fn tags(&self) -> &[&str] {
        &["headings", "emphasis"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let punctuation = config
            .and_then(|c| c.get("punctuation"))
            .and_then(|v| v.as_str())
            .unwrap_or(".,;:!?。，；：！？");

        let mut violations = Vec::new();
        // Open tags enclosing the current event.  As in markdownlint, only a
        // paragraph directly in the document can stand in for a heading.
        let mut depth = 0usize;
        // Events of the top-level paragraph being read, if any.
        let mut paragraph: Option<(usize, Vec<Event<'_>>)> = None;

        for (event, range) in parser.parse_with_offsets() {
            match &event {
                Event::Start(Tag::Paragraph) if depth == 0 => {
                    paragraph = Some((parser.offset_to_line(range.start), Vec::new()));
                }
                Event::End(TagEnd::Paragraph) if depth == 1 => {
                    if let Some((line, events)) = paragraph.take()
                        && let Some(text) = sole_emphasis_text(&events)
                        && text
                            .trim_end()
                            .chars()
                            .last()
                            .is_some_and(|last| !punctuation.contains(last))
                    {
                        violations.push(Violation {
                            line,
                            column: Some(1),
                            rule: self.name().to_owned(),
                            message: "Emphasis used instead of a heading".to_owned(),
                            fix: None,
                        });
                    }
                }
                _ => {
                    if let Some((_, events)) = paragraph.as_mut() {
                        events.push(event.clone());
                    }
                }
            }
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) => depth -= 1,
                _ => {}
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        false
    }
}

/// The text of a paragraph that is one emphasis or strong span holding only
/// text on a single line, and nothing else.  A paragraph that goes on after the
/// emphasis -- on the same line or the next -- is prose, not a heading.
fn sole_emphasis_text(events: &[Event<'_>]) -> Option<String> {
    let (open, rest) = events.split_first()?;
    let (close, inner) = rest.split_last()?;
    let matched = matches!(
        (open, close),
        (Event::Start(Tag::Emphasis), Event::End(TagEnd::Emphasis))
            | (Event::Start(Tag::Strong), Event::End(TagEnd::Strong))
    );
    let mut text = String::new();
    for event in inner {
        let Event::Text(part) = event else {
            return None;
        };
        text.push_str(part);
    }
    (matched && !text.trim().is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::rendered;
    use indoc::indoc;

    #[test]
    fn test_real_heading() {
        let content = "# Heading\n## Another Heading";
        let parser = MarkdownParser::new(content);
        let rule = MD036;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_emphasis_as_heading() {
        let content = indoc! {"
            **Summary**

            Some content"};
        let parser = MarkdownParser::new(content);
        let rule = MD036;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD036 Emphasis used instead of a heading"]
        );
    }

    #[test]
    fn test_emphasis_with_punctuation() {
        let content = "**Note:** This is fine.";
        let parser = MarkdownParser::new(content);
        let rule = MD036;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Ends with punctuation, not a heading
    }

    #[test]
    fn test_inline_emphasis() {
        let content = "This has **bold text** in the middle.";
        let parser = MarkdownParser::new(content);
        let rule = MD036;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Not emphasis-only line
    }

    #[test]
    fn test_emphasis_outside_code_block_still_flags() {
        let content = indoc! {"
            *2026-05-12*

            Some content"};
        let parser = MarkdownParser::new(content);
        let rule = MD036;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD036 Emphasis used instead of a heading"]
        );
    }

    #[test]
    fn test_emphasis_inside_fenced_code_block_not_flagged() {
        let content = indoc! {"
            # Example

            ```text
            *2026-05-12*
            ```
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD036;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Emphasis-like text inside a fenced code block isn't a heading
    }

    #[test]
    fn test_emphasis_starting_a_paragraph_not_flagged() {
        // Regression test for https://github.com/swanysimon/mdlint/issues/100
        let content = indoc! {r#"
            # Title

            **"A question in bold?"**
            The answer continues the same paragraph.
        "#};
        let parser = MarkdownParser::new(content);
        assert_eq!(MD036.check(&parser, None).len(), 0);
    }

    #[test]
    fn test_emphasis_followed_by_text_on_the_same_line_not_flagged() {
        let parser = MarkdownParser::new("**Bold** then prose\n");
        assert_eq!(MD036.check(&parser, None).len(), 0);
    }

    #[test]
    fn test_emphasis_in_a_list_item_or_blockquote_not_flagged() {
        let content = indoc! {"
            - **Item**

            > **Quoted**
        "};
        let parser = MarkdownParser::new(content);
        assert_eq!(MD036.check(&parser, None).len(), 0);
    }

    #[test]
    fn test_underscore_emphasis_as_heading() {
        let parser = MarkdownParser::new("Intro.\n\n__Summary__\n");
        assert_eq!(
            rendered(&MD036.check(&parser, None)),
            ["test.md:3:1: MD036 Emphasis used instead of a heading"]
        );
    }
}
