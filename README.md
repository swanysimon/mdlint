# mdlint

[![CI](https://github.com/swanysimon/mdlint/workflows/CI/badge.svg)](https://github.com/swanysimon/mdlint/actions/workflows/ci.yml?query=branch%3Amain)

[![Crates.io](https://img.shields.io/crates/v/markdownlint-rs.svg)](https://crates.io/crates/markdownlint-rs)
[![NPM](https://img.shields.io/npm/v/markdownlint-rs.svg)](https://www.npmjs.com/package/markdownlint-rs)
[![PyPi](https://img.shields.io/pypi/v/markdownlint-rs.svg)](https://pypi.org/project/markdownlint-rs)

An opinionated Markdown formatter and linter, written in Rust.

What [ruff](https://github.com/astral-sh/ruff/) did for Python and [gofmt](https://pkg.go.dev/cmd/gofmt) did for Go,
`mdlint` aims to do for Markdown: enforce a single, consistent canonical style so that style debates disappear and diffs
stay meaningful. As AI coding agents increasingly read and write Markdown, well-structured files matter more than ever.
Run `mdlint format` and stop thinking about it.

**Project Status**: Active development, but no one's top priority.

## Features

- **Formatter first**: `mdlint format` rewrites files to a canonical style — no configuration required
- **Linter second**: `mdlint check` reports violations; fixable rules are auto-corrected by `mdlint format` or
  `mdlint check --fix`
- **Fast**: written in Rust for performance
- **Portable**: single, small, 0-dependency binary (Linux x86_64/ARM64, macOS Intel/Apple Silicon, Windows)
- **Git-aware**: respects `.gitignore` files by default

## Quickstart

No install needed — run directly with [uvx](https://docs.astral.sh/uv/guides/tools/):

```shell
uvx markdownlint-rs check   # lint all Markdown files in the current directory
uvx markdownlint-rs format  # format all Markdown files in the current directory
```

## Installation

`mdlint` comes packaged in many forms: static binaries, from a Python wrapper, from an NPM wrapper, and in a Docker
container! More ways of installing are in the works, but here's the current list:

> :warning: **Be aware:** `mdlint` is the executable name, but most package names are still `markdownlint-rs`!

```shell
# cargo
cargo install markdownlint-rs

# uv
uv tool install markdownlint-rs

# or as a project dependency
uv add --dev markdownlint-rs

# pip
pip install markdownlint-rs

# npm project dependency
npm install --save-dev markdownlint-rs

# Docker (linux/amd64 and linux/arm64) - hosted on both DockerHub and GitHub Container Registry
docker run --rm -v "$PWD:/workspace" ghcr.io/swanysimon/mdlint:latest check
docker run --rm -v "$PWD:/workspace" simonswanson/mdlint:latest       format
```

Pre-built binaries for Linux (x86_64/ARM64, glibc and musl), macOS (Intel/Apple Silicon), and Windows are available
on the [releases page](https://github.com/swanysimon/mdlint/releases). A [Homebrew](https://brew.sh) formula is planned.

### pre-commit framework

Add to `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/swanysimon/mdlint
    # use the latest release tag
    rev: v0.3.22
    hooks:
      - id: mdlint-format
      - id: mdlint-check
```

Or use additional arguments, e.g. to disable auto-fix:

```yaml
    hooks:
      - id: mdlint-format
        args: [--check]
      - id: mdlint-check
        args: [--no-fix]
```

## Usage

### mdlint check

Lint Markdown files and report issues.

```text
Usage: mdlint check [OPTIONS] [FILES]...

Arguments:
  [FILES]...                       Files or directories to check (defaults to current directory)

Options:
      --fix                        Apply auto-fixes where possible
      --no-fix                     Disable auto-fix even if enabled in config
      --output-format <FORMAT>     Output format [default: default] [possible values: default, json]
      --select <RULE_CODE>,...     Enable only the specified rules (comma-separated, or ALL)
      --ignore <RULE_CODE>,...     Disable the specified rules (comma-separated)
      --exclude <PATH>             Exclude files or directories from analysis
      --no-respect-ignore          Do not respect .gitignore files
      --parallel                   Lint files in parallel (experimental)
  -h, --help                       Print help

Global options:
      --config <CONFIG>            Path to TOML configuration file
      --no-config                  Ignore all configuration files
  -v, --verbose                    Enable verbose logging
  -q, --quiet                      Print diagnostics only
  -s, --silent                     Disable all logging (exit code still reflects result)
      --color <COLOR>              Control colors in output [default: auto] [possible values: auto, always, never]
```

### mdlint format

Format Markdown files with opinionated style.

```text
Usage: mdlint format [OPTIONS] [FILES]...

Arguments:
  [FILES]...                       Files or directories to format (defaults to current directory)

Options:
      --check                      Check formatting only; do not modify files (exits 1 if any file would change)
      --exclude <PATH>             Exclude files or directories
      --no-respect-ignore          Do not respect .gitignore files
  -h, --help                       Print help
```

### Examples

```bash
# check all Markdown files and apply auto-fixes
mdlint check --fix

# check specific files
mdlint check README.md docs/

# check with JSON output (for CI integrations)
mdlint check --output-format json

# enable only specific rules
mdlint check --select MD001,MD022

# disable specific rules for this run
mdlint check --ignore MD013,MD033

# format all files
mdlint format

# verify formatting without modifying files (for CI)
mdlint format --check

# format specific files
mdlint format README.md docs/

# use a custom config file
mdlint check --config path/to/mdlint.toml

# ignore all config files
mdlint check --no-config
```

## Configuration

mdlint uses TOML configuration files, discovered by searching upward from the current directory. The tool searches for
these files in order (first found wins per directory level), walking up from the current directory:

1. `mdlint.toml`
2. `.mdlint.toml`

Planned: `package.json` and `pyproject.toml` support.

### Configuration hierarchy

Configs are discovered by walking up the directory tree. Scalar values from closer configs override those farther away;
arrays are extended. Priority order (highest to lowest):

1. `--config` flag on the CLI
2. `mdlint.toml` / `.mdlint.toml` in the current directory
3. Config files in parent directories (walking up to the filesystem root)
4. Built-in defaults

### Global options

| Option | Default | Description |
| --- | --- | --- |
| `default_enabled` | `true` | Enable all rules unless explicitly disabled; `false` to enable only configured rules. |
| `gitignore` | `true` | Respect `.gitignore` files when discovering Markdown files. |
| `no_inline_config` | `false` | Ignore all `<!-- mdlint-disable -->` comments. |
| `fix` | `true` | `mdlint check` automatically applies all fixable violations; equivalent to passing `--fix` on the CLI. |
| `front_matter` | auto | Front matter delimiter. Auto-detects `---` (YAML) and `+++` (TOML). Set to `"---"` to accept YAML only. |
| `exclude` | `[]` | Paths/glob patterns excluded from discovery; merged with any `--exclude` CLI flags. |
| `custom_rules` | `[]` | Paths to external rule modules (future feature). |

### Rule configuration

Each rule is configured in its own `[rules.MDxxx]` section. Providing any parameter enables the rule. Use
`enabled = false` to explicitly disable a rule when `default_enabled = true`.

```toml
# disable a rule
[rules.MD013]
enabled = false

# configure parameters (also enables the rule)
[rules.MD013]
line_length = 100
code_blocks = false

# combine enabled flag with parameters
[rules.MD044]
enabled = true
names = ["JavaScript", "TypeScript", "GitHub"]
```

See [`mdlint.default.toml`](mdlint.default.toml) for every option with its default value.

### Inline configuration

Rules can be suppressed for specific lines using HTML comments:

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

Multiple rules: `<!-- mdlint-disable MD001 MD013 -->` — space-separate rule codes. Set `no_inline_config = true`
in `mdlint.toml` to ignore all inline comments project-wide.

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | Success — no lint violations (or files are already formatted with `format --check`) |
| `1` | Lint violations found (or files need formatting with `format --check`) |
| `2` | Runtime error (invalid config, file not found, etc.) |

## Rules

Rules marked ✓ in the **Fix** column are auto-corrected by `mdlint check --fix` and `mdlint format`. Rules without ✓
are reported by `mdlint check` only and require manual correction. **Default** shows mdlint's configured default for
the rule's key parameter(s); **markdownlint** shows the
[original markdownlint](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md) default where it differs from
mdlint's. `—` means the rule has no configurable parameters.

| Rule | Fix | Default | markdownlint | Description | Notes |
| --- | --- | --- | --- | --- | --- |
| [MD001](https://github.com/DavidAnson/markdownlint/blob/main/doc/md001.md) |  | — | — | Heading levels should only increment by one level at a time | Catches accidental heading skips (e.g. h1 → h3 without h2) |
| [MD003](https://github.com/DavidAnson/markdownlint/blob/main/doc/md003.md) |  | `atx` | `consistent` | Heading style should be consistent throughout the document | Config: `style` — `atx` (`# Heading`), `setext`, `atx_closed`, `consistent` |
| [MD004](https://github.com/DavidAnson/markdownlint/blob/main/doc/md004.md) | ✓ | `dash` | `consistent` | Unordered list style should be consistent | Config: `style` — `dash`, `asterisk`, `plus`, `consistent` |
| [MD005](https://github.com/DavidAnson/markdownlint/blob/main/doc/md005.md) |  | — | — | Inconsistent indentation for list items at the same level | Catches copy-paste errors where sibling items have different indentation |
| [MD007](https://github.com/DavidAnson/markdownlint/blob/main/doc/md007.md) |  | `indent: 2` |  | Unordered list indentation | Config: `indent` — spaces per nesting level |
| [MD009](https://github.com/DavidAnson/markdownlint/blob/main/doc/md009.md) | ✓ | `br_spaces: 2` |  | Trailing spaces | Two trailing spaces mean a hard line break; format converts them to `\` syntax. Config: `br_spaces`, `strict` |
| [MD010](https://github.com/DavidAnson/markdownlint/blob/main/doc/md010.md) | ✓ | `code_blocks: true` |  | Hard tabs | Tabs render inconsistently across editors; format replaces with spaces. Config: `code_blocks` |
| [MD011](https://github.com/DavidAnson/markdownlint/blob/main/doc/md011.md) |  | — | — | Reversed link syntax | Catches the common typo of swapped parentheses and brackets; should always be enabled |
| [MD012](https://github.com/DavidAnson/markdownlint/blob/main/doc/md012.md) | ✓ | `maximum: 1` |  | Multiple consecutive blank lines | Config: `maximum` — max consecutive blank lines allowed |
| [MD013](https://github.com/DavidAnson/markdownlint/blob/main/doc/md013.md) |  | `line: 120, heading: 80` | `line: 80` | Line length | mdlint raises the line limit to 120 to better fit URLs and long identifiers. Config: `line_length`, `heading_line_length`, `code_blocks`, `tables`, `headings` |
| [MD014](https://github.com/DavidAnson/markdownlint/blob/main/doc/md014.md) | ✓ | — | — | Dollar signs used before commands without showing output | `$`-prefixed shell commands cannot be copy-pasted; omit the `$` prompt |
| [MD018](https://github.com/DavidAnson/markdownlint/blob/main/doc/md018.md) | ✓ | — | — | No space after hash on atx style heading | `#Title` renders inconsistently; format inserts the required space |
| [MD019](https://github.com/DavidAnson/markdownlint/blob/main/doc/md019.md) | ✓ | — | — | Multiple spaces after hash on atx style heading | `#  Title` → `# Title`; format normalises to one space |
| [MD020](https://github.com/DavidAnson/markdownlint/blob/main/doc/md020.md) | ✓ | — | — | No space inside hashes on closed atx style heading | Only relevant if using `#Title#` style headings |
| [MD021](https://github.com/DavidAnson/markdownlint/blob/main/doc/md021.md) | ✓ | — | — | Multiple spaces inside hashes on closed atx style heading | Only relevant if using `#Title#` style headings |
| [MD022](https://github.com/DavidAnson/markdownlint/blob/main/doc/md022.md) | ✓ | — | — | Headings should be surrounded by blank lines | Required by many renderers for correct parsing; format inserts blank lines |
| [MD023](https://github.com/DavidAnson/markdownlint/blob/main/doc/md023.md) | ✓ | — | — | Headings must start at the beginning of the line | Indented headings are treated as code or paragraphs in CommonMark |
| [MD024](https://github.com/DavidAnson/markdownlint/blob/main/doc/md024.md) |  | `siblings_only: false` |  | Multiple headings with the same content | Config: `siblings_only` — set `true` to flag only duplicates within the same parent heading |
| [MD025](https://github.com/DavidAnson/markdownlint/blob/main/doc/md025.md) |  | — | — | Multiple top-level headings in the same document | Disable for document fragments that intentionally lack a single top-level title |
| [MD026](https://github.com/DavidAnson/markdownlint/blob/main/doc/md026.md) |  | `".,;:!?。，；：！？"` | `".,;:!。，；：！"` | Trailing punctuation in heading | Headings are labels, not sentences. mdlint additionally disallows `?`. Config: `punctuation` |
| [MD027](https://github.com/DavidAnson/markdownlint/blob/main/doc/md027.md) | ✓ | — | — | Multiple spaces after blockquote symbol | `>  text` → `> text`; format normalises |
| [MD028](https://github.com/DavidAnson/markdownlint/blob/main/doc/md028.md) |  | — | — | Blank line inside blockquote | Blank lines split blockquotes into separate elements in CommonMark; may be intentional |
| [MD029](https://github.com/DavidAnson/markdownlint/blob/main/doc/md029.md) | ✓ | `ordered` | `one_or_ordered` | Ordered list item prefix | mdlint requires sequential numbering. Config: `style` — `ordered` (1. 2. 3.), `one` (all 1s), `one_or_ordered` |
| [MD030](https://github.com/DavidAnson/markdownlint/blob/main/doc/md030.md) | ✓ | `all: 1` |  | Spaces after list markers | Config: `ul_single`, `ul_multi`, `ol_single`, `ol_multi` — spaces after marker per context |
| [MD031](https://github.com/DavidAnson/markdownlint/blob/main/doc/md031.md) | ✓ | — | — | Fenced code blocks should be surrounded by blank lines | Some renderers require blank lines around fences to parse correctly |
| [MD032](https://github.com/DavidAnson/markdownlint/blob/main/doc/md032.md) |  | — | — | Lists should be surrounded by blank lines | Consistent blank lines around lists improve rendering across processors |
| [MD033](https://github.com/DavidAnson/markdownlint/blob/main/doc/md033.md) |  | `allowed_elements: []` |  | Inline HTML | HTML reduces portability. Config: `allowed_elements` — add e.g. `["details", "summary"]` |
| [MD034](https://github.com/DavidAnson/markdownlint/blob/main/doc/md034.md) |  | — | — | Bare URL used | Plain URLs don't render as links in all Markdown processors; use `[text](url)` |
| [MD035](https://github.com/DavidAnson/markdownlint/blob/main/doc/md035.md) | ✓ | `---` | `consistent` | Horizontal rule style | Config: `style` — `---`, `***`, `___`, `consistent` |
| [MD036](https://github.com/DavidAnson/markdownlint/blob/main/doc/md036.md) |  | `".,;:!?。，；：！？"` |  | Emphasis used instead of a heading | Bold/italic-only lines won't appear in a table of contents. Config: `punctuation` |
| [MD037](https://github.com/DavidAnson/markdownlint/blob/main/doc/md037.md) |  | — | — | Spaces inside emphasis markers | `* text *` and `** text **` do not render as emphasis in CommonMark |
| [MD038](https://github.com/DavidAnson/markdownlint/blob/main/doc/md038.md) |  | — | — | Spaces inside code span elements | `` ` text ` `` is technically valid but inconsistent with expected style |
| [MD039](https://github.com/DavidAnson/markdownlint/blob/main/doc/md039.md) |  | — | — | Spaces inside link text | `[ text ]` is valid but inconsistent |
| [MD040](https://github.com/DavidAnson/markdownlint/blob/main/doc/md040.md) |  | — | — | Fenced code blocks should have a language specified | Language tags enable syntax highlighting. Config: `allowed_languages` |
| [MD041](https://github.com/DavidAnson/markdownlint/blob/main/doc/md041.md) |  | `level: 1` |  | First line in file should be a top-level heading | Disable for files starting with badges, front matter, or that are document fragments |
| [MD042](https://github.com/DavidAnson/markdownlint/blob/main/doc/md042.md) |  | — | — | No empty links | `[text]()` is almost always a mistake |
| [MD043](https://github.com/DavidAnson/markdownlint/blob/main/doc/md043.md) |  | — | — | Required heading structure | Useful for template-driven documentation; too restrictive for most projects. Config: `headings` |
| [MD044](https://github.com/DavidAnson/markdownlint/blob/main/doc/md044.md) |  | `names: []` |  | Proper names should have the correct capitalization | Requires configuration to be useful. Config: `names`, `code_blocks` |
| [MD045](https://github.com/DavidAnson/markdownlint/blob/main/doc/md045.md) |  | — | — | Images should have alternate text (alt text) | Alt text is required for accessibility; screen readers depend on it |
| [MD046](https://github.com/DavidAnson/markdownlint/blob/main/doc/md046.md) |  | `fenced` | `consistent` | Code block style | Config: `style` — `fenced`, `indented`, `consistent` |
| [MD047](https://github.com/DavidAnson/markdownlint/blob/main/doc/md047.md) | ✓ | — | — | Files should end with a single newline character | POSIX standard; prevents "no newline at end of file" noise in git diffs |
| [MD048](https://github.com/DavidAnson/markdownlint/blob/main/doc/md048.md) |  | `backtick` | `consistent` | Code fence style | Config: `style` — `backtick`, `tilde`, `consistent` |
| [MD049](https://github.com/DavidAnson/markdownlint/blob/main/doc/md049.md) | ✓ | `asterisk` | `consistent` | Emphasis style should be consistent | Config: `style` — `asterisk`, `underscore`, `consistent` |
| [MD050](https://github.com/DavidAnson/markdownlint/blob/main/doc/md050.md) | ✓ | `asterisk` | `consistent` | Strong style should be consistent | Config: `style` — `asterisk`, `underscore`, `consistent` |
| [MD051](https://github.com/DavidAnson/markdownlint/blob/main/doc/md051.md) |  | — | — | Link fragments should be valid | Broken `#anchor` links are invisible to parsers but silently break in-page navigation |
| [MD052](https://github.com/DavidAnson/markdownlint/blob/main/doc/md052.md) |  | — | — | Reference links and images should use a label that is defined | Undefined reference links silently render as plain text instead of a link |
| [MD053](https://github.com/DavidAnson/markdownlint/blob/main/doc/md053.md) |  | — | — | Link and image reference definitions should be needed | Cleans up leftover link definitions after references are removed |
| [MD054](https://github.com/DavidAnson/markdownlint/blob/main/doc/md054.md) |  | — | — | Link and image style | Enforces consistent use of inline vs reference link syntax |
| [MD055](https://github.com/DavidAnson/markdownlint/blob/main/doc/md055.md) |  | `leading_and_trailing` | `consistent` | Table pipe style | Config: `style` — `leading_and_trailing`, `leading_only`, `trailing_only`, `no_leading_or_trailing`, `consistent` |
| [MD056](https://github.com/DavidAnson/markdownlint/blob/main/doc/md056.md) |  | — | — | Table column count | Mismatched column counts cause unpredictable table rendering across processors |
| [MD058](https://github.com/DavidAnson/markdownlint/blob/main/doc/md058.md) |  | — | — | Tables should be surrounded by blank lines | Blank lines ensure tables are consistently parsed across Markdown processors |
| [MD059](https://github.com/DavidAnson/markdownlint/blob/main/doc/md059.md) |  | — | — | Link text should be descriptive | "click here" and "read more" are inaccessible; use meaningful link text |
| [MD060](https://github.com/DavidAnson/markdownlint/blob/main/doc/md060.md) |  | `consistent` | `any` | Table column style | mdlint requires a consistent alignment choice. Config: `style` — `consistent`, `default`, `left`, `right`, `center` |

## Contributing

Contributions are welcome!

### Development setup

Prerequisites: [mise](https://mise.jdx.dev/) and [Rust](https://rustup.rs/). Optionally, Docker is needed for
Dockerfile linting. [uv](https://docs.astral.sh/uv/) is required only if working on the Python package.

```bash
git clone https://github.com/swanysimon/mdlint.git
cd mdlint
mise install   # installs prek, tombi, hadolint
cargo build
```

### Code quality

All quality checks run via `prek run -a`. This must pass before submitting a pull request.

### Pull request process

1. Create a feature branch from `main`
2. Make focused commits with clear messages
3. Add tests for new functionality
4. Run `prek run -a` and fix any failures
5. Submit a PR with a description of what changed and why

### Release process

Releases use [`cargo-release`](https://github.com/crate-ci/cargo-release), which bumps all package manifests in sync
and pushes the tag that triggers CI to build, package, and publish everything automatically:

```bash
cargo release patch --execute   # or minor / major
```

Once the tag is pushed, CI verifies manifest versions, builds binaries for all 7 platforms, and publishes to
crates.io, PyPI, and npm via trusted publishing (no tokens required).

## License

The Unlicense - see [LICENSE](./LICENSE) for details.

## Acknowledgments

- [markdownlint](https://github.com/DavidAnson/markdownlint) by David Anson — original rule definitions
- [markdownlint-cli2](https://github.com/DavidAnson/markdownlint-cli2) also by David Anson - most people's first
  frontend to markdownlint
- [mdformat](https://github.com/hukkin/mdformat) — inspiration for the formatter-first approach
- [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) — Markdown parsing

## Resources

- [Documentation](./README.md)
- [Issue Tracker](https://github.com/swanysimon/mdlint/issues)
- [Releases](https://github.com/swanysimon/mdlint/releases)
- [markdownlint Rules Reference](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md)
