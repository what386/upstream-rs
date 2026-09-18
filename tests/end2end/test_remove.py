#!/usr/bin/env python3
"""Remove a locally served package and verify its state is gone."""

from __future__ import annotations

import unittest

from tests.framework.commands import run_upstream, run_upstream_json, run_upstream_result
from tests.framework.environment import reset_fakehome
from tests.framework.packages import package_from_list, package_path
from tests.framework.server import start_rollback_server


PACKAGE = "rollback-tool"


class EndToEndRemoveTests(unittest.TestCase):
    def test_remove_package(self) -> None:
        reset_fakehome()
        server = start_rollback_server()
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
            executable = package_path(package)
            assert executable.is_file(), executable

            run_upstream("remove", package_id, "--yes", "--purge")

            packages = run_upstream_json("list")
            assert isinstance(packages, list), packages
            assert all(item.get("id") != package_id for item in packages), packages
            assert not executable.exists(), executable

            info = run_upstream_result("info", package_id, "--json")
            assert info.returncode != 0, f"package {PACKAGE!r} is still available through info"
        finally:
            server.close()

        print(f"removed {PACKAGE} and verified its metadata and executable are gone")


if __name__ == "__main__":
    unittest.main()
