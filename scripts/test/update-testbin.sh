#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

cargo build --features testing_donotuseinrelease

host="$(rustc -vV | sed -n 's/^host: //p')"
binary_dir="$repo_root/tests/fakehome/.upstream/packages/binaries"
binary_path="$binary_dir/upstream-$host"
symlink_path="$repo_root/tests/fakehome/.upstream/state/symlinks/upstream"

mkdir -p "$binary_dir"
rm -f "$binary_path"
cp "$repo_root/target/debug/upstream" "$binary_path"

mkdir -p "$(dirname -- "$symlink_path")"
rm -f "$symlink_path"
ln -s "$binary_path" "$symlink_path"
