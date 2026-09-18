#!/usr/bin/env python3
"""Probe a real Gitea release through the Gitea provider."""

from __future__ import annotations

import unittest

from tests.framework.commands import run_upstream_json
from tests.framework.environment import reset_fakehome


REPO = "gitea/tea"


class GiteaLiveTests(unittest.TestCase):
    def test_probe_release(self) -> None:
        reset_fakehome()

        result = run_upstream_json(
            "probe",
            REPO,
            "--provider",
            "gitea",
            "--limit",
            "1",
            "--include-incompatible",
        )

        self.assertIsInstance(result, dict)
        source = result["source"]
        self.assertEqual(source["repo_slug"], REPO)
        self.assertEqual(source["provider"], "gitea")

        releases = result["releases"]
        self.assertEqual(len(releases), 1)
        release = releases[0]
        self.assertGreater(release["assets_count"], 0)

        candidates = release["candidates"]
        self.assertTrue(candidates)
        self.assertTrue(
            any(
                candidate["name"].startswith("tea-")
                and candidate["download_url"].startswith("https://")
                for candidate in candidates
            ),
            candidates,
        )


if __name__ == "__main__":
    unittest.main()
