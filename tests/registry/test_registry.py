"""Tests for registry validation and deterministic index generation."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
REGISTRY_SCRIPTS = ROOT / "scripts" / "registry"
sys.path.insert(0, str(REGISTRY_SCRIPTS))
SPEC = importlib.util.spec_from_file_location("registry_common", REGISTRY_SCRIPTS / "common.py")
assert SPEC and SPEC.loader
COMMON = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = COMMON
SPEC.loader.exec_module(COMMON)


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

if __name__ == "__main__":
    unittest.main()
