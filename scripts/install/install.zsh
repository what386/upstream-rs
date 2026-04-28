#!/usr/bin/env zsh
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

GITHUB_USER="what386"
GITHUB_REPO="upstream-rs"
BINARY_NAME="upstream-rs"
OS="apple-darwin"

INSTALL_COMMANDS=(
  "hooks init"
  "install upstream what386/upstream-rs -k binary"
)

install_completion() {
  local helper_url helper_script
  helper_url="https://raw.githubusercontent.com/${GITHUB_USER}/${GITHUB_REPO}/main/scripts/install/completions.sh"
  helper_script="${TMP_DIR}/completions.sh"

  echo -e "${YELLOW}Installing zsh completion...${NC}"

  if command -v curl &>/dev/null; then
    if ! curl -fsSL "$helper_url" -o "$helper_script"; then
      echo -e "${YELLOW}Warning: Failed to download completion installer from ${helper_url}${NC}"
      return
    fi
  elif command -v wget &>/dev/null; then
    if ! wget -q "$helper_url" -O "$helper_script"; then
      echo -e "${YELLOW}Warning: Failed to download completion installer from ${helper_url}${NC}"
      return
    fi
  else
    echo -e "${YELLOW}Warning: Neither curl nor wget found; skipping completion install.${NC}"
    return
  fi

  if ! chmod +x "$helper_script"; then
    echo -e "${YELLOW}Warning: Failed to make completion installer executable${NC}"
    return
  fi

  if ! "$helper_script" zsh; then
    echo -e "${YELLOW}Warning: Completion installer failed for zsh${NC}"
    return
  fi

  echo -e "${GREEN}Zsh completion installed${NC}"
}

detect_arch() {
  case "$(uname -m)" in
    x86_64|amd64) echo "x86_64" ;;
    aarch64|arm64) echo "aarch64" ;;
    armv7l) echo "armv7" ;;
    i386|i686) echo "i686" ;;
    *) echo "unknown" ;;
  esac
}

main() {
  echo -e "${GREEN}Starting installation...${NC}"

  ARCH="$(detect_arch)"

  if [ "$ARCH" = "unknown" ]; then
    echo -e "${RED}Error: Unsupported architecture ($ARCH)${NC}"
    exit 1
  fi

  echo "Detected OS: $OS"
  echo "Detected Architecture: $ARCH"

  DOWNLOAD_URL="https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases/latest/download/${BINARY_NAME}-${ARCH}-${OS}"
  echo "Downloading from: $DOWNLOAD_URL"

  TMP_DIR="$(mktemp -d)"
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

  total="${#INSTALL_COMMANDS[@]}"
  current=0
  for cmd in "${INSTALL_COMMANDS[@]}"; do
    current=$((current + 1))
    echo -e "${YELLOW}Running command ${current}/${total}: ${NC}$cmd"

    if ! "$TMP_FILE" $cmd; then
      echo -e "${RED}Error: Command failed: $cmd${NC}"
      rm -rf "$TMP_DIR"
      exit 1
    fi
  done

  # Best-effort completion setup; do not fail installation if it cannot be configured.
  install_completion

  rm -rf "$TMP_DIR"
  echo -e "${GREEN}Installation complete!${NC}"
}

main "$@"
