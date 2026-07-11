"""Repository and fakehome paths used by integration tests."""

from __future__ import annotations

from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[3]
FAKEHOME = ROOT / "tests" / "fakehome"
RESET_TESTHOME = ROOT / "scripts" / "test" / "reset-testhome.sh"


def reset_fakehome() -> None:
    subprocess.run(["bash", str(RESET_TESTHOME)], cwd=ROOT, check=True)


def upstream_binary() -> Path:
    binaries = FAKEHOME / ".upstream" / "packages" / "binaries"
    candidates = sorted(path for path in binaries.glob("upstream-*") if path.is_file())
    if len(candidates) != 1:
        raise AssertionError(f"expected one upstream test binary in {binaries}, found {candidates}")
    return candidates[0]
