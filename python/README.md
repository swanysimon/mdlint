# mdlint

[![CI](https://github.com/swanysimon/mdlint/workflows/CI/badge.svg)](https://github.com/swanysimon/mdlint/actions/workflows/ci.yml?query=branch%3Amain)

An opinionated Markdown formatter and linter, written in Rust.

What [ruff](https://github.com/astral-sh/ruff/) did for Python and [gofmt](https://pkg.go.dev/cmd/gofmt) did for Go,
`mdlint` aims to do for Markdown: enforce a single, consistent canonical style so that style debates disappear and diffs
stay meaningful. As AI coding agents increasingly read and write Markdown, well-structured files matter more than ever.
Run `mdlint format` and stop thinking about it.

**Project Status**: Active development, in between my day job.

## Features

- **Formatter first**: `mdlint format` rewrites files to a canonical style — no configuration required
- **Linter second**: `mdlint check` reports violations; most are auto-fixable by the formatter
- **Fast**: written in Rust for performance
- **Portable**: single, small, 0-dependency binary (Linux x86_64/ARM64, macOS Intel/Apple Silicon, Windows)
- **Git-aware**: respects `.gitignore` files by default

## Installation

```shell
pip install markdownlint-rs
```

Or with uv:

```shell
uv tool install markdownlint-rs
```

## How it works

`pip install markdownlint-rs` downloads a platform-specific wheel that bundles the correct pre-built `mdlint` binary
for your OS and architecture. The `mdlint` command is a thin Python wrapper that locates and execs that binary.
No Rust toolchain is required.

| Platform | Architecture | Wheel tag |
| --- | --- | --- |
| Linux (glibc) | x86_64 | `manylinux_2_17_x86_64` |
| Linux (glibc) | aarch64 | `manylinux_2_17_aarch64` |
| Linux (musl) | x86_64 | `musllinux_1_2_x86_64` |
| Linux (musl) | aarch64 | `musllinux_1_2_aarch64` |
| macOS | x86_64 | `macosx_10_12_x86_64` |
| macOS | arm64 (Apple Silicon) | `macosx_11_0_arm64` |
| Windows | x86_64 | `win_amd64` |

## Usage

### Basic Usage

**Check** Markdown files for issues:

```shell
# check all Markdown files (auto-detected)
mdlint check

# check specific files or directories
mdlint check README.md docs/

# check and apply auto-fixes
mdlint check --fix
```

**Format** Markdown files (opinionated, fixes everything):

```shell
# format all Markdown files
mdlint format

# verify formatting without modifying files
mdlint format --check

# format specific files or directories
mdlint format README.md docs/
```

### Command-Line Options

#### `mdlint check`

Lint Markdown files and report issues.

```text
Usage: mdlint check [OPTIONS] [FILES]...

Arguments:
  [FILES]...              Files or directories to check (defaults to current directory)

Options:
      --fix               Apply auto-fixes where possible
      --format <FORMAT>   Output format: default or json [default: default]
      --exclude <PATH>    Exclude files or directories
      --config <CONFIG>   Path to configuration file
  -v, --verbose           Print each file name as it is checked
      --color <COLOR>     Color output: auto, always, never [default: auto]
  -h, --help              Print help
```

#### `mdlint format`

Format Markdown files with opinionated fixes.

```text
Usage: mdlint format [OPTIONS] [FILES]...

Arguments:
  [FILES]...              Files or directories to format (defaults to current directory)

Options:
      --check             Only verify formatting, don't modify files
      --exclude <PATH>    Exclude files or directories
      --config <CONFIG>   Path to configuration file
      --color <COLOR>     Color output: auto, always, never [default: auto]
  -h, --help              Print help
```

### Examples

**Check with auto-fix:**

```bash
mdlint check --fix
```

**Check with custom config file:**

```bash
mdlint check --config mdlint.toml
```

**Check with JSON output:**

```bash
mdlint check --format json
```

**Check specific files:**

```bash
mdlint check README.md CONTRIBUTING.md docs/
```

**Format all files:**

```bash
mdlint format
```

**Verify formatting in CI:**

```bash
mdlint format --check
```

**Disable color output:**

```bash
mdlint check --color never
```

**Show each file as it is checked:**

```bash
mdlint check --verbose
```

## Configuration

mdlint uses TOML configuration files, similar to how ruff uses `ruff.toml`.
The tool automatically discovers configuration files by searching up from the current directory.

### Configuration File Locations

The tool searches for these files in order (first found wins per directory level):

1. `mdlint.toml`
2. `.mdlint.toml`

### Configuration File Format

Create a `mdlint.toml` file in your project root:

```toml
# Enable all rules by default
default_enabled = true

# Respect .gitignore files when discovering files
gitignore = true

# Disable inline configuration comments
no_inline_config = false

# Rule-specific configuration
[rules.MD013]
line_length = 120
heading_line_length = 80
code_blocks = false

[rules.MD003]
style = "atx"

[rules.MD004]
style = "asterisk"

[rules.MD007]
indent = 2

# Disable specific rules
[rules.MD034]
enabled = false
```

### Configuration Options

See [`mdlint.default.toml`](https://github.com/swanysimon/mdlint/blob/main/mdlint.default.toml) for every option
with its default value and a description of what it does. The global options are summarised below.

#### Global Options

- `default_enabled` (boolean): When `true`, all rules are enabled unless explicitly disabled. Default: `false`
- `gitignore` (boolean): Respect `.gitignore` files when discovering markdown files. Default: `true`
- `no_inline_config` (boolean): Disable inline configuration via HTML comments. Default: `false`
- `exclude` (array): Paths to exclude from file discovery; merged with any `--exclude` CLI flags. Default: `[]`
- `custom_rules` (array): Paths to custom rule modules (future feature). Default: `[]`
- `front_matter` (string): Pattern for front matter detection. Default: auto-detects YAML (`---`) and TOML (`+++`)
- `fix` (boolean): When `true`, `mdlint check` automatically applies all auto-fixable violations,
  equivalent to passing `--fix` on the command line. Default: `true`

#### Rule Configuration

Rules can be configured in three ways:

1. **Enable/Disable a rule:**

   ```toml
   [rules.MD013]
   enabled = false
   ```

2. **Configure rule parameters:**

   ```toml
   [rules.MD013]
   line_length = 100
   code_blocks = false
   ```

3. **Use both (parameters implicitly enable the rule):**

   ```toml
   [rules.MD003]
   style = "atx"
   ```

### Configuration Hierarchy

Configurations are discovered by walking up the directory tree. When multiple configs are found, they are merged with
the following precedence (highest to lowest):

1. Command-line options (`--config`)
2. Local directory config (`mdlint.toml` in current dir)
3. Parent directory configs (walking up to root)
4. Default configuration

Later configs override earlier ones for scalar values. When a rule is configured in multiple places, the most specific
configuration wins.

See the [markdownlint rules documentation](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md) for
details on each rule and its configuration options.

### Inline Configuration

Rules can be suppressed for specific lines using HTML comments, without modifying `mdlint.toml`:

```markdown
<!-- mdlint-disable-next-line MD013 -->
This line may be longer than the configured limit.

<!-- mdlint-disable MD033 -->
<div>Raw HTML block that needs to stay as-is</div>
<!-- mdlint-enable MD033 -->
```

| Comment | Effect |
| --- | --- |
| `<!-- mdlint-disable MD001 -->` | Disable rule from this line onward |
| `<!-- mdlint-enable MD001 -->` | Re-enable rule from this line onward |
| `<!-- mdlint-disable-next-line MD001 -->` | Disable rule for the next line only |
| `<!-- mdlint-disable -->` | Disable all rules from this line onward |
| `<!-- mdlint-enable -->` | Re-enable all rules |

Multiple rules: `<!-- mdlint-disable MD001 MD013 -->` — space-separate rule codes.
Set `no_inline_config = true` in `mdlint.toml` to ignore all inline comments.

## Exit Codes

- **0**: Success - no linting errors found (or files successfully formatted with `format`)
- **1**: Linting errors found (or formatting issues found with `format --check`)
- **2**: Runtime error (invalid config, file not found, etc.)

Use exit codes in CI/CD pipelines:

```bash
# Fail build on linting errors
mdlint check || exit 1

# Fail build if files need formatting
mdlint format --check || exit 1
```

## Supported Rules

mdlint implements the [markdownlint](https://github.com/DavidAnson/markdownlint) rule set. Rules marked
✅ are enforced automatically by `mdlint format`; rules marked ❌ are reported by `mdlint check` only.

| Rule | Description | Format fixes |
| --- | --- | --- |
| MD001 | Heading levels should only increment by one level at a time | ❌ |
| MD003 | Heading style | ✅ |
| MD004 | Unordered list style | ✅ |
| MD005 | Inconsistent indentation for list items at the same level | ❌ |
| MD007 | Unordered list indentation | ❌ |
| MD009 | Trailing spaces | ✅ |
| MD010 | Hard tabs | ✅ |
| MD011 | Reversed link syntax | ❌ |
| MD012 | Multiple consecutive blank lines | ✅ |
| MD013 | Line length | ❌ |
| MD018 | No space after hash on atx style heading | ✅ |
| MD019 | Multiple spaces after hash on atx style heading | ✅ |
| MD022 | Headings should be surrounded by blank lines | ✅ |
| MD023 | Headings must start at the beginning of the line | ✅ |
| MD025 | Multiple top-level headings in the same document | ❌ |
| ... | See [markdownlint rules](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md) | ... |

## Pre-commit Hooks

### Native git hook

Create `.git/hooks/pre-commit` (and make it executable with `chmod +x`):

```bash
#!/bin/sh
mdlint format --check
```

This causes `git commit` to fail if any staged Markdown file needs formatting.

### pre-commit framework

Add to `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/swanysimon/mdlint
    rev: v0.3.3  # use the latest release tag
    hooks:
      - id: mdlint-format-check
        name: mdlint format --check
        language: system
        entry: mdlint format --check
        types: [markdown]
      - id: mdlint-check
        name: mdlint check
        language: system
        entry: mdlint check
        types: [markdown]
```

Or use `mdlint check --fix` to auto-fix and stage the result:

```yaml
      - id: mdlint-fix
        name: mdlint check --fix
        language: system
        entry: mdlint check --fix
        types: [markdown]
        pass_filenames: false
```

### GitHub Actions

```yaml
- name: Check Markdown formatting
  run: mdlint format --check

- name: Lint Markdown
  run: mdlint check
```

## Contributing

Contributions are welcome! See the [main repository](https://github.com/swanysimon/mdlint) for development setup,
code quality standards, and the pull request process.

### Build a wheel locally

```shell
cd python

# Pure-Python wheel (no binary bundled — for metadata validation only)
uv build --wheel

# Platform-specific wheel with a binary
cp /path/to/mdlint-binary mdlint/mdlint
MDLINT_PLATFORM_TAG=macosx_11_0_arm64 uv build --wheel
```

`MDLINT_PLATFORM_TAG` is read by `hatch_build.py` to stamp the correct platform tag onto the wheel.
Without it, the wheel is tagged `py3-none-any` and contains no binary — useful for metadata validation in CI
but not for distribution.

### Validate package metadata

```shell
cd python
uv build --wheel
uvx twine check dist/*.whl
```

### Platform tags

| Asset | `MDLINT_PLATFORM_TAG` |
| --- | --- |
| `mdlint-linux-x86_64` | `manylinux_2_17_x86_64.manylinux2014_x86_64` |
| `mdlint-linux-x86_64-musl` | `musllinux_1_2_x86_64` |
| `mdlint-linux-aarch64` | `manylinux_2_17_aarch64.manylinux2014_aarch64` |
| `mdlint-linux-aarch64-musl` | `musllinux_1_2_aarch64` |
| `mdlint-macos-x86_64` | `macosx_10_12_x86_64` |
| `mdlint-macos-aarch64` | `macosx_11_0_arm64` |
| `mdlint-windows-x86_64.exe` | `win_amd64` |

### Release

Releases are automated via `.github/workflows/publish-python.yml`. On a version tag push, the workflow downloads
each pre-built binary from the GitHub release, builds a platform-specific wheel, and publishes it to PyPI via
trusted publishing (OIDC, no token required).

## License

The Unlicense - see [LICENSE](https://github.com/swanysimon/mdlint/blob/main/LICENSE) for details.

## Acknowledgments

- [markdownlint](https://github.com/DavidAnson/markdownlint) by David Anson — original rule definitions
- [mdformat](https://github.com/hukkin/mdformat) — inspiration for the formatter-first approach
- [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) — Markdown parsing

## Resources

- [Documentation](https://github.com/swanysimon/mdlint/tree/main/.github)
- [Issue Tracker](https://github.com/swanysimon/mdlint/issues)
- [Changelog](https://github.com/swanysimon/mdlint/releases)
- [markdownlint Rules Reference](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md)
