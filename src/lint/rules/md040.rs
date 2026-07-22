use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::{CodeBlockKind, Event, Tag};
use serde_json::Value;

pub struct MD040;

impl Rule for MD040 {
    fn name(&self) -> &'static str {
        "MD040"
    }

    fn description(&self) -> &'static str {
        "Fenced code blocks should have a language specified"
    }

    fn tags(&self) -> &[&str] {
        &["code", "language"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let allowed_languages: Option<Vec<String>> = config
            .and_then(|c| c.get("allowed_languages"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            });

        let mut violations = Vec::new();

        for (event, range) in parser.parse_with_offsets() {
            if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) = event {
                let lang_str = lang.to_string();
                let line = parser.offset_to_line(range.start);

                if lang_str.is_empty() {
                    // Always report code blocks without a language
                    violations.push(Violation {
                        line,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "Fenced code block should have a language specified".to_owned(),
                        fix: None,
                    });
                } else if let Some(ref allowed) = allowed_languages {
                    // If allowed_languages is specified, check if lang is in the list
                    if !allowed.contains(&lang_str.to_lowercase()) {
                        violations.push(Violation {
                            line,
                            column: Some(1),
                            rule: self.name().to_owned(),
                            message: format!("Language '{lang_str}' is not in the allowed list"),
                            fix: None,
                        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_language() {
        let content = "```rust\nlet x = 5;\n```";
        let parser = MarkdownParser::new(content);
        let rule = MD040;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_without_language() {
        let content = "```\ncode here\n```";
        let parser = MarkdownParser::new(content);
        let rule = MD040;
        let config = serde_json::json!({ "allowed_languages": ["rust", "python"] });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("should have a language"));
    }

    #[test]
    fn test_allowed_languages() {
        let content = "```javascript\ncode here\n```";
        let parser = MarkdownParser::new(content);
        let rule = MD040;
        let config = serde_json::json!({ "allowed_languages": ["rust", "python"] });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("not in the allowed list"));
    }

    #[test]
    fn test_indented_code_block() {
        let content = "    indented code\n    more code";
        let parser = MarkdownParser::new(content);
        let rule = MD040;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // Indented blocks are ignored
    }
}
