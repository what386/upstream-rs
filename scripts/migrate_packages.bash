#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: scripts/migrate_packages.bash [packages.json] [defaults-json]

Adds missing fields to package records in packages.json using default values.
Existing fields are preserved.

Behavior:
  - Creates a backup: <packages.json>.bak
  - Rewrites file in place using jq (preferred) or yq (fallback)

Defaults JSON:
  - Optional 2nd argument; must be a JSON object.
  - Default: {"install_type":"release"}
  - Applied per item in the packages array as: (defaults * item)

Examples:
  scripts/migrate_packages.bash
  scripts/migrate_packages.bash ~/.upstream/metadata/packages.json \
    '{"install_type":"release","is_pinned":false}'
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

PACKAGES_FILE="${1:-$HOME/.upstream/metadata/packages.json}"
DEFAULTS_JSON="${2:-{\"install_type\":\"release\"}}"

if [[ ! -f "$PACKAGES_FILE" ]]; then
    echo "Error: packages file not found: $PACKAGES_FILE" >&2
    exit 1
fi

if [[ ! -s "$PACKAGES_FILE" ]]; then
    echo "No changes: file is empty: $PACKAGES_FILE"
    exit 0
fi

TMP_FILE="$(mktemp "${TMPDIR:-/tmp}/upstream-packages-migrate.XXXXXX")"
trap 'rm -f "$TMP_FILE"' EXIT

if command -v jq >/dev/null 2>&1; then
    jq --argjson defaults "$DEFAULTS_JSON" '
        ($defaults | type) as $dtype
        | if $dtype != "object" then
            error("defaults-json must be a JSON object")
          else .
          end
        | if type == "array" then
            map($defaults * .)
          else
            error("packages.json must be a JSON array")
          end
    ' "$PACKAGES_FILE" > "$TMP_FILE"
elif command -v yq >/dev/null 2>&1; then
    DEFAULTS_JSON="$DEFAULTS_JSON" yq eval -o=json 'map((env(DEFAULTS_JSON)) * .)' "$PACKAGES_FILE" > "$TMP_FILE"
else
    echo "Error: neither 'jq' nor 'yq' is installed." >&2
    exit 1
fi

cp "$PACKAGES_FILE" "${PACKAGES_FILE}.bak"
mv "$TMP_FILE" "$PACKAGES_FILE"
trap - EXIT

echo "Updated: $PACKAGES_FILE"
echo "Backup : ${PACKAGES_FILE}.bak"
