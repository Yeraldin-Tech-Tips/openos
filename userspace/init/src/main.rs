#![allow(unused)]

use abi::ipc::{UiChannel, UiMessageHeader, UiMessageKind};

fn main() {
    boot_sequence();
    launch_shell();
    launch_core_apps();
    idle_loop();
}

fn boot_sequence() {
    // TODO: Mount filesystems, initialize service registry, load policy.
}

fn launch_shell() {
    // TODO: Replace with proc_spawn syscall invocation.
    let _msg = UiMessageHeader {
        channel: UiChannel::ShellLifecycle,
        kind: UiMessageKind::LaunchApp,
        payload_len: 0,
    };
}

fn launch_core_apps() {
    // Settings and Files should prewarm for fast first launch.
}

fn idle_loop() {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
