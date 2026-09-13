use crate::config::Config;
use crate::config::embedded;
use crate::error::{MarkdownlintError, Result};
use std::path::{Path, PathBuf};
use std::{fs, iter};

/// Config file names, in the order a single directory is searched. The first name that yields
/// a config wins; `pyproject.toml` and `package.json` only yield one when they carry an mdlint
/// section, so an unrelated manifest never shadows a config further up the tree.
const CONFIG_FILE_NAMES: &[&str] = &[
    "mdlint.toml",
    ".mdlint.toml",
    "pyproject.toml",
    "package.json",
];

pub enum ConfigLoader {
    Detect,
    File(PathBuf),
    None,
}

impl ConfigLoader {
    pub fn load(&self) -> Result<Config> {
        match self {
            ConfigLoader::Detect => {
                let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                discover_config(&start)
            }
            ConfigLoader::File(path) => load_explicit_config(path),
            ConfigLoader::None => Ok(Config::default()),
        }
    }
}

pub fn discover_config(start_dir: &Path) -> Result<Config> {
    Ok(find_dir_configs(start_dir)
        .find_map(Result::transpose)
        .transpose()?
        .map_or_else(Config::default, |(_, config)| config))
}

pub fn find_all_configs(start_dir: &Path) -> Result<Vec<(PathBuf, Config)>> {
    let mut configs = find_dir_configs(start_dir)
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>>>()?;
    configs.reverse();
    Ok(configs)
}

/// Walk from `start_dir` up to the filesystem root, yielding at most one config per directory.
fn find_dir_configs(start_dir: &Path) -> impl Iterator<Item = Result<Option<(PathBuf, Config)>>> {
    iter::successors(Some(start_dir.to_path_buf()), |path| {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
    })
    .map(|dir| load_dir_config(&dir))
}

fn load_dir_config(dir: &Path) -> Result<Option<(PathBuf, Config)>> {
    for name in CONFIG_FILE_NAMES {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        if let Some(config) = load_config(&path)? {
            return Ok(Some((path, config)));
        }
    }
    Ok(None)
}

/// Load a config the user named explicitly with `--config`. A manifest without an mdlint
/// section is an error here: the user pointed at it, so silently falling back to defaults
/// would hide the mistake.
fn load_explicit_config(path: &Path) -> Result<Config> {
    load_config(path)?.ok_or_else(|| {
        MarkdownlintError::Config(format!(
            "No mdlint configuration found in {}: expected a [tool.{}] table in pyproject.toml \
             or an \"{}\" key in package.json",
            path.display(),
            embedded::SECTION,
            embedded::SECTION,
        ))
    })
}

/// Returns `Ok(None)` when `path` is a manifest of another tool carrying no mdlint section.
fn load_config(path: &Path) -> Result<Option<Config>> {
    let content = fs::read_to_string(path).map_err(|e| {
        MarkdownlintError::Config(format!(
            "Failed to read config file {}: {e}",
            path.display()
        ))
    })?;
    match path.file_name().and_then(|name| name.to_str()) {
        Some("pyproject.toml") => embedded::parse_pyproject(&content, path),
        Some("package.json") => embedded::parse_package_json(&content, path),
        _ => parse_toml_config(&content, path).map(Some),
    }
}

