//! End-to-end coverage for configuration discovery through the CLI, including the
//! `[tool.mdlint]` table in `pyproject.toml` and the `"mdlint"` key in `package.json`.

use indoc::indoc;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn mdlint_bin() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // remove test binary name
    if path.ends_with("deps") {
        path.pop();
    }
    path.push("mdlint");
    path
}

/// Run `mdlint check --no-fix` from `dir` and return the exit code.
fn check_in(dir: &Path) -> i32 {
    Command::new(mdlint_bin())
        .args(["check", "--no-fix"])
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap()
}

/// A document whose only violation is MD013 (line length).
fn long_line_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let long_line = "word ".repeat(40);
    fs::write(
        dir.path().join("doc.md"),
        format!("# Heading\n\n{}\n", long_line.trim()),
    )
    .unwrap();
    dir
}

#[test]
fn long_line_is_a_violation_without_config() {
    assert_eq!(check_in(long_line_project().path()), 1);
}

#[test]
fn pyproject_tool_table_configures_rules() {
    let dir = long_line_project();
    fs::write(
        dir.path().join("pyproject.toml"),
        indoc! {r#"
            [project]
            name = "example"
            version = "0.1.0"

            [tool.mdlint.rules.MD013]
            enabled = false
        "#},
    )
    .unwrap();

    assert_eq!(check_in(dir.path()), 0);
}

#[test]
fn package_json_key_configures_rules() {
    let dir = long_line_project();
    fs::write(
        dir.path().join("package.json"),
        indoc! {r#"
            {
              "name": "example",
              "version": "1.0.0",
              "mdlint": {
                "rules": {
                  "MD013": { "enabled": false }
                }
              }
            }
        "#},
    )
    .unwrap();

    assert_eq!(check_in(dir.path()), 0);
}

#[test]
fn manifest_without_mdlint_section_is_ignored() {
    let dir = long_line_project();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"example\"\n",
    )
    .unwrap();
    fs::write(dir.path().join("package.json"), r#"{"name": "example"}"#).unwrap();

    assert_eq!(check_in(dir.path()), 1);
}

#[test]
fn dedicated_config_wins_over_manifest() {
    let dir = long_line_project();
    fs::write(
        dir.path().join("mdlint.toml"),
        "[rules.MD013]\nenabled = true\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[tool.mdlint.rules.MD013]\nenabled = false\n",
    )
    .unwrap();

    assert_eq!(check_in(dir.path()), 1);
}

#[test]
fn explicit_config_flag_accepts_pyproject() {
    let dir = long_line_project();
    let manifest = dir.path().join("pyproject.toml");
    fs::write(&manifest, "[tool.mdlint.rules.MD013]\nenabled = false\n").unwrap();

    let status = Command::new(mdlint_bin())
        .args(["check", "--no-fix", "--config"])
        .arg(&manifest)
        .current_dir(dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert_eq!(status.code(), Some(0));
}

#[test]
fn explicit_config_flag_errors_when_manifest_has_no_mdlint_section() {
    let dir = long_line_project();
    let manifest = dir.path().join("pyproject.toml");
    fs::write(&manifest, "[project]\nname = \"example\"\n").unwrap();

    let output = Command::new(mdlint_bin())
        .args(["check", "--no-fix", "--config"])
        .arg(&manifest)
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No mdlint configuration found"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn parent_config_scalar_survives_a_child_config_that_omits_it() {
    // Regression: a nearer config that says nothing about `fix` must not reset it to the
    // built-in default and start rewriting files.
    let root = TempDir::new().unwrap();
    let child = root.path().join("child");
    fs::create_dir(&child).unwrap();
    fs::write(root.path().join("mdlint.toml"), "fix = false\n").unwrap();
    fs::write(
        child.join("mdlint.toml"),
        "[rules.MD013]\nline_length = 90\n",
    )
    .unwrap();

    let doc = child.join("doc.md");
    let unfixed = "#Heading\n";
    fs::write(&doc, unfixed).unwrap();

    Command::new(mdlint_bin())
        .arg("check")
        .current_dir(&child)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert_eq!(
        fs::read_to_string(&doc).unwrap(),
        unfixed,
        "fix = false in the parent config was reset by the child config"
    );
}

#[test]
fn config_gitignore_false_disables_gitignore_discovery() {
    // Regression: `gitignore` was only ever read from the CLI flag, so setting it in a
    // config file did nothing.
    let dir = TempDir::new().unwrap();
    Command::new("git")
        .arg("init")
        .current_dir(dir.path())
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_PREFIX")
        .output()
        .unwrap();
    fs::write(dir.path().join(".gitignore"), "ignored.md\n").unwrap();
    fs::write(dir.path().join("ignored.md"), "#Heading\n").unwrap();

    fs::write(dir.path().join("mdlint.toml"), "gitignore = true\n").unwrap();
    assert_eq!(check_in(dir.path()), 0, "ignored file should be skipped");

    fs::write(dir.path().join("mdlint.toml"), "gitignore = false\n").unwrap();
    assert_eq!(check_in(dir.path()), 1, "ignored file should be checked");
}
