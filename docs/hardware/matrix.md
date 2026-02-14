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

- Ethernet: baseline required
- Wi-Fi: Intel chipset family prioritized

## Graphics

- UEFI GOP framebuffer required at boot
- Native GPU acceleration pipeline is deferred until post-v1

## Storage

- GPT disk layout
- ext4 root filesystem default
- EFI System Partition (FAT32) required
