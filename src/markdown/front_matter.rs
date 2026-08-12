#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontMatterType {
    Yaml,
    Toml,
}

const YAML_FRONT_MATTER: &str = "---";
const TOML_FRONT_MATTER: &str = "+++";

#[derive(Debug, Clone)]
pub struct FrontMatter {
    pub matter_type: FrontMatterType,
    pub content: String,
    pub end_line: usize,
}

#[must_use]
pub fn detect_front_matter(content: &str) -> Option<FrontMatter> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return None;
    }

    detect_filetype_front_matter(&lines, FrontMatterType::Yaml)
        .or_else(|| detect_filetype_front_matter(&lines, FrontMatterType::Toml))
}

fn detect_filetype_front_matter(
    lines: &[&str],
    matter_type: FrontMatterType,
) -> Option<FrontMatter> {
    if lines.is_empty() {
        return None;
    }

    let front_matter = match matter_type {
        FrontMatterType::Yaml => YAML_FRONT_MATTER,
        FrontMatterType::Toml => TOML_FRONT_MATTER,
    };
    if lines.first().copied() != Some(front_matter) {
        return None;
    }

    for (i, line) in lines.iter().enumerate().skip(1) {
        if *line != front_matter {
            continue;
        }
        let content = lines.get(1..i).unwrap_or_default().join("\n");
        return Some(FrontMatter {
            matter_type,
            content,
            end_line: i + 1,
        });
    }

    None
}

/// Extracts the `title` field from front matter content, returning its value and
/// the 1-indexed line number it appears on in the original document. Front matter
/// content starts at document line 2 (line 1 is the opening `---`/`+++` delimiter).
#[must_use]
pub fn extract_title(front_matter: &FrontMatter) -> Option<(String, usize)> {
    let delimiter = match front_matter.matter_type {
        FrontMatterType::Yaml => ':',
        FrontMatterType::Toml => '=',
    };
    for (idx, line) in front_matter.content.lines().enumerate() {
        let Some((key, value)) = line.split_once(delimiter) else {
            continue;
        };
        let key = key.trim().trim_matches('"').trim_matches('\'');
        if key != "title" {
            continue;
        }
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if !value.is_empty() {
            return Some((value.to_owned(), idx + 2));
        }
    }
    None
}

#[allow(dead_code)]
pub fn strip_front_matter(content: &str) -> String {
    if let Some(front_matter) = detect_front_matter(content) {
        let lines: Vec<&str> = content.lines().collect();
        lines
            .get(front_matter.end_line..)
            .expect("end_line bounded by doc length")
            .join("\n")
    } else {
        content.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_yaml_front_matter() {
        let content = "---\ntitle: Test\nauthor: John\n---\n# Heading";
        let fm = detect_front_matter(content).unwrap();

        assert_eq!(fm.matter_type, FrontMatterType::Yaml);
        assert_eq!(fm.content, "title: Test\nauthor: John");
        assert_eq!(fm.end_line, 4);
    }

    #[test]
    fn test_detect_toml_front_matter() {
        let content = "+++\ntitle = \"Test\"\nauthor = \"John\"\n+++\n# Heading";
        let fm = detect_front_matter(content).unwrap();

        assert_eq!(fm.matter_type, FrontMatterType::Toml);
        assert_eq!(fm.content, "title = \"Test\"\nauthor = \"John\"");
        assert_eq!(fm.end_line, 4);
    }

    #[test]
    fn test_no_front_matter() {
        let content = "# Heading\nSome content";
        assert!(detect_front_matter(content).is_none());
    }

    #[test]
    fn test_incomplete_front_matter() {
        let content = "---\ntitle: Test\n# Heading";
        assert!(detect_front_matter(content).is_none());
    }

    #[test]
    fn test_strip_front_matter() {
        let content = "---\ntitle: Test\n---\n# Heading\nContent";
        let stripped = strip_front_matter(content);

        assert_eq!(stripped, "# Heading\nContent");
    }

    #[test]
    fn test_strip_no_front_matter() {
        let content = "# Heading\nContent";
        let stripped = strip_front_matter(content);

        assert_eq!(stripped, content);
    }

    #[test]
    fn test_extract_title_yaml() {
        let content = "---\ntags:\n  - a\ntitle: My Title\n---\n# Heading";
        let fm = detect_front_matter(content).unwrap();
        let (title, line) = extract_title(&fm).unwrap();

        assert_eq!(title, "My Title");
        assert_eq!(line, 4);
    }

    #[test]
    fn test_extract_title_toml() {
        let content = "+++\nauthor = \"John\"\ntitle = \"My Title\"\n+++\n# Heading";
        let fm = detect_front_matter(content).unwrap();
        let (title, line) = extract_title(&fm).unwrap();

        assert_eq!(title, "My Title");
        assert_eq!(line, 3);
    }

    #[test]
    fn test_extract_title_missing() {
        let content = "---\nauthor: John\n---\n# Heading";
        let fm = detect_front_matter(content).unwrap();

        assert!(extract_title(&fm).is_none());
    }

    #[test]
    fn test_extract_title_colon_in_value() {
        let content = "---\ntitle: Something: Else\n---\n# Heading";
        let fm = detect_front_matter(content).unwrap();
        let (title, line) = extract_title(&fm).unwrap();

        assert_eq!(title, "Something: Else");
        assert_eq!(line, 2);
    }
}
