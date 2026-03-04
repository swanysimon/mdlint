use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD049;

impl Rule for MD049 {
    fn name(&self) -> &str {
        "MD049"
    }

    fn description(&self) -> &str {
        "Emphasis style should be consistent"
    }

    fn tags(&self) -> &[&str] {
        &["emphasis"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let style = config
            .and_then(|c| c.get("style"))
            .and_then(|v| v.as_str())
            .unwrap_or("consistent");

        let mut violations = Vec::new();
        let mut first_style: Option<char> = None;

        // Get byte ranges that are in code (more precise than line numbers)
        let code_ranges = parser.get_code_ranges();

        // Helper function to check if a position is within code
        let is_in_code = |line_num: usize, byte_offset: usize| -> bool {
            let absolute_offset = parser.line_offset_to_absolute(line_num, byte_offset);
            code_ranges
                .iter()
                .any(|range| range.contains(&absolute_offset))
        };

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            // Look for emphasis patterns: *text* or _text_ (not ** or __)
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;

            while i < chars.len() {
                let ch = chars[i];

                // Check for single * or _ (emphasis, not strong)
                if (ch == '*' || ch == '_') && i + 1 < chars.len() {
                    // Make sure it's not strong (**  or __)
                    let is_strong = (i + 1 < chars.len() && chars[i + 1] == ch)
                        || (i > 0 && chars[i - 1] == ch);

                    // For `_`, apply CommonMark left-flanking rule: the opening `_` must not
                    // be preceded by an alphanumeric character (spec section 6.2). This
                    // prevents snake_case words from being treated as emphasis.
                    let can_open = ch == '*' || i == 0 || !chars[i - 1].is_alphanumeric();

                    if !is_strong && can_open {
                        // Find closing marker
                        for j in (i + 1)..chars.len() {
                            if chars[j] == ch {
                                // Make sure closing is also not strong
                                let close_is_strong = (j + 1 < chars.len() && chars[j + 1] == ch)
                                    || (j > 0 && chars[j - 1] == ch);

                                // For `_`, apply CommonMark right-flanking rule: the closing
                                // `_` must not be followed by an alphanumeric character.
                                let can_close = ch == '*'
                                    || j + 1 >= chars.len()
                                    || !chars[j + 1].is_alphanumeric();

                                if !close_is_strong && can_close {
                                    // Skip if this emphasis is inside code
                                    if is_in_code(line_number, i) {
                                        i = j; // Skip to after closing
                                        break;
                                    }

                                    // Track style and report violations for both opening and closing
                                    let make_fix = |col: usize, target: char| Fix {
                                        line_start: line_number,
                                        line_end: line_number,
                                        column_start: Some(col),
                                        column_end: Some(col),
                                        replacement: target.to_string(),
                                        description: "Replace emphasis marker".to_string(),
                                    };

                                    if style == "consistent" {
                                        if let Some(first) = first_style {
                                            if ch != first {
                                                violations.push(Violation {
                                                    line: line_number,
                                                    column: Some(i + 1),
                                                    rule: self.name().to_string(),
                                                    message: format!(
                                                        "Emphasis style should be consistent: expected '{}', found '{}'",
                                                        first, ch
                                                    ),
                                                    fix: Some(make_fix(i + 1, first)),
                                                });
                                                violations.push(Violation {
                                                    line: line_number,
                                                    column: Some(j + 1),
                                                    rule: self.name().to_string(),
                                                    message: format!(
                                                        "Emphasis style should be consistent: expected '{}', found '{}'",
                                                        first, ch
                                                    ),
                                                    fix: Some(make_fix(j + 1, first)),
                                                });
                                            }
                                        } else {
                                            first_style = Some(ch);
                                        }
                                    } else {
                                        let expected = if style == "asterisk" { '*' } else { '_' };
                                        if ch != expected {
                                            violations.push(Violation {
                                                line: line_number,
                                                column: Some(i + 1),
                                                rule: self.name().to_string(),
                                                message: format!(
                                                    "Emphasis style should be '{}', found '{}'",
                                                    expected, ch
                                                ),
                                                fix: Some(make_fix(i + 1, expected)),
                                            });
                                            violations.push(Violation {
                                                line: line_number,
                                                column: Some(j + 1),
                                                rule: self.name().to_string(),
                                                message: format!(
                                                    "Emphasis style should be '{}', found '{}'",
                                                    expected, ch
                                                ),
                                                fix: Some(make_fix(j + 1, expected)),
                                            });
                                        }
                                    }

                                    i = j; // Skip to after closing
                                    break;
                                }
                            }
                        }
                    }
                }

                i += 1;
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
    fn test_consistent_asterisk() {
        let content = "This is *italic* and *more italic*.";
        let parser = MarkdownParser::new(content);
        let rule = MD049;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_consistent_underscore() {
        let content = "This is _italic_ and _more italic_.";
        let parser = MarkdownParser::new(content);
        let rule = MD049;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inconsistent() {
        let content = "This is *italic* and _also italic_.";
        let parser = MarkdownParser::new(content);
        let rule = MD049;
        let violations = rule.check(&parser, None);

        // Reports violation for both opening and closing markers of the second emphasis
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_enforced_style() {
        let content = "This is _italic_ text.";
        let parser = MarkdownParser::new(content);
        let rule = MD049;
        let config = serde_json::json!({ "style": "asterisk" });
        let violations = rule.check(&parser, Some(&config));

        // Reports violation for both opening and closing markers
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_code_block_with_underscores() {
        let content = "Normal text\n\n```sql\nCREATE POLICY territory_contact_access ON contacts\n  FOR SELECT\n  USING (\n    territory_id IN (\n      SELECT territory_id\n      FROM user_territory_assignments\n      WHERE user_id = current_setting('app.current_user_id')::uuid\n        AND (valid_to IS NULL OR valid_to > NOW())\n    )\n  );\n```\n\nMore text";
        let parser = MarkdownParser::new(content);
        let rule = MD049;
        let violations = rule.check(&parser, None);

        // Should not flag underscores in SQL identifiers as emphasis
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inline_code_with_underscores() {
        let content = "Use the `user_id` variable in *bold* text.";
        let parser = MarkdownParser::new(content);
        let rule = MD049;
        let violations = rule.check(&parser, None);

        // Should not flag underscores in inline code
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_snake_case_not_flagged() {
        // Underscores inside words (snake_case) must not be treated as emphasis:
        // CommonMark spec §6.2 says a `_` can only open emphasis when not preceded
        // by an alphanumeric character.
        let content = "Call offset_to_line, parse_with_offsets(), and heading_line_length.";
        let parser = MarkdownParser::new(content);
        let rule = MD049;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_underscore_emphasis_in_punctuation_context() {
        // `_italic_` after `(` should still be treated as emphasis.
        let content = "See (_italic_) for details.";
        let parser = MarkdownParser::new(content);
        let rule = MD049;
        let config = serde_json::json!({ "style": "asterisk" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_typescript_multiplication() {
        let content = "```typescript\nconst result = value_a * value_b * value_c;\n```";
        let parser = MarkdownParser::new(content);
        let rule = MD049;
        let violations = rule.check(&parser, None);

        // Should not flag asterisks in code as emphasis markers
        assert_eq!(violations.len(), 0);
    }
}
