"""Python helpers for using the repository's Ruby artifact server."""

from __future__ import annotations

import io
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile

from .environment import ROOT, upstream_binary


class Server:
    """Runs a Ruby server backed by a temporary directory of stored artifacts."""

    def __init__(
        self,
        *,
        throttle_pattern: str | None = None,
        throttle_delay: float = 0.0,
        prefix: str = "upstream-server-",
        start: bool = True,
    ) -> None:
        self.directory = Path(tempfile.mkdtemp(prefix=prefix))
        self.request_marker = self.directory / ".request-marker"
        self.throttle_pattern = throttle_pattern
        self.throttle_delay = throttle_delay
        self.process: subprocess.Popen[str] | None = None
        if start:
            self.start()

    @property
    def base_url(self) -> str:
        if self.process is None:
            raise RuntimeError("server has not started")
        return f"http://127.0.0.1:{self.port}"

    def url_for(self, artifact: str) -> str:
        return f"{self.base_url}/artifacts/{artifact.lstrip('/')}"

    def write_bytes(self, artifact: str, content: bytes) -> Path:
        path = self.directory / artifact
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
        return path

    def write_text(self, artifact: str, content: str) -> Path:
        path = self.directory / artifact
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        return path

    def start(self) -> None:
        if self.process is not None:
            return

        environment = os.environ.copy()
        environment.update(
            {
                "SERVER_ARTIFACT_ROOT": str(self.directory),
                "SERVER_PORT": "0",
                "SERVER_REQUEST_MARKER": str(self.request_marker),
            }
        )
        if self.throttle_pattern:
            environment.update(
                {
                    "SERVER_THROTTLE_PATTERN": self.throttle_pattern,
                    "SERVER_THROTTLE_DELAY": str(self.throttle_delay),
                }
            )

        self.process = subprocess.Popen(
            ["ruby", str(ROOT / "test-server" / "main.rb")],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        startup = self.process.stdout.readline() if self.process.stdout else ""
        match = re.search(r"http://127\.0\.0\.1:(\d+)", startup)
        if not match:
            self.close()
            raise RuntimeError(f"Ruby artifact server failed to start: {startup!r}")
        self.port = int(match.group(1))

    def wait_for_request(self, timeout: float = 30.0) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.request_marker.exists():
                return
            time.sleep(0.05)
        raise AssertionError("server did not receive the expected artifact request")

    def close(self) -> None:
        process = self.process
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        if process is not None and process.stdout is not None:
            process.stdout.close()
        shutil.rmtree(self.directory, ignore_errors=True)


PACKAGE = "fixture-tool"


def platform_archive_suffix() -> str:
    if os.name == "nt":
        return "windows-x86_64.zip"
    if sys.platform == "darwin":
        return "macos-x86_64.tar.gz"
    return "linux-x86_64.tar.gz"


def write_upgrade_fixtures(server: Server) -> None:
    """Store the versioned archive fixtures used by lifecycle tests."""
    old_executable = (
        upstream_binary().read_bytes()
        if os.name == "nt"
        else b"#!/bin/sh\nprintf 'fixture-tool 1.0.0\\n'\n"
    )
    _write_archive(server, 1, old_executable)
    _write_archive(server, 2, b"not a tar archive\n" + b"x" * (4 * 1024 * 1024))
    write_release_page(server, 1)


def start_fixture_server() -> Server:
    """Start a local server with versioned lifecycle fixtures."""
    server = Server(start=False)
    write_upgrade_fixtures(server)
    server.start()
    return server


def write_release_page(server: Server, version: int) -> None:
    """Store a release page exposing the requested fixture versions."""
    releases = range(1, version + 1)
    suffix = platform_archive_suffix()
    links = "".join(
        f'<a href="/artifacts/{PACKAGE}-v{release}.0.0-{suffix}">v{release}</a>'
        for release in releases
    )
    server.write_text("releases.html", f"<html><body>{links}</body></html>")


def _write_archive(server: Server, version: int, executable: bytes) -> None:
    suffix = platform_archive_suffix()
    name = f"{PACKAGE}-v{version}.0.0-{suffix}"
    if version == 2:
        server.write_bytes(name, executable)
        return
    if os.name == "nt":
        path = server.directory / name
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr(f"{PACKAGE}.exe", executable)
        return
    content = io.BytesIO()
    with tarfile.open(fileobj=content, mode="w:gz") as archive:
        info = tarfile.TarInfo(PACKAGE)
        info.mode = 0o755
        info.size = len(executable)
        archive.addfile(info, io.BytesIO(executable))
    server.write_bytes(name, content.getvalue())
