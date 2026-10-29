#!/usr/bin/env bash
# Benchmarks `mdlint check` and `mdlint format` end to end over a pinned copy of
# the Rust book. These are whole-command numbers: they include process startup,
# file discovery, I/O and diagnostic rendering, so they measure the tool as a
# user experiences it rather than any one rule.
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

# `check` exits 1 on this corpus because the Rust book has violations, so
# hyperfine has to be told to ignore exit codes. Assert them once up front
# instead, so a binary that dies on startup fails here rather than quietly
# benchmarking a crash.
expect_exit() {
  local expected="$1" label="$2" status=0
  shift 2
  rm -rf "$work"
  cp -R "$pristine" "$work"
  "$@" >/dev/null 2>&1 || status=$?
  if [[ "$status" -ne "$expected" ]]; then
    echo "$label: expected exit $expected, got $status" >&2
    exit 1
  fi
}

expect_exit 1 "check" "$binary" check --no-fix --color never "$work"
expect_exit 0 "format" "$binary" format --color never "$work"
expect_exit 0 "format --reflow" "$binary" format --reflow --color never "$work"

# `format` rewrites files and `check` does not, so every run starts from a fresh
# copy of the corpus. The restore is setup, not measured.
hyperfine --warmup 3 --runs 10 --ignore-failure \
  --prepare "rm -rf '$work' && cp -R '$pristine' '$work'" \
  --command-name "check" \
  "$binary check --no-fix --color never '$work'" \
  --command-name "check --parallel" \
  "$binary check --no-fix --parallel --color never '$work'" \
  --command-name "format" \
  "$binary format --color never '$work'" \
  --command-name "format --reflow" \
  "$binary format --reflow --color never '$work'"
