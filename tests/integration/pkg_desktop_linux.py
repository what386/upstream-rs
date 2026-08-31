#!/usr/bin/env python3
"""Install ripgrep with desktop integration and exercise its lifecycle."""

from __future__ import annotations

import sys
from pathlib import Path

from framework.commands import run_upstream
from framework.environment import FAKEHOME, reset_fakehome
from framework.packages import package_from_list, package_path


REPO = "BurntSushi/ripgrep"
PACKAGE = "ripgrep"
TAG = "15.1.0"
DESKTOP_ENTRY = FAKEHOME / ".local/share/applications" / f"{PACKAGE}.desktop"


def assert_desktop_entry(executable: Path) -> None:
    assert DESKTOP_ENTRY.is_file(), DESKTOP_ENTRY
    contents = DESKTOP_ENTRY.read_text(encoding="utf-8")
    assert "[Desktop Entry]" in contents, contents
    assert "Name=rg" in contents, contents
    assert f"Exec={executable}" in contents, contents
    assert "Terminal=false" in contents, contents


def main() -> None:
    if not sys.platform.startswith("linux"):
        print("desktop integration test requires Linux; skipped")
        return

    reset_fakehome()
    run_upstream("install", REPO, "--tag", TAG, "--desktop", "--yes")
    package = package_from_list(PACKAGE)
    executable = package_path(package)

    assert_desktop_entry(executable)

    run_upstream("package", "rm-entry", PACKAGE)
    assert not DESKTOP_ENTRY.exists(), DESKTOP_ENTRY

    run_upstream("package", "add-entry", PACKAGE)
    assert_desktop_entry(executable)

    print(f"verified desktop integration lifecycle for {PACKAGE}")


if __name__ == "__main__":
    main()
