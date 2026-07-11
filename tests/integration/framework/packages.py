"""Helpers for checking package records and installed executables."""

from __future__ import annotations

from pathlib import Path
import subprocess

from .commands import run_upstream, run_upstream_json


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


def install_package(repo: str, package: str, tag: str) -> dict[str, object]:
    run_upstream("install", repo, package, "--tag", tag, "--yes")
    return package_from_list(package)


def package_version(package: dict[str, object]) -> tuple[int, int, int]:
    version = package.get("version")
    if not isinstance(version, dict):
        raise AssertionError(f"package has no version: {package!r}")
    return (version["major"], version["minor"], version["patch"])


def assert_executable_version(package: dict[str, object], expected_prefix: str) -> None:
    executable = package_path(package)
    assert executable.is_file(), executable
    result = subprocess.run(
        [str(executable), "--version"],
        check=True,
        text=True,
        capture_output=True,
    )
    assert result.stdout.startswith(expected_prefix), result.stdout
