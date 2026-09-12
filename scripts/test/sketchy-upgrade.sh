#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly REPO_ROOT="$repo_root"
cd -- "$REPO_ROOT"

if [[ -z "${HOME:-}" || "$HOME" == "/" ]]; then
    printf 'Refusing to use unsafe HOME=%q\n' "${HOME:-}" >&2
    exit 1
fi

readonly FISH_COMPLETIONS_DIR="$HOME/.config/fish/completions"
readonly COMPLETION_FILE="$FISH_COMPLETIONS_DIR/upstream.fish"
readonly COMPLETION_FILE_BACKUP="$COMPLETION_FILE.old"
readonly COMPLETION_SOURCE="$REPO_ROOT/completions/completions.fish"

readonly UPSTREAM_PKG="$HOME/.upstream/packages/binaries/upstream-x86_64-unknown-linux-gnu"
upstream_stash="$REPO_ROOT/$(basename -- "$UPSTREAM_PKG")"
readonly UPSTREAM_STASH="$upstream_stash"
readonly RELEASE_BINARY="$REPO_ROOT/target/release/upstream"

for required_file in "$COMPLETION_FILE" "$COMPLETION_SOURCE" "$UPSTREAM_PKG" "$RELEASE_BINARY"; do
    if [[ ! -f "$required_file" ]]; then
        printf 'Required file is missing: %s\n' "$required_file" >&2
        exit 1
    fi
done

if [[ -e "$UPSTREAM_STASH" ]]; then
    printf 'Refusing to overwrite existing stash: %s\n' "$UPSTREAM_STASH" >&2
    exit 1
fi

mkdir -p -- "$FISH_COMPLETIONS_DIR"

completion_moved=false
binary_moved=false

restore_on_failure() {
    local status=$?
    if ((status != 0)); then
        if [[ "$binary_moved" == true && ! -e "$UPSTREAM_PKG" && -e "$UPSTREAM_STASH" ]]; then
            mv -- "$UPSTREAM_STASH" "$UPSTREAM_PKG"
        fi
        if [[ "$completion_moved" == true && -e "$COMPLETION_FILE_BACKUP" ]]; then
            rm -f -- "$COMPLETION_FILE"
            mv -- "$COMPLETION_FILE_BACKUP" "$COMPLETION_FILE"
        fi
    fi
    exit "$status"
}
trap restore_on_failure EXIT

rm -f -- "$COMPLETION_FILE_BACKUP"
mv -- "$COMPLETION_FILE" "$COMPLETION_FILE_BACKUP"
completion_moved=true

cp -- "$COMPLETION_SOURCE" "$COMPLETION_FILE"

mv -- "$UPSTREAM_PKG" "$UPSTREAM_STASH"
binary_moved=true

cp -- "$RELEASE_BINARY" "$UPSTREAM_PKG"

trap - EXIT
