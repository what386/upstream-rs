#!/usr/bin/env python3
"""Reset the isolated test home and install the freshly built test binary."""

from __future__ import annotations

import getpass
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
FAKEHOME = Path(
    os.environ.get(
        "UPSTREAM_TEST_HOME",
        str(Path(tempfile.gettempdir()) / f"upstream-rs-test-{getpass.getuser()}"),
    )
)


def remove_path(path: Path) -> None:
    if path.is_symlink() or path.is_file():
        path.unlink()
    elif path.exists():
        shutil.rmtree(path)


def host_triple() -> str:
    result = subprocess.run(
        ["rustc", "-vV"],
        check=True,
        capture_output=True,
        text=True,
    )
    for line in result.stdout.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ")
    raise RuntimeError("rustc -vV did not report a host triple")


def main() -> None:
    remove_path(FAKEHOME)
    (FAKEHOME / ".config").mkdir(parents=True)

    subprocess.run(
        ["cargo", "build", "--features", "testing_donotuseinrelease"],
        cwd=ROOT,
        check=True,
    )

    environment = os.environ.copy()
    environment["UPSTREAM_TEST_HOME"] = str(FAKEHOME)
    executable_name = "upstream.exe" if os.name == "nt" else "upstream"
    built_binary = ROOT / "target" / "debug" / executable_name
    subprocess.run(
        [str(built_binary), "--no-pager", "hooks", "init"],
        cwd=ROOT,
        env=environment,
        check=True,
    )

    host = host_triple()
    binary_dir = FAKEHOME / ".upstream" / "packages" / "binaries"
    binary_dir.mkdir(parents=True, exist_ok=True)
    binary_path = binary_dir / f"upstream-{host}{'.exe' if os.name == 'nt' else ''}"
    remove_path(binary_path)
    shutil.copy2(built_binary, binary_path)

    symlink_dir = FAKEHOME / ".upstream" / "state" / "symlinks"
    symlink_dir.mkdir(parents=True, exist_ok=True)
    link_path = symlink_dir / ("upstream.exe" if os.name == "nt" else "upstream")
    remove_path(link_path)
    if os.name == "nt":
        os.link(binary_path, link_path)
    else:
        link_path.symlink_to(binary_path)


if __name__ == "__main__":
    main()
