#!/bin/bash
set -euo pipefail

# Smoke-test every tool in the installed sysmon snap under strict confinement.
# Usage:
#   ./snap-e2e.sh              # build, install, test all
#   ./snap-e2e.sh --no-build   # assume snap already installed, just test
#
# Exits non-zero if any tool prints an error pattern to stderr within RUN_SECS.

SNAP_NAME="sysmon"
SNAP_FILE="${SNAP_NAME}_4.0.0_amd64.snap"
RUN_SECS="${RUN_SECS:-8}"
LOG_DIR="${LOG_DIR:-/tmp/sysmon-e2e}"

SKIP_BUILD=false
if [[ "${1:-}" == "--no-build" ]]; then
    SKIP_BUILD=true
fi

ERROR_PATTERNS=(
    "fetch error"
    "ENOENT"
    "No such file"
    "Permission denied"
    "operation timed out"
    "NVML init failed"
    "failed to"
    "panicked at"
)

TOOLS=(cpu gpu ram dio net audio poly astro)

mkdir -p "$LOG_DIR"

if [[ "$SKIP_BUILD" == false ]]; then
    echo "==> cargo build --release"
    cargo build --release
    echo "==> snapcraft pack --destructive-mode"
    snapcraft pack --destructive-mode
    echo "==> snap install --dangerous"
    sudo snap install "$SNAP_FILE" --dangerous
fi

echo "==> connecting interfaces"
for plug in system-observe hardware-observe opengl network network-observe process-control audio-playback audio-record home; do
    sudo snap connect "${SNAP_NAME}:${plug}" 2>/dev/null || true
done

FAILURES=()

run_tool() {
    local app="$1"
    local log="$LOG_DIR/${app}.log"
    local cmd="${SNAP_NAME}.${app}"

    echo "==> ${cmd} (${RUN_SECS}s)"
    # Feed 'q' on stdin so the TUI exits cleanly; timeout guards against hangs.
    timeout --preserve-status -s TERM "$RUN_SECS" "$cmd" </dev/null >/dev/null 2>"$log" || true

    for pat in "${ERROR_PATTERNS[@]}"; do
        if grep -qi "$pat" "$log"; then
            echo "   FAIL: matched '$pat' in stderr"
            echo "   --- ${log} ---"
            sed 's/^/   | /' "$log"
            FAILURES+=("${app}:${pat}")
            return
        fi
    done
    echo "   ok"
}

for app in "${TOOLS[@]}"; do
    run_tool "$app"
done

echo "==> sysmon (compositor)"
COMP_LOG="$LOG_DIR/compositor.log"
timeout --preserve-status -s TERM "$RUN_SECS" sysmon --cpu --ram --net </dev/null >/dev/null 2>"$COMP_LOG" || true
for pat in "${ERROR_PATTERNS[@]}"; do
    if grep -qi "$pat" "$COMP_LOG"; then
        echo "   FAIL: matched '$pat' in compositor stderr"
        sed 's/^/   | /' "$COMP_LOG"
        FAILURES+=("compositor:${pat}")
        break
    fi
done

echo ""
if [[ ${#FAILURES[@]} -gt 0 ]]; then
    echo "E2E FAILED (${#FAILURES[@]}):"
    printf '  - %s\n' "${FAILURES[@]}"
    exit 1
fi
echo "E2E PASSED: all ${#TOOLS[@]} tools + compositor launched cleanly under strict confinement"
