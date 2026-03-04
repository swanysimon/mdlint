use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::{Fix, Violation};
use serde_json::Value;

pub struct MD050;

impl Rule for MD050 {
    fn name(&self) -> &str {
        "MD050"
    }

    fn description(&self) -> &str {
        "Strong style should be consistent"
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
        let mut first_style: Option<&str> = None;

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            // Look for strong patterns: **text** or __text__
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;

            while i + 1 < chars.len() {
                // Check for ** or __
                if i + 1 < chars.len() {
                    let two_char = format!("{}{}", chars[i], chars[i + 1]);

                    if two_char == "**" || two_char == "__" {
                        // Find closing marker
                        let mut found_close = false;
                        for j in (i + 2)..chars.len().saturating_sub(1) {
                            if j + 1 < chars.len() {
                                let close_two = format!("{}{}", chars[j], chars[j + 1]);
                                if close_two == two_char {
                                    found_close = true;

                                    // Track style
                                    let current_style = if two_char == "**" {
                                        "asterisk"
                                    } else {
                                        "underscore"
                                    };

                                    let make_fix = |col: usize, target: &str| Fix {
                                        line_start: line_number,
                                        line_end: line_number,
                                        column_start: Some(col),
                                        column_end: Some(col + 1),
                                        replacement: target.to_string(),
                                        description: "Replace strong marker".to_string(),
                                    };

                                    if style == "consistent" {
                                        if let Some(first) = first_style {
                                            if current_style != first {
                                                let expected_marker =
                                                    if first == "asterisk" { "**" } else { "__" };
                                                // Report violation for both opening and closing markers
                                                violations.push(Violation {
                                                    line: line_number,
                                                    column: Some(i + 1),
                                                    rule: self.name().to_string(),
                                                    message: format!(
                                                        "Strong style should be consistent: expected '{}', found '{}'",
                                                        expected_marker, two_char
                                                    ),
                                                    fix: Some(make_fix(i + 1, expected_marker)),
                                                });
                                                violations.push(Violation {
                                                    line: line_number,
                                                    column: Some(j + 1),
                                                    rule: self.name().to_string(),
                                                    message: format!(
                                                        "Strong style should be consistent: expected '{}', found '{}'",
                                                        expected_marker, close_two
                                                    ),
                                                    fix: Some(make_fix(j + 1, expected_marker)),
                                                });
                                            }
                                        } else {
                                            first_style = Some(current_style);
                                        }
                                    } else {
                                        let expected_marker =
                                            if style == "asterisk" { "**" } else { "__" };
                                        if two_char != expected_marker {
                                            // Report violation for both opening and closing markers
                                            violations.push(Violation {
                                                line: line_number,
                                                column: Some(i + 1),
                                                rule: self.name().to_string(),
                                                message: format!(
                                                    "Strong style should be '{}', found '{}'",
                                                    expected_marker, two_char
                                                ),
                                                fix: Some(make_fix(i + 1, expected_marker)),
                                            });
                                            violations.push(Violation {
                                                line: line_number,
                                                column: Some(j + 1),
                                                rule: self.name().to_string(),
                                                message: format!(
                                                    "Strong style should be '{}', found '{}'",
                                                    expected_marker, close_two
                                                ),
                                                fix: Some(make_fix(j + 1, expected_marker)),
                                            });
                                        }
                                    }

                                    i = j + 1; // Skip to after closing
                                    break;
                                }
                            }
                        }

                        if found_close {
                            i += 1;
                            continue;
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
        let content = "This is **bold** and **more bold**.";
        let parser = MarkdownParser::new(content);
        let rule = MD050;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_consistent_underscore() {
        let content = "This is __bold__ and __more bold__.";
        let parser = MarkdownParser::new(content);
        let rule = MD050;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inconsistent() {
        let content = "This is **bold** and __also bold__.";
        let parser = MarkdownParser::new(content);
        let rule = MD050;
        let violations = rule.check(&parser, None);

        // Reports violation for both opening and closing markers of the second strong emphasis
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_enforced_style() {
        let content = "This is __bold__ text.";
        let parser = MarkdownParser::new(content);
        let rule = MD050;
        let config = serde_json::json!({ "style": "asterisk" });
        let violations = rule.check(&parser, Some(&config));

        // Reports violation for both opening and closing markers
        assert_eq!(violations.len(), 2);
    }
}
