#!/usr/bin/env bash
set -euo pipefail

if command -v qemu-system-x86_64 >/dev/null 2>&1; then
  QEMU_BIN="qemu-system-x86_64"
elif command -v qemu-system-i386 >/dev/null 2>&1; then
  QEMU_BIN="qemu-system-i386"
else
  echo "qemu-system-x86_64 or qemu-system-i386 not found"
  exit 1
fi

# Avoid running host QEMU with a polluted linker/runtime env (common in snap shells).
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
  "$QEMU_BIN" "$@"
