use core::{
    arch::{asm, global_asm},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

use crate::arch::x86_64::{serial, tables};

pub const IRQ_BASE_VECTOR: u8 = 0x20;
pub const TIMER_VECTOR: u8 = IRQ_BASE_VECTOR;
pub const KEYBOARD_VECTOR: u8 = IRQ_BASE_VECTOR + 1;
pub const MOUSE_VECTOR: u8 = IRQ_BASE_VECTOR + 12;

const UD_VECTOR: u8 = 0x06;
const GP_VECTOR: u8 = 0x0D;
const PF_VECTOR: u8 = 0x0E;

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;
const PIC_EOI: u8 = 0x20;

const PIT_COMMAND: u16 = 0x43;
const PIT_CHANNEL0: u16 = 0x40;
const PIT_INPUT_HZ: u32 = 1_193_182;
const PIT_FREQUENCY_HZ: u32 = 100;
const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;
const PS2_COMMAND_PORT: u16 = 0x64;
const PS2_CMD_ENABLE_AUX: u8 = 0xA8;
const PS2_CMD_READ_CONFIG: u8 = 0x20;
const PS2_CMD_WRITE_CONFIG: u8 = 0x60;
const PS2_CMD_WRITE_AUX: u8 = 0xD4;
const PS2_MOUSE_DEFAULTS: u8 = 0xF6;
const PS2_MOUSE_ENABLE_STREAMING: u8 = 0xF4;
const PS2_ACK: u8 = 0xFA;
const PS2_STATUS_OUTPUT_FULL: u8 = 1 << 0;
const PS2_STATUS_INPUT_FULL: u8 = 1 << 1;
const PS2_TIMEOUT_SPINS: usize = 100_000;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InterruptFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,
    pub vector: u64,
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

static TIMER_IRQ_ENABLED: AtomicBool = AtomicBool::new(false);
static TIMER_TICK_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    TIMER_IRQ_ENABLED.store(false, Ordering::Release);
    TIMER_TICK_COUNT.store(0, Ordering::Release);

    tables::install_kernel_interrupt(
        UD_VECTOR,
        openos_fault_ud_entry as *const () as usize as u64,
    );
    tables::install_kernel_interrupt(
        GP_VECTOR,
        openos_fault_gp_entry as *const () as usize as u64,
    );
    tables::install_kernel_interrupt(
        PF_VECTOR,
        openos_fault_pf_entry as *const () as usize as u64,
    );
    tables::install_kernel_interrupt(
        TIMER_VECTOR,
        openos_irq_timer_entry as *const () as usize as u64,
    );
    tables::install_kernel_interrupt(
        KEYBOARD_VECTOR,
        openos_irq_keyboard_entry as *const () as usize as u64,
    );
    tables::install_kernel_interrupt(
        MOUSE_VECTOR,
        openos_irq_mouse_entry as *const () as usize as u64,
    );

    init_pic();
    mask_all_irqs();
    program_pit(PIT_FREQUENCY_HZ);
    init_ps2_mouse();

    serial::write_hex_u64("[openos-kernel] irq.timer.vector=", TIMER_VECTOR as u64);
    serial::write_hex_u64("[openos-kernel] pit.hz=", PIT_FREQUENCY_HZ as u64);
}

pub fn enable_timer_irq() {
    unsafe {
        let master_mask = inb(PIC1_DATA) & !0x07;
        let slave_mask = inb(PIC2_DATA) & !(1 << 4);
        outb(PIC1_DATA, master_mask);
        outb(PIC2_DATA, slave_mask);
    }

    if !TIMER_IRQ_ENABLED.swap(true, Ordering::AcqRel) {
        serial::write_line("[openos-kernel] timer irq enabled");
    }
}

pub fn enable_input_irqs() {
    unsafe {
        let mut master_mask = inb(PIC1_DATA);
        master_mask &= !((1 << 1) | (1 << 2));
        master_mask |= 1 << 0;
        let slave_mask = inb(PIC2_DATA) & !(1 << 4);
        outb(PIC1_DATA, master_mask);
        outb(PIC2_DATA, slave_mask);
    }

    serial::write_line("[openos-kernel] input irqs enabled");
}

