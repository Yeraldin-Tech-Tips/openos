# OpenOS

OpenOS is a from-scratch x86_64 operating system project with an iOS/iPadOS-inspired shell, designed for real UEFI laptop hardware.

## Current state

This repository now contains:

- A monolithic kernel scaffold (`kernel/`)
- A UEFI loader scaffold (`boot/efi-loader/`)
- Native userspace foundations (`userspace/`)
- Shared ABI contracts for syscalls, boot handoff, IPC, and app manifests (`shared/abi/`)
- GUI installer architecture scaffold (`installer/gui/`)
- Image/signing/build scripts (`tools/`)
- CI and technical docs (`ci/`, `docs/`)

Boot status today:

- UEFI loader boots and hands off validated `BootInfo` to kernel
- Kernel initializes core subsystems and discovers `init.bin` as a boot module
- PID1 launch path uses boot-module metadata with ELF validation, userspace virtual mapping, and ring3 `iretq` dispatch
- Userspace `int 0x80` trap path is active with kernel dispatch + `iretq` return to PID1
- Userspace `FsWrite` now sends ring3 bytes to kernel serial for early process telemetry
- Host-side `openos-init` PID1 stub now performs explicit VFS/service-policy bootstrap probes and launches shell via `ProcSpawn(1)` with serial/stdout diagnostics
- `ProcSpawn` now creates runnable user tasks from the active userspace image (no hardcoded `pid2`)
- `ProcSpawn(spawn_arg)` now supports module-selected app spawning (`shell/settings/files`) in addition to clone semantics
- PID1 init payload now runs a gesture-driven launcher loop with queued app requests and child reaping
- Kernel app lifecycle registry now tracks launched app modules with foreground/exited state transitions
- App lifecycle retention is now bounded with oldest-record eviction to keep `/proc/apps` stable over long sessions
- Kernel lifecycle now exposes launch sequence history for shell quick-switch introspection via `/proc/launcher-history`
- `ProcExit` now requests cooperative task retirement; scheduler reclaims ASID/page resources on switch-out
- `ProcWait` now lets parents reap child exit events/status from a kernel wait queue
- `ProcWait`/`FsRead`/`NetRecv`/`IpcSend`/`IpcRecv` now copy through MM-backed user read/write validation helpers instead of direct unchecked user pointer access
- `VmMap`/`VmUnmap` now back per-task dynamic user mappings with page-table updates and recycle on unmap
- MM user-range validation now includes mapped-page and writable-page checks, with kernel unit tests covering unmapped/read-only/cross-page spans
- `FsOpen`/`FsRead`/`FsClose` now expose an in-memory tree-backed read-only VFS with directory iteration + dynamic `/proc` nodes (`self/status`, `tree`, `tasks`, `apps`)
- `NetSocket`/`NetConnect`/`NetSend`/`NetRecv` now provide loopback sockets plus a NIC transmit path (`nic0`) for userspace validation
- `IpcSend`/`IpcRecv` now move validated UI lifecycle messages through a kernel queue with bounded payloads
- `GfxSubmitScene`/`GfxPresent` now render a simple gradient + dock style backdrop on the boot framebuffer
- Framebuffer fill paths now consistently address pixels by `(y * stride + x)` across solid, gradient, strip, and console clear operations
- Compositor overlay now draws a visible status bar, dock icons, and lifecycle-driven app cards over scene gradients
- Home UI now renders an iPadOS-like layout with wallpaper layers, widgets, app grid, and dock
- Foreground app panels for shell/settings/files now include close/actions controls and live lifecycle + IPC status text
- Hover and press feedback are now rendered for widgets, app icons, dock icons, and foreground panel controls
- Widget drag (clock/match/weather) and dock icon reorder are now available as prototype interactions
- `InputSubscribe`/`InputRead` now run through PS/2 IRQ1 keyboard + IRQ12 mouse input, including pointer motion, click, and drag actions
- PID1 launcher now keeps recent-app quick-switch history (left/right) and Home toggles between shell and last non-shell app
- Scheduler now resolves spawn-from-current images by stable module source ID to avoid stale shared ELF staging pointers
- Scheduler reserves PID 0 and PID 1 permanently; allocator wraparound skips both IDs so dynamic tasks always receive PID >= 2
- Scheduler now guarantees child-exit collection when parent reap queues are saturated by falling back to exited-task scans
- Kernel installs `#UD/#GP/#PF` handlers plus timer IRQ0 (PIC+PIT) for early fault containment and scheduling ticks
- Scheduler now captures user register context on timer ticks and supports round-robin preemption state transitions
- Userspace tasks now get distinct ASIDs/page tables so timer switches can hop across isolated CR3 contexts
- User faults retire offending tasks and release their address-space/page resources for reuse
- MM recycle free-lists now use interrupt-safe critical sections and ordered publication/consumption semantics
- A shared `userspace/syscall` crate now centralizes ring3 `int 0x80` wrappers for payload/app reuse
- Headless QEMU validation works via serial logs, and SDL UI boot is available through the UI runner script
- Framebuffer full-screen fill and console clear paths now follow width/height/stride addressing rules used by other compositor paths

## Boot targets

- Live USB image (UEFI)
- Partition install (dual-boot safe flow)

## Security model (planned)

- Signed EFI loader
- Signed kernel image
- Signed app bundles
- MOK-based development Secure Boot enrollment

## Build prerequisites

- Rust nightly + `rust-src`
- `llvm-tools-preview`
- `lld`
- `nasm`
- `xorriso` (ISO assembly, optional)
- `qemu-img` (raw USB image assembly)
- `sbsigntool` and `openssl` (for Secure Boot signing)
- `qemu-system-x86_64` (for local boot validation)
- OVMF firmware at `bios/OVMF_CODE.fd` (auto-copied by QEMU scripts from `/usr/share/OVMF/OVMF_CODE.fd` when available)

## Quick start (once dependencies are installed)

```bash
./tools/image/install-deps-ubuntu.sh
./tools/image/build.sh
./tools/image/make_usb_image.sh
./tools/image/run_qemu.sh
```

`run_qemu.sh` prefers `out/openos-usb-x86_64.img` when present, then `out/efi-root` via a temporary ESP image, then `out/openos-live-x86_64.iso`.

Run with SDL window output (requires desktop/X11 availability):

```bash
./tools/image/run_qemu_ui.sh
```

Generate an ISO (experimental path):

```bash
./tools/image/make_live_iso.sh
```

Write raw USB image to a real device:

```bash
./tools/image/write_usb.sh out/openos-usb-x86_64.img /dev/sdX
```

## Parallel work lanes

Run quick parallel checks across boot/kernel/shell/installer lanes:

```bash
./tools/dev/parallel-lanes.sh
```

See `docs/install/live-and-partition.md`, `docs/secure-boot/mok-enrollment.md`, and `docs/abi/bootinfo_v2.md`.
