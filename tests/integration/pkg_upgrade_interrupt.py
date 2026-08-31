#!/usr/bin/env python3
"""Interrupt an upgrade during download and verify the old install survives."""

from __future__ import annotations

import signal
import os

from framework.commands import run_upstream, run_upstream_json, start_upstream
from framework.environment import reset_fakehome
from framework.packages import assert_executable_version, package_from_list, package_path
from framework.rollback_server import PACKAGE, RollbackServer


def main() -> None:
    reset_fakehome()
    server = RollbackServer(throttle_update=True)
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
        process = start_upstream("upgrade", PACKAGE, "--yes", "--trust", "none")
        server.wait_for_update_request()
        process.send_signal(signal.CTRL_C_EVENT if os.name == "nt" else signal.SIGINT)
        stdout, stderr = process.communicate(timeout=30)
        assert process.returncode == 130, (process.returncode, stdout, stderr)

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

    print("interrupted upgrade restored the working package")


def assert_working(package: dict[str, object]) -> None:
    if os.name == "nt":
        assert package_path(package).is_file(), package
    else:
        assert_executable_version(package, "rollback-tool 1.0.0")


if __name__ == "__main__":
    main()
