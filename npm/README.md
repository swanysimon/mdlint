# mdlint

[mdlint](https://github.com/swanysimon/mdlint) is an opinionated Markdown formatter and linter written in Rust.

The package wraps the pre-built `mdlint` binary via platform-specific optional dependencies.
No Rust toolchain is required to install or use it.

## Installation

```shell
npm install --save-dev markdownlint-rs
```

## Usage

```shell
# Format Markdown files
mdlint format

# Check for issues and auto-fix
mdlint check

# Check only
mdlint check --no-fix
```

See the [full documentation](https://github.com/swanysimon/mdlint) for all options,
configuration, and CI integration examples.

## How it works

`npm install mdlint` also installs the platform-specific optional dependency that bundles the
correct pre-built `mdlint` binary for your OS and architecture. The `mdlint` command is a thin
Node.js wrapper that locates and execs that binary.

Supported platforms:

| Platform | Architecture | Bundled binary |
| --- | --- | --- |
| Linux (glibc) | x64 | `mdlint-linux-x64` |
| Linux (glibc) | arm64 | `mdlint-linux-arm64` |
| Linux (musl) | x64 | `mdlint-linux-x64-musl` |
| Linux (musl) | arm64 | `mdlint-linux-arm64-musl` |
| macOS | x64 | `mdlint-darwin-x64` |
| macOS | arm64 (Apple Silicon) | `mdlint-darwin-arm64` |
| Windows | x64 | `mdlint-win32-x64.exe` |

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) >= 14

### Validate package locally

```shell
cd npm
npm pack --dry-run
```

### Release

Releases are automated via `.github/workflows/build-npm.yml`. On a version tag push, the
workflow downloads each pre-built binary from the GitHub release, creates a platform-specific
npm package, and publishes it alongside the main package to npm.

The npm package uses trusted publishing via GitHub Actions OIDC — no token is required.
