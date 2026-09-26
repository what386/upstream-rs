"""Hermetic tests for native and generated shell completion behavior."""

from __future__ import annotations

import subprocess
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
        for shell in ("bash", "fish", "zsh", "ps1", "elvish"):
            path = self.completions_dir / f"completions.{shell}"
            self.assertIn(expected_marker, path.read_text(encoding="utf-8"), str(path))

        fish = (self.completions_dir / "completions.fish").read_text(encoding="utf-8")
        self.assertIn("-f -a '(__upstream_dynamic)'", fish)

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
