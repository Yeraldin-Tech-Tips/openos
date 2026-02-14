#!/usr/bin/env bash
set -euo pipefail

required=(cargo rustc rustup clang ld.lld xorriso qemu-system-x86_64 qemu-img sgdisk sbsign openssl)
missing=()
for bin in "${required[@]}"; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    missing+=("$bin")
  fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
  echo "Missing dependencies: ${missing[*]}"
  exit 1
fi

echo "Environment looks ready."
