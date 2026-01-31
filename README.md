# markdownlint-rs

[![CI](https://github.com/swanysimon/markdownlint-rs/workflows/CI/badge.svg)](https://github.com/swanysimon/markdownlint-rs/actions/workflows/ci.yml?query=branch%3Amain)
[![Crates.io](https://img.shields.io/crates/v/markdownlint-rs.svg)](https://crates.io/crates/markdownlint-rs)

A fast, flexible, configuration-based command-line interface for linting Markdown files, written in Rust.

What [black](https://github.com/psf/black) and then [ruff](https://github.com/astral-sh/ruff/) did for
standardization of formatting in the Python ecosystem, markdownlint-rs hopes to accomplish for Markdown. In particular,
as AI coding agents become more and more popular, the need for well-structured Markdown files only grows. We hope that
`mdlint` becomes just as ubiquitous a command as `ruff` in your day to day.

**Project Status**: Active development, in between my day job.

## Features

- **Speed**: written in Rust for performance
- **Portable**: single, small, 0-dependency, cross-platform (Linux x86_64 or ARM64, macOS Intel or Apple Silicon,
  and Windows) binary
- **Opinionated**: formats your Markdown files in a consistent, reliable way
- **54 Built-in Rules**: uses rules from [markdownlint](https://github.com/DavidAnson/markdownlint) for a collection
  of best practices in your Markdown files
- **Git Support**: Respects `.gitignore` files by default

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
# run in the current directory
docker run --rm -v "$PWD:/workspace" ghcr.io/swanysimon/markdownlint-rs:latest --fix
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

Lint specific, or all, Markdown files in the current directory:

```shell
# lint specific files or Markdown files in a directory
mdlint README.md docs/

# lint all Markdown files
mdlint
```

Fix all auto-fixable issues:

```shell
mdlint --fix
```

## Everything below this line is potentially outdated

### Command-Line Options

```
mdlint [OPTIONS] [PATTERNS]...

Arguments:
  [PATTERNS]...  Glob patterns for files to lint (defaults to current directory)

Options:
      --config <PATH>     Path to configuration file
      --fix               Apply fixes to files
      --no-globs          Ignore globs from configuration
      --format <FORMAT>   Output format: default or json [default: default]
      --no-color          Disable color output
  -h, --help              Print help
  -V, --version           Print version
```

### Examples

**Lint with custom config file:**

```bash
mdlint --config .markdownlint.json
```

**Output as JSON:**

```bash
mdlint --format json
```

**Lint specific glob patterns:**

```bash
mdlint "**/*.md" "!node_modules/**"
```

**Fix issues automatically:**

```bash
mdlint --fix docs/
```

**Disable color output (for CI):**

```bash
mdlint --no-color
```

## Configuration

markdownlint-rs discovers configuration files automatically by searching up from the current directory:

### Configuration File Locations

The tool searches for these files in order (first found wins per directory level):

1. `.markdownlint-cli2.jsonc`
2. `.markdownlint-cli2.yaml`
3. `.markdownlint-cli2.json`
4. `.markdownlint.jsonc`
5. `.markdownlint.json`
6. `.markdownlint.yaml`
7. `package.json` (in `markdownlint-cli2` key)

### Configuration File Format

**JSONC/JSON** (`.markdownlint-cli2.jsonc`):

```jsonc
{
  // Rule configuration
  "config": {
    "default": true,              // Enable all rules by default
    "MD013": false,               // Disable line length rule
    "MD003": { "style": "atx" }   // Configure heading style
  },

  // File selection
  "globs": ["**/*.md"],
  "ignores": ["node_modules/**", "dist/**"],

  // Options
  "fix": false,
  "gitignore": true,
  "noInlineConfig": false
}
```

**YAML** (`.markdownlint-cli2.yaml`):

```yaml
config:
  default: true
  MD013: false
  MD003:
    style: atx

globs:
  - "**/*.md"
ignores:
  - "node_modules/**"
  - "dist/**"

fix: false
gitignore: true
```

### Configuration Hierarchies

Configurations are discovered by walking up the directory tree. When multiple configs are found, they are merged with
the following precedence (highest to lowest):

1. Command-line options (`--config`, `--fix`, etc.)
2. Local directory config (`.markdownlint-cli2.jsonc` in current dir)
3. Parent directory configs (walking up to root)
4. Default configuration

Arrays (like `globs` and `ignores`) are **extended**, not replaced.

### Rule Configuration

Each rule can be configured in multiple ways:

```jsonc
{
  "config": {
    "MD001": true,                    // Enable rule
    "MD002": false,                   // Disable rule
    "MD003": { "style": "atx" },      // Configure with options
    "MD007": { "indent": 4 },         // Set specific parameters
    "default": true                   // Enable all rules by default
  }
}
```

See the [markdownlint rules documentation](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md) for
details on each rule and its configuration options.

## Exit Codes

- **0**: Success - no linting errors found
- **1**: Linting errors found
- **2**: Runtime error (invalid config, file not found, etc.)

Use exit codes in CI/CD pipelines:

```bash
mdlint || exit 1  # Fail build on linting errors
```

## Supported Rules

markdownlint-rs implements 54 rules compatible with markdownlint:

| Rule  | Description                                                                                               | Fixable |
|-------|-----------------------------------------------------------------------------------------------------------|---------|
| MD001 | Heading levels should only increment by one level at a time                                               | ❌       |
| MD003 | Heading style                                                                                             | ❌       |
| MD004 | Unordered list style                                                                                      | ❌       |
| MD005 | Inconsistent indentation for list items at the same level                                                 | ❌       |
| MD007 | Unordered list indentation                                                                                | ❌       |
| MD009 | Trailing spaces                                                                                           | ✅       |
| MD010 | Hard tabs                                                                                                 | ✅       |
| MD011 | Reversed link syntax                                                                                      | ❌       |
| MD012 | Multiple consecutive blank lines                                                                          | ✅       |
| MD013 | Line length                                                                                               | ❌       |
| MD018 | No space after hash on atx style heading                                                                  | ✅       |
| MD019 | Multiple spaces after hash on atx style heading                                                           | ✅       |
| MD022 | Headings should be surrounded by blank lines                                                              | ✅       |
| MD023 | Headings must start at the beginning of the line                                                          | ✅       |
| MD025 | Multiple top-level headings in the same document                                                          | ❌       |
| ...   | See [markdownlint rules](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md) for full list | ...     |

## Differences from markdownlint-cli2

While markdownlint-rs aims for compatibility, there are some intentional differences:

- **No JavaScript custom rules**: Use Rust API instead (future feature)
- **No markdown-it plugins**: Uses CommonMark-compliant parser with standard extensions
- **Faster execution**: Compiled binary vs Node.js runtime
- **Single binary**: No npm/node dependencies required

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for:

- Development setup
- Code quality standards
- How to add new rules
- Release process

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [markdownlint](https://github.com/DavidAnson/markdownlint) by David Anson - Original rule definitions
- [markdownlint-cli2](https://github.com/DavidAnson/markdownlint-cli2) - Configuration format and behavior
- [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) - Markdown parsing

## Resources

- [Documentation](https://github.com/swanysimon/markdownlint-rs/tree/main/.github)
- [Issue Tracker](https://github.com/swanysimon/markdownlint-rs/issues)
- [Changelog](https://github.com/swanysimon/markdownlint-rs/releases)
- [markdownlint Rules Reference](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md)
