#!/usr/bin/env bash
#
# safe-update.sh — Update Cargo dependencies with a cool-off period.
#
# Only upgrades to versions published at least COOLOFF_DAYS ago (default 7).
# This avoids pulling in freshly-published versions that could be compromised
# (supply chain attacks like the axios incident).
#
# Usage:
#   ./scripts/safe-update.sh          # dry run (default)
#   ./scripts/safe-update.sh --apply  # apply safe updates
#   COOLOFF_DAYS=14 ./scripts/safe-update.sh  # custom cool-off

set -euo pipefail

COOLOFF_DAYS="${COOLOFF_DAYS:-7}"
APPLY=false
if [[ "${1:-}" == "--apply" ]]; then
    APPLY=true
fi

COOLOFF_SECS=$((COOLOFF_DAYS * 86400))
NOW=$(date +%s)

cd "$(git rev-parse --show-toplevel)"

echo "=== Vulnerability scan ==="
cargo audit || true
echo ""

echo "=== Checking for updates (${COOLOFF_DAYS}-day cool-off) ==="

# Get the list of crates that would be updated
UPDATE_OUTPUT=$(cargo update --dry-run 2>&1) || true

# Parse lines like: "Updating crate_name v0.1.0 -> v0.2.0"
UPDATES=$(echo "$UPDATE_OUTPUT" | grep -E '^\s*(Updating|Adding)' | grep -v 'crates\.io index' | sed 's/^\s*//' || true)

if [[ -z "$UPDATES" ]]; then
    echo "All dependencies are up to date."
    exit 0
fi

SAFE_CRATES=()
BLOCKED_CRATES=()
FAILED_CRATES=()

while IFS= read -r line; do
    [[ -z "$line" ]] && continue

    # Extract crate name and target version
    # Format: "Updating crate_name v0.1.0 -> v0.2.0"
    #     or: "Adding crate_name v0.1.0"
    CRATE=$(echo "$line" | awk '{print $2}')
    if echo "$line" | grep -qF -- '->'; then
        VERSION=$(echo "$line" | awk '{print $NF}' | sed 's/^v//')
    else
        VERSION=$(echo "$line" | awk '{print $3}' | sed 's/^v//')
    fi

    # Query crates.io for the version's publication date
    RESP=$(curl -sf -H "User-Agent: sysmon-safe-update" \
        "https://crates.io/api/v1/crates/${CRATE}/${VERSION}" 2>/dev/null || echo "FETCH_FAILED")

    if [[ "$RESP" == "FETCH_FAILED" ]]; then
        FAILED_CRATES+=("$CRATE@$VERSION (could not query crates.io)")
        continue
    fi

    CREATED_AT=$(echo "$RESP" | jq -r '.version.created_at // empty' 2>/dev/null || true)

    if [[ -z "$CREATED_AT" ]]; then
        FAILED_CRATES+=("$CRATE@$VERSION (no publish date found)")
        continue
    fi

    PUBLISH_TS=$(date -d "$CREATED_AT" +%s 2>/dev/null || true)
    if [[ -z "$PUBLISH_TS" ]]; then
        FAILED_CRATES+=("$CRATE@$VERSION (could not parse date: $CREATED_AT)")
        continue
    fi

    AGE_SECS=$((NOW - PUBLISH_TS))
    AGE_DAYS=$((AGE_SECS / 86400))

    if [[ $AGE_SECS -ge $COOLOFF_SECS ]]; then
        SAFE_CRATES+=("$CRATE@$VERSION (${AGE_DAYS}d old)")
    else
        BLOCKED_CRATES+=("$CRATE@$VERSION (${AGE_DAYS}d old, need ${COOLOFF_DAYS}d)")
    fi

    # Rate-limit crates.io queries
    sleep 0.2
done <<< "$UPDATES"

echo ""

if [[ ${#SAFE_CRATES[@]} -gt 0 ]]; then
    echo "Safe to update (>= ${COOLOFF_DAYS}d old):"
    for c in "${SAFE_CRATES[@]}"; do
        echo "  ✓ $c"
    done
fi

if [[ ${#BLOCKED_CRATES[@]} -gt 0 ]]; then
    echo ""
    echo "Blocked (too new):"
    for c in "${BLOCKED_CRATES[@]}"; do
        echo "  ✗ $c"
    done
fi

if [[ ${#FAILED_CRATES[@]} -gt 0 ]]; then
    echo ""
    echo "Could not check:"
    for c in "${FAILED_CRATES[@]}"; do
        echo "  ? $c"
    done
fi

if [[ ${#SAFE_CRATES[@]} -eq 0 ]]; then
    echo ""
    echo "No updates passed the cool-off period."
    exit 0
fi

echo ""

if [[ "$APPLY" == true ]]; then
    echo "=== Applying safe updates ==="
    for entry in "${SAFE_CRATES[@]}"; do
        CRATE=$(echo "$entry" | cut -d@ -f1)
        cargo update -p "$CRATE" 2>&1 | grep -v "^\s*$" || true
    done

    echo ""
    echo "=== Running tests ==="
    cargo test --workspace

    echo ""
    echo "=== Post-update vulnerability scan ==="
    cargo audit || true

    echo ""
    echo "Done. Review changes with: git diff Cargo.lock"
else
    echo "Dry run complete. Run with --apply to update."
fi
