#![no_std]
#![no_main]

use core::arch::asm;

use abi::ipc::{UiChannel, UiMessageHeader, UiMessageKind};
use openos_syscall::{
    fs_close, fs_open, fs_read, fs_write, gfx_present, gfx_submit_scene, input_read,
    input_subscribe, ipc_send, net_connect, net_recv, net_send, net_socket, proc_exit, vm_map,
    vm_unmap,
};

const GESTURE_HOME: u64 = 0;
const GESTURE_APP_SWITCHER_LEFT: u64 = 1;
const GESTURE_APP_SWITCHER_RIGHT: u64 = 2;
const GESTURE_CONTROL_CENTER: u64 = 3;
const GESTURE_NOTIFICATION_CENTER: u64 = 4;

const IDLE_EXIT_THRESHOLD: usize = 3000;
const WORKSPACE_COUNT: u8 = 3;

#[no_mangle]
pub extern "sysv64" fn _start(_task_id: u64) -> ! {
    let _ = fs_write(1, b"[openos-app-shell] started\r\n");
    let _ = input_subscribe(true);

    log_file_prefix(b"/proc/apps", b"[openos-app-shell] apps: ");
    log_file_prefix(
        b"/proc/launcher-history",
        b"[openos-app-shell] launch-hist: ",
    );

    let mut mapped_addr = 0u64;
    let vm_result = vm_map(0, 4096, 1);
    if vm_result.code == 0 {
        mapped_addr = vm_result.value;
        write_workspace_state(mapped_addr, 0);
        let _ = fs_write(1, b"[openos-app-shell] vm state mapped\r\n");
    }

    render_scene(0, true);
    run_loopback_ping();

    let mut workspace = 0u8;
    let mut idle_ticks = 0usize;

    loop {
        let input = input_read();
        if input.code == 0 {
            idle_ticks = 0;
            match input.value {
                GESTURE_HOME => {
                    let _ = fs_write(1, b"[openos-app-shell] home exit\r\n");
                    break;
                }
                GESTURE_APP_SWITCHER_LEFT => {
                    workspace = (workspace + WORKSPACE_COUNT - 1) % WORKSPACE_COUNT;
                    write_workspace_state(mapped_addr, workspace);
                    render_scene(workspace, true);
                    let _ = fs_write(1, b"[openos-app-shell] workspace left\r\n");
                }
                GESTURE_APP_SWITCHER_RIGHT => {
                    workspace = (workspace + 1) % WORKSPACE_COUNT;
                    write_workspace_state(mapped_addr, workspace);
                    render_scene(workspace, true);
                    let _ = fs_write(1, b"[openos-app-shell] workspace right\r\n");
                }
                GESTURE_CONTROL_CENTER => {
                    send_ipc(
                        UiChannel::ControlCenter,
                        UiMessageKind::ToggleControl,
                        b"shell:toggle-control",
                    );
                }
                GESTURE_NOTIFICATION_CENTER => {
                    send_ipc(
                        UiChannel::NotificationCenter,
                        UiMessageKind::PublishNotification,
                        b"shell:new-notification",
                    );
                }
                _ => {}
            }
        } else if input.code == -11 {
            idle_ticks += 1;
            if (idle_ticks % 600) == 0 {
                run_loopback_ping();
            }
            if idle_ticks >= IDLE_EXIT_THRESHOLD {
                let _ = fs_write(1, b"[openos-app-shell] idle timeout exit\r\n");
                break;
            }
        } else {
            let _ = fs_write(1, b"[openos-app-shell] input error\r\n");
            break;
        }
        idle_pause();
    }

    if mapped_addr != 0 {
        let _ = vm_unmap(mapped_addr, 4096);
    }

    let _ = proc_exit(17);
    halt_forever();
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    halt_forever()
}

fn render_scene(workspace: u8, log: bool) {
    let (top, bottom, dock, dock_height) = match workspace {
        0 => (0xDCEBFF, 0xB7CCF2, 0xE8F2FF, 108),
        1 => (0xD5F1FF, 0xA8D5F1, 0xE4F6FF, 108),
        _ => (0xE2ECFF, 0xB9CAF0, 0xECF3FF, 108),
    };

    let submit = gfx_submit_scene(top, bottom, dock, dock_height);
    if submit.code == 0 {
        let present = gfx_present();
        if present.code == 0 && log {
            let _ = fs_write(1, b"[openos-app-shell] scene updated\r\n");
        }
    }
}

fn write_workspace_state(mapped_addr: u64, workspace: u8) {
    if mapped_addr == 0 {
        return;
    }

    unsafe {
        let ptr = mapped_addr as *mut u8;
        ptr.write(workspace);
        ptr.add(1).write(workspace + b'0');
    }
}

fn run_loopback_ping() {
    let sock_res = net_socket(2, 1, 0);
    if sock_res.code != 0 {
        return;
    }

    let sock = sock_res.value;
    if net_connect(sock, b"loopback").code != 0 {
        return;
    }

    if net_send(sock, b"shell-heartbeat").code != 0 {
        return;
    }

    let mut recv_buf = [0u8; 24];
    let recv = net_recv(sock, &mut recv_buf);
    if recv.code == 0 && recv.value as usize == 15 && &recv_buf[..15] == b"shell-heartbeat" {
        let _ = fs_write(1, b"[openos-app-shell] net ok\r\n");
    }
}

fn send_ipc(channel: UiChannel, kind: UiMessageKind, payload: &[u8]) {
    let header = UiMessageHeader {
        channel,
        kind,
        payload_len: payload.len() as u16,
    };

    let result = ipc_send(&header, payload);
    if result.code == 0 {
        let _ = fs_write(1, b"[openos-app-shell] ipc sent\r\n");
    } else {
        let _ = fs_write(1, b"[openos-app-shell] ipc send error\r\n");
    }
}

fn log_file_prefix(path: &[u8], prefix: &[u8]) {
    let open_res = fs_open(path, 0);
    if open_res.code != 0 {
        return;
    }

    let fd = open_res.value;
    let mut buf = [0u8; 192];
    let read_res = fs_read(fd, &mut buf);
    if read_res.code == 0 && read_res.value != 0 {
        let _ = fs_write(1, prefix);
        let _ = fs_write(1, &buf[..read_res.value as usize]);
    }
    let _ = fs_close(fd);
}

fn idle_pause() {
    core::hint::spin_loop();
    unsafe {
        asm!("pause", options(nomem, nostack, preserves_flags));
    }
}

fn halt_forever() -> ! {
    loop {
        idle_pause();
    }
}
