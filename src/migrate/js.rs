use crate::error::{MarkdownlintError, Result};
use crate::migrate::cli2::Cli2Source;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Best-effort extraction of a markdownlint-cli2 config from a `.cjs`/`.mjs` file.
///
/// This does not execute JavaScript. It locates the object literal assigned to
/// `module.exports` or `export default`, relaxes it into valid JSON (quoting bare
/// keys, normalizing quotes, dropping trailing commas), and parses that. Configs that
/// use variables, function calls, or spread syntax cannot be recovered this way and
/// produce an error directing the user to export a JSON config instead.
pub fn parse_js(content: &str, path: &Path) -> Result<Cli2Source> {
    let unparsable = || {
        MarkdownlintError::Migrate(format!(
            "Could not parse {:?} as a static object literal. JS configs that use \
             variables, function calls, or computed values cannot be migrated \
             automatically — run `console.log(JSON.stringify(config))` in your config \
             and save the output as a `.json` file, then migrate that instead.",
            path
        ))
    };

    let object_text = extract_object_literal(content).ok_or_else(unparsable)?;
    let relaxed = relax_to_json(&object_text);
    let document: HashMap<String, Value> =
        serde_json::from_str(&relaxed).map_err(|_| unparsable())?;

    Ok(Cli2Source {
        config: document
            .get("config")
            .and_then(|v| serde_json::from_value::<HashMap<String, Value>>(v.clone()).ok())
            .or_else(|| Some(document.clone())),
        ignores: document
            .get("ignores")
            .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok()),
        fix: document.get("fix").and_then(Value::as_bool),
    })
}

/// Find the brace-balanced object literal following `module.exports =` or
/// `export default`.
fn extract_object_literal(content: &str) -> Option<String> {
    let marker_positions = ["module.exports", "export default"]
        .iter()
        .filter_map(|marker| content.find(marker).map(|idx| idx + marker.len()));

    let start_idx = marker_positions.min()?;
    let brace_start = content[start_idx..].find('{')? + start_idx;

    let mut depth = 0usize;
    let mut in_string: Option<char> = None;
    let mut escaped = false;

    for (offset, c) in content[brace_start..].char_indices() {
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == quote {
                in_string = None;
            }
            continue;
        }

        match c {
            '"' | '\'' | '`' => in_string = Some(c),
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(content[brace_start..brace_start + offset + 1].to_string());
                }
            }
            _ => {}
        }
    }

    None
}

/// Strip `//` and `/* */` comments from JS source, respecting single-, double-, and
/// backtick-quoted strings (unlike `cli2::strip_jsonc_comments`, which only tracks
/// double-quoted JSON strings).
fn strip_js_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string: Option<char> = None;

    while let Some(c) = chars.next() {
        if let Some(quote) = in_string {
            output.push(c);
            if c == '\\' {
                if let Some(escaped) = chars.next() {
                    output.push(escaped);
                }
            } else if c == quote {
                in_string = None;
            }
            continue;
        }

        match c {
            '"' | '\'' | '`' => {
                in_string = Some(c);
                output.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = ' ';
                for c in chars.by_ref() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
            }
            _ => output.push(c),
        }
    }

    output
}

/// Relax a JS object literal into valid JSON: strip comments, quote bare/single-quoted
/// keys with double quotes, normalize single-quoted string values, and drop trailing
/// commas before `}`/`]`.
fn relax_to_json(object_text: &str) -> String {
    let no_comments = strip_js_comments(object_text);

    let key_pattern = regex::Regex::new(
        r#"([{,]\s*)(?:'([A-Za-z_$][A-Za-z0-9_$-]*)'|"([A-Za-z_$][A-Za-z0-9_$-]*)"|([A-Za-z_$][A-Za-z0-9_$-]*))(\s*):"#,
    )
    .expect("static regex is valid");
    let quoted_keys = key_pattern.replace_all(&no_comments, "$1\"$2$3$4\"$5:");

    let single_quoted_string =
        regex::Regex::new(r"'([^'\\]*(?:\\.[^'\\]*)*)'").expect("static regex is valid");
    let double_quoted = single_quoted_string.replace_all(&quoted_keys, "\"$1\"");

    let trailing_comma = regex::Regex::new(r",\s*([}\]])").expect("static regex is valid");
    trailing_comma
        .replace_all(&double_quoted, "$1")
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_simple_cjs_export() {
        let content = r#"
            // comment
            module.exports = {
                config: {
                    default: true,
                    'MD013': { line_length: 100 },
                },
                ignores: ['dist/**'],
                fix: true,
            };
        "#;
        let source = parse_js(content, &PathBuf::from(".markdownlint-cli2.cjs")).unwrap();
        assert_eq!(source.fix, Some(true));
        assert_eq!(source.ignores, Some(vec!["dist/**".to_string()]));
        assert!(source.config.unwrap().contains_key("MD013"));
    }

    #[test]
    fn unparsable_js_returns_error() {
        let content = r#"
            const base = require('./base');
            module.exports = { ...base, config: someFunction() };
        "#;
        let result = parse_js(content, &PathBuf::from(".markdownlint-cli2.cjs"));
        assert!(result.is_err());
    }
}
