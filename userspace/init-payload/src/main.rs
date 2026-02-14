#![no_std]
#![no_main]

use core::arch::asm;

use abi::ipc::{UiChannel, UiMessageHeader, UiMessageKind};
use openos_syscall::{
    fs_close, fs_open, fs_read, fs_write, gfx_present, gfx_submit_scene, input_read, input_subscribe,
    ipc_recv, ipc_send, net_connect, net_recv, net_send, net_socket, proc_exit, proc_spawn, proc_wait,
    vm_map, vm_unmap,
};

const GESTURE_HOME: u64 = 0;
const GESTURE_APP_SWITCHER_LEFT: u64 = 1;
const GESTURE_APP_SWITCHER_RIGHT: u64 = 2;
const GESTURE_CONTROL_CENTER: u64 = 3;
const GESTURE_NOTIFICATION_CENTER: u64 = 4;
const GESTURE_LAUNCH_SHELL: u64 = 5;
const GESTURE_LAUNCH_SETTINGS: u64 = 6;
const GESTURE_LAUNCH_FILES: u64 = 7;

const SPAWN_APP_SHELL: u64 = 1;
const SPAWN_APP_SETTINGS: u64 = 2;
const SPAWN_APP_FILES: u64 = 3;

const LAUNCH_QUEUE_CAPACITY: usize = 8;
const APP_HISTORY_CAPACITY: usize = 3;

struct LaunchQueue {
    items: [u64; LAUNCH_QUEUE_CAPACITY],
    head: usize,
    len: usize,
}

impl LaunchQueue {
    const fn new() -> Self {
        Self {
            items: [0; LAUNCH_QUEUE_CAPACITY],
            head: 0,
            len: 0,
        }
    }

    fn push(&mut self, spawn_arg: u64) -> bool {
        if self.len >= LAUNCH_QUEUE_CAPACITY {
            return false;
        }
        let tail = (self.head + self.len) % LAUNCH_QUEUE_CAPACITY;
        self.items[tail] = spawn_arg;
        self.len += 1;
        true
    }

    fn pop(&mut self) -> Option<u64> {
        if self.len == 0 {
            return None;
        }
        let value = self.items[self.head];
        self.head = (self.head + 1) % LAUNCH_QUEUE_CAPACITY;
        self.len -= 1;
        Some(value)
    }
}

struct AppHistory {
    recent: [u64; APP_HISTORY_CAPACITY],
    count: usize,
    cursor: usize,
    last_launched: u64,
    last_non_shell: u64,
}

impl AppHistory {
    fn new() -> Self {
        Self {
            recent: [SPAWN_APP_SHELL, SPAWN_APP_SETTINGS, SPAWN_APP_FILES],
            count: APP_HISTORY_CAPACITY,
            cursor: 0,
            last_launched: SPAWN_APP_SHELL,
            last_non_shell: SPAWN_APP_SETTINGS,
        }
    }

    fn on_launch(&mut self, spawn_arg: u64) {
        self.last_launched = spawn_arg;
        if spawn_arg != SPAWN_APP_SHELL {
            self.last_non_shell = spawn_arg;
        }

        let mut idx = 0usize;
        while idx < self.count {
            if self.recent[idx] == spawn_arg {
                break;
            }
            idx += 1;
        }

        if idx < self.count {
            let value = self.recent[idx];
            while idx > 0 {
                self.recent[idx] = self.recent[idx - 1];
                idx -= 1;
            }
            self.recent[0] = value;
        } else {
            let limit = if self.count < APP_HISTORY_CAPACITY {
                let new_count = self.count + 1;
                self.count = new_count;
                new_count
            } else {
                APP_HISTORY_CAPACITY
            };
            let mut i = limit - 1;
            while i > 0 {
                self.recent[i] = self.recent[i - 1];
                i -= 1;
            }
            self.recent[0] = spawn_arg;
        }
        self.cursor = 0;
    }

    fn home_target(&self) -> u64 {
        if self.last_launched == SPAWN_APP_SHELL {
            self.last_non_shell
        } else {
            SPAWN_APP_SHELL
        }
    }

