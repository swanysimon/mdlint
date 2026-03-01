# markdownlint-rs

[![CI](https://github.com/swanysimon/markdownlint-rs/workflows/CI/badge.svg)](https://github.com/swanysimon/markdownlint-rs/actions/workflows/ci.yml?query=branch%3Amain)
[![Crates.io](https://img.shields.io/crates/v/markdownlint-rs.svg)](https://crates.io/crates/markdownlint-rs)

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
- **Portable**: single, small, 0-dependency, cross-platform binary (Linux x86_64 or ARM64, macOS Intel or
  Apple Silicon, Windows)
- **Git-aware**: respects `.gitignore` files by default

## Installation

Efforts to make markdownlint-rs available via [Homebrew](https://brew.sh) and other package managers is planned. For
now, pick between downloading binaries from GitHub releases, pulling from
[crates.io](https://crates.io/crates/markdownlint-rs), or using a
[Docker container](https://github.com/swanysimon/markdownlint-rs/pkgs/container/markdownlint-rs).

### From GitHub Releases (Recommended)

Download the latest release for your platform from the
[releases page](https://github.com/swanysimon/markdownlint-rs/releases), or download the binary via the command line.
For example, to download the latest Linux x86_64 build:

```shell
curl -LO https://github.com/swanysimon/markdownlint-rs/releases/latest/download/mdlint-linux-x86_64.tar.gz
tar xzf mdlint-linux-x86_64.tar.gz

# verify checksum
sha256sum -c mdlint-*.sha256

# add to PATH
sudo mv mdlint /usr/local/bin/
```

### From crates.io

```shell
cargo install markdownlint-rs
```

### From Docker

```shell
# check files in the current directory
docker run --rm -v "$PWD:/workspace" ghcr.io/swanysimon/markdownlint-rs:latest check

# format files in the current directory
docker run --rm -v "$PWD:/workspace" ghcr.io/swanysimon/markdownlint-rs:latest format
```

The Docker image supports both `linux/amd64` and `linux/arm64` platforms.

### From source

```shell
git clone https://github.com/swanysimon/markdownlint-rs.git
cd markdownlint-rs
cargo build --release
sudo cp target/release/mdlint /usr/local/bin/
```

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
Usage: mdlint check [OPTIONS] [PATTERNS]...

Arguments:
  [PATTERNS]...          File patterns to check (auto-detected if omitted)

Options:
      --config <CONFIG>  Path to configuration file
      --fix              Apply auto-fixes where possible
      --format <FORMAT>  Output format: default or json [default: default]
      --no-color         Disable color output
  -h, --help             Print help
```

#### `mdlint format`

Format Markdown files with opinionated fixes.

```text
Usage: mdlint format [OPTIONS] [PATTERNS]...

Arguments:
  [PATTERNS]...          File patterns to format (auto-detected if omitted)

Options:
      --check            Only verify formatting, don't modify files
      --config <CONFIG>  Path to configuration file
      --no-color         Disable color output
  -h, --help             Print help
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

**Disable color output (for CI):**

```bash
mdlint check --no-color
```

## Configuration

markdownlint-rs uses TOML configuration files, similar to how ruff uses `ruff.toml`.
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

#### Global Options

- `default_enabled` (boolean): Enable all rules by default. When `true`, rules are enabled unless explicitly disabled.
  Default: `false`
- `gitignore` (boolean): Respect `.gitignore` files when discovering markdown files. Default: `true`
- `no_inline_config` (boolean): Disable inline configuration via HTML comments. Default: `false`
- `custom_rules` (array): Paths to custom rule modules (future feature). Default: `[]`
- `front_matter` (string): Pattern for front matter detection. Default: auto-detects YAML (`---`) and TOML (`+++`)

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

Rule  | Description                                                      | Format fixes
------|------------------------------------------------------------------|-------------
MD001 | Heading levels should only increment by one level at a time      | ❌
MD003 | Heading style                                                    | ✅
MD004 | Unordered list style                                             | ✅
MD005 | Inconsistent indentation for list items at the same level        | ❌
MD007 | Unordered list indentation                                       | ❌
MD009 | Trailing spaces                                                  | ✅
MD010 | Hard tabs                                                        | ✅
MD011 | Reversed link syntax                                             | ❌
MD012 | Multiple consecutive blank lines                                 | ✅
MD013 | Line length                                                      | ❌
MD018 | No space after hash on atx style heading                         | ✅
MD019 | Multiple spaces after hash on atx style heading                  | ✅
MD022 | Headings should be surrounded by blank lines                     | ✅
MD023 | Headings must start at the beginning of the line                 | ✅
MD025 | Multiple top-level headings in the same document                 | ❌
...   | See [markdownlint rules](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md) | ...

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, code quality
standards, how to add new rules and formatting behaviors, and the release process.

The full development task list is in [AIDEV.md](AIDEV.md).

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [markdownlint](https://github.com/DavidAnson/markdownlint) by David Anson — original rule definitions
- [mdformat](https://github.com/hukkin/mdformat) — inspiration for the formatter-first approach
- [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) — Markdown parsing

## Resources

- [Documentation](https://github.com/swanysimon/markdownlint-rs/tree/main/.github)
- [Issue Tracker](https://github.com/swanysimon/markdownlint-rs/issues)
- [Changelog](https://github.com/swanysimon/markdownlint-rs/releases)
- [markdownlint Rules Reference](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md)
