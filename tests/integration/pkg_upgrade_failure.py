#!/usr/bin/env python3
"""Install a corrupt update and verify the previous working package is restored."""

from __future__ import annotations

from framework.commands import run_upstream, run_upstream_json
from framework.environment import reset_fakehome
import os

from framework.packages import assert_executable_version, package_from_list, package_path
from framework.rollback_server import PACKAGE, RollbackServer


def main() -> None:
    reset_fakehome()
    server = RollbackServer()
    try:
        run_upstream(
            "install",
            server.url,
            "--kind",
            "archive",
            "--yes",
            "--trust",
            "none",
        )
        old = package_from_list(PACKAGE)
        assert_working(old)

        server.publish_update()
        result = run_upstream("upgrade", PACKAGE, "--yes", "--trust", "none")
        assert "failed" in result.stdout.lower(), result.stdout

        restored = run_upstream_json("info", PACKAGE)
        assert restored["version"] == {
            "major": 1,
            "minor": 0,
            "patch": 0,
            "is_prerelease": False,
        }, restored
        assert_working(restored)
    finally:
        server.close()

    print("failed update restored the working package")


def assert_working(package: dict[str, object]) -> None:
    if os.name == "nt":
        assert package_path(package).is_file(), package
    else:
        assert_executable_version(package, "rollback-tool 1.0.0")


if __name__ == "__main__":
    main()
