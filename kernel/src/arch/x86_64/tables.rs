use core::{arch::asm, mem::size_of};

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_DATA_SELECTOR: u16 = 0x1B;
pub const USER_CODE_SELECTOR: u16 = 0x23;
const TSS_SELECTOR: u16 = 0x28;

const INT80_VECTOR: u8 = 0x80;
const INTERRUPT_STACK_SIZE: usize = 16 * 1024;

const GDT_LEN: usize = 7;
const IDT_LEN: usize = 256;

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
struct Tss64 {
    _reserved0: u32,
    rsp: [u64; 3],
    _reserved1: u64,
    ist: [u64; 7],
    _reserved2: u64,
    _reserved3: u16,
    io_map_base: u16,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    zero: u32,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            zero: 0,
        }
    }

    fn interrupt_gate(handler_addr: u64, dpl: u8) -> Self {
        let type_attr = 0x8E | ((dpl & 0x3) << 5);
        Self {
            offset_low: handler_addr as u16,
            selector: KERNEL_CODE_SELECTOR,
            ist: 0,
            type_attr,
            offset_mid: (handler_addr >> 16) as u16,
            offset_high: (handler_addr >> 32) as u32,
            zero: 0,
        }
    }
}

static mut GDT: [u64; GDT_LEN] = [0; GDT_LEN];
static mut TSS: Tss64 = Tss64 {
    _reserved0: 0,
    rsp: [0; 3],
    _reserved1: 0,
    ist: [0; 7],
    _reserved2: 0,
    _reserved3: 0,
    io_map_base: 0,
};
static mut IDT: [IdtEntry; IDT_LEN] = [IdtEntry::missing(); IDT_LEN];
static mut INTERRUPT_STACK: [u8; INTERRUPT_STACK_SIZE] = [0; INTERRUPT_STACK_SIZE];

pub fn init() {
    unsafe {
        init_tss();
        init_gdt();
        load_gdt();
        load_tss();
        init_idt();
        load_idt();
    }
}

pub fn int80_vector() -> u8 {
    INT80_VECTOR
}

pub fn install_kernel_interrupt(vector: u8, handler_addr: u64) {
    unsafe {
        IDT[vector as usize] = IdtEntry::interrupt_gate(handler_addr, 0);
    }
}

pub fn install_user_interrupt(vector: u8, handler_addr: u64) {
    unsafe {
        IDT[vector as usize] = IdtEntry::interrupt_gate(handler_addr, 3);
    }
}

unsafe fn init_tss() {
    let stack_top =
        core::ptr::addr_of!(INTERRUPT_STACK).cast::<u8>() as u64 + INTERRUPT_STACK_SIZE as u64;
    TSS.rsp[0] = stack_top & !0xFu64;
    TSS.io_map_base = size_of::<Tss64>() as u16;
}

unsafe fn init_gdt() {
    // Kernel/user code/data segments.
    GDT[0] = 0;
    GDT[1] = 0x00AF_9A00_0000_FFFF;
    GDT[2] = 0x00AF_9200_0000_FFFF;
    GDT[3] = 0x00AF_F200_0000_FFFF;
    GDT[4] = 0x00AF_FA00_0000_FFFF;

    // 64-bit available TSS descriptor (16 bytes across GDT[5..=6]).
    let tss_base = core::ptr::addr_of!(TSS) as u64;
    let tss_limit = (size_of::<Tss64>() - 1) as u64;

    GDT[5] = (tss_limit & 0xFFFF)
        | ((tss_base & 0x00FF_FFFF) << 16)
        | (0x89u64 << 40)
        | (((tss_limit >> 16) & 0xF) << 48)
        | (((tss_base >> 24) & 0xFF) << 56);
    GDT[6] = tss_base >> 32;
}

unsafe fn load_gdt() {
    let gdtr = DescriptorTablePointer {
        limit: (size_of::<[u64; GDT_LEN]>() - 1) as u16,
        base: core::ptr::addr_of!(GDT) as u64,
    };

    asm!("lgdt [{}]", in(reg) &gdtr, options(readonly, nostack, preserves_flags));

    asm!(
        "mov ax, {data_sel:x}",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",
        "mov ss, ax",
        data_sel = in(reg) KERNEL_DATA_SELECTOR,
        options(nostack, preserves_flags)
    );

    // Reload CS with the kernel code selector.
    asm!(
        "push {code_sel}",
        "lea rax, [rip + 2f]",
        "push rax",
        "retfq",
        "2:",
        code_sel = const KERNEL_CODE_SELECTOR as u64,
        out("rax") _,
    );
}

unsafe fn load_tss() {
    asm!(
        "mov ax, {tss_sel:x}",
        "ltr ax",
        tss_sel = in(reg) TSS_SELECTOR,
        options(nostack, preserves_flags)
    );
}

unsafe fn init_idt() {
    let mut i = 0usize;
    while i < IDT_LEN {
        IDT[i] = IdtEntry::missing();
        i += 1;
    }
}

unsafe fn load_idt() {
    let idtr = DescriptorTablePointer {
        limit: (size_of::<[IdtEntry; IDT_LEN]>() - 1) as u16,
        base: core::ptr::addr_of!(IDT) as u64,
    };
    asm!("lidt [{}]", in(reg) &idtr, options(readonly, nostack, preserves_flags));
}
