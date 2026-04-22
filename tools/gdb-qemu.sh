#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO_FILE="$ROOT_DIR/out/smallix.iso"

"$ROOT_DIR/tools/mkiso.sh"

exec "$ROOT_DIR/tools/qemu-safe.sh" \
  -m 256M \
  -serial stdio \
  -netdev user,id=n0,guestfwd=tcp:10.0.2.100:2323-cmd:/bin/cat \
  -device rtl8139,netdev=n0 \
  -cdrom "$ISO_FILE" \
  -boot d \
  -S -s
