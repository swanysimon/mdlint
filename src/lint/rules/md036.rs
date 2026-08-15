use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
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
        let lines = parser.lines();
        let code_block_lines = parser.get_code_block_line_numbers();

        // Track if we're in emphasis on a line-by-line basis
        for (line_num, line) in lines.iter().enumerate() {
            let line_number = line_num + 1;
            if code_block_lines.contains(&line_number) {
                continue;
            }
            let trimmed = line.trim();

            // Check if the line looks like emphasis-only content
            // Simple patterns: **text**, *text*, __text__, _text_
            if is_emphasis_only_line(trimmed) {
                // Check if it ends with punctuation (if so, likely not a heading)
                if let Some(last_char) = trimmed
                    .trim_end_matches('*')
                    .trim_end_matches('_')
                    .chars()
                    .last()
                    && !punctuation.contains(last_char)
                {
                    // Likely being used as a heading
                    violations.push(Violation {
                        line: line_number,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "Emphasis used instead of a heading".to_owned(),
                        fix: None,
                    });
                }
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        false
    }
}

fn is_emphasis_only_line(line: &str) -> bool {
    let trimmed = line.trim();

    // Check for **text** or __text__ (strong)
    if (trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4)
        || (trimmed.starts_with("__") && trimmed.ends_with("__") && trimmed.len() > 4)
    {
        // Make sure it's not just asterisks/underscores
        let inner = trimmed
            .trim_start_matches('*')
            .trim_start_matches('_')
            .trim_end_matches('*')
            .trim_end_matches('_');
        return !inner.is_empty() && !inner.chars().all(|c| c == '*' || c == '_');
    }

    // Check for *text* or _text_ (emphasis)
    if (trimmed.starts_with('*')
        && trimmed.ends_with('*')
        && !trimmed.starts_with("**")
        && trimmed.len() > 2)
        || (trimmed.starts_with('_')
            && trimmed.ends_with('_')
            && !trimmed.starts_with("__")
            && trimmed.len() > 2)
    {
        let inner = trimmed
            .trim_start_matches('*')
            .trim_start_matches('_')
            .trim_end_matches('*')
            .trim_end_matches('_');
        return !inner.is_empty() && !inner.chars().all(|c| c == '*' || c == '_');
    }

    false
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
}
