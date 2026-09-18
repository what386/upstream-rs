#!/usr/bin/env python3
"""Probe a real HTML download page through the HTTP scraper provider."""

from __future__ import annotations

import unittest

from tests.framework.commands import run_upstream_json
from tests.framework.environment import reset_fakehome


DOWNLOAD_PAGE = "https://ziglang.org/download/"


class HttpLiveTests(unittest.TestCase):
    def test_probe_zig_download_page(self) -> None:
        reset_fakehome()

        result = run_upstream_json(
            "probe",
            DOWNLOAD_PAGE,
            "--provider",
            "scraper",
            "--limit",
            "1",
        )

        self.assertIsInstance(result, dict)
        source = result["source"]
        self.assertEqual(source["input"], DOWNLOAD_PAGE)
        self.assertEqual(source["repo_slug"], DOWNLOAD_PAGE)
        self.assertEqual(source["provider"], "scraper")

        releases = result["releases"]
        self.assertEqual(len(releases), 1)
        release = releases[0]
        self.assertGreater(release["assets_count"], 0)

        candidates = release["candidates"]
        self.assertTrue(candidates)
        zig_downloads = []
        for candidate in candidates:
            self.assertTrue(
                candidate["download_url"].startswith(("http://", "https://")),
                candidate,
            )
            if candidate["name"].startswith("zig"):
                zig_downloads.append(candidate)

        self.assertTrue(zig_downloads, candidates)


if __name__ == "__main__":
    unittest.main()
