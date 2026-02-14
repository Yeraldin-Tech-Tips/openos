use core::{
    arch::asm,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    arch::x86_64::serial,
    boot::MemoryMap,
};

const PAGE_SIZE: usize = 4096;
const TABLE_ENTRIES: usize = 512;

const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITABLE: u64 = 1 << 1;
const PTE_USER: u64 = 1 << 2;
const PTE_HUGE: u64 = 1 << 7;
const PTE_NX: u64 = 1 << 63;
const PHYS_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

const MAX_PAGE_TABLES: usize = 512;
const MAX_USER_FRAMES: usize = 4608;
pub const MAX_USER_ADDRESS_SPACES: usize = 32;

const MAX_PAGE_TABLES_PER_SPACE: usize = 128;
const MAX_USER_FRAMES_PER_SPACE: usize = 1024;

const LOWER_CANONICAL_MAX: u64 = 0x0000_7FFF_FFFF_FFFF;

const LOW_IDENTITY_SIZE: u64 = 4 * 1024 * 1024 * 1024;
const LOW_IDENTITY_PDPT_COUNT: usize = (LOW_IDENTITY_SIZE / (1024 * 1024 * 1024)) as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddressSpaceId(pub u16);

impl AddressSpaceId {
    pub const INVALID: Self = Self(u16::MAX);
}

#[repr(C, align(4096))]
#[derive(Clone, Copy)]
struct PageTable {
    entries: [u64; TABLE_ENTRIES],
}

impl PageTable {
    const fn zeroed() -> Self {
        Self {
            entries: [0; TABLE_ENTRIES],
        }
    }
}

#[repr(C, align(4096))]
#[derive(Clone, Copy)]
struct PageFrame {
    bytes: [u8; PAGE_SIZE],
}

impl PageFrame {
    const fn zeroed() -> Self {
        Self {
            bytes: [0; PAGE_SIZE],
        }
    }
}

#[derive(Clone, Copy)]
struct UserAddressSpace {
    in_use: bool,
    pml4: PageTable,
    low_pdpt: PageTable,
    low_pd: [PageTable; LOW_IDENTITY_PDPT_COUNT],
    page_table_refs: [u16; MAX_PAGE_TABLES_PER_SPACE],
    page_table_ref_count: usize,
    frame_refs: [u16; MAX_USER_FRAMES_PER_SPACE],
    frame_ref_count: usize,
}

impl UserAddressSpace {
    const fn empty() -> Self {
        Self {
            in_use: false,
            pml4: PageTable::zeroed(),
            low_pdpt: PageTable::zeroed(),
            low_pd: [PageTable::zeroed(); LOW_IDENTITY_PDPT_COUNT],
            page_table_refs: [0; MAX_PAGE_TABLES_PER_SPACE],
            page_table_ref_count: 0,
            frame_refs: [0; MAX_USER_FRAMES_PER_SPACE],
            frame_ref_count: 0,
        }
    }
}

static NEXT_PAGE_TABLE: AtomicUsize = AtomicUsize::new(0);
static NEXT_USER_FRAME: AtomicUsize = AtomicUsize::new(0);
static FREE_PAGE_TABLE_COUNT: AtomicUsize = AtomicUsize::new(0);
static FREE_USER_FRAME_COUNT: AtomicUsize = AtomicUsize::new(0);

static mut ADDRESS_SPACES: [UserAddressSpace; MAX_USER_ADDRESS_SPACES] =
    [UserAddressSpace::empty(); MAX_USER_ADDRESS_SPACES];
static mut PAGE_TABLE_POOL: [PageTable; MAX_PAGE_TABLES] = [PageTable::zeroed(); MAX_PAGE_TABLES];
static mut USER_FRAME_POOL: [PageFrame; MAX_USER_FRAMES] = [PageFrame::zeroed(); MAX_USER_FRAMES];
static mut FREE_PAGE_TABLE_STACK: [u16; MAX_PAGE_TABLES] = [0; MAX_PAGE_TABLES];
static mut FREE_USER_FRAME_STACK: [u16; MAX_USER_FRAMES] = [0; MAX_USER_FRAMES];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserMapError {
    InvalidImage,
    InvalidEntry,
    InvalidRange,
    AddressOutOfRange,
    AlreadyMapped,
    AddressSpaceTableFull,
    InvalidAddressSpace,
    ResourceTrackingOverflow,
    PageTablePoolExhausted,
    UserFramePoolExhausted,
}

