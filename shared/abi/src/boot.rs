pub const BOOTINFO_MAGIC: u64 = 0x4F_50_45_4E_4F_53_42_49;
pub const BOOTINFO_VERSION: u32 = 2;

pub const BOOT_FLAG_FRAMEBUFFER_PRESENT: u32 = 1 << 0;
pub const BOOT_FLAG_INIT_MODULE_PRESENT: u32 = 1 << 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryMapEntry {
    pub base: u64,
    pub len: u64,
    pub kind: u32,
    pub _reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryMap {
    pub entries: *const MemoryMapEntry,
    pub count: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub base: *mut u8,
    pub size: usize,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bytes_per_pixel: u32,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootModuleKind {
    InitExecutable = 1,
    AppShellExecutable = 2,
    AppSettingsExecutable = 3,
    AppFilesExecutable = 4,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootModule {
    pub kind: BootModuleKind,
    pub _reserved: u32,
    pub base: *const u8,
    pub size: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootModules {
    pub entries: *const BootModule,
    pub count: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootInfo {
    pub magic: u64,
    pub version: u32,
    pub flags: u32,
    pub memory_map: MemoryMap,
    pub framebuffer: FramebufferInfo,
    pub modules: BootModules,
}

impl BootInfo {
    pub const fn has_framebuffer(&self) -> bool {
        (self.flags & BOOT_FLAG_FRAMEBUFFER_PRESENT) != 0
    }

    pub const fn is_valid(&self) -> bool {
        self.magic == BOOTINFO_MAGIC && self.version == BOOTINFO_VERSION
    }

    pub const fn has_init_module(&self) -> bool {
        (self.flags & BOOT_FLAG_INIT_MODULE_PRESENT) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;

    fn make_boot_info(magic: u64, version: u32, flags: u32) -> BootInfo {
        BootInfo {
            magic,
            version,
            flags,
            memory_map: MemoryMap {
                entries: ptr::null(),
                count: 0,
            },
            framebuffer: FramebufferInfo {
                base: ptr::null_mut(),
                size: 0,
                width: 0,
                height: 0,
                stride: 0,
                bytes_per_pixel: 0,
            },
            modules: BootModules {
                entries: ptr::null(),
                count: 0,
            },
        }
    }

    #[test]
    fn bootinfo_magic_constant() {
        // "OPENOS_BI" in little-endian u64
        assert_eq!(BOOTINFO_MAGIC, 0x4F_50_45_4E_4F_53_42_49);
    }

    #[test]
    fn bootinfo_version_is_v2() {
        assert_eq!(BOOTINFO_VERSION, 2);
    }

    #[test]
    fn boot_flags_are_distinct_bits() {
        assert_eq!(BOOT_FLAG_FRAMEBUFFER_PRESENT, 1);
        assert_eq!(BOOT_FLAG_INIT_MODULE_PRESENT, 2);
        assert_eq!(
            BOOT_FLAG_FRAMEBUFFER_PRESENT & BOOT_FLAG_INIT_MODULE_PRESENT,
            0,
            "flags must not overlap"
        );
    }

    #[test]
    fn is_valid_with_correct_magic_and_version() {
        let info = make_boot_info(BOOTINFO_MAGIC, BOOTINFO_VERSION, 0);
        assert!(info.is_valid());
    }

    #[test]
    fn is_valid_rejects_wrong_magic() {
        let info = make_boot_info(0xDEADBEEF, BOOTINFO_VERSION, 0);
        assert!(!info.is_valid());
    }

    #[test]
    fn is_valid_rejects_wrong_version() {
        let info = make_boot_info(BOOTINFO_MAGIC, 99, 0);
        assert!(!info.is_valid());
    }

    #[test]
    fn has_framebuffer_when_flag_set() {
        let info = make_boot_info(
            BOOTINFO_MAGIC,
            BOOTINFO_VERSION,
            BOOT_FLAG_FRAMEBUFFER_PRESENT,
        );
        assert!(info.has_framebuffer());
    }

    #[test]
    fn no_framebuffer_when_flag_unset() {
        let info = make_boot_info(BOOTINFO_MAGIC, BOOTINFO_VERSION, 0);
        assert!(!info.has_framebuffer());
    }

    #[test]
    fn has_init_module_when_flag_set() {
        let info = make_boot_info(
            BOOTINFO_MAGIC,
            BOOTINFO_VERSION,
            BOOT_FLAG_INIT_MODULE_PRESENT,
        );
        assert!(info.has_init_module());
    }

    #[test]
    fn no_init_module_when_flag_unset() {
        let info = make_boot_info(BOOTINFO_MAGIC, BOOTINFO_VERSION, 0);
        assert!(!info.has_init_module());
    }

    #[test]
    fn both_flags_set_simultaneously() {
        let flags = BOOT_FLAG_FRAMEBUFFER_PRESENT | BOOT_FLAG_INIT_MODULE_PRESENT;
        let info = make_boot_info(BOOTINFO_MAGIC, BOOTINFO_VERSION, flags);
        assert!(info.has_framebuffer());
        assert!(info.has_init_module());
    }

    #[test]
    fn boot_module_kind_discriminants() {
        assert_eq!(BootModuleKind::InitExecutable as u32, 1);
        assert_eq!(BootModuleKind::AppShellExecutable as u32, 2);
        assert_eq!(BootModuleKind::AppSettingsExecutable as u32, 3);
        assert_eq!(BootModuleKind::AppFilesExecutable as u32, 4);
    }

    #[test]
    fn boot_module_kind_equality() {
        assert_eq!(
            BootModuleKind::InitExecutable,
            BootModuleKind::InitExecutable
        );
        assert_ne!(
            BootModuleKind::InitExecutable,
            BootModuleKind::AppShellExecutable
        );
    }

    #[test]
    fn memory_map_entry_layout() {
        // Verify repr(C) struct has expected field sizes
        assert_eq!(
            core::mem::size_of::<MemoryMapEntry>(),
            8 + 8 + 4 + 4, // base + len + kind + _reserved
            "MemoryMapEntry must be 24 bytes for ABI stability"
        );
    }

    #[test]
    fn boot_module_layout() {
        let module = BootModule {
            kind: BootModuleKind::InitExecutable,
            _reserved: 0,
            base: ptr::null(),
            size: 0,
        };
        assert_eq!(module.kind, BootModuleKind::InitExecutable);
        assert_eq!(module.size, 0);
    }
}
