"""Tests for registry validation and deterministic index generation."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[2]
REGISTRY_SCRIPTS = ROOT / "scripts" / "registry"
sys.path.insert(0, str(REGISTRY_SCRIPTS))
SPEC = importlib.util.spec_from_file_location("registry_common", REGISTRY_SCRIPTS / "common.py")
assert SPEC and SPEC.loader
COMMON = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = COMMON
SPEC.loader.exec_module(COMMON)

IMPORT_SPEC = importlib.util.spec_from_file_location(
    "registry_import_list", REGISTRY_SCRIPTS / "import_list.py"
)
assert IMPORT_SPEC and IMPORT_SPEC.loader
IMPORT_LIST = importlib.util.module_from_spec(IMPORT_SPEC)
sys.modules[IMPORT_SPEC.name] = IMPORT_LIST
IMPORT_SPEC.loader.exec_module(IMPORT_LIST)


class RegistryTests(unittest.TestCase):
    def test_repository_registry_is_valid_and_index_is_current(self) -> None:
        packages = COMMON.load_registry(ROOT / "registry" / "packages")
        readable = (ROOT / "registry" / "index.json").read_text(encoding="utf-8")
        minified = (ROOT / "registry" / "index.min.json").read_text(encoding="utf-8")
        self.assertEqual(readable, COMMON.render_index(packages))
        self.assertEqual(minified, COMMON.render_minified_index(packages))
        self.assertEqual(json.loads(minified), json.loads(readable))
        self.assertNotIn("\n", minified.rstrip("\n"))

    def test_index_is_keyed_by_package_name(self) -> None:
        packages = COMMON.load_registry(ROOT / "registry" / "packages")
        rendered = json.loads(COMMON.render_index(packages))
        self.assertEqual(rendered["version"], 1)
        self.assertIn("upstream", rendered["packages"])
        self.assertNotIn("name", rendered["packages"]["upstream"])
        self.assertEqual(rendered["packages"]["upstream"]["revision"], 1)
        self.assertEqual(rendered["packages"]["upstream"]["provider"], "github")

    def test_invalid_entries_report_all_schema_errors(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            packages_dir = Path(directory)
            (packages_dir / "different.toml").write_text(
                '\n'.join(
                    [
                        'name = "Bad"',
                        'revision = 0',
                        'repo = "http://github.com/owner/repo?ref=main"',
                        'provider = "unknown"',
                        'desktop = "false"',
                        'trust = "sometimes"',
                        'extra = true',
                    ]
                ),
                encoding="utf-8",
            )

            with self.assertRaises(COMMON.RegistryValidationError) as raised:
                COMMON.load_registry(packages_dir)

            message = str(raised.exception)
            self.assertIn("unknown fields: extra", message)
            self.assertIn("must match filename", message)
            self.assertIn("canonical HTTPS", message)
            self.assertIn("unsupported provider", message)
            self.assertIn("'desktop' must be a boolean", message)
            self.assertIn("unsupported trust mode", message)

    def test_duplicate_patterns_are_rejected_case_insensitively(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            packages_dir = Path(directory)
            (packages_dir / "tool.toml").write_text(
                '\n'.join(
                    [
                        'name = "tool"',
                        'revision = 1',
                        'repo = "https://github.com/owner/tool"',
                        'provider = "github"',
                        'desktop = false',
                        'trust = "checksum"',
                        'match = ["Linux", "linux"]',
                    ]
                ),
                encoding="utf-8",
            )

            with self.assertRaises(COMMON.RegistryValidationError) as raised:
                COMMON.load_registry(packages_dir)

            self.assertIn("duplicate pattern", str(raised.exception))

    def test_binary_is_indexed_and_installed_names_must_be_unique(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            packages_dir = Path(directory)
            for name, binary in (("ripgrep", "rg"), ("rg-tools", "rg")):
                (packages_dir / f"{name}.toml").write_text(
                    "\n".join(
                        [
                            f'name = "{name}"',
                            "revision = 1",
                            f'binary = "{binary}"',
                            f'repo = "https://github.com/owner/{name}"',
                            'provider = "github"',
                            "desktop = false",
                            'trust = "checksum"',
                        ]
                    ),
                    encoding="utf-8",
                )

            with self.assertRaises(COMMON.RegistryValidationError) as raised:
                COMMON.load_registry(packages_dir)

            self.assertIn("installed name 'rg' conflicts", str(raised.exception))

            (packages_dir / "rg-tools.toml").unlink()
            packages = COMMON.load_registry(packages_dir)
            self.assertEqual(packages["ripgrep"]["binary"], "rg")

    def test_binary_rejects_platform_extensions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            packages_dir = Path(directory)
            (packages_dir / "tool.toml").write_text(
                "\n".join(
                    [
                        'name = "tool"',
                        "revision = 1",
                        'binary = "tool.exe"',
                        'repo = "https://github.com/owner/tool"',
                        'provider = "github"',
                        "desktop = false",
                        'trust = "checksum"',
                    ]
                ),
                encoding="utf-8",
            )

            with self.assertRaises(COMMON.RegistryValidationError) as raised:
                COMMON.load_registry(packages_dir)

            self.assertIn("must not include a platform extension", str(raised.exception))

    def test_names_and_binaries_allow_spaces_and_uppercase(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            packages_dir = Path(directory)
            (packages_dir / "Audacity Editor.toml").write_text(
                "\n".join(
                    [
                        'name = "Audacity Editor"',
                        "revision = 1",
                        'binary = "Audacity App"',
                        'repo = "https://github.com/audacity/audacity"',
                        'provider = "github"',
                        "desktop = true",
                        'trust = "checksum"',
                    ]
                ),
                encoding="utf-8",
            )

            packages = COMMON.load_registry(packages_dir)

            self.assertEqual(packages["Audacity Editor"]["binary"], "Audacity App")

    def test_revision_changes_are_enforced(self) -> None:
        previous = {
            "unchanged": {"revision": 4, "repo": "https://example.com/unchanged"},
            "modified": {"revision": 2, "repo": "https://example.com/old"},
        }
        current = {
            "unchanged": {"revision": 5, "repo": "https://example.com/unchanged"},
            "modified": {"revision": 2, "repo": "https://example.com/new"},
            "new": {"revision": 3, "repo": "https://example.com/new"},
        }

        errors = COMMON.validate_revision_changes(previous, current)

        self.assertEqual(len(errors), 3)
        self.assertTrue(any("unchanged package 'unchanged'" in error for error in errors))
        self.assertTrue(any("modified package 'modified'" in error for error in errors))
        self.assertTrue(any("new package 'new'" in error for error in errors))

    def test_list_import_creates_registry_name_and_binary_alias(self) -> None:
        record = {
            "name": "rg",
            "repo_slug": "BurntSushi/ripgrep",
            "provider": "Github",
            "install_type": "Release",
            "base_url": None,
            "icon_path": None,
            "match_pattern": ["linux"],
            "exclude_pattern": ["debug"],
        }

        name, entry = IMPORT_LIST.entry_from_record(record, "best-effort")
        rendered = tomllib.loads(IMPORT_LIST.render_entry(entry))

        self.assertEqual(name, "ripgrep")
        self.assertEqual(rendered["name"], "ripgrep")
        self.assertEqual(rendered["binary"], "rg")
        self.assertEqual(rendered["repo"], "https://github.com/BurntSushi/ripgrep")
        self.assertEqual(rendered["match"], ["linux"])
        self.assertEqual(rendered["exclude"], ["debug"])

    def test_list_import_normalizes_name_and_preserves_binary(self) -> None:
        record = {
            "name": "Audacity App",
            "repo_slug": "audacity/Audacity-App",
            "provider": "Github",
            "install_type": "Release",
            "icon_path": "/icons/audacity.png",
        }

        name, entry = IMPORT_LIST.entry_from_record(record, "best-effort")

        self.assertEqual(name, "audacity-app")
        self.assertEqual(entry["binary"], "Audacity App")

    def test_list_import_writes_only_missing_release_packages(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            packages_dir = root / "packages"
            packages_dir.mkdir()
            (packages_dir / "existing.toml").write_text(
                "\n".join(
                    [
                        'name = "existing"',
                        "revision = 1",
                        'repo = "https://github.com/owner/existing"',
                        'provider = "github"',
                        "desktop = false",
                        'trust = "checksum"',
                    ]
                ),
                encoding="utf-8",
            )
            input_path = root / "packages.json"
            input_path.write_text(
                json.dumps(
                    [
                        {
                            "name": "tool",
                            "repo_slug": "owner/tool",
                            "provider": "Github",
                            "install_type": "Release",
                            "icon_path": "/icons/tool.png",
                        },
                        {
                            "name": "source-tool",
                            "repo_slug": "owner/source-tool",
                            "provider": "Github",
                            "install_type": "Build",
                        },
                    ]
                ),
                encoding="utf-8",
            )

            result = IMPORT_LIST.main(
                [str(input_path), "--packages-dir", str(packages_dir)]
            )

            self.assertEqual(result, 0)
            generated = tomllib.loads(
                (packages_dir / "tool.toml").read_text(encoding="utf-8")
            )
            self.assertTrue(generated["desktop"])
            self.assertEqual(generated["trust"], "best-effort")
            self.assertFalse((packages_dir / "source-tool.toml").exists())

if __name__ == "__main__":
    unittest.main()
