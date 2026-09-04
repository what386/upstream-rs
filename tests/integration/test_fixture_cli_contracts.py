"""Hermetic state and dry-run contracts for the CLI."""

from __future__ import annotations

import unittest

from tests.framework.commands import run_upstream, run_upstream_json
from tests.framework.environment import FAKEHOME, reset_fakehome


class FixtureCliContractTests(unittest.TestCase):
    def test_list_json_and_cache_clean_dry_run_are_state_safe(self) -> None:
        reset_fakehome()
        cache = FAKEHOME / ".upstream" / "cache" / "docs"
        cache.mkdir(parents=True, exist_ok=True)
        marker = cache / "fixture"
        marker.write_text("cached", encoding="utf-8")

        packages = run_upstream_json("list")
        self.assertIsInstance(packages, list)
        before = marker.read_text(encoding="utf-8")

        result = run_upstream("cache", "clean", "docs", "--dry-run")

        self.assertIn("docs", result.stdout.lower())
        self.assertTrue(marker.is_file())
        self.assertEqual(marker.read_text(encoding="utf-8"), before)


if __name__ == "__main__":
    unittest.main()
