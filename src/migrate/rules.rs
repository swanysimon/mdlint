use crate::lint::rules::create_default_registry;

/// markdownlint rule aliases (kebab-case names) mapped to the mdlint rule code that
/// implements them. Both the alias and the bare `MDxxx` code (case-insensitive) are
/// accepted as keys in a markdownlint config.
const ALIASES: &[(&str, &str)] = &[
    ("heading-increment", "MD001"),
    ("header-increment", "MD001"),
    ("heading-style", "MD003"),
    ("header-style", "MD003"),
    ("ul-style", "MD004"),
    ("list-indent", "MD005"),
    ("ul-indent", "MD007"),
    ("no-trailing-spaces", "MD009"),
    ("no-hard-tabs", "MD010"),
    ("no-reversed-links", "MD011"),
    ("no-multiple-blanks", "MD012"),
    ("line-length", "MD013"),
    ("commands-show-output", "MD014"),
    ("no-missing-space-atx", "MD018"),
    ("no-multiple-space-atx", "MD019"),
    ("no-missing-space-closed-atx", "MD020"),
    ("no-multiple-space-closed-atx", "MD021"),
    ("blanks-around-headings", "MD022"),
    ("blanks-around-headers", "MD022"),
    ("heading-start-left", "MD023"),
    ("header-start-left", "MD023"),
    ("no-duplicate-heading", "MD024"),
    ("no-duplicate-header", "MD024"),
    ("single-title", "MD025"),
    ("single-h1", "MD025"),
    ("no-trailing-punctuation", "MD026"),
    ("no-multiple-space-blockquote", "MD027"),
    ("no-blanks-blockquote", "MD028"),
    ("ol-prefix", "MD029"),
    ("list-marker-space", "MD030"),
    ("blanks-around-fences", "MD031"),
    ("blanks-around-lists", "MD032"),
    ("no-inline-html", "MD033"),
    ("no-bare-urls", "MD034"),
    ("hr-style", "MD035"),
    ("no-emphasis-as-heading", "MD036"),
    ("no-emphasis-as-header", "MD036"),
    ("no-space-in-emphasis", "MD037"),
    ("no-space-in-code", "MD038"),
    ("no-space-in-links", "MD039"),
    ("fenced-code-language", "MD040"),
    ("first-line-heading", "MD041"),
    ("first-line-h1", "MD041"),
    ("no-empty-links", "MD042"),
    ("required-headings", "MD043"),
    ("required-headers", "MD043"),
    ("proper-names", "MD044"),
    ("no-alt-text", "MD045"),
    ("code-block-style", "MD046"),
    ("single-trailing-newline", "MD047"),
    ("code-fence-style", "MD048"),
    ("emphasis-style", "MD049"),
    ("strong-style", "MD050"),
    ("link-fragments", "MD051"),
    ("reference-links-images", "MD052"),
    ("link-image-reference-definitions", "MD053"),
    ("link-image-style", "MD054"),
    ("table-pipe-style", "MD055"),
    ("table-column-count", "MD056"),
    ("blanks-around-tables", "MD058"),
    ("descriptive-link-text", "MD059"),
    ("table-column-style", "MD060"),
];

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
