#!/usr/bin/env bash
set -euo pipefail

readonly RED="\033[0;31m"
readonly GREEN="\033[0;32m"
readonly NC="\033[0m"

if [[ $# == 0 ]] || (( $# > 1 )); then
    echo -e "${RED}Usage: prepare <version>${NC}"
    exit 1
fi

version="${1}"

if [[ ! -r .release-state ]] || [[ "$(<.release-state)" != "ready" ]]; then
    echo -e "${RED}Invalid release state: should be 'ready'${NC}"
    exit 1
fi

if [[ "$(git branch --show-current)" != "dev" ]]; then
    echo -e "${RED}Not on dev branch${NC}"
    exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
    echo -e "${RED}Working tree must be clean before preparing a release.${NC}"
    exit 1
fi

cargo_version="$(awk '$0 == "[package]" { package = 1; next } package && /^\[/ { exit } package && /^version = / { gsub(/version = |\"/, ""); print; exit }' Cargo.toml)"
if [[ "${version#v}" != "$cargo_version" ]]; then
    echo -e "${RED}Version ${version} does not match Cargo.toml version ${cargo_version}.${NC}"
    exit 1
fi

cargo publish --dry-run --locked

cargo fmt
cargo spaced

git add src/
if ! git diff --cached --quiet; then
    git commit -m "formatting"
fi

tally semver "${version}"

if [[ "$(tally list --released "${version}")" == "No released tasks found." ]]; then
    echo -e "${RED}No completed tasks for version ${version}.${NC}"
    exit 1
fi

git add CHANGELOG.md TODO.md
if ! git diff --cached --quiet; then
    git commit -m "Update changelog for release ${version}"
fi

just gen-completions

git add ./completions
if ! git diff --cached --quiet; then
    git commit -m "Release ${version}: Update shell completions"
fi

printf "prepared" > .release-state

echo -e "${GREEN}Release ${version} prepared.${NC}"
