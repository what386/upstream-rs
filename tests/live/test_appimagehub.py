#!/usr/bin/env python3
"""Probe a real AppImageHub listing through the openDesktop OCS API."""

from __future__ import annotations

import unittest

from tests.framework.commands import run_upstream_json
from tests.framework.environment import reset_fakehome


CONTENT_ID = "2259793"  # Krita AppImage
PRODUCT_URL = f"https://www.appimagehub.com/p/{CONTENT_ID}"


class AppImageHubLiveTests(unittest.TestCase):
    def test_search_catalog(self) -> None:
        reset_fakehome()

        result = run_upstream_json("search", "krita", "--provider", "appimagehub", "--limit", "5")

        self.assertEqual(result["provider"], "appimagehub")
        results = result["results"]
        self.assertTrue(results)
        self.assertLessEqual(len(results), 5)
        self.assertEqual(len({item["repo_slug"] for item in results}), len(results))
        self.assertTrue(any("krita" in item["display_name"].lower() for item in results), results)
        for item in results:
            self.assertTrue(item["repo_slug"].isdigit(), item)
            self.assertTrue(item["display_name"], item)
            self.assertEqual(item["stars"], 0)
            self.assertEqual(item["language"], "")

    def test_probe_release(self) -> None:
        reset_fakehome()

        for source_args in (
            (PRODUCT_URL,),
            (CONTENT_ID, "--provider", "appimagehub"),
        ):
            with self.subTest(source=source_args):
                result = run_upstream_json(
                    "probe",
                    *source_args,
                    "--include-incompatible",
                )

                self.assertIsInstance(result, dict)
                source = result["source"]
                self.assertEqual(source["input"], source_args[0])
                self.assertEqual(source["repo_slug"], CONTENT_ID)
                self.assertEqual(source["provider"], "appimagehub")

                releases = result["releases"]
                self.assertEqual(len(releases), 1)
                release = releases[0]
                self.assertTrue(release["tag"])
                self.assertGreater(release["assets_count"], 0)

                candidates = release["candidates"]
                self.assertTrue(candidates)
                appimages = [
                    candidate
                    for candidate in candidates
                    if candidate["filetype"] == "AppImage"
                    and candidate["name"].lower().startswith("krita")
                ]
                self.assertTrue(appimages, candidates)
                for candidate in appimages:
                    self.assertTrue(candidate["download_url"].startswith("https://"))
                    self.assertGreater(candidate["size"], 0)
                    self.assertEqual(candidate["target_os"], "Linux")
                    self.assertIsNotNone(candidate["target_arch"])


if __name__ == "__main__":
    unittest.main()
