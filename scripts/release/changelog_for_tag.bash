#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <tag>" >&2
    exit 1
fi

tag="$1"
version="${tag#v}"
changelog_file="CHANGELOG.md"

if [[ ! -f "$changelog_file" ]]; then
    echo "Missing $changelog_file" >&2
    exit 1
fi

if awk -v version="$version" '
    BEGIN {
        in_section = 0
        found = 0
    }
    /^## / {
        if (in_section) {
            exit
        }
        if ($1 == "##" && $2 == version) {
            in_section = 1
            found = 1
        }
    }
    in_section {
        print
    }
    END {
        if (!found) {
            exit 2
        }
    }
' "$changelog_file"; then
    :
else
    status=$?
    if [[ $status -eq 2 ]]; then
        echo "No changelog section found for tag '$tag' (version '$version')" >&2
    fi
    exit "$status"
fi
