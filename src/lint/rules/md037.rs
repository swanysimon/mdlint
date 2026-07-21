use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use pulldown_cmark::Event;
use serde_json::Value;

pub struct MD037;

impl MD037 {
    fn check_pattern(
        text: &str,
        base_offset: usize,
        marker: &str,
        parser: &MarkdownParser,
        violations: &mut Vec<Violation>,
    ) {
        let marker_len = marker.len();
        let open_pattern = format!("{marker} ");
        let close_pattern = format!(" {marker}");

        let mut offset = 0;
        while let Some(pos) = text[offset..].find(&open_pattern) {
            let start_pos = offset + pos;
            let search_start = start_pos + marker_len + 1; // After "marker "

            if let Some(end_offset) = text[search_start..].find(&close_pattern) {
                let end_pos = search_start + end_offset;
                let between = &text[search_start..end_pos];

                // Make sure there are no emphasis markers in between (prevents matching across blocks)
                if !between.contains(marker) {
                    // Found opening marker with space
                    let abs_start = base_offset + start_pos;
                    let (line, col) = parser.offset_to_position(abs_start);
                    violations.push(Violation {
                        line,
                        column: Some(col),
                        rule: "MD037".to_string(),
                        message: "Spaces inside emphasis markers".to_string(),
                        fix: None,
                    });

                    // Found closing marker with space
                    let abs_end = base_offset + end_pos;
                    let (end_line, end_col) = parser.offset_to_position(abs_end);
                    violations.push(Violation {
                        line: end_line,
                        column: Some(end_col),
                        rule: "MD037".to_string(),
                        message: "Spaces inside emphasis markers".to_string(),
                        fix: None,
                    });
                }
            }
            offset = start_pos + 1;
        }
    }

    fn check_single_marker_pattern(
        text: &str,
        base_offset: usize,
        marker: &str,
        parser: &MarkdownParser,
        violations: &mut Vec<Violation>,
    ) {
        let open_pattern = format!("{marker} ");
        let close_pattern = format!(" {marker}");
        let double_marker = marker.repeat(2);

        let mut offset = 0;
        while let Some(pos) = text[offset..].find(&open_pattern) {
            let start_pos = offset + pos;

            // Check if this is part of a double marker (e.g., ** when looking for *)
            let before = if start_pos > 0 {
                &text[start_pos.saturating_sub(1)..start_pos]
            } else {
                ""
            };
            let after_marker = start_pos + marker.len();
            let after = if after_marker + 1 < text.len() {
                &text[after_marker + 1..after_marker + 2]
            } else {
                ""
            };

            // Skip if this is part of a double marker
            if before == marker || after == marker {
                offset = start_pos + 1;
                continue;
            }

            let search_start = start_pos + marker.len() + 1; // After "marker "

            if let Some(end_offset) = text[search_start..].find(&close_pattern) {
                let end_pos = search_start + end_offset;
                let between = &text[search_start..end_pos];

                // Make sure there are no markers in between
                if !between.contains(marker) && !between.contains(&double_marker) {
                    // Check the closing marker isn't part of a double marker
                    let before_close = end_pos.saturating_sub(1);
                    let after_close_marker = end_pos + marker.len() + 1;
                    let before_close_char = if before_close < search_start {
                        ""
                    } else {
                        &text[before_close..=before_close]
                    };
                    let after_close_char = if after_close_marker < text.len() {
                        &text[after_close_marker..=after_close_marker]
                    } else {
                        ""
                    };

                    if before_close_char == marker || after_close_char == marker {
                        offset = start_pos + 1;
                        continue;
                    }

                    // Found opening marker with space
                    let abs_start = base_offset + start_pos;
                    let (line, col) = parser.offset_to_position(abs_start);
                    violations.push(Violation {
                        line,
                        column: Some(col),
                        rule: "MD037".to_string(),
                        message: "Spaces inside emphasis markers".to_string(),
                        fix: None,
                    });

                    // Found closing marker with space
                    let abs_end = base_offset + end_pos;
                    let (end_line, end_col) = parser.offset_to_position(abs_end);
                    violations.push(Violation {
                        line: end_line,
                        column: Some(end_col),
                        rule: "MD037".to_string(),
                        message: "Spaces inside emphasis markers".to_string(),
                        fix: None,
                    });
                }
            }
            offset = start_pos + 1;
        }
    }
}

