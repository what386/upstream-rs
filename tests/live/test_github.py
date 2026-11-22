#!/usr/bin/env python3
"""Check release discovery and asset downloads against GitHub itself."""

from __future__ import annotations

import unittest

from tests.framework.commands import run_upstream, run_upstream_json
from tests.framework.environment import reset_fakehome
from tests.framework.packages import assert_executable_version, package_from_list


REPO = "cli/cli"


class GithubLiveTests(unittest.TestCase):
    def test_install_release_asset(self) -> None:
        # Real downloads catch API/content-negotiation/redirect changes that a
        # fixture implementing our assumptions about GitHub cannot detect.
        for threads in (1, 4):
            with self.subTest(threads=threads):
                reset_fakehome()
                run_upstream(
                    "config", "set", "download.low_threshold_mb=0",
                    "download.high_threshold_mb=0", f"download.low_threads={threads}",
                    f"download.high_threads={threads}",
                )
                run_upstream(
                    "install", REPO, "--provider", "github", "--kind", "archive",
                    "--desktop=false", "--trust", "none", "--yes",
                )
                package = package_from_list(f"github:{REPO}")
                assert_executable_version(package, "gh version ")

    def test_probe_release(self) -> None:
        reset_fakehome()

        result = run_upstream_json(
            "probe",
            REPO,
            "--provider",
            "github",
            "--limit",
            "1",
        )

        self.assertIsInstance(result, dict)
        source = result["source"]
        self.assertEqual(source["repo_slug"], REPO)
        self.assertEqual(source["provider"], "github")

        releases = result["releases"]
        self.assertEqual(len(releases), 1)
        release = releases[0]
        self.assertGreater(release["assets_count"], 0)

        candidates = release["candidates"]
        self.assertTrue(candidates)
        self.assertTrue(
            any(
                candidate["name"].startswith("gh")
                and candidate["download_url"].startswith("https://")
                for candidate in candidates
            ),
            candidates,
        )


if __name__ == "__main__":
    unittest.main()
