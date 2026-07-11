#!/usr/bin/env python3
"""Remove ripgrep from tests/fakehome and verify its state is gone."""

from __future__ import annotations

from common import (
    package_from_list,
    package_path,
    run_upstream,
    run_upstream_json,
    run_upstream_result,
)


PACKAGE = "rg"


def main() -> None:
    package = package_from_list(PACKAGE)
    executable = package_path(package)
    assert executable.is_file(), executable

    run_upstream("remove", PACKAGE, "--yes", "--purge")

    packages = run_upstream_json("list")
    assert isinstance(packages, list), packages
    assert all(item.get("name") != PACKAGE for item in packages), packages
    assert not executable.exists(), executable

    info = run_upstream_result("info", PACKAGE, "--json")
    assert info.returncode != 0, f"package {PACKAGE!r} is still available through info"

    print(f"removed {PACKAGE} and verified its metadata and executable are gone")


if __name__ == "__main__":
    main()
