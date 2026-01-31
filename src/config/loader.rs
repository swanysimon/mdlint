use crate::config::Config;
use crate::error::{MarkdownlintError, Result};
use std::path::{Path, PathBuf};
use std::{fs, iter};

const CONFIG_FILE_NAMES: &[&str] = &["mdlint.toml", ".mdlint.toml"];

pub enum ConfigLoader {
    Detect,
    File(PathBuf),
    None,
}

impl ConfigLoader {
    pub fn load(&self) -> Result<Config> {
        match self {
            ConfigLoader::Detect => discover_config(),
            ConfigLoader::File(path) => load_config(path),
            ConfigLoader::None => Ok(Config::default()),
        }
    }
}

pub fn discover_config() -> Result<Config> {
    let current_dir = std::env::current_dir().ok();
    let config_file = iter::successors(current_dir, |path| path.parent().map(|p| p.to_path_buf()))
        .flat_map(|path| CONFIG_FILE_NAMES.iter().map(move |name| path.join(name)))
        .find(|path| path.exists());
    match config_file {
        Some(path) => load_config(&path),
        None => Ok(Config::default()),
    }
}

pub fn find_all_configs(start_dir: &Path) -> Result<Vec<(PathBuf, Config)>> {
    let mut configs = Vec::new();
    let mut current = start_dir.to_path_buf();

    loop {
        for config_file in CONFIG_FILE_NAMES {
            let config_path = current.join(config_file);
            if config_path.exists() {
                let config = load_config(&config_path)?;
                configs.push((config_path, config));
                break;
            }
        }

        if !current.pop() {
            break;
        }
    }

    configs.reverse();
    Ok(configs)
}
fn load_config(path: &PathBuf) -> Result<Config> {
    let content = fs::read_to_string(path).map_err(|e| {
        MarkdownlintError::Config(format!("Failed to read config file {:?}: {}", path, e))
    })?;
    parse_toml_config(&content, path)
}

fn parse_toml_config(content: &str, _path: &Path) -> Result<Config> {
    toml::from_str(content)
        .map_err(|e| MarkdownlintError::Config(format!("Failed to parse TOML: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_parse_toml() {
        let content = r#"
gitignore = true
default_enabled = true

[rules.MD013]
line_length = 100

[rules.MD003]
style = "atx"
"#;

        let config = parse_toml_config(content, Path::new("test.toml")).unwrap();
        assert!(config.gitignore);
        assert!(config.default_enabled);
        assert_eq!(config.rules.len(), 2);
    }

    #[test]
    fn test_load_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("mdlint.toml");

        let mut file = fs::File::create(&config_path).unwrap();
        write!(
            file,
            r#"
gitignore = true
default_enabled = true

[rules.MD013]
line_length = 80
"#
        )
        .unwrap();

        let config = load_config(&config_path).unwrap();
        assert!(config.gitignore);
        assert!(config.default_enabled);
    }

    #[test]
    fn test_discover_config() {
        let temp_dir = TempDir::new().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();

        let config_path = temp_dir.path().join("mdlint.toml");
        let mut file = fs::File::create(&config_path).unwrap();
        writeln!(file, "gitignore = true").unwrap();

        // Change working directory for the test
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&sub_dir).unwrap();

        let config = discover_config().unwrap();
        assert!(config.gitignore);

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }
}
