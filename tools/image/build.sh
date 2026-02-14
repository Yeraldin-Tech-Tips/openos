#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$ROOT_DIR/out"
mkdir -p "$OUT_DIR/bin"
BUILD_ROOT="$ROOT_DIR/.build"
TARGET_DIR="$BUILD_ROOT/target"
TMP_BUILD_DIR="$BUILD_ROOT/tmp"
mkdir -p "$TARGET_DIR"
mkdir -p "$TMP_BUILD_DIR"
export TMPDIR="$TMP_BUILD_DIR"
export CARGO_TARGET_DIR="$TARGET_DIR"

if ! command -v cargo >/dev/null 2>&1 || ! command -v rustup >/dev/null 2>&1; then
  if [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env"
  fi
fi

echo "[build] Building EFI loader (x86_64-unknown-uefi)"
rustup target add x86_64-unknown-uefi
cargo build --release -p openos-efi-loader --target x86_64-unknown-uefi

echo "[build] Building kernel (custom x86_64-openos target)"
RUSTFLAGS="-C link-arg=-Tkernel/linker.ld" cargo build \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins \
  -Zbuild-std-features=compiler-builtins-mem \
  --release -p openos-kernel --target kernel/x86_64-openos.json

echo "[build] Building init payload (custom x86_64-openos target)"
RUSTFLAGS="-C link-arg=-Tuserspace/init-payload/linker.ld" cargo build \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins \
  -Zbuild-std-features=compiler-builtins-mem \
  --release -p openos-init-payload --target kernel/x86_64-openos.json

echo "[build] Building app payloads (custom x86_64-openos target)"
RUSTFLAGS="-C link-arg=-Tuserspace/app-shell-payload/linker.ld" cargo build \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins \
  -Zbuild-std-features=compiler-builtins-mem \
  --release -p openos-app-shell-payload --target kernel/x86_64-openos.json
RUSTFLAGS="-C link-arg=-Tuserspace/app-settings-payload/linker.ld" cargo build \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins \
  -Zbuild-std-features=compiler-builtins-mem \
  --release -p openos-app-settings-payload --target kernel/x86_64-openos.json
RUSTFLAGS="-C link-arg=-Tuserspace/app-files-payload/linker.ld" cargo build \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins \
  -Zbuild-std-features=compiler-builtins-mem \
  --release -p openos-app-files-payload --target kernel/x86_64-openos.json

echo "[build] Building userspace binaries"
cargo build --release -p openos-init -p openos-shell -p openos-settings -p openos-files -p openos-terminal-lite

echo "[build] Building installer"
cargo build --release -p openos-installer-gui

echo "[build] Copying and transforming artifacts"
cp "$TARGET_DIR/x86_64-unknown-uefi/release/openos-efi-loader.efi" "$OUT_DIR/bin/openos.efi"
cp "$TARGET_DIR/x86_64-openos/release/openos-kernel" "$OUT_DIR/bin/openos-kernel.elf"
cp "$TARGET_DIR/x86_64-openos/release/openos-init-payload" "$OUT_DIR/bin/init.bin"
cp "$TARGET_DIR/x86_64-openos/release/openos-app-shell-payload" "$OUT_DIR/bin/shell.bin"
cp "$TARGET_DIR/x86_64-openos/release/openos-app-settings-payload" "$OUT_DIR/bin/settings.bin"
cp "$TARGET_DIR/x86_64-openos/release/openos-app-files-payload" "$OUT_DIR/bin/files.bin"
objcopy -O binary "$OUT_DIR/bin/openos-kernel.elf" "$OUT_DIR/bin/kernel.bin"
cp "$TARGET_DIR/release/openos-init" "$OUT_DIR/bin/"
cp "$TARGET_DIR/release/openos-shell" "$OUT_DIR/bin/"
cp "$TARGET_DIR/release/openos-settings" "$OUT_DIR/bin/"
cp "$TARGET_DIR/release/openos-files" "$OUT_DIR/bin/"
cp "$TARGET_DIR/release/openos-terminal-lite" "$OUT_DIR/bin/"
cp "$TARGET_DIR/release/openos-installer-gui" "$OUT_DIR/bin/"

echo "[build] Preparing UEFI FAT directory for QEMU"
EFI_DIR="$OUT_DIR/efi-root"
mkdir -p "$EFI_DIR/EFI/BOOT" "$EFI_DIR/EFI/OPENOS"
cp "$OUT_DIR/bin/openos.efi" "$EFI_DIR/EFI/BOOT/BOOTX64.EFI"
cp "$OUT_DIR/bin/openos.efi" "$EFI_DIR/EFI/OPENOS/openos.efi"
cp "$OUT_DIR/bin/kernel.bin" "$EFI_DIR/EFI/OPENOS/kernel.bin"
cp "$OUT_DIR/bin/init.bin" "$EFI_DIR/EFI/OPENOS/init.bin"
cp "$OUT_DIR/bin/shell.bin" "$EFI_DIR/EFI/OPENOS/shell.bin"
cp "$OUT_DIR/bin/settings.bin" "$EFI_DIR/EFI/OPENOS/settings.bin"
cp "$OUT_DIR/bin/files.bin" "$EFI_DIR/EFI/OPENOS/files.bin"

echo "[build] Done"
