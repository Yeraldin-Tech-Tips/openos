# CLAUDE.md - OpenOS Development Guide

## Project Overview

OpenOS is a from-scratch x86_64 operating system written in Rust, targeting real UEFI laptop hardware. It features a monolithic kernel with ring3 userspace, gesture-driven shell, IPC-based app lifecycle, and an iOS/iPadOS-inspired GUI.

## Quick Reference

```bash
# Install dependencies (Ubuntu)
./tools/image/install-deps-ubuntu.sh

# Full build
make build                    # or: ./tools/image/build.sh

# Code quality (run before committing)
make check                    # runs: fmt + clippy + test

# Individual checks
make fmt                      # cargo fmt --all
make clippy                   # cargo clippy --workspace --all-targets -- -D warnings
make test                     # cargo test -p abi -p openos-installer-gui

# Run in QEMU
make qemu                    # or: ./tools/image/run_qemu.sh

# Build ISO/USB images
make image                   # bootable ISO via xorriso
make usb-image               # raw USB image with ESP
```

## Repository Structure

```
openos/
├── boot/efi-loader/          # UEFI bootloader (x86_64-unknown-uefi target)
├── kernel/                   # Monolithic kernel (#![no_std], #![no_main])
│   ├── src/
│   │   ├── arch/x86_64/      # CPU tables, interrupts, serial, context switch
│   │   ├── mm/               # Physical/virtual memory, paging, ASID
│   │   ├── sched/            # Round-robin scheduler, preemption, task dispatch
│   │   ├── syscall/          # int 0x80 handler dispatch
│   │   ├── init/             # PID1 launch and ELF validation
│   │   ├── fs/               # In-memory VFS with /proc
│   │   ├── ipc/              # UI message queue
│   │   ├── input/            # PS/2 gesture input queue
│   │   ├── net/              # Loopback + NIC transmit
│   │   ├── ui/               # Framebuffer compositor
│   │   ├── lifecycle/        # App lifecycle tracking
│   │   ├── drivers/          # HID, Ethernet, Wi-Fi stubs
│   │   └── main.rs           # Kernel entry (openos_kernel_main)
│   ├── linker.ld             # Kernel linker script (1M base)
│   └── x86_64-openos.json    # Custom bare-metal target spec
├── shared/abi/               # Shared ABI contracts between kernel and userspace
│   └── src/
│       ├── syscalls.rs       # Syscall enums and numbers
│       ├── boot.rs           # BootInfo, FramebufferInfo
│       ├── ipc.rs            # UiChannel, UiMessageHeader
│       ├── input.rs          # GestureAction enums
│       └── app_manifest.rs   # App metadata
├── userspace/
│   ├── syscall/              # Syscall wrapper library for userspace
│   ├── init/                 # PID1 launcher (managed binary)
│   ├── init-payload/         # PID1 bare-metal ring3 image
│   ├── shell/                # Gesture-driven shell
│   ├── app-shell-payload/    # Shell bare-metal ring3 image
│   ├── apps/
│   │   ├── files/            # File manager app
│   │   ├── settings/         # Settings app
│   │   └── terminal-lite/    # Terminal app
│   └── app-*-payload/        # Per-app bare-metal ring3 images
├── installer/gui/            # GUI installer (Rust, serde/JSON)
├── tools/
│   ├── image/                # Build scripts, ISO/USB creation, QEMU harness
│   ├── sign/                 # Secure Boot signing scripts
│   └── dev/                  # Development helper scripts
├── docs/                     # ABI specs, boot protocol, install guides
├── ci/                       # CI scripts and hardware smoke checklists
├── specs/                    # Target hardware profile specs
├── Cargo.toml                # Workspace root (14 crate members)
├── Makefile                  # Build shortcuts
└── rust-toolchain.toml       # Nightly + rust-src, llvm-tools-preview
```

## Build System

### Toolchain

