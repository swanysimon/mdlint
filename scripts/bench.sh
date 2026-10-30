#!/usr/bin/env bash
# Benchmarks `mdlint check` and `mdlint format` end to end over a pinned copy of
# the Rust book. These are whole-command numbers: they include process startup,
# file discovery, I/O and diagnostic rendering, so they measure the tool as a
# user experiences it rather than any one rule. hyperfine's default metrics add
# peak resident memory next to wall-clock time.
set -euo pipefail

# rust-lang/book, pinned so the corpus never shifts under the numbers.
readonly BOOK_REV="917544888a55e4da7109bdba8c88c893c0da70f4"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cache="$root/target/bench"
pristine="$cache/pristine"
work="$cache/work"
binary="$root/target/release/mdlint"

if [[ ! -d "$pristine" ]]; then
  echo "fetching corpus: rust-lang/book @ ${BOOK_REV:0:12}"
  rm -rf "$cache/book-$BOOK_REV"
  mkdir -p "$cache"
  curl --fail --silent --show-error --location \
    "https://github.com/rust-lang/book/archive/$BOOK_REV.tar.gz" |
    tar -xz -C "$cache"
  cp -R "$cache/book-$BOOK_REV/src" "$pristine"
  rm -rf "$cache/book-$BOOK_REV"
fi

cargo build --release

echo "corpus: $(find "$pristine" -name '*.md' | wc -l | tr -d ' ') Markdown files"

# hyperfine compares every command with the first one in its invocation, so each
# invocation holds the variants of a single command: `check --parallel` against
# `check`, `format --reflow` against `format`. Commands run without a shell, so
# the quoted paths below are split by hyperfine itself.
common=(--warmup 3 --runs 10)

# `check` exits 1 on this corpus because the Rust book has violations. Accepting
# exactly that code keeps a crash or a startup error fatal, and `check --no-fix`
# never writes, so it runs on the pristine copy.
hyperfine "${common[@]}" --ignore-failure=1 \
  --command-name "check" \
  "$binary check --no-fix --color never '$pristine'" \
  --command-name "check --parallel" \
  "$binary check --no-fix --parallel --color never '$pristine'"

# `format` rewrites files, so every run starts from a fresh copy of the corpus.
# The restore is setup, not measured, and runs in a shell even though the
# benchmarked commands do not.
hyperfine "${common[@]}" \
  --prepare "rm -rf '$work' && cp -R '$pristine' '$work'" \
  --command-name "format" \
  "$binary format --color never '$work'" \
  --command-name "format --reflow" \
  "$binary format --reflow --color never '$work'"
