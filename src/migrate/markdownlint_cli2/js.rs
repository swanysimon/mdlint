use super::cli2::Cli2Source;
use crate::error::{MarkdownlintError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

const NODE_EVAL_SCRIPT: &str = r"import(process.argv[1]).then(m => {
  const cfg = m.default ?? m;
  process.stdout.write(JSON.stringify(cfg));
}).catch(e => { console.error(e.message); process.exit(1); });";

fn node_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    })
}

fn document_to_source(document: HashMap<String, Value>) -> Cli2Source {
    Cli2Source {
        config: document
            .get("config")
            .and_then(|v| serde_json::from_value::<HashMap<String, Value>>(v.clone()).ok())
            .or_else(|| Some(document.clone())),
        ignores: document
            .get("ignores")
            .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok()),
        fix: document.get("fix").and_then(Value::as_bool),
        gitignore: document.get("gitignore").and_then(Value::as_bool),
        no_inline_config: document.get("noInlineConfig").and_then(Value::as_bool),
        front_matter: document
            .get("frontMatterPattern")
            .and_then(|v| v.as_str().map(str::to_string)),
    }
}

/// Evaluate a `.cjs`/`.mjs` markdownlint-cli2 config with a real Node.js runtime, the
/// only way to correctly resolve `require()`, spread syntax, or computed values. Returns
/// `None` if Node is not on `PATH`; returns `Some(Err)` if Node is available but the
/// config itself fails to evaluate (e.g. a missing `require`).
///
/// This executes the user's own config file — the same thing `markdownlint-cli2` itself
/// would do when loading it — so no new trust boundary is crossed by running it here.
fn try_node_eval(path: &Path) -> Option<Result<Cli2Source>> {
    if !node_available() {
        return None;
    }

    let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let output = Command::new("node")
        .arg("-e")
        .arg(NODE_EVAL_SCRIPT)
        .arg("--")
        .arg(&absolute)
        .output();

    let result = match output {
        Ok(output) if output.status.success() => {
            serde_json::from_slice::<HashMap<String, Value>>(&output.stdout)
                .map(document_to_source)
                .map_err(|e| {
                    MarkdownlintError::Migrate(format!(
                        "Node evaluated {path:?} but its output was not valid JSON: {e}"
                    ))
                })
        }
        Ok(output) => Err(MarkdownlintError::Migrate(format!(
            "Node failed to evaluate {:?}: {}",
            path,
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
        Err(e) => Err(MarkdownlintError::Migrate(format!(
            "Failed to run node for {path:?}: {e}"
        ))),
    };

    Some(result)
}

/// Best-effort extraction of a markdownlint-cli2 config from a `.cjs`/`.mjs` file via a
/// regex/text scrape, used when Node is unavailable (or as a fallback if Node eval
/// fails). It locates the object literal assigned to `module.exports` or
/// `export default`, relaxes it into valid JSON (quoting bare keys, normalizing quotes,
/// dropping trailing commas), and parses that. Configs that use variables, function
/// calls, or spread syntax cannot be recovered this way and produce an error directing
/// the user to export a JSON config instead.
fn scrape_js(content: &str, path: &Path) -> Result<Cli2Source> {
    let unparsable = || {
        MarkdownlintError::Migrate(format!(
            "Could not parse {path:?} as a static object literal. JS configs that use \
             variables, function calls, or computed values cannot be migrated \
             automatically without Node.js — install Node for full fidelity, or run \
             `console.log(JSON.stringify(config))` in your config and save the output \
             as a `.json` file, then migrate that instead."
        ))
    };

    let object_text = extract_object_literal(content).ok_or_else(unparsable)?;
    let relaxed = relax_to_json(&object_text);
    let document: HashMap<String, Value> = serde_json::from_str(&relaxed).map_err(|e| {
        MarkdownlintError::Migrate(format!(
            "Could not parse {path:?} as a static object literal ({e}). JS configs that use \
             variables, function calls, or computed values cannot be migrated \
             automatically without Node.js — install Node for full fidelity, or run \
             `console.log(JSON.stringify(config))` in your config and save the output \
             as a `.json` file, then migrate that instead."
        ))
    })?;

    Ok(document_to_source(document))
}

/// Parse a `.cjs`/`.mjs` markdownlint-cli2 config, preferring a real Node.js evaluation
/// (handles `require()`, spread, and computed values) and falling back to a best-effort
/// regex scrape when Node is unavailable or fails. Returns any non-fatal warning about
/// the strategy used alongside the parsed source.
pub fn parse_js(content: &str, path: &Path) -> Result<(Cli2Source, Vec<String>)> {
    match try_node_eval(path) {
        Some(Ok(source)) => Ok((source, Vec::new())),
        Some(Err(node_error)) => scrape_js(content, path)
            .map(|source| (source, Vec::new()))
            .map_err(|_| node_error),
        None => match scrape_js(content, path) {
            Ok(source) => Ok((
                source,
                vec![format!(
                    "Node.js was not found on PATH; parsed {:?} with a best-effort scrape \
                     instead of evaluating it — require()/spread/computed values, if \
                     present, were not resolved. Install Node for full fidelity.",
                    path
                )],
            )),
            Err(e) => Err(e),
        },
    }
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
                    return Some(content[brace_start..=(brace_start + offset)].to_string());
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
        let content = r"
            // comment
            module.exports = {
                config: {
                    default: true,
                    'MD013': { line_length: 100 },
                },
                ignores: ['dist/**'],
                fix: true,
            };
        ";
        let (source, _warnings) =
            parse_js(content, &PathBuf::from(".markdownlint-cli2.cjs")).unwrap();
        assert_eq!(source.fix, Some(true));
        assert_eq!(source.ignores, Some(vec!["dist/**".to_string()]));
        assert!(source.config.unwrap().contains_key("MD013"));
    }

    #[test]
    fn unparsable_js_returns_error() {
        let content = r"
            const base = require('./base');
            module.exports = { ...base, config: someFunction() };
        ";
        let result = parse_js(content, &PathBuf::from(".markdownlint-cli2.cjs"));
        assert!(result.is_err());
    }

    #[test]
    fn falls_back_to_scrape_without_node() {
        let content = r"
            module.exports = {
                config: { default: true, MD013: { line_length: 100 } },
                ignores: ['dist/**'],
                fix: true,
            };
        ";
        let source = scrape_js(content, &PathBuf::from(".markdownlint-cli2.cjs")).unwrap();
        assert_eq!(source.fix, Some(true));
        assert!(source.config.unwrap().contains_key("MD013"));
    }

    #[test]
    fn node_eval_resolves_require_and_spread() {
        if !node_available() {
            eprintln!("skipping: node not found on PATH");
            return;
        }

        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("base.js"),
            "module.exports = { config: { default: true, MD013: { line_length: 100 } } };",
        )
        .unwrap();
        let config_path = dir.path().join(".markdownlint-cli2.cjs");
        std::fs::write(
            &config_path,
            r"
            const base = require('./base');
            module.exports = { ...base, ignores: ['dist/**'] };
            ",
        )
        .unwrap();

        let result = try_node_eval(&config_path).expect("node is available");
        let source = result.unwrap();
        assert_eq!(source.ignores, Some(vec!["dist/**".to_string()]));
        let config = source.config.unwrap();
        assert!(config.contains_key("MD013"));
    }
}
