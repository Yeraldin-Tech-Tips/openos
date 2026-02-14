#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <unsigned-efi> <signed-efi> <key-dir>"
  exit 1
fi

UNSIGNED="$1"
SIGNED="$2"
KEY_DIR="$3"

sbsign --key "$KEY_DIR/MOK.key" --cert "$KEY_DIR/MOK.crt" --output "$SIGNED" "$UNSIGNED"
echo "Signed EFI binary: $SIGNED"
