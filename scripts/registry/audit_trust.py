#!/usr/bin/env python3
"""Audit registry release recipes against published GitHub checksums.

This command is deliberately read-only. It reports evidence and recommends a
trust policy, but never rewrites registry package definitions or indexes.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import sys
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlsplit
from urllib.request import Request, urlopen

from common import RegistryValidationError, load_registry


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_RELEASES = 3
COMMON_CHECKSUM_NAMES = (
    "checksums-bsd",
    "checksums-bsd.txt",
    "checksums.txt",
    "checksum.txt",
    "sha256sums.txt",
    "sha256sum.txt",
    "sha256sums",
    "sha256sum",
    "sha512sums.txt",
    "sha512sum.txt",
    "sha512sums",
    "sha512sum",
    "checksums",
)


@dataclass(frozen=True)
class Target:
    name: str
    os_tokens: tuple[str, ...]
    other_os_tokens: tuple[str, ...]
    arch_tokens: tuple[str, ...]
    other_arch_tokens: tuple[str, ...]


TARGETS = (
    Target("linux-x86_64", ("linux",), ("darwin", "macos", "windows", "msvc"), ("x86_64", "amd64"), ("aarch64", "arm64", "armv7", "i686")),
    Target("linux-aarch64", ("linux",), ("darwin", "macos", "windows", "msvc"), ("aarch64", "arm64"), ("x86_64", "amd64", "armv7", "i686")),
    Target("macos-x86_64", ("darwin", "macos", "osx"), ("linux", "windows", "msvc"), ("x86_64", "amd64"), ("aarch64", "arm64", "armv7", "i686")),
    Target("macos-aarch64", ("darwin", "macos", "osx"), ("linux", "windows", "msvc"), ("aarch64", "arm64"), ("x86_64", "amd64", "armv7", "i686")),
    Target("windows-x86_64", ("windows", "msvc", "win64"), ("linux", "darwin", "macos"), ("x86_64", "amd64", "win64"), ("aarch64", "arm64", "armv7", "i686")),
    Target("windows-aarch64", ("windows", "msvc"), ("linux", "darwin", "macos"), ("aarch64", "arm64"), ("x86_64", "amd64", "armv7", "i686")),
)


class AuditNetworkError(RuntimeError):
    """A provider or download request failed independently of trust coverage."""


def is_checksum_asset(name: str) -> bool:
    lowered = name.casefold()
    return (
        lowered in COMMON_CHECKSUM_NAMES
        or lowered.endswith((".sha256", ".sha512", ".sha256sum", ".sha512sum", ".sha256.txt", ".sha512.txt", ".sum"))
        or "checksums" in lowered
    )


def select_asset(assets: list[dict[str, Any]], package: dict[str, Any], target: Target) -> str | None:
    matches = tuple(value.casefold() for value in package.get("match", []))
    excludes = tuple(value.casefold() for value in package.get("exclude", []))
    identity = package.get("binary", "").casefold()
    candidates: list[tuple[int, str]] = []

    for asset in assets:
        name = str(asset.get("name", ""))
        lowered = name.casefold()
        if not name or is_checksum_asset(name) or lowered.endswith((".sig", ".minisig", ".pem", ".sbom", ".spdx.json")):
            continue
        if matches and not any(pattern in lowered for pattern in matches):
            continue
        if any(pattern in lowered for pattern in excludes):
            continue
        if any(token in lowered for token in target.other_os_tokens):
            continue
        if any(token in lowered for token in target.other_arch_tokens):
            continue

        score = 0
        score += 80 if any(token in lowered for token in target.os_tokens) else 0
        score += 80 if any(token in lowered for token in target.arch_tokens) else 0
        score += 20 if identity and identity in lowered else 0
        score += min(int(asset.get("size", 0)) // 1_000_000, 20)
        candidates.append((score, name))

    return max(candidates, default=(0, None))[1]


def find_checksum_asset(assets: list[dict[str, Any]], selected: str) -> dict[str, Any] | None:
    by_name = {str(asset.get("name", "")).casefold(): asset for asset in assets}
    basename = Path(selected).name
    for candidate in (
        f"{selected}.sha256",
        f"{selected}.sha512",
        f"{basename}.sha256",
        f"{basename}.sha512",
        f"{basename}.sha256sum",
        f"{basename}.sha512sum",
        *COMMON_CHECKSUM_NAMES,
    ):
        if asset := by_name.get(candidate.casefold()):
            return asset
    return next((asset for asset in assets if is_checksum_asset(str(asset.get("name", "")))), None)


def parse_checksum_names(contents: str) -> tuple[set[str], str]:
    names: set[str] = set()
    bare_hashes = 0
    for line in contents.splitlines():
        line = line.strip()
        if match := re.match(
            r"^(?:sha(?:256|512)[:=])?[A-Fa-f0-9]{64,128}\s+\*?(.+)$",
            line,
            re.IGNORECASE,
        ):
            names.add(match.group(1))
        elif match := re.match(r"^SHA(?:256|512) \((.+)\) = [A-Fa-f0-9]{64,128}$", line, re.IGNORECASE):
            names.add(match.group(1))
        elif match := re.match(
            r"^(.+?):\s*(?:sha(?:256|512)[:=])?[A-Fa-f0-9]{64,128}$",
            line,
            re.IGNORECASE,
        ):
            names.add(match.group(1))
        elif re.fullmatch(r"[A-Fa-f0-9]{64,128}", line):
            bare_hashes += 1
    if names:
        return names, "named"
    if bare_hashes == 1:
        return set(), "bare"
    return set(), "unsupported"


def checksum_covers(selected: str, checksum_name: str, contents: str) -> tuple[bool, str]:
    names, format_name = parse_checksum_names(contents)
    if format_name == "bare":
        specific = checksum_name.casefold().startswith(selected.casefold())
        return specific, "bare" if specific else "unsupported-bare"
    return (
        selected in names or any(Path(name).name == selected for name in names),
        format_name,
    )


def request_json(url: str, token: str | None) -> Any:
    headers = {"Accept": "application/vnd.github+json", "User-Agent": "upstream-registry-audit"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    try:
        with urlopen(Request(url, headers=headers), timeout=30) as response:
            return json.load(response)
    except (HTTPError, URLError, TimeoutError, json.JSONDecodeError) as error:
        raise AuditNetworkError(f"GET {url}: {error}") from error


def request_text(url: str, token: str | None) -> str:
    headers = {"User-Agent": "upstream-registry-audit"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    try:
        with urlopen(Request(url, headers=headers), timeout=30) as response:
            return response.read().decode("utf-8")
    except (HTTPError, URLError, TimeoutError, UnicodeError) as error:
        raise AuditNetworkError(f"GET {url}: {error}") from error


def github_slug(repo: str) -> str:
    return urlsplit(repo).path.strip("/")


def audit_release(package: dict[str, Any], release: dict[str, Any], token: str | None) -> list[dict[str, str]]:
    assets = list(release.get("assets", []))
    checksum_cache: dict[str, str] = {}
    evidence: list[dict[str, str]] = []
    for target in TARGETS:
        selected = select_asset(assets, package, target)
        if selected is None:
            evidence.append({"target": target.name, "asset": "-", "checksum": "-", "format": "none", "status": "missing-asset"})
            continue
        checksum = find_checksum_asset(assets, selected)
        if checksum is None:
            evidence.append({"target": target.name, "asset": selected, "checksum": "-", "format": "none", "status": "missing-checksum"})
            continue
        checksum_name = str(checksum["name"])
        if checksum_name not in checksum_cache:
            checksum_cache[checksum_name] = request_text(str(checksum["browser_download_url"]), token)
        covered, format_name = checksum_covers(selected, checksum_name, checksum_cache[checksum_name])
        evidence.append({"target": target.name, "asset": selected, "checksum": checksum_name, "format": format_name, "status": "covered" if covered else "uncovered"})
    return evidence


def audit_package(name: str, package: dict[str, Any], releases: int, token: str | None) -> dict[str, Any]:
    install = package["install"]
    if install["type"] != "release":
        return {"name": name, "configured": package["trust"], "recommended": "none", "releases": []}
    if install["provider"] != "github":
        raise AuditNetworkError(f"{name}: audit does not yet support provider {install['provider']}")
    url = f"https://api.github.com/repos/{github_slug(install['repo'])}/releases?per_page={max(releases * 2, 6)}"
    published = [release for release in request_json(url, token) if not release.get("draft") and not release.get("prerelease")][:releases]
    note = None
    if len(published) < releases:
        note = f"provider exposes {len(published)} stable release(s); requested {releases}"
    audited = []
    for release in published:
        audited.append({"tag": str(release.get("tag_name", "unknown")), "evidence": audit_release(package, release, token)})
    complete = bool(audited) and all(
        item["status"] == "covered"
        for release in audited
        for item in release["evidence"]
    )
    return {
        "name": name,
        "configured": package["trust"],
        "recommended": "checksum" if complete else "best-effort",
        "releases": audited,
        "note": note,
    }


def render_report(results: list[dict[str, Any]], errors: list[str]) -> str:
    lines = ["# Registry trust audit", ""]
    for result in results:
        lines.extend([f"## {result['name']}", "", f"Configured: `{result['configured']}`  ", f"Recommended: `{result['recommended']}`", ""])
        if result.get("note"):
            lines.extend([f"Note: {result['note']}", ""])
        for release in result["releases"]:
            lines.extend([f"### {release['tag']}", "", "| Target | Selected asset | Checksum asset | Format | Status |", "| --- | --- | --- | --- | --- |"])
            for item in release["evidence"]:
                lines.append(f"| {item['target']} | `{item['asset']}` | `{item['checksum']}` | {item['format']} | {item['status']} |")
            lines.append("")
    if errors:
        lines.extend(["## Provider/network errors", "", *[f"- {error}" for error in errors], ""])
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--packages-dir", type=Path, default=ROOT / "registry" / "packages")
    parser.add_argument("--package", action="append", default=[], help="audit only this package (repeatable)")
    parser.add_argument("--releases", type=int, default=DEFAULT_RELEASES)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    if args.releases < 1:
        parser.error("--releases must be at least 1")
    try:
        packages = load_registry(args.packages_dir)
    except RegistryValidationError as error:
        print(error, file=sys.stderr)
        return 2
    selected_names = args.package or list(packages)
    unknown = sorted(set(selected_names) - set(packages))
    if unknown:
        parser.error(f"unknown package(s): {', '.join(unknown)}")
    token = os.environ.get("GITHUB_TOKEN")
    results: list[dict[str, Any]] = []
    errors: list[str] = []
    for name in selected_names:
        try:
            results.append(audit_package(name, packages[name], args.releases, token))
        except AuditNetworkError as error:
            errors.append(str(error))
    report = render_report(results, errors)
    if args.output:
        args.output.write_text(report, encoding="utf-8")
    else:
        print(report)
    strict_losses = [result["name"] for result in results if result["configured"] == "checksum" and result["recommended"] != "checksum"]
    if strict_losses:
        print(f"strict checksum coverage lost: {', '.join(strict_losses)}", file=sys.stderr)
        return 1
    return 2 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
