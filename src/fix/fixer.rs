use crate::error::{MarkdownlintError, Result};
use crate::types::{FileResult, Fix};
use std::fs;
use std::path::Path;

pub struct Fixer {
    dry_run: bool,
}

impl Fixer {
    #[must_use]
    pub fn new() -> Self {
        Self { dry_run: false }
    }

    #[must_use]
    pub fn with_dry_run(dry_run: bool) -> Self {
        Self { dry_run }
    }

    /// Apply fixes to a file and return the fixed content
    #[allow(clippy::missing_errors_doc)]
    pub fn apply_fixes(&self, path: &Path, fixes: &[Fix]) -> Result<String> {
        let content = fs::read_to_string(path)?;
        self.apply_fixes_to_content(&content, fixes)
    }

    /// Apply fixes to content string
    #[allow(clippy::missing_errors_doc)]
    pub fn apply_fixes_to_content(&self, content: &str, fixes: &[Fix]) -> Result<String> {
        if fixes.is_empty() {
            return Ok(content.to_string());
        }

        // Detect line ending style
        let line_ending = detect_line_ending(content);
        let had_trailing_newline = content.ends_with('\n');

        // Split into lines
        let mut lines: Vec<String> = content
            .lines()
            .map(std::string::ToString::to_string)
            .collect();

        // Sort fixes in reverse order (by line, then by column) to apply from end to start
        let mut sorted_fixes = fixes.to_vec();
        sorted_fixes.sort_by(|a, b| {
            match b.line_start.cmp(&a.line_start) {
                std::cmp::Ordering::Equal => {
                    // If same line, sort by column (reverse)
                    match (&b.column_start, &a.column_start) {
                        (Some(bc), Some(ac)) => bc.cmp(ac),
                        _ => std::cmp::Ordering::Equal,
                    }
                }
                other => other,
            }
        });

        // Check for overlapping fixes
        if has_overlaps(&sorted_fixes) {
            return Err(MarkdownlintError::Fix(
                "Cannot apply fixes: overlapping fix ranges detected".to_string(),
            ));
        }

        // Apply each fix
        for fix in sorted_fixes {
            apply_single_fix(&mut lines, &fix)?;
        }

        // Rejoin with original line ending, preserving a trailing newline if
        // the input had one (str::lines() silently drops it).
        let mut result = lines.join(line_ending);
        if had_trailing_newline {
            result.push_str(line_ending);
        }
        Ok(result)
    }

    /// Apply fixes from a `FileResult` and write to disk
    #[allow(clippy::missing_errors_doc)]
    pub fn apply_file_fixes(&self, file_result: &FileResult) -> Result<()> {
        let fixes: Vec<Fix> = file_result
            .violations
            .iter()
            .filter_map(|v| v.fix.clone())
            .collect();

        if fixes.is_empty() {
            return Ok(());
        }

        let fixed_content = self.apply_fixes(&file_result.path, &fixes)?;

        if !self.dry_run {
            fs::write(&file_result.path, fixed_content)?;
        }

        Ok(())
    }
}

