#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$ROOT_DIR/out"
ISO_ROOT="$OUT_DIR/iso-root"
EFI_BOOT="$ISO_ROOT/EFI/BOOT"
EFI_OPENOS="$ISO_ROOT/EFI/OPENOS"

mkdir -p "$EFI_BOOT" "$EFI_OPENOS"

if [[ ! -f "$OUT_DIR/bin/openos.efi" || ! -f "$OUT_DIR/bin/kernel.bin" || ! -f "$OUT_DIR/bin/init.bin" ]]; then
  echo "Missing artifacts. Run ./tools/image/build.sh first."
  exit 1
fi

cp "$OUT_DIR/bin/openos.efi" "$EFI_BOOT/BOOTX64.EFI"
cp "$OUT_DIR/bin/openos.efi" "$EFI_OPENOS/openos.efi"
cp "$OUT_DIR/bin/kernel.bin" "$EFI_OPENOS/kernel.bin"
cp "$OUT_DIR/bin/init.bin" "$EFI_OPENOS/init.bin"
cp "$ROOT_DIR/boot/efi-loader/assets/startup.nsh" "$ISO_ROOT/startup.nsh"

if command -v xorriso >/dev/null 2>&1; then
  xorriso -as mkisofs \
    -R -J \
    -V OPENOS \
    -eltorito-alt-boot \
    -e EFI/BOOT/BOOTX64.EFI \
    -no-emul-boot \
    -isohybrid-gpt-basdat \
    -o "$OUT_DIR/openos-live-x86_64.iso" \
    "$ISO_ROOT"
  echo "ISO created at $OUT_DIR/openos-live-x86_64.iso"
else
  echo "xorriso not found; prepared ISO root at $ISO_ROOT"
  exit 2
fi
