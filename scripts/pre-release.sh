#!/usr/bin/env bash
#
# pre-release.sh — Run before tagging a release.
#
# Checks:
#   1. No known vulnerabilities (cargo audit)
#   2. All tests pass
#   3. Release build succeeds
#
# Exit code 0 = safe to release, non-zero = fix issues first.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "=== 1/3 Vulnerability scan ==="
if ! cargo audit; then
    echo ""
    echo "FAIL: Vulnerabilities found. Fix before releasing."
    exit 1
fi
echo "PASS"
echo ""

echo "=== 2/3 Tests ==="
if ! cargo test --workspace; then
    echo ""
    echo "FAIL: Tests failed."
    exit 1
fi
echo "PASS"
echo ""

echo "=== 3/3 Release build ==="
if ! cargo build --release; then
    echo ""
    echo "FAIL: Release build failed."
    exit 1
fi
echo "PASS"
echo ""

echo "All checks passed. Safe to release."
