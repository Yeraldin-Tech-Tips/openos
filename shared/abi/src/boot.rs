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
