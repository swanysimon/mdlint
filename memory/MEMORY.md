# markdownlint-rs Project Memory

## Project Pivot (Feb 2026)

mdlint is now positioned as an **opinionated formatter first, linter second** — like ruff/gofmt, not
markdownlint-cli2. The formatter (`mdlint format`) is the hero feature; the linter (`mdlint check`)
is secondary.

## Key Files

- `AIDEV.md` — task checklist as AI-addressable prompts (authoritative source of truth for work)
- `CLAUDE.md` — lessons learned and architecture notes for AI context (kept concise, ~100 lines)
- `IMPROVEMENTS.md` — redirect to AIDEV.md (deprecated)
- `FORMAT_SPEC.md` — does not exist yet; needs to be written (Priority 1 task in AIDEV.md)

## Architecture

- Config: TOML (`mdlint.toml` / `.mdlint.toml`), hierarchical discovery
- File discovery: `ignore` crate (gitignore-aware)
- Markdown parsing: `pulldown-cmark` wrapper in `src/markdown/`
- Rules: `src/lint/rules/md*.rs`, registered in `create_default_registry()`
- Many rules exist beyond what old CLAUDE.md docs listed — check `src/lint/rules/` directly

## Tooling

- `mise install` bootstraps all tools (`prek`, `tombi`, `hadolint`) — only Rust and mise are manual prereqs
- `prek run -a` runs ALL quality checks (TOML fmt, rustfmt, clippy --fix, tests, mdlint dogfood, hadolint)
- This is the single command for both local dev and CI quality gates
- Config: `mise.toml` (tool versions) + `prek.toml` (hook definitions)

## User Preferences

- Tasks tracked in AIDEV.md as AI-readable prompts, not numbered phases
- CLAUDE.md should stay under 200 lines (truncation limit)
- No compatibility goal with markdownlint-cli2; mdlint defines its own canonical format
