#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <kernel-bin> <signature-file> <key-dir>"
  exit 1
fi

KERNEL_BIN="$1"
SIG_FILE="$2"
KEY_DIR="$3"

openssl dgst -sha256 -sign "$KEY_DIR/MOK.key" -out "$SIG_FILE" "$KERNEL_BIN"
echo "Kernel signature created: $SIG_FILE"
