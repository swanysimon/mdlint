use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use regex::Regex;
use serde_json::Value;

pub struct MD011;

impl Rule for MD011 {
    fn name(&self) -> &'static str {
        "MD011"
    }

    fn description(&self) -> &'static str {
        "Reversed link syntax"
    }

    fn tags(&self) -> &[&str] {
        &["links"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        // `(text)[url]` isn't a link, so pulldown-cmark emits it as several
        // separate Text events — the source lines are the only place the whole
        // pattern is visible. The parser's precomputed code ranges still do the
        // exclusion, covering code spans as well as code blocks.
        let code_ranges = parser.get_code_ranges();

        // Pattern for reversed link syntax: (text)[url]
        // Capture the bracket content so we can exclude GFM task list checkboxes ([ ], [x], [X])
        let re = Regex::new(r"\([^)]+\)\[([^\]]+)\]").expect("valid regex");

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            for caps in re.captures_iter(line) {
                // Skip GFM task list checkboxes: [ ] and [x]/[X]
                let bracket_content = &caps[1];
                if matches!(bracket_content, " " | "x" | "X") {
                    continue;
                }
                let m = caps.get(0).expect("group 0 always present");
                let absolute = parser.line_offset_to_absolute(line_number, m.start());
                if code_ranges.iter().any(|range| range.contains(&absolute)) {
                    continue;
                }
                violations.push(Violation {
                    line: line_number,
                    column: Some(m.start() + 1),
                    rule: self.name().to_owned(),
                    message: "Reversed link syntax (found '(text)[url]', should be '[text](url)')"
                        .to_owned(),
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
    fn test_correct_link_syntax() {
        let content = "This is [a link](http://example.com) and [another](url).";
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_reversed_link_syntax() {
        let content = "This is (a link)[http://example.com] which is wrong.";
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:9: MD011 Reversed link syntax (found '(text)[url]', should be '[text](url)')"
            ]
        );
    }

    #[test]
    fn test_multiple_reversed_links() {
        let content = "First (link)[url1] and second (link)[url2].";
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:7: MD011 Reversed link syntax (found '(text)[url]', should be '[text](url)')",
                "test.md:1:31: MD011 Reversed link syntax (found '(text)[url]', should be '[text](url)')",
            ]
        );
    }

    #[test]
    fn test_mixed_correct_and_reversed() {
        let content = "Correct [link](url) and (reversed)[url].";
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:25: MD011 Reversed link syntax (found '(text)[url]', should be '[text](url)')"
            ]
        );
    }

    #[test]
    fn test_no_false_positives() {
        let content = "Some (parentheses) and [brackets] but not links.";
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_task_list_checkbox_not_flagged() {
        // (text)[ ] should not be flagged — the [ ] is a GFM task list checkbox
        let content = indoc! {"
            - [ ] Task item
            - [x] Done task
            - (description)[ ] another task
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_code_span_not_flagged() {
        let content = "Use `array(0)[index]` and `function(param)[key]` in code.";
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_code_block_not_flagged() {
        let content = indoc! {"
            # Code Example

            ```python
            result = function(param)[index]
            data = array(0)[key]
            ```

            This (is)[wrong] though.
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        // Should only flag the actual reversed link, not code
        assert_eq!(
            rendered(&violations),
            [
                "test.md:8:6: MD011 Reversed link syntax (found '(text)[url]', should be '[text](url)')"
            ]
        );
    }
}
