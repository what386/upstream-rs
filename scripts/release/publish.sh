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

if [[ ! -r .release-state ]] || [[ "$(<.release-state)" != "promoted" ]]; then
    echo -e "${RED}Invalid release state: should be 'promoted'${NC}"
    exit 1
fi

if [[ "$(git branch --show-current)" != "main" ]]; then
    echo -e "${RED}Not on main branch${NC}"
    exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
    echo -e "${RED}Working tree must be clean before publishing a release.${NC}"
    exit 1
fi

cargo_version="$(awk '$0 == "[package]" { package = 1; next } package && /^\[/ { exit } package && /^version = / { gsub(/version = |\"/, ""); print; exit }' Cargo.toml)"
if [[ "${version#v}" != "$cargo_version" ]]; then
    echo -e "${RED}Version ${version} does not match Cargo.toml version ${cargo_version}.${NC}"
    exit 1
fi

if [[ "$(git tag --list "${version}")" != "" ]]; then
    echo -e "${RED}Tag ${version} already exists.${NC}"
    exit 1
fi

cargo publish --dry-run --locked

git tag "${version}"

echo -e "${BLUE}Publishing release on GitHub...${NC}"
git push github "${version}"
echo -e "${GREEN}Published on GitHub${NC}"

echo -e "${BLUE}Publishing on crates.io...${NC}"
cargo publish --locked
echo -e "${GREEN}Published on crates.io${NC}"

printf "published" > .release-state

echo -e "${GREEN}${version} published successfully.${NC}"
