"""HTTP regressions exercised through the CLI and the shared Ruby test server."""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path
import unittest

from tests.framework.commands import run_upstream, run_upstream_json, run_upstream_result
from tests.framework.environment import FAKEHOME, reset_fakehome
from tests.framework.packages import package_from_list, package_path
from tests.framework.server import Server, platform_architecture


class HttpTests(unittest.TestCase):
    def setUp(self) -> None:
        reset_fakehome()
        self.server = Server()
        self.addCleanup(self.server.close)

    def route(self, path: str, **options: object) -> dict[str, object]:
        return {"path": path, **options}

    def json_route(self, path: str, body: object, **options: object) -> dict[str, object]:
        return self.route(path, body=json.dumps(body), content_type="application/json", **options)

    def probe(self, source: str, *args: str) -> dict[str, object]:
        return run_upstream_json("probe", source, *args)

    def install(self, source: str, *args: str) -> None:
        run_upstream("install", source, "--kind", "binary", "--desktop=false", "--trust", "none", "--yes", *args)

    def release(self, tag: str, name: str = "tool.bin", **options: object) -> dict[str, object]:
        return {
            "id": 1, "tag_name": tag, "name": tag, "draft": False,
            "prerelease": "rc" in tag, "published_at": "2026-02-01T00:00:00Z",
            "assets": [{"id": 1, "name": name, "size": 10,
                        "browser_download_url": self.server.url_for(name)}],
            **options,
        }

    def test_reinstall_options_dry_run_and_failed_resolution_preserve_install(self) -> None:
        api = "/api/v1/repos/owner/tool/releases"
        old = self.release("v1.0.0", "tool-old.bin")
        new = self.release("v2.0.0", "tool-new.bin")
        self.server.write_bytes("tool-old.bin", b"old binary")
        self.server.write_bytes("tool-new.bin", b"new binary")
        self.server.routes(self.json_route(f"{api}/tags/v1.0.0", old),
                           self.json_route(f"{api}/tags/v2.0.0", new))
        self.install("owner/tool", "--provider", "gitea", "--base-url", self.server.base_url,
                     "--tag", "v1.0.0", "--match-pattern", "old")
        original = package_from_list("tool")
        active = package_path(original)
        failed = run_upstream_result("reinstall", "tool", "--tag", "missing")
        self.assertNotEqual(failed.returncode, 0, failed.stdout)
        self.assertEqual(active.read_bytes(), b"old binary")
        self.assertEqual(package_from_list("tool"), original)
        options = ("--tag", "v2.0.0", "--kind", "binary", "--channel", "preview",
                   "--match-pattern", "new", "--exclude-pattern", "old", "--desktop=false", "--trust", "none")
        before = len(self.server.requests())
        run_upstream("reinstall", "tool", *options, "--dry-run")
        self.assertEqual(active.read_bytes(), b"old binary")
        self.assertEqual(package_from_list("tool"), original)
        self.assertFalse(any(r["path"] == "/artifacts/tool-new.bin" for r in self.server.requests()[before:]))
        run_upstream("reinstall", "tool", *options, "--yes")
        updated = package_from_list("tool")
        self.assertEqual(package_path(updated).read_bytes(), b"new binary")
        self.assertEqual(updated["release_tag"], "v2.0.0")
        self.assertEqual(updated["filetype"], "Binary")
        self.assertEqual(updated["channel"], "Preview")
        self.assertIn("new", str(updated["match_pattern"]))
        self.assertIn("old", str(updated["exclude_pattern"]))

    def test_explicit_gitea_url_and_semver_use_the_correct_endpoints(self) -> None:
        api = "/api/v1/repos/owner/tool/releases"
        self.server.write_bytes("tool.bin", b"new binary")
        self.server.routes(self.json_route(f"{api}/latest", self.release("v2.0.0")),
                           self.json_route(f"{api}/tags/v1.2.4", self.release("v1.2.4")))
        source = f"{self.server.base_url}/owner/tool/releases"
        self.install(source, "--provider", "gitea", "--semver", "1.2.4")
        package = package_from_list("tool")
        self.assertEqual(package["id"], "gitea:owner/tool")
        self.assertEqual(package["base_url"], self.server.base_url)
        self.assertEqual(package["release_tag"], "v1.2.4")
        paths = [r["path"] for r in self.server.requests()]
        self.assertEqual(paths[:2], [f"{api}/latest", f"{api}/tags/v1.2.4"])
        self.assertNotIn(api, paths)

    def test_update_check_uses_the_installed_preview_channel(self) -> None:
        api = "/api/v1/repos/owner/tool/releases"
        self.server.write_bytes("tool.bin", b"old binary")
        self.server.routes(self.json_route(f"{api}/tags/v1.0.0", self.release("v1.0.0")),
                           self.json_route(f"{api}/latest", self.release("v2.0.0")),
                           self.json_route(api, [self.release("v2.1.0-rc1")]))
        self.install("owner/tool", "--provider", "gitea", "--base-url", self.server.base_url,
                     "--tag", "v1.0.0", "--channel", "preview")
        checks = run_upstream_json("upgrade", "tool", "--check")
        self.assertEqual(checks[0]["state"], "update_available")
        self.assertIn("2.1.0", checks[0]["latest"])
        self.assertFalse(any(r["path"] == f"{api}/latest" for r in self.server.requests()))

    def test_download_parallel_ranges_and_ignored_range_fallback(self) -> None:
        run_upstream("config", "set", "download.low_threshold_mb=0", "download.high_threshold_mb=0",
                     "download.low_threads=2", "download.high_threads=4")
        body = bytes(index % 251 for index in range(4096))
        for mode in ("honor", "ignore"):
            with self.subTest(mode=mode):
                name = f"tool-{mode}.bin"
                self.server.write_bytes(name, body)
                self.server.routes(self.route(f"/artifacts/{name}", file=name, ranges=mode))
                before = len(self.server.requests())
                self.install(self.server.url_for(name), "--provider", "direct")
                package = package_from_list(name)
                self.assertEqual(package_path(package).read_bytes(), body)
                requests = [r for r in self.server.requests()[before:] if r["method"] == "GET"]
                ranges = sorted(r["headers"]["range"] for r in requests if "range" in r["headers"])
                self.assertTrue(ranges, requests)
                if mode == "ignore":
                    self.assertNotIn("range", requests[-1]["headers"])

    def test_download_logs_actual_bytes_and_redacts_source(self) -> None:
        run_upstream("config", "set", "logging.level=debug")
        body = b"x" * (128 * 1024)
        self.server.write_bytes("tool", body)
        source = self.server.base_url.replace("http://", "http://user:private-password@") + "/tool?token=private-query#private-fragment"
        release = self.release("v1.0.0")
        release["assets"][0]["browser_download_url"] = source
        self.server.routes(self.route("/tool", method="GET", file="tool", omit_length=True),
                           self.json_route("/api/v1/repos/owner/tool/releases/latest", release))
        self.install("owner/tool", "--provider", "gitea", "--base-url", self.server.base_url)
        log_path = FAKEHOME / ".upstream" / "upstream.log"
        log = log_path.read_text()
        events = [json.loads(line) for line in log.splitlines()]
        downloads = [e for e in events if e["target"].endswith("download_handler")]
        completed = [e for e in downloads if e["fields"]["message"] == "Download completed"]
        self.assertEqual(len(completed), 1)
        self.assertEqual(completed[0]["fields"]["bytes"], len(body))
        self.assertEqual(completed[0]["fields"]["source"], f"{self.server.base_url}/tool")
        for secret in ("private-password", "private-query", "private-fragment"):
            self.assertNotIn(secret, json.dumps(downloads))
        before = len(events)
        self.server.routes(self.route("/bad-tool.bin", method="HEAD", file="tool"),
                           self.route("/bad-tool.bin", method="GET", status=500))
        result = run_upstream_result("install", f"{self.server.base_url}/bad-tool.bin", "--kind", "binary", "--provider", "direct", "--trust", "none", "--yes")
        self.assertNotEqual(result.returncode, 0)
        events = [json.loads(line) for line in log_path.read_text().splitlines()][before:]
        self.assertTrue(any(e["fields"].get("message") == "Download started" for e in events))
        self.assertFalse(any(e["fields"].get("message") == "Download completed" for e in events))

    def content(self, version: str, name: str = "Example.AppImage", content_id: int = 2259793) -> dict[str, object]:
        return {"id": content_id, "name": "Example AppImage", "version": version,
                "changed": "2026-01-02T03:04:05+00:00", "created": "2020-01-01T00:00:00+00:00",
                "downloadlink1": self.server.url_for(f"Example-{version}.AppImage"), "downloadname1": name,
                "download_version1": version, "download_package_type1": "appimage",
                "download_package_arch1": "arm64" if platform_architecture() == "aarch64" else "x86-64"}

    def ocs(self, *items: object, **options: object) -> dict[str, object]:
        return {"status": "ok", "statuscode": 100, "data": list(items), **options}

    def test_appimagehub_search_filters_paginates_and_preserves_ids(self) -> None:
        first = self.content("v1.0.0")
        second = {**self.content("v1.0.0", content_id=2259794), "name": "Painting tool",
                  "summary": "Paint & draw ...", "description": "<b>Paint</b> &amp; draw\n pictures"}
        query = {"format": "json", "search": "paint & draw", "categories": "239x244", "pagesize": "10", "sortmode": "new"}
        self.server.routes(
            self.json_route("/ocs/v1/content/categories", self.ocs({"id": "239", "xdg_type": "bin"}, {"id": 244, "xdg_type": "bin"}, {"id": 999, "xdg_type": "wallpaper"})),
            self.json_route("/ocs/v1/content/data", self.ocs({"id": 10, "downloadlink1": "https://example.test/downloads/", "downloadname1": "downloads"}, first, totalitems=20), query={**query, "page": "0"}),
            self.json_route("/ocs/v1/content/data", self.ocs(first, second, totalitems=20), query={**query, "page": "1"}),
        )
        result = run_upstream_json("search", "paint & draw", "--provider", "appimagehub", "--base-url", f"{self.server.base_url}/ocs/v1/", "--limit", "2")
        results = result["results"]
        self.assertEqual([r["repo_slug"] for r in results], ["2259793", "2259794"])
        self.assertEqual(results[1]["display_name"], "Painting tool")
        self.assertEqual(results[1]["description"], "Paint & draw pictures")

    @unittest.skipUnless(os.name != "nt" and sys.platform != "darwin", "AppImage installation requires Linux")
    def test_appimagehub_archived_release_install_and_updates(self) -> None:
        api = "/ocs/v1/content/data/2259793"
        current = self.json_route(api, self.ocs(self.content("v1.1.0")))
        archived = self.json_route(f"{api}/archived", self.ocs(self.content("v1.0.0")))
        self.server.write_bytes("Example-v1.0.0.AppImage", b"fixture artifact bytes")
        self.server.routes(current, archived)
        run_upstream("install", "2259793", "--provider", "appimagehub", "--base-url", f"{self.server.base_url}/ocs/v1/",
                     "--tag", "v1.0.0", "--desktop=false", "--trust", "none", "--yes")
        package = package_from_list("appimagehub:2259793")
        self.assertEqual(Path(package["install_path"]).read_bytes(), b"fixture artifact bytes")
        self.assertEqual(package["release_tag"], "v1.0.0")
        self.assertEqual(package["provider"], "AppImageHub")
        checks = run_upstream_json("upgrade", "appimagehub:2259793", "--check")
        self.assertEqual(checks[0]["state"], "update_available")
        self.assertIn("1.1.0", checks[0]["latest"])


if __name__ == "__main__":
    unittest.main()
