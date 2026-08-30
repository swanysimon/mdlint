#!/usr/bin/env python3
"""Parse Renovate's dry-run debug log and apply every computed branch's
file changes directly onto the current working tree.

Renovate cannot merge its own update branches (regular dependency bumps
vs lock file maintenance) into a single PR - that split was made a hard
requirement upstream in v43+ and is not configurable away. Running
Renovate with dryRun=full skips every push/PR/branch-mutating call, but
it still computes each update branch in full and logs its complete file
contents at debug level before discarding it (see commitFilesToBranch in
Renovate's source, lib/workers/repository/update/branch/commit.ts - the
log message is literally "DRY-RUN: Would commit files to branch ...").
This script reads that log and writes every branch's file changes onto
the working tree so the caller can make one commit and open one PR
covering everything Renovate found, across every configured manager.

This depends on Renovate's internal logging behavior, which is not a
documented or stable public interface - re-verify this script's parsing
against the actual log shape after any Renovate version bump.
"""

import json
import sys
from pathlib import Path

LOG_MESSAGE_PREFIX = "DRY-RUN: Would commit files to branch"


def decode_contents(raw: object) -> bytes:
    if raw is None:
        return b""
    if isinstance(raw, str):
        return raw.encode("utf-8")
    if isinstance(raw, dict) and raw.get("type") == "Buffer" and "data" in raw:
        return bytes(raw["data"])
    raise TypeError(f"unrecognized file contents encoding: {raw!r}")


def iter_branch_entries(log_path: Path):
    with open(log_path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError:
                continue
            msg = record.get("msg", "")
            if isinstance(msg, str) and msg.startswith(LOG_MESSAGE_PREFIX):
                yield record


def apply_branch(record: dict, repo_root: Path) -> tuple[str, list[str]]:
    branch_name = record.get("branchName", "<unknown>")
    files = record.get("files") or []
    applied = []
    for file in files:
        path = file.get("path")
        if not path:
            continue
        target = repo_root / path
        if file.get("type") == "deletion":
            if target.exists():
                target.unlink()
                applied.append(f"deleted {path}")
            continue
        contents = decode_contents(file.get("rawContents", file.get("contents")))
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(contents)
        applied.append(f"wrote {path}")
    return branch_name, applied


def main() -> None:
    if len(sys.argv) != 2:
        print("usage: renovate_harvest.py <renovate-debug-log>", file=sys.stderr)
        sys.exit(2)

    log_path = Path(sys.argv[1])
    repo_root = Path.cwd()

    entries = list(iter_branch_entries(log_path))
    if not entries:
        print(
            "no 'DRY-RUN: Would commit files to branch' log entries found; "
            "nothing to apply this week (or Renovate's log format has changed - "
            "check the uploaded raw log if a PR was expected)"
        )
        return

    for record in entries:
        branch_name, applied = apply_branch(record, repo_root)
        summary = ", ".join(applied) if applied else "no file changes"
        print(f"applied computed branch '{branch_name}': {summary}")


if __name__ == "__main__":
    main()
