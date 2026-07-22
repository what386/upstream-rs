#!/usr/bin/env python3
"""Validate all package definitions in registry/packages."""

from __future__ import annotations

from pathlib import Path
import sys

from common import RegistryValidationError, load_registry


ROOT = Path(__file__).resolve().parents[2]


def main() -> int:
    try:
        packages = load_registry(ROOT / "registry" / "packages")
    except RegistryValidationError as error:
        for message in error.errors:
            print(f"error: {message}", file=sys.stderr)
        return 1

    print(f"validated {len(packages)} registry package(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
