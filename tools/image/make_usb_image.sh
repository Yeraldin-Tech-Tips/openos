#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$ROOT_DIR/out"
EFI_DIR="$OUT_DIR/efi-root"
ESP_IMG="$OUT_DIR/openos-esp.img"
USB_IMG="$OUT_DIR/openos-usb-x86_64.img"
TMPDIR="${TMPDIR:-/tmp}"

BUILD_TMP_DIR="$(mktemp -d "${TMPDIR%/}/openos-usb-build-XXXXXX")"
TMP_ESP_IMG="$BUILD_TMP_DIR/openos-esp.img"
TMP_USB_IMG="$BUILD_TMP_DIR/openos-usb-x86_64.img"

cleanup() {
  rm -rf "$BUILD_TMP_DIR"
}

trap cleanup EXIT

if [[ ! -f "$EFI_DIR/EFI/BOOT/BOOTX64.EFI" || ! -f "$EFI_DIR/EFI/OPENOS/kernel.bin" ]]; then
  echo "Missing EFI payload in $EFI_DIR. Run ./tools/image/build.sh first."
  exit 1
fi

if ! command -v qemu-img >/dev/null 2>&1; then
  echo "qemu-img is required to create the ESP image."
  exit 1
fi
# Build a FAT ESP image from the EFI directory tree without requiring root mounts.
qemu-img convert -f vvfat -O raw "fat:$EFI_DIR" "$TMP_ESP_IMG"

esp_size_bytes=$(stat -c%s "$TMP_ESP_IMG")
min_usb_size_bytes=$((64 * 1024 * 1024))
img_size_bytes="$esp_size_bytes"
if (( img_size_bytes < min_usb_size_bytes )); then
  img_size_bytes=$min_usb_size_bytes
fi

truncate -s "$img_size_bytes" "$TMP_USB_IMG"
dd if="$TMP_ESP_IMG" of="$TMP_USB_IMG" bs=4M conv=notrunc status=none

cp "$TMP_ESP_IMG" "$ESP_IMG"
cp "$TMP_USB_IMG" "$USB_IMG"

echo "USB image created: $USB_IMG"
echo "Layout: FAT superfloppy (UEFI removable-media path /EFI/BOOT/BOOTX64.EFI)."