fn parse_toml_config(content: &str, path: &Path) -> Result<Config> {
    toml::from_str(content)
        .map_err(|e| MarkdownlintError::Config(format!("Failed to parse {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use tempfile::TempDir;

    fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_parse_toml() {
        let content = indoc! {r#"
            gitignore = true
            default_enabled = true

            [rules.MD013]
            line_length = 100

            [rules.MD003]
            style = "atx"
        "#};

        let config = parse_toml_config(content, Path::new("test.toml")).unwrap();
        assert!(config.gitignore);
        assert!(config.default_enabled);
        assert_eq!(config.rules.len(), 2);
    }

    #[test]
    fn test_load_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = write(
            temp_dir.path(),
            "mdlint.toml",
            indoc! {"
                gitignore = true
                default_enabled = true

                [rules.MD013]
                line_length = 80
            "},
        );

        let config = load_config(&config_path).unwrap().unwrap();
        assert!(config.gitignore);
        assert!(config.default_enabled);
    }

    #[test]
    fn test_discover_config() {
        let temp_dir = TempDir::new().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        write(temp_dir.path(), "mdlint.toml", "gitignore = true\n");

        let config = discover_config(&sub_dir).unwrap();
        assert!(config.gitignore);
    }

    #[test]
    fn discovers_pyproject_tool_table() {
        let temp_dir = TempDir::new().unwrap();
        write(
            temp_dir.path(),
            "pyproject.toml",
            indoc! {r#"
                [project]
                name = "example"

                [tool.mdlint]
                gitignore = false
            "#},
        );

        let config = discover_config(temp_dir.path()).unwrap();
        assert!(!config.gitignore);
    }

    #[test]
    fn discovers_package_json_key() {
        let temp_dir = TempDir::new().unwrap();
        write(
            temp_dir.path(),
            "package.json",
            r#"{"name": "example", "mdlint": {"gitignore": false}}"#,
        );

        let config = discover_config(temp_dir.path()).unwrap();
        assert!(!config.gitignore);
    }

    #[test]
    fn manifest_without_mdlint_section_does_not_shadow_parent_config() {
        let temp_dir = TempDir::new().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        write(temp_dir.path(), "mdlint.toml", "no_inline_config = true\n");
        write(
            &sub_dir,
            "pyproject.toml",
            "[project]\nname = \"example\"\n",
        );
        write(&sub_dir, "package.json", r#"{"name": "example"}"#);

        let config = discover_config(&sub_dir).unwrap();
        assert!(config.no_inline_config);
        let configs = find_all_configs(&sub_dir).unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].0, temp_dir.path().join("mdlint.toml"));
    }

    #[test]
    fn dedicated_config_wins_over_manifest_in_same_directory() {
        let temp_dir = TempDir::new().unwrap();
        write(temp_dir.path(), "mdlint.toml", "no_inline_config = true\n");
        write(
            temp_dir.path(),
            "pyproject.toml",
            "[tool.mdlint]\nno_inline_config = false\n",
        );

        let configs = find_all_configs(temp_dir.path()).unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].0, temp_dir.path().join("mdlint.toml"));
    }

    #[test]
    fn pyproject_wins_over_package_json_in_same_directory() {
        let temp_dir = TempDir::new().unwrap();
        write(
            temp_dir.path(),
            "pyproject.toml",
            "[tool.mdlint]\nfront_matter = \"+++\"\n",
        );
        write(
            temp_dir.path(),
            "package.json",
            r#"{"mdlint": {"front_matter": "---"}}"#,
        );

        let config = discover_config(temp_dir.path()).unwrap();
        assert_eq!(config.front_matter.as_deref(), Some("+++"));
    }

    #[test]
    fn find_all_configs_orders_root_first() {
        let temp_dir = TempDir::new().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        write(temp_dir.path(), "pyproject.toml", "[tool.mdlint]\n");
        write(&sub_dir, "mdlint.toml", "\n");

        let configs = find_all_configs(&sub_dir).unwrap();
        let paths: Vec<_> = configs.iter().map(|(path, _)| path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                temp_dir.path().join("pyproject.toml"),
                sub_dir.join("mdlint.toml"),
            ]
        );
    }

    #[test]
    fn explicit_manifest_without_section_errors() {
        let temp_dir = TempDir::new().unwrap();
        let path = write(
            temp_dir.path(),
            "pyproject.toml",
            "[project]\nname = \"x\"\n",
        );

        let error = load_explicit_config(&path).unwrap_err();
        assert!(error.to_string().contains("No mdlint configuration found"));
    }

    #[test]
    fn explicit_manifest_with_section_loads() {
        let temp_dir = TempDir::new().unwrap();
        let path = write(
            temp_dir.path(),
            "package.json",
            r#"{"mdlint": {"default_enabled": false}}"#,
        );

        let config = load_explicit_config(&path).unwrap();
        assert!(!config.default_enabled);
    }
}
