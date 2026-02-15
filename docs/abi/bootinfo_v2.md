# BootInfo ABI v2

`BootInfo` is shared between EFI loader and kernel through `shared/abi/src/boot.rs`.

## Structure

- `magic` (`u64`): must equal `BOOTINFO_MAGIC`.
- `version` (`u32`): must equal `BOOTINFO_VERSION` (`2`).
- `flags` (`u32`): bitfield, including:
  - `BOOT_FLAG_FRAMEBUFFER_PRESENT`
  - `BOOT_FLAG_INIT_MODULE_PRESENT`
- `memory_map` (`MemoryMap`): pointer + count of physical memory entries.
- `framebuffer` (`FramebufferInfo`): valid when framebuffer flag is set.
- `modules` (`BootModules`): pointer + count of boot modules.

## Boot Modules

Each module is a `BootModule` entry:

- `kind`: `BootModuleKind`
  - `InitExecutable`
  - `AppShellExecutable`
  - `AppSettingsExecutable`
  - `AppFilesExecutable`
- `base`: module base address
- `size`: module size in bytes

Current loader behavior:

- Loads `\\EFI\\OPENOS\\init.bin` and publishes it as `InitExecutable` when present.
- Loads optional app payloads (`shell.bin`, `settings.bin`, `files.bin`) and publishes matching app module kinds.

## Validation rules

1. Kernel must reject boot on magic/version mismatch.
2. Kernel must read framebuffer only when framebuffer flag is set.
3. Kernel must validate module pointers/sizes before use.
4. Loader must allocate memory map/module backing in loader-owned pages before boot handoff.

## Framebuffer format contract

When `BOOT_FLAG_FRAMEBUFFER_PRESENT` is set, `framebuffer` must describe a linear, 32-bits-per-
pixel GOP framebuffer with one of these UEFI pixel formats:

- `PixelFormat::Rgb` (memory order `R8G8B8X8`)
- `PixelFormat::Bgr` (memory order `B8G8R8X8`)

`framebuffer.bytes_per_pixel` must be `4` for any published framebuffer.

If the current GOP mode uses an unsupported format (for example `Bitmask` or `BltOnly`), the
loader must not publish framebuffer data in `BootInfo`:

- clear `BOOT_FLAG_FRAMEBUFFER_PRESENT`
- provide a zeroed `FramebufferInfo`

This fallback allows the kernel to skip graphics initialization cleanly and continue with a
non-graphical boot path.
