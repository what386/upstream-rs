"""End-to-end tests for the standalone bootstrap installers."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
INSTALLERS = ROOT / "scripts" / "install"


class PosixInstallerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.home = self.root / "home"
        self.tools = self.root / "tools"
        self.home.mkdir()
        self.tools.mkdir()
        self.existing_marker = self.home / ".upstream" / "keep-me"
        self.existing_marker.parent.mkdir()
        self.existing_marker.write_text("existing", encoding="utf-8")
        self.command_log = self.root / "commands.log"
        self.download_log = self.root / "downloads.log"
        self.binary = self.root / "upstream"
        self.binary.write_text(
            "#!/bin/sh\n"
            'printf "%s\\n" "$*" >> "$MOCK_COMMAND_LOG"\n'
            'if [ "$1" = "list" ]; then printf \'[{"name":"upstream"}]\\n\'; fi\n',
            encoding="utf-8",
        )
        self.binary.chmod(self.binary.stat().st_mode | stat.S_IXUSR)
        digest = hashlib.sha256(self.binary.read_bytes()).hexdigest()
        self.good_checksum = self.root / "good-checksums.txt"
        self.bad_checksum = self.root / "bad-checksums.txt"
        self.good_checksum.write_text(f"{digest}  $MOCK_ASSET_NAME\n", encoding="utf-8")
        self.bad_checksum.write_text(f"{'0' * 64}  $MOCK_ASSET_NAME\n", encoding="utf-8")
        self.write_tool("uname", '#!/bin/sh\nprintf "%s\\n" "$MOCK_ARCH"\n')
        self.write_tool(
            "curl",
            """#!/bin/sh
url=""
destination=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o) shift; destination="$1" ;;
        http*) url="$1" ;;
    esac
    shift
done
printf '%s|%s\n' "$url" "$destination" >> "$MOCK_DOWNLOAD_LOG"
case "$url" in
    */SHA256SUMS.txt) sed "s/\\$MOCK_ASSET_NAME/$MOCK_ASSET_NAME/" "$MOCK_CHECKSUM_FILE" > "$destination" ;;
    *) cp "$MOCK_BINARY" "$destination" ;;
esac
""",
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write_tool(self, name: str, contents: str) -> None:
        path = self.tools / name
        path.write_text(contents, encoding="utf-8")
        path.chmod(path.stat().st_mode | stat.S_IXUSR)

    def run_installer(
        self,
        shell: str,
        script: str,
        asset_name: str,
        *,
        architecture: str = "x86_64",
        valid_checksum: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        environment = os.environ.copy()
        environment.update(
            {
                "HOME": str(self.home),
                "PATH": f"{self.tools}{os.pathsep}{environment['PATH']}",
                "MOCK_ARCH": architecture,
                "MOCK_ASSET_NAME": asset_name,
                "MOCK_BINARY": str(self.binary),
                "MOCK_CHECKSUM_FILE": str(
                    self.good_checksum if valid_checksum else self.bad_checksum
                ),
                "MOCK_COMMAND_LOG": str(self.command_log),
                "MOCK_DOWNLOAD_LOG": str(self.download_log),
                "UPSTREAM_EXISTING_DATA": "keep",
            }
        )
        return subprocess.run(
            [shell, str(INSTALLERS / script)],
            cwd=ROOT,
            env=environment,
            text=True,
            capture_output=True,
            check=False,
        )

    def assert_successful_install(self, result: subprocess.CompletedProcess[str]) -> None:
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("hooks init", self.command_log.read_text(encoding="utf-8"))
        self.assertTrue(self.existing_marker.is_file())
        destinations = [
            Path(line.split("|", 1)[1])
            for line in self.download_log.read_text(encoding="utf-8").splitlines()
        ]
        self.assertTrue(destinations)
        self.assertTrue(all(not destination.parent.exists() for destination in destinations))

    def test_bash_verifies_checksum_before_running_binary(self) -> None:
        result = self.run_installer(
            "bash",
            "install.bash",
            "upstream-x86_64-unknown-linux-gnu",
        )
        self.assert_successful_install(result)

    @unittest.skipUnless(shutil.which("zsh"), "zsh is not installed")
    def test_zsh_verifies_checksum_before_running_binary(self) -> None:
        result = self.run_installer(
            "zsh", "install.zsh", "upstream-x86_64-apple-darwin"
        )
        self.assert_successful_install(result)

    def test_checksum_failure_does_not_touch_existing_data_or_run_binary(self) -> None:
        result = self.run_installer(
            "bash",
            "install.bash",
            "upstream-x86_64-unknown-linux-gnu",
            valid_checksum=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Checksum verification failed", result.stderr)
        self.assertTrue(self.existing_marker.is_file())
        self.assertFalse(self.command_log.exists())

    @unittest.skipUnless(shutil.which("zsh"), "zsh is not installed")
    def test_zsh_checksum_failure_does_not_run_binary(self) -> None:
        result = self.run_installer(
            "zsh",
            "install.zsh",
            "upstream-x86_64-apple-darwin",
            valid_checksum=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Checksum verification failed", result.stderr)
        self.assertTrue(self.existing_marker.is_file())
        self.assertFalse(self.command_log.exists())

    def test_unsupported_target_fails_before_downloading(self) -> None:
        result = self.run_installer(
            "bash",
            "install.bash",
            "unused",
            architecture="armv7l",
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unsupported architecture: armv7l", result.stderr)
        self.assertFalse(self.download_log.exists())
        self.assertTrue(self.existing_marker.is_file())

    @unittest.skipUnless(shutil.which("zsh"), "zsh is not installed")
    def test_zsh_unsupported_target_fails_before_downloading(self) -> None:
        result = self.run_installer(
            "zsh", "install.zsh", "unused", architecture="i686"
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unsupported architecture: i686", result.stderr)
        self.assertFalse(self.download_log.exists())


@unittest.skipUnless(shutil.which("pwsh"), "pwsh is not installed")
class PowerShellChecksumTests(unittest.TestCase):
    def test_confirm_checksum_accepts_match_and_rejects_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "upstream.exe"
            binary.write_bytes(b"test bootstrap binary")
            digest = hashlib.sha256(binary.read_bytes()).hexdigest()
            checksums = root / "SHA256SUMS.txt"
            checksums.write_text(
                f"{digest}  upstream-x86_64-pc-windows-msvc.exe\n",
                encoding="utf-8",
            )
            source = (INSTALLERS / "install.ps1").read_text(encoding="utf-8")
            definitions = source.rsplit("\nMain", 1)[0]
            harness = root / "checksum-test.ps1"
            harness.write_text(
                definitions
                + "\nConfirm-Checksum -Binary $args[0] -ChecksumFile $args[1] "
                + "-AssetName 'upstream-x86_64-pc-windows-msvc.exe'\n",
                encoding="utf-8",
            )
            valid = subprocess.run(
                ["pwsh", "-NoProfile", "-File", str(harness), str(binary), str(checksums)],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(valid.returncode, 0, valid.stdout + valid.stderr)
            checksums.write_text(
                f"{'0' * 64}  upstream-x86_64-pc-windows-msvc.exe\n",
                encoding="utf-8",
            )
            invalid = subprocess.run(
                ["pwsh", "-NoProfile", "-File", str(harness), str(binary), str(checksums)],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(invalid.returncode, 0)
            self.assertIn("Checksum verification failed", invalid.stderr)


if __name__ == "__main__":
    unittest.main()
