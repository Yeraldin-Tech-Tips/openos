# Hardware Support Matrix (v1 target)

## CPU/Firmware

- x86_64 only
- UEFI required
- Secure Boot supported with MOK enrollment flow

## Input

- Keyboard: required
- Mouse/trackpad: required for gesture emulation
- Touchscreen: optional and not required for v1 parity

## Network

- Ethernet: baseline required at product level; current kernel Intel Ethernet driver is scaffold-only and does not detect hardware yet
- Wi-Fi: Intel chipset family prioritized, but current kernel Intel Wi-Fi driver is scaffold-only and reports unsupported/not-ready

## Graphics

- UEFI GOP framebuffer required at boot
- Native GPU acceleration pipeline is deferred until post-v1

## Storage

- GPT disk layout
- ext4 root filesystem default
- EFI System Partition (FAT32) required

## Kernel Driver Maturity (Current Tree)

| Driver module | Probe support | Init support | Runtime operations |
| --- | --- | --- | --- |
| `kernel/src/drivers/ethernet_intel.rs` | Scaffold only (`probe()` always returns `false`) | Scaffold only (`init()` returns `DriverError::NotReady`) | Partial scaffold: `transmit()` serial-logs payload bytes; no NIC hardware path yet |
| `kernel/src/drivers/wifi_intel.rs` | Scaffold only (`probe()` always returns `false`) | Scaffold only (`init()` returns `DriverError::Unsupported`) | Not implemented (no runtime tx/rx control path yet) |
| `kernel/src/drivers/hid.rs` | Scaffold only (`probe()` always returns `false`) | Scaffold only (`init()` returns `DriverError::NotReady`) | Not implemented (no runtime input controller path yet) |
| `kernel/src/drivers/mod.rs` | Implemented manager iteration over in-tree drivers | Implemented probe/init orchestration with structured error logging | Exposes `ethernet_transmit()` handoff for net syscall path validation |

See `docs/hardware/driver-roadmap.md` for linked checklist tracking and minimum viable milestones.
