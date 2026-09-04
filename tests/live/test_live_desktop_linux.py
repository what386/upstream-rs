#!/usr/bin/env python3
"""Install ripgrep with desktop integration and exercise its lifecycle."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

from tests.framework.commands import run_upstream
from tests.framework.environment import FAKEHOME, reset_fakehome
from tests.framework.packages import package_from_list, package_path


REPO = "BurntSushi/ripgrep"
PACKAGE = "ripgrep"
TAG = "15.1.0"


def desktop_entry_path(package_id: str) -> Path:
    filesystem_name = "".join(
        character if character.isascii() and (character.isalnum() or character in "-_.") else "_"
        for character in package_id
    )
    return FAKEHOME / ".local/share/applications" / f"{filesystem_name}.desktop"


def assert_desktop_entry(package_id: str, executable: Path) -> None:
    desktop_entry = desktop_entry_path(package_id)
    assert desktop_entry.is_file(), desktop_entry
    contents = desktop_entry.read_text(encoding="utf-8")
    assert "[Desktop Entry]" in contents, contents
    assert f"Name={package_id}" in contents, contents
    assert f"Exec={executable}" in contents, contents
    assert "Terminal=false" in contents, contents


class LiveDesktopTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform.startswith("linux"), "desktop integration requires Linux")
    def test_desktop_entry_lifecycle(self) -> None:
        reset_fakehome()

        run_upstream("install", REPO, "--tag", TAG, "--desktop", "--yes")
        package = package_from_list(PACKAGE)
        package_id = package["id"]
        assert isinstance(package_id, str), package
        executable = package_path(package)

        assert_desktop_entry(package_id, executable)

        run_upstream("package", "rm-entry", PACKAGE)
        desktop_entry = desktop_entry_path(package_id)
        assert not desktop_entry.exists(), desktop_entry

        run_upstream("package", "add-entry", PACKAGE)
        assert_desktop_entry(package_id, executable)

        print(f"verified desktop integration lifecycle for {PACKAGE}")


if __name__ == "__main__":
    unittest.main()
