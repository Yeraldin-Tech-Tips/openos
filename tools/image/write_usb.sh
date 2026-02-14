#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <image-path> <usb-device>"
  echo "Example: $0 out/openos-usb-x86_64.img /dev/sdb"
  exit 1
fi

IMG="$1"
USB_DEV="$2"

if [[ ! -f "$IMG" ]]; then
  echo "Image not found: $IMG"
  exit 1
fi

echo "About to overwrite $USB_DEV with $IMG"
read -r -p "Type YES to continue: " confirm
if [[ "$confirm" != "YES" ]]; then
  echo "Aborted"
  exit 1
fi

sudo dd if="$IMG" of="$USB_DEV" bs=4M status=progress conv=fsync
sync

echo "USB write complete"
