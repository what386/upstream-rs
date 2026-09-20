"""Install the generated archive through the local HTTP server."""

from __future__ import annotations

import io
import os
import subprocess
import tarfile
import unittest
import zipfile

from tests.framework.commands import run_upstream
from tests.framework.environment import reset_fakehome, upstream_binary
from tests.framework.packages import package_from_list, package_path, package_version
from tests.framework.server import Server, platform_archive_suffix


def write_archive(server: Server) -> str:
    suffix = platform_archive_suffix()
    if os.name == "nt":
        name = f"fixture-tool-1.0.0-{suffix}"
        archive = io.BytesIO()
        with zipfile.ZipFile(archive, "w") as contents:
            contents.writestr("fixture-tool.exe", upstream_binary().read_bytes())
        server.write_bytes(name, archive.getvalue())
        return name

    name = f"fixture-tool-1.0.0-{suffix}"
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode="w:gz") as contents:
        executable = b"#!/bin/sh\nprintf 'fixture-tool 1.0.0\\n'\n"
        info = tarfile.TarInfo("fixture-tool")
        info.mode = 0o755
        info.size = len(executable)
        contents.addfile(info, io.BytesIO(executable))
    server.write_bytes(name, archive.getvalue())
    return name


class DirectInstallTests(unittest.TestCase):
    def test_install_archive(self) -> None:
        reset_fakehome()

        server = Server()
        try:
            artifact_name = write_archive(server)
            run_upstream(
                "install",
                server.url_for(artifact_name),
                "--yes",
                "--trust",
                "none",
            )

            package = package_from_list("fixture-tool")
            self.assertEqual(package["filetype"], "Archive", package)
            self.assertEqual(package_version(package), (1, 0, 0), package)
            executable = package_path(package)
            self.assertTrue(executable.is_file(), executable)
            if os.name != "nt":
                result = subprocess.run(
                    [str(executable), "--version"],
                    check=True,
                    text=True,
                    capture_output=True,
                )
                self.assertTrue(result.stdout.startswith("fixture-tool 1.0.0"), result.stdout)
        finally:
            server.close()


if __name__ == "__main__":
    unittest.main()
