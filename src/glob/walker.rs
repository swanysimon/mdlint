use crate::error::{MarkdownlintError, Result};
use crate::glob::GlobMatcher;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

const MARKDOWN_EXTENSIONS: &[&str] = &[
    "md", "markdown", "mdown", "mkdn", "mkd", "mdwn", "mdtxt", "mdtext",
];

pub struct FileWalker {
    respect_gitignore: bool,
}

impl FileWalker {
    #[must_use]
    pub fn new(respect_gitignore: bool) -> Self {
        Self { respect_gitignore }
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn find_markdown_files(&self, root: &Path) -> Result<Vec<PathBuf>> {
        self.walk_files(root, None)
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn find_files_with_matcher(
        &self,
        root: &Path,
        matcher: &GlobMatcher,
    ) -> Result<Vec<PathBuf>> {
        if !matcher.has_patterns() {
            return self.find_markdown_files(root);
        }

        self.walk_files(root, Some(matcher))
    }

    fn walk_files(&self, root: &Path, matcher: Option<&GlobMatcher>) -> Result<Vec<PathBuf>> {
        let root = root.canonicalize().map_err(MarkdownlintError::Io)?;
        let mut builder = WalkBuilder::new(&root);
        builder.git_ignore(self.respect_gitignore);
        builder.git_global(self.respect_gitignore);
        builder.git_exclude(self.respect_gitignore);
        builder.hidden(false);

        let mut files = Vec::new();
        for entry in builder.build() {
            let entry = entry.map_err(|e| {
                MarkdownlintError::Io(std::io::Error::other(format!("Walk error: {e}")))
            })?;
            if !(entry.file_type().is_some_and(|ft| ft.is_file())) {
                continue;
            }

            let path = entry.path();
            if !is_markdown_file(path) {
                continue;
            }

            if let Some(m) = matcher {
                let relative_path = path.strip_prefix(&root).unwrap_or(path);
                if m.matches(relative_path) {
                    files.push(path.to_path_buf());
                }
            } else {
                files.push(path.to_path_buf());
            }
        }
        Ok(files)
    }
}

fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| MARKDOWN_EXTENSIONS.contains(&ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_find_markdown_files() {
        let temp_dir = TempDir::new().unwrap();

        fs::File::create(temp_dir.path().join("README.md")).unwrap();
        fs::File::create(temp_dir.path().join("test.txt")).unwrap();
        fs::File::create(temp_dir.path().join("guide.markdown")).unwrap();

        let walker = FileWalker::new(false);
        let files = walker.find_markdown_files(temp_dir.path()).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|p| p.ends_with("README.md")));
        assert!(files.iter().any(|p| p.ends_with("guide.markdown")));
    }

    #[test]
    fn test_find_nested_markdown_files() {
        let temp_dir = TempDir::new().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir(&docs_dir).unwrap();

        fs::File::create(temp_dir.path().join("README.md")).unwrap();
        fs::File::create(docs_dir.join("guide.md")).unwrap();

        let walker = FileWalker::new(false);
        let files = walker.find_markdown_files(temp_dir.path()).unwrap();

        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_gitignore_respect() {
        let temp_dir = TempDir::new().unwrap();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        let ignored_dir = temp_dir.path().join("node_modules");
        fs::create_dir(&ignored_dir).unwrap();

        let mut gitignore = fs::File::create(temp_dir.path().join(".gitignore")).unwrap();
        writeln!(gitignore, "node_modules/").unwrap();
        drop(gitignore);

        fs::File::create(temp_dir.path().join("README.md")).unwrap();
        fs::File::create(ignored_dir.join("package.md")).unwrap();

        let walker = FileWalker::new(true);
        let files = walker.find_markdown_files(temp_dir.path()).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("README.md"));
    }

    #[test]
    fn test_find_files_with_matcher() {
        let temp_dir = TempDir::new().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir(&docs_dir).unwrap();

        fs::File::create(temp_dir.path().join("README.md")).unwrap();
        fs::File::create(docs_dir.join("guide.md")).unwrap();
        fs::File::create(temp_dir.path().join("CHANGELOG.md")).unwrap();

        let matcher = GlobMatcher::new(&["docs/**/*.md".to_string()]).unwrap();
        let walker = FileWalker::new(false);
        let files = walker
            .find_files_with_matcher(temp_dir.path(), &matcher)
            .unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("docs/guide.md"));
    }

    #[test]
    fn test_is_markdown_file() {
        assert!(is_markdown_file(Path::new("README.md")));
        assert!(is_markdown_file(Path::new("guide.markdown")));
        assert!(is_markdown_file(Path::new("doc.mdown")));
        assert!(is_markdown_file(Path::new("file.mkd")));
        assert!(!is_markdown_file(Path::new("README.txt")));
        assert!(!is_markdown_file(Path::new("README")));
    }
}
