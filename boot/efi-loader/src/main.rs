#![no_std]
#![no_main]

extern crate alloc;

use abi::boot::{
    BootInfo, BootModule, BootModuleKind, BootModules, FramebufferInfo, MemoryMap, MemoryMapEntry,
    BOOTINFO_MAGIC, BOOTINFO_VERSION, BOOT_FLAG_FRAMEBUFFER_PRESENT, BOOT_FLAG_INIT_MODULE_PRESENT,
};
use alloc::vec::Vec;
use core::mem::size_of;
use core::ptr::NonNull;
use uefi::boot::{self, AllocateType, MemoryType};
use uefi::mem::memory_map::MemoryMap as _;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType, RegularFile};
use uefi::CStr16;

const KERNEL_PATH: &CStr16 = cstr16!("\\EFI\\OPENOS\\kernel.bin");
const INIT_PATH: &CStr16 = cstr16!("\\EFI\\OPENOS\\init.bin");
const SHELL_PATH: &CStr16 = cstr16!("\\EFI\\OPENOS\\shell.bin");
const SETTINGS_PATH: &CStr16 = cstr16!("\\EFI\\OPENOS\\settings.bin");
const FILES_PATH: &CStr16 = cstr16!("\\EFI\\OPENOS\\files.bin");
const MAX_MMAP_ENTRIES: usize = 2048;
const MAX_BOOT_MODULES: usize = 8;

type KernelEntry = extern "sysv64" fn(*const BootInfo) -> !;

#[entry]
fn main() -> Status {
    if uefi::helpers::init().is_err() {
        return Status::ABORTED;
    }

    let framebuffer = capture_framebuffer();

    let (kernel_base, _kernel_size) = match load_blob(KERNEL_PATH, Some(0x100000)) {
        Ok(v) => v,
        Err(s) => return s,
    };

    let init_blob = load_blob(INIT_PATH, None).ok();
    let shell_blob = load_blob(SHELL_PATH, None).ok();
    let settings_blob = load_blob(SETTINGS_PATH, None).ok();
    let files_blob = load_blob(FILES_PATH, None).ok();

    let mmap_storage = match allocate_mmap_storage() {
        Ok(ptr) => ptr,
        Err(s) => return s,
    };

    let modules_storage = match allocate_boot_modules_storage() {
        Ok(ptr) => ptr,
        Err(s) => return s,
    };

    let boot_info_ptr = match allocate_boot_info() {
        Ok(ptr) => ptr,
        Err(s) => return s,
    };

    let mut flags = 0u32;
    let fb_info = if let Some(fb) = framebuffer {
        flags |= BOOT_FLAG_FRAMEBUFFER_PRESENT;
        fb
    } else {
        FramebufferInfo {
            base: core::ptr::null_mut(),
            size: 0,
            width: 0,
            height: 0,
            stride: 0,
            bytes_per_pixel: 0,
        }
    };

    let mut modules_count = 0usize;
    if let Some((init_base, init_size)) = init_blob {
        unsafe {
            modules_storage.add(modules_count).write(BootModule {
                kind: BootModuleKind::InitExecutable,
                _reserved: 0,
                base: init_base.as_ptr().cast_const(),
                size: init_size,
            });
        }
        modules_count += 1;
        flags |= BOOT_FLAG_INIT_MODULE_PRESENT;
    }

    if let Some((shell_base, shell_size)) = shell_blob {
        unsafe {
            modules_storage.add(modules_count).write(BootModule {
                kind: BootModuleKind::AppShellExecutable,
                _reserved: 0,
                base: shell_base.as_ptr().cast_const(),
                size: shell_size,
            });
        }
        modules_count += 1;
    }

    if let Some((settings_base, settings_size)) = settings_blob {
        unsafe {
            modules_storage.add(modules_count).write(BootModule {
                kind: BootModuleKind::AppSettingsExecutable,
                _reserved: 0,
                base: settings_base.as_ptr().cast_const(),
                size: settings_size,
            });
        }
        modules_count += 1;
    }

    if let Some((files_base, files_size)) = files_blob {
        unsafe {
            modules_storage.add(modules_count).write(BootModule {
                kind: BootModuleKind::AppFilesExecutable,
                _reserved: 0,
                base: files_base.as_ptr().cast_const(),
                size: files_size,
            });
        }
        modules_count += 1;
    }

    let efi_mmap = unsafe { boot::exit_boot_services(Some(MemoryType::LOADER_DATA)) };
    let converted_count = convert_memory_map(&efi_mmap, mmap_storage, MAX_MMAP_ENTRIES);
    core::mem::forget(efi_mmap);

    unsafe {
        boot_info_ptr.write(BootInfo {
            magic: BOOTINFO_MAGIC,
            version: BOOTINFO_VERSION,
            flags,
            memory_map: MemoryMap {
                entries: mmap_storage,
                count: converted_count,
            },
            framebuffer: fb_info,
            modules: BootModules {
                entries: modules_storage,
                count: modules_count,
            },
        });
    }

    let kernel_entry: KernelEntry = unsafe { core::mem::transmute(kernel_base.as_ptr() as usize) };
    kernel_entry(boot_info_ptr as *const BootInfo)
}

