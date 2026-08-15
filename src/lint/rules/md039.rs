use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use regex::Regex;
use serde_json::Value;

pub struct MD039;

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
        let mut violations = Vec::new();

        // Regex to detect spaces inside link text: [ text](url) or [text ](url)
        // Use [^\]]+ to avoid matching across ] boundaries (e.g. task list checkboxes like [ ])
        let pattern = Regex::new(r"\[( [^\]]+|[^\]]+ )\]\([^\)]+\)").expect("valid regex");

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            for mat in pattern.find_iter(line) {
                let matched_text = mat.as_str();
                // Extract the link text between [ and ]
                if let Some(bracket_end) = matched_text.find(']') {
                    let link_text = &matched_text[1..bracket_end];

                    // Report separate violations for leading and trailing spaces
                    if link_text.starts_with(' ') {
                        violations.push(Violation {
                            line: line_number,
                            column: Some(mat.start() + 1),
                            rule: self.name().to_owned(),
                            message: "Spaces inside link text".to_owned(),
                            fix: None,
                        });
                    }
                    if link_text.ends_with(' ') {
                        violations.push(Violation {
                            line: line_number,
                            column: Some(mat.start() + 1),
                            rule: self.name().to_owned(),
                            message: "Spaces inside link text".to_owned(),
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
    use crate::lint::rules::rendered;

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
            ["test.md:1:1: MD039 Spaces inside link text"]
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
            ["test.md:1:1: MD039 Spaces inside link text"]
        );
    }

    #[test]
    fn test_both_spaces() {
        let content = "[ Link text ](https://example.com)";
        let parser = MarkdownParser::new(content);
        let rule = MD039;
        let violations = rule.check(&parser, None);

        // AIDEV: the leading- and trailing-space violations are reported at the
        // match start with the same message, so the two are indistinguishable.
        // Pointing the trailing one at the trailing space would separate them.
        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:1: MD039 Spaces inside link text",
                "test.md:1:1: MD039 Spaces inside link text",
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
}