- **Rust nightly** (required for `#![no_std]`, `#![no_main]`, `-Zbuild-std`)
- Components: `rust-src`, `llvm-tools-preview`, `clippy`, `rustfmt`
- Linker: `rust-lld`

### Build Targets

The project compiles for multiple targets in a single build:

| Component | Target | Linker Script |
|-----------|--------|---------------|
| EFI Loader | `x86_64-unknown-uefi` | (default) |
| Kernel | `kernel/x86_64-openos.json` | `kernel/linker.ld` |
| Init payload | `kernel/x86_64-openos.json` | `userspace/init-payload/linker.ld` |
| App payloads | `kernel/x86_64-openos.json` | `userspace/app-*-payload/linker.ld` |
| Userspace binaries | native x86_64 | (default) |
| Installer | native x86_64 | (default) |

Bare-metal crates use `-Zbuild-std=core,alloc,compiler_builtins` and custom RUSTFLAGS for linker scripts. The master build orchestrator (`tools/image/build.sh`) handles all of this.

### Build Artifacts

Output goes to `out/bin/` and `out/efi-root/`. The `.build/` directory holds intermediate cargo target output. Both `out/` and `.build/` are gitignored.

## Testing

```bash
# Run all tests
make test

# Specific crate tests
cargo test -p abi
cargo test -p openos-installer-gui
```

Only the `abi` and `openos-installer-gui` crates have unit tests (the kernel and bare-metal payloads cannot run standard test harnesses). When adding new ABI types or installer logic, add corresponding tests.

Hardware smoke testing is documented in `ci/hardware-smoke-checklist.md` (manual process for real UEFI hardware).

## CI Pipeline

GitHub Actions (`.github/workflows/build.yml`) runs on all pushes and PRs:

1. **Lint** - `cargo fmt --all -- --check` + `cargo clippy -p abi -p openos-installer-gui -- -D warnings`
2. **Test** - `cargo test -p abi -p openos-installer-gui`
3. **Build** - Full `build.sh` (requires: clang, lld, xorriso, mtools, qemu-utils, gdisk, sbsigntool, openssl)
4. **ISO** - Bootable live ISO generation

Lint and test must pass before build runs.

## Architecture

### Boot Chain

```
UEFI Firmware → EFI Loader (ring0) → Kernel (ring0) → PID1 (ring3)
```

1. EFI loader loads kernel binary + boot modules (init, shell, settings, files payloads)
2. Constructs `BootInfo` struct (magic, version, framebuffer, memory map, modules)
3. Calls `openos_kernel_main(boot_info_ptr)`
4. Kernel initializes subsystems, validates PID1, dispatches to ring3 via `iretq`

### Kernel Subsystems

- **MM** (`mm/`) - Physical allocator, per-task page tables, ASID isolation, 4K/2M pages
- **Scheduler** (`sched/`) - Round-robin, 5-tick preemption, 32 max tasks, per-task CR3
- **Syscall** (`syscall/`) - `int 0x80` trap, 4-register arg passing (a0-a3), `SyscallResult { code, value }`
- **VFS** (`fs/`) - In-memory tree, read-only, dynamic `/proc` nodes
- **IPC** (`ipc/`) - Bounded UI message queue for lifecycle events
- **Graphics** (`ui/`) - Framebuffer compositor, gradient background, dock, status bar
- **Input** (`input/`) - PS/2 IRQ1 handler, gesture translation
- **Networking** (`net/`) - Loopback, NIC transmit path

### Syscall ABI

Defined in `shared/abi/src/syscalls.rs`. Groups by subsystem:

| Group | Range | Syscalls |
|-------|-------|----------|
| Process | `0x00xx` | ProcSpawn, ProcExit, ProcWait |
| VM | `0x01xx` | VmMap, VmUnmap |
| FS | `0x02xx` | FsOpen, FsRead, FsWrite, FsClose |
| NET | `0x03xx` | NetSocket, NetConnect, NetSend, NetRecv |
| IPC | `0x04xx` | IpcSend, IpcRecv |
| GFX | `0x05xx` | GfxSubmitScene, GfxPresent |
| INPUT | `0x06xx` | InputSubscribe, InputRead |

