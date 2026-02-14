#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

lane() {
  local name="$1"
  shift
  (
    echo "[$name] start"
    if "$@"; then
      echo "[$name] done"
    else
      echo "[$name] completed with non-zero status"
    fi
  )
}

lane boot bash -lc 'bash -n tools/image/build.sh tools/image/make_live_iso.sh tools/image/run_qemu.sh tools/sign/sign-efi.sh tools/sign/sign-kernel.sh' &
P1=$!

lane kernel bash -lc 'ci/verify-environment.sh || true; rg -n "BOOTINFO|Syscall|KernelDriver" kernel shared/abi >/tmp/openos-kernel-lane.log' &
P2=$!

lane shell bash -lc 'rg -n "GestureAction|default-bindings|ControlCenter|NotificationCenter" userspace/shell shared/abi >/tmp/openos-shell-lane.log' &
P3=$!

lane installer bash -lc 'rg -n "InstallPlan|rollback|ext4|BootEntryPolicy" installer docs/install >/tmp/openos-installer-lane.log' &
P4=$!

wait "$P1" "$P2" "$P3" "$P4"

echo "All lanes completed"
