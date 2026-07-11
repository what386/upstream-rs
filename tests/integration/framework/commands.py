"""Helpers for invoking the upstream CLI."""

from __future__ import annotations

import json
import os
import subprocess

from .environment import FAKEHOME, ROOT, upstream_binary


def run_upstream_result(*args: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["HOME"] = str(FAKEHOME)
    return subprocess.run(
        [str(upstream_binary()), "--no-pager", *args],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
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
