"""Install the generated AppImage and verify desktop integration."""

from __future__ import annotations

from pathlib import Path
import sys
import unittest

from tests.framework.commands import run_upstream
from tests.framework.environment import FAKEHOME, ROOT, reset_fakehome
from tests.framework.packages import package_from_list, package_path
from tests.framework.server import Server


FIXTURE_APPIMAGE = ROOT / "test-server" / "artifacts" / "appimages" / (
    "fixture-tool-desktop-1.0.0-x86_64.AppImage"
)


class AppImageDesktopInstallTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform.startswith("linux"), "AppImage integration requires Linux")
    def test_install_appimage_desktop(self) -> None:
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

            package = package_from_list("fixture-tool")
            self.assertEqual(package["filetype"], "AppImage", package)
            self.assertTrue(package_path(package).name.endswith(".AppImage"), package)
            icon_path = package["icon_path"]
            self.assertIsInstance(icon_path, str, package)
            self.assertEqual(Path(icon_path).name, "fixture-tool.png")

            package_id = package["id"]
            self.assertIsInstance(package_id, str, package)
            desktop_name = "".join(
                character
                if character.isascii() and (character.isalnum() or character in "-_.")
                else "_"
                for character in package_id
            )
            desktop_entry = (
                FAKEHOME / ".local" / "share" / "applications" / f"{desktop_name}.desktop"
            )
            contents = desktop_entry.read_text(encoding="utf-8")
            self.assertIn("Name=Fixture Tool Desktop", contents)
            self.assertIn(f"Exec={package_path(package)}", contents)
            self.assertIn("fixture-tool.png", contents)
            self.assertIn("Categories=Utility;", contents)
            self.assertIn("Terminal=false", contents)
        finally:
            server.close()


if __name__ == "__main__":
    unittest.main()
