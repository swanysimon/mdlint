use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{CodeBlockKind, Event, Tag};
use serde_json::Value;

pub struct MD046;

impl Rule for MD046 {
    fn name(&self) -> &'static str {
        "MD046"
    }

    fn description(&self) -> &'static str {
        "Code block style"
    }

    fn tags(&self) -> &[&str] {
        &["code"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let style = config
            .and_then(|c| c.get("style"))
            .and_then(|v| v.as_str())
            .unwrap_or("fenced");

        let mut violations = Vec::new();
        let mut first_style: Option<&str> = None;

        for (event, range) in parser.parse_with_offsets() {
            let line = parser.offset_to_line(range.start);

            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) => {
                    let current_style = "fenced";

                    if style == "consistent" {
                        if let Some(first) = first_style {
                            if current_style != first {
                                violations.push(Violation {
                                    line,
                                    column: Some(1),
                                    rule: self.name().to_owned(),
                                    message: format!(
                                        "Code block style should be consistent: expected {first}, found {current_style}"
                                    ),
                                    fix: None,
                                });
                            }
                        } else {
                            first_style = Some(current_style);
                        }
                    } else if style == "indented" {
                        violations.push(Violation {
                            line,
                            column: Some(1),
                            rule: self.name().to_owned(),
                            message: "Code block style should be 'indented', found 'fenced'"
                                .to_owned(),
                            fix: None,
                        });
                    }
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                    let current_style = "indented";

                    if style == "consistent" {
                        if let Some(first) = first_style {
                            if current_style != first {
                                violations.push(Violation {
                                    line,
                                    column: Some(1),
                                    rule: self.name().to_owned(),
                                    message: format!(
                                        "Code block style should be consistent: expected {first}, found {current_style}"
                                    ),
                                    fix: None,
                                });
                            }
                        } else {
                            first_style = Some(current_style);
                        }
                    } else if style == "fenced" {
                        violations.push(Violation {
                            line,
                            column: Some(1),
                            rule: self.name().to_owned(),
                            message: "Code block style should be 'fenced', found 'indented'"
                                .to_owned(),
                            fix: None,
                        });
                    }
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
    fn test_consistent_fenced() {
        let content = indoc! {"
            ```
            code1
            ```

            ```
            code2
            ```"};
        let parser = MarkdownParser::new(content);
        let rule = MD046;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_consistent_indented() {
        let content = "    code1\n\n    code2";
        let parser = MarkdownParser::new(content);
        let rule = MD046;
        let config = serde_json::json!({ "style": "consistent" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inconsistent() {
        let content = indoc! {"
            ```
            code1
            ```

                code2"};
        let parser = MarkdownParser::new(content);
        let rule = MD046;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:5:1: MD046 Code block style should be 'fenced', found 'indented'"]
        );
    }

    #[test]
    fn test_enforced_fenced() {
        let content = "    code";
        let parser = MarkdownParser::new(content);
        let rule = MD046;
        let config = serde_json::json!({ "style": "fenced" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            ["test.md:1:1: MD046 Code block style should be 'fenced', found 'indented'"]
        );
    }
}