pub fn init(_map: MemoryMap) {
    reset_allocator_state();
    reset_address_space_table();
}

pub fn map_user_task_image(
    entry_virtual: u64,
    entry_staging: *const u8,
    image_base: u64,
    image_size: usize,
    user_stack_top: u64,
    user_stack_size: usize,
) -> Result<AddressSpaceId, UserMapError> {
    if image_size == 0 || entry_staging.is_null() || user_stack_size == 0 {
        return Err(UserMapError::InvalidImage);
    }

    let image_end = image_base
        .checked_add(image_size as u64)
        .ok_or(UserMapError::InvalidImage)?;
    if image_end <= image_base
        || entry_virtual < image_base
        || entry_virtual >= image_end
        || !is_lower_canonical(image_base)
        || !is_lower_canonical(image_end)
    {
        return Err(UserMapError::InvalidImage);
    }

    let user_stack_bottom = user_stack_top
        .checked_sub(user_stack_size as u64)
        .ok_or(UserMapError::AddressOutOfRange)?;
    if !is_lower_canonical(user_stack_top) || !is_lower_canonical(user_stack_bottom) {
        return Err(UserMapError::AddressOutOfRange);
    }

    let entry_offset = entry_virtual
        .checked_sub(image_base)
        .ok_or(UserMapError::InvalidEntry)? as usize;
    let staging_base = unsafe { entry_staging.sub(entry_offset) };

    let (asid, space_ptr) = unsafe { alloc_address_space()? };
    init_low_identity_kernel_map(space_ptr);

    if let Err(err) = map_image_pages(space_ptr, image_base, image_end, staging_base) {
        let _ = release_user_address_space(asid);
        return Err(err);
    }
    if let Err(err) = map_stack_pages(space_ptr, user_stack_bottom, user_stack_top) {
        let _ = release_user_address_space(asid);
        return Err(err);
    }

    serial::write_hex_u64("[openos-kernel] mm.asid=", asid.0 as u64);
    serial::write_hex_u64("[openos-kernel] mm.user_image_base=", image_base);
    serial::write_hex_u64("[openos-kernel] mm.user_image_size=", image_size as u64);
    serial::write_hex_u64("[openos-kernel] mm.user_stack_top=", user_stack_top);
    Ok(asid)
}

pub fn activate_user_address_space(asid: AddressSpaceId) {
    let space_ptr = unsafe {
        match address_space_ptr(asid) {
            Some(ptr) => ptr,
            None => {
                serial::write_line("[openos-kernel] mm.activate invalid asid");
                return;
            }
        }
    };

    unsafe {
        let cr3 = (core::ptr::addr_of!((*space_ptr).pml4) as u64) & PHYS_ADDR_MASK;
        asm!("mov cr3, {}", in(reg) cr3, options(nostack, preserves_flags));
    }
}

pub fn release_user_address_space(asid: AddressSpaceId) -> Result<(), UserMapError> {
    let space_ptr = unsafe { address_space_ptr(asid).ok_or(UserMapError::InvalidAddressSpace)? };

    unsafe {
        recycle_address_space_resources(space_ptr);
        *space_ptr = UserAddressSpace::empty();
    }
    Ok(())
}

pub fn map_user_range(
    asid: AddressSpaceId,
    virt_addr: u64,
    len: usize,
    writable: bool,
    executable: bool,
) -> Result<u64, UserMapError> {
    if len == 0 {
        return Err(UserMapError::InvalidRange);
    }

    let end = virt_addr
        .checked_add(len as u64)
        .ok_or(UserMapError::InvalidRange)?;
    if end <= virt_addr || !is_lower_canonical(virt_addr) || !is_lower_canonical(end - 1) {
        return Err(UserMapError::AddressOutOfRange);
    }

    let start_page = align_down(virt_addr);
    let end_page = align_up(end);
    let space_ptr = unsafe { address_space_ptr(asid).ok_or(UserMapError::InvalidAddressSpace)? };
    let root = unsafe { core::ptr::addr_of_mut!((*space_ptr).pml4) };

    let mut page = start_page;
    let mut mapped_pages = 0usize;
    while page < end_page {
        let already_mapped = unsafe { user_page_present(root, page) };
        if already_mapped {
            if mapped_pages != 0 {
                let _ = unmap_user_range(asid, start_page, mapped_pages * PAGE_SIZE);
            }
            return Err(UserMapError::AlreadyMapped);
        }

        let frame = match alloc_user_frame(space_ptr) {
            Ok(frame) => frame,
            Err(err) => {
                if mapped_pages != 0 {
                    let _ = unmap_user_range(asid, start_page, mapped_pages * PAGE_SIZE);
                }
                return Err(err);
            }
        };

        let map_result = unsafe { map_user_page(root, space_ptr, page, frame as u64, writable, executable) };
        if let Err(err) = map_result {
            unsafe {
                recycle_user_frame_by_phys(space_ptr, frame as u64);
            }
            if mapped_pages != 0 {
                let _ = unmap_user_range(asid, start_page, mapped_pages * PAGE_SIZE);
            }
            return Err(err);
        }

        mapped_pages += 1;
        page += PAGE_SIZE as u64;
    }

    Ok(virt_addr)
}

