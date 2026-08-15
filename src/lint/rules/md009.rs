use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD009;

impl Rule for MD009 {
    fn name(&self) -> &'static str {
        "MD009"
    }

    fn description(&self) -> &'static str {
        "Trailing spaces"
    }

    fn tags(&self) -> &[&str] {
        &["whitespace"]
    }

    #[allow(clippy::cast_possible_truncation)] // serde_json gives u64; values are small config counts
    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let br_spaces = config
            .and_then(|c| c.get("br_spaces"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(2) as usize;

        let strict = config
            .and_then(|c| c.get("strict"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let mut violations = Vec::new();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let trimmed = line.trim_end();
            let trailing_spaces = line.len() - trimmed.len();

            if trailing_spaces > 0 {
                // Allow br_spaces for line breaks unless strict mode
                if !strict && trailing_spaces == br_spaces {
                    continue;
                }

                violations.push(Violation {
                    line: line_num + 1,
                    column: Some(trimmed.len() + 1),
                    rule: self.name().to_owned(),
                    message: format!("Trailing spaces ({trailing_spaces} spaces)"),
                    fix: Some(Fix {
                        line_start: line_num + 1,
                        line_end: line_num + 1,
                        column_start: Some(trimmed.len() + 1),
                        column_end: Some(line.len()),
                        replacement: String::new(),
                        description: "Remove trailing spaces".to_owned(),
                    }),
                });
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
    use crate::lint::rules::rendered;
    use indoc::indoc;

    fn apply_fixes(content: &str, violations: &[Violation]) -> String {
        let fixes: Vec<_> = violations.iter().filter_map(|v| v.fix.clone()).collect();
        Fixer::new()
            .apply_fixes_to_content(content, &fixes)
            .unwrap()
    }

    #[test]
    fn test_no_trailing_spaces() {
        let content = indoc! {"
            Line 1
            Line 2
            Line 3"};
        let parser = MarkdownParser::new(content);
        let rule = MD009;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_trailing_spaces() {
        let content = "Line 1  \nLine 2\nLine 3   ";
        let parser = MarkdownParser::new(content);
        let rule = MD009;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            ["test.md:3:7: MD009 Trailing spaces (3 spaces)"]
        );
    }

    #[test]
    fn test_strict_mode() {
        let content = "Line 1  \nLine 2";
        let parser = MarkdownParser::new(content);
        let rule = MD009;
        let config = serde_json::json!({ "strict": true });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            ["test.md:1:7: MD009 Trailing spaces (2 spaces)"]
        );
    }

    #[test]
    fn test_custom_br_spaces() {
        let content = "Line 1   \nLine 2";
        let parser = MarkdownParser::new(content);
        let rule = MD009;
        let config = serde_json::json!({ "br_spaces": 3 });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0); // 3 spaces allowed for br
    }

    #[test]
    fn test_fix_removes_trailing_spaces() {
        let content = "Line 1   \nLine 2\n";
        let parser = MarkdownParser::new(content);
        let rule = MD009;
        let violations = rule.check(&parser, None);
        let fixed = apply_fixes(content, &violations);
        assert_eq!(
            fixed,
            indoc! {"
                Line 1
                Line 2
            "}
        );
    }
}
