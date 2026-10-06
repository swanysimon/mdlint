use crate::types::{Fix, Violation};
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, TextEdit, Uri};
use std::path::PathBuf;

/// Convert UTF-8 character index to UTF-16 code unit offset within a line.
#[allow(clippy::cast_possible_truncation)] // UTF-16 code units per char is 1 or 2; sum fits u32
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
#[allow(clippy::cast_possible_truncation)] // LSP positions are u32; line counts in real files fit
pub fn violation_to_diagnostic(v: &Violation, content: &str) -> Diagnostic {
    let lines: Vec<&str> = content.lines().collect();
    let lsp_line = v.line.saturating_sub(1) as u32;
    let lsp_char = match v.column {
        None => 0,
        Some(col) => {
            let char_idx = col.saturating_sub(1);
            lines
                .get(v.line.saturating_sub(1))
                .map_or(0, |line| char_idx_to_utf16(line, char_idx))
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
        source: Some("mdlint".to_owned()),
        message: v.message.clone(),
        ..Default::default()
    }
}

/// Convert a `Fix` to an LSP `TextEdit`.
///
/// Whole-line fixes (no column range) span from the start of `line_start`
/// to the start of the line after `line_end`, capturing the newline.
#[allow(clippy::cast_possible_truncation)] // LSP positions are u32; line counts in real files fit
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
        let start_char_idx = fix.column_start.map_or(0, |c| c.saturating_sub(1));
        let end_char_idx = fix.column_end.map_or(0, |c| c.saturating_sub(1));

        let start_utf16 = lines
            .get(start_line)
            .map_or(0, |l| char_idx_to_utf16(l, start_char_idx));
        let end_utf16 = lines
            .get(end_line)
            .map_or(0, |l| char_idx_to_utf16(l, end_char_idx));

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

/// Build the edits that turn `content` into `formatted`, narrowed to the lines
/// that actually differ.
///
/// Returns an empty vec when the document is already formatted.
///
/// Replacing the whole document would be simpler, but reflow touches nearly
/// every paragraph, so a whole-document replace on each format-on-save would
/// move the cursor and collapse undo granularity. Trimming the common leading
/// and trailing lines keeps the edit proportional to the real change.
#[allow(clippy::cast_possible_truncation)] // LSP positions are u32; line counts in real files fit
pub fn minimal_edits(content: &str, formatted: &str) -> Vec<TextEdit> {
    if content == formatted {
        return vec![];
    }

    // `split_inclusive` keeps each line's own newline, so a slice of elements
    // concatenates back to exactly the text spanned by the line range below.
    let old: Vec<&str> = content.split_inclusive('\n').collect();
    let new: Vec<&str> = formatted.split_inclusive('\n').collect();

    let max_trim = old.len().min(new.len());
    let prefix = old
        .iter()
        .zip(&new)
        .take_while(|(a, b)| a == b)
        .count()
        .min(max_trim);
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(max_trim - prefix);

    vec![TextEdit {
        range: Range {
            start: Position {
                line: prefix as u32,
                character: 0,
            },
            end: Position {
                line: (old.len() - suffix) as u32,
                character: 0,
            },
        },
        new_text: new
            .get(prefix..new.len() - suffix)
            .unwrap_or_default()
            .concat(),
    }]
}

/// Convert a `file://` URI to a `PathBuf`. Returns `None` for non-file schemes.
pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    url::Url::parse(uri.as_str()).ok()?.to_file_path().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Violation;
    use indoc::indoc;
    use std::str::FromStr;

    fn make_violation(line: usize, column: Option<usize>) -> Violation {
        Violation {
            line,
            column,
            rule: "MD001".to_owned(),
            message: "test".to_owned(),
            fix: None,
        }
    }

    #[test]
    fn test_coord_no_column() {
        let v = make_violation(3, None);
        let diag = violation_to_diagnostic(
            &v,
            indoc! {"
                line1
                line2
                line3
            "},
        );
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

    #[test]
    fn minimal_edits_empty_when_already_formatted() {
        assert!(minimal_edits("# Title\n", "# Title\n").is_empty());
    }

    #[test]
    fn minimal_edits_spans_only_the_changed_lines() {
        let edits = minimal_edits("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.line, 1);
        assert_eq!(edits[0].range.end.line, 2);
        assert_eq!(edits[0].new_text, "B\n");
    }

    #[test]
    fn minimal_edits_represents_a_pure_insertion_as_an_empty_range() {
        let edits = minimal_edits("a\nb\n", "a\nx\nb\n");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.line, 1);
        assert_eq!(edits[0].range.end.line, 1);
        assert_eq!(edits[0].new_text, "x\n");
    }

    /// Applying the edit must reproduce the target exactly, including when the
    /// whole document differs and when a trailing newline is added.
    #[test]
    fn minimal_edits_round_trip() {
        for (old, new) in [
            ("a\nb\nc\n", "a\nB\nc\n"),
            ("a\nb\n", "a\nx\nb\n"),
            ("one\ntwo\n", "totally\ndifferent\n"),
            ("", "added\n"),
            ("no trailing newline", "no trailing newline\n"),
            ("drop\nlines\nhere\n", "drop\n"),
        ] {
            let edits = minimal_edits(old, new);
            let lines: Vec<&str> = old.split_inclusive('\n').collect();
            let mut got = String::new();
            let mut cursor = 0usize;
            for edit in &edits {
                let start = (edit.range.start.line as usize).min(lines.len());
                let end = (edit.range.end.line as usize).min(lines.len());
                got.push_str(&lines[cursor..start].concat());
                got.push_str(&edit.new_text);
                cursor = end;
            }
            got.push_str(&lines[cursor..].concat());
            assert_eq!(got, new, "round trip failed for {old:?} -> {new:?}");
        }
    }
}