    fn quick_switch_left_target(&mut self) -> u64 {
        if self.count == 0 {
            return SPAWN_APP_SHELL;
        }
        self.cursor = (self.cursor + 1) % self.count;
        self.recent[self.cursor]
    }

    fn quick_switch_right_target(&mut self) -> u64 {
        if self.count == 0 {
            return SPAWN_APP_SHELL;
        }
        if self.cursor == 0 {
            self.cursor = self.count - 1;
        } else {
            self.cursor -= 1;
        }
        self.recent[self.cursor]
    }

    fn last_launched(&self) -> u64 {
        self.last_launched
    }
}

#[no_mangle]
pub extern "sysv64" fn _start(task_id: u64) -> ! {
    let _ = fs_write(1, b"[openos-pid] hello from ring3 fswrite\r\n");
    let _ = fs_write(1, b"[openos-pid] second userspace syscall\r\n");

    if task_id != 1 {
        let _ = fs_write(1, b"[openos-pid-child] exiting\r\n");
        let _ = proc_exit(7);
        halt_forever();
    }

    let _ = fs_write(1, b"[openos-pid1] boot checks start\r\n");
    boot_smoke_checks();
    let _ = fs_write(1, b"[openos-pid1] launcher start\r\n");
    run_launcher_loop();
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    halt_forever()
}

fn boot_smoke_checks() {
    let scene_result = gfx_submit_scene(0xEEF4FF, 0xCEDFFF, 0xF7FBFF, 112);
    let present_result = gfx_present();
    if scene_result.code == 0 && present_result.code == 0 {
        let _ = fs_write(1, b"[openos-pid1] scene presented\r\n");
    } else {
        let _ = fs_write(1, b"[openos-pid1] scene present error\r\n");
    }

    let vm_result = vm_map(0, 4096, 1);
    if vm_result.code == 0 {
        let mapped_ptr = vm_result.value as *mut u8;
        let vm_msg = b"[openos-pid1] vm map ok\r\n";
        unsafe {
            core::ptr::copy_nonoverlapping(vm_msg.as_ptr(), mapped_ptr, vm_msg.len());
        }
        let vm_slice = unsafe { core::slice::from_raw_parts(mapped_ptr as *const u8, vm_msg.len()) };
        let _ = fs_write(1, vm_slice);
        let unmap_result = vm_unmap(vm_result.value, 4096);
        if unmap_result.code == 0 {
            let _ = fs_write(1, b"[openos-pid1] vm unmap ok\r\n");
        } else {
            let _ = fs_write(1, b"[openos-pid1] vm unmap error\r\n");
        }
    } else {
        let _ = fs_write(1, b"[openos-pid1] vm map error\r\n");
    }

    log_file_prefix(b"/etc/openos-release", b"[openos-pid1] fs read: ");
    log_file_prefix(b"/etc", b"[openos-pid1] fs dir: ");
    log_file_prefix(b"/proc/self/status", b"[openos-pid1] fs self: ");
    log_file_prefix(b"/proc/tasks", b"[openos-pid1] fs tasks: ");
    log_file_prefix(b"/proc/apps", b"[openos-pid1] fs apps: ");
    log_file_prefix(b"/proc/launcher-history", b"[openos-pid1] fs launch-hist: ");

    let sock_res = net_socket(2, 1, 0);
    if sock_res.code == 0 {
        let sock = sock_res.value;
        let connect_res = net_connect(sock, b"loopback");
        if connect_res.code == 0 {
            let send_res = net_send(sock, b"ping");
            if send_res.code == 0 {
                let mut net_buf = [0u8; 8];
                let recv_res = net_recv(sock, &mut net_buf);
                if recv_res.code == 0 && recv_res.value as usize == 4 && &net_buf[..4] == b"ping" {
                    let _ = fs_write(1, b"[openos-pid1] net loopback ok\r\n");
                } else {
                    let _ = fs_write(1, b"[openos-pid1] net recv error\r\n");
                }
            } else {
                let _ = fs_write(1, b"[openos-pid1] net send error\r\n");
            }
        } else {
            let _ = fs_write(1, b"[openos-pid1] net connect error\r\n");
        }
    } else {
        let _ = fs_write(1, b"[openos-pid1] net socket error\r\n");
    }

    let nic_sock_res = net_socket(2, 1, 0);
    if nic_sock_res.code == 0 {
        let nic_sock = nic_sock_res.value;
        let nic_connect = net_connect(nic_sock, b"nic0");
        if nic_connect.code == 0 {
            let nic_send = net_send(nic_sock, b"nic-ping");
            if nic_send.code == 0 {
                let _ = fs_write(1, b"[openos-pid1] net nic send ok\r\n");
            } else {
                let _ = fs_write(1, b"[openos-pid1] net nic send error\r\n");
            }
        } else {
            let _ = fs_write(1, b"[openos-pid1] net nic connect error\r\n");
        }
    }

    let launch_payload = b"openos-shell";
    let launch_header = UiMessageHeader {
        channel: UiChannel::ShellLifecycle,
        kind: UiMessageKind::LaunchApp,
        payload_len: launch_payload.len() as u16,
    };
    let send_result = ipc_send(&launch_header, launch_payload);
    if send_result.code == 0 {
        let _ = fs_write(1, b"[openos-pid1] ipc send queued\r\n");
    } else {
        let _ = fs_write(1, b"[openos-pid1] ipc send error\r\n");
    }

    let mut recv_header = UiMessageHeader {
        channel: UiChannel::ShellLifecycle,
        kind: UiMessageKind::LaunchApp,
        payload_len: 0,
    };
    let mut recv_payload = [0u8; 64];
    let recv_result = ipc_recv(&mut recv_header, &mut recv_payload);
    if recv_result.code == 0
        && recv_result.value as usize == launch_payload.len()
        && recv_header.channel == UiChannel::ShellLifecycle
        && recv_header.kind == UiMessageKind::LaunchApp
        && &recv_payload[..launch_payload.len()] == launch_payload
    {
        let _ = fs_write(1, b"[openos-pid1] ipc loopback ok\r\n");
    } else {
        let _ = fs_write(1, b"[openos-pid1] ipc recv mismatch\r\n");
    }

    let _ = input_subscribe(true);
    let input_result = input_read();
    if input_result.code == -11 {
        let _ = fs_write(1, b"[openos-pid1] input queue empty\r\n");
    }
}