pub fn disable_timer_irq() {
    unsafe {
        let master_mask = inb(PIC1_DATA) | 0x07;
        let slave_mask = inb(PIC2_DATA) | (1 << 4);
        outb(PIC1_DATA, master_mask);
        outb(PIC2_DATA, slave_mask);
    }

    if TIMER_IRQ_ENABLED.swap(false, Ordering::AcqRel) {
        serial::write_line("[openos-kernel] timer irq disabled");
    }
}

#[no_mangle]
pub extern "C" fn openos_interrupt_dispatch(frame: &mut InterruptFrame) {
    match frame.vector as u8 {
        TIMER_VECTOR => {
            let tick = TIMER_TICK_COUNT.fetch_add(1, Ordering::AcqRel) + 1;
            if tick == 1 {
                serial::write_hex_u64("[openos-kernel] timer.tick=", tick);
            }
            crate::sched::on_timer_tick(frame, tick);
            acknowledge_irq(TIMER_VECTOR);
        }
        KEYBOARD_VECTOR => {
            let scancode = unsafe { inb(PS2_DATA_PORT) };
            crate::drivers::hid_on_ps2_scancode(scancode);
            acknowledge_irq(KEYBOARD_VECTOR);
        }
        MOUSE_VECTOR => {
            let byte = unsafe { inb(PS2_DATA_PORT) };
            crate::drivers::hid_on_ps2_mouse_byte(byte);
            acknowledge_irq(MOUSE_VECTOR);
        }
        UD_VECTOR | GP_VECTOR | PF_VECTOR => handle_fault(frame),
        _ => {}
    }
}

fn handle_fault(frame: &mut InterruptFrame) {
    serial::write_hex_u64("[openos-kernel] fault.vector=", frame.vector);
    serial::write_hex_u64("[openos-kernel] fault.error=", frame.error_code);
    serial::write_hex_u64("[openos-kernel] fault.rip=", frame.rip);
    serial::write_hex_u64("[openos-kernel] fault.cs=", frame.cs);
    if frame.vector as u8 == PF_VECTOR {
        serial::write_hex_u64("[openos-kernel] fault.cr2=", read_cr2());
    }

    let user_mode = (frame.cs & 0x3) == 0x3;
    if user_mode && crate::sched::handle_user_fault(frame, frame.vector as u8, frame.error_code) {
        return;
    }

    serial::write_line("[openos-kernel] fatal fault halt");
    halt_forever();
}

fn halt_forever() -> ! {
    unsafe {
        asm!("cli", options(nomem, nostack, preserves_flags));
        loop {
            asm!("hlt", options(nomem, nostack));
        }
    }
}

fn acknowledge_irq(vector: u8) {
    if vector < IRQ_BASE_VECTOR {
        return;
    }

    let irq = vector - IRQ_BASE_VECTOR;
    unsafe {
        if irq >= 8 {
            outb(PIC2_COMMAND, PIC_EOI);
        }
        outb(PIC1_COMMAND, PIC_EOI);
    }
}

fn init_pic() {
    unsafe {
        let mask1 = inb(PIC1_DATA);
        let mask2 = inb(PIC2_DATA);

        outb(PIC1_COMMAND, 0x11);
        io_wait();
        outb(PIC2_COMMAND, 0x11);
        io_wait();

        outb(PIC1_DATA, IRQ_BASE_VECTOR);
        io_wait();
        outb(PIC2_DATA, IRQ_BASE_VECTOR + 8);
        io_wait();

        outb(PIC1_DATA, 0x04);
        io_wait();
        outb(PIC2_DATA, 0x02);
        io_wait();

        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();

        outb(PIC1_DATA, mask1);
        outb(PIC2_DATA, mask2);
    }
}

fn mask_all_irqs() {
    unsafe {
        outb(PIC1_DATA, 0xFF);
        outb(PIC2_DATA, 0xFF);
    }
}

