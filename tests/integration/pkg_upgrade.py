#!/usr/bin/env python3
"""Upgrade a pinned real release and verify metadata and executable behavior."""

from __future__ import annotations

from framework.commands import run_upstream, run_upstream_json
from framework.environment import reset_fakehome
from framework.packages import (
    assert_executable_version,
    install_package,
    package_version,
    release_version,
)

REPO = "BurntSushi/ripgrep"
PACKAGE = "rg"
OLD_TAG = "14.1.0"


def main() -> None:
    reset_fakehome()
    old_package = install_package(REPO, PACKAGE, OLD_TAG)
    assert package_version(old_package) == (14, 1, 0), old_package
    assert_executable_version(old_package, "ripgrep 14.1.0")

    check = run_upstream_json("upgrade", PACKAGE, "--check")
    assert isinstance(check, list), check
    row = next((item for item in check if item.get("name") == PACKAGE), None)
    assert row is not None, check
    assert row["state"] == "update_available", row
    assert row["current"] == "14.1.0", row
    latest_tag = row["latest"]
    assert isinstance(latest_tag, str), row
    expected_version = release_version(latest_tag)

    run_upstream("upgrade", PACKAGE, "--yes", "--trust", "none")
    upgraded = run_upstream_json("info", PACKAGE)
    assert package_version(upgraded) == expected_version, upgraded
    assert_executable_version(upgraded, f"ripgrep {latest_tag}")

    print(f"upgraded {PACKAGE} from {OLD_TAG} to {latest_tag}")


if __name__ == "__main__":
    main()