impl Rule for MD037 {
    fn name(&self) -> &'static str {
        "MD037"
    }

    fn description(&self) -> &'static str {
        "Spaces inside emphasis markers"
    }

    fn tags(&self) -> &[&str] {
        &["whitespace", "emphasis"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();

        // Emphasis with spaces inside is not parsed as emphasis by pulldown-cmark,
        // so we need to find these patterns in Text events (but not in code)
        let events: Vec<_> = parser.parse_with_offsets().collect();
        let code_ranges = parser.get_code_ranges();

        for (event, range) in &events {
            if let Event::Text(text) = event {
                let text_str = text.as_ref();
                let offset = range.start;

                // Skip if this text is inside code
                let in_code = code_ranges.iter().any(|r| r.contains(&offset));
                if in_code {
                    continue;
                }

                // Check for ** X ** pattern (strong with spaces)
                Self::check_pattern(text_str, offset, "**", parser, &mut violations);

                // Check for __ X __ pattern (strong with underscores)
                Self::check_pattern(text_str, offset, "__", parser, &mut violations);

                // Check for * X * pattern (emphasis with spaces) - but avoid matching inside **
                Self::check_single_marker_pattern(text_str, offset, "*", parser, &mut violations);

                // Check for _ X _ pattern (emphasis with underscores) - but avoid matching inside __
                Self::check_single_marker_pattern(text_str, offset, "_", parser, &mut violations);
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
    fn test_correct_emphasis() {
        let content = "This is **bold** and *italic* text.";
        let parser = MarkdownParser::new(content);
        let rule = MD037;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_spaces_in_strong() {
        let content = "This is ** bold ** text.";
        let parser = MarkdownParser::new(content);
        let rule = MD037;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2); // One for opening, one for closing
    }

    #[test]
    fn test_spaces_in_emphasis() {
        let content = "This is * italic * text.";
        let parser = MarkdownParser::new(content);
        let rule = MD037;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2); // One for opening, one for closing
    }

    #[test]
    fn test_underscores() {
        let content = "This is __ bold __ text.";
        let parser = MarkdownParser::new(content);
        let rule = MD037;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 2); // One for opening, one for closing
    }

    #[test]
    fn test_code_block_with_underscores() {
        let content = "Normal text\n\n```sql\nCREATE POLICY territory_contact_access ON contacts\n  FOR SELECT\n  USING (\n    territory_id IN (\n      SELECT territory_id\n      FROM user_territory_assignments\n      WHERE user_id = current_setting('app.current_user_id')::uuid\n        AND (valid_to IS NULL OR valid_to > NOW())\n    )\n  );\n```\n\nMore text";
        let parser = MarkdownParser::new(content);
        let rule = MD037;
        let violations = rule.check(&parser, None);

        // Should not flag underscores in SQL identifiers as emphasis
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inline_code_with_underscores() {
        let content = "Use the `user_id` variable for * 2 * 3 multiplication.";
        let parser = MarkdownParser::new(content);
        let rule = MD037;
        let violations = rule.check(&parser, None);

        // The * 2 * 3 part should be flagged (not in code), but user_id should not
        assert_eq!(violations.len(), 2); // Only the * 2 * emphasis
    }

    #[test]
    fn test_typescript_multiplication() {
        let content = "```typescript\nconst result = value_a * value_b * value_c;\n```";
        let parser = MarkdownParser::new(content);
        let rule = MD037;
        let violations = rule.check(&parser, None);

        // Should not flag asterisks in code as emphasis markers
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_adjacent_emphasis_blocks() {
        let content = "**CASL** or **Permify**: Attribute-based access control";
        let parser = MarkdownParser::new(content);
        let rule = MD037;
        let violations = rule.check(&parser, None);

        // Should not match across emphasis blocks (** or ** is not a single emphasis)
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_multiple_bold_words() {
        let content = "Use **bold** and **more bold** text.";
        let parser = MarkdownParser::new(content);
        let rule = MD037;
        let violations = rule.check(&parser, None);

        // Should not flag correctly formatted adjacent bold sections
        assert_eq!(violations.len(), 0);
    }
}
