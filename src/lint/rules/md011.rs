use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{Event, Tag, TagEnd};
use regex::Regex;
use serde_json::Value;

pub struct MD011;

impl Rule for MD011 {
    fn name(&self) -> &str {
        "MD011"
    }

    fn description(&self) -> &str {
        "Reversed link syntax"
    }

    fn tags(&self) -> &[&str] {
        &["links"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        // Track code blocks to exclude them from checking
        let mut code_block_lines = std::collections::HashSet::new();
        let mut in_code_block = false;

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::CodeBlock(_)) => {
                    in_code_block = true;
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                }
                Event::Text(_) if in_code_block => {
                    // Mark all lines that this text event spans
                    let start_line = parser.offset_to_line(range.start);
                    let end_line = parser.offset_to_line(range.end.saturating_sub(1));
                    for line in start_line..=end_line {
                        code_block_lines.insert(line);
                    }
                }
                _ => {}
            }
        }

        // Pattern for reversed link syntax: (text)[url]
        // Capture the bracket content so we can exclude GFM task list checkboxes ([ ], [x], [X])
        let re = Regex::new(r"\([^)]+\)\[([^\]]+)\]").unwrap();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            // Skip code blocks
            if code_block_lines.contains(&line_number) {
                continue;
            }

            for caps in re.captures_iter(line) {
                // Skip GFM task list checkboxes: [ ] and [x]/[X]
                let bracket_content = &caps[1];
                if matches!(bracket_content, " " | "x" | "X") {
                    continue;
                }
                let m = caps.get(0).unwrap();
                violations.push(Violation {
                    line: line_number,
                    column: Some(m.start() + 1),
                    rule: self.name().to_string(),
                    message: "Reversed link syntax (found '(text)[url]', should be '[text](url)')"
                        .to_string(),
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

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 1);
    }

    #[test]
    fn test_multiple_reversed_links() {
        let content = "First (link)[url1] and second (link)[url2].";
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_mixed_correct_and_reversed() {
        let content = "Correct [link](url) and (reversed)[url].";
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 1);
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
        let content = "- [ ] Task item\n- [x] Done task\n- (description)[ ] another task\n";
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_code_block_not_flagged() {
        let content = r#"# Code Example

```python
result = function(param)[index]
data = array(0)[key]
```

This (is)[wrong] though.
"#;
        let parser = MarkdownParser::new(content);
        let rule = MD011;
        let violations = rule.check(&parser, None);

        // Should only flag the actual reversed link, not code
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 8);
    }
}
