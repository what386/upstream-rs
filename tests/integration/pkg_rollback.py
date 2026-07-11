#!/usr/bin/env python3
"""Upgrade ripgrep, restore its previous version, and verify rollback state."""

from __future__ import annotations

from framework.commands import run_upstream, run_upstream_json
from framework.environment import reset_fakehome
from framework.packages import assert_executable_version, install_package, package_version

REPO = "BurntSushi/ripgrep"
PACKAGE = "rg"
OLD_TAG = "14.1.0"
NEW_TAG = "15.1.0"


def main() -> None:
    reset_fakehome()
    old_package = install_package(REPO, PACKAGE, OLD_TAG)
    assert package_version(old_package) == (14, 1, 0), old_package

    run_upstream("upgrade", PACKAGE, "--yes", "--trust", "none")
    upgraded = run_upstream_json("info", PACKAGE)
    assert package_version(upgraded) == (15, 1, 0), upgraded
    assert_executable_version(upgraded, "ripgrep 15.1.0")

    rollback_list = run_upstream("rollback", "--list").stdout
    assert PACKAGE in rollback_list, rollback_list
    assert "14.1.0" in rollback_list, rollback_list

    run_upstream("rollback", PACKAGE, "--yes")
    restored = run_upstream_json("info", PACKAGE)
    assert package_version(restored) == (14, 1, 0), restored
    assert_executable_version(restored, "ripgrep 14.1.0")

    run_upstream("rollback", "--prune", PACKAGE, "--yes")
    assert PACKAGE not in run_upstream("rollback", "--list").stdout

    print(f"upgraded to {NEW_TAG}, restored {OLD_TAG}, and pruned rollback data")


if __name__ == "__main__":
    main()
