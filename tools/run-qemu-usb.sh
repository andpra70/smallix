#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO_FILE="$ROOT_DIR/out/smallix.iso"
USB_IMG="$ROOT_DIR/out/usb0.img"

"$ROOT_DIR/tools/mkiso.sh"
"$ROOT_DIR/tools/mkusbdisk.sh"

exec "$ROOT_DIR/tools/qemu-safe.sh" \
  -m 256M \
  -serial stdio \
  -netdev user,id=n0,guestfwd=tcp:10.0.2.100:2323-cmd:/bin/cat \
  -device rtl8139,netdev=n0 \
  -cdrom "$ISO_FILE" \
  -boot d \
  -device qemu-xhci,id=xhci \
  -drive if=none,id=usbstick,file="$USB_IMG",format=raw \
  -device usb-storage,drive=usbstick \
  -no-reboot
