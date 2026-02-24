#!/usr/bin/env bash
declare -a __lash_argv=("$@")
set -euo pipefail
if [[ ${#__lash_argv[@]} == 0 ]] || (( ${#__lash_argv[@]} > 1 )); then
    echo "Invalid version"
    exit 1
fi
readonly version=${__lash_argv[0]}
tally semver $version --auto
tally changelog > CHANGELOG.md
git checkout main
echo "Merging dev into main..."
git merge dev -m "Merge dev into main"
echo "Pushing main branch to remotes..."
git push github main
git push gitea main
git tag $version
git push github $version
cargo publish
