use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use pulldown_cmark::{Event, Tag};
use serde_json::Value;

pub struct MD022;

impl Rule for MD022 {
    fn name(&self) -> &'static str {
        "MD022"
    }

    fn description(&self) -> &'static str {
        "Headings should be surrounded by blank lines"
    }

    fn tags(&self) -> &[&str] {
        &["headings", "headers", "blank_lines"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let lines = parser.lines();

        // Find all heading lines using the AST
        let mut heading_lines = Vec::new();
        for (event, range) in parser.parse_with_offsets() {
            if let Event::Start(Tag::Heading { .. }) = event {
                let line = parser.offset_to_line(range.start);
                heading_lines.push(line);
            }
        }

        for &heading_line in &heading_lines {
            let line_idx = heading_line - 1;

            // Check if there's a blank line before (skip if first line or after blank)
            if line_idx > 0 {
                let prev_line = lines.get(line_idx - 1).expect("line_idx > 0").trim();
                if !prev_line.is_empty() {
                    // Replace the heading line with "\n<heading>" — the embedded newline
                    // causes the Fixer to produce a blank line before the heading.
                    violations.push(Violation {
                        line: heading_line,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "Heading should be surrounded by blank lines (missing before)"
                            .to_owned(),
                        fix: Some(Fix {
                            line_start: heading_line,
                            line_end: heading_line,
                            column_start: None,
                            column_end: None,
                            replacement: format!(
                                "\n{}",
                                lines.get(line_idx).expect("heading line is valid")
                            ),
                            description: "Add blank line before heading".to_owned(),
                        }),
                    });
                }
            }

            // Check if there's a blank line after (skip if last line)
            if line_idx + 1 < lines.len() {
                let next_line = lines
                    .get(line_idx + 1)
                    .expect("line_idx + 1 < lines.len()")
                    .trim();
                // Allow another heading right after (for closed headings or setext underlines)
                if !next_line.is_empty()
                    && !next_line.starts_with('#')
                    && !next_line
                        .chars()
                        .all(|c| c == '=' || c == '-' || c.is_whitespace())
                {
                    // Replace the heading line with "<heading>\n" — the embedded newline
                    // causes the Fixer to produce a blank line after the heading.
                    violations.push(Violation {
                        line: heading_line,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "Heading should be surrounded by blank lines (missing after)"
                            .to_owned(),
                        fix: Some(Fix {
                            line_start: heading_line,
                            line_end: heading_line,
                            column_start: None,
                            column_end: None,
                            replacement: format!(
                                "{}\n",
                                lines.get(line_idx).expect("heading line is valid")
                            ),
                            description: "Add blank line after heading".to_owned(),
                        }),
                    });
                }
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
    fn test_properly_surrounded() {
        let content = indoc! {"
            Paragraph

            # Heading

            Another paragraph"};
        let parser = MarkdownParser::new(content);
        let rule = MD022;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_missing_blank_before() {
        let content = indoc! {"
            Paragraph
            # Heading

            Content"};
        let parser = MarkdownParser::new(content);
        let rule = MD022;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("before"));
    }

    #[test]
    fn test_missing_blank_after() {
        let content = indoc! {"

            # Heading
            Content"};
        let parser = MarkdownParser::new(content);
        let rule = MD022;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("after"));
    }

    #[test]
    fn test_first_line() {
        let content = indoc! {"
            # Heading

            Content"};
        let parser = MarkdownParser::new(content);
        let rule = MD022;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // First line is exempt from "before" check
    }

    #[test]
    fn test_fix_inserts_blank_before_heading() {
        let content = indoc! {"
            Paragraph
            # Heading

            Content
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD022;
        let violations = rule.check(&parser, None);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("before"));
        let fixed = apply_fixes(content, &violations);
        assert_eq!(
            fixed,
            indoc! {"
                Paragraph

                # Heading

                Content
            "}
        );
    }

    #[test]
    fn test_fix_inserts_blank_after_heading() {
        let content = indoc! {"
            # Heading
            Content
        "};
        let parser = MarkdownParser::new(content);
        let rule = MD022;
        let violations = rule.check(&parser, None);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("after"));
        let fixed = apply_fixes(content, &violations);
        assert_eq!(
            fixed,
            indoc! {"
                # Heading

                Content
            "}
        );
    }
}