impl Default for Fixer {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect line ending style (\n or \r\n)
fn detect_line_ending(content: &str) -> &str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Check if any fixes overlap
fn has_overlaps(fixes: &[Fix]) -> bool {
    for i in 0..fixes.len() {
        for j in (i + 1)..fixes.len() {
            if fixes_overlap(&fixes[i], &fixes[j]) {
                return true;
            }
        }
    }
    false
}

/// Check if two fixes overlap
fn fixes_overlap(a: &Fix, b: &Fix) -> bool {
    // If fixes are on different lines and don't span, they don't overlap
    if a.line_end < b.line_start || b.line_end < a.line_start {
        return false;
    }

    // If they share any lines, check column overlap
    if a.line_start == b.line_start && a.line_end == b.line_end {
        match (
            &a.column_start,
            &a.column_end,
            &b.column_start,
            &b.column_end,
        ) {
            (Some(a_start), Some(a_end), Some(b_start), Some(b_end)) => {
                // Check column overlap
                !(a_end < b_start || b_end < a_start)
            }
            _ => true, // If columns not specified, assume overlap
        }
    } else {
        true // Multi-line fixes that touch same lines overlap
    }
}

/// Apply a single fix to the lines
fn apply_single_fix(lines: &mut Vec<String>, fix: &Fix) -> Result<()> {
    // Convert to 0-indexed
    let start_line = fix.line_start.saturating_sub(1);
    let end_line = fix.line_end.saturating_sub(1);

    if start_line >= lines.len() {
        return Err(MarkdownlintError::Fix(format!(
            "Fix start line {} out of bounds",
            fix.line_start
        )));
    }

    if end_line >= lines.len() {
        return Err(MarkdownlintError::Fix(format!(
            "Fix end line {} out of bounds",
            fix.line_end
        )));
    }

    // Handle column-based fixes (single line, specific columns)
    if start_line == end_line
        && let (Some(col_start), Some(col_end)) = (fix.column_start, fix.column_end)
    {
        let line = &lines[start_line];
        let chars: Vec<char> = line.chars().collect();

        if col_start > chars.len() || col_end > chars.len() {
            return Err(MarkdownlintError::Fix(format!(
                "Fix column range {}..{} out of bounds for line length {}",
                col_start,
                col_end,
                chars.len()
            )));
        }

        // Build new line with replacement
        let before: String = chars[..col_start.saturating_sub(1)].iter().collect();
        let after: String = chars[col_end..].iter().collect();
        lines[start_line] = format!("{}{}{}", before, fix.replacement, after);
        return Ok(());
    }

    // Handle line-based fixes (replace entire lines)
    if start_line == end_line {
        if fix.replacement.is_empty() && fix.column_start.is_none() {
            // Empty replacement with no column range = "delete this line".
            lines.remove(start_line);
        } else {
            lines[start_line].clone_from(&fix.replacement);
        }
    } else {
        // Multi-line replacement
        let replacement_lines: Vec<String> = fix
            .replacement
            .lines()
            .map(std::string::ToString::to_string)
            .collect();

        // Remove old lines and insert new ones
        lines.splice(start_line..=end_line, replacement_lines);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_line_ending_lf() {
        let content = "line1\nline2\nline3";
        assert_eq!(detect_line_ending(content), "\n");
    }

    #[test]
    fn test_detect_line_ending_crlf() {
        let content = "line1\r\nline2\r\nline3";
        assert_eq!(detect_line_ending(content), "\r\n");
    }

    #[test]
    fn test_apply_single_line_fix() {
        let content = "line 1\nline 2\nline 3";
        let fix = Fix {
            line_start: 2,
            line_end: 2,
            column_start: None,
            column_end: None,
            replacement: "REPLACED".to_string(),
            description: "Test".to_string(),
        };

        let fixer = Fixer::new();
        let result = fixer.apply_fixes_to_content(content, &[fix]).unwrap();
        assert_eq!(result, "line 1\nREPLACED\nline 3");
    }

    #[test]
    fn test_apply_column_fix() {
        let content = "hello world";
        let fix = Fix {
            line_start: 1,
            line_end: 1,
            column_start: Some(7), // "world" starts at column 7 (1-indexed)
            column_end: Some(11),  // ends at column 11
            replacement: "Rust".to_string(),
            description: "Test".to_string(),
        };

        let fixer = Fixer::new();
        let result = fixer.apply_fixes_to_content(content, &[fix]).unwrap();
        assert_eq!(result, "hello Rust");
    }

    #[test]
    fn test_multiple_fixes_reverse_order() {
        let content = "line 1\nline 2\nline 3";
        let fixes = vec![
            Fix {
                line_start: 1,
                line_end: 1,
                column_start: None,
                column_end: None,
                replacement: "FIRST".to_string(),
                description: "Test".to_string(),
            },
            Fix {
                line_start: 3,
                line_end: 3,
                column_start: None,
                column_end: None,
                replacement: "THIRD".to_string(),
                description: "Test".to_string(),
            },
        ];

        let fixer = Fixer::new();
        let result = fixer.apply_fixes_to_content(content, &fixes).unwrap();
        assert_eq!(result, "FIRST\nline 2\nTHIRD");
    }

    #[test]
    fn test_preserve_crlf() {
        let content = "line 1\r\nline 2\r\nline 3";
        let fix = Fix {
            line_start: 2,
            line_end: 2,
            column_start: None,
            column_end: None,
            replacement: "FIXED".to_string(),
            description: "Test".to_string(),
        };

        let fixer = Fixer::new();
        let result = fixer.apply_fixes_to_content(content, &[fix]).unwrap();
        assert_eq!(result, "line 1\r\nFIXED\r\nline 3");
    }
}
