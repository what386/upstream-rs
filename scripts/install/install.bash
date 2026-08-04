#!/usr/bin/env bash
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

GITHUB_USER="what386"
GITHUB_REPO="upstream-rs"
BINARY_NAME="upstream"
OS="unknown-linux-gnu"
UPSTREAM_DIR="${HOME}/.upstream"

detect_arch() {
    case "$(uname -m)" in
    x86_64 | amd64) echo "x86_64" ;;
    aarch64 | arm64) echo "aarch64" ;;
    armv7l) echo "armv7" ;;
    i386 | i686) echo "i686" ;;
    *) echo "unknown" ;;
    esac
}

download_file() {
    local url="$1"
    local destination="$2"

    if command -v curl &>/dev/null; then
        curl -fsSL "$url" -o "$destination"
    elif command -v wget &>/dev/null; then
        wget -q "$url" -O "$destination"
    else
        echo -e "${RED}Error: Neither curl nor wget found. Please install one.${NC}" >&2
        exit 1
    fi
}

verify_checksum() {
    local expected actual
    expected="$(awk -v asset="$ASSET_NAME" '$2 == asset || $2 == "*" asset { print tolower($1); exit }' "$CHECKSUM_FILE")"

    if [[ -z "$expected" ]]; then
        echo -e "${RED}Error: No checksum found for ${ASSET_NAME}.${NC}" >&2
        exit 1
    fi

    if command -v sha256sum &>/dev/null; then
        actual="$(sha256sum "$TMP_FILE" | awk '{ print tolower($1) }')"
    elif command -v shasum &>/dev/null; then
        actual="$(shasum -a 256 "$TMP_FILE" | awk '{ print tolower($1) }')"
    else
        echo -e "${RED}Error: Neither sha256sum nor shasum found.${NC}" >&2
        exit 1
    fi

    if [[ "$actual" != "$expected" ]]; then
        echo -e "${RED}Error: Checksum verification failed for ${ASSET_NAME}.${NC}" >&2
        exit 1
    fi
}

choose_existing_data_action() {
    if [[ ! -e "$UPSTREAM_DIR" ]]; then
        echo "new"
        return 0
    fi

    if [[ ! -d "$UPSTREAM_DIR" ]]; then
        echo -e "${RED}Error: '$UPSTREAM_DIR' exists but is not a directory.${NC}" >&2
        exit 1
    fi

    case "${UPSTREAM_EXISTING_DATA:-}" in
    keep | replace)
        echo "$UPSTREAM_EXISTING_DATA"
        return 0
        ;;
    "")
        ;;
    *)
        echo -e "${RED}Error: UPSTREAM_EXISTING_DATA must be 'keep' or 'replace'.${NC}" >&2
        exit 1
        ;;
    esac

    if ! { : </dev/tty; } 2>/dev/null; then
        echo -e "${YELLOW}Existing '$UPSTREAM_DIR' found; no TTY available, keeping it.${NC}" >&2
        echo "keep"
        return 0
    fi

    while true; do
        printf "%bExisting '%s' found. Keep it and refresh hooks, or replace it? [K/r] %b" "$YELLOW" "$UPSTREAM_DIR" "$NC" >/dev/tty
        read -r answer </dev/tty
        case "${answer:-keep}" in
        keep | Keep | KEEP | k | K | "")
            echo "keep"
            return 0
            ;;
        replace | Replace | REPLACE | r | R)
            echo "replace"
            return 0
            ;;
        *)
            echo "Please answer 'keep' or 'replace'." >/dev/tty
            ;;
        esac
    done
}

run_upstream() {
    echo -e "${YELLOW}Running:${NC} upstream $*"
    if ! "$TMP_FILE" "$@"; then
        echo -e "${RED}Error: Command failed: upstream $*${NC}"
        rm -rf "$TMP_DIR"
        exit 1
    fi
}

upstream_package_installed() {
    "$TMP_FILE" list upstream --json >/dev/null 2>&1
}

install_upstream_if_missing() {
    if upstream_package_installed; then
        echo -e "${GREEN}Managed upstream package already present; skipping package install.${NC}"
    else
        run_upstream --yes install what386/upstream-rs upstream -k binary
    fi
}

main() {
    echo -e "${GREEN}Starting installation...${NC}"

    ARCH=$(detect_arch)

    case "$ARCH" in
    x86_64 | aarch64) ;;
    *)
        echo -e "${RED}Error: Unsupported architecture: $(uname -m)${NC}" >&2
        exit 1
        ;;
    esac

    echo "Detected Architecture: $ARCH"

    ASSET_NAME="${BINARY_NAME}-${ARCH}-${OS}"
    DOWNLOAD_URL="https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases/latest/download/${ASSET_NAME}"
    CHECKSUM_URL="https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases/latest/download/SHA256SUMS.txt"

    echo "Downloading from: $DOWNLOAD_URL"

    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TMP_DIR"' EXIT
    TMP_FILE="${TMP_DIR}/${BINARY_NAME}"
    CHECKSUM_FILE="${TMP_DIR}/SHA256SUMS.txt"

    download_file "$DOWNLOAD_URL" "$TMP_FILE"
    download_file "$CHECKSUM_URL" "$CHECKSUM_FILE"
    verify_checksum

    chmod +x "$TMP_FILE"

    echo -e "${GREEN}Download complete!${NC}"

    existing_action="$(choose_existing_data_action)"
    if [[ "$existing_action" == "replace" ]]; then
        echo -e "${YELLOW}Removing existing '$UPSTREAM_DIR'...${NC}"
        rm -rf "$UPSTREAM_DIR"
    elif [[ "$existing_action" == "keep" ]]; then
        echo -e "${GREEN}Keeping existing '$UPSTREAM_DIR'.${NC}"
    fi

    run_upstream hooks init
    install_upstream_if_missing

    echo -e "${GREEN}Installation complete!${NC}"
}

main "$@"