pub fn unmap_user_range(asid: AddressSpaceId, virt_addr: u64, len: usize) -> Result<usize, UserMapError> {
    if len == 0 {
        return Err(UserMapError::InvalidRange);
    }

    let end = virt_addr
        .checked_add(len as u64)
        .ok_or(UserMapError::InvalidRange)?;
    if end <= virt_addr || !is_lower_canonical(virt_addr) || !is_lower_canonical(end - 1) {
        return Err(UserMapError::AddressOutOfRange);
    }

    let start_page = align_down(virt_addr);
    let end_page = align_up(end);
    let space_ptr = unsafe { address_space_ptr(asid).ok_or(UserMapError::InvalidAddressSpace)? };
    let root = unsafe { core::ptr::addr_of_mut!((*space_ptr).pml4) };

    let mut page = start_page;
    let mut unmapped_pages = 0usize;
    while page < end_page {
        unsafe {
            if let Some(pte_ptr) = find_leaf_pte_mut(root, page) {
                let entry = *pte_ptr;
                let is_user_page = (entry & PTE_PRESENT) != 0 && (entry & PTE_USER) != 0;
                if is_user_page {
                    let phys = entry & PHYS_ADDR_MASK;
                    *pte_ptr = 0;
                    recycle_user_frame_by_phys(space_ptr, phys);
                    flush_page(page);
                    unmapped_pages += 1;
                }
            }
        }
        page += PAGE_SIZE as u64;
    }

    Ok(unmapped_pages)
}

unsafe fn alloc_address_space() -> Result<(AddressSpaceId, *mut UserAddressSpace), UserMapError> {
    let mut i = 0usize;
    while i < MAX_USER_ADDRESS_SPACES {
        let ptr = core::ptr::addr_of_mut!(ADDRESS_SPACES)
            .cast::<UserAddressSpace>()
            .add(i);
        if !(*ptr).in_use {
            *ptr = UserAddressSpace::empty();
            (*ptr).in_use = true;
            return Ok((AddressSpaceId(i as u16), ptr));
        }
        i += 1;
    }

    Err(UserMapError::AddressSpaceTableFull)
}

unsafe fn address_space_ptr(asid: AddressSpaceId) -> Option<*mut UserAddressSpace> {
    let idx = asid.0 as usize;
    if idx >= MAX_USER_ADDRESS_SPACES {
        return None;
    }

    let ptr = core::ptr::addr_of_mut!(ADDRESS_SPACES)
        .cast::<UserAddressSpace>()
        .add(idx);
    if !(*ptr).in_use {
        return None;
    }
    Some(ptr)
}

fn map_image_pages(
    space: *mut UserAddressSpace,
    image_base: u64,
    image_end: u64,
    staging_base: *const u8,
) -> Result<(), UserMapError> {
    let start = align_down(image_base);
    let end = align_up(image_end);
    let mut virt_page = start;

    while virt_page < end {
        let frame = alloc_user_frame(space)?;
        let page_start = virt_page;
        let page_end = virt_page + PAGE_SIZE as u64;

        let copy_start = page_start.max(image_base);
        let copy_end = page_end.min(image_end);
        if copy_start < copy_end {
            let dst_off = (copy_start - page_start) as usize;
            let src_off = (copy_start - image_base) as usize;
            let copy_len = (copy_end - copy_start) as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(staging_base.add(src_off), frame.add(dst_off), copy_len);
            }
        }

        unsafe {
            map_user_page(
                core::ptr::addr_of_mut!((*space).pml4),
                space,
                virt_page,
                frame as u64,
                true,
                true,
            )?;
        }
        virt_page += PAGE_SIZE as u64;
    }

    Ok(())
}

