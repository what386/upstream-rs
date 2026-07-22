"""Validation and deterministic index generation for the package registry."""

from __future__ import annotations

import json
from pathlib import Path
import tomllib
from typing import Any
import unicodedata
from urllib.parse import urlsplit


REQUIRED_FIELDS = {"name", "revision", "desktop", "trust", "install"}
OPTIONAL_FIELDS = {"binary", "match", "exclude"}
ALLOWED_FIELDS = REQUIRED_FIELDS | OPTIONAL_FIELDS
PROVIDERS = {"github", "gitlab", "gitea"}
TRUST_MODES = {"none", "best-effort", "checksum", "signature", "all"}
INSTALL_TYPES = {"release", "build", "http"}
BUILD_PROFILES = {"rust", "dotnet", "go", "zig", "cmake"}
FILETYPES = {
    "appimage",
    "mac-app",
    "mac-dmg",
    "archive",
    "compressed",
    "binary",
    "win-exe",
    "auto",
}


class RegistryValidationError(Exception):
    """Raised when one or more registry entries are invalid."""

    def __init__(self, errors: list[str]) -> None:
        self.errors = errors
        super().__init__("\n".join(errors))


def is_safe_basename(value: str) -> bool:
    """Return whether a registry or installed name is a single safe filename."""
    return (
        bool(value)
        and value not in {".", ".."}
        and value == value.strip()
        and "/" not in value
        and "\\" not in value
        and not any(unicodedata.category(character).startswith("C") for character in value)
    )