fn run_launcher_loop() -> ! {
    let _ = input_subscribe(true);
    present_home_scene();
    run_transition_self_test();

    let mut queue = LaunchQueue::new();
    let _ = queue.push(SPAWN_APP_SHELL);
    let _ = queue.push(SPAWN_APP_SETTINGS);
    let _ = queue.push(SPAWN_APP_FILES);
    let mut history = AppHistory::new();

    let mut active_children = 0usize;

    loop {
        collect_gesture_launches(&mut queue, &mut history);

        if active_children == 0 {
            if let Some(spawn_arg) = queue.pop() {
                present_launch_transition(history.last_launched(), spawn_arg);
                if launch_app(spawn_arg) {
                    active_children = 1;
                    history.on_launch(spawn_arg);
                    log_file_prefix(b"/proc/apps", b"[openos-pid1] apps: ");
                    log_file_prefix(b"/proc/launcher-history", b"[openos-pid1] launch-hist: ");
                }
            }
        }

        let mut status = 0i64;
        loop {
            let wait_result = proc_wait(Some(&mut status));
            if wait_result.code == 0 {
                active_children = active_children.saturating_sub(1);
                let _ = fs_write(1, b"[openos-pid1] child reaped\r\n");
                log_file_prefix(b"/proc/apps", b"[openos-pid1] apps: ");
                log_file_prefix(b"/proc/launcher-history", b"[openos-pid1] launch-hist: ");
                present_home_scene();
                continue;
            }
            if wait_result.code != -11 {
                let _ = fs_write(1, b"[openos-pid1] wait error\r\n");
            }
            break;
        }

        idle_pause();
    }
}

