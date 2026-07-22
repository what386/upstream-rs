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
        expected = COMMON.render_index(packages)
        actual = (ROOT / "registry" / "index.json").read_text(encoding="utf-8")
        self.assertEqual(actual, expected)

    def test_index_is_keyed_by_package_name(self) -> None:
        packages = COMMON.load_registry(ROOT / "registry" / "packages")
        rendered = json.loads(COMMON.render_index(packages))
        self.assertIn("upstream", rendered)
        self.assertNotIn("name", rendered["upstream"])
        self.assertEqual(rendered["upstream"]["provider"], "github")

    def test_invalid_entries_report_all_schema_errors(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            packages_dir = Path(directory)
            (packages_dir / "different.toml").write_text(
                '\n'.join(
                    [
                        'name = "Bad"',
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


if __name__ == "__main__":
    unittest.main()
