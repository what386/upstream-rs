#!/usr/bin/env bash
set -euo pipefail

readonly RED="\033[0;31m"
readonly GREEN="\033[0;32m"
readonly BLUE="\033[0;34m"
readonly NC="\033[0m"

if [[ $# == 0 ]] || (( $# > 1 )); then
    echo -e "${RED}Usage: prepare <version>${NC}"
    exit 1
fi

version="${1}"

cargo fmt

git add src/
git commit -m "cargo fmt" || true

if [[ "$(git branch --show-current)" != "dev" ]]; then
    echo -e "${RED}Not on dev branch${NC}"
    exit 1
fi

tally semver "${version}"

if [[ "$(tally list --released "${version}")" == "No released tasks found." ]]; then
    echo -e "${RED}No completed tasks for version ${version}.${NC}"
    exit 1
fi

just gen-completions

git add ./completions
git commit -m "Release ${version}: Update shell completions" || true

echo -e "${GREEN}Release ${version} prepared.${NC}"
