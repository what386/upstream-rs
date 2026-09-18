#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
artifact_root="$repo_root/server/artifacts"
temporary="$(mktemp -d)"
trap 'rm -rf -- "$temporary"' EXIT

download() {
    local url="$1"
    local destination="$2"

    if command -v curl >/dev/null 2>&1; then
        curl --fail --location --retry 3 --silent --show-error \
            --output "$destination" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget --quiet --tries=3 --output-document="$destination" "$url"
    else
        printf 'fetch-artifacts.sh requires curl or wget.\n' >&2
        exit 1
    fi
}

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{ print tolower($1) }'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{ print tolower($1) }'
    else
        printf 'fetch-artifacts.sh requires sha256sum or shasum.\n' >&2
        exit 1
    fi
}

fetch_artifact() {
    local category="$1"
    local repository="$2"
    local release="$3"
    local filename="$4"
    local target="$artifact_root/$category/$filename"
    local base_url="https://github.com/$repository/releases/download/$release"
    local artifact_tmp="$temporary/$filename"
    local checksum_tmp="$temporary/$filename.sha256"

    if [[ -s "$target" ]]; then
        printf 'Already present: %s\n' "${target#"$repo_root/"}"
        return
    fi

    mkdir -p "$(dirname -- "$target")"
    printf 'Downloading %s\n' "$filename"
    download "$base_url/$filename" "$artifact_tmp"
    download "$base_url/$filename.sha256" "$checksum_tmp"

    local expected actual
    expected="$(awk -v asset="$filename" '$2 == asset || $2 == "*" asset { print tolower($1); exit }' "$checksum_tmp")"
    if [[ ! "$expected" =~ ^[[:xdigit:]]{64}$ ]]; then
        printf 'No valid SHA-256 checksum found for %s.\n' "$filename" >&2
        exit 1
    fi

    actual="$(sha256 "$artifact_tmp")"
    if [[ "$actual" != "$expected" ]]; then
        printf 'Checksum verification failed for %s.\n' "$filename" >&2
        exit 1
    fi

    mv -- "$artifact_tmp" "$target"
    printf 'Installed %s\n' "${target#"$repo_root/"}"
}

fetch_artifact \
    archives \
    BurntSushi/ripgrep \
    15.2.0 \
    ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz

fetch_artifact \
    appimages \
    wez/wezterm \
    20240203-110809-5046fc22 \
    WezTerm-20240203-110809-5046fc22-Ubuntu20.04.AppImage
