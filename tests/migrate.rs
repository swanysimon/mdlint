use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn mdlint_bin() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.push("mdlint");
    p
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/migrate")
        .join(name)
}

#[test]
fn migrate_writes_mdlint_toml_from_jsonc() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join(".markdownlint-cli2.jsonc");
    fs::copy(fixture(".markdownlint-cli2.jsonc"), &input).unwrap();
    let output = dir.path().join("mdlint.toml");

    let status = Command::new(mdlint_bin())
        .args(["migrate", input.to_str().unwrap(), "--output"])
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(status.success());
    let written = fs::read_to_string(&output).unwrap();
    assert!(written.contains("[rules.MD013]"));
    assert!(written.contains("line_length = 100"));
    assert!(written.contains("exclude = [\"dist/**\", \"node_modules\"]"));

    let reloaded: mdlint::config::Config = toml::from_str(&written).unwrap();
    assert!(reloaded.rules.contains_key("MD013"));
}

#[test]
fn migrate_dry_run_does_not_write_output() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join(".markdownlint-cli2.yaml");
    fs::copy(fixture(".markdownlint-cli2.yaml"), &input).unwrap();
    let output = dir.path().join("mdlint.toml");

    let status = Command::new(mdlint_bin())
        .args(["migrate", input.to_str().unwrap(), "--dry-run", "--output"])
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(status.success());
    assert!(!output.exists());
}

#[test]
fn migrate_refuses_to_overwrite_without_force() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join(".markdownlint-cli2.jsonc");
    fs::copy(fixture(".markdownlint-cli2.jsonc"), &input).unwrap();
    let output = dir.path().join("mdlint.toml");
    fs::write(&output, "# pre-existing\n").unwrap();

    let status = Command::new(mdlint_bin())
        .args(["migrate", input.to_str().unwrap(), "--output"])
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(!status.success());
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        "# pre-existing\n",
        "existing file must not be overwritten without --force"
    );

    let status = Command::new(mdlint_bin())
        .args(["migrate", input.to_str().unwrap(), "--force", "--output"])
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(status.success());
    assert_ne!(fs::read_to_string(&output).unwrap(), "# pre-existing\n");
}

#[test]
fn migrate_reports_unparsable_js_config() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join(".markdownlint-cli2.cjs");
    fs::copy(fixture(".markdownlint-cli2.cjs"), &input).unwrap();
    let output = dir.path().join("mdlint.toml");

    let status = Command::new(mdlint_bin())
        .args(["migrate", input.to_str().unwrap(), "--output"])
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(!status.success());
    assert!(!output.exists());
}
