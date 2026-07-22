#!/usr/bin/env python3
"""Validate package revision changes against a Git baseline."""

from __future__ import annotations

from pathlib import Path, PurePosixPath
import subprocess
import sys
import tomllib
from typing import Any

from common import RegistryValidationError, load_registry, validate_revision_changes


ROOT = Path(__file__).resolve().parents[2]
REGISTRY_PREFIX = PurePosixPath("registry/packages")


def packages_at_ref(ref: str) -> dict[str, dict[str, Any]]:
    if not ref or set(ref) == {"0"}:
        return {}

    listed = subprocess.run(
        ["git", "ls-tree", "-r", "--name-only", ref, "--", str(REGISTRY_PREFIX)],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    packages: dict[str, dict[str, Any]] = {}
    for relative in sorted(listed):
        path = PurePosixPath(relative)
        if path.parent != REGISTRY_PREFIX or path.suffix != ".toml":
            continue
        content = subprocess.run(
            ["git", "show", f"{ref}:{relative}"],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        ).stdout
        entry = tomllib.loads(content)
        name = entry.get("name")
        if not isinstance(name, str):
            continue
        packages[name] = {
            key: value for key, value in entry.items() if key != "name"
        }
    return packages


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: validate_revisions.py <base-git-ref>", file=sys.stderr)
        return 2

    try:
        current = load_registry(ROOT / "registry" / "packages")
        previous = packages_at_ref(argv[1])
    except RegistryValidationError as error:
        for message in error.errors:
            print(f"error: {message}", file=sys.stderr)
        return 1
    except (subprocess.CalledProcessError, tomllib.TOMLDecodeError) as error:
        print(f"error: failed to read registry baseline: {error}", file=sys.stderr)
        return 1

    errors = validate_revision_changes(previous, current)
    if errors:
        for message in errors:
            print(f"error: {message}", file=sys.stderr)
        return 1

    print(f"validated registry revisions against {argv[1]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
