"""Install an archive discovered from a checked-in local download page."""

from __future__ import annotations

import io
import os
import tarfile
import unittest
import zipfile

from tests.framework.commands import run_upstream
from tests.framework.environment import reset_fakehome, upstream_binary
from tests.framework.packages import package_from_list, package_version
from tests.framework.server import Server, platform_archive_suffix


def write_page_artifact(server: Server) -> None:
    archive = io.BytesIO()
    executable = (
        upstream_binary().read_bytes()
        if os.name == "nt"
        else b"#!/bin/sh\nprintf 'fixture-tool 1.0.0\\n'\n"
    )
    if os.name == "nt":
        name = f"archives/fixture-tool-1.0.0-{platform_archive_suffix()}"
        with zipfile.ZipFile(archive, "w") as contents:
            contents.writestr("fixture-tool.exe", executable)
    else:
        name = f"archives/fixture-tool-1.0.0-{platform_archive_suffix()}"
        with tarfile.open(fileobj=archive, mode="w:gz") as contents:
            info = tarfile.TarInfo("fixture-tool")
            info.mode = 0o755
            info.size = len(executable)
            contents.addfile(info, io.BytesIO(executable))
    server.write_bytes(name, archive.getvalue())


class ScrapedDownloadPageTests(unittest.TestCase):
    def test_install_from_checked_in_download_page(self) -> None:
        reset_fakehome()
        server = Server(start=False)
        try:
            write_page_artifact(server)
            server.start()
            run_upstream(
                "install",
                f"{server.base_url}/pages/downloads/standard.html",
                "--yes",
                "--trust",
                "none",
            )

            package = package_from_list("fixture-tool")
            self.assertEqual(package["filetype"], "Archive", package)
            self.assertEqual(package_version(package), (1, 0, 0), package)
        finally:
            server.close()


if __name__ == "__main__":
    unittest.main()
