"""Deterministic fixtures for the read-only registry trust audit."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts" / "registry"
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location("audit_trust", SCRIPTS / "audit_trust.py")
assert SPEC and SPEC.loader
AUDIT = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = AUDIT
SPEC.loader.exec_module(AUDIT)


class TrustAuditTests(unittest.TestCase):
    def registry_directory(self, root: Path) -> Path:
        packages = root / "packages"
        packages.mkdir()
        (packages / "tool.toml").write_text(
            "\n".join(
                [
                    'name = "tool"',
                    "revision = 1",
                    "desktop = false",
                    'trust = "checksum"',
                    "[install]",
                    'type = "release"',
                    'repo = "https://github.com/owner/tool"',
                    'provider = "github"',
                ]
            ),
            encoding="utf-8",
        )
        return packages

    def test_complete_checksum_manifest_covers_selected_asset(self) -> None:
        contents = "a" * 64 + "  tool-x86_64-unknown-linux-gnu.tar.gz\n"
        self.assertEqual(
            AUDIT.checksum_covers(
                "tool-x86_64-unknown-linux-gnu.tar.gz", "SHA256SUMS.txt", contents
            ),
            (True, "named"),
        )

    def test_partial_platform_manifest_reports_uncovered_asset(self) -> None:
        contents = "a" * 64 + "  tool-x86_64-unknown-linux-gnu.tar.gz\n"
        covered, format_name = AUDIT.checksum_covers(
            "tool-aarch64-unknown-linux-gnu.tar.gz", "SHA256SUMS.txt", contents
        )
        self.assertFalse(covered)
        self.assertEqual(format_name, "named")

    def test_intermitent_releases_do_not_recommend_checksum(self) -> None:
        audited = [
            {"evidence": [{"status": "covered"}]},
            {"evidence": [{"status": "missing-checksum"}]},
        ]
        complete = all(
            item["status"] == "covered"
            for release in audited
            for item in release["evidence"]
        )
        self.assertFalse(complete)

    def test_unsupported_checksum_manifest_is_identified(self) -> None:
        self.assertEqual(
            AUDIT.checksum_covers("tool.tar.gz", "checksums.txt", "tool: not-a-hash"),
            (False, "unsupported"),
        )

    def test_colon_and_prefixed_digest_formats_are_supported(self) -> None:
        digest = "a" * 64
        contents = f"tool-one.tar.gz: {digest}\nsha256={digest}  tool-two.tar.gz\n"
        names, format_name = AUDIT.parse_checksum_names(contents)
        self.assertEqual(format_name, "named")
        self.assertEqual(names, {"tool-one.tar.gz", "tool-two.tar.gz"})

    def test_no_checksum_asset_is_distinct_from_unsupported_format(self) -> None:
        assets = [{"name": "tool-linux-x86_64.tar.gz"}]
        self.assertIsNone(
            AUDIT.find_checksum_asset(assets, "tool-linux-x86_64.tar.gz")
        )

    def test_target_selection_rejects_other_platform_and_architecture(self) -> None:
        assets = [
            {"name": "tool-linux-x86_64.tar.gz", "size": 10},
            {"name": "tool-linux-aarch64.tar.gz", "size": 10},
            {"name": "tool-windows-x86_64.zip", "size": 10},
        ]
        self.assertEqual(
            AUDIT.select_asset(assets, {"binary": "tool"}, AUDIT.TARGETS[0]),
            "tool-linux-x86_64.tar.gz",
        )

    def test_strict_coverage_loss_has_a_distinct_exit_status(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            packages = self.registry_directory(Path(directory))
            result = {
                "name": "tool",
                "configured": "checksum",
                "recommended": "best-effort",
                "releases": [],
            }
            with mock.patch.object(AUDIT, "audit_package", return_value=result):
                self.assertEqual(
                    AUDIT.main(["--packages-dir", str(packages), "--package", "tool"]),
                    1,
                )

    def test_provider_error_has_a_distinct_exit_status(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            packages = self.registry_directory(Path(directory))
            with mock.patch.object(
                AUDIT,
                "audit_package",
                side_effect=AUDIT.AuditNetworkError("provider unavailable"),
            ):
                self.assertEqual(
                    AUDIT.main(["--packages-dir", str(packages), "--package", "tool"]),
                    2,
                )


if __name__ == "__main__":
    unittest.main()