fn collect_gesture_launches(queue: &mut LaunchQueue, history: &mut AppHistory) {
    let mut reads = 0usize;
    while reads < 4 {
        let input_result = input_read();
        if input_result.code != 0 {
            break;
        }
        reads += 1;

        match input_result.value {
            GESTURE_HOME => {
                present_home_scene();
                let target = history.home_target();
                log_gesture_target(b"[openos-pid1] home target=", target);
                enqueue_spawn(queue, target);
            }
            GESTURE_APP_SWITCHER_LEFT => {
                present_switcher_left_transition();
                let target = history.quick_switch_left_target();
                log_gesture_target(b"[openos-pid1] switch-left target=", target);
                enqueue_spawn(queue, target);
            }
            GESTURE_APP_SWITCHER_RIGHT => {
                present_switcher_right_transition();
                let target = history.quick_switch_right_target();
                log_gesture_target(b"[openos-pid1] switch-right target=", target);
                enqueue_spawn(queue, target);
            }
            GESTURE_CONTROL_CENTER => {
                present_control_center_transition();
                log_gesture_target(b"[openos-pid1] control target=", SPAWN_APP_SETTINGS);
                enqueue_spawn(queue, SPAWN_APP_SETTINGS);
            }
            GESTURE_NOTIFICATION_CENTER => {
                present_notification_transition();
                log_gesture_target(b"[openos-pid1] notifications target=", SPAWN_APP_FILES);
                enqueue_spawn(queue, SPAWN_APP_FILES);
            }
            GESTURE_LAUNCH_SHELL => {
                present_home_scene();
                log_gesture_target(b"[openos-pid1] launch target=", SPAWN_APP_SHELL);
                enqueue_spawn(queue, SPAWN_APP_SHELL);
            }
            GESTURE_LAUNCH_SETTINGS => {
                present_home_scene();
                log_gesture_target(b"[openos-pid1] launch target=", SPAWN_APP_SETTINGS);
                enqueue_spawn(queue, SPAWN_APP_SETTINGS);
            }
            GESTURE_LAUNCH_FILES => {
                present_home_scene();
                log_gesture_target(b"[openos-pid1] launch target=", SPAWN_APP_FILES);
                enqueue_spawn(queue, SPAWN_APP_FILES);
            }
            _ => {}
        }
    }
}

fn enqueue_spawn(queue: &mut LaunchQueue, spawn_arg: u64) {
    if !queue.push(spawn_arg) {
        let _ = fs_write(1, b"[openos-pid1] launch queue full\r\n");
    }
}

fn launch_app(spawn_arg: u64) -> bool {
    let app_payload = app_payload(spawn_arg);
    let launch_header = UiMessageHeader {
        channel: UiChannel::AppLaunch,
        kind: UiMessageKind::LaunchApp,
        payload_len: app_payload.len() as u16,
    };
    if ipc_send(&launch_header, app_payload).code == 0 {
        drain_one_ipc_message();
    }

    let spawn_result = proc_spawn(spawn_arg);
    if spawn_result.code == 0 {
        let _ = fs_write(1, b"[openos-pid1] app launched\r\n");
        true
    } else {
        let _ = fs_write(1, b"[openos-pid1] app launch error\r\n");
        false
    }
}

fn drain_one_ipc_message() {
    let mut recv_header = UiMessageHeader {
        channel: UiChannel::AppLaunch,
        kind: UiMessageKind::LaunchApp,
        payload_len: 0,
    };
    let mut recv_payload = [0u8; 32];
    let _ = ipc_recv(&mut recv_header, &mut recv_payload);
}

fn app_payload(spawn_arg: u64) -> &'static [u8] {
    match spawn_arg {
        SPAWN_APP_SHELL => b"openos-shell",
        SPAWN_APP_SETTINGS => b"openos-settings",
        SPAWN_APP_FILES => b"openos-files",
        _ => b"openos-unknown",
    }
}

fn app_name(spawn_arg: u64) -> &'static [u8] {
    match spawn_arg {
        SPAWN_APP_SHELL => b"shell",
        SPAWN_APP_SETTINGS => b"settings",
        SPAWN_APP_FILES => b"files",
        _ => b"unknown",
    }
}

fn log_gesture_target(prefix: &[u8], spawn_arg: u64) {
    let _ = fs_write(1, prefix);
    let _ = fs_write(1, app_name(spawn_arg));
    let _ = fs_write(1, b"\r\n");
}

