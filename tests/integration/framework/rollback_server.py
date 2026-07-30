"""Local versioned archive server used by rollback integration tests."""

from __future__ import annotations

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import os
from pathlib import Path
import zipfile
import tarfile
import tempfile
from threading import Event, Thread
import time

from .environment import upstream_binary


PACKAGE = "rollback-tool"


class RollbackServer:
    def __init__(self, *, throttle_update: bool = False) -> None:
        self.directory = Path(tempfile.mkdtemp(prefix="upstream-rollback-server-"))
        self.throttle_update = throttle_update
        self.update_requested = Event()
        self.version = 1
        old_executable = (
            upstream_binary().read_bytes()
            if os.name == "nt"
            else b"#!/bin/sh\nprintf 'rollback-tool 1.0.0\\n'\n"
        )
        self._write_archive(1, old_executable)
        self._write_archive(2, b"not a tar archive")

        owner = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:  # noqa: N802
                if self.path == "/releases.html":
                    body = owner._page().encode()
                    self.send_response(200)
                    self.send_header("Content-Type", "text/html")
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                    return

                path = owner.directory / Path(self.path.lstrip("/"))
                if path.parent != owner.directory or not path.is_file():
                    self.send_error(404)
                    return
                data = path.read_bytes()
                if "v2.0.0" in path.name:
                    owner.update_requested.set()
                self.send_response(200)
                self.send_header("Content-Type", "application/octet-stream")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                if owner.throttle_update and "v2.0.0" in path.name:
                    try:
                        for offset in range(0, len(data), 1):
                            self.wfile.write(data[offset : offset + 1])
                            self.wfile.flush()
                            time.sleep(0.05)
                    except BrokenPipeError:
                        pass
                else:
                    self.wfile.write(data)

            def log_message(self, *_args: object) -> None:
                return

        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.httpd.server_port}/releases.html"

    def publish_update(self) -> None:
        self.version = 2

    def wait_for_update_request(self, timeout: float = 30.0) -> None:
        if not self.update_requested.wait(timeout):
            raise AssertionError("upgrade never requested the v2 archive")

    def close(self) -> None:
        self.httpd.shutdown()
        self.thread.join(timeout=5)
        for path in self.directory.glob("*"):
            path.unlink()
        self.directory.rmdir()

    def _page(self) -> str:
        releases = [1, 2] if self.version == 2 else [1]
        suffix = "windows-x86_64.zip" if os.name == "nt" else "linux-x86_64.tar.gz"
        links = "".join(
            f'<a href="/rollback-tool-v{version}.0.0-{suffix}">v{version}</a>'
            for version in releases
        )
        return f"<html><body>{links}</body></html>"

    def _write_archive(self, version: int, executable: bytes) -> None:
        suffix = "windows-x86_64.zip" if os.name == "nt" else "linux-x86_64.tar.gz"
        path = self.directory / f"rollback-tool-v{version}.0.0-{suffix}"
        if version == 2:
            path.write_bytes(executable)
            return
        if os.name == "nt":
            with zipfile.ZipFile(path, "w") as archive:
                archive.writestr(f"{PACKAGE}.exe", executable)
            return
        content = io.BytesIO()
        with tarfile.open(fileobj=content, mode="w:gz") as archive:
            info = tarfile.TarInfo(PACKAGE)
            info.mode = 0o755
            info.size = len(executable)
            archive.addfile(info, io.BytesIO(executable))
        path.write_bytes(content.getvalue())
