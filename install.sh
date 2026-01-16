#!/usr/bin/env bash
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

GITHUB_USER="what386"
GITHUB_REPO="upstream-rs"
BINARY_NAME="upstream-rs"

INSTALL_COMMANDS=(
    "init"
    "install upstream what386/upstream-rs -k binary"
)

detect_os() {
    case "$(uname -s)" in
    Linux*) echo "unknown-linux-gnu" ;;
    Darwin*) echo "apple-darwin" ;;
    CYGWIN* | MINGW* | MSYS*) echo "windows" ;;
    *) echo "unknown" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
    x86_64 | amd64) echo "x86_64" ;;
    aarch64 | arm64) echo "aarch64" ;;
    armv7l) echo "armv7" ;;
    i386 | i686) echo "i686" ;;
    *) echo "unknown" ;;
    esac
}

main() {
    echo -e "${GREEN}Starting installation...${NC}"

    OS=$(detect_os)
    ARCH=$(detect_arch)

    if [ "$OS" = "unknown" ] || [ "$ARCH" = "unknown" ]; then
        echo -e "${RED}Error: Unsupported OS ($OS) or architecture ($ARCH)${NC}"
        exit 1
    fi

    echo "Detected OS: $OS"
    echo "Detected Architecture: $ARCH"

    DOWNLOAD_URL="https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases/latest/download/${BINARY_NAME}-${ARCH}-${OS}"

    # Add .exe extension for Windows
    if [ "$OS" = "windows" ]; then
        DOWNLOAD_URL="${DOWNLOAD_URL}.exe"
    fi

    echo "Downloading from: $DOWNLOAD_URL"

    TMP_DIR=$(mktemp -d)
    TMP_FILE="${TMP_DIR}/${BINARY_NAME}"

    if command -v curl &>/dev/null; then
        curl -fsSL "$DOWNLOAD_URL" -o "$TMP_FILE"
    elif command -v wget &>/dev/null; then
        wget -q "$DOWNLOAD_URL" -O "$TMP_FILE"
    else
        echo -e "${RED}Error: Neither curl nor wget found. Please install one.${NC}"
        exit 1
    fi

    chmod +x "$TMP_FILE"

    echo -e "${GREEN}Download complete!${NC}"

    for i in "${!INSTALL_COMMANDS[@]}"; do
        cmd="${INSTALL_COMMANDS[$i]}"
        echo -e "${YELLOW}Running command $((i + 1))/${#INSTALL_COMMANDS[@]}: ${NC}$cmd"

        if ! $TMP_FILE $cmd; then
            echo -e "${RED}Error: Command failed: $cmd${NC}"
            rm -rf "$TMP_DIR"
            exit 1
        fi
    done

    # Cleanup
    rm -rf "$TMP_DIR"

    echo -e "${GREEN}Installation complete!${NC}"
}

main "$@"
