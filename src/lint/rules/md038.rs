use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::Event;
use serde_json::Value;

pub struct MD038;

impl Rule for MD038 {
    fn name(&self) -> &'static str {
        "MD038"
    }

    fn description(&self) -> &'static str {
        "Spaces inside code span elements"
    }

    fn tags(&self) -> &[&str] {
        &["whitespace", "code"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        for (event, range) in parser.parse_with_offsets() {
            if let Event::Code(text) = event {
                // Exception: allow code spans that are all spaces (e.g., ` `, `  `)
                if text.trim().is_empty() {
                    continue;
                }

                // Exception: allow symmetric single-space padding (` code `)
                // which is the result of CommonMark trimming single spaces on both sides
                let leading_spaces = text.len() - text.trim_start().len();
                let trailing_spaces = text.len() - text.trim_end().len();

                let is_symmetric_single = leading_spaces == 1 && trailing_spaces == 1;
                if is_symmetric_single {
                    continue;
                }

                // Report violations for any other spacing
                if leading_spaces > 0 || trailing_spaces > 0 {
                    let line = parser.offset_to_line(range.start);

                    // Report violation for leading space
                    if leading_spaces > 0 {
                        let space_offset = range.start + 1; // Opening backtick + 1 = first space
                        let (_line_num, column) = parser.offset_to_position(space_offset);
                        violations.push(Violation {
                            line,
                            column: Some(column),
                            rule: self.name().to_string(),
                            message: "Spaces inside code span elements".to_string(),
                            fix: None,
                        });
                    }

                    // Report violation for trailing space (if different from leading, or no leading space)
                    if trailing_spaces > 0
                        && (leading_spaces == 0 || leading_spaces != trailing_spaces)
                    {
                        let space_offset = range.end - trailing_spaces - 1; // Point to first trailing space
                        let (_line_num, column) = parser.offset_to_position(space_offset);
                        violations.push(Violation {
                            line,
                            column: Some(column),
                            rule: self.name().to_string(),
                            message: "Spaces inside code span elements".to_string(),
                            fix: None,
                        });
                    }
                }
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
    fn test_correct_code_span() {
        let content = "Use the `function()` to call it.";
        let parser = MarkdownParser::new(content);
        let rule = MD038;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_leading_space() {
        let content = "Use the ` function()` to call it.";
        let parser = MarkdownParser::new(content);
        let rule = MD038;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_trailing_space() {
        let content = "Use the `function() ` to call it.";
        let parser = MarkdownParser::new(content);
        let rule = MD038;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_both_spaces() {
        let content = "Use the ` function() ` to call it.";
        let parser = MarkdownParser::new(content);
        let rule = MD038;
        let violations = rule.check(&parser, None);

        // Spaces immediately inside backticks are treated as delimiters by CommonMark,
        // not as part of the code content, so no violation should be reported
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_intentional_space() {
        let content = "A single space ` ` character.";
        let parser = MarkdownParser::new(content);
        let rule = MD038;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Intentional space, should be allowed
    }

    #[test]
    fn test_symmetric_single_space() {
        // Symmetric single-space padding is allowed per CommonMark spec
        let content = "Use ` code ` in text.";
        let parser = MarkdownParser::new(content);
        let rule = MD038;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_multiple_leading_spaces() {
        let content = "Use `  code` in text.";
        let parser = MarkdownParser::new(content);
        let rule = MD038;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_multiple_trailing_spaces() {
        let content = "Use `code  ` in text.";
        let parser = MarkdownParser::new(content);
        let rule = MD038;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_symmetric_multiple_spaces() {
        // CommonMark parser trims leading/trailing spaces, so `  code  ` becomes ` code `
        // which is valid symmetric single spacing
        let content = "Use `  code  ` in text.";
        let parser = MarkdownParser::new(content);
        let rule = MD038;
        let violations = rule.check(&parser, None);

        // The parser sees ` code ` (single space padding, which is allowed)
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_asymmetric_spaces() {
        // CommonMark parser trims leading/trailing spaces, but asymmetric spacing
        // results in uneven trimming. `  code ` becomes ` code` (only trailing space removed)
        let content = "Use `  code ` in text.";
        let parser = MarkdownParser::new(content);
        let rule = MD038;
        let violations = rule.check(&parser, None);

        // The parser sees ` code` (leading space without trailing), which is flagged
        assert_eq!(violations.len(), 1);
    }
}
