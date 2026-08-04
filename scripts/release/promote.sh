#!/usr/bin/env bash
set -euo pipefail

readonly RED="\033[0;31m"
readonly GREEN="\033[0;32m"
readonly BLUE="\033[0;34m"
readonly NC="\033[0m"

if [[ ! -r .release-state ]] || [[ "$(<.release-state)" != "prepared" ]]; then
    echo -e "${RED}Invalid release state: should be 'prepared'${NC}"
    exit 1
fi

if [[ "$(git branch --show-current)" != "dev" ]]; then
    echo -e "${RED}Not on dev branch${NC}"
    exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
    echo -e "${RED}Working tree must be clean before promoting a release.${NC}"
    exit 1
fi

just verify-release

echo -e "${BLUE}Pushing dev to remotes...${NC}"

git push github dev
git push gitea dev

echo -e "${BLUE}Merging dev into main...${NC}"

git switch main
git merge dev -m "Merge dev into main"

echo -e "${BLUE}Pushing main to remotes...${NC}"

git push github main
git push gitea main

printf "promoted" > .release-state

echo -e "${GREEN}Promoted dev to main.${NC}"
