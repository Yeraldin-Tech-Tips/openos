#![no_std]
#![no_main]

mod arch;
mod boot;
mod drivers;
mod fs;
mod init;
mod input;
mod ipc;
mod lifecycle;
mod mm;
mod net;
mod sched;
mod start;
mod syscall;
mod ui;

use boot::BootInfo;
use ui::framebuffer::{self, FrameBufferConsole};

#[no_mangle]
pub extern "sysv64" fn openos_kernel_main(boot_info_ptr: *const BootInfo) -> ! {
    arch::x86_64::serial::init();
    arch::x86_64::serial::write_line("[openos-kernel] entry");
    arch::x86_64::serial::write_hex_u64("[openos-kernel] boot_info_ptr=", boot_info_ptr as u64);

    let boot = unsafe { boot_info_ptr.as_ref() }.expect("boot info pointer must be valid");
    arch::x86_64::serial::write_hex_u64("[openos-kernel] boot.magic=", boot.magic);
    arch::x86_64::serial::write_hex_u64("[openos-kernel] boot.version=", boot.version as u64);
    arch::x86_64::serial::write_hex_u64("[openos-kernel] boot.flags=", boot.flags as u64);

    if !boot.is_valid() {
        arch::x86_64::serial::write_line("[openos-kernel] invalid boot info");
        loop {
            arch::x86_64::halt();
        }
    }

    arch::x86_64::early_init();
    mm::init(boot.memory_map);
    sched::init();
    lifecycle::init();
    fs::init();
    ipc::init();
    input::init();
    drivers::init();
    net::init();
    syscall::init();

    arch::x86_64::serial::write_line("[openos-kernel] core subsystems initialized");
    let pid1 = init::launch_pid1(boot);
    arch::x86_64::serial::write_hex_u64("[openos-kernel] pid1=", pid1.0 as u64);
    if let Some(task) = sched::task_descriptor(pid1) {
        arch::x86_64::serial::write_hex_u64(
            "[openos-kernel] pid1.entry_virtual=",
            task.context.instruction_pointer,
        );
        arch::x86_64::serial::write_hex_u64(
            "[openos-kernel] pid1.asid=",
            task.address_space.0 as u64,
        );
        arch::x86_64::serial::write_hex_u64("[openos-kernel] pid1.image_base=", task.image_base);
        arch::x86_64::serial::write_hex_u64(
            "[openos-kernel] pid1.image_size=",
            task.image_size as u64,
        );
        arch::x86_64::serial::write_hex_u64(
            "[openos-kernel] pid1.segment_count=",
            task.segment_count as u64,
        );
    } else {
        arch::x86_64::serial::write_line("[openos-kernel] pid1 task descriptor missing");
    }

    if boot.has_framebuffer() {
        framebuffer::install(boot.framebuffer);
        let mut console = unsafe { FrameBufferConsole::new(boot.framebuffer) };
        console.clear(0x101418);
        console.write_line("OpenOS kernel booted");
        console.write_line("PID1 ring3 dispatch active");
        console.write_line("Userspace IPC queue active");
    }

    arch::x86_64::serial::write_line("[openos-kernel] dispatch pid1 (ring3)");
    if let Err(err) = sched::dispatch_task(pid1) {
        match err {
            sched::DispatchTaskError::MissingTask => arch::x86_64::serial::write_line(
                "[openos-kernel] pid1 dispatch failed: missing task",
            ),
            sched::DispatchTaskError::InvalidContext => arch::x86_64::serial::write_line(
                "[openos-kernel] pid1 dispatch failed: invalid context",
            ),
        }
    }

    arch::x86_64::serial::write_line("[openos-kernel] halt loop");

    loop {
        arch::x86_64::halt();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    arch::x86_64::serial::write_line("[openos-kernel] panic");
    loop {
        arch::x86_64::halt();
    }
}
