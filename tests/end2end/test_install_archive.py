"""Install the real ripgrep archive through the direct HTTP provider."""

from __future__ import annotations

import subprocess
import unittest

from tests.framework.commands import run_upstream
from tests.framework.environment import ROOT, reset_fakehome
from tests.framework.packages import package_from_list, package_path, package_version
from tests.framework.server import Server


REAL_ARCHIVE = ROOT / "server" / "artifacts" / "archives" / (
    "ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz"
)


class DirectInstallTests(unittest.TestCase):
    def test_install_archive(self) -> None:
        reset_fakehome()

        server = Server()
        try:
            artifact_name = f"archives/{REAL_ARCHIVE.name}"
            server.write_bytes(artifact_name, REAL_ARCHIVE.read_bytes())
            run_upstream(
                "install",
                server.url_for(artifact_name),
                "--yes",
                "--trust",
                "none",
            )

            package = package_from_list("rg")
            self.assertEqual(package["filetype"], "Archive", package)
            self.assertEqual(package_version(package), (15, 2, 0), package)
            executable = package_path(package)
            self.assertTrue(executable.is_file(), executable)
            result = subprocess.run(
                [str(executable), "--version"],
                check=True,
                text=True,
                capture_output=True,
            )
            self.assertTrue(result.stdout.startswith("ripgrep 15.2.0"), result.stdout)
        finally:
            server.close()


if __name__ == "__main__":
    unittest.main()
