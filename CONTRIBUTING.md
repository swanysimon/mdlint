# Contributing to mdlint

Thank you for your interest in contributing to mdlint!

## Development Setup

The only prerequisites are [mise](https://mise.jdx.dev/) and [Rust](https://rustup.rs/). Optionally,
Docker is needed for Dockerfile linting. mise manages every other tool automatically.

To work with the Python package in `python/`, [uv](https://docs.astral.sh/uv/) is also required.
It is not needed for Rust development or running the general quality checks.

1. **Install mise**: Follow the [mise installation guide](https://mise.jdx.dev/getting-started.html)

2. **Install Rust**: Use [rustup](https://rustup.rs/)

3. **Clone the repository and install tools**:

```bash
git clone https://github.com/swanysimon/mdlint.git
cd mdlint
mise install   # installs prek, tombi, hadolint
```

1. **Build the project**:

```bash
cargo build
```

## Code Quality Standards

All quality checks are managed by `prek` and defined in `prek.toml`.
To run all checks locally (the same checks CI runs):

```bash
prek run -a
```

This runs in order: TOML formatting (`tombi`), Rust formatting (`cargo fmt`), Clippy with auto-fix,
tests (`cargo test`), mdlint dogfooding, and Dockerfile linting (`hadolint`).

Before submitting a pull request, `prek run -a` must pass cleanly.

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

### Version Sync

All package manifests must have matching versions at release time:

- `Cargo.toml` — bumped by `cargo-release`
- `npm/package.json` — synced via `release.toml`
- `python/pyproject.toml` — synced via `release.toml`

`release.toml` contains `[[pre-release-replacements]]` entries that update all of these automatically
when `cargo release` runs. **No version-setting commands are needed in CI.**

The `tag.yml` workflow verifies that all three manifests match the git tag before anything publishes.

### Creating a Release

#### Option 1: Using cargo-release (Recommended)

cargo-release automates version bumping across all manifests:

```bash
# Dry run to see what will happen
cargo release patch --dry-run    # 0.1.0 -> 0.1.1
cargo release minor --dry-run    # 0.1.0 -> 0.2.0
cargo release major --dry-run    # 0.1.0 -> 1.0.0

# Execute the release
cargo release patch --execute
```

This will:

1. Verify working directory is clean
2. Run tests
3. Bump version in `Cargo.toml` and apply all `release.toml` replacements (npm + Python manifests)
4. Create a commit: "Release X.Y.Z"
5. Create a git tag: `vX.Y.Z`
6. Push commit and tag to GitHub

Once the tag is pushed, GitHub Actions automatically:

1. Verifies tag matches all manifest versions (`Cargo.toml`, `npm/package.json`, `python/pyproject.toml`)
2. Runs all CI checks (tests, clippy, fmt, build)
3. Creates a draft GitHub release with release notes
4. Builds binaries for all 7 platforms (Linux x86_64/aarch64 glibc+musl, macOS x86_64/aarch64, Windows x86_64)
5. Generates SHA256 checksums for all binaries
6. Uploads binaries to GitHub release
7. Publishes to crates.io via trusted publishing (no token required)
8. Publishes Python wheels to PyPI via trusted publishing (no token required)
9. Publishes single npm package (all binaries bundled) to npm via trusted publishing (no token required)
10. Publishes the draft release

#### Option 2: Manual Release

If you prefer manual control, you must update all manifests yourself:

```bash
# 1. Update versions in all manifests
vim Cargo.toml             # Change version = "0.1.0" to "0.1.1"
vim npm/package.json       # Update "version"
vim python/pyproject.toml  # Update version

# 2. Commit and tag
git add Cargo.toml npm/package.json python/pyproject.toml
git commit -m "Release 0.1.1"
git tag v0.1.1
git push origin main
git push origin v0.1.1
```

Use `cargo release` instead — it handles all of this automatically via `release.toml`.

### Version Verification

The release workflow (`tag.yml`) automatically verifies all manifest versions before proceeding:

- `Cargo.toml`, `npm/package.json`, and `python/pyproject.toml` must all match the git tag
- If any version doesn't match, the release fails before creating a draft release
- Example: Tag `v0.2.0` requires `version = "0.2.0"` in all three manifests

### Adding a New Platform

When adding a new binary platform, update both of these in sync:

1. **`.github/workflows/publish-npm.yml`** — add the binary to the download and `mv` steps
2. **`.github/workflows/publish-python.yml`** — add a matrix entry with `asset`, `binary`, and `platform_tag`

Also add the new target to `build-binaries.yml` so the binary is built and uploaded to the release.

### Release Checklist

Before creating a release:

- [ ] All checks pass locally: `prek run -a`
- [ ] Version follows [SemVer](https://semver.org/) conventions:
  - **MAJOR**: Incompatible API changes
  - **MINOR**: New backwards-compatible functionality
  - **PATCH**: Backwards-compatible bug fixes

### Troubleshooting Releases

#### Tag version doesn't match manifests

- `cargo-release` updates all manifests automatically via `release.toml` — use it
- If releasing manually, ensure `Cargo.toml`, `npm/package.json`, `python/pyproject.toml`,
  and all `npm/packages/*/package.json` files all have the same version as the tag

#### CI checks failed

- The release is blocked if any quality checks fail
- Fix the issues and create a new tag

#### Release already exists

- The workflow checks for an existing release before creating one and fails if found
- Delete the existing release and tag, then re-push

## Adding New Rules

mdlint has two overlapping concepts: **formatter rules** (enforced by `mdlint format`) and **linting
rules** (reported by `mdlint check`). Many rules should be both — the formatter fixes the issue and
the linter reports it if the formatter has not been run.

### Adding a linting rule

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

1. **Register the rule**: Add it to `create_default_registry()` in `src/lint/rules/mod.rs`

2. **Write tests**: Add comprehensive tests in the same file

3. **Mark fixable**: If `mdlint format` can fix this violation, return `true` from `fixable()`

### Adding a formatting behavior

The canonical Markdown rewriter lives in `src/formatter/mod.rs`. It walks pulldown-cmark events and
re-emits canonical text. Output formatters (default, JSON, SARIF, JUnit) live in `src/format/`.

When adding a new canonical style choice, document the decision in `FORMAT_SPEC.md` first — the spec
is the source of truth. Key constraints: the formatter must be idempotent (`format(format(x)) == format(x)`)
and must not change the semantic content of the document.

### Task list

See [AIDEV.md](AIDEV.md) for the full development task list, including open tasks for rules and
formatter behaviors, phrased as AI-addressable prompts.

## Questions?

Feel free to open an issue for any questions or clarifications.
