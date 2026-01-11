#!/bin/bash

set -euo pipefail

echo "Switching to main branch..."
git checkout main
git pull origin main

echo "Merging dev into main..."
git merge dev -m "Merge dev into main"

VERSION=$(grep '^version' Cargo.toml | head -n 1 | awk -F\" '{print $2}')
if [ -z "$VERSION" ]; then
    echo "Error: Could not find version in Cargo.toml"
    exit 1
fi
echo "Version from Cargo.toml: $VERSION"

echo "Creating git tag: v$VERSION"
git tag -a "v$VERSION" -m "Release version $VERSION"

echo "Pushing tag to origin..."
git push origin "v$VERSION"
