#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_FILE="$ROOT_DIR/out/test-fat32.log"
ISO_FILE="$ROOT_DIR/out/smallix.iso"
FAT_IMG="$ROOT_DIR/out/fat32.img"

"$ROOT_DIR/tools/mkiso.sh" >/dev/null
"$ROOT_DIR/tools/mkfat32.sh" >/dev/null

set +e
timeout 12s "$ROOT_DIR/tools/qemu-safe.sh" \
  -m 256M \
  -display none \
  -serial stdio \
  -netdev user,id=n0,guestfwd=tcp:10.0.2.100:2323-cmd:/bin/cat \
  -device rtl8139,netdev=n0 \
  -cdrom "$ISO_FILE" \
  -boot d \
  -drive file="$FAT_IMG",if=ide,format=raw \
  -no-reboot \
  >"$LOG_FILE" 2>&1
rc=$?
set -e

if grep -q "hda fat32 detected, skip native persistence marker" "$LOG_FILE"; then
  echo "fat32 mount detect test PASS (qemu exit code: $rc)"
  exit 0
fi

echo "fat32 mount detect test FAILED"
tail -n 120 "$LOG_FILE"
exit 1
