#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ASSETS_DIR="$ROOT_DIR/www/tinyemu-assets"
IMG_DIR="$ASSETS_DIR/images"
WWW_DIR="$ROOT_DIR/www"
DISK_WEB_DIR="$WWW_DIR/tinyemu-disks/smallix"

mkdir -p "$IMG_DIR"

fetch() {
  local url="$1"
  local out="$2"
  echo "download: $url -> $out"
  curl -fsSL "$url" -o "$out"
}

BASE="https://bellard.org/jslinux"
fetch "$BASE/term.js" "$ASSETS_DIR/term.js"
fetch "$BASE/jslinux.js" "$ASSETS_DIR/jslinux.js"
fetch "$BASE/style.css" "$ASSETS_DIR/style.css"
fetch "$BASE/images/upload-icon.png" "$IMG_DIR/upload-icon.png"
fetch "$BASE/images/bg-scrollbar-track-y.png" "$IMG_DIR/bg-scrollbar-track-y.png"
fetch "$BASE/images/bg-scrollbar-trackend-y.png" "$IMG_DIR/bg-scrollbar-trackend-y.png"
fetch "$BASE/images/bg-scrollbar-thumb-y.png" "$IMG_DIR/bg-scrollbar-thumb-y.png"
fetch "$BASE/bios.bin" "$ASSETS_DIR/bios.bin"
fetch "$BASE/vgabios.bin" "$ASSETS_DIR/vgabios.bin"
fetch "$BASE/x86emu-wasm.js" "$WWW_DIR/x86emu-wasm.js"
fetch "$BASE/x86emu-wasm.wasm" "$WWW_DIR/x86emu-wasm.wasm"

if [[ "${TINYEMU_SKIP_DISK_SPLIT:-0}" == "1" ]]; then
  echo "TinyEMU web assets ready in: $ASSETS_DIR"
  echo "Disk split skipped (TINYEMU_SKIP_DISK_SPLIT=1)"
  echo "Serve the project root and open: /www/tinyemu.html"
  exit 0
fi

if [[ ! -f "$ROOT_DIR/out/smallix.img" ]]; then
  echo "smallix.img missing, generating via tools/mkdisk.sh"
  "$ROOT_DIR/tools/mkdisk.sh"
fi

echo "Preparing TinyEMU block disk manifest from out/smallix.img"
mkdir -p "$DISK_WEB_DIR"
rm -f "$DISK_WEB_DIR"/blk*.bin "$DISK_WEB_DIR"/blk*.bin.en "$DISK_WEB_DIR/blk.txt"

DISK_IMG="$ROOT_DIR/out/smallix.img"
SECTOR_SIZE=512
BLOCK_SECTORS=256
BLOCK_BYTES=$((SECTOR_SIZE * BLOCK_SECTORS))
DISK_SIZE="$(stat -c%s "$DISK_IMG")"
N_BLOCKS=$(( (DISK_SIZE + BLOCK_BYTES - 1) / BLOCK_BYTES ))

for ((i=0; i<N_BLOCKS; i++)); do
  printf -v blkname "blk%09d.bin" "$i"
  # TinyEMU expects each network block file to be exactly block_size * 512 bytes.
  # conv=sync zero-pads the final partial block to fixed BLOCK_BYTES.
  dd if="$DISK_IMG" of="$DISK_WEB_DIR/$blkname" bs="$BLOCK_BYTES" skip="$i" count=1 conv=sync status=none
  # Keep compatibility with localized naming variants used by some TinyEMU builds.
  cp -f "$DISK_WEB_DIR/$blkname" "$DISK_WEB_DIR/${blkname}.en"
done

PREFETCH_MAX=16
if ((N_BLOCKS < PREFETCH_MAX)); then
  PREFETCH_MAX=$N_BLOCKS
fi
PREFETCH_LIST=""
for ((i=0; i<PREFETCH_MAX; i++)); do
  if ((i > 0)); then
    PREFETCH_LIST+=","
  fi
  PREFETCH_LIST+="$i"
done

cat > "$DISK_WEB_DIR/blk.txt" <<EOF
{
  "block_size": $BLOCK_SECTORS,
  "n_block": $N_BLOCKS,
  "prefetch": [ $PREFETCH_LIST ]
}
EOF

echo "TinyEMU web assets ready in: $ASSETS_DIR"
echo "Serve the project root and open: /www/tinyemu.html"
