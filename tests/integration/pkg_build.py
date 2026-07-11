#!/usr/bin/env python3
"""Build ripgrep from its pinned source tag and smoke-test the result."""

from __future__ import annotations

from framework.commands import run_upstream, run_upstream_json
from framework.environment import reset_fakehome
from framework.packages import assert_executable_version, package_from_list, package_version

REPO = "BurntSushi/ripgrep"
PACKAGE = "rg"
TAG = "15.1.0"


def main() -> None:
    reset_fakehome()
    run_upstream(
        "build",
        REPO,
        PACKAGE,
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
    main()
