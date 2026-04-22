#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMG_FILE="$ROOT_DIR/out/fat32.img"
ROOT_SEED="$ROOT_DIR/root"

mkdir -p "$ROOT_DIR/out"

if command -v qemu-img >/dev/null 2>&1; then
  qemu-img create -f raw "$IMG_FILE" 64M >/dev/null
else
  dd if=/dev/zero of="$IMG_FILE" bs=1M count=64 status=none
fi

if command -v mkfs.fat >/dev/null 2>&1; then
  mkfs.fat -F 32 "$IMG_FILE" >/dev/null
elif command -v mkfs.vfat >/dev/null 2>&1; then
  mkfs.vfat -F 32 "$IMG_FILE" >/dev/null
else
  echo "mkfs.fat/mkfs.vfat not found; image created but not formatted FAT32"
  exit 1
fi

if ! command -v mcopy >/dev/null 2>&1 || ! command -v mmd >/dev/null 2>&1; then
  echo "mtools not found (need mcopy/mmd) to populate FAT32 from root/"
  exit 1
fi

if [ ! -d "$ROOT_SEED" ]; then
  echo "root seed directory missing: $ROOT_SEED"
  exit 1
fi

# Create directories first (excluding root itself).
while IFS= read -r d; do
  rel="${d#$ROOT_SEED/}"
  [ -z "$rel" ] && continue
  mmd -i "$IMG_FILE" "::/$rel" >/dev/null 2>&1 || true
done < <(find "$ROOT_SEED" -type d | sort)

# Copy files preserving relative layout from root/.
while IFS= read -r f; do
  rel="${f#$ROOT_SEED/}"
  mcopy -i "$IMG_FILE" -o "$f" "::/$rel" >/dev/null
done < <(find "$ROOT_SEED" -type f | sort)

echo "FAT32 image ready: $IMG_FILE"