fn map_stack_pages(
    space: *mut UserAddressSpace,
    stack_bottom: u64,
    stack_top: u64,
) -> Result<(), UserMapError> {
    let start = align_down(stack_bottom);
    let end = align_up(stack_top);
    let mut virt_page = start;

    while virt_page < end {
        let frame = alloc_user_frame(space)?;
        unsafe {
            map_user_page(
                core::ptr::addr_of_mut!((*space).pml4),
                space,
                virt_page,
                frame as u64,
                true,
                false,
            )?;
        }
        virt_page += PAGE_SIZE as u64;
    }

    Ok(())
}

fn reset_allocator_state() {
    NEXT_PAGE_TABLE.store(0, Ordering::Release);
    NEXT_USER_FRAME.store(0, Ordering::Release);
    FREE_PAGE_TABLE_COUNT.store(0, Ordering::Release);
    FREE_USER_FRAME_COUNT.store(0, Ordering::Release);
}

fn reset_address_space_table() {
    unsafe {
        ADDRESS_SPACES = [UserAddressSpace::empty(); MAX_USER_ADDRESS_SPACES];
    }
}

fn init_low_identity_kernel_map(space: *mut UserAddressSpace) {
    unsafe {
        let pdpt_phys = (core::ptr::addr_of!((*space).low_pdpt) as u64) & PHYS_ADDR_MASK;
        (*space).pml4.entries[0] = pdpt_phys | PTE_PRESENT | PTE_WRITABLE;

        let mut pdpt_i = 0usize;
        while pdpt_i < LOW_IDENTITY_PDPT_COUNT {
            let pd_phys = (core::ptr::addr_of!((*space).low_pd[pdpt_i]) as u64) & PHYS_ADDR_MASK;
            (*space).low_pdpt.entries[pdpt_i] = pd_phys | PTE_PRESENT | PTE_WRITABLE;

            let mut pd_i = 0usize;
            while pd_i < TABLE_ENTRIES {
                let phys = (pdpt_i as u64) * (1024 * 1024 * 1024) as u64
                    + (pd_i as u64) * (2 * 1024 * 1024) as u64;
                (*space).low_pd[pdpt_i].entries[pd_i] = phys | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE;
                pd_i += 1;
            }

            pdpt_i += 1;
        }
    }
}

unsafe fn map_user_page(
    root: *mut PageTable,
    space: *mut UserAddressSpace,
    virt_addr: u64,
    phys_addr: u64,
    writable: bool,
    executable: bool,
) -> Result<(), UserMapError> {
    if !is_lower_canonical(virt_addr) {
        return Err(UserMapError::AddressOutOfRange);
    }

    let pml4_index = table_index(virt_addr, 39);
    let pdpt_index = table_index(virt_addr, 30);
    let pd_index = table_index(virt_addr, 21);
    let pt_index = table_index(virt_addr, 12);

    let pdpt = get_or_create_next_table(root, pml4_index, true, space)?;
    let pd = get_or_create_next_table(pdpt, pdpt_index, true, space)?;
    let pt = get_or_create_next_table(pd, pd_index, true, space)?;

    let mut flags = PTE_PRESENT | PTE_USER;
    if writable {
        flags |= PTE_WRITABLE;
    }
    if !executable {
        flags |= PTE_NX;
    }

    (*pt).entries[pt_index] = (phys_addr & PHYS_ADDR_MASK) | flags;
    Ok(())
}

unsafe fn user_page_present(root: *mut PageTable, virt_addr: u64) -> bool {
    let Some(pte_ptr) = find_leaf_pte_mut(root, virt_addr) else {
        return false;
    };
    let entry = *pte_ptr;
    (entry & PTE_PRESENT) != 0 && (entry & PTE_USER) != 0
}

