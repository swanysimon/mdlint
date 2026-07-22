use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use pulldown_cmark::{Event, Tag, TagEnd};
use serde_json::Value;

pub struct MD010;

impl Rule for MD010 {
    fn name(&self) -> &'static str {
        "MD010"
    }

    fn description(&self) -> &'static str {
        "Hard tabs"
    }

    fn tags(&self) -> &[&str] {
        &["whitespace", "hard_tab"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let code_blocks = config
            .and_then(|c| c.get("code_blocks"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        let mut violations = Vec::new();
        let mut in_code_block = false;

        // Track code blocks using the parser
        let mut code_block_lines = std::collections::HashSet::new();

        if !code_blocks {
            for (event, range) in parser.parse_with_offsets() {
                let current_line = parser.offset_to_line(range.start);

                match event {
                    Event::Start(Tag::CodeBlock(_)) => {
                        in_code_block = true;
                    }
                    Event::End(TagEnd::CodeBlock) => {
                        in_code_block = false;
                    }
                    Event::Text(_) if in_code_block => {
                        code_block_lines.insert(current_line);
                    }
                    _ => {}
                }
            }
        }

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            // Skip code blocks if configured
            if !code_blocks && code_block_lines.contains(&line_number) {
                continue;
            }

            if let Some(tab_pos) = line.find('\t') {
                violations.push(Violation {
                    line: line_number,
                    column: Some(tab_pos + 1),
                    rule: self.name().to_owned(),
                    message: "Hard tabs found".to_owned(),
                    fix: Some(Fix {
                        line_start: line_number,
                        line_end: line_number,
                        column_start: None,
                        column_end: None,
                        replacement: line.replace('\t', "    "),
                        description: "Replace tabs with spaces".to_owned(),
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

    fn apply_fixes(content: &str, violations: &[Violation]) -> String {
        let fixes: Vec<_> = violations.iter().filter_map(|v| v.fix.clone()).collect();
        Fixer::new()
            .apply_fixes_to_content(content, &fixes)
            .unwrap()
    }

    #[test]
    fn test_no_tabs() {
        let content = "Line 1\n    Line 2\nLine 3";
        let parser = MarkdownParser::new(content);
        let rule = MD010;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_hard_tabs() {
        let content = "Line 1\n\tLine 2\nLine 3";
        let parser = MarkdownParser::new(content);
        let rule = MD010;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 2);
        assert_eq!(violations[0].column, Some(1));
    }

    #[test]
    fn test_tabs_in_code_block() {
        let content = "Text\n```\n\tcode\n```";
        let parser = MarkdownParser::new(content);
        let rule = MD010;
        let violations = rule.check(&parser, None);

        // By default, code_blocks is true, so tabs in code blocks are violations
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_ignore_code_blocks() {
        let content = "Text\n```\n\tcode\n```";
        let parser = MarkdownParser::new(content);
        let rule = MD010;
        let config = serde_json::json!({ "code_blocks": false });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_fix_replaces_tab_with_spaces() {
        let content = "# Heading\n\n\tTabbed line\n";
        let parser = MarkdownParser::new(content);
        let rule = MD010;
        let violations = rule.check(&parser, None);
        let fixed = apply_fixes(content, &violations);
        assert!(!fixed.contains('\t'), "tabs should be replaced after fix");
        assert!(
            fixed.contains("    Tabbed line"),
            "tab should become 4 spaces"
        );
    }
}
