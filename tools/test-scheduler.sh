#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERIAL_LOG="${TMPDIR:-/tmp}/smallix-sched-serial.log"

"$ROOT_DIR/tools/mkiso.sh" >/dev/null
rm -f "$SERIAL_LOG"

set +e
timeout 12s "$ROOT_DIR/tools/qemu-safe.sh" \
  -m 256M \
  -cdrom "$ROOT_DIR/out/smallix.iso" \
  -boot d \
  -no-reboot \
  -display none \
  -serial "file:$SERIAL_LOG" \
  -monitor none >/dev/null 2>&1
rc=$?
set -e

if ! grep -q "scheduler self-test PASS" "$SERIAL_LOG"; then
  echo "scheduler test FAILED"
  echo "--- serial log ---"
  cat "$SERIAL_LOG"
  exit 1
fi

if ! grep -q "posix syscall test PASS" "$SERIAL_LOG"; then
  echo "posix syscall test FAILED"
  echo "--- serial log ---"
  cat "$SERIAL_LOG"
  exit 1
fi

echo "scheduler+posix test PASS (qemu exit code: $rc)"
