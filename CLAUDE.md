# mdlint

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
  formatter/             # Canonical markdown rewriter (mdlint format)
  format/                # Output formatters (default, JSON, JUnit, SARIF)
  logger/                # Log level handling
  error.rs / types.rs    # Shared types and error definitions
```

## Lessons Learned

### Configuration System

- TOML is the config format (`mdlint.toml` or `.mdlint.toml`); hierarchical discovery walks up from cwd
- Config merging: later (closer to root) configs override earlier; arrays extend rather than replace
- Front matter: string-based detection for YAML (`---`) and TOML (`+++`) delimiters avoids regex overhead

### File Discovery and Globbing

- Use `ignore` crate for gitignore-aware traversal; requires actual git repo to respect `.gitignore`
- Relative path matching: canonicalize root path, use `strip_prefix()` before glob matching
- Exclude pattern normalization: simple names like `node_modules` → `**/node_modules/**`
- Markdown extensions: md, markdown, mdown, mkdn, mkd, mdwn, mdtxt, mdtext

### Markdown Parsing

- pulldown-cmark: return `impl Iterator<Item = Event<'a>>` to avoid lifetime complexity
- Position tracking: cumulative byte offsets with `line.len() + 1` (accounting for newlines)
- Extensions enabled: tables, footnotes, strikethrough, tasklists, heading attributes

### Rule System

- `Rule` trait: `name()`, `description()`, `tags()`, `check(&MarkdownParser, Option<&Value>)`,
  `fixable()` (default false)
- Registry pattern: `HashMap`-based with `create_default_registry()`
- Rules parse their own config from `Option<&Value>`

### Formatter

- Architecture: emit canonical text directly from pulldown-cmark events; no separate IR needed
- State machine tracks previous block type and inserts blank lines before each new block element
- Idempotency is a hard requirement: `format(format(x)) == format(x)`; proptest found real bugs
- Hard line breaks: trailing-space syntax (two spaces + `\n`) must become backslash continuation
  (`\\\n`) before trailing-whitespace stripping, otherwise the line break is lost
- Code blocks inside list items must be indented by the marker width (3 spaces for an ordered
  `1.` marker, 2 for an unordered `-` marker) to remain inside the list; tracked via
  `list_item_widths` stack in `FormatterState`;
  pulldown-cmark strips the indent on parse so the formatter must re-add it on emit
- `src/formatter/mod.rs` = canonical markdown rewriter; `src/format/` = output formatters
  (JSON, SARIF, JUnit, default) — different concerns, different directories
- Raw HTML blocks and code block contents are passed through verbatim

### Code Quality

- All checks run via `prek run -a` (defined in `prek.toml`), managed by `mise` (`mise.toml`)
- `prek run -a` runs hooks sequentially with `fail_fast = true`:
  trailing-whitespace → end-of-file-fixer → actionlint → hadolint → tombi check → tombi format →
  clippy (with `--fix`) → rustfmt → cargo test → mdlint check (dogfood) → mdlint format (dogfood)
- With `fail_fast`, each failure is one thing to fix; re-run after fixing until all hooks pass
- Formatting hooks (rustfmt, tombi format, end-of-file-fixer, trailing-whitespace) modify files in
  place and report failure — re-running immediately after often passes with no further changes needed
- Clippy runs as errors: `-D warnings`; autofix (`--fix --allow-dirty`) applied before tests
- Common fixes: `unwrap_or()` over manual `is_some()`, iterators over range loops, `!is_empty()`
- Prefer functional patterns: `str::replace` over char loops, `"x".repeat(n)` over push loops,
  bitflags OR operator over `.insert()` chains, `level as u8` over manual match-to-discriminant
- Extract long inline `match` arms to named methods; keep match arms as single expressions
- Avoid bare `{}` scope blocks to limit variable lifetimes — restructure with `if let` or extract
- Keep comments for *why*, not *what*; delete comments that restate adjacent code

### Testing Strategy

- Test business logic, not libraries — focus on merge algorithms, discovery patterns, rule logic
- `cargo test --lib` for unit tests via `src/lib.rs`; `tests/compatibility.rs` for Docker-based tests
- Compatibility tests skip gracefully when Docker is unavailable
- Property-based tests via `proptest`: generate random strings, verify formatter never panics, is
  idempotent, and produces valid CommonMark
- **Every transformation test must cover both modes:**
  - Formatter tests: use `assert_formats_to(input, expected)` — verifies `format(input) == expected`
    AND `format(expected) == expected` (idempotency / not-fix side) in one call
  - Lint rule tests: add an `apply_fixes` helper (calls `Fixer::apply_fixes_to_content`) and a
    separate test that applies the violations to the input and asserts the corrected output
- Write tests rather than running ad-hoc `cargo run` commands to verify behaviour
- CLI integration tests (in `tests/formatter.rs`) invoke the binary via `Command`; always add
  `.stdout(Stdio::null()).stderr(Stdio::null())` so binary output doesn't pollute test output

### Fix Framework

- `Fix` struct uses **1-indexed** line/column numbers; `Fixer` converts to 0-indexed internally with
  `saturating_sub(1)` — rule implementations must use 1-indexed values (e.g. from `offset_to_line`)
- Empty-replacement whole-line fix (`replacement: ""`, no column range) = **delete the line**;
  the Fixer splices it out of the Vec rather than setting it to an empty string
- Embedded newlines in a replacement string create extra lines in the output — used by some rules
  (e.g. MD022) to insert blank lines around headings without requiring a multi-line fix range
- Default config has `fix = true`, so `mdlint check` without an explicit config always applies
  fixes; tests that verify the no-fix path must supply a config file with `fix = false`

### CI/CD Architecture

- Pipeline stages: (1) fast checks in parallel — test, clippy, fmt; (2) dogfooding;
  (3) slow checks — build, compatibility, security audit
- Job dependencies via `needs: [test, clippy, fmt]`
- Three workflow files: `ci.yml` (reusable quality gates), `tag.yml` (production release),
  `release.yml` (manual testing)
- Cross-compilation: native builds for Linux x86, macOS, Windows; `cross` tool for Linux ARM

### Release Process

- `cargo-release`: `cargo release patch --execute` bumps version, commits, tags, pushes
- `release.toml` uses `pre-release-replacements` to keep manifests in sync:
  `Cargo.toml` (by cargo-release), `npm/package.json`, `python/pyproject.toml`.
  No version-setting commands are needed in CI.
- Tag version must match all manifests — verified in CI (`tag.yml`) before anything publishes
- Seven binary platforms: Linux x86_64/aarch64 (glibc + musl), macOS x86_64/aarch64, Windows x86_64

### npm Publishing

- Single package (`markdownlint-rs`) bundles all 7 platform binaries in `npm/bin/`
- `bin/mdlint.js` detects platform at runtime; uses `/proc/self/maps` to distinguish glibc vs musl on Linux
- Binaries are downloaded from the GitHub release and placed in `npm/bin/` during CI publish
- When adding a new platform, update `publish-npm.yml` (download + mv step) and `publish-python.yml` matrix
- npm trusted publishing (OIDC, passwordless): requires npm ≥ 11.5.1 (`npm install -g npm@latest`
  in CI), `id-token: write` permission, and the npmjs.com trusted publisher config pointing to the
  **calling** workflow (`tag.yml`), not the reusable workflow (`publish-npm.yml`)

### Python Publishing

- PyPI trusted publishing: use a single job with a bash loop over all 7 platforms rather than a
  matrix job — avoids concurrent upload errors (HTTP 500) and reduces environment approval prompts
  to one per release instead of one per matrix element

## References

- [FORMAT_SPEC.md](FORMAT_SPEC.md) — canonical formatter style decisions (source of truth)
- [markdownlint rules](https://github.com/DavidAnson/markdownlint)
- [mdformat](https://github.com/hukkin/mdformat) — formatter-first inspiration
- [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) — Markdown parser
- [cargo-release](https://github.com/crate-ci/cargo-release) — release automation
