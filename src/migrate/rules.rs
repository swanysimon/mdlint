use crate::lint::aliases::ALIASES;
use crate::lint::rules::create_default_registry;

/// Resolve a markdownlint rule name (either an `MDxxx` code or a kebab-case alias) to
/// the mdlint rule code that implements it. Returns `None` if mdlint has no
/// implementation for the rule (e.g. deprecated rules like MD006).
pub fn resolve_rule_code(name: &str) -> Option<String> {
    let registry = create_default_registry();
    let upper = name.to_ascii_uppercase();

    if registry.get(&upper).is_some() {
        return Some(upper);
    }

    let lower = name.to_ascii_lowercase();
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == lower)
        .map(|(_, code)| code.to_string())
        .filter(|code| registry.get(code).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_alias() {
        assert_eq!(resolve_rule_code("line-length"), Some("MD013".to_string()));
    }

    #[test]
    fn resolves_bare_code_case_insensitively() {
        assert_eq!(resolve_rule_code("md013"), Some("MD013".to_string()));
    }

    #[test]
    fn unknown_alias_returns_none() {
        assert_eq!(resolve_rule_code("not-a-real-rule"), None);
    }

    #[test]
    fn deprecated_rule_not_implemented_returns_none() {
        // MD006 is deprecated and intentionally not registered by mdlint.
        assert_eq!(resolve_rule_code("MD006"), None);
    }
}