fn run_transition_self_test() {
    let _ = fs_write(1, b"[openos-pid1] transition self-test\r\n");
    present_switcher_left_transition();
    present_switcher_right_transition();
    present_control_center_transition();
    present_notification_transition();
    present_home_scene();
}

fn present_home_scene() {
    present_scene(0xEEF4FF, 0xCEDFFF, 0xF7FBFF, 112);
}

fn present_switcher_left_transition() {
    let _ = fs_write(1, b"[openos-pid1] transition switch-left\r\n");
    present_scene(0xDCE7FF, 0xC0D3F6, 0xE9F1FF, 106);
    present_scene(0xCFDFFF, 0xB5C8EE, 0xE1ECFF, 102);
}

fn present_switcher_right_transition() {
    let _ = fs_write(1, b"[openos-pid1] transition switch-right\r\n");
    present_scene(0xDFECFF, 0xC3D6F4, 0xEAF3FF, 106);
    present_scene(0xD2E2FF, 0xB8CCEE, 0xE2ECFF, 102);
}

fn present_control_center_transition() {
    let _ = fs_write(1, b"[openos-pid1] transition control-center\r\n");
    present_scene(0xDCEBFF, 0xAFC7F2, 0xE6F0FF, 120);
}

fn present_notification_transition() {
    let _ = fs_write(1, b"[openos-pid1] transition notifications\r\n");
    present_scene(0xF6F0FF, 0xDCCFFF, 0xF7F4FF, 116);
}

fn present_launch_transition(from_spawn_arg: u64, to_spawn_arg: u64) {
    let _ = fs_write(1, b"[openos-pid1] transition launch\r\n");
    if from_spawn_arg == to_spawn_arg {
        let (top, bottom, dock, dock_height) = app_palette(to_spawn_arg);
        present_scene(top, bottom, dock, dock_height);
        return;
    }

    let (from_top, from_bottom, from_dock, from_height) = app_palette(from_spawn_arg);
    let (to_top, to_bottom, to_dock, to_height) = app_palette(to_spawn_arg);
    present_scene(
        blend_color(from_top, to_top),
        blend_color(from_bottom, to_bottom),
        blend_color(from_dock, to_dock),
        (from_height + to_height) / 2,
    );
    present_scene(to_top, to_bottom, to_dock, to_height);
}

fn app_palette(spawn_arg: u64) -> (u32, u32, u32, u32) {
    match spawn_arg {
        SPAWN_APP_SHELL => (0xDCEBFF, 0xB7CCF2, 0xE8F2FF, 108),
        SPAWN_APP_SETTINGS => (0xE5FFF4, 0xC7F2E1, 0xEEFFF7, 110),
        SPAWN_APP_FILES => (0xFFF5E6, 0xF2DFC5, 0xFFF8EE, 110),
        _ => (0xEEF4FF, 0xCEDFFF, 0xF7FBFF, 112),
    }
}

fn blend_color(a: u32, b: u32) -> u32 {
    let ar = (a >> 16) & 0xFF;
    let ag = (a >> 8) & 0xFF;
    let ab = a & 0xFF;

    let br = (b >> 16) & 0xFF;
    let bg = (b >> 8) & 0xFF;
    let bb = b & 0xFF;

    let rr = ((ar + br) / 2) & 0xFF;
    let rg = ((ag + bg) / 2) & 0xFF;
    let rb = ((ab + bb) / 2) & 0xFF;
    (rr << 16) | (rg << 8) | rb
}

fn present_scene(top_color: u32, bottom_color: u32, dock_color: u32, dock_height: u32) {
    let submit = gfx_submit_scene(top_color, bottom_color, dock_color, dock_height);
    if submit.code == 0 {
        let _ = gfx_present();
    }
}

fn log_file_prefix(path: &[u8], prefix: &[u8]) {
    let open_result = fs_open(path, 0);
    if open_result.code != 0 {
        return;
    }

    let fd = open_result.value;
    let mut data = [0u8; 192];
    let read_result = fs_read(fd, &mut data);
    if read_result.code == 0 && read_result.value != 0 {
        let _ = fs_write(1, prefix);
        let _ = fs_write(1, &data[..read_result.value as usize]);
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
