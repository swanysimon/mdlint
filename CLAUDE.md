# markdownlint-rs

@README.md

## Project Philosophy

mdlint is an **opinionated Markdown formatter** first, linter second — analogous to ruff or gofmt.
The formatter (`mdlint format`) enforces a canonical style by rewriting files. The linter
(`mdlint check`) reports violations that fall outside what the formatter can fix automatically.

Core principles: correctness over performance, type safety, minimal code, no duplication,
comprehensive testing.

## Task Tracking

All development tasks are tracked in [AIDEV.md](AIDEV.md) as AI-addressable prompts.

## Architecture

```text
src/
  main.rs / lib.rs       # Entry point and library root
  args.rs                # CLI argument definitions (clap)
  config/                # TOML config loading, types, and hierarchical merging
  glob/                  # File discovery (ignore crate) and glob matching
  markdown/              # pulldown-cmark wrapper with position tracking
  lint/                  # Rule trait, registry, engine, Violation type
    rules/               # Individual rule implementations (md001.rs, etc.)
  fix/                   # Auto-fix framework
  format/                # Output formatters (default, JSON, JUnit, SARIF)
  logger/                # Log level handling
  error.rs / types.rs    # Shared types and error definitions
```

## Lessons Learned

### Configuration System

* TOML is the config format (`mdlint.toml` or `.mdlint.toml`); hierarchical discovery walks up from cwd
* Config merging: later (closer to root) configs override earlier; arrays extend rather than replace
* Front matter: string-based detection for YAML (`---`) and TOML (`+++`) delimiters avoids regex overhead

### File Discovery and Globbing

* Use `ignore` crate for gitignore-aware traversal; requires actual git repo to respect `.gitignore`
* Relative path matching: canonicalize root path, use `strip_prefix()` before glob matching
* Exclude pattern normalization: simple names like `node_modules` → `**/node_modules/**`
* Markdown extensions: md, markdown, mdown, mkdn, mkd, mdwn, mdtxt, mdtext

### Markdown Parsing

* pulldown-cmark: return `impl Iterator<Item = Event<'a>>` to avoid lifetime complexity
* Position tracking: cumulative byte offsets with `line.len() + 1` (accounting for newlines)
* Extensions enabled: tables, footnotes, strikethrough, tasklists, heading attributes

### Rule System

* `Rule` trait: `name()`, `description()`, `tags()`, `check(&MarkdownParser, Option<&Value>)`,
  `fixable()` (default false)
* Registry pattern: `HashMap`-based with `create_default_registry()`
* Rules parse their own config from `Option<&Value>`

### Code Quality

* All checks run via `prek run -a` (defined in `prek.toml`), managed by `mise` (`mise.toml`)
* `prek run -a` runs: tombi TOML fmt/check → rustfmt → clippy (with `--fix`) → cargo test → mdlint
  dogfood → hadolint Dockerfile check
* Clippy runs as errors: `-D warnings`; clippy autofix applied before tests by prek
* Common fixes: `unwrap_or()` over manual `is_some()`, iterators over range loops, `!is_empty()`

### Testing Strategy

* Test business logic, not libraries — focus on merge algorithms, discovery patterns, rule logic
* `cargo test --lib` for unit tests via `src/lib.rs`; `tests/compatibility.rs` for Docker-based tests
* Compatibility tests skip gracefully when Docker is unavailable

### CI/CD Architecture

* Pipeline stages: (1) fast checks in parallel — test, clippy, fmt; (2) dogfooding;
  (3) slow checks — build, compatibility, security audit
* Job dependencies via `needs: [test, clippy, fmt]`
* Three workflow files: `ci.yml` (reusable quality gates), `tag.yml` (production release),
  `release.yml` (manual testing)
* Cross-compilation: native builds for Linux x86, macOS, Windows; `cross` tool for Linux ARM

### Release Process

* `cargo-release`: `cargo release patch --execute` bumps version, commits, tags, pushes
* Tag version must match `Cargo.toml` version — verified in CI before running tests
* Five platforms: Linux x86_64 (glibc + musl), Linux aarch64, macOS x86_64, macOS aarch64, Windows x86_64

## References

* [markdownlint rules](https://github.com/DavidAnson/markdownlint)
* [mdformat](https://github.com/hukkin/mdformat) — formatter-first inspiration
* [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) — Markdown parser
* [cargo-release](https://github.com/crate-ci/cargo-release) — release automation
