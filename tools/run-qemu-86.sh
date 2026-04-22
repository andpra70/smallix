#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO_FILE="$ROOT_DIR/out/smallix.iso"

"$ROOT_DIR/tools/mkiso.sh"

if ! command -v qemu-system-i386 >/dev/null 2>&1; then
  echo "qemu-system-i386 not found"
  exit 1
fi

# Interactive serial console on the current terminal.
exec env \
  -u LD_PRELOAD \
  -u LD_LIBRARY_PATH \
  -u LD_AUDIT \
  -u LD_DEBUG \
  -u LD_ORIGIN_PATH \
  -u LD_PROFILE \
  -u LD_USE_LOAD_BIAS \
  -u LD_ASSUME_KERNEL \
  -u SNAP_LIBRARY_PATH \
  -u GTK_PATH \
  -u LOCPATH \
  qemu-system-i386 \
  -m 256M \
  -serial stdio \
  -cdrom "$ISO_FILE" \
  -boot d \
  -no-reboot
