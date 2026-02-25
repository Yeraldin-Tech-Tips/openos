#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
USB_IMG="$ROOT_DIR/out/openos-usb-x86_64.img"
ISO="$ROOT_DIR/out/openos-live-x86_64.iso"
FAT_DIR="$ROOT_DIR/out/efi-root"
BIOS_DIR="$ROOT_DIR/bios"
OVMF_CODE="$BIOS_DIR/OVMF_CODE.fd"
SYSTEM_OVMF_CANDIDATES=(
  "/usr/share/OVMF/OVMF_CODE.fd"
  "/usr/share/ovmf/OVMF.fd"
  "/usr/share/ovmf/OVMF_CODE.fd"
  "/usr/share/ovmf/OVMF_CODE_4M.fd"
)
export TMPDIR="${TMPDIR:-/tmp}"
TMP_ESP_IMG=""

mkdir -p "$BIOS_DIR"
if [[ ! -f "$OVMF_CODE" ]]; then
  for candidate in "${SYSTEM_OVMF_CANDIDATES[@]}"; do
    if [[ -f "$candidate" ]]; then
      cp "$candidate" "$OVMF_CODE"
      echo "Saved firmware: $OVMF_CODE (from $candidate)"
      break
    fi
  done
  if [[ ! -f "$OVMF_CODE" ]]; then
    echo "Missing OVMF firmware."
    echo "Expected one of local/cache paths:"
    echo "  - $OVMF_CODE"
    for candidate in "${SYSTEM_OVMF_CANDIDATES[@]}"; do
      echo "  - $candidate"
    done
    exit 1
  fi
fi

if [[ ! -f "$USB_IMG" && ! -f "$ISO" && ! -d "$FAT_DIR" ]]; then
  echo "No boot artifact found."
  echo "Expected one of:"
  echo "  - $USB_IMG"
  echo "  - $ISO"
  echo "  - $FAT_DIR"
  echo "Run ./tools/image/build.sh first."
  exit 1
fi

ACCEL="tcg"
CPU_MODEL="qemu64"
if [[ -r /dev/kvm && -w /dev/kvm ]]; then
  ACCEL="kvm:tcg"
  CPU_MODEL="host"
fi

cleanup() {
  if [[ -n "$TMP_ESP_IMG" && -f "$TMP_ESP_IMG" ]]; then
    rm -f "$TMP_ESP_IMG"
  fi
}

trap cleanup EXIT

if [[ -f "$USB_IMG" ]]; then
  qemu-system-x86_64 \
    -machine q35,accel="$ACCEL" \
    -m 4096 \
    -smp 4 \
    -cpu "$CPU_MODEL" \
    -bios "$OVMF_CODE" \
    -drive format=raw,file="$USB_IMG" \
    -display sdl,gl=off \
    -serial none \
    -monitor none

elif [[ -d "$FAT_DIR" ]]; then
  if ! command -v qemu-img >/dev/null 2>&1; then
    echo "qemu-img is required to boot from $FAT_DIR."
    exit 1
  fi

  TMP_ESP_IMG="$(mktemp "${TMPDIR%/}/openos-esp-run-XXXXXX.img")"
  qemu-img convert -f vvfat -O raw "fat:$FAT_DIR" "$TMP_ESP_IMG"

  qemu-system-x86_64 \
    -machine q35,accel="$ACCEL" \
    -m 4096 \
    -smp 4 \
    -cpu "$CPU_MODEL" \
    -bios "$OVMF_CODE" \
    -drive format=raw,file="$TMP_ESP_IMG" \
    -display sdl,gl=off \
    -serial none \
    -monitor none
else
  qemu-system-x86_64 \
    -machine q35,accel="$ACCEL" \
    -m 4096 \
    -smp 4 \
    -cpu "$CPU_MODEL" \
    -bios "$OVMF_CODE" \
    -cdrom "$ISO" \
    -boot d \
    -display sdl,gl=off \
    -serial none \
    -monitor none
fi
