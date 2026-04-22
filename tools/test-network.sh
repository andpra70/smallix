#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERIAL_LOG="${TMPDIR:-/tmp}/smallix-net-serial.log"

"$ROOT_DIR/tools/mkiso.sh" >/dev/null
rm -f "$SERIAL_LOG"

set +e
timeout 15s "$ROOT_DIR/tools/qemu-safe.sh" \
  -m 256M \
  -cdrom "$ROOT_DIR/out/smallix.iso" \
  -boot d \
  -no-reboot \
  -display none \
  -serial "file:$SERIAL_LOG" \
  -monitor none \
  -netdev user,id=n0 \
  -device rtl8139,netdev=n0 >/dev/null 2>&1
rc=$?
set -e

if ! grep -q "net ping gateway PASS" "$SERIAL_LOG"; then
  echo "network ping test FAILED"
  cat "$SERIAL_LOG"
  exit 1
fi

echo "network ping test PASS (qemu exit code: $rc)"
