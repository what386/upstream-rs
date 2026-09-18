#!/usr/bin/env python3
"""Build the small committed artifacts used by local HTTP/E2E tests."""

from __future__ import annotations

import argparse
import io
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import zipfile


ROOT = Path(__file__).resolve().parents[2]
FIXTURE_TOOL_SOURCE = ROOT / "tests" / "fixtures" / "artifacts" / "fixture-tool"
OUTPUT = ROOT / "server" / "artifacts"
VERSION = "1.0.0"
APPIMAGE_ARCH = "x86_64"
APPIMAGE_RUNTIME = Path(__file__).with_name(f"runtime-{APPIMAGE_ARCH}")
BINARY_NAME = f"fixture-tool-{VERSION}"
ARCHIVE_STEM = f"fixture-tool-{VERSION}-linux-{APPIMAGE_ARCH}"
APPIMAGE_NAME = f"fixture-tool-desktop-{VERSION}-{APPIMAGE_ARCH}.AppImage"

def run(command: list[str], *, cwd: Path | None = None) -> None:
    print("+", " ".join(command))
    subprocess.run(command, cwd=cwd, check=True)


def compile_binary(destination: Path) -> None:
    compiler = shutil.which(os.environ.get("CC", "cc"))
    if compiler is None:
        raise SystemExit("generator requires a C compiler (set CC to override)")

    destination.parent.mkdir(parents=True, exist_ok=True)
    run(
        [
            compiler,
            "-std=c11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-ffile-prefix-map=" + str(ROOT) + "=/source",
            str(FIXTURE_TOOL_SOURCE / "fixture-tool.c"),
            "-o",
            str(destination),
        ]
    )
    destination.chmod(destination.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def payload_files(binary: Path) -> dict[str, bytes]:
    return {
        "bin/fixture-tool": binary.read_bytes(),
        "config.toml": (FIXTURE_TOOL_SOURCE / "config.toml").read_bytes(),
    }


def write_tar(destination: Path, files: dict[str, bytes]) -> None:
    with destination.open("wb") as output:
        import gzip

        with gzip.GzipFile(fileobj=output, mode="wb", mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
                for name in sorted(files):
                    info = tarfile.TarInfo(f"fixture-tool-{VERSION}/{name}")
                    info.size = len(files[name])
                    info.mode = 0o755 if name == "bin/fixture-tool" else 0o644
                    info.uid = 0
                    info.gid = 0
                    info.uname = ""
                    info.gname = ""
                    info.mtime = 0
                    archive.addfile(info, io.BytesIO(files[name]))


def write_zip(destination: Path, files: dict[str, bytes]) -> None:
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name in sorted(files):
            info = zipfile.ZipInfo(
                f"fixture-tool-{VERSION}/{name}",
                date_time=(1980, 1, 1, 0, 0, 0),
            )
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (0o755 if name == "bin/fixture-tool" else 0o644) << 16
            archive.writestr(info, files[name])


def build_appimage(binary: Path, destination: Path) -> None:
    appimagetool = shutil.which("appimagetool")
    if appimagetool is None:
        raise SystemExit(
            "generator requires appimagetool to build the genuine AppImage fixture"
        )
    if not APPIMAGE_RUNTIME.is_file():
        raise SystemExit(f"missing AppImage runtime: {APPIMAGE_RUNTIME}")

    with tempfile.TemporaryDirectory(prefix="upstream-generator-") as temporary:
        appdir = Path(temporary) / "FixtureTool.AppDir"
        (appdir / "usr" / "bin").mkdir(parents=True)
        (appdir / "usr" / "share" / "applications").mkdir(parents=True)
        icon_dir = appdir / "usr" / "share" / "icons" / "hicolor" / "64x64" / "apps"
        icon_dir.mkdir(parents=True)
        shutil.copyfile(binary, appdir / "usr" / "bin" / "fixture-tool")
        (appdir / "usr" / "bin" / "fixture-tool").chmod(0o755)
        desktop = FIXTURE_TOOL_SOURCE / "fixture-tool.desktop"
        shutil.copyfile(desktop, appdir / "fixture-tool-desktop.desktop")
        shutil.copyfile(desktop, appdir / "usr" / "share" / "applications" / desktop.name)
        icon = FIXTURE_TOOL_SOURCE / "fixture-tool.png"
        shutil.copyfile(icon, appdir / icon.name)
        shutil.copyfile(icon, icon_dir / icon.name)
        (appdir / "config.toml").write_bytes((FIXTURE_TOOL_SOURCE / "config.toml").read_bytes())
        destination.parent.mkdir(parents=True, exist_ok=True)
        run(
            [
                appimagetool,
                "--no-appstream",
                "--runtime-file",
                str(APPIMAGE_RUNTIME),
                str(appdir),
                str(destination),
            ]
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--skip-appimage",
        action="store_true",
        help="build only the binary and archive fixtures",
    )
    args = parser.parse_args()

    with tempfile.TemporaryDirectory(prefix="upstream-generator-build-") as temporary:
        binary = Path(temporary) / BINARY_NAME
        compile_binary(binary)
        files = payload_files(binary)

        binary_output = OUTPUT / "binaries" / BINARY_NAME
        binary_output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(binary, binary_output)
        binary_output.chmod(0o755)

        archive_output = OUTPUT / "archives"
        archive_output.mkdir(parents=True, exist_ok=True)
        write_tar(archive_output / f"{ARCHIVE_STEM}.tar.gz", files)
        write_zip(archive_output / f"{ARCHIVE_STEM}.zip", files)

        if not args.skip_appimage:
            build_appimage(binary, OUTPUT / "appimages" / APPIMAGE_NAME)

    print("generated artifacts under", OUTPUT)
    return 0


if __name__ == "__main__":
    sys.exit(main())
