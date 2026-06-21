use crate::error::{MarkdownlintError, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Loosely-typed view of a markdownlint-cli2 config file. Only the fields mdlint can
/// translate are modeled; `globs`, `customRules`, and `outputFormatters` have no mdlint
/// equivalent and are intentionally left unmapped.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Cli2Source {
    pub config: Option<HashMap<String, Value>>,
    pub ignores: Option<Vec<String>>,
    pub fix: Option<bool>,
    pub gitignore: Option<bool>,
    pub no_inline_config: Option<bool>,
    pub front_matter: Option<String>,
}

/// Whether a config file is the markdownlint-cli2 wrapper format (with a nested
/// `config` block) or a standalone markdownlint rule config (the whole document *is*
/// the rule config), determined by filename since both shapes use overlapping keys.
fn is_cli2_wrapper(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains("cli2"))
}

/// Strip `//` line comments and `/* */` block comments from JSONC, respecting string
/// literals so that occurrences of `//` or `/*` inside strings are left untouched.
pub fn strip_jsonc_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            output.push(c);
            if c == '\\' {
                if let Some(escaped) = chars.next() {
                    output.push(escaped);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        match c {
            '"' => {
                in_string = true;
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

pub fn parse_json(content: &str, path: &Path) -> Result<Cli2Source> {
    let stripped = strip_jsonc_comments(content);
    let document: HashMap<String, Value> = serde_json::from_str(&stripped)
        .map_err(|e| MarkdownlintError::Parse(format!("Failed to parse {:?}: {}", path, e)))?;
    Ok(document_to_source(document, path))
}

pub fn parse_yaml(content: &str, path: &Path) -> Result<Cli2Source> {
    let document: HashMap<String, Value> = yaml_serde::from_str(content)
        .map_err(|e| MarkdownlintError::Parse(format!("Failed to parse {:?}: {}", path, e)))?;
    Ok(document_to_source(document, path))
}

/// Parse a `markdownlint-cli2` config nested under the `"markdownlint-cli2"` field of a
/// `package.json` file, used as a last-resort fallback when no dedicated config file is
/// found. That field always has the wrapper shape (`config`/`ignores`/`fix`/...), so it
/// is extracted and treated as a wrapper unconditionally rather than relying on the
/// filename-based `is_cli2_wrapper` heuristic.
pub fn parse_package_json(content: &str, path: &Path) -> Result<Cli2Source> {
    let mut document: HashMap<String, Value> = serde_json::from_str(content)
        .map_err(|e| MarkdownlintError::Parse(format!("Failed to parse {:?}: {}", path, e)))?;
    let field = document.remove("markdownlint-cli2").ok_or_else(|| {
        MarkdownlintError::Migrate(format!(
            "{:?} has no \"markdownlint-cli2\" field to migrate",
            path
        ))
    })?;
    let wrapper: HashMap<String, Value> = serde_json::from_value(field)
        .map_err(|e| MarkdownlintError::Parse(format!("Failed to parse {:?}: {}", path, e)))?;
    Ok(document_to_source_as_wrapper(wrapper))
}

fn document_to_source(document: HashMap<String, Value>, path: &Path) -> Cli2Source {
    if is_cli2_wrapper(path) {
        document_to_source_as_wrapper(document)
    } else {
        Cli2Source {
            config: Some(document),
            ignores: None,
            fix: None,
            gitignore: None,
            no_inline_config: None,
            front_matter: None,
        }
    }
}

fn document_to_source_as_wrapper(mut document: HashMap<String, Value>) -> Cli2Source {
    let config = document
        .remove("config")
        .and_then(|v| serde_json::from_value::<HashMap<String, Value>>(v).ok());
    let ignores = document
        .remove("ignores")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok());
    let fix = document.remove("fix").and_then(|v| v.as_bool());
    let gitignore = document.remove("gitignore").and_then(|v| v.as_bool());
    let no_inline_config = document.remove("noInlineConfig").and_then(|v| v.as_bool());
    let front_matter = document
        .remove("frontMatterPattern")
        .and_then(|v| v.as_str().map(str::to_string));

    Cli2Source {
        config,
        ignores,
        fix,
        gitignore,
        no_inline_config,
        front_matter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn strips_line_and_block_comments() {
        let input = "{\n  // a comment\n  \"a\": 1, /* inline */ \"b\": \"// not a comment\"\n}";
        let stripped = strip_jsonc_comments(input);
        let value: Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(value["a"], 1);
        assert_eq!(value["b"], "// not a comment");
    }

    #[test]
    fn parses_cli2_wrapper_json() {
        let content = r#"{
            "config": { "MD013": { "line_length": 100 }, "default": true },
            "ignores": ["dist/**"],
            "fix": true
        }"#;
        let source = parse_json(content, &PathBuf::from(".markdownlint-cli2.jsonc")).unwrap();
        assert_eq!(source.ignores, Some(vec!["dist/**".to_string()]));
        assert_eq!(source.fix, Some(true));
        assert!(source.config.unwrap().contains_key("MD013"));
    }

    #[test]
    fn parses_standalone_markdownlint_json() {
        let content = r#"{ "default": true, "MD013": { "line_length": 100 } }"#;
        let source = parse_json(content, &PathBuf::from(".markdownlint.json")).unwrap();
        assert!(source.ignores.is_none());
        let config = source.config.unwrap();
        assert!(config.contains_key("MD013"));
        assert!(config.contains_key("default"));
    }

    #[test]
    fn parses_cli2_wrapper_yaml() {
        let content = "config:\n  MD013:\n    line_length: 100\nignores:\n  - dist/**\nfix: true\n";
        let source = parse_yaml(content, &PathBuf::from(".markdownlint-cli2.yaml")).unwrap();
        assert_eq!(source.ignores, Some(vec!["dist/**".to_string()]));
        assert_eq!(source.fix, Some(true));
    }

    #[test]
    fn parses_gitignore_no_inline_config_and_front_matter() {
        let content = r#"{
            "config": { "default": true },
            "gitignore": false,
            "noInlineConfig": true,
            "frontMatterPattern": "^-{3}\\s*\\n(?:.*?\\n)?-{3}\\s*\\n"
        }"#;
        let source = parse_json(content, &PathBuf::from(".markdownlint-cli2.jsonc")).unwrap();
        assert_eq!(source.gitignore, Some(false));
        assert_eq!(source.no_inline_config, Some(true));
        assert_eq!(
            source.front_matter,
            Some("^-{3}\\s*\\n(?:.*?\\n)?-{3}\\s*\\n".to_string())
        );
    }

    #[test]
    fn parses_markdownlint_cli2_field_from_package_json() {
        let content = r#"{
            "name": "some-package",
            "markdownlint-cli2": {
                "config": { "default": true, "MD013": { "line_length": 100 } },
                "ignores": ["dist/**"],
                "fix": true
            }
        }"#;
        let source = parse_package_json(content, &PathBuf::from("package.json")).unwrap();
        assert_eq!(source.ignores, Some(vec!["dist/**".to_string()]));
        assert_eq!(source.fix, Some(true));
        assert!(source.config.unwrap().contains_key("MD013"));
    }

    #[test]
    fn package_json_without_field_errors() {
        let content = r#"{ "name": "some-package" }"#;
        let result = parse_package_json(content, &PathBuf::from("package.json"));
        assert!(result.is_err());
    }
}
