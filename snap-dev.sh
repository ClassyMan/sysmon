#!/bin/bash
set -euo pipefail

# Build, install, and test a snap locally.
# Usage:
#   ./snap-dev.sh              # build + install all, run astro
#   ./snap-dev.sh <app>        # build + install all, run sysmon-tools.<app>
#   ./snap-dev.sh --install    # skip cargo build, just re-snap + install

APP="${1:-astro}"
SKIP_CARGO=false
if [[ "$APP" == "--install" ]]; then
    SKIP_CARGO=true
    APP="${2:-astro}"
fi

SNAP_NAME="sysmon"
SNAP_FILE="${SNAP_NAME}_4.0.0_amd64.snap"

# 1. Cargo build (release)
if [[ "$SKIP_CARGO" == false ]]; then
    echo "==> cargo build --release"
    cargo build --release
fi

# 2. Snap pack
echo "==> snapcraft --destructive-mode"
snapcraft pack --destructive-mode

# 3. Install
echo "==> snap install (--dangerous)"
sudo snap install "$SNAP_FILE" --dangerous

# 4. Connect interfaces
echo "==> connecting interfaces"
for plug in system-observe hardware-observe network network-observe process-control audio-playback audio-record home; do
    sudo snap connect "${SNAP_NAME}:${plug}" 2>/dev/null || true
done

# 5. Run
# When app name matches snap name, snap exposes it as just the snap name
if [[ "$APP" == "sysmon" ]]; then
    RUN_CMD="sysmon"
else
    RUN_CMD="${SNAP_NAME}.${APP}"
fi
echo "==> running ${RUN_CMD}  (stderr -> /tmp/${APP}_snap.log)"
"${RUN_CMD}" 2>"/tmp/${APP}_snap.log"

echo ""
echo "--- stderr log ---"
cat "/tmp/${APP}_snap.log"
