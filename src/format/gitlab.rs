use crate::{format::Formatter};
use crate::lint::LintResult;
use crate::types::FileResult;
use hex::ToHex;
use serde::Serialize;
use sha2::{Sha256, Digest};
use std::env;
use std::path::PathBuf;

pub struct GitlabFormatter {
    pretty: bool,
}

impl GitlabFormatter {
    pub fn new(pretty: bool) -> Self {
        Self { pretty }
    }
}

fn file_violations(file_result: &FileResult, path_relative: &str) -> Vec<GitlabViolation> {
    file_result.violations
        .iter()
        .map(|violation| {
            let key = format!("{}:{}", path_relative, violation.line);
            GitlabViolation {
                description: violation.message.clone(),
                check_name: violation.rule.clone(),
                fingerprint: create_fingerprint(&key),
                location: GitlabLocation {
                    path: path_relative.to_string(),
                    lines: GitlabLines {
                        begin: violation.line,
                    },
                },
                severity: if violation.fix.is_some() {
                    Severity::Minor
                } else {
                    Severity::Major
                },
            }
        })
        .collect()
}

fn create_fingerprint(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize().to_vec();
    result.encode_hex::<String>()
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Severity {
    #[allow(dead_code)]
    Info,
    Minor,
    Major,
    #[allow(dead_code)]
    Critical,
    #[allow(dead_code)]
    Blocker,
}

#[derive(Serialize)]
struct GitlabViolation {
    description: String,
    check_name: String,
    fingerprint: String,
    location: GitlabLocation,
    severity: Severity,
}

#[derive(Serialize)]
struct GitlabLocation {
    path: String,
    lines: GitlabLines,
}

#[derive(Serialize)]
struct GitlabLines {
    begin: usize,
}

impl Formatter for GitlabFormatter {
    fn format(&self, result: &LintResult) -> String {
        let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from(""));
        let violations: Vec<GitlabViolation> = result.file_results
            .iter()
            .flat_map(|file_result| {
                let path_relative = file_result
                    .path
                    .strip_prefix(&current_dir)
                    .map(|rel_path| rel_path.to_path_buf())
                    .unwrap_or_else(|_| file_result.path.clone())
                    .display()
                    .to_string();

                file_violations(file_result, &path_relative)
            })
            .collect();

        if self.pretty {
            serde_json::to_string_pretty(&violations)
                .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize JSON: {}\"}}", e))
        } else {
            serde_json::to_string(&violations)
                .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize JSON: {}\"}}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Violation;

    #[test]
    fn test_empty_result() {
        let formatter = GitlabFormatter::new(false);
        let result = LintResult::new();
        let output = formatter.format(&result);

        assert!(output.eq("[]"));
    }

    #[test]
    fn test_single_violation() {
        let formatter = GitlabFormatter::new(false);
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
        let fingerprint = create_fingerprint("test.md:5");

        assert!(output.contains("\"description\":\"Test message\""));
        assert!(output.contains("\"check_name\":\"MD001\""));
        assert!(output.contains(&format!("\"fingerprint\":\"{}\"", fingerprint)));
        assert!(output.contains("\"location\":{\"path\":\"test.md\","));
        assert!(output.contains("\"lines\":{\"begin\":5"));
        assert!(output.contains("\"severity\":\"major\""));
    }

    #[test]
    fn test_pretty_print() {
        let formatter = GitlabFormatter::new(true);
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
    fn test_fixable_severity() {
        let formatter = GitlabFormatter::new(false);
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

        assert!(output.contains("\"severity\":\"minor\""));
    }
}
