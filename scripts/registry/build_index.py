#!/usr/bin/env python3
"""Build registry/index.json from validated package TOML files."""

from __future__ import annotations

from pathlib import Path
import sys

from common import RegistryValidationError, write_index


ROOT = Path(__file__).resolve().parents[2]


def main() -> int:
    output = ROOT / "registry" / "index.json"
    try:
        write_index(ROOT / "registry" / "packages", output)
    except RegistryValidationError as error:
        for message in error.errors:
            print(f"error: {message}", file=sys.stderr)
        return 1

    print(f"wrote {output.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
