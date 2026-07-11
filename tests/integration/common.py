#!/usr/bin/env python3
"""Helpers for black-box tests that run the test-feature upstream binary."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
FAKEHOME = ROOT / "tests" / "fakehome"
RESET_TESTHOME = ROOT / "scripts" / "test" / "reset-testhome.sh"


def reset_fakehome() -> None:
    subprocess.run(["bash", str(RESET_TESTHOME)], cwd=ROOT, check=True)


def upstream_binary() -> Path:
    binaries = FAKEHOME / ".upstream" / "packages" / "binaries"
    candidates = sorted(path for path in binaries.glob("upstream-*") if path.is_file())
    if len(candidates) != 1:
        raise AssertionError(f"expected one upstream test binary in {binaries}, found {candidates}")
    return candidates[0]


def run_upstream_result(*args: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["HOME"] = str(FAKEHOME)
    return subprocess.run(
        [str(upstream_binary()), "--no-pager", *args],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
    )


def run_upstream(*args: str) -> subprocess.CompletedProcess[str]:
    result = run_upstream_result(*args)
    if result.returncode:
        command = " ".join(result.args)
        raise AssertionError(
            f"upstream command failed ({result.returncode}): {command}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def run_upstream_json(*args: str) -> object:
    result = run_upstream(*args, "--json")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise AssertionError(f"expected JSON from upstream {args!r}: {result.stdout!r}") from error


def package_from_list(name: str) -> dict[str, object]:
    packages = run_upstream_json("list")
    if not isinstance(packages, list):
        raise AssertionError(f"expected package list, got {packages!r}")
    matches = [package for package in packages if package.get("name") == name]
    if len(matches) != 1:
        raise AssertionError(f"expected one package named {name!r}, got {matches!r}")
    return matches[0]


def package_path(package: dict[str, object]) -> Path:
    value = package.get("exec_path") or package.get("install_path")
    if not isinstance(value, str):
        raise AssertionError(f"package has no executable path: {package!r}")
    return Path(value)


def fail(message: str) -> "NoReturn":
    print(message, file=sys.stderr)
    raise SystemExit(1)
