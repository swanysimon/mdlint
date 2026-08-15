use crate::lint::rule::Rule;
use crate::markdown::MarkdownParser;
use crate::types::Violation;
use serde_json::Value;

pub struct MD006;

impl Rule for MD006 {
    fn name(&self) -> &'static str {
        "MD006"
    }

    fn description(&self) -> &'static str {
        "Consider starting bulleted lists at the beginning of the line"
    }

    fn tags(&self) -> &[&str] {
        &["bullet", "ul", "indentation"]
    }

    fn check(&self, _parser: &MarkdownParser, _config: Option<&Value>) -> Vec<Violation> {
        // MD006 is deprecated and not enabled by default in markdownlint
        // Always return no violations for compatibility
        Vec::new()
    }

    fn fixable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_list_at_start() {
        let content = "* Item 1\n* Item 2";
        let parser = MarkdownParser::new(content);
        let rule = MD006;
        let violations = rule.check(&parser, None);

        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_indented_list() {
        let content = "  * Item 1\n  * Item 2";
        let parser = MarkdownParser::new(content);
        let rule = MD006;
        let violations = rule.check(&parser, None);

        // MD006 is deprecated, always returns 0 violations
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_nested_list() {
        let content = "* Item 1\n  * Nested item";
        let parser = MarkdownParser::new(content);
        let rule = MD006;
        let violations = rule.check(&parser, None);

        // MD006 is deprecated, always returns 0 violations
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_mixed() {
        let content = indoc! {"
            * Good
              * Nested (violation)
            + Also good"};
        let parser = MarkdownParser::new(content);
        let rule = MD006;
        let violations = rule.check(&parser, None);

        // MD006 is deprecated, always returns 0 violations
        assert_eq!(violations.len(), 0);
    }
}
