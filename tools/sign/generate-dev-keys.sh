#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
KEY_DIR="$ROOT_DIR/keys/dev"
mkdir -p "$KEY_DIR"

openssl req -new -x509 -newkey rsa:4096 \
  -keyout "$KEY_DIR/MOK.key" \
  -out "$KEY_DIR/MOK.crt" \
  -nodes \
  -days 3650 \
  -subj "/CN=OpenOS Dev MOK/"

echo "Generated dev keypair under $KEY_DIR"
