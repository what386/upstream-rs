#!/usr/bin/env bash
set -euo pipefail

repository="what386/upstream-rs"
release_base="https://github.com/${repository}/releases/latest/download"

run_upstream() {
    local binary="$1"
    shift
    printf 'Running: upstream %s\n' "$*"
    "$binary" "$@"
}

valid_upstream() {
    local binary="$1"
    [[ -n "$binary" && -x "$binary" ]] || return 1
    "$binary" list --json 2>/dev/null | grep -Eq '"name"[[:space:]]*:[[:space:]]*"upstream"'
}

complete_repair() {
    local binary="$1"
    run_upstream "$binary" hooks init
    run_upstream "$binary" doctor --fix
}

installed_binary="$(command -v upstream 2>/dev/null || true)"
if valid_upstream "$installed_binary"; then
    if run_upstream "$installed_binary" --yes reinstall upstream --force; then
        complete_repair "$installed_binary"
        printf 'Repair complete.\n'
        exit 0
    fi
    printf 'In-place repair failed; falling back to a clean bootstrap.\n' >&2
fi

case "$(uname -s)" in
    Linux) platform="unknown-linux-gnu" ;;
    Darwin) platform="apple-darwin" ;;
    *) printf 'Unsupported operating system: %s\n' "$(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
    x86_64 | amd64) architecture="x86_64" ;;
    arm64 | aarch64) architecture="aarch64" ;;
    *) printf 'Unsupported architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
esac

temporary="$(mktemp -d)"
trap 'rm -rf -- "$temporary"' EXIT
asset="upstream-${architecture}-${platform}"
bootstrap="${temporary}/upstream"
checksums="${temporary}/SHA256SUMS.txt"

if command -v curl >/dev/null 2>&1; then
    curl --fail --location --silent --show-error "${release_base}/${asset}" --output "$bootstrap"
    curl --fail --location --silent --show-error "${release_base}/SHA256SUMS.txt" --output "$checksums"
elif command -v wget >/dev/null 2>&1; then
    wget --quiet "${release_base}/${asset}" --output-document="$bootstrap"
    wget --quiet "${release_base}/SHA256SUMS.txt" --output-document="$checksums"
else
    printf 'Repair requires curl or wget.\n' >&2
    exit 1
fi

expected="$(awk -v asset="$asset" '$2 == asset || $2 == "*" asset { print $1; exit }' "$checksums")"
[[ "$expected" =~ ^[[:xdigit:]]{64}$ ]] || { printf 'No checksum found for %s.\n' "$asset" >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$bootstrap" | awk '{print $1}')"
else
    actual="$(shasum -a 256 "$bootstrap" | awk '{print $1}')"
fi
actual="$(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]')"
expected="$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')"
[[ "$actual" == "$expected" ]] || { printf 'Checksum verification failed for %s.\n' "$asset" >&2; exit 1; }
chmod +x "$bootstrap"

if ! run_upstream "$bootstrap" --yes remove upstream --force; then
    printf 'Forced removal did not complete; continuing with a fresh install attempt.\n' >&2
fi
run_upstream "$bootstrap" --yes install "$repository" upstream
complete_repair "$bootstrap"
printf 'Repair complete. Restart separately launched shells before testing upstream.\n'
