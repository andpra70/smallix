#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/out"
ISO_FILE="$OUT_DIR/smallix.iso"
IMG_FILE="$OUT_DIR/smallix.img"

"$ROOT_DIR/tools/mkiso.sh"

if ! command -v qemu-img >/dev/null 2>&1; then
  cp "$ISO_FILE" "$IMG_FILE"
  echo "qemu-img not found, copied ISO as raw image fallback: $IMG_FILE"
  exit 0
fi

qemu-img convert -f raw -O raw "$ISO_FILE" "$IMG_FILE"
echo "Raw image generated: $IMG_FILE"
