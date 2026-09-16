#!/usr/bin/env python3
"""Remove ripgrep from tests/fakehome and verify its state is gone."""

from __future__ import annotations

import unittest

from tests.framework.commands import run_upstream, run_upstream_json, run_upstream_result
from tests.framework.environment import reset_fakehome
from tests.framework.packages import install_package, package_from_list, package_path


PACKAGE = "ripgrep"


class LiveRemoveTests(unittest.TestCase):
    def test_remove_package(self) -> None:
        reset_fakehome()
        install_package("BurntSushi/ripgrep", PACKAGE, "15.1.0")
        package = package_from_list(PACKAGE)
        executable = package_path(package)
        assert executable.is_file(), executable

        run_upstream("remove", PACKAGE, "--yes", "--purge")

        packages = run_upstream_json("list")
        assert isinstance(packages, list), packages
        assert all(item.get("id") != package["id"] for item in packages), packages
        assert not executable.exists(), executable

        info = run_upstream_result("info", PACKAGE, "--json")
        assert info.returncode != 0, f"package {PACKAGE!r} is still available through info"

        print(f"removed {PACKAGE} and verified its metadata and executable are gone")


if __name__ == "__main__":
    unittest.main()
