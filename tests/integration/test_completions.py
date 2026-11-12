"""Hermetic tests for native and generated shell completion behavior."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tests.framework.commands import run_upstream_result
from tests.framework.environment import FAKEHOME, ROOT, reset_fakehome


class CompletionIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        output_dir = tempfile.TemporaryDirectory(prefix="upstream-completions-")
        cls.addClassCleanup(output_dir.cleanup)
        cls.completions_dir = Path(output_dir.name)
        subprocess.run(
            ["bash", str(ROOT / "scripts" / "generate-completions.sh"), str(cls.completions_dir)],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )

    def test_completion_protocol_handles_missing_candidates(self) -> None:
        reset_fakehome()
        log = FAKEHOME / ".upstream" / "log.jsonl"
        before_log = log.read_bytes() if log.exists() else b""

        result = run_upstream_result("__complete", "info", "0", "--", "missing")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")
        self.assertEqual(log.read_bytes() if log.exists() else b"", before_log)

    def test_generated_completions_delegate_dynamic_values(self) -> None:
        expected_marker = "__complete"
        for shell in ("bash", "fish", "zsh", "ps1", "elvish", "nu"):
            path = self.completions_dir / f"completions.{shell}"
            self.assertIn(expected_marker, path.read_text(encoding="utf-8"), str(path))

        fish = (self.completions_dir / "completions.fish").read_text(encoding="utf-8")
        self.assertIn("-f -a '(__upstream_dynamic)'", fish)

    def run_nushell(self, script: str, *, fail_completer: bool = False) -> object:
        with tempfile.TemporaryDirectory(prefix="upstream-nushell-") as directory:
            root = Path(directory)
            # Exercise the generated module's real external protocol without
            # depending on the developer's installed packages or shell config.
            executable = root / "upstream"
            executable.write_text(
                f"#!{sys.executable}\n"
                "import sys\n"
                f"if {fail_completer!r}:\n"
                "    print('unavailable metadata', file=sys.stderr)\n"
                "    sys.exit(1)\n"
                "if sys.argv[1] != '__complete':\n"
                "    import json\n"
                "    print(json.dumps(sys.argv[1:]))\n"
                "    sys.exit(0)\n"
                "if sys.argv[1:] == ['__complete', 'info', '0', '--', '']:\n"
                "    print('bat\\nripgrep\\ntwo words')\n"
                "elif sys.argv[1:] == ['__complete', 'package', '1', '--', 'rename', '']:\n"
                "    print('rg')\n"
                "else:\n"
                "    raise SystemExit(f'unexpected completion request: {sys.argv!r}')\n",
                encoding="utf-8",
            )
            executable.chmod(0o755)
            env = os.environ.copy()
            env["PATH"] = str(root) + os.pathsep + env["PATH"]
            env["HOME"] = str(root)
            module = self.completions_dir / "completions.nu"
            result = subprocess.run(
                ["nu", "--no-config-file", "-c", f"use {json.dumps(str(module))} *\n{script} | to json"],
                cwd=root,
                env=env,
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stderr, "")
            return json.loads(result.stdout)

    @unittest.skipUnless(shutil.which("nu") and os.name != "nt", "Nushell/POSIX unavailable")
    def test_nushell_static_completions_and_signatures(self) -> None:
        result = self.run_nushell("""
            [
                ('upstream package ' | commandline complete)
                ('upstream remove --' | commandline complete)
                ('upstream install --trust ' | commandline complete)
                ('upstream package get ripgrep ' | commandline complete)
                (help upstream remove | ansi strip)
            ]
        """)
        self.assertIn("upstream package rename", result[0])
        self.assertIn("--dry-run", result[1])
        self.assertIn("--yes", result[1])
        self.assertEqual(result[2], ["all", "best-effort", "checksum", "none", "signature"])
        self.assertIn("trust_mode", result[3])
        self.assertIn("names", result[4])

    @unittest.skipUnless(shutil.which("nu") and os.name != "nt", "Nushell/POSIX unavailable")
    def test_nushell_completes_packages_only_in_package_arguments(self) -> None:
        commands = [
            "upstream info ri", "upstream list ri", "upstream changelog ri",
            "upstream remove bat ri", "upstream uninstall ri", "upstream reinstall ri",
            "upstream upgrade --dry-run ri", "upstream doctor ri",
            "upstream history --package ri", "upstream package pin ri",
            "upstream package get ri", "upstream package set ri",
            "upstream package unset ri", "upstream package unpin ri",
            "upstream package add-entry ri", "upstream package rm-entry ri",
            "upstream docs ri", "upstream docs --fetch ri",
            "upstream docs --fetch bat ri", "upstream docs bat --fetch ri",
            "echo ignored | upstream remove ri",
        ]
        script = "[" + "\n".join(
            f"({json.dumps(command)} | commandline complete)" for command in commands
        ) + "]"
        self.assertEqual(self.run_nushell(script), [["ripgrep"] for _ in commands])

        self.assertEqual(self.run_nushell("""
            alias upr = upstream remove
            [
                ('upr ri' | commandline complete)
                ('upstream package rename r' | commandline complete)
                ('upstream docs ripgrep ri' | commandline complete)
                ('upstream package rename rg ri' | commandline complete)
                ('upstream remove "two w' | commandline complete)
            ]
        """), [["ripgrep"], ["rg"], [], [], ["two words"]])

    @unittest.skipUnless(shutil.which("nu") and os.name != "nt", "Nushell/POSIX unavailable")
    def test_nushell_completion_errors_are_silent(self) -> None:
        self.assertEqual(self.run_nushell(
            "'upstream info ri' | commandline complete", fail_completer=True
        ), [])

    @unittest.skipUnless(shutil.which("nu") and os.name != "nt", "Nushell/POSIX unavailable")
    def test_nushell_signatures_preserve_external_arguments(self) -> None:
        self.assertEqual(self.run_nushell("""
            [
                (upstream docs --fetch | from json)
                (upstream docs --fetch bat ripgrep | from json)
                (upstream package rename rg 'new alias' | from json)
                (upstream --yes remove bat | from json)
                (upstream info --help | from json)
                (upstream info -h | from json)
                (upstream remove -- -dash | from json)
            ]
        """), [
            ["docs", "--fetch"],
            ["docs", "--fetch", "bat", "ripgrep"],
            ["package", "rename", "rg", "new alias"],
            ["--yes", "remove", "bat"],
            ["info", "--help"],
            ["info", "-h"],
            ["remove", "--", "-dash"],
        ])

    @unittest.skipUnless(
        subprocess.run(["bash", "--version"], capture_output=True).returncode == 0,
        "bash unavailable",
    )
    def test_bash_completion_script_has_valid_syntax(self) -> None:
        path = self.completions_dir / "completions.bash"
        result = subprocess.run(["bash", "-n", str(path)], text=True, capture_output=True)

        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