fn capture_framebuffer() -> Option<FramebufferInfo> {
    let handle = boot::get_handle_for_protocol::<GraphicsOutput>().ok()?;
    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(handle).ok()?;

    let mode_info = gop.current_mode_info();
    let (width, height) = mode_info.resolution();
    let width = u32::try_from(width).ok()?;
    let height = u32::try_from(height).ok()?;
    let mut fb = gop.frame_buffer();

    Some(FramebufferInfo {
        base: fb.as_mut_ptr(),
        size: fb.size(),
        width,
        height,
        stride: mode_info.stride() as u32,
        bytes_per_pixel: 4,
    })
}

fn allocate_boot_info() -> Result<*mut BootInfo, Status> {
    let pages = pages_for(size_of::<BootInfo>());
    let ptr = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
        .map_err(|_| Status::OUT_OF_RESOURCES)?;
    Ok(ptr.as_ptr().cast())
}

fn allocate_mmap_storage() -> Result<*mut MemoryMapEntry, Status> {
    let bytes = MAX_MMAP_ENTRIES * size_of::<MemoryMapEntry>();
    let pages = pages_for(bytes);
    let ptr = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
        .map_err(|_| Status::OUT_OF_RESOURCES)?;
    Ok(ptr.as_ptr().cast())
}

fn allocate_boot_modules_storage() -> Result<*mut BootModule, Status> {
    let bytes = MAX_BOOT_MODULES * size_of::<BootModule>();
    let pages = pages_for(bytes);
    let ptr = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
        .map_err(|_| Status::OUT_OF_RESOURCES)?;
    Ok(ptr.as_ptr().cast())
}

fn load_blob(path: &CStr16, preferred_addr: Option<u64>) -> Result<(NonNull<u8>, usize), Status> {
    let mut fs =
        boot::get_image_file_system(boot::image_handle()).map_err(|_| Status::NOT_FOUND)?;
    let mut root = fs.open_volume().map_err(|_| Status::NOT_FOUND)?;

    let file = root
        .open(path, FileMode::Read, FileAttribute::empty())
        .map_err(|_| Status::NOT_FOUND)?;

    let mut regular_file: RegularFile = match file.into_type().map_err(|_| Status::LOAD_ERROR)? {
        FileType::Regular(f) => f,
        _ => return Err(Status::LOAD_ERROR),
    };

    let mut buffer = Vec::new();
    loop {
        let mut chunk = [0u8; 4096];
        let read = regular_file
            .read(&mut chunk)
            .map_err(|_| Status::LOAD_ERROR)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    let pages = pages_for(buffer.len());
    let blob_ptr = if let Some(addr) = preferred_addr {
        boot::allocate_pages(AllocateType::Address(addr), MemoryType::LOADER_DATA, pages)
            .or_else(|_| {
                boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
            })
            .map_err(|_| Status::OUT_OF_RESOURCES)?
    } else {
        boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
            .map_err(|_| Status::OUT_OF_RESOURCES)?
    };

    unsafe {
        core::ptr::copy_nonoverlapping(buffer.as_ptr(), blob_ptr.as_ptr(), buffer.len());
    }

    Ok((blob_ptr, buffer.len()))
}

fn convert_memory_map(
    efi_mmap: &uefi::mem::memory_map::MemoryMapOwned,
    out_ptr: *mut MemoryMapEntry,
    capacity: usize,
) -> usize {
    let mut written = 0usize;
    for desc in efi_mmap.entries() {
        if written >= capacity {
            break;
        }

        unsafe {
            out_ptr.add(written).write(MemoryMapEntry {
                base: desc.phys_start,
                len: desc.page_count * 4096,
                kind: desc.ty.0,
                _reserved: 0,
            });
        }

        written += 1;
    }

    written
}

const fn pages_for(bytes: usize) -> usize {
    let pages = (bytes + 4095) / 4096;
    if pages == 0 {
        1
    } else {
        pages
    }
}
