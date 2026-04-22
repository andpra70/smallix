#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO_FILE="$ROOT_DIR/out/smallix.iso"
USB_IMG="$ROOT_DIR/out/usb0.img"

"$ROOT_DIR/tools/mkiso.sh"
"$ROOT_DIR/tools/mkusbdisk.sh"

if command -v qemu-system-x86_64 >/dev/null 2>&1; then
  QEMU="qemu-system-x86_64"
elif command -v qemu-system-i386 >/dev/null 2>&1; then
  QEMU="qemu-system-i386"
else
  echo "qemu-system-x86_64 or qemu-system-i386 not found"
  exit 1
fi

exec "$QEMU" \
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
