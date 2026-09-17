"""Hermetic package lifecycle tests using the local rollback server."""

from __future__ import annotations

import os
import unittest

from tests.framework.commands import run_upstream, run_upstream_json
from tests.framework.environment import reset_fakehome
from tests.framework.packages import assert_executable_version, package_from_list, package_path
from tests.framework.server import PACKAGE, Server, write_release_page, write_rollback_fixtures


class FixtureLifecycleTests(unittest.TestCase):
    def test_failed_upgrade_preserves_previous_package(self) -> None:
        reset_fakehome()
        server = Server(start=False)
        write_rollback_fixtures(server)
        server.start()
        try:
            run_upstream(
                "install",
                server.url_for("releases.html"),
                "--kind",
                "archive",
                "--yes",
                "--trust",
                "none",
            )
            package = package_from_list(PACKAGE)
            package_id = package["id"]
            self.assertIsInstance(package_id, str)
            self._assert_working(package)

            write_release_page(server, 2)
            result = run_upstream("upgrade", package_id, "--yes", "--trust", "none")
            self.assertIn("failed", result.stdout.lower())

            restored = run_upstream_json("info", package_id)
            self.assertEqual(
                restored["version"],
                {"major": 1, "minor": 0, "patch": 0, "is_prerelease": False},
            )
            self._assert_working(restored)
            self.assertFalse(any((path.name.endswith(".part") for path in (package_path(restored).parent).iterdir())))
        finally:
            server.close()

    @staticmethod
    def _assert_working(package: dict[str, object]) -> None:
        if os.name == "nt":
            assert package_path(package).is_file(), package
        else:
            assert_executable_version(package, "rollback-tool 1.0.0")


if __name__ == "__main__":
    unittest.main()
