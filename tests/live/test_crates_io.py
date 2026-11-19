#!/usr/bin/env python3
"""Check the real crates.io search API and sparse index without downloading sources."""

from __future__ import annotations

import unittest

from tests.framework.commands import run_upstream, run_upstream_json
from tests.framework.environment import reset_fakehome


class CratesIoLiveTests(unittest.TestCase):
    def test_search_and_preview_published_version(self) -> None:
        reset_fakehome()
        result = run_upstream_json("search", "ripgrep", "--provider", "crates-io", "--limit", "5")
        self.assertEqual(result["provider"], "crates-io")
        results = result["results"]
        self.assertTrue(results)
        self.assertLessEqual(len(results), 5)
        crate = next(item for item in results if item["repo_slug"] == "ripgrep")
        version = crate["metadata"]["version"]
        self.assertTrue(version)
        self.assertGreater(crate["metadata"]["downloads"], 0)

        # Each invocation creates a fresh client; the second exercises persistent
        # conditional caching against the real sparse index. Both are previews.
        for source in ["crates-io:ripgrep", "https://crates.io/crates/ripgrep"]:
            preview = run_upstream("build", source, "--semver", version, "--dry-run")
            self.assertIn("crates-io:ripgrep", preview.stdout)
            self.assertIn(version, preview.stdout)


if __name__ == "__main__":
    unittest.main()
