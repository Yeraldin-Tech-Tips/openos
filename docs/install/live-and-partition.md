# Live USB and Spare Partition Install

## Live USB boot

1. Build artifacts:

```bash
./tools/image/build.sh
./tools/image/make_usb_image.sh
```

2. Write the raw USB image:

```bash
./tools/image/write_usb.sh out/openos-usb-x86_64.img /dev/sdX
```

3. Boot from USB through UEFI boot menu.

## Spare partition install (dual-boot safe target)

1. Boot OpenOS live media.
2. Launch installer GUI.
3. Select target disk and spare partition.
4. Keep existing EFI entries and create OpenOS entry + fallback.
5. Confirm install plan preview and execute.

## Partition layout

- Existing `EFI System Partition` reused
- New/extant OpenOS root partition formatted as `ext4`
- Optional separate user data partition

## Safety checks required by installer

- Target partition is not mounted as current root
- Free space threshold is satisfied
- Existing OS boot entry preserved
- Rollback checkpoint created before each destructive step
