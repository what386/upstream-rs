#!/usr/bin/env python3
"""Export local state, reset it, and restore it through import commands."""

from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path

from tests.framework.commands import read_json, run_upstream
from tests.framework.environment import reset_fakehome
from tests.framework.packages import (
    assert_executable_version,
    package_from_list,
    package_path,
    package_version,
)
from tests.framework.server import start_fixture_server


PACKAGE = "fixture-tool"


class EndToEndExportImportTests(unittest.TestCase):
    def test_export_import(self) -> None:
        reset_fakehome()
        server = start_fixture_server()
        try:
            run_upstream(
                "install",
                server.url_for("releases.html"),
                "--kind",
                "archive",
                "--yes",
                "--trust",
                "none",
            )
            run_upstream("config", "set", "download.low_threads=3")
            package = package_from_list(PACKAGE)
            package_id = package["id"]

            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                config_path = root / "config.toml"
                keys_path = root / "keys.json"
                packages_path = root / "packages.json"
                profile_path = root / "profile.json"

                run_upstream("export", "config", str(config_path))
                run_upstream("export", "keys", str(keys_path))
                run_upstream("export", "packages", str(packages_path))
                run_upstream("export", "profile", str(profile_path))

                assert "low_threads = 3" in config_path.read_text(encoding="utf-8")
                packages_export = read_json(packages_path)
                exported_package = next(
                    item for item in packages_export["packages"] if item["id"] == package_id
                )
                assert exported_package["filetype"] == "Archive", exported_package
                profile_export = read_json(profile_path)
                profile_package = next(
                    item
                    for item in profile_export["packages"]["packages"]
                    if item["id"] == package_id
                )
                assert profile_package["filetype"] == "Archive", profile_package
                assert read_json(keys_path)["version"] >= 1

                # Package and config/key imports are independently useful restore paths.
                reset_fakehome()
                run_upstream("import", "config", str(config_path))
                config = run_upstream("config", "get", "download.low_threads").stdout
                assert "download.low_threads" in config and "3" in config, config
                run_upstream("import", "keys", str(keys_path))
                run_upstream("import", "packages", str(packages_path))
                restored = package_from_list(PACKAGE)
                assert package_version(restored) == (1, 0, 0), restored
                if os.name == "nt":
                    assert package_path(restored).is_file(), restored
                else:
                    assert_executable_version(restored, "fixture-tool 1.0.0")
        finally:
            server.close()

        print("config, keys, packages, and profile exports were written; imports restored state")


if __name__ == "__main__":
    unittest.main()