def load_registry(
    packages_dir: Path, *, allow_empty: bool = False
) -> dict[str, dict[str, Any]]:
    """Load and validate every TOML entry, returning metadata keyed by name."""
    errors: list[str] = []
    packages: dict[str, dict[str, Any]] = {}

    if not packages_dir.is_dir():
        raise RegistryValidationError(
            [f"registry packages directory does not exist: {packages_dir}"]
        )

    paths = sorted(packages_dir.glob("*.toml"), key=lambda path: path.name)
    if not paths and not allow_empty:
        raise RegistryValidationError([f"no package TOML files found in {packages_dir}"])

    for path in paths:
        try:
            raw = tomllib.loads(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
            errors.append(f"{path}: invalid TOML: {error}")
            continue

        entry_errors = validate_entry(path, raw)
        errors.extend(entry_errors)
        if entry_errors:
            continue

        name = raw["name"]
        if name in packages:
            errors.append(f"{path}: duplicate package name '{name}'")
            continue

        packages[name] = {
            key: raw[key]
            for key in (
                "revision",
                "binary",
                "desktop",
                "trust",
                "match",
                "exclude",
                "install",
            )
            if key in raw
        }

    installed_names: dict[str, str] = {}
    for name, package in packages.items():
        installed_name = package.get("binary", name)
        if previous_name := installed_names.get(installed_name):
            errors.append(
                f"{packages_dir / f'{name}.toml'}: installed name '{installed_name}' "
                f"conflicts with package '{previous_name}'"
            )
        else:
            installed_names[installed_name] = name

    if errors:
        raise RegistryValidationError(errors)
    return dict(sorted(packages.items()))


def validate_entry(path: Path, entry: object) -> list[str]:
    """Return all validation errors for one decoded TOML document."""
    if not isinstance(entry, dict):
        return [f"{path}: package entry must be a TOML table"]

    errors: list[str] = []
    keys = set(entry)
    missing = sorted(REQUIRED_FIELDS - keys)
    unknown = sorted(keys - ALLOWED_FIELDS)
    if missing:
        errors.append(f"{path}: missing required fields: {', '.join(missing)}")
    if unknown:
        errors.append(f"{path}: unknown fields: {', '.join(unknown)}")

    name = entry.get("name")
    if not isinstance(name, str):
        errors.append(f"{path}: 'name' must be a string")
    else:
        if not is_safe_basename(name):
            errors.append(f"{path}: 'name' must be a safe filename without path separators")
        if name != path.stem:
            errors.append(f"{path}: package name '{name}' must match filename '{path.stem}'")

    revision = entry.get("revision")
    if isinstance(revision, bool) or not isinstance(revision, int):
        errors.append(f"{path}: 'revision' must be an integer")
    elif revision < 1:
        errors.append(f"{path}: 'revision' must be at least 1")

    if "binary" in entry:
        binary = entry["binary"]
        if not isinstance(binary, str):
            errors.append(f"{path}: 'binary' must be a string")
        elif not is_safe_basename(binary):
            errors.append(f"{path}: 'binary' must be a safe basename without path separators")
        elif binary.endswith(".exe"):
            errors.append(f"{path}: 'binary' must not include a platform extension")

    desktop = entry.get("desktop")
    if not isinstance(desktop, bool):
        errors.append(f"{path}: 'desktop' must be a boolean")

    trust = entry.get("trust")
    if not isinstance(trust, str):
        errors.append(f"{path}: 'trust' must be a string")
    elif trust not in TRUST_MODES:
        errors.append(
            f"{path}: unsupported trust mode '{trust}'; expected one of: {', '.join(sorted(TRUST_MODES))}"
        )

    for key in ("match", "exclude"):
        if key in entry:
            errors.extend(validate_patterns(path, key, entry[key]))

    errors.extend(validate_install(path, entry.get("install"), keys))

    return errors


def validate_install(path: Path, install: object, entry_keys: set[str]) -> list[str]:
    if not isinstance(install, dict):
        return [f"{path}: 'install' must be a table"]

    install_type = install.get("type")
    if not isinstance(install_type, str) or install_type not in INSTALL_TYPES:
        return [
            f"{path}: 'install.type' must be one of: {', '.join(sorted(INSTALL_TYPES))}"
        ]

    allowed = {
        "release": {"type", "repo", "provider"},
        "build": {"type", "repo", "provider", "profile", "branch"},
        "http": {"type", "url", "filetype"},
    }[install_type]
    errors: list[str] = []
    unknown = sorted(set(install) - allowed)
    if unknown:
        errors.append(
            f"{path}: unknown fields for {install_type} install: {', '.join(unknown)}"
        )

    if install_type in {"release", "build"}:
        repo = install.get("repo")
        provider = install.get("provider")
        if not isinstance(repo, str):
            errors.append(f"{path}: 'install.repo' must be a string")
        else:
            errors.extend(validate_repo(path, repo, provider, key="install.repo"))
        if not isinstance(provider, str):
            errors.append(f"{path}: 'install.provider' must be a string")
        elif provider not in PROVIDERS:
            errors.append(
                f"{path}: unsupported install provider '{provider}'; expected one of: {', '.join(sorted(PROVIDERS))}"
            )

    if install_type == "build":
        profile = install.get("profile")
        if profile is not None and (
            not isinstance(profile, str) or profile not in BUILD_PROFILES
        ):
            errors.append(
                f"{path}: 'install.profile' must be one of: {', '.join(sorted(BUILD_PROFILES))}"
            )
        branch = install.get("branch")
        if branch is not None and (
            not isinstance(branch, str) or not branch or branch != branch.strip()
        ):
            errors.append(f"{path}: 'install.branch' must be a non-empty trimmed string")
        for key in ("match", "exclude"):
            if key in entry_keys:
                errors.append(f"{path}: '{key}' is not supported for build installs")

    if install_type == "http":
        url = install.get("url")
        if not isinstance(url, str):
            errors.append(f"{path}: 'install.url' must be a string")
        else:
            errors.extend(validate_http_url(path, url))
        filetype = install.get("filetype", "auto")
        if not isinstance(filetype, str) or filetype not in FILETYPES:
            errors.append(
                f"{path}: 'install.filetype' must be one of: {', '.join(sorted(FILETYPES))}"
            )
        for key in ("match", "exclude"):
            if key in entry_keys:
                errors.append(f"{path}: '{key}' is not supported for http installs")

    return errors


def validate_repo(
    path: Path, repo: str, provider: object, *, key: str = "repo"
) -> list[str]:
    errors: list[str] = []
    parsed = urlsplit(repo)
    if (
        parsed.scheme != "https"
        or not parsed.hostname
        or not parsed.path.strip("/")
        or parsed.username
        or parsed.password
        or parsed.query
        or parsed.fragment
    ):
        errors.append(
            f"{path}: '{key}' must be a canonical HTTPS repository URL without credentials, query, or fragment"
        )
        return errors

    hostname = parsed.hostname.lower()
    expected_hosts = {"github": "github.com", "gitlab": "gitlab.com"}
    expected = expected_hosts.get(provider)
    if expected and hostname != expected:
        errors.append(
            f"{path}: provider '{provider}' requires a repository hosted on {expected}"
        )
    return errors


def validate_http_url(path: Path, url: str) -> list[str]:
    parsed = urlsplit(url)
    if (
        parsed.scheme != "https"
        or not parsed.hostname
        or not parsed.path.strip("/")
        or parsed.username
        or parsed.password
        or parsed.fragment
    ):
        return [
            f"{path}: 'install.url' must be an HTTPS download URL without credentials or a fragment"
        ]
    return []


def validate_patterns(path: Path, key: str, value: object) -> list[str]:
    if not isinstance(value, list):
        return [f"{path}: '{key}' must be an array of strings"]
    if not value:
        return [f"{path}: '{key}' must not be empty; omit it when no override is needed"]

    errors: list[str] = []
    seen: set[str] = set()
    for index, pattern in enumerate(value):
        if not isinstance(pattern, str):
            errors.append(f"{path}: '{key}[{index}]' must be a string")
            continue
        if not pattern or pattern != pattern.strip():
            errors.append(f"{path}: '{key}[{index}]' must be a non-empty trimmed string")
            continue
        normalized = pattern.casefold()
        if normalized in seen:
            errors.append(f"{path}: '{key}' contains duplicate pattern '{pattern}'")
        seen.add(normalized)
    return errors


def render_index(packages: dict[str, dict[str, Any]]) -> str:
    """Serialize the versioned name-to-metadata mapping deterministically."""
    index = {"version": 1, "packages": packages}
    return json.dumps(index, indent=2, ensure_ascii=False) + "\n"


def render_minified_index(packages: dict[str, dict[str, Any]]) -> str:
    """Serialize the versioned index without insignificant whitespace."""
    index = {"version": 1, "packages": packages}
    return json.dumps(index, separators=(",", ":"), ensure_ascii=False) + "\n"


def validate_revision_changes(
    previous: dict[str, dict[str, Any]], current: dict[str, dict[str, Any]]
) -> list[str]:
    """Validate monotonic revisions for new and modified package metadata."""
    errors: list[str] = []
    for name, current_entry in sorted(current.items()):
        current_revision = current_entry.get("revision")
        previous_entry = previous.get(name)
        if previous_entry is None:
            if current_revision != 1:
                errors.append(
                    f"new package '{name}' must start at revision 1, got {current_revision}"
                )
            continue

        if "revision" not in previous_entry:
            if current_revision != 1:
                errors.append(
                    f"package '{name}' must initialize revision at 1, "
                    f"got {current_revision}"
                )
            continue

        previous_revision = previous_entry["revision"]
        previous_metadata = {
            key: value for key, value in previous_entry.items() if key != "revision"
        }
        current_metadata = {
            key: value for key, value in current_entry.items() if key != "revision"
        }
        if current_metadata != previous_metadata:
            expected = previous_revision + 1
            if current_revision != expected:
                errors.append(
                    f"modified package '{name}' must increment revision from "
                    f"{previous_revision} to {expected}, got {current_revision}"
                )
        elif current_revision != previous_revision:
            errors.append(
                f"unchanged package '{name}' must keep revision {previous_revision}, "
                f"got {current_revision}"
            )
    return errors


def write_index(packages_dir: Path, output: Path, *, minified: bool = False) -> None:
    packages = load_registry(packages_dir)
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name(f".{output.name}.tmp")
    render = render_minified_index if minified else render_index
    temporary.write_text(render(packages), encoding="utf-8")
    temporary.replace(output)
