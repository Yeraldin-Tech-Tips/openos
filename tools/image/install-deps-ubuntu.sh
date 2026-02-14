#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update
sudo apt-get install -y \
  build-essential \
  clang lld llvm \
  nasm \
  gdisk \
  xorriso mtools \
  qemu-system-x86 \
  ovmf \
  sbsigntool openssl \
  pkg-config

if ! command -v rustup >/dev/null 2>&1; then
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi

source "$HOME/.cargo/env"
rustup toolchain install nightly
rustup default nightly
rustup component add rust-src llvm-tools-preview rustfmt clippy

echo "Dependencies installed."