fn program_pit(freq_hz: u32) {
    let freq = freq_hz.max(1);
    let divisor = (PIT_INPUT_HZ / freq).clamp(1, u16::MAX as u32) as u16;

    unsafe {
        outb(PIT_COMMAND, 0x36);
        outb(PIT_CHANNEL0, (divisor & 0xFF) as u8);
        outb(PIT_CHANNEL0, (divisor >> 8) as u8);
    }
}

fn init_ps2_mouse() {
    unsafe {
        if !ps2_wait_input_ready() {
            return;
        }
        outb(PS2_COMMAND_PORT, PS2_CMD_ENABLE_AUX);

        if !ps2_wait_input_ready() {
            return;
        }
        outb(PS2_COMMAND_PORT, PS2_CMD_READ_CONFIG);
        if !ps2_wait_output_full() {
            return;
        }
        let mut config = inb(PS2_DATA_PORT);
        config |= 0x02;
        config &= !0x20;

        if !ps2_wait_input_ready() {
            return;
        }
        outb(PS2_COMMAND_PORT, PS2_CMD_WRITE_CONFIG);
        if !ps2_wait_input_ready() {
            return;
        }
        outb(PS2_DATA_PORT, config);

        let _ = ps2_write_mouse(PS2_MOUSE_DEFAULTS);
        let _ = ps2_write_mouse(PS2_MOUSE_ENABLE_STREAMING);
    }
}

unsafe fn ps2_write_mouse(command: u8) -> bool {
    if !ps2_wait_input_ready() {
        return false;
    }
    outb(PS2_COMMAND_PORT, PS2_CMD_WRITE_AUX);
    if !ps2_wait_input_ready() {
        return false;
    }
    outb(PS2_DATA_PORT, command);
    if !ps2_wait_output_full() {
        return false;
    }
    inb(PS2_DATA_PORT) == PS2_ACK
}

unsafe fn ps2_wait_input_ready() -> bool {
    let mut spins = 0usize;
    while spins < PS2_TIMEOUT_SPINS {
        if (inb(PS2_STATUS_PORT) & PS2_STATUS_INPUT_FULL) == 0 {
            return true;
        }
        core::hint::spin_loop();
        spins += 1;
    }
    false
}

unsafe fn ps2_wait_output_full() -> bool {
    let mut spins = 0usize;
    while spins < PS2_TIMEOUT_SPINS {
        if (inb(PS2_STATUS_PORT) & PS2_STATUS_OUTPUT_FULL) != 0 {
            return true;
        }
        core::hint::spin_loop();
        spins += 1;
    }
    false
}

fn read_cr2() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

unsafe fn io_wait() {
    outb(0x80, 0);
}

unsafe fn outb(port: u16, value: u8) {
    asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags)
    );
}

unsafe fn inb(port: u16) -> u8 {
    let mut value: u8;
    asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nomem, nostack, preserves_flags)
    );
    value
}

global_asm!(
    r#"
    .global openos_fault_ud_entry
openos_fault_ud_entry:
    push 0
    push 6
    jmp openos_interrupt_common_entry

    .global openos_fault_gp_entry
openos_fault_gp_entry:
    push 13
    jmp openos_interrupt_common_entry

    .global openos_fault_pf_entry
openos_fault_pf_entry:
    push 14
    jmp openos_interrupt_common_entry

    .global openos_irq_timer_entry
openos_irq_timer_entry:
    push 0
    push 32
    jmp openos_interrupt_common_entry

    .global openos_irq_keyboard_entry
openos_irq_keyboard_entry:
    push 0
    push 33
    jmp openos_interrupt_common_entry

    .global openos_irq_mouse_entry
openos_irq_mouse_entry:
    push 0
    push 44
    jmp openos_interrupt_common_entry

openos_interrupt_common_entry:
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rdi, rsp
    call openos_interrupt_dispatch

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    add rsp, 16
    iretq
"#
);

unsafe extern "C" {
    fn openos_fault_ud_entry();
    fn openos_fault_gp_entry();
    fn openos_fault_pf_entry();
    fn openos_irq_timer_entry();
    fn openos_irq_keyboard_entry();
    fn openos_irq_mouse_entry();
}
