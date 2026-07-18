#!/usr/bin/env python3
"""Exercise mutating commands that update local upstream state."""

from __future__ import annotations

from framework.commands import run_upstream, run_upstream_json
from framework.environment import FAKEHOME, reset_fakehome
from framework.packages import install_package, package_from_list


REPO = "BurntSushi/ripgrep"
PACKAGE = "rg"
RENAMED_PACKAGE = "ripgrep"
TAG = "15.1.0"


def main() -> None:
    reset_fakehome()

    # Config and auth updates persist through their respective read paths, and
    # --yes makes the reset operations suitable for unattended integration runs.
    run_upstream("config", "set", "download.low_threads=3")
    config = run_upstream("config", "get", "download.low_threads").stdout
    assert "download.low_threads" in config and "3" in config, config
    run_upstream("--yes", "config", "reset")
    config = run_upstream("config", "get", "download.low_threads").stdout
    assert "download.low_threads" in config and "2" in config, config

    run_upstream("auth", "set", "github.api_token=test-token")
    auth = run_upstream("auth", "get", "github.api_token").stdout
    assert "github.api_token" in auth and "test-token" in auth, auth
    run_upstream("--yes", "auth", "reset")
    auth = run_upstream("auth", "list").stdout
    assert "test-token" not in auth, auth

    run_upstream("hooks", "init")
    assert (FAKEHOME / ".upstream" / "generated" / "paths.sh").is_file()
    run_upstream("hooks", "check")
    run_upstream("hooks", "clean")

    # Package metadata mutations do not reinstall the artifact.
    package = install_package(REPO, PACKAGE, TAG)
    assert package["name"] == PACKAGE, package
    run_upstream("package", "pin", PACKAGE)
    assert package_from_list(PACKAGE)["is_pinned"] is True
    run_upstream("package", "unpin", PACKAGE)
    assert package_from_list(PACKAGE)["is_pinned"] is False

    run_upstream("package", "rename", PACKAGE, RENAMED_PACKAGE)
    assert package_from_list(RENAMED_PACKAGE)["name"] == RENAMED_PACKAGE
    assert not any(
        item.get("name") == PACKAGE for item in run_upstream_json("list")
    )

    run_upstream("package", "rename", RENAMED_PACKAGE, PACKAGE)
    assert package_from_list(PACKAGE)["name"] == PACKAGE

    print("config, auth, pin/unpin, and package rename mutations passed")


if __name__ == "__main__":
    main()