unsafe fn find_leaf_pte_mut(root: *mut PageTable, virt_addr: u64) -> Option<*mut u64> {
    let pml4_index = table_index(virt_addr, 39);
    let pdpt_index = table_index(virt_addr, 30);
    let pd_index = table_index(virt_addr, 21);
    let pt_index = table_index(virt_addr, 12);

    let pml4e = (*root).entries[pml4_index];
    if (pml4e & PTE_PRESENT) == 0 {
        return None;
    }
    let pdpt = (pml4e & PHYS_ADDR_MASK) as *mut PageTable;

    let pdpte = (*pdpt).entries[pdpt_index];
    if (pdpte & PTE_PRESENT) == 0 || (pdpte & PTE_HUGE) != 0 {
        return None;
    }
    let pd = (pdpte & PHYS_ADDR_MASK) as *mut PageTable;

    let pde = (*pd).entries[pd_index];
    if (pde & PTE_PRESENT) == 0 || (pde & PTE_HUGE) != 0 {
        return None;
    }
    let pt = (pde & PHYS_ADDR_MASK) as *mut PageTable;

    Some(core::ptr::addr_of_mut!((*pt).entries[pt_index]))
}

unsafe fn get_or_create_next_table(
    table: *mut PageTable,
    index: usize,
    user_accessible: bool,
    space: *mut UserAddressSpace,
) -> Result<*mut PageTable, UserMapError> {
    let entry = (*table).entries[index];
    if (entry & PTE_PRESENT) != 0 {
        let mut updated = entry;
        if user_accessible && (updated & PTE_USER) == 0 {
            updated |= PTE_USER;
            (*table).entries[index] = updated;
        }
        return Ok((updated & PHYS_ADDR_MASK) as *mut PageTable);
    }

    let next = alloc_page_table(space)?;
    let mut flags = PTE_PRESENT | PTE_WRITABLE;
    if user_accessible {
        flags |= PTE_USER;
    }
    (*table).entries[index] = ((next as u64) & PHYS_ADDR_MASK) | flags;
    Ok(next)
}

unsafe fn alloc_page_table(space: *mut UserAddressSpace) -> Result<*mut PageTable, UserMapError> {
    let idx = if let Some(recycled) = pop_recycled_page_table_idx() {
        recycled
    } else {
        let next = NEXT_PAGE_TABLE.fetch_add(1, Ordering::AcqRel);
        if next >= MAX_PAGE_TABLES {
            return Err(UserMapError::PageTablePoolExhausted);
        }
        next
    };

    if let Err(err) = record_page_table_ref(space, idx) {
        recycle_page_table_idx(idx);
        return Err(err);
    }

    let table_ptr = page_table_ptr(idx);
    table_ptr.write(PageTable::zeroed());
    Ok(table_ptr)
}

fn alloc_user_frame(space: *mut UserAddressSpace) -> Result<*mut u8, UserMapError> {
    let idx = if let Some(recycled) = pop_recycled_frame_idx() {
        recycled
    } else {
        let next = NEXT_USER_FRAME.fetch_add(1, Ordering::AcqRel);
        if next >= MAX_USER_FRAMES {
            return Err(UserMapError::UserFramePoolExhausted);
        }
        next
    };

    unsafe {
        if let Err(err) = record_frame_ref(space, idx) {
            recycle_frame_idx(idx);
            return Err(err);
        }

        let frame_ptr = frame_ptr(idx);
        frame_ptr.write(PageFrame::zeroed());
        Ok(core::ptr::addr_of_mut!((*frame_ptr).bytes).cast::<u8>())
    }
}

unsafe fn record_page_table_ref(space: *mut UserAddressSpace, idx: usize) -> Result<(), UserMapError> {
    let count = (*space).page_table_ref_count;
    if count >= MAX_PAGE_TABLES_PER_SPACE {
        return Err(UserMapError::ResourceTrackingOverflow);
    }
    (*space).page_table_refs[count] = idx as u16;
    (*space).page_table_ref_count = count + 1;
    Ok(())
}

unsafe fn record_frame_ref(space: *mut UserAddressSpace, idx: usize) -> Result<(), UserMapError> {
    let count = (*space).frame_ref_count;
    if count >= MAX_USER_FRAMES_PER_SPACE {
        return Err(UserMapError::ResourceTrackingOverflow);
    }
    (*space).frame_refs[count] = idx as u16;
    (*space).frame_ref_count = count + 1;
    Ok(())
}

unsafe fn remove_frame_ref(space: *mut UserAddressSpace, idx: usize) -> bool {
    let mut i = 0usize;
    while i < (*space).frame_ref_count {
        if (*space).frame_refs[i] as usize == idx {
            let last = (*space).frame_ref_count - 1;
            (*space).frame_refs[i] = (*space).frame_refs[last];
            (*space).frame_refs[last] = 0;
            (*space).frame_ref_count = last;
            return true;
        }
        i += 1;
    }
    false
}

