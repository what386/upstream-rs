"""Helpers for invoking the upstream CLI."""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

from .environment import FAKEHOME, ROOT, upstream_binary


def upstream_environment() -> dict[str, str]:
    env = os.environ.copy()
    host_home = env.get("HOME")
    env["HOME"] = str(FAKEHOME)
    # Deliberately re-add the caller's Rust toolchain and Cargo registry so
    # integration packages can build while Upstream itself uses FAKEHOME state.
    if host_home:
        env.setdefault("RUSTUP_HOME", str(Path(host_home) / ".rustup"))
        env.setdefault("CARGO_HOME", str(Path(host_home) / ".cargo"))
    return env


def run_upstream_result(*args: str) -> subprocess.CompletedProcess[str]:
    env = upstream_environment()
    return subprocess.run(
        [str(upstream_binary()), "--no-pager", *args],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
    )


def start_upstream(*args: str) -> subprocess.Popen[str]:
    options: dict[str, object] = {}
    if os.name == "nt":
        options["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    return subprocess.Popen(
        [str(upstream_binary()), "--no-pager", *args],
        cwd=ROOT,
        env=upstream_environment(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        **options,
    )


def run_upstream(*args: str) -> subprocess.CompletedProcess[str]:
    result = run_upstream_result(*args)
    if result.returncode:
        command = " ".join(result.args)
        raise AssertionError(
            f"upstream command failed ({result.returncode}): {command}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def run_upstream_json(*args: str) -> object:
    result = run_upstream(*args, "--json")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise AssertionError(f"expected JSON from upstream {args!r}: {result.stdout!r}") from error


def read_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AssertionError(f"expected a JSON file at {path}") from error
