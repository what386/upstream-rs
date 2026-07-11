#!/usr/bin/env python3
"""Install a pinned release into tests/fakehome and smoke-test the result."""

from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path

from common import package_from_list, package_path, reset_fakehome, run_upstream, run_upstream_json


REPO = "BurntSushi/ripgrep"
PACKAGE = "rg"
TAG = "15.1.0"


def main() -> None:
    reset_fakehome()

    run_upstream(
        "install",
        REPO,
        PACKAGE,
        "--tag",
        TAG,
        "--yes",
    )

    listed = package_from_list(PACKAGE)
    assert listed["repo_slug"] == REPO, listed
    assert listed["filetype"] == "Binary", listed
    assert listed["version"] == {
        "major": 15,
        "minor": 1,
        "patch": 0,
        "is_prerelease": False,
    }, listed

    info = run_upstream_json("info", PACKAGE)
    assert info == listed, (info, listed)

    executable = package_path(listed)
    assert executable.is_file(), executable
    version = subprocess.run(
        [str(executable), "--version"],
        check=True,
        text=True,
        capture_output=True,
    )
    assert version.stdout.startswith("ripgrep 15.1.0"), version.stdout

    with tempfile.TemporaryDirectory() as directory:
        input_file = Path(directory) / "input.txt"
        input_file.write_text("first line\nneedle line\n", encoding="utf-8")
        search = subprocess.run(
            [str(executable), "needle", str(input_file)],
            check=True,
            text=True,
            capture_output=True,
        )
        assert "needle line" in search.stdout, search.stdout

    print(f"installed {PACKAGE} {TAG} and executed {executable}")


if __name__ == "__main__":
    main()
