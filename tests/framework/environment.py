"""Repository and fakehome paths used by integration tests."""

from __future__ import annotations

import getpass
import os
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
FAKEHOME = Path(
    os.environ.get(
        "UPSTREAM_TEST_HOME",
        str(Path(tempfile.gettempdir()) / f"upstream-rs-test-{getpass.getuser()}"),
    )
)
RESET_TESTHOME = ROOT / "scripts" / "test" / "reset-testhome.sh"


def reset_fakehome() -> None:
    env = os.environ.copy()
    env["UPSTREAM_TEST_HOME"] = str(FAKEHOME)
    subprocess.run(["bash", str(RESET_TESTHOME)], cwd=ROOT, env=env, check=True)


def upstream_binary() -> Path:
    binaries = FAKEHOME / ".upstream" / "packages" / "binaries"
    candidates = sorted(path for path in binaries.glob("upstream-*") if path.is_file())
    if len(candidates) != 1:
        raise AssertionError(f"expected one upstream test binary in {binaries}, found {candidates}")
    return candidates[0]
