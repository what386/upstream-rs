#!/usr/bin/env python3
"""Build readable and minified registry indexes from package TOML files."""

from __future__ import annotations

from pathlib import Path
import sys

from common import RegistryValidationError, write_index


ROOT = Path(__file__).resolve().parents[2]


def main() -> int:
    outputs = (
        (ROOT / "registry" / "index.json", False),
        (ROOT / "registry" / "index.min.json", True),
    )
    try:
        for output, minified in outputs:
            write_index(
                ROOT / "registry" / "packages", output, minified=minified
            )
    except RegistryValidationError as error:
        for message in error.errors:
            print(f"error: {message}", file=sys.stderr)
        return 1

    for output, _ in outputs:
        print(f"wrote {output.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
