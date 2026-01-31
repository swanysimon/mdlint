use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use serde_json::Value;

pub struct MD031;

impl Rule for MD031 {
    fn name(&self) -> &str {
        "MD031"
    }

    fn description(&self) -> &str {
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
                Event::End(TagEnd::CodeBlock) => {
                    if in_fenced_block {
                        let line = parser.offset_to_line(range.end);
                        code_block_ends.push(line);
                        in_fenced_block = false;
                    }
                }
                _ => {}
            }
        }

        // Check each code block
        for &start_line in &code_block_starts {
            let line_idx = start_line - 1;

            // Check blank line before (skip if first line)
            if line_idx > 0 {
                let prev_line = lines[line_idx - 1].trim();
                if !prev_line.is_empty() {
                    // Insert blank line before code block
                    violations.push(Violation {
                        line: start_line,
                        column: Some(1),
                        rule: self.name().to_string(),
                        message:
                            "Fenced code blocks should be surrounded by blank lines (missing before)"
                                .to_string(),
                        fix: Some(Fix {
                            line_start: start_line,
                            line_end: start_line,
                            column_start: None,
                            column_end: None,
                            replacement: format!("\n{}", lines[line_idx]),
                            description: "Add blank line before code block".to_string(),
                        }),
                    });
                }
            }
        }

        for &end_line in &code_block_ends {
            let line_idx = end_line - 1;

            // Check blank line after (skip if last line)
            if line_idx + 1 < lines.len() {
                let next_line = lines[line_idx + 1].trim();
                if !next_line.is_empty() {
                    // Insert blank line after code block
                    violations.push(Violation {
                        line: end_line,
                        column: Some(1),
                        rule: self.name().to_string(),
                        message:
                            "Fenced code blocks should be surrounded by blank lines (missing after)"
                                .to_string(),
                        fix: Some(Fix {
                            line_start: end_line,
                            line_end: end_line,
                            column_start: None,
                            column_end: None,
                            replacement: format!("{}\n", lines[line_idx]),
                            description: "Add blank line after code block".to_string(),
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

    #[test]
    fn test_properly_surrounded() {
        let content = "Text\n\n```\ncode\n```\n\nMore text";
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_missing_blank_before() {
        let content = "Text\n```\ncode\n```\n\nMore text";
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        assert!(!violations.is_empty());
        assert!(violations.iter().any(|v| v.message.contains("before")));
    }

    #[test]
    fn test_missing_blank_after() {
        let content = "Text\n\n```\ncode\n```\nMore text";
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        assert!(!violations.is_empty());
        assert!(violations.iter().any(|v| v.message.contains("after")));
    }

    #[test]
    fn test_first_line() {
        let content = "```\ncode\n```\n\nText";
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // First line exempt from "before" check
    }

    #[test]
    fn test_numbered_list_with_code_block() {
        // Test that code blocks in numbered lists get proper fixes
        let content = "1. **Enable/Disable a rule:**\n   ```toml\n   [rules.MD013]\n   enabled = false\n   ```\n\n2. **Next item**";
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        // Should detect missing blank line before code block
        assert!(!violations.is_empty());
        assert!(violations.iter().any(|v| v.message.contains("before")));

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

        let content = "Text\n```\ncode\n```\nMore";
        let parser = MarkdownParser::new(content);
        let rule = MD031;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2); // Missing before and after

        // Apply fixes
        let fixes: Vec<_> = violations.iter().filter_map(|v| v.fix.clone()).collect();
        let fixer = Fixer::new();
        let fixed = fixer.apply_fixes_to_content(content, &fixes).unwrap();

        // Verify blank lines were added
        let expected = "Text\n\n```\ncode\n```\n\nMore";
        assert_eq!(fixed, expected);
    }
}
