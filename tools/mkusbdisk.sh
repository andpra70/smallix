#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
USB_IMG="$ROOT_DIR/out/usb0.img"

mkdir -p "$ROOT_DIR/out"

if command -v qemu-img >/dev/null 2>&1; then
  qemu-img create -f raw "$USB_IMG" 8M >/dev/null
else
  dd if=/dev/zero of="$USB_IMG" bs=1M count=8 status=none
fi

echo "USB raw disk created: $USB_IMG"
