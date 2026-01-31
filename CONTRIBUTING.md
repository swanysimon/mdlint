# Contributing to markdownlint-rs

Thank you for your interest in contributing to markdownlint-rs!

## Development Setup

1. **Install Rust**: Use [rustup](https://rustup.rs/) to install Rust
2. **Clone the repository**:

   ```bash
   git clone https://github.com/swanysimon/markdownlint-rs.git
   cd markdownlint-rs
   ```

3. **Build the project**:

   ```bash
   cargo build
   ```

4. **Run tests**:

   ```bash
   cargo test
   ```

## Code Quality Standards

Before submitting a pull request, ensure your code passes all quality checks:

```bash
# Format code
cargo fmt

# Run linter
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test --all-features

```

## Pull Request Process

1. **Create a feature branch** from `main`
2. **Make your changes** with clear, focused commits
3. **Add tests** for new functionality
4. **Update documentation** if needed
5. **Ensure CI passes** - all tests, clippy, and formatting checks must pass
6. **Submit PR** with a clear description of changes

## Release Process

Releases are managed using [`cargo-release`](https://github.com/crate-ci/cargo-release) and automated via GitHub
Actions.

### Prerequisites

Install cargo-release:

```bash
cargo install cargo-release
```

### Creating a Release

#### Option 1: Using cargo-release (Recommended)

cargo-release automates the entire process:

```bash
# Dry run to see what will happen
cargo release patch --dry-run    # 0.1.0 -> 0.1.1
cargo release minor --dry-run    # 0.1.0 -> 0.2.0
cargo release major --dry-run    # 0.1.0 -> 1.0.0

# Execute the release
cargo release patch --execute
```

This will:

1. ✅ Verify working directory is clean
2. ✅ Run tests
3. ✅ Bump version in `Cargo.toml`
4. ✅ Create a commit: "Release X.Y.Z"
5. ✅ Create a git tag: `vX.Y.Z`
6. ✅ Push commit and tag to GitHub

Once the tag is pushed, GitHub Actions automatically:

1. ✅ Verifies tag matches Cargo.toml version
2. ✅ Runs all CI checks (tests, clippy, fmt, build)
3. ✅ Creates GitHub release with release notes
4. ✅ Builds binaries for all platforms (Linux x86/ARM, macOS x86/ARM, Windows)
5. ✅ Generates SHA256 checksums for all binaries
6. ✅ Uploads binaries to GitHub release
7. ✅ Publishes to crates.io (requires `CARGO_REGISTRY_TOKEN` secret)

#### Option 2: Manual Release

If you prefer manual control:

```bash
# 1. Update version in Cargo.toml
vim Cargo.toml  # Change version = "0.1.0" to "0.1.1"

# 2. Commit the version change
git add Cargo.toml
git commit -m "Release 0.1.1"

# 3. Create and push the tag
git tag v0.1.1
git push origin main
git push origin v0.1.1
```

The GitHub Actions workflow will take over from here.

### Version Verification

The release workflow includes automatic version verification:

* If the git tag doesn't match the version in `Cargo.toml`, the release will fail
* This prevents accidental mismatches between tags and package versions
* Example: Tag `v0.2.0` requires `version = "0.2.0"` in Cargo.toml

### Release Checklist

Before creating a release:

* [ ] All tests pass locally: `cargo test --all-features`
* [ ] Code is formatted: `cargo fmt`
* [ ] No clippy warnings: `cargo clippy --all-targets --all-features`
* [ ] CHANGELOG.md is updated (if you maintain one)
* [ ] Version follows [SemVer](https://semver.org/) conventions:
  * **MAJOR**: Incompatible API changes
  * **MINOR**: New backwards-compatible functionality
  * **PATCH**: Backwards-compatible bug fixes

### Troubleshooting Releases

#### Tag version doesn't match Cargo.toml

* Make sure you updated the version in `Cargo.toml` before creating the tag
* Use `cargo-release` to avoid this issue

#### CI checks failed

* The release is blocked if any quality checks fail
* Fix the issues and create a new tag

#### Release already exists

* The workflow is idempotent - it will reuse existing releases
* This is normal if you push multiple tags for the same commit

## Adding New Linting Rules

To add a new linting rule:

1. **Create the rule file**: `src/lint/rules/mdXXX.rs`
2. **Implement the Rule trait**:

   ```rust
   pub struct MDXXX;

   impl Rule for MDXXX {
       fn name(&self) -> &str { "MD###" }
       fn description(&self) -> &str { "Your description" }
       fn tags(&self) -> Vec<&str> { vec!["tag1", "tag2"] }
       fn check(&self, parser: &MarkdownParser, config: Option<&Value>) -> Vec<Violation> {
           // Implementation
       }
   }
   ```

3. **Register the rule**: Add it to `create_default_registry()` in `src/lint/rule.rs`
4. **Write tests**: Add comprehensive tests in the same file
5. **Update documentation**: Document the rule's behavior

## Questions?

Feel free to open an issue for any questions or clarifications.
