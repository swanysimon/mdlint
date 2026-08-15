use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use serde_json::Value;

pub struct MD031;

impl Rule for MD031 {
    fn name(&self) -> &'static str {
        "MD031"
    }

    fn description(&self) -> &'static str {
        "Fenced code blocks should be surrounded by blank lines"
    }

    fn tags(&self) -> &[&str] {
        &["code", "blank_lines"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let lines = parser.lines();

        // Find fenced code block boundaries
        let mut code_block_starts = Vec::new();
        let mut code_block_ends = Vec::new();
        let mut in_fenced_block = false;

        for (event, range) in parser.parse_with_offsets() {
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) => {
                    let line = parser.offset_to_line(range.start);
                    code_block_starts.push(line);
                    in_fenced_block = true;
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                    // Track that we're in an indented block, but don't record it
                    in_fenced_block = false;
                }
                Event::End(TagEnd::CodeBlock) if in_fenced_block => {
                    let line = parser.offset_to_line(range.end);
                    code_block_ends.push(line);
                    in_fenced_block = false;
                }
                _ => {}
            }
        }

        // Check each code block
        for &start_line in &code_block_starts {
            let line_idx = start_line - 1;

            // Check blank line before (skip if first line)
            if line_idx > 0 {
                let prev_line = lines.get(line_idx - 1).expect("line_idx > 0").trim();
                if !prev_line.is_empty() {
                    // Insert blank line before code block
                    violations.push(Violation {
                        line: start_line,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message:
                            "Fenced code blocks should be surrounded by blank lines (missing before)".to_owned(),
                        fix: Some(Fix {
                            line_start: start_line,
                            line_end: start_line,
                            column_start: None,
                            column_end: None,
                            replacement: format!("\n{}", lines.get(line_idx).expect("line_idx bounded")),
                            description: "Add blank line before code block".to_owned(),
                        }),
                    });
                }
            }
        }

        for &end_line in &code_block_ends {
            let line_idx = end_line - 1;

            // Check blank line after (skip if last line)
            if line_idx + 1 < lines.len() {
                let next_line = lines
                    .get(line_idx + 1)
                    .expect("line_idx + 1 < lines.len()")
                    .trim();
                if !next_line.is_empty() {
                    // Insert blank line after code block
                    violations.push(Violation {
                        line: end_line,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message:
                            "Fenced code blocks should be surrounded by blank lines (missing after)"
                                .to_owned(),
                        fix: Some(Fix {
                            line_start: end_line,
                            line_end: end_line,
                            column_start: None,
                            column_end: None,
                            replacement: format!(
                                "{}\n",
                                lines.get(line_idx).expect("line_idx bounded")
                            ),
                            description: "Add blank line after code block".to_owned(),
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
    use crate::lint::rules::rendered;
    use indoc::indoc;

    #[test]
    fn test_properly_surrounded() {
        let content = indoc! {"
            Text

            ```
            code
            ```

            More text"};
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_missing_blank_before() {
        let content = indoc! {"
            Text
            ```
            code
            ```

            More text"};
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:2:1: MD031 Fenced code blocks should be surrounded by blank lines (missing before)"
            ]
        );
    }

    #[test]
    fn test_missing_blank_after() {
        let content = indoc! {"
            Text

            ```
            code
            ```
            More text"};
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:5:1: MD031 Fenced code blocks should be surrounded by blank lines (missing after)"
            ]
        );
    }

    #[test]
    fn test_first_line() {
        let content = indoc! {"
            ```
            code
            ```

            Text"};
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // First line exempt from "before" check
    }

    #[test]
    fn test_numbered_list_with_code_block() {
        // Test that code blocks in numbered lists get proper fixes
        let content = indoc! {"
            1. **Enable/Disable a rule:**
               ```toml
               [rules.MD013]
               enabled = false
               ```

            2. **Next item**"};
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        // Should detect missing blank line before code block
        assert_eq!(
            rendered(&violations),
            [
                "test.md:2:1: MD031 Fenced code blocks should be surrounded by blank lines (missing before)"
            ]
        );

        // Check that fix has correct line numbers
        if let Some(fix) = &violations[0].fix {
            // The code block starts at line 2, so fix should target line 2
            assert_eq!(fix.line_start, 2);
            assert_eq!(fix.line_end, 2);
            // Replacement should be newline + original line content
            assert!(fix.replacement.starts_with('\n'));
        }
    }

    #[test]
    fn test_fix_creates_blank_line() {
        use crate::fix::Fixer;

        let content = indoc! {"
            Text
            ```
            code
            ```
            More"};
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        assert_eq!(
            rendered(&violations),
            [
                "test.md:2:1: MD031 Fenced code blocks should be surrounded by blank lines (missing before)",
                "test.md:4:1: MD031 Fenced code blocks should be surrounded by blank lines (missing after)",
            ]
        );

        // Apply fixes
        let fixes: Vec<_> = violations.iter().filter_map(|v| v.fix.clone()).collect();
        let runner = Fixer::new();
        let result = runner.apply_fixes_to_content(content, &fixes).unwrap();

        // Verify blank lines were added
        let expected = indoc! {"
            Text

            ```
            code
            ```

            More"};
        assert_eq!(result, expected);
    }
}
