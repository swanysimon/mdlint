use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;

pub struct MD056;

impl Rule for MD056 {
    fn name(&self) -> &'static str {
        "MD056"
    }

    fn description(&self) -> &'static str {
        "Table column count"
    }

    fn tags(&self) -> &[&str] {
        &["table"]
    }

    fn check(&self, parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        let mut violations = Vec::new();
        let lines = parser.lines();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Check if this looks like a table row (contains pipes)
            if !line.contains('|') {
                i += 1;
                continue;
            }

            // Count columns in this row
            let row_columns = count_columns(line);

            // Check if next line is a separator (making this a table header)
            if i + 1 < lines.len() {
                let next_line = lines[i + 1].trim();
                if is_separator_line(next_line) {
                    // This is a table header, verify all subsequent rows
                    let expected_columns = row_columns;
                    let separator_columns = count_columns(next_line);

                    if separator_columns != expected_columns {
                        violations.push(Violation {
                            line: i + 2, // +1 for 1-indexed, +1 for next line
                            column: Some(1),
                            rule: self.name().to_string(),
                            message: format!(
                                "Table separator has {separator_columns} columns, expected {expected_columns}"
                            ),
                            fix: None,
                        });
                    }

                    // Check data rows
                    i += 2; // Skip header and separator
                    while i < lines.len() {
                        let data_line = lines[i].trim();
                        if !data_line.contains('|') || is_separator_line(data_line) {
                            break;
                        }

                        let data_columns = count_columns(data_line);
                        if data_columns != expected_columns {
                            violations.push(Violation {
                                line: i + 1,
                                column: Some(1),
                                rule: self.name().to_string(),
                                message: format!(
                                    "Table row has {data_columns} columns, expected {expected_columns}"
                                ),
                                fix: None,
                            });
                        }

                        i += 1;
                    }
                    continue;
                }
            }

            i += 1;
        }

        violations
    }

    fn fixable(&self) -> bool {
        false
    }
}

/// Count the number of columns in a table row by counting pipe separators
fn count_columns(line: &str) -> usize {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return 0;
    }

    // Count pipes, adjusting for leading/trailing pipes
    let mut count = 1;
    let mut in_escape = false;

    for ch in trimmed.chars() {
        if ch == '\\' && !in_escape {
            in_escape = true;
            continue;
        }
        if ch == '|' && !in_escape {
            count += 1;
        }
        in_escape = false;
    }

    // If line starts with pipe, we overcounted by 1
    if trimmed.starts_with('|') {
        count -= 1;
    }
    // If line ends with pipe, we overcounted by 1
    if trimmed.ends_with('|') && !trimmed.ends_with("\\|") {
        count -= 1;
    }

    count
}

/// Check if a line is a table separator (contains ---)
fn is_separator_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.contains("---") || trimmed.contains(":--") || trimmed.contains("--:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_table() {
        let content = "| Col1 | Col2 | Col3 |\n|------|------|------|\n| A    | B    | C    |\n| D    | E    | F    |";
        let parser = MarkdownParser::new(content);
        let rule = MD056;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_mismatched_columns() {
        let content = "| Col1 | Col2 |\n|------|------|\n| A    | B    | C    |";
        let parser = MarkdownParser::new(content);
        let rule = MD056;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn test_separator_mismatch() {
        let content = "| Col1 | Col2 | Col3 |\n|------|------|\n| A    | B    | C    |";
        let parser = MarkdownParser::new(content);
        let rule = MD056;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1); // Separator has wrong column count
    }

    #[test]
    fn test_multiple_rows() {
        let content = "| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 | 5 |\n| 6 | 7 |";
        let parser = MarkdownParser::new(content);
        let rule = MD056;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 1); // Only middle row is wrong
    }

    #[test]
    fn test_no_table() {
        let content = "This is just text without tables.";
        let parser = MarkdownParser::new(content);
        let rule = MD056;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }
}
