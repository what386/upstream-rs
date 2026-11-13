#!/usr/bin/env bash
set -euo pipefail

if [[ -n "${UPSTREAM_TEST_HOME:-}" ]]; then
    printf '%s\n' "$UPSTREAM_TEST_HOME"
    exit 0
fi

username="${USER:-${USERNAME:-$(id -un)}}"
printf '%s/upstream-rs-test-%s\n' "${TMPDIR:-/tmp}" "$username"
