#!/bin/bash

set -euo pipefail

echo "Updating dev branch..."
git checkout dev
git pull github dev
git pull gitea dev
git push github dev
git push gitea dev

echo "Switching to main branch..."
git checkout main
git pull github main
git pull gitea main

echo "Merging dev into main..."
git merge dev -m "Merge dev into main"

echo "Pushing main branch to remotes..."
git push github main
git push gitea main

VERSION=$(grep '^version' Cargo.toml | head -n 1 | awk -F\" '{print $2}')
if [ -z "$VERSION" ]; then
    echo "Error: Could not find version in Cargo.toml"
    exit 1
fi
echo "Version from Cargo.toml: $VERSION"

echo "Creating git tag: v$VERSION"
tally tag "v$VERSION"
echo "Pushing tag to github..."
git push github "v$VERSION"

git checkout dev

echo "Done! Merged dev into main and created tag v$VERSION."
