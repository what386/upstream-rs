"""Exercise the developer updater against a temporary managed installation."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import unittest

from tests.framework.environment import ROOT


@unittest.skipUnless(os.name == "posix", "Bash developer updater requires POSIX")
class LocalBuildUpdateIntegrationTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory(prefix="upstream-local-update-")
        self.addCleanup(temporary.cleanup)
        self.home = Path(temporary.name)
        self.env = os.environ.copy()
        original_home = Path(self.env["HOME"])
        self.env.setdefault("CARGO_HOME", str(original_home / ".cargo"))
        self.env.setdefault("RUSTUP_HOME", str(original_home / ".rustup"))
        self.env.update(
            HOME=str(self.home),
            XDG_CONFIG_HOME=str(self.home / ".config"),
            XDG_DATA_HOME=str(self.home / ".local/share"),
            XDG_DATA_DIRS=str(self.home / ".local/share"),
        )
        self.binary = self.home / ".upstream/packages/binaries/custom-upstream-filename"
        self.binary.parent.mkdir(parents=True)
        self.original_binary = b"#!/bin/sh\nprintf 'old fixture binary\\n'\n"
        self.binary.write_bytes(self.original_binary)
        self.binary.chmod(0o755)
        links = self.home / ".upstream/state/symlinks"
        links.mkdir(parents=True)
        for alias in ("upstream", "upstream-dev"):
            (links / alias).symlink_to(self.binary)

        self.database = self.home / ".upstream/metadata/packages.db"
        self.database.parent.mkdir(parents=True)
        with sqlite3.connect(self.database) as database:
            database.executescript((ROOT / "src/storage/database/schema.sql").read_text())
            database.execute(
                """INSERT INTO packages
                   (id, repo_slug, filetype, version_major, version_minor, version_patch,
                    version_is_prerelease, release_tag, channel, provider, install_type,
                    is_pinned, install_path, last_upgraded)
                   VALUES (?, ?, 'Binary', 1, 0, 0, 0, 'v1.0.0', 'Stable', 'Github',
                           'Release', 1, ?, '2026-01-01T00:00:00Z')""",
                ("github:what386/upstream-rs", "what386/upstream-rs", str(self.binary)),
            )
            database.executemany(
                "INSERT INTO package_executables (package_id, path, name) VALUES (?, ?, ?)",
                [("github:what386/upstream-rs", str(self.binary), alias)
                 for alias in ("upstream", "upstream-dev")],
            )
        self.nushell = self.home / ".local/share/nushell/vendor/autoload/upstream.nu"
        self.nushell.parent.mkdir(parents=True)
        self.nushell.write_text("# previous completion\n")

    def update(self) -> subprocess.CompletedProcess[str]:
        # The script must discover the checkout independently of the caller's cwd.
        return subprocess.run(
            ["bash", str(ROOT / "scripts/dev/sketchy-upgrade.sh"), "--debug"],
            cwd=self.home, env=self.env, text=True, capture_output=True,
        )

    def test_updates_local_binary_and_preserves_package_identity_and_aliases(self) -> None:
        result = self.update()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotEqual(self.binary.read_bytes(), self.original_binary)
        self.assertIn("local build", result.stdout)
        if shutil.which("nu"):
            self.assertIn('export extern "upstream remove"', self.nushell.read_text())
        self.assertTrue(
            (self.home / ".local/share/bash-completion/completions/upstream").is_file()
        )
        with sqlite3.connect(self.database) as database:
            self.assertEqual(database.execute(
                "SELECT id, install_path, is_pinned, release_tag FROM packages"
            ).fetchone(), ("github:what386/upstream-rs", str(self.binary), 1, None))
            self.assertEqual(database.execute(
                "SELECT name, path FROM package_executables ORDER BY name"
            ).fetchall(), [(alias, str(self.binary)) for alias in ("upstream", "upstream-dev")])
        for alias in ("upstream", "upstream-dev"):
            executable = self.home / ".upstream/state/symlinks" / alias
            self.assertEqual(executable.resolve(), self.binary)
            version = subprocess.run([str(executable), "--version"], env=self.env,
                                     text=True, capture_output=True)
            self.assertEqual(version.returncode, 0, version.stderr)
            self.assertNotIn("old fixture", version.stdout)

    def test_failed_metadata_update_restores_binary_and_completions(self) -> None:
        with sqlite3.connect(self.database) as database:
            database.executescript("""
                CREATE TRIGGER reject_local_build BEFORE UPDATE ON packages BEGIN
                    SELECT RAISE(FAIL, 'injected metadata failure');
                END;
            """)
        result = self.update()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("injected metadata failure", result.stderr)
        self.assertEqual(self.binary.read_bytes(), self.original_binary)
        self.assertEqual(self.nushell.read_text(), "# previous completion\n")