**Compatibility rules**: Existing syscall numbers are immutable. New syscalls append within their range. See `docs/abi/syscalls_v1.md` for full argument contracts.

## Key Conventions

### Code Style

- **Rust 2021 edition** across all crates
- `cargo fmt --all` for formatting (enforced in CI)
- `clippy` with `-D warnings` on host-target crates
- `#![no_std]` and `#![no_main]` for kernel and payload crates
- `#[repr(C)]` on all ABI-crossing structs for layout stability
- Serial debug output via `arch::x86_64::serial::write_line()` / `write_hex_u64()`

### ABI Stability

- Syscall numbers in `shared/abi/src/syscalls.rs` are immutable once released
- `SyscallResult` must remain exactly 16 bytes (`i64 + u64`)
- `BootInfo` uses magic + version validation before any field access
- All cross-boundary types use `#[repr(C)]` or `#[repr(u16)]`

### Workspace Organization

- Shared types go in `shared/abi/` - both kernel and userspace depend on this crate
- Each bare-metal app has two crates: a managed binary (e.g., `userspace/shell/`) and a payload image (e.g., `userspace/app-shell-payload/`)
- Payload images have their own linker scripts for memory layout control
- The `userspace/syscall/` crate provides the syscall wrapper library for all userspace code

### Adding a New Syscall

1. Add the variant to `Syscall` enum in `shared/abi/src/syscalls.rs` (append to the appropriate group range)
2. Add discriminant tests in the same file
3. Implement the handler in `kernel/src/syscall/mod.rs`
4. Add the wrapper in `userspace/syscall/`
5. Document the argument contract in `docs/abi/syscalls_v1.md`

### Adding a New App

1. Create `userspace/apps/<name>/` with a managed binary crate
2. Create `userspace/app-<name>-payload/` with bare-metal payload crate + `linker.ld`
3. Add both crates to workspace `members` in root `Cargo.toml`
4. Add build steps to `tools/image/build.sh`
5. Add a `BootModuleKind` variant in `shared/abi/src/boot.rs` if loaded as a boot module
6. Add a `ProcSpawn` argument mapping in the kernel syscall handler

### Environment Setup

Required system dependencies (Ubuntu): clang, lld, xorriso, mtools, qemu-utils, gdisk, sbsigntool, openssl, OVMF firmware.

Verify with:
```bash
./ci/verify-environment.sh
```

## Important Files

| File | Purpose |
|------|---------|
| `Cargo.toml` | Workspace root with all crate members |
| `rust-toolchain.toml` | Pins nightly + required components |
| `.cargo/config.toml` | Linker config for UEFI target |
| `kernel/x86_64-openos.json` | Custom bare-metal target specification |
| `kernel/linker.ld` | Kernel memory layout (1M base address) |
| `tools/image/build.sh` | Master build orchestrator |
| `tools/image/run_qemu.sh` | QEMU emulation with KVM/TCG fallback |
| `.github/workflows/build.yml` | CI/CD pipeline definition |
| `docs/abi/syscalls_v1.md` | Syscall argument contracts |
| `shared/abi/src/syscalls.rs` | Canonical syscall number definitions |

## Common Tasks

### Verifying changes don't break the build

```bash
make check   # fmt + clippy + test
make build   # full artifact compilation
```

### Testing in QEMU

```bash
make build && make qemu
```

Serial output goes to the terminal. The QEMU harness uses KVM when available, falls back to TCG.

### Parallel validation lanes

```bash
./tools/dev/parallel-lanes.sh
```

Runs 4 independent validation scans (boot scripts, kernel ABI, shell mappings, installer contracts) concurrently. Does not replace `make check` but catches integration drift.
