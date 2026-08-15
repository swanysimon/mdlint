use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;

pub struct MD048;

impl Rule for MD048 {
    fn name(&self) -> &'static str {
        "MD048"
    }

    fn description(&self) -> &'static str {
        "Code fence style"
    }

    fn tags(&self) -> &[&str] {
        &["code"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let style = config
            .and_then(|c| c.get("style"))
            .and_then(|v| v.as_str())
            .unwrap_or("backtick");

        let mut violations = Vec::new();
        let mut first_style: Option<char> = None;
        let mut in_code_block = false;

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;
            let trimmed = line.trim();

            // Check if line is a code fence (opening or closing)
            if trimmed.starts_with("```") {
                // Only check opening fence, not closing
                if !in_code_block {
                    let fence_char = '`';
                    if style == "consistent" {
                        if let Some(first) = first_style {
                            if fence_char != first {
                                violations.push(Violation {
                                    line: line_number,
                                    column: Some(1),
                                    rule: self.name().to_owned(),
                                    message: format!(
                                        "Code fence style should be consistent: expected '{first}', found '{fence_char}'"
                                    ),
                                    fix: None,
                                });
                            }
                        } else {
                            first_style = Some(fence_char);
                        }
                    } else if style == "tilde" {
                        violations.push(Violation {
                            line: line_number,
                            column: Some(1),
                            rule: self.name().to_owned(),
                            message: "Code fence style should be 'tilde' (~), found backtick (`)"
                                .to_owned(),
                            fix: None,
                        });
                    }
                }
                in_code_block = !in_code_block;
            } else if trimmed.starts_with("~~~") {
                // Only check opening fence, not closing
                if !in_code_block {
                    let fence_char = '~';
                    if style == "consistent" {
                        if let Some(first) = first_style {
                            if fence_char != first {
                                violations.push(Violation {
                                    line: line_number,
                                    column: Some(1),
                                    rule: self.name().to_owned(),
                                    message: format!(
                                        "Code fence style should be consistent: expected '{first}', found '{fence_char}'"
                                    ),
                                    fix: None,
                                });
                            }
                        } else {
                            first_style = Some(fence_char);
                        }
                    } else if style == "backtick" {
                        violations.push(Violation {
                            line: line_number,
                            column: Some(1),
                            rule: self.name().to_owned(),
                            message: "Code fence style should be 'backtick' (`), found tilde (~)"
                                .to_owned(),
                            fix: None,
                        });
                    }
                }
                in_code_block = !in_code_block;
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
    fn test_consistent_backtick() {
        let content = indoc! {"
            ```
            code1
            ```

            ```
            code2
            ```"};
        let parser = MarkdownParser::new(content);
        let rule = MD048;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_consistent_tilde() {
        let content = indoc! {"
            ~~~
            code1
            ~~~

            ~~~
            code2
            ~~~"};
        let parser = MarkdownParser::new(content);
        let rule = MD048;
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

            ~~~
            code2
            ~~~"};
        let parser = MarkdownParser::new(content);
        let rule = MD048;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1); // Only opening of second block
    }

    #[test]
    fn test_enforced_backtick() {
        let content = indoc! {"
            ~~~
            code
            ~~~"};
        let parser = MarkdownParser::new(content);
        let rule = MD048;
        let config = serde_json::json!({ "style": "backtick" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1); // Only opening
    }
}
