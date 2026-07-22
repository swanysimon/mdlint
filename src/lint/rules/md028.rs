use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;

pub struct MD028;

impl Rule for MD028 {
    fn name(&self) -> &'static str {
        "MD028"
    }

    fn description(&self) -> &'static str {
        "Blank line inside blockquote"
    }

    fn tags(&self) -> &[&str] {
        &["blockquote", "whitespace"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let lines = parser.lines();
        let mut in_blockquote = false;

        for (line_num, line) in lines.iter().enumerate() {
            let line_number = line_num + 1;
            let trimmed = line.trim_start();

            let is_blockquote_line = trimmed.starts_with('>');
            let is_blank = line.trim().is_empty();

            if is_blockquote_line {
                in_blockquote = true;
            } else if is_blank && in_blockquote {
                // Look ahead to find if blockquote continues (skip multiple blank lines)
                let mut found_continuation = false;
                for future_line in lines.iter().skip(line_num + 1) {
                    let trimmed_start = future_line.trim_start();
                    if trimmed_start.starts_with('>') {
                        found_continuation = true;
                        break;
                    } else if !future_line.trim().is_empty() {
                        // Non-blank, non-blockquote line means blockquote ended
                        break;
                    }
                }

                if found_continuation {
                    violations.push(Violation {
                        line: line_number,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "Blank line inside blockquote".to_owned(),
                        fix: None,
                    });
                    // After reporting violation, don't check subsequent blank lines
                    in_blockquote = false;
                } else {
                    in_blockquote = false;
                }
            } else if !is_blank {
                // Non-blockquote, non-blank line ends the blockquote
                in_blockquote = false;
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
    fn test_continuous_blockquote() {
        let content = "> Line 1\n> Line 2\n> Line 3";
        let parser = MarkdownParser::new(content);
        let rule = MD028;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_blank_inside_blockquote() {
        let content = "> Line 1\n\n> Line 2";
        let parser = MarkdownParser::new(content);
        let rule = MD028;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 2);
    }

    #[test]
    fn test_blank_ends_blockquote() {
        let content = "> Line 1\n\nNormal text";
        let parser = MarkdownParser::new(content);
        let rule = MD028;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Blank line ends the blockquote
    }

    #[test]
    fn test_multiple_blank_lines() {
        let content = "> Line 1\n\n\n> Line 2";
        let parser = MarkdownParser::new(content);
        let rule = MD028;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1); // First blank line is the violation
    }
}
