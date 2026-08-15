use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use regex::Regex;
use serde_json::Value;

pub struct MD044;

impl Rule for MD044 {
    fn name(&self) -> &'static str {
        "MD044"
    }

    fn description(&self) -> &'static str {
        "Proper names should have the correct capitalization"
    }

    fn tags(&self) -> &[&str] {
        &["spelling"]
    }

    fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
        let names = config
            .and_then(|c| c.get("names"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            });

        let code_blocks = config
            .and_then(|c| c.get("code_blocks"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        // If no names are specified, skip check
        let proper_names = match names {
            Some(n) if !n.is_empty() => n,
            _ => return Vec::new(),
        };

        let mut violations = Vec::new();

        for (line_num, line) in parser.lines().iter().enumerate() {
            let line_number = line_num + 1;

            // Skip code blocks if configured
            if !code_blocks
                && (line.starts_with("    ") || line.starts_with('\t') || line.contains("```"))
            {
                continue;
            }

            // Check each proper name
            for name in &proper_names {
                // Create case-insensitive regex with word boundaries
                let pattern = format!(r"(?i)\b{}\b", regex::escape(name));
                if let Ok(re) = Regex::new(&pattern) {
                    for mat in re.find_iter(line) {
                        let found = mat.as_str();
                        // Check if capitalization matches
                        if found != name {
                            violations.push(Violation {
                                line: line_number,
                                column: Some(mat.start() + 1),
                                rule: self.name().to_owned(),
                                message: format!(
                                    "Proper name '{found}' should be capitalized as '{name}'"
                                ),
                                fix: None,
                            });
                        }
                    }
                }
            }
        }

        violations
    }

    fn fixable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::rendered;

    #[test]
    fn test_no_config() {
        let content = "This mentions javascript and rust";
        let parser = MarkdownParser::new(content);
        let rule = MD044;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0); // No names configured
    }

    #[test]
    fn test_correct_capitalization() {
        let content = "We use JavaScript and TypeScript.";
        let parser = MarkdownParser::new(content);
        let rule = MD044;
        let config = serde_json::json!({
            "names": ["JavaScript", "TypeScript"]
        });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_incorrect_capitalization() {
        let content = "We use javascript and typescript.";
        let parser = MarkdownParser::new(content);
        let rule = MD044;
        let config = serde_json::json!({
            "names": ["JavaScript", "TypeScript"]
        });
        let violations = rule.check(&parser, Some(&config));

        assert_eq!(
            rendered(&violations),
            [
                "test.md:1:8: MD044 Proper name 'javascript' should be capitalized as 'JavaScript'",
                "test.md:1:23: MD044 Proper name 'typescript' should be capitalized as 'TypeScript'",
            ]
        );
    }

    #[test]
    fn test_partial_match() {
        let content = "JavaScriptCore is different from JavaScript";
        let parser = MarkdownParser::new(content);
        let rule = MD044;
        let config = serde_json::json!({
            "names": ["JavaScript"]
        });
        let violations = rule.check(&parser, Some(&config));

        // Should only match whole word "JavaScript", not "JavaScriptCore"
        assert_eq!(violations.len(), 0);
    }
}
