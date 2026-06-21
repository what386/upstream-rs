#!/usr/bin/env bash
set -euo pipefail

readonly RED="\033[0;31m"
readonly GREEN="\033[0;32m"
readonly BLUE="\033[0;34m"
readonly NC="\033[0m"

if [[ $# == 0 ]] || (( $# > 1 )); then
    echo -e "${RED}Usage: publish <version>${NC}"
    exit 1
fi

version="${1}"

if [[ "$(git branch --show-current)" != "main" ]]; then
    echo -e "${RED}Not on main branch${NC}"
    exit 1
fi

if [[ "$(git tag --list "${version}")" != "" ]]; then
    echo -e "${RED}Tag ${version} already exists.${NC}"
    exit 1
fi

git tag "${version}"

echo -e "${BLUE}Publishing release on GitHub...${NC}"
git push github "${version}"
echo -e "${GREEN}Published on GitHub${NC}"

echo -e "${BLUE}Publishing on crates.io...${NC}"
cargo publish
echo -e "${GREEN}Published on crates.io${NC}"

echo -e "${GREEN}${version} published successfully.${NC}"
