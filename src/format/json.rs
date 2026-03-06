use crate::format::Formatter;
use crate::lint::LintResult;
use crate::types::FileResult;
use serde::Serialize;

pub struct JsonFormatter {
    pretty: bool,
}

impl JsonFormatter {
    pub fn new(pretty: bool) -> Self {
        Self { pretty }
    }
}

fn file_violations(file_result: &FileResult) -> Vec<JsonViolation> {
    file_result
        .violations
        .iter()
        .map(|violation| JsonViolation {
            line: violation.line,
            column: violation.column,
            rule: violation.rule.clone(),
            message: violation.message.clone(),
            fixable: violation.fix.as_ref().map(|_| true),
        })
        .collect()
}

#[derive(Serialize)]
struct JsonOutput {
    files: Vec<JsonFile>,
    total_errors: usize,
}

#[derive(Serialize)]
struct JsonFile {
    path: String,
    violations: Vec<JsonViolation>,
}

#[derive(Serialize)]
struct JsonViolation {
    line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<usize>,
    rule: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fixable: Option<bool>,
}

impl Formatter for JsonFormatter {
    fn format(&self, result: &LintResult) -> String {
        let json_output = JsonOutput {
            files: result
                .file_results
                .iter()
                .map(|file_result| JsonFile {
                    path: file_result.path.display().to_string(),
                    violations: file_violations(file_result),
                })
                .collect(),
            total_errors: result.total_errors,
        };

        if self.pretty {
            serde_json::to_string_pretty(&json_output)
                .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize JSON: {}\"}}", e))
        } else {
            serde_json::to_string(&json_output)
                .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize JSON: {}\"}}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Violation;
    use std::path::PathBuf;

    #[test]
    fn test_empty_result() {
        let formatter = JsonFormatter::new(false);
        let result = LintResult::new();
        let output = formatter.format(&result);

        assert!(output.contains("\"total_errors\":0"));
        assert!(output.contains("\"files\":[]"));
    }

    #[test]
    fn test_single_violation() {
        let formatter = JsonFormatter::new(false);
        let mut result = LintResult::new();

        result.add_file_result(
            PathBuf::from("test.md"),
            vec![Violation {
                line: 5,
                column: Some(10),
                rule: "MD001".to_string(),
                message: "Test message".to_string(),
                fix: None,
            }],
            vec![],
        );

        let output = formatter.format(&result);

        assert!(output.contains("\"line\":5"));
        assert!(output.contains("\"column\":10"));
        assert!(output.contains("\"rule\":\"MD001\""));
        assert!(output.contains("\"message\":\"Test message\""));
        assert!(output.contains("\"total_errors\":1"));
    }

    #[test]
    fn test_pretty_print() {
        let formatter = JsonFormatter::new(true);
        let mut result = LintResult::new();

        result.add_file_result(
            PathBuf::from("test.md"),
            vec![Violation {
                line: 1,
                column: None,
                rule: "MD001".to_string(),
                message: "Test".to_string(),
                fix: None,
            }],
            vec![],
        );

        let output = formatter.format(&result);

        // Pretty print should have indentation
        assert!(output.contains("  ") || output.contains("\n"));
    }

    #[test]
    fn test_fixable_flag() {
        let formatter = JsonFormatter::new(false);
        let mut result = LintResult::new();

        result.add_file_result(
            PathBuf::from("test.md"),
            vec![Violation {
                line: 1,
                column: Some(1),
                rule: "MD009".to_string(),
                message: "Trailing spaces".to_string(),
                fix: Some(crate::types::Fix {
                    line_start: 1,
                    line_end: 1,
                    column_start: None,
                    column_end: None,
                    replacement: "fixed".to_string(),
                    description: "Remove trailing spaces".to_string(),
                }),
            }],
            vec![],
        );

        let output = formatter.format(&result);

        assert!(output.contains("\"fixable\":true"));
    }
}
