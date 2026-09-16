#!/usr/bin/env python3
"""Build ripgrep from its pinned source tag and smoke-test the result."""

from __future__ import annotations

import unittest

from tests.framework.commands import run_upstream, run_upstream_json
from tests.framework.environment import reset_fakehome
from tests.framework.packages import assert_executable_version, package_from_list, package_version

REPO = "BurntSushi/ripgrep"
PACKAGE = "ripgrep"
TAG = "15.1.0"


class LiveBuildTests(unittest.TestCase):
    def test_build_pinned_release(self) -> None:
        reset_fakehome()
        run_upstream(
            "build",
            REPO,
            "--tag",
            TAG,
            "--build-profile",
            "rust",
            "--yes",
        )

        package = package_from_list(PACKAGE)
        assert package["repo_slug"] == REPO, package
        assert package["install_type"] == "Build", package
        assert package_version(package) == (15, 1, 0), package
        assert_executable_version(package, "ripgrep 15.1.0")
        assert run_upstream_json("info", PACKAGE) == package

        print(f"built {PACKAGE} from {REPO}@{TAG}")


if __name__ == "__main__":
    unittest.main()
