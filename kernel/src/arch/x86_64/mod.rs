use core::arch::asm;

pub mod interrupts;
pub mod serial;
pub mod tables;
pub mod user;

pub fn early_init() {
    tables::init();
    serial::init();
    interrupts::init();
    serial::write_line("[openos-kernel] early_init");
}

pub fn halt() {
    unsafe {
        asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}
