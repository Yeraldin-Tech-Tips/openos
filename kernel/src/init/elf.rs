use core::mem::size_of;

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELF_CLASS_64: u8 = 2;
const ELF_DATA_LSB: u8 = 1;
const ELF_TYPE_EXEC: u16 = 2;
const ELF_TYPE_DYN: u16 = 3;
const ELF_MACHINE_X86_64: u16 = 62;
const ELF_VERSION_CURRENT: u32 = 1;
const PT_LOAD: u32 = 1;

const MAX_LOAD_SEGMENTS: usize = 16;
const USERSPACE_IMAGE_MAX: usize = 16 * 1024 * 1024;
const USERSPACE_MIN_VADDR: u64 = 0x0000_0000_0040_0000;
const USERSPACE_MAX_VADDR_EXCLUSIVE: u64 = 0x0000_8000_0000_0000;

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Header {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

#[derive(Clone, Copy)]
pub struct LoadedSegment {
    pub vaddr: u64,
    pub file_size: u64,
    pub mem_size: u64,
    pub flags: u32,
    pub staging_offset: usize,
}

#[derive(Clone, Copy)]
pub struct LoadedInitImage {
    pub entry_virtual: u64,
    pub entry_staging: *const u8,
    pub image_base: u64,
    pub image_size: usize,
    pub segments: [LoadedSegment; MAX_LOAD_SEGMENTS],
    pub segment_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    InvalidHeader,
    UnsupportedElf,
    InvalidProgramHeader,
    NoLoadSegments,
    TooManySegments,
    ImageTooLarge,
    AddressOutOfRange,
    SegmentOutOfBounds,
    EntryOutOfBounds,
}

const EMPTY_SEGMENT: LoadedSegment = LoadedSegment {
    vaddr: 0,
    file_size: 0,
    mem_size: 0,
    flags: 0,
    staging_offset: 0,
};

static mut USERSPACE_IMAGE_STAGING: [u8; USERSPACE_IMAGE_MAX] = [0; USERSPACE_IMAGE_MAX];

pub fn load_elf64_image(image: &[u8]) -> Result<LoadedInitImage, LoadError> {
    let header = read_struct::<Elf64Header>(image, 0).ok_or(LoadError::InvalidHeader)?;

    if header.e_ident[0..4] != ELF_MAGIC {
        return Err(LoadError::InvalidHeader);
    }
    if header.e_ident[4] != ELF_CLASS_64 || header.e_ident[5] != ELF_DATA_LSB {
        return Err(LoadError::UnsupportedElf);
    }
    if (header.e_type != ELF_TYPE_EXEC && header.e_type != ELF_TYPE_DYN)
        || header.e_machine != ELF_MACHINE_X86_64
        || header.e_version != ELF_VERSION_CURRENT
    {
        return Err(LoadError::UnsupportedElf);
    }
    if header.e_phentsize as usize != size_of::<Elf64ProgramHeader>() {
        return Err(LoadError::InvalidProgramHeader);
    }

    let phoff = header.e_phoff as usize;
    let phnum = header.e_phnum as usize;
    let phent = header.e_phentsize as usize;

    let ph_table_end = phoff
        .checked_add(phnum.checked_mul(phent).ok_or(LoadError::InvalidProgramHeader)?)
        .ok_or(LoadError::InvalidProgramHeader)?;
    if ph_table_end > image.len() {
        return Err(LoadError::InvalidProgramHeader);
    }

    let mut min_vaddr = u64::MAX;
    let mut max_vaddr = 0u64;
    let mut loadable_segments = 0usize;

    for i in 0..phnum {
        let off = phoff + i * phent;
        let ph = read_struct::<Elf64ProgramHeader>(image, off).ok_or(LoadError::InvalidProgramHeader)?;
        if ph.p_type != PT_LOAD {
            continue;
        }

        if ph.p_memsz < ph.p_filesz {
            return Err(LoadError::InvalidProgramHeader);
        }
        if ph.p_memsz == 0 {
            continue;
        }

        let seg_end = ph
            .p_vaddr
            .checked_add(ph.p_memsz)
            .ok_or(LoadError::InvalidProgramHeader)?;
        if ph.p_vaddr < USERSPACE_MIN_VADDR
            || seg_end > USERSPACE_MAX_VADDR_EXCLUSIVE
            || seg_end <= ph.p_vaddr
        {
            return Err(LoadError::AddressOutOfRange);
        }
        min_vaddr = min_vaddr.min(ph.p_vaddr);
        max_vaddr = max_vaddr.max(seg_end);
        loadable_segments += 1;
    }

    if loadable_segments == 0 {
        return Err(LoadError::NoLoadSegments);
    }
    if loadable_segments > MAX_LOAD_SEGMENTS {
        return Err(LoadError::TooManySegments);
    }

    let image_size = (max_vaddr - min_vaddr) as usize;
    if image_size == 0 || image_size > USERSPACE_IMAGE_MAX {
        return Err(LoadError::ImageTooLarge);
    }
    if min_vaddr < USERSPACE_MIN_VADDR || max_vaddr > USERSPACE_MAX_VADDR_EXCLUSIVE {
        return Err(LoadError::AddressOutOfRange);
    }

    unsafe {
        let staging_ptr = core::ptr::addr_of_mut!(USERSPACE_IMAGE_STAGING).cast::<u8>();
        core::ptr::write_bytes(staging_ptr, 0, image_size);
    }

    let mut segments = [EMPTY_SEGMENT; MAX_LOAD_SEGMENTS];
    let mut seg_idx = 0usize;

    for i in 0..phnum {
        let off = phoff + i * phent;
        let ph = read_struct::<Elf64ProgramHeader>(image, off).ok_or(LoadError::InvalidProgramHeader)?;
        if ph.p_type != PT_LOAD || ph.p_memsz == 0 {
            continue;
        }

        let src_start = ph.p_offset as usize;
        let src_end = src_start
            .checked_add(ph.p_filesz as usize)
            .ok_or(LoadError::SegmentOutOfBounds)?;
        if src_end > image.len() {
            return Err(LoadError::SegmentOutOfBounds);
        }

        let dst_start = (ph.p_vaddr - min_vaddr) as usize;
        let dst_file_end = dst_start
            .checked_add(ph.p_filesz as usize)
            .ok_or(LoadError::SegmentOutOfBounds)?;
        let dst_mem_end = dst_start
            .checked_add(ph.p_memsz as usize)
            .ok_or(LoadError::SegmentOutOfBounds)?;

        if dst_mem_end > image_size || dst_file_end > image_size {
            return Err(LoadError::SegmentOutOfBounds);
        }

        unsafe {
            let dst = core::ptr::addr_of_mut!(USERSPACE_IMAGE_STAGING)
                .cast::<u8>()
                .add(dst_start);
            core::ptr::copy_nonoverlapping(image.as_ptr().add(src_start), dst, ph.p_filesz as usize);
        }

        segments[seg_idx] = LoadedSegment {
            vaddr: ph.p_vaddr,
            file_size: ph.p_filesz,
            mem_size: ph.p_memsz,
            flags: ph.p_flags,
            staging_offset: dst_start,
        };
        seg_idx += 1;
    }

    if header.e_entry < USERSPACE_MIN_VADDR || header.e_entry >= USERSPACE_MAX_VADDR_EXCLUSIVE {
        return Err(LoadError::AddressOutOfRange);
    }

    let entry_off = (header.e_entry)
        .checked_sub(min_vaddr)
        .ok_or(LoadError::EntryOutOfBounds)? as usize;
    if entry_off >= image_size {
        return Err(LoadError::EntryOutOfBounds);
    }

    let entry_staging = unsafe {
        core::ptr::addr_of!(USERSPACE_IMAGE_STAGING)
            .cast::<u8>()
            .add(entry_off)
    };

    Ok(LoadedInitImage {
        entry_virtual: header.e_entry,
        entry_staging,
        image_base: min_vaddr,
        image_size,
        segments,
        segment_count: seg_idx,
    })
}

fn read_struct<T: Copy>(bytes: &[u8], offset: usize) -> Option<T> {
    let end = offset.checked_add(size_of::<T>())?;
    if end > bytes.len() {
        return None;
    }

    let ptr = unsafe { bytes.as_ptr().add(offset).cast::<T>() };
    Some(unsafe { core::ptr::read_unaligned(ptr) })
}
