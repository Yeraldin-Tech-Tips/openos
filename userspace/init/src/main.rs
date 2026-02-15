const SPAWN_APP_SHELL: u64 = 1;
const SPAWN_APP_SETTINGS: u64 = 2;
const SPAWN_APP_FILES: u64 = 3;

const PROC_BOOT_STATE: &[u8] = b"/proc/boot-state";
const PROC_APPS: &[u8] = b"/proc/apps";
const PROC_LAUNCHER_HISTORY: &[u8] = b"/proc/launcher-history";
const ETC_GESTURE_MAP: &[u8] = b"/etc/gesture-map";

fn main() {
    boot_sequence();
    launch_shell();
    launch_core_apps();
    idle_loop();
}

fn boot_sequence() {
    log_info("[openos-init] boot: begin bootstrap");

    let boot_state = probe_file(PROC_BOOT_STATE);
    if boot_state.ok {
        log_info("[openos-init] boot: filesystem bootstrap confirmed via /proc/boot-state");
    } else {
        log_error_code(
            "[openos-init] boot: filesystem bootstrap probe failed for /proc/boot-state",
            boot_state.code,
        );
    }

    let registry = probe_file(PROC_APPS);
    if registry.ok {
        log_info("[openos-init] boot: service registry endpoint ready at /proc/apps");
    } else {
        log_error_code(
            "[openos-init] boot: service registry bootstrap failed for /proc/apps",
            registry.code,
        );
    }

    let launch_history = probe_file(PROC_LAUNCHER_HISTORY);
    if launch_history.ok {
        log_info(
            "[openos-init] boot: launcher policy state endpoint ready at /proc/launcher-history",
        );
    } else {
        log_error_code(
            "[openos-init] boot: launcher policy endpoint unavailable at /proc/launcher-history",
            launch_history.code,
        );
    }

    let gesture_map = probe_file(ETC_GESTURE_MAP);
    if gesture_map.ok {
        log_info("[openos-init] boot: interaction policy map loaded from /etc/gesture-map");
    } else {
        log_error_code(
            "[openos-init] boot: interaction policy bootstrap failed for /etc/gesture-map",
            gesture_map.code,
        );
    }
}

fn launch_shell() {
    log_info("[openos-init] launch: shell spawn requested");
    let spawn_result = openos_syscall::proc_spawn(SPAWN_APP_SHELL);
    if spawn_result.code == 0 {
        log_info_with_u64(
            "[openos-init] launch: shell spawned pid=",
            spawn_result.value,
        );
        return;
    }

    log_error_code(
        "[openos-init] launch: shell ProcSpawn failed (spawn_arg=1)",
        spawn_result.code,
    );
    log_info("[openos-init] launch: shell unavailable; continuing with PID1 idle supervisor loop");
}

fn launch_core_apps() {
    // Intentional no-op for PID1 stub: prewarming Settings/Files is handled by
    // the ring3 init payload launcher where graphics/input are active.
    log_info_with_u64(
        "[openos-init] launch: core prewarm deferred (settings spawn_arg)=",
        SPAWN_APP_SETTINGS,
    );
    log_info_with_u64(
        "[openos-init] launch: core prewarm deferred (files spawn_arg)=",
        SPAWN_APP_FILES,
    );
}

fn idle_loop() {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

fn log_info(message: &str) {
    println!("{message}");
    log_info_to_openos_serial(message);
}

#[cfg(feature = "openos-syscall-logging")]
fn log_info_to_openos_serial(message: &str) {
    let _ = openos_syscall::fs_write(1, message.as_bytes());
    let _ = openos_syscall::fs_write(1, b"\n");
}

#[cfg(not(feature = "openos-syscall-logging"))]
fn log_info_to_openos_serial(_message: &str) {}

fn log_info_with_u64(prefix: &str, value: u64) {
    let mut message = String::from(prefix);
    message.push_str(&value.to_string());
    log_info(&message);
}

fn log_error_code(prefix: &str, code: i64) {
    eprintln!("{prefix}: code={code}");
    let mut message = String::from(prefix);
    message.push_str(": code=");
    message.push_str(&code.to_string());
    log_info(&message);
}

#[derive(Clone, Copy)]
struct ProbeResult {
    ok: bool,
    code: i64,
}

fn probe_file(path: &[u8]) -> ProbeResult {
    let open_result = openos_syscall::fs_open(path, 0);
    if open_result.code != 0 {
        return ProbeResult {
            ok: false,
            code: open_result.code,
        };
    }

    let fd = open_result.value;
    let mut scratch = [0u8; 64];
    let read_result = openos_syscall::fs_read(fd, &mut scratch);
    let _ = openos_syscall::fs_close(fd);

    ProbeResult {
        ok: read_result.code == 0,
        code: read_result.code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_arguments_match_kernel_contract() {
        assert_eq!(SPAWN_APP_SHELL, 1);
        assert_eq!(SPAWN_APP_SETTINGS, 2);
        assert_eq!(SPAWN_APP_FILES, 3);
    }

    #[test]
    fn bootstrap_probe_paths_match_vfs_contract() {
        assert_eq!(PROC_BOOT_STATE, b"/proc/boot-state");
        assert_eq!(PROC_APPS, b"/proc/apps");
        assert_eq!(PROC_LAUNCHER_HISTORY, b"/proc/launcher-history");
        assert_eq!(ETC_GESTURE_MAP, b"/etc/gesture-map");
    }
}
