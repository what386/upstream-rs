"""Helpers for checking package records and installed executables."""

from __future__ import annotations

from pathlib import Path
import subprocess

from .commands import run_upstream, run_upstream_json


def package_from_list(name: str) -> dict[str, object]:
    packages = run_upstream_json("list")
    if not isinstance(packages, list):
        raise AssertionError(f"expected package list, got {packages!r}")
    matches = [
        package
        for package in packages
        if package.get("id") == name
        or package.get("repo_slug", "").rsplit("/", 1)[-1].casefold() == name.casefold()
        or any(
            isinstance(executable, dict) and executable.get("name") == name
            for executable in package.get("executables", [])
        )
    ]
    if len(matches) != 1:
        raise AssertionError(f"expected one package named {name!r}, got {matches!r}")
    return matches[0]


def package_path(package: dict[str, object]) -> Path:
    executables = package.get("executables")
    if not isinstance(executables, list) or not executables:
        raise AssertionError(f"package has no executables: {package!r}")
    path = executables[0].get("path") if isinstance(executables[0], dict) else None
    if not isinstance(path, str):
        raise AssertionError(f"package has no executable path: {package!r}")
    return Path(path)


def install_package(repo: str, package: str, tag: str) -> dict[str, object]:
    run_upstream("install", repo, "--tag", tag, "--yes")
    return package_from_list(package)


def package_version(package: dict[str, object]) -> tuple[int, int, int]:
    version = package.get("version")
    if not isinstance(version, dict):
        raise AssertionError(f"package has no version: {package!r}")
    return (version["major"], version["minor"], version["patch"])


def release_version(tag: str) -> tuple[int, int, int]:
    parts = tag.split(".")
    if len(parts) < 3:
        raise AssertionError(f"expected a semantic release tag, got {tag!r}")
    try:
        return tuple(int(part) for part in parts[:3])
    except ValueError as error:
        raise AssertionError(f"expected a semantic release tag, got {tag!r}") from error


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
