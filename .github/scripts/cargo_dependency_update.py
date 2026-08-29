#!/usr/bin/env python3
"""Run `cargo update` and revert any dependency bump whose new version was
published on crates.io more recently than MIN_RELEASE_AGE_DAYS.

`cargo update` resolves both in-range direct dependencies and transitive
dependencies in one pass, so this is the entire weekly Cargo maintenance
job - Renovate does not manage the cargo manager for this repository (see
renovate.json), specifically so both kinds of update land in a single PR.
Renovate's minimumReleaseAge guard does not apply to a raw lockfile
refresh like this one, so this script re-implements the same 20-day
policy directly against crates.io.
"""

import json
import os
import shutil
import subprocess
import sys
import tomllib
import urllib.error
import urllib.request
from datetime import datetime, timedelta, timezone

MIN_RELEASE_AGE_DAYS = 20
LOCKFILE = "Cargo.lock"
BACKUP = f"{LOCKFILE}.before"


def load_versions(path: str) -> dict[tuple[str, str], str | None]:
    with open(path, "rb") as f:
        data = tomllib.load(f)
    return {
        (pkg["name"], pkg["version"]): pkg.get("source")
        for pkg in data.get("package", [])
    }


def published_at(name: str, version: str) -> datetime:
    url = f"https://crates.io/api/v1/crates/{name}/{version}"
    request = urllib.request.Request(
        url, headers={"User-Agent": "mdlint-lockfile-maintenance (github.com/swanysimon/mdlint)"}
    )
    with urllib.request.urlopen(request, timeout=15) as response:
        body = json.load(response)
    return datetime.fromisoformat(body["version"]["created_at"].replace("Z", "+00:00"))


def main() -> None:
    shutil.copyfile(LOCKFILE, BACKUP)
    try:
        subprocess.run(["cargo", "update"], check=True)
        before = load_versions(BACKUP)
        after = load_versions(LOCKFILE)
    finally:
        os.remove(BACKUP)

    added = {key: source for key, source in after.items() if key not in before}
    removed_versions_by_name: dict[str, list[str]] = {}
    for name, version in set(before) - set(after):
        removed_versions_by_name.setdefault(name, []).append(version)

    cutoff = datetime.now(timezone.utc) - timedelta(days=MIN_RELEASE_AGE_DAYS)
    reverted = []

    for (name, new_version), source in added.items():
        if not source or "crates.io" not in source:
            continue

        old_versions = removed_versions_by_name.get(name)
        if not old_versions or len(old_versions) != 1:
            # No single prior version to revert to (brand-new transitive
            # dependency, or an ambiguous multi-version bump) - leave as-is.
            continue
        old_version = old_versions[0]

        try:
            when = published_at(name, new_version)
        except (urllib.error.URLError, KeyError, ValueError) as exc:
            print(f"warning: could not check release date for {name} {new_version}: {exc}", file=sys.stderr)
            continue

        if when > cutoff:
            spec = f"{name}@{new_version}"
            result = subprocess.run(["cargo", "update", "-p", spec, "--precise", old_version])
            if result.returncode != 0:
                # Some transitive dependencies are released in lockstep with
                # sibling crates that still require the newer version (e.g. a
                # shared internal crate family); reverting one in isolation
                # can fail to resolve. Best-effort: leave it at the newer
                # version rather than fail the whole maintenance run.
                print(
                    f"warning: could not revert {name} {new_version} -> {old_version} "
                    "in isolation (likely required by another updated package); leaving as-is",
                    file=sys.stderr,
                )
                continue
            print(
                f"reverted {name} {old_version} -> {new_version} "
                f"(published {when.date()}, younger than {MIN_RELEASE_AGE_DAYS} days)"
            )
            reverted.append(name)

    if reverted:
        print(f"reverted {len(reverted)} package(s) younger than {MIN_RELEASE_AGE_DAYS} days: {', '.join(sorted(reverted))}")


if __name__ == "__main__":
    main()
