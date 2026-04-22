#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ISO_FILE="$ROOT_DIR/out/smallix.iso"
HDA_FILE="$ROOT_DIR/out/persist-hda.img"

# 1) Build kernel and bootable ISO.
"$ROOT_DIR/tools/mkiso.sh"

# 2) Create persistent HDA once; keep it across runs.
if [[ ! -f "$HDA_FILE" ]]; then
  echo "Creating persistent HDA image: $HDA_FILE"
  "$ROOT_DIR/tools/mkfat32.sh"
  cp "$ROOT_DIR/out/fat32.img" "$HDA_FILE"
fi

exec "$ROOT_DIR/tools/qemu-safe.sh" \
  -m 256M \
  -serial stdio \
  -netdev user,id=n0,guestfwd=tcp:10.0.2.100:2323-cmd:/bin/cat \
  -device rtl8139,netdev=n0 \
  -cdrom "$ISO_FILE" \
  -boot d \
  -hda "$HDA_FILE" \
  -no-reboot
