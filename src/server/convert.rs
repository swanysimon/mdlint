use crate::types::{Fix, Violation};
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, TextEdit, Uri};
use std::path::PathBuf;

/// Convert UTF-8 character index to UTF-16 code unit offset within a line.
fn char_idx_to_utf16(line_text: &str, char_idx: usize) -> u32 {
    line_text
        .chars()
        .take(char_idx)
        .map(|c| c.len_utf16() as u32)
        .sum()
}

/// Convert a `Violation` to an LSP `Diagnostic`.
///
/// mdlint uses 1-indexed lines and columns; LSP uses 0-indexed UTF-16 positions.
pub fn violation_to_diagnostic(v: &Violation, content: &str) -> Diagnostic {
    let lines: Vec<&str> = content.lines().collect();
    let lsp_line = v.line.saturating_sub(1) as u32;
    let lsp_char = match v.column {
        None => 0,
        Some(col) => {
            let char_idx = col.saturating_sub(1);
            lines
                .get(v.line.saturating_sub(1))
                .map(|line| char_idx_to_utf16(line, char_idx))
                .unwrap_or(0)
        }
    };
    let position = Position {
        line: lsp_line,
        character: lsp_char,
    };
    Diagnostic {
        range: Range {
            start: position,
            end: position,
        },
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some(NumberOrString::String(v.rule.clone())),
        source: Some("mdlint".to_string()),
        message: v.message.clone(),
        ..Default::default()
    }
}

/// Convert a `Fix` to an LSP `TextEdit`.
///
/// Whole-line fixes (no column range) span from the start of `line_start`
/// to the start of the line after `line_end`, capturing the newline.
pub fn fix_to_text_edit(fix: &Fix, content: &str) -> TextEdit {
    let lines: Vec<&str> = content.lines().collect();

    if fix.column_start.is_none() && fix.column_end.is_none() {
        // Whole-line operation: span from start of line_start to start of line after line_end.
        // fix.line_end (1-indexed) maps directly to the 0-indexed start of the following line.
        let start = Position {
            line: fix.line_start.saturating_sub(1) as u32,
            character: 0,
        };
        let end = Position {
            line: fix.line_end as u32,
            character: 0,
        };
        TextEdit {
            range: Range { start, end },
            new_text: fix.replacement.clone(),
        }
    } else {
        let start_line = fix.line_start.saturating_sub(1);
        let end_line = fix.line_end.saturating_sub(1);
        let start_char_idx = fix.column_start.map(|c| c.saturating_sub(1)).unwrap_or(0);
        let end_char_idx = fix.column_end.map(|c| c.saturating_sub(1)).unwrap_or(0);

        let start_utf16 = lines
            .get(start_line)
            .map(|l| char_idx_to_utf16(l, start_char_idx))
            .unwrap_or(0);
        let end_utf16 = lines
            .get(end_line)
            .map(|l| char_idx_to_utf16(l, end_char_idx))
            .unwrap_or(0);

        TextEdit {
            range: Range {
                start: Position {
                    line: start_line as u32,
                    character: start_utf16,
                },
                end: Position {
                    line: end_line as u32,
                    character: end_utf16,
                },
            },
            new_text: fix.replacement.clone(),
        }
    }
}

/// Build a `TextEdit` that replaces the entire document with `formatted`.
///
/// The end range is `(line_count, 0)` — the start of the line after the last,
/// which captures any trailing newline.
pub fn whole_doc_edit(content: &str, formatted: &str) -> TextEdit {
    let line_count = content.lines().count() as u32;
    TextEdit {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: line_count,
                character: 0,
            },
        },
        new_text: formatted.to_string(),
    }
}

/// Convert a `file://` URI to a `PathBuf`. Returns `None` for non-file schemes.
pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    url::Url::parse(uri.as_str()).ok()?.to_file_path().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Violation;
    use std::str::FromStr;

    fn make_violation(line: usize, column: Option<usize>) -> Violation {
        Violation {
            line,
            column,
            rule: "MD001".to_string(),
            message: "test".to_string(),
            fix: None,
        }
    }

    #[test]
    fn test_coord_no_column() {
        let v = make_violation(3, None);
        let diag = violation_to_diagnostic(&v, "line1\nline2\nline3\n");
        assert_eq!(diag.range.start.line, 2);
        assert_eq!(diag.range.start.character, 0);
    }

    #[test]
    fn test_coord_ascii() {
        // 1-indexed col 5 → LSP character 4
        let v = make_violation(1, Some(5));
        let diag = violation_to_diagnostic(&v, "hello world\n");
        assert_eq!(diag.range.start.line, 0);
        assert_eq!(diag.range.start.character, 4);
    }

    #[test]
    fn test_coord_utf16() {
        // Line contains a 2-code-unit emoji (U+1F600 = 😀).
        // Content: "😀bc" — char 0 is emoji (2 UTF-16 units), char 1 is 'b', char 2 is 'c'.
        // mdlint col 3 (1-indexed) = char index 2 = 'c'.
        // UTF-16 offset: 2 (emoji) + 1 (b) = 3.
        let content = "\u{1F600}bc\n";
        let v = make_violation(1, Some(3));
        let diag = violation_to_diagnostic(&v, content);
        assert_eq!(diag.range.start.character, 3);
    }

    #[test]
    fn test_uri_file_scheme() {
        let uri = Uri::from_str("file:///tmp/foo.md").unwrap();
        let path = uri_to_path(&uri).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/foo.md"));
    }

    #[test]
    fn test_uri_non_file() {
        let uri = Uri::from_str("untitled:foo.md").unwrap();
        assert!(uri_to_path(&uri).is_none());
    }
}
