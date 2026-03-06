use crate::format::Formatter;
use crate::lint::LintResult;

pub struct DefaultFormatter {
    use_color: bool,
    /// Show the offending source line and column indicator under each violation.
    show_context: bool,
}

impl DefaultFormatter {
    pub fn new(use_color: bool) -> Self {
        Self {
            use_color,
            show_context: true,
        }
    }

    pub fn without_context(use_color: bool) -> Self {
        Self {
            use_color,
            show_context: false,
        }
    }

    fn colorize(&self, text: &str, color_code: &str) -> String {
        if self.use_color {
            format!("\x1b[{}m{}\x1b[0m", color_code, text)
        } else {
            text.to_string()
        }
    }

    fn red(&self, text: &str) -> String {
        self.colorize(text, "31")
    }

    fn yellow(&self, text: &str) -> String {
        self.colorize(text, "33")
    }

    fn gray(&self, text: &str) -> String {
        self.colorize(text, "90")
    }
}

impl Formatter for DefaultFormatter {
    fn format(&self, result: &LintResult) -> String {
        let mut output = String::new();

        // Output violations by file
        for file_result in &result.file_results {
            if file_result.violations.is_empty() {
                continue;
            }

            // File path header
            let path_display = file_result.path.display();
            output.push_str(&format!("{}\n", self.yellow(&path_display.to_string())));

            // Each violation
            for violation in &file_result.violations {
                let location = if let Some(col) = violation.column {
                    format!("{}:{}", violation.line, col)
                } else {
                    format!("{}", violation.line)
                };

                output.push_str(&format!(
                    "  {}: {} {}\n",
                    self.gray(&location),
                    self.red(&violation.rule),
                    violation.message
                ));

                // Source snippet
                if self.show_context {
                    let line_idx = violation.line.saturating_sub(1);
                    if let Some(src) = file_result.source_lines.get(line_idx) {
                        let src_trimmed = src.trim_end();
                        output.push_str(&format!("       | {}\n", src_trimmed));
                        if let Some(col) = violation.column {
                            // Point at the column with a caret (col is 1-indexed)
                            let spaces = " ".repeat(col.saturating_sub(1));
                            output.push_str(&format!(
                                "       | {}{}\n",
                                spaces,
                                self.red("^")
                            ));
                        }
                    }
                }
            }

            output.push('\n');
        }

        // Summary line
        let files_with_errors = result.file_results.len();
        let total = result.total_files_checked;
        if result.total_errors == 0 {
            let msg = format!("Checked {} file(s), no errors found.", total);
            output.push_str(&format!("{}\n", self.gray(&msg)));
        } else {
            let summary = format!(
                "Found {} error(s) in {} file(s) ({} checked)",
                result.total_errors, files_with_errors, total
            );
            output.push_str(&format!("{}\n", self.red(&summary)));
        }

        output
    }

    fn supports_color(&self) -> bool {
        self.use_color
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Violation;
    use std::path::PathBuf;

    #[test]
    fn test_no_errors() {
        let formatter = DefaultFormatter::new(false);
        let result = LintResult::new();
        let output = formatter.format(&result);

        assert!(output.contains("no errors found"));
    }

    fn make_violation(line: usize, col: Option<usize>, rule: &str, msg: &str) -> Violation {
        Violation {
            line,
            column: col,
            rule: rule.to_string(),
            message: msg.to_string(),
            fix: None,
        }
    }

    #[test]
    fn test_single_violation() {
        let formatter = DefaultFormatter::without_context(false);
        let mut result = LintResult::new();
        result.add_file_result(
            PathBuf::from("test.md"),
            vec![make_violation(5, Some(10), "MD001", "Heading levels should increment by one")],
            vec![],
        );
        let output = formatter.format(&result);
        assert!(output.contains("test.md"));
        assert!(output.contains("5:10"));
        assert!(output.contains("MD001"));
        assert!(output.contains("Heading levels"));
        assert!(output.contains("Found 1 error(s)"));
    }

    #[test]
    fn test_multiple_violations() {
        let formatter = DefaultFormatter::without_context(false);
        let mut result = LintResult::new();
        result.add_file_result(
            PathBuf::from("file1.md"),
            vec![
                make_violation(1, Some(1), "MD001", "First error"),
                make_violation(10, None, "MD002", "Second error"),
            ],
            vec![],
        );
        result.add_file_result(
            PathBuf::from("file2.md"),
            vec![make_violation(3, Some(5), "MD003", "Third error")],
            vec![],
        );
        let output = formatter.format(&result);
        assert!(output.contains("file1.md"));
        assert!(output.contains("file2.md"));
        assert!(output.contains("Found 3 error(s) in 2 file(s)"));
    }

    #[test]
    fn test_with_color() {
        let formatter = DefaultFormatter::new(true);
        let mut result = LintResult::new();
        result.add_file_result(
            PathBuf::from("test.md"),
            vec![make_violation(5, Some(10), "MD001", "Test error")],
            vec![],
        );
        let output = formatter.format(&result);
        assert!(output.contains("\x1b["));
    }

    #[test]
    fn test_source_snippet_shown() {
        let formatter = DefaultFormatter::new(false);
        let mut result = LintResult::new();
        let source_lines = vec![
            "# Good Heading".to_string(),
            "#Bad heading".to_string(),
            "More text".to_string(),
        ];
        result.add_file_result(
            PathBuf::from("test.md"),
            vec![make_violation(2, Some(1), "MD018", "No space after hash")],
            source_lines,
        );
        let output = formatter.format(&result);
        assert!(output.contains("#Bad heading"), "snippet should appear in output");
        assert!(output.contains('^'), "caret should appear under the column");
    }
}
