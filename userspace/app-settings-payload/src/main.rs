#![no_std]
#![no_main]

use core::arch::asm;

use openos_syscall::{fs_close, fs_open, fs_read, fs_write, gfx_present, gfx_submit_scene, proc_exit};

#[no_mangle]
pub extern "sysv64" fn _start(_task_id: u64) -> ! {
    let _ = fs_write(1, b"[openos-app-settings] started\r\n");
    render_scene();
    log_file_prefix(b"/proc/self/status", b"[openos-app-settings] self: ");
    log_file_prefix(b"/proc/tasks", b"[openos-app-settings] tasks: ");

    let _ = proc_exit(18);
    halt_forever();
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    halt_forever()
}

fn render_scene() {
    let submit = gfx_submit_scene(0xE5FFF4, 0xC7F2E1, 0xEEFFF7, 110);
    if submit.code == 0 {
        let present = gfx_present();
        if present.code == 0 {
            let _ = fs_write(1, b"[openos-app-settings] scene presented\r\n");
        }
    }
}

fn log_file_prefix(path: &[u8], prefix: &[u8]) {
    let open_res = fs_open(path, 0);
    if open_res.code != 0 {
        return;
    }

    let fd = open_res.value;
    let mut buf = [0u8; 160];
    let read_res = fs_read(fd, &mut buf);
    if read_res.code == 0 && read_res.value != 0 {
        let _ = fs_write(1, prefix);
        let _ = fs_write(1, &buf[..read_res.value as usize]);
    }
    let _ = fs_close(fd);
}

fn halt_forever() -> ! {
    loop {
        core::hint::spin_loop();
        unsafe {
            asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
}
