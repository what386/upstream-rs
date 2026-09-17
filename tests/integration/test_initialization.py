"""Hermetic tests for first-run and repeat shell integration initialization."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import unittest

from tests.framework.commands import run_upstream
from tests.framework.environment import FAKEHOME, reset_fakehome


class InitializationIntegrationTests(unittest.TestCase):
    def setUp(self) -> None:
        reset_fakehome()
        self._remove_initialized_state()

    def test_hooks_init_creates_required_state(self) -> None:
        result = run_upstream("hooks", "init")

        self.assertIn("shell integration initialized", result.stdout.lower())

        data = FAKEHOME / ".upstream"
        for path in (
            data,
            data / "generated",
            data / "metadata",
            data / "packages" / "appimages",
            data / "packages" / "archives",
            data / "packages" / "binaries",
            data / "state" / "icons",
            data / "state" / "rollback",
            data / "state" / "symlinks",
        ):
            self.assertTrue(path.is_dir(), path)

        for path in (
            data / "generated" / "paths.sh",
            data / "generated" / "paths.nu",
            data / "metadata" / "packages.db",
            data / "metadata" / "trust.json",
        ):
            self.assertTrue(path.is_file(), path)

        config_candidates = (
            data / "config.toml",
            FAKEHOME / ".config" / "upstream" / "config.toml",
        )
        self.assertTrue(any(path.is_file() for path in config_candidates), config_candidates)

        if os.name != "nt":
            self.assert_profile_hooks_are_unique()

    def test_repeated_init_preserves_profile_content_without_duplicate_hooks(self) -> None:
        run_upstream("hooks", "init")

        profile = FAKEHOME / ".bashrc"
        marker = "# user configuration"
        profile.write_text(profile.read_text(encoding="utf-8") + f"\n{marker}\n", encoding="utf-8")

        run_upstream("hooks", "init")

        content = profile.read_text(encoding="utf-8")
        self.assertIn(marker, content)
        self.assertEqual(content.count("source $HOME/.upstream/generated/paths.sh"), 1)

    @staticmethod
    def _remove_initialized_state() -> None:
        data = FAKEHOME / ".upstream"
        binaries = data / "packages" / "binaries"
        upstream_binaries = {
            path.name: (path.read_bytes(), path.stat().st_mode)
            for path in binaries.glob("upstream-*")
            if path.is_file()
        }

        shutil.rmtree(data)
        binaries.mkdir(parents=True)
        for name, (contents, mode) in upstream_binaries.items():
            path = binaries / name
            path.write_bytes(contents)
            path.chmod(mode)

    def assert_profile_hooks_are_unique(self) -> None:
        profiles = {
            FAKEHOME / ".bashrc": "[ -f $HOME/.upstream/generated/paths.sh ] && source $HOME/.upstream/generated/paths.sh",
            FAKEHOME / ".zshrc": "[ -f $HOME/.upstream/generated/paths.sh ] && source $HOME/.upstream/generated/paths.sh",
            FAKEHOME / ".config" / "fish" / "config.fish": "test -f $HOME/.upstream/generated/paths.sh; and source $HOME/.upstream/generated/paths.sh",
            FAKEHOME / ".config" / "nushell" / "config.nu": 'const upstream_paths_nu = if ("~/.upstream/generated/paths.nu" | path expand | path exists) { ("~/.upstream/generated/paths.nu" | path expand) } else { null }; source-env $upstream_paths_nu',
        }
        for profile, hook in profiles.items():
            if not profile.exists():
                continue
            content = profile.read_text(encoding="utf-8")
            self.assertEqual(
                content.count(hook),
                1,
                profile,
            )


if __name__ == "__main__":
    unittest.main()
