#!/usr/bin/env python3
"""Install a locally served AppImage and exercise desktop integration."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

from tests.framework.commands import run_upstream
from tests.framework.environment import FAKEHOME, ROOT, reset_fakehome
from tests.framework.packages import package_from_list, package_path
from tests.framework.server import Server


PACKAGE = "fixture-tool"
FIXTURE_APPIMAGE = ROOT / "server" / "artifacts" / "appimages" / (
    "fixture-tool-desktop-1.0.0-x86_64.AppImage"
)


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
    assert "Name=Fixture Tool Desktop" in contents, contents
    assert f"Exec={executable}" in contents, contents
    assert "Terminal=false" in contents, contents


class EndToEndDesktopTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform.startswith("linux"), "desktop integration requires Linux")
    def test_desktop_entry_lifecycle(self) -> None:
        reset_fakehome()

        server = Server()
        try:
            artifact_name = f"appimages/{FIXTURE_APPIMAGE.name}"
            server.write_bytes(artifact_name, FIXTURE_APPIMAGE.read_bytes())
            run_upstream(
                "install",
                server.url_for(artifact_name),
                "--desktop",
                "--yes",
                "--trust",
                "none",
            )
            package = package_from_list(PACKAGE)
            package_id = package["id"]
            assert isinstance(package_id, str), package
            executable = package_path(package)

            assert_desktop_entry(package_id, executable)

            run_upstream("package", "rm-entry", package_id)
            desktop_entry = desktop_entry_path(package_id)
            assert not desktop_entry.exists(), desktop_entry

            run_upstream("package", "add-entry", package_id)
            assert_desktop_entry(package_id, executable)
        finally:
            server.close()

        print(f"verified desktop integration lifecycle for {PACKAGE}")


if __name__ == "__main__":
    unittest.main()
