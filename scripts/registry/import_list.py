#!/usr/bin/env python3
"""Create missing registry TOML entries from `upstream list --json` output."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys
from typing import Any
from urllib.parse import urlsplit

from common import TRUST_MODES, RegistryValidationError, is_safe_basename, load_registry, validate_entry


ROOT = Path(__file__).resolve().parents[2]
SUPPORTED_PROVIDERS = {"Github", "Gitlab", "Gitea"}


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Create missing registry package TOML files from Upstream list JSON."
    )
    parser.add_argument(
        "input",
        nargs="?",
        default="-",
        help="JSON input file, or '-' for stdin (default)",
    )
    parser.add_argument(
        "--trust",
        choices=sorted(TRUST_MODES),
        default="best-effort",
        help="trust mode assigned to generated entries (default: best-effort)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="show files that would be created without writing them",
    )
    parser.add_argument(
        "--packages-dir",
        type=Path,
        default=ROOT / "registry" / "packages",
        help=argparse.SUPPRESS,
    )
    return parser.parse_args(argv)


def read_records(input_name: str) -> list[dict[str, Any]]:
    try:
        text = sys.stdin.read() if input_name == "-" else Path(input_name).read_text(encoding="utf-8")
        value = json.loads(text)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"failed to read package JSON: {error}") from error

    if isinstance(value, dict) and isinstance(value.get("packages"), list):
        value = value["packages"]
    if not isinstance(value, list):
        raise ValueError("package JSON must be an array or an object containing a 'packages' array")
    if not all(isinstance(record, dict) for record in value):
        raise ValueError("every package JSON item must be an object")
    return value


def repository_name(repo_slug: str) -> str:
    parsed = urlsplit(repo_slug)
    path = parsed.path if parsed.scheme else repo_slug
    source_name = path.strip("/").rsplit("/", 1)[-1].removesuffix(".git")
    name = re.sub(r"[^a-z0-9]+", "-", source_name.lower()).strip("-")
    if not name:
        raise ValueError(f"cannot derive a valid registry name from repository '{repo_slug}'")
    return name


def repository_url(record: dict[str, Any]) -> tuple[str, str]:
    provider = record.get("provider")
    repo_slug = record.get("repo_slug")
    if provider not in SUPPORTED_PROVIDERS:
        raise ValueError(f"unsupported provider '{provider}'")
    if not isinstance(repo_slug, str) or not repo_slug.strip("/"):
        raise ValueError("missing non-empty repo_slug")

    parsed = urlsplit(repo_slug)
    slug = parsed.path.strip("/") if parsed.scheme else repo_slug.strip("/")
    slug = slug.removesuffix(".git")
    if len(slug.split("/")) < 2:
        raise ValueError(f"repository slug '{repo_slug}' must include an owner and repository")

    if provider == "Github":
        return "github", f"https://github.com/{slug}"
    if provider == "Gitlab":
        return "gitlab", f"https://gitlab.com/{slug}"

    base_url = record.get("base_url") or "https://gitea.com"
    if not isinstance(base_url, str):
        raise ValueError("Gitea base_url must be a string")
    base = urlsplit(base_url if "://" in base_url else f"https://{base_url}")
    if base.scheme != "https" or not base.netloc:
        raise ValueError(f"invalid Gitea base_url '{base_url}'")
    return "gitea", f"https://{base.netloc}/{slug}"


def entry_from_record(record: dict[str, Any], trust: str) -> tuple[str, dict[str, Any]]:
    if record.get("install_type") != "Release":
        raise ValueError("only release-installed packages can be imported")

    provider, repo = repository_url(record)
    name = repository_name(record["repo_slug"])
    binary = record.get("name")
    if not isinstance(binary, str) or not is_safe_basename(binary):
        raise ValueError("installed package name is not registry-safe")

    entry: dict[str, Any] = {
        "name": name,
        "revision": 1,
        "repo": repo,
        "provider": provider,
        "desktop": record.get("icon_path") is not None,
        "trust": trust,
    }
    if binary != name:
        entry["binary"] = binary
    for source_key, target_key in (("match_pattern", "match"), ("exclude_pattern", "exclude")):
        patterns = record.get(source_key)
        if isinstance(patterns, list) and patterns:
            entry[target_key] = patterns
    return name, entry


def toml_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def render_entry(entry: dict[str, Any]) -> str:
    lines = [
        f'name = {toml_string(entry["name"])}',
        f'revision = {entry["revision"]}',
    ]
    if "binary" in entry:
        lines.append(f'binary = {toml_string(entry["binary"])}')
    lines.extend(
        [
            f'repo = {toml_string(entry["repo"])}',
            f'provider = {toml_string(entry["provider"])}',
            "",
            f'desktop = {str(entry["desktop"]).lower()}',
            f'trust = {toml_string(entry["trust"])}',
        ]
    )
    for key in ("match", "exclude"):
        if key in entry:
            lines.extend(
                [
                    "",
                    f"{key} = [",
                    *(f"    {toml_string(pattern)}," for pattern in entry[key]),
                    "]",
                ]
            )
    return "\n".join(lines) + "\n"


def write_atomic(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp")
    temporary.write_text(text, encoding="utf-8")
    temporary.replace(path)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        records = read_records(args.input)
        existing = load_registry(args.packages_dir)
    except (ValueError, RegistryValidationError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    existing_installed = {
        package.get("binary", name): name for name, package in existing.items()
    }
    pending_names: set[str] = set()
    pending_installed: dict[str, str] = {}
    generated: list[tuple[Path, dict[str, Any]]] = []
    skipped: list[str] = []
    errors: list[str] = []

    for index, record in enumerate(records):
        label = record.get("name", f"item {index}")
        try:
            name, entry = entry_from_record(record, args.trust)
        except (KeyError, ValueError) as error:
            skipped.append(f"{label}: {error}")
            continue

        if name in existing or name in pending_names:
            skipped.append(f"{label}: registry package '{name}' already exists")
            continue
        installed_name = entry.get("binary", name)
        conflict = existing_installed.get(installed_name) or pending_installed.get(installed_name)
        if conflict:
            errors.append(
                f"{label}: installed name '{installed_name}' conflicts with registry package '{conflict}'"
            )
            continue

        path = args.packages_dir / f"{name}.toml"
        entry_errors = validate_entry(path, entry)
        if entry_errors:
            errors.extend(entry_errors)
            continue
        pending_names.add(name)
        pending_installed[installed_name] = name
        generated.append((path, entry))

    for message in skipped:
        print(f"skip: {message}", file=sys.stderr)
    if errors:
        for message in errors:
            print(f"error: {message}", file=sys.stderr)
        return 1

    for path, entry in generated:
        if not args.dry_run:
            write_atomic(path, render_entry(entry))
        print(f"{'would create' if args.dry_run else 'created'} {path.relative_to(ROOT) if path.is_relative_to(ROOT) else path}")

    print(f"added {len(generated)} package(s); skipped {len(skipped)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
