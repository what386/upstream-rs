#!/usr/bin/env sh
set -eu

GITHUB_USER="what386"
GITHUB_REPO="upstream-rs"

usage() {
    cat <<'EOF'
Usage: scripts/install/completions.sh <shell>

Supported shells:
  bash
  fish
  zsh
  elvish
EOF
}

if [ "$#" -ne 1 ]; then
    usage
    exit 1
fi

shell="$1"

case "$shell" in
    bash)
        asset="completions.bash"
        dest_dir="${HOME}/.local/share/bash-completion/completions"
        dest_file="${dest_dir}/upstream"
        ;;
    fish)
        asset="completions.fish"
        dest_dir="${HOME}/.config/fish/completions"
        dest_file="${dest_dir}/upstream.fish"
        ;;
    zsh)
        asset="completions.zsh"
        dest_dir="${HOME}/.zfunc"
        dest_file="${dest_dir}/_upstream"
        ;;
    elvish)
        asset="completions.elvish"
        dest_dir="${HOME}/.config/elvish/lib"
        dest_file="${dest_dir}/upstream.elv"
        ;;
    *)
        echo "Unsupported shell: $shell" >&2
        usage
        exit 1
        ;;
esac

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT INT TERM

url="https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases/latest/download/${asset}"
tmp_file="${tmp_dir}/${asset}"

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$tmp_file"
elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$tmp_file"
else
    echo "Neither curl nor wget found. Please install one." >&2
    exit 1
fi

mkdir -p "$dest_dir"
cp "$tmp_file" "$dest_file"

echo "Installed $shell completion to: $dest_file"
