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
    use indoc::indoc;

    #[test]
    fn test_detect_yaml_front_matter() {
        let content = indoc! {"
            ---
            title: Test
            author: John
            ---
            # Heading"};
        let fm = detect_front_matter(content).unwrap();

        assert_eq!(fm.matter_type, FrontMatterType::Yaml);
        assert_eq!(fm.content, "title: Test\nauthor: John");
        assert_eq!(fm.end_line, 4);
    }

    #[test]
    fn test_detect_toml_front_matter() {
        let content = indoc! {"
            +++
            title = \"Test\"
            author = \"John\"
            +++
            # Heading"};
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
        let content = indoc! {"
            ---
            title: Test
            # Heading"};
        assert!(detect_front_matter(content).is_none());
    }

    #[test]
    fn test_strip_front_matter() {
        let content = indoc! {"
            ---
            title: Test
            ---
            # Heading
            Content"};
        let stripped = strip_front_matter(content);

        assert_eq!(stripped, "# Heading\nContent");
    }

    #[test]
    fn test_strip_no_front_matter() {
        let content = "# Heading\nContent";
        let stripped = strip_front_matter(content);

        assert_eq!(stripped, content);
    }
}
