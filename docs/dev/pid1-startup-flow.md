# PID1 Startup Flow

This document describes the host-side `openos-init` startup sequence (`userspace/init/src/main.rs`) and its dependency checks before handing over to long-running supervision.

## Startup order

1. `boot_sequence()` runs architecture-contract probes against the kernel VFS.
2. `launch_shell()` issues `ProcSpawn(1)` via `openos-syscall` wrappers to start `openos-shell`.
3. `launch_core_apps()` remains an intentional no-op in the host PID1 stub.
4. `idle_loop()` keeps PID1 resident as a supervisor process.

## Bootstrap probes

`boot_sequence()` currently validates these contract surfaces:

- `/proc/boot-state`: confirms core boot/fs bootstrap state is exposed.
- `/proc/apps`: confirms app lifecycle registry visibility.
- `/proc/launcher-history`: confirms launcher policy history visibility.
- `/etc/gesture-map`: confirms interaction policy file visibility.

Each probe opens/reads/closes the file and logs success/failure to both stdout and serial (`FsWrite` fd `1`).

## Shell launch behavior

- PID1 uses `openos_syscall::proc_spawn(1)` (`spawn_arg=1`) to request the shell module.
- On success, PID1 logs the spawned child PID.
- On failure, PID1 logs the syscall code and continues running (non-fatal) so diagnostics remain visible and future recovery paths can still be added.

## Why `launch_core_apps()` is a no-op here

Prewarming `settings` and `files` is intentionally left to `openos-init-payload` (ring3 UI launcher), where graphics/input and app transition policy are active. The host `openos-init` stub keeps only bootstrap + primary shell handoff responsibilities to avoid duplicate launch policy between host test flow and real payload flow.