unsafe fn recycle_address_space_resources(space: *mut UserAddressSpace) {
    let mut i = 0usize;
    while i < (*space).page_table_ref_count {
        let idx = (*space).page_table_refs[i] as usize;
        page_table_ptr(idx).write(PageTable::zeroed());
        recycle_page_table_idx(idx);
        i += 1;
    }
    (*space).page_table_ref_count = 0;

    let mut j = 0usize;
    while j < (*space).frame_ref_count {
        let idx = (*space).frame_refs[j] as usize;
        frame_ptr(idx).write(PageFrame::zeroed());
        recycle_frame_idx(idx);
        j += 1;
    }
    (*space).frame_ref_count = 0;
}

fn pop_recycled_page_table_idx() -> Option<usize> {
    let count = FREE_PAGE_TABLE_COUNT.load(Ordering::Acquire);
    if count == 0 {
        return None;
    }

    let next_count = count - 1;
    FREE_PAGE_TABLE_COUNT.store(next_count, Ordering::Release);
    Some(unsafe { FREE_PAGE_TABLE_STACK[next_count] as usize })
}

fn pop_recycled_frame_idx() -> Option<usize> {
    let count = FREE_USER_FRAME_COUNT.load(Ordering::Acquire);
    if count == 0 {
        return None;
    }

    let next_count = count - 1;
    FREE_USER_FRAME_COUNT.store(next_count, Ordering::Release);
    Some(unsafe { FREE_USER_FRAME_STACK[next_count] as usize })
}

unsafe fn recycle_page_table_idx(idx: usize) {
    let count = FREE_PAGE_TABLE_COUNT.load(Ordering::Acquire);
    if count >= MAX_PAGE_TABLES {
        return;
    }

    FREE_PAGE_TABLE_STACK[count] = idx as u16;
    FREE_PAGE_TABLE_COUNT.store(count + 1, Ordering::Release);
}

unsafe fn recycle_frame_idx(idx: usize) {
    let count = FREE_USER_FRAME_COUNT.load(Ordering::Acquire);
    if count >= MAX_USER_FRAMES {
        return;
    }

    FREE_USER_FRAME_STACK[count] = idx as u16;
    FREE_USER_FRAME_COUNT.store(count + 1, Ordering::Release);
}

unsafe fn recycle_user_frame_by_phys(space: *mut UserAddressSpace, phys_addr: u64) {
    let Some(idx) = frame_idx_from_phys(phys_addr) else {
        return;
    };
    if !remove_frame_ref(space, idx) {
        return;
    }

    frame_ptr(idx).write(PageFrame::zeroed());
    recycle_frame_idx(idx);
}

unsafe fn page_table_ptr(idx: usize) -> *mut PageTable {
    core::ptr::addr_of_mut!(PAGE_TABLE_POOL)
        .cast::<PageTable>()
        .add(idx)
}

unsafe fn frame_ptr(idx: usize) -> *mut PageFrame {
    core::ptr::addr_of_mut!(USER_FRAME_POOL)
        .cast::<PageFrame>()
        .add(idx)
}

fn frame_idx_from_phys(phys_addr: u64) -> Option<usize> {
    let phys = (phys_addr & PHYS_ADDR_MASK) as usize;
    let base = core::ptr::addr_of!(USER_FRAME_POOL).cast::<PageFrame>() as usize;
    let bytes = MAX_USER_FRAMES * core::mem::size_of::<PageFrame>();
    let end = base.checked_add(bytes)?;
    if phys < base || phys >= end {
        return None;
    }

    let off = phys - base;
    let frame_size = core::mem::size_of::<PageFrame>();
    if (off % frame_size) != 0 {
        return None;
    }
    Some(off / frame_size)
}

fn table_index(addr: u64, shift: u64) -> usize {
    ((addr >> shift) & 0x1FF) as usize
}

const fn align_down(addr: u64) -> u64 {
    addr & !(PAGE_SIZE as u64 - 1)
}

const fn align_up(addr: u64) -> u64 {
    (addr + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1)
}

const fn is_lower_canonical(addr: u64) -> bool {
    addr <= LOWER_CANONICAL_MAX
}

unsafe fn flush_page(addr: u64) {
    asm!("invlpg [{}]", in(reg) addr, options(nostack, preserves_flags));
}
