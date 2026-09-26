#!/usr/bin/env bash
set -euo pipefail

if (( $# != 1 )); then
    printf 'Usage: %s <output-directory>\n' "$0" >&2
    exit 1
fi

output_dir="$1"
mkdir -p -- "$output_dir"

for shell in bash fish powershell zsh elvish; do
    extension="$shell"
    if [[ "$shell" == powershell ]]; then
        extension=ps1
    fi
    cargo run --locked --quiet --bin completions --features shell-completions -- "$shell" \
        > "$output_dir/completions.$extension"
done
