#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/out"
ISO_ROOT="$OUT_DIR/isofiles"
KERNEL_ELF="$ROOT_DIR/target/i686-smallix/debug/smallix-kernel"
ISO_FILE="$OUT_DIR/smallix.iso"

mkdir -p "$OUT_DIR" "$ISO_ROOT/boot/grub"

(
  cd "$ROOT_DIR"
  cargo +nightly build \
    -Z build-std=core,compiler_builtins \
    -Z build-std-features=compiler-builtins-mem \
    -Z json-target-spec
)

cp "$KERNEL_ELF" "$ISO_ROOT/boot/smallix-kernel"
cp "$ROOT_DIR/boot/grub/grub.cfg" "$ISO_ROOT/boot/grub/grub.cfg"

if command -v grub-mkrescue >/dev/null 2>&1; then
  if grub-mkrescue -o "$ISO_FILE" "$ISO_ROOT"; then
    echo "ISO generated with grub-mkrescue: $ISO_FILE"
    exit 0
  fi
  echo "grub-mkrescue failed, trying standalone fallback..."
fi

if ! command -v grub-mkstandalone >/dev/null 2>&1 || ! command -v xorriso >/dev/null 2>&1; then
  echo "Cannot build ISO: need grub-mkrescue or (grub-mkstandalone + xorriso)."
  exit 1
fi

GRUB_DIR="/usr/lib/grub/i386-pc"
if [ ! -f "$GRUB_DIR/cdboot.img" ]; then
  echo "Missing $GRUB_DIR/cdboot.img; install GRUB i386-pc platform files."
  exit 1
fi

grub-mkstandalone \
  -O i386-pc \
  --fonts="" \
  --locales="" \
  --themes="" \
  --install-modules="biosdisk iso9660 multiboot2 normal configfile search search_fs_file test echo terminal serial" \
  --modules="biosdisk iso9660 multiboot2 normal configfile search search_fs_file test echo terminal serial" \
  -o "$OUT_DIR/core.img" \
  "boot/grub/grub.cfg=$ROOT_DIR/boot/grub/grub.cfg"

cat "$GRUB_DIR/cdboot.img" "$OUT_DIR/core.img" > "$ISO_ROOT/boot/grub/bios.img"

xorriso -as mkisofs \
  -R \
  -b boot/grub/bios.img \
  -c boot/grub/boot.cat \
  -no-emul-boot \
  -boot-load-size 4 \
  -boot-info-table \
  -o "$ISO_FILE" \
  "$ISO_ROOT"

echo "ISO generated: $ISO_FILE"
