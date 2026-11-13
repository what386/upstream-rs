#!/usr/bin/env bash
# Update the managed upstream installation from this checkout, without fetching a release.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd -- "$repo_root"

build_profile=release
if [[ "${1:-}" == --debug && $# == 1 ]]; then
    build_profile=dev
elif (( $# != 0 )); then
    printf 'Usage: %s [--debug]\n' "$0" >&2
    exit 1
fi

if [[ -z "${HOME:-}" || "$HOME" == / ]]; then
    printf 'Refusing to use unsafe HOME=%q\n' "${HOME:-}" >&2
    exit 1
fi
command -v python3 >/dev/null

work_dir="$(mktemp -d)"
trap 'rm -rf -- "$work_dir"' EXIT

# Cargo reports actual executable paths, including custom target directories.
# Build without the isolated-test-home feature.
cargo build --locked --profile "$build_profile" --features shell-completions \
    --bin upstream --bin completions --example install-local-build \
    --message-format=json-render-diagnostics > "$work_dir/build.jsonl"

python3 - "$work_dir/build.jsonl" > "$work_dir/executables" <<'PY'
import json
import sys

executables = {}
with open(sys.argv[1], encoding="utf-8") as build_output:
    for line in build_output:
        message = json.loads(line)
        if message.get("reason") == "compiler-artifact" and message.get("executable"):
            executables[message["target"]["name"]] = message["executable"]
for name in ("upstream", "completions", "install-local-build"):
    print(executables[name])
PY

executables=()
while IFS= read -r executable; do
    executables+=("$executable")
done < "$work_dir/executables"

UPSTREAM_COMPLETION_GENERATOR="${executables[1]}" \
    bash scripts/generate-completions.sh "$work_dir/completions"

# Reuse the normal replacement lifecycle, including shell discovery and rollback.
"${executables[2]}" "${executables[0]}" "$work_dir/completions"
