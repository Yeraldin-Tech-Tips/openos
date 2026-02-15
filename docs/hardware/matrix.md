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

- `kernel/src/drivers/ethernet_intel.rs`: probe currently returns `false`; `init()` returns `DriverError::NotReady`
- `kernel/src/drivers/wifi_intel.rs`: probe currently returns `false`; `init()` returns `DriverError::Unsupported`
- `kernel/src/drivers/hid.rs`: probe currently returns `false`; `init()` returns `DriverError::NotReady`
- `kernel/src/drivers/mod.rs`: driver manager logs per-driver probe and init success/failure to serial for bring-up visibility
