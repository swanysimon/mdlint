use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use serde_json::Value;

pub struct MD014;

impl Rule for MD014 {
    fn name(&self) -> &'static str {
        "MD014"
    }

    fn description(&self) -> &'static str {
        "Dollar signs used before commands without showing output"
    }

    fn tags(&self) -> &[&str] {
        &["code"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let mut in_shell_code_block = false;
        let mut code_block_start_line = 0;
        let mut code_block_lines: Vec<String> = Vec::new();

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) => {
                    let lang_str = lang.to_string().to_lowercase();
                    in_shell_code_block = lang_str == "bash"
                        || lang_str == "sh"
                        || lang_str == "shell"
                        || lang_str == "console";
                    if in_shell_code_block {
                        code_block_start_line = parser.offset_to_line(range.start);
                        code_block_lines.clear();
                    }
                }
                Event::Text(text) if in_shell_code_block => {
                    code_block_lines.push(text.to_string());
                }
                Event::End(TagEnd::CodeBlock) if in_shell_code_block => {
                    // Check if all non-empty lines start with $
                    let code_text = code_block_lines.join("");
                    let lines: Vec<&str> = code_text.lines().collect();
                    let non_empty_lines: Vec<&str> = lines
                        .iter()
                        .filter(|l| !l.trim().is_empty())
                        .copied()
                        .collect();

                    if !non_empty_lines.is_empty() {
                        let all_start_with_dollar = non_empty_lines
                            .iter()
                            .all(|line| line.trim_start().starts_with('$'));

                        if all_start_with_dollar {
                            // Report a violation for each line that starts with $
                            for (current_line, line) in
                                (code_block_start_line + 1..).zip(lines.iter())
                            {
                                if !line.trim().is_empty() && line.trim_start().starts_with('$') {
                                    // Remove leading $ and any spaces after it
                                    let trimmed = line.trim_start();
                                    let after_dollar = trimmed
                                        .strip_prefix('$')
                                        .expect("starts_with('$') checked above");
                                    let after_dollar_trimmed = after_dollar.trim_start();
                                    // Preserve leading whitespace before $
                                    let leading_spaces = line.len() - trimmed.len();
                                    let replacement = format!(
                                        "{}{}",
                                        " ".repeat(leading_spaces),
                                        after_dollar_trimmed
                                    );

                                    violations.push(Violation {
                                        line: current_line,
                                        column: Some(1),
                                        rule: self.name().to_owned(),
                                        message:
                                            "Dollar signs should not be used before commands without showing output".to_owned(),
                                        fix: Some(Fix {
                                            line_start: current_line,
                                            line_end: current_line,
                                            column_start: None,
                                            column_end: None,
                                            replacement,
                                            description: "Remove dollar sign".to_owned(),
                                        }),
                                    });
                                }
                            }
                        }
                    }

                    in_shell_code_block = false;
                    code_block_lines.clear();
                }
                _ => {}
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fix::Fixer;
    use indoc::indoc;

    fn apply_fixes(content: &str, violations: &[Violation]) -> String {
        let fixes: Vec<_> = violations.iter().filter_map(|v| v.fix.clone()).collect();
        Fixer::new()
            .apply_fixes_to_content(content, &fixes)
            .unwrap()
    }

    #[test]
    fn test_no_dollar_signs() {
        let content = indoc! {"
            ```bash
            ls -la
            echo hello
            ```"};
        let parser = MarkdownParser::new(content);
        let rule = MD014;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_all_dollar_signs() {
        let content = indoc! {"
            ```bash
            $ ls -la
            $ echo hello
            ```"};
        let parser = MarkdownParser::new(content);
        let rule = MD014;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2); // One for each line with $
        assert_eq!(violations[0].line, 2); // First line of code content
        assert_eq!(violations[1].line, 3); // Second line of code content
    }

    #[test]
    fn test_dollar_with_output() {
        let content = indoc! {"
            ```bash
            $ ls -la
            total 64
            $ echo hello
            hello
            ```"};
        let parser = MarkdownParser::new(content);
        let rule = MD014;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Mixed lines, showing output
    }

    #[test]
    fn test_non_shell_language() {
        let content = indoc! {"
            ```python
            $ this is not a shell
            ```"};
        let parser = MarkdownParser::new(content);
        let rule = MD014;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Not a shell language
    }

    #[test]
    fn test_fix_removes_dollar_signs() {
        let content = indoc! {"
            ```bash
            $ ls -la
            $ echo hello
            ```
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD014;
        let violations = rule.check(&parser, None);
        assert_eq!(violations.len(), 2);
        let fixed = apply_fixes(content, &violations);
        assert_eq!(
            fixed,
            indoc! {"
                ```bash
                ls -la
                echo hello
                ```
            "}
        );
    }
}
