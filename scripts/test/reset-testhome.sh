#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

fakehome="$repo_root/tests/fakehome"
fakehome_template="$repo_root/tests/metadata/fakehome-template"

rm -rf "$fakehome"
mkdir -p "$fakehome"
cp -a "$fakehome_template"/. "$fakehome"/

cargo build --features testing_donotuseinrelease

host="$(rustc -vV | sed -n 's/^host: //p')"
binary_dir="$fakehome/.upstream/packages/binaries"
binary_path="$binary_dir/upstream-$host"

mkdir -p "$binary_dir"
rm -f "$binary_path"
cp "$repo_root/target/debug/upstream" "$binary_path"
