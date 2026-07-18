#!/usr/bin/env python3
"""Export local state, reset it, and restore it through import commands."""

from __future__ import annotations

import tempfile
from pathlib import Path

from framework.commands import read_json, run_upstream
from framework.environment import reset_fakehome
from framework.packages import assert_executable_version, install_package, package_from_list, package_version


REPO = "BurntSushi/ripgrep"
PACKAGE = "rg"
TAG = "15.1.0"


def main() -> None:
    reset_fakehome()
    install_package(REPO, PACKAGE, TAG)
    run_upstream("config", "set", "download.low_threads=3")

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
        assert packages_export["packages"][0]["name"] == PACKAGE, packages_export
        profile_export = read_json(profile_path)
        assert profile_export["packages"]["packages"][0]["name"] == PACKAGE, profile_export
        assert read_json(keys_path)["version"] >= 1

        # Package and config/key imports are independently useful restore paths.
        reset_fakehome()
        run_upstream("import", "config", str(config_path))
        config = run_upstream("config", "get", "download.low_threads").stdout
        assert "download.low_threads" in config and "3" in config, config
        run_upstream("import", "keys", str(keys_path))
        run_upstream("import", "packages", str(packages_path))
        restored = package_from_list(PACKAGE)
        assert package_version(restored) == (15, 1, 0), restored
        assert_executable_version(restored, "ripgrep 15.1.0")

    print("config, keys, packages, and profile exports were written; imports restored state")


if __name__ == "__main__":
    main()
