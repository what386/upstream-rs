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
            'if [ "$1" = "list" ]; then\n'
            '    if [ "$MOCK_PACKAGE_INSTALLED" = "yes" ]; then '
            "printf '[{\"id\":\"upstream\"}]\\n'; else exit 1; fi\n"
            "fi\n",
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
        existing_data: str = "keep",
        package_installed: bool = True,
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
                "MOCK_PACKAGE_INSTALLED": "yes" if package_installed else "no",
                "UPSTREAM_EXISTING_DATA": existing_data,
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

    def test_keep_existing_data_preserves_user_files_and_skips_package_install(self) -> None:
        result = self.run_installer("bash", "install.bash", "upstream-x86_64-unknown-linux-gnu")

        self.assert_successful_install(result)
        self.assertEqual(
            self.command_log.read_text(encoding="utf-8").splitlines(),
            ["hooks init", "list upstream --json"],
        )
        self.assertEqual(self.existing_marker.read_text(encoding="utf-8"), "existing")

    def test_replace_existing_data_removes_stale_files_before_refreshing(self) -> None:
        result = self.run_installer(
            "bash",
            "install.bash",
            "upstream-x86_64-unknown-linux-gnu",
            existing_data="replace",
        )

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.existing_marker.exists())
        self.assertEqual(
            self.command_log.read_text(encoding="utf-8").splitlines(),
            ["hooks init", "list upstream --json"],
        )

    def test_invalid_existing_data_setting_fails_before_running_upstream(self) -> None:
        result = self.run_installer(
            "bash",
            "install.bash",
            "upstream-x86_64-unknown-linux-gnu",
            existing_data="invalid",
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("UPSTREAM_EXISTING_DATA must be 'keep' or 'replace'", result.stderr)
        self.assertFalse(self.command_log.exists())

    def test_existing_data_path_that_is_a_file_fails_before_running_upstream(self) -> None:
        self.existing_marker.unlink()
        data_path = self.home / ".upstream"
        data_path.rmdir()
        data_path.write_text("not a directory", encoding="utf-8")

        result = self.run_installer("bash", "install.bash", "upstream-x86_64-unknown-linux-gnu")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exists but is not a directory", result.stderr)
        self.assertFalse(self.command_log.exists())
        self.assertEqual(data_path.read_text(encoding="utf-8"), "not a directory")

    def test_missing_managed_package_runs_install_once(self) -> None:
        result = self.run_installer(
            "bash",
            "install.bash",
            "upstream-x86_64-unknown-linux-gnu",
            package_installed=False,
        )

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            self.command_log.read_text(encoding="utf-8").splitlines(),
            [
                "hooks init",
                "list upstream --json",
                "--yes install what386/upstream-rs upstream -k binary",
            ],
        )

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
class PowerShellInstallerTests(unittest.TestCase):
    def test_missing_managed_package_continues_to_self_install(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            command_log = root / "commands.log"
            if os.name == "nt":
                binary = root / "upstream.cmd"
                binary.write_text(
                    "@echo off\n"
                    'echo %*>>"%MOCK_COMMAND_LOG%"\n'
                    'if "%~1"=="list" if "%~2"=="--json" (\n'
                    "    echo []\n"
                    "    exit /b 0\n"
                    ")\n"
                    'if "%~1"=="list" (\n'
                    "    echo No installed packages match upstream. 1>&2\n"
                    "    exit /b 1\n"
                    ")\n",
                    encoding="utf-8",
                )
            else:
                binary = root / "upstream"
                binary.write_text(
                    "#!/bin/sh\n"
                    'printf "%s\\n" "$*" >> "$MOCK_COMMAND_LOG"\n'
                    'if [ "$1" = "list" ] && [ "$2" = "--json" ]; then\n'
                    "    printf '[]\\n'\n"
                    "    exit 0\n"
                    "fi\n"
                    'if [ "$1" = "list" ]; then\n'
                    '    printf "No installed packages match upstream.\\n" >&2\n'
                    "    exit 1\n"
                    "fi\n",
                    encoding="utf-8",
                )
                binary.chmod(binary.stat().st_mode | stat.S_IXUSR)

            source = (INSTALLERS / "install.ps1").read_text(encoding="utf-8")
            definitions = source.rsplit("\nMain", 1)[0]
            harness = root / "self-install-test.ps1"
            harness.write_text(
                definitions
                + "\nif (Test-Path Variable:PSNativeCommandUseErrorActionPreference) {\n"
                + "    $PSNativeCommandUseErrorActionPreference = $true\n"
                + "}\n"
                + "$env:MOCK_COMMAND_LOG = $args[1]\n"
                + "Install-UpstreamIfMissing -Binary $args[0]\n",
                encoding="utf-8",
            )

            result = subprocess.run(
                [
                    "pwsh",
                    "-NoProfile",
                    "-File",
                    str(harness),
                    str(binary),
                    str(command_log),
                ],
                text=True,
                capture_output=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(
                command_log.read_text(encoding="utf-8").splitlines(),
                [
                    "list --json",
                    "--yes install what386/upstream-rs upstream -k win-exe",
                ],
            )

    def test_missing_visual_cpp_runtime_reports_microsoft_download(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = (INSTALLERS / "install.ps1").read_text(encoding="utf-8")
            definitions = source.rsplit("\nMain", 1)[0]
            harness = root / "runtime-test.ps1"
            harness.write_text(
                definitions
                + "\nfunction Get-ItemProperty { throw 'registry entry not found' }\n"
                + "try { Assert-VisualCppRuntime -Architecture x86_64; exit 2 } "
                + "catch { Write-Output $_.Exception.Message; exit 0 }\n",
                encoding="utf-8",
            )

            result = subprocess.run(
                ["pwsh", "-NoProfile", "-File", str(harness)],
                text=True,
                capture_output=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Microsoft Visual C++ Redistributable", result.stdout)
            self.assertIn(
                "https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist",
                result.stdout,
            )

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
