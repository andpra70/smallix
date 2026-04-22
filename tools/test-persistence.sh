#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO_FILE="$ROOT_DIR/out/smallix.iso"
IMG_FILE="$ROOT_DIR/out/persist-hda.img"
LOG1="${TMPDIR:-/tmp}/smallix-persist-serial-1.log"
LOG2="${TMPDIR:-/tmp}/smallix-persist-serial-2.log"

"$ROOT_DIR/tools/mkiso.sh" >/dev/null
rm -f "$LOG1" "$LOG2"
truncate -s 16M "$IMG_FILE"

# First boot seeds the persistence marker on /dev/hda.
timeout 15s "$ROOT_DIR/tools/qemu-safe.sh" \
  -m 256M \
  -cdrom "$ISO_FILE" \
  -boot d \
  -display none \
  -serial "file:$LOG1" \
  -monitor none \
  -netdev user,id=n0,guestfwd=tcp:10.0.2.100:2323-cmd:/bin/cat \
  -device rtl8139,netdev=n0 \
  -drive file="$IMG_FILE",format=raw,if=ide \
  -no-reboot >/dev/null 2>&1 || true

if ! grep -q "hda persistence SEED" "$LOG1" && ! grep -q "hda persistence PASS" "$LOG1"; then
  echo "persistence seed phase FAILED"
  cat "$LOG1"
  exit 1
fi

# Second boot must observe the marker created in first boot.
timeout 15s "$ROOT_DIR/tools/qemu-safe.sh" \
  -m 256M \
  -cdrom "$ISO_FILE" \
  -boot d \
  -display none \
  -serial "file:$LOG2" \
  -monitor none \
  -netdev user,id=n0,guestfwd=tcp:10.0.2.100:2323-cmd:/bin/cat \
  -device rtl8139,netdev=n0 \
  -drive file="$IMG_FILE",format=raw,if=ide \
  -no-reboot >/dev/null 2>&1 || true

if grep -q "hda persistence PASS" "$LOG2"; then
  echo "hda persistence test PASS"
  exit 0
fi

echo "hda persistence test FAILED"
cat "$LOG2"
exit 1
