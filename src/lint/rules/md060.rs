use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;

pub struct MD060;

impl Rule for MD060 {
    fn name(&self) -> &'static str {
        "MD060"
    }

    fn description(&self) -> &'static str {
        "Table column style"
    }

    fn tags(&self) -> &[&str] {
        &["table"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let style = config
            .and_then(|c| c.get("style"))
            .and_then(|v| v.as_str())
            .unwrap_or("consistent");

        let mut violations = Vec::new();
        let lines = parser.lines();
        let mut first_alignment: Option<Vec<&str>> = None;

        for (line_num, line) in lines.iter().enumerate() {
            let line_number = line_num + 1;
            let trimmed = line.trim();

            // Check if this is a table separator line
            if is_separator_line(trimmed) {
                let alignments = parse_alignments(trimmed);

                if style == "consistent" {
                    if let Some(first) = &first_alignment {
                        if alignments.len() == first.len() {
                            for (i, (current, expected)) in
                                alignments.iter().zip(first.iter()).enumerate()
                            {
                                if current != expected {
                                    violations.push(Violation {
                                        line: line_number,
                                        column: Some(1),
                                        rule: self.name().to_owned(),
                                        message: format!(
                                            "Table column {} alignment should be consistent: expected '{}', found '{}'",
                                            i + 1,
                                            expected,
                                            current
                                        ),
                                        fix: None,
                                    });
                                }
                            }
                        }
                    } else {
                        first_alignment = Some(alignments);
                    }
                } else {
                    // Check enforced style for each column
                    for (i, alignment) in alignments.iter().enumerate() {
                        if *alignment != style {
                            violations.push(Violation {
                                line: line_number,
                                column: Some(1),
                                rule: self.name().to_owned(),
                                message: format!(
                                    "Table column {} should use '{}' alignment, found '{}'",
                                    i + 1,
                                    style,
                                    alignment
                                ),
                                fix: None,
                            });
                        }
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

/// Check if a line is a table separator
fn is_separator_line(line: &str) -> bool {
    line.contains("---") || line.contains(":--") || line.contains("--:")
}

/// Parse alignment from separator line
fn parse_alignments(line: &str) -> Vec<&str> {
    let trimmed = line.trim();
    let parts: Vec<&str> = trimmed
        .split('|')
        .filter(|s| !s.trim().is_empty())
        .collect();

    parts
        .iter()
        .map(|part| {
            let p = part.trim();
            if p.starts_with(':') && p.ends_with(':') {
                "center"
            } else if p.ends_with(':') {
                "right"
            } else if p.starts_with(':') {
                "left"
            } else {
                "default"
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_consistent_alignment() {
        let content = indoc! {"
            | A | B |
            |---|---|
            | 1 | 2 |"};
        let parser = MarkdownParser::new(content);
        let rule = MD060;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_mixed_alignment() {
        let content = indoc! {"
            | A | B | C |
            |:--|--:|:--:|
            | 1 | 2 | 3 |"};
        let parser = MarkdownParser::new(content);
        let rule = MD060;
        let violations = rule.check(&parser, None);

        // With consistent style, different alignments are okay
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_enforced_left() {
        let content = indoc! {"
            | A | B |
            |:--|--:|
            | 1 | 2 |"};
        let parser = MarkdownParser::new(content);
        let rule = MD060;
        let config = serde_json::json!({ "style": "left" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1); // Second column is right-aligned
    }

    #[test]
    fn test_enforced_default() {
        let content = indoc! {"
            | A | B |
            |---|:--|
            | 1 | 2 |"};
        let parser = MarkdownParser::new(content);
        let rule = MD060;
        let config = serde_json::json!({ "style": "default" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1); // Second column is left-aligned
    }

    #[test]
    fn test_no_table() {
        let content = "Just some text.";
        let parser = MarkdownParser::new(content);
        let rule = MD060;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }
}
