use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;

pub struct MD055;

impl Rule for MD055 {
    fn name(&self) -> &'static str {
        "MD055"
    }

    fn description(&self) -> &'static str {
        "Table pipe style"
    }

    fn tags(&self) -> &[&str] {
        &["table"]
    }

    #[allow(clippy::too_many_lines)] // rule logic requires tracking leading/trailing pipe state per row
    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let style = config
            .and_then(|c| c.get("style"))
            .and_then(|v| v.as_str())
            .unwrap_or("leading_and_trailing");

        let mut violations = Vec::new();
        let mut first_style: Option<&str> = None;
        let code_block_lines = parser.get_code_block_line_numbers();
        let code_ranges = parser.get_code_ranges();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            if code_block_lines.contains(&line_number) {
                continue;
            }

            // Check if line is a table row: it must contain a pipe that isn't
            // inside an inline code span (e.g. `a | b`), otherwise it's just
            // prose that happens to mention a pipe character.
            let has_real_pipe = line.match_indices('|').any(|(byte_offset, _)| {
                let absolute = parser.line_offset_to_absolute(line_number, byte_offset);
                !code_ranges.iter().any(|range| range.contains(&absolute))
            });
            if !has_real_pipe {
                continue;
            }

            let trimmed = line.trim();

            // Determine the style of this line
            let has_leading = trimmed.starts_with('|');
            let has_trailing = trimmed.ends_with('|');

            let current_style = match (has_leading, has_trailing) {
                (true, true) => "leading_and_trailing",
                (true, false) => "leading_only",
                (false, true) => "trailing_only",
                (false, false) => "no_leading_or_trailing",
            };

            if style == "consistent" {
                if let Some(first) = first_style {
                    if current_style != first {
                        // Report separate violations for leading and trailing mismatches
                        let (first_leading, first_trailing) = match first {
                            "leading_and_trailing" => (true, true),
                            "leading_only" => (true, false),
                            "trailing_only" => (false, true),
                            _ => (false, false),
                        };

                        // Check leading pipe
                        if has_leading != first_leading {
                            violations.push(Violation {
                                line: line_number,
                                column: Some(1),
                                rule: self.name().to_owned(),
                                message: format!(
                                    "Table pipe style should be consistent: expected {}, found {}",
                                    if first_leading {
                                        "leading pipe"
                                    } else {
                                        "no leading pipe"
                                    },
                                    if has_leading {
                                        "leading pipe"
                                    } else {
                                        "no leading pipe"
                                    }
                                ),
                                fix: None,
                            });
                        }

                        // Check trailing pipe
                        if has_trailing != first_trailing {
                            violations.push(Violation {
                                line: line_number,
                                column: Some(1),
                                rule: self.name().to_owned(),
                                message: format!(
                                    "Table pipe style should be consistent: expected {}, found {}",
                                    if first_trailing {
                                        "trailing pipe"
                                    } else {
                                        "no trailing pipe"
                                    },
                                    if has_trailing {
                                        "trailing pipe"
                                    } else {
                                        "no trailing pipe"
                                    }
                                ),
                                fix: None,
                            });
                        }
                    }
                } else {
                    first_style = Some(current_style);
                }
            } else if style == "leading_and_trailing" && current_style != "leading_and_trailing" {
                // Report separate violations for missing leading/trailing
                if !has_leading {
                    violations.push(Violation {
                        line: line_number,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "Table should have leading pipe".to_owned(),
                        fix: None,
                    });
                }
                if !has_trailing {
                    violations.push(Violation {
                        line: line_number,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "Table should have trailing pipe".to_owned(),
                        fix: None,
                    });
                }
            } else if style == "no_leading_or_trailing" && (has_leading || has_trailing) {
                // Report separate violations for unwanted leading/trailing
                if has_leading {
                    violations.push(Violation {
                        line: line_number,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "Table should not have leading pipe".to_owned(),
                        fix: None,
                    });
                }
                if has_trailing {
                    violations.push(Violation {
                        line: line_number,
                        column: Some(1),
                        rule: self.name().to_owned(),
                        message: "Table should not have trailing pipe".to_owned(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::rendered;
    use indoc::indoc;

    #[test]
    fn test_consistent_with_pipes() {
        let content = indoc! {"
            | Col1 | Col2 |
            |------|------|
            | A    | B    |"};
        let parser = MarkdownParser::new(content);
        let rule = MD055;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_consistent_without_pipes() {
        let content = indoc! {"
            Col1 | Col2
            -----|-----
            A    | B"};
        let parser = MarkdownParser::new(content);
        let rule = MD055;
        let config = serde_json::json!({ "style": "consistent" });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_inconsistent_pipes() {
        let content = indoc! {"
            | Col1 | Col2 |
            |------|------|
            A    | B"};
        let parser = MarkdownParser::new(content);
        let rule = MD055;
        let violations = rule.check(&parser, None);

        // Last row is inconsistent: reports 2 violations (missing leading and trailing)
        assert_eq!(
            rendered(&violations),
            [
                "test.md:3:1: MD055 Table should have leading pipe",
                "test.md:3:1: MD055 Table should have trailing pipe",
            ]
        );
    }

    #[test]
    fn test_enforced_leading_and_trailing() {
        let content = indoc! {"
            Col1 | Col2
            -----|-----
            A | B"};
        let parser = MarkdownParser::new(content);
        let rule = MD055;
        let config = serde_json::json!({ "style": "leading_and_trailing" });
        let violations = rule.check(&parser, Some(&config));

        // 3 rows (header, separator, data) × 2 violations each (missing leading and trailing)
        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:1: MD055 Table should have leading pipe",
                "test.md:1:1: MD055 Table should have trailing pipe",
                "test.md:2:1: MD055 Table should have leading pipe",
                "test.md:2:1: MD055 Table should have trailing pipe",
                "test.md:3:1: MD055 Table should have leading pipe",
                "test.md:3:1: MD055 Table should have trailing pipe",
            ]
        );
    }

    #[test]
    fn test_simple_table() {
        let content = indoc! {"
            | Header |
            | ------ |
            | Cell   |"};
        let parser = MarkdownParser::new(content);
        let rule = MD055;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_pipe_only_in_inline_code_span_ignored() {
        // https://github.com/swanysimon/mdlint/issues/65
        let content = indoc! {"
            # Example

            This is a line with an `a | b` inline code span."};
        let parser = MarkdownParser::new(content);
        let rule = MD055;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_real_table_with_code_span_pipe_still_flagged() {
        // A genuine table row whose leading/trailing pipes are real, even
        // though it also contains a code span with an internal pipe.
        let content = indoc! {"
            Col1 | `a|b`
            -----|-----
            A | B"};
        let parser = MarkdownParser::new(content);
        let rule = MD055;
        let config = serde_json::json!({ "style": "leading_and_trailing" });
        let violations = rule.check(&parser, Some(&config));

        // 3 rows x 2 violations each (missing leading and trailing pipe)
        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:1: MD055 Table should have leading pipe",
                "test.md:1:1: MD055 Table should have trailing pipe",
                "test.md:2:1: MD055 Table should have leading pipe",
                "test.md:2:1: MD055 Table should have trailing pipe",
                "test.md:3:1: MD055 Table should have leading pipe",
                "test.md:3:1: MD055 Table should have trailing pipe",
            ]
        );
    }
}
