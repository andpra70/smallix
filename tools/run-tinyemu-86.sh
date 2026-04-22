#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${PORT:-8000}"
OPEN_BROWSER=1
DISK_IMAGE="${DISK_IMAGE:-out/smallix.img}"

usage() {
  cat <<EOF
Usage: $(basename "$0") [--port N] [--disk PATH] [--no-open]

Builds Smallix ISO, prepares TinyEMU x86 web assets, starts a local HTTP server,
and opens TinyEMU in TTY mode (serial-style terminal in browser).
The disk source can be .img or .iso and is exposed as a virtual TinyEMU drive.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --port)
      PORT="${2:-}"
      shift 2
      ;;
    --disk)
      DISK_IMAGE="${2:-}"
      shift 2
      ;;
    --no-open)
      OPEN_BROWSER=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1"
      usage
      exit 1
      ;;
  esac
done

if ! [[ "$PORT" =~ ^[0-9]+$ ]]; then
  echo "Invalid port: $PORT"
  exit 1
fi

if ! command -v node >/dev/null 2>&1; then
  echo "node not found"
  exit 1
fi

if command -v temu >/dev/null 2>&1; then
  if ! strings "$(command -v temu)" | grep -q "pc_machine_init"; then
    echo "Note: installed 'temu' does not include x86 machine support."
    echo "      Using TinyEMU browser mode (x86 wasm + TTY) instead."
  fi
fi

echo "[1/4] Building ISO"
"$ROOT_DIR/tools/mkiso.sh"

echo "[2/4] Converting ISO to TinyEMU disk image"
"$ROOT_DIR/tools/mkdisk.sh"

echo "[3/4] Preparing TinyEMU web assets"
TINYEMU_SKIP_DISK_SPLIT=1 "$ROOT_DIR/tools/setup-tinyemu-web.sh"

if [[ ! -f "$ROOT_DIR/$DISK_IMAGE" && ! -f "$DISK_IMAGE" ]]; then
  echo "Disk image not found: $DISK_IMAGE"
  exit 1
fi

CFG_BUST="$(date +%s)"
CFG_URL_ENC="tinyemu-smallix.cfg%3Fv%3D${CFG_BUST}"
URL="http://127.0.0.1:${PORT}/www/tinyemu-vm.html?cpu=x86&graphic=tty&w=1024&h=768&mem=256&net_url=&url=${CFG_URL_ENC}"

echo "[4/4] Starting local server on port ${PORT}"
echo "Open TinyEMU TTY serial terminal at:"
echo "  ${URL}"
echo "Virtual disk source:"
echo "  ${DISK_IMAGE}"

if [[ "$OPEN_BROWSER" -eq 1 ]] && command -v xdg-open >/dev/null 2>&1; then
  xdg-open "$URL" >/dev/null 2>&1 || true
fi

exec node "$ROOT_DIR/tools/tinyemu-single-disk-server.mjs" \
  --root "$ROOT_DIR" \
  --disk "$DISK_IMAGE" \
  --port "$PORT"
