# OpenOS Code Audit - Issues Found

> **Last verified against HEAD**
>
> - **Commit:** `7bf2090`
> - **Timestamp (UTC):** `2026-02-15 23:32:50Z`
> - **Verification focus files:** `shared/abi/src/syscalls.rs`, `kernel/src/sched/mod.rs`, `kernel/src/net/mod.rs`, `kernel/src/mm/mod.rs`, plus all other files referenced below.

Severity scale: Critical > High > Medium > Low.

---

## Resolved

### Issue 1: Memory allocator free-list had TOCTOU race between atomic load/store

**Status:** Resolved  
**How verified:** `mm` free-list operations now execute under `FreeListCriticalSection`, which disables interrupts, acquires a lock (`FREE_LIST_LOCK` CAS), and only then reads/writes free-list counts/stacks in `pop_*`/`recycle_*`. This closes the interrupt-window race described in the audit.

### Issue 2: `get_or_create_next_table` huge-page handling

**Status:** Resolved  
**How verified:** `get_or_create_next_table()` now explicitly checks `PTE_HUGE` on present entries and returns `UserMapError::EncounteredHugeMapping` instead of treating huge mappings as lower-level tables.

### Issue 3: PID allocator wraparound could reassign PID 1

**Status:** Resolved  
**How verified:** `allocate_pid_from_counter()` now only accepts values `>= 2` after truncation, and a unit test (`allocate_pid_skips_reserved_ids_after_u32_wrap`) verifies wraparound skips both 0 and 1.

### Issue 4: unsynchronized `static mut` across kernel subsystems

**Status:** Resolved (with current design constraints)  
**How verified:** FS/NET/IPC/Input/Lifecycle state is now behind `IrqSafeLock`, and scheduler mutations are serialized via `SCHED_STATE_LOCK` even though backing arrays remain `static mut`. The original “no interrupt safety” claim no longer matches current implementation.

### Issue 6: `InputRead` missing from syscall overlap test

**Status:** Resolved  
**How verified:** `syscall_groups_do_not_overlap` now uses a 19-entry array and includes `Syscall::InputRead`.

### Issue 7: exit-event queue overflow dropped child exits

**Status:** Resolved / mitigated  
**How verified:** Full queue path now evicts oldest event instead of silently dropping newest, and `collect_child_exit()` also falls back to `recover_child_exit()` scan of exited children to preserve eventual reapability.

### Issue 8: `proc_wait` userspace write lacked mapped+writable validation

**Status:** Resolved  
**How verified:** `proc_wait` now writes via `copy_to_user()`, which calls `validate_user_write()`, which in turn checks mapped/user/writable PTEs through `mm::validate_user_write_range()`.

### Issue 9: `spawn_from_current` reused stale parent staging pointer

**Status:** Resolved  
**How verified:** `spawn_from_current()` now reloads image metadata via `init::resolve_task_image(parent.image_source_id)` and uses that loaded image data for child registration, rather than directly reusing parent task staging pointer.

### Issue 10: socket close-on-exit race (net table synchronization)

**Status:** Resolved  
**How verified:** networking state (including `close_all_for_pid`) is guarded by `NET_STATE: IrqSafeLock<NetState>`, so teardown and socket syscalls serialize on the same lock.

### Issue 11: framebuffer `fill_solid` linear write path

**Status:** Resolved  
**How verified:** `fill_solid()` now delegates to `fill_rows_cols()`, which writes row/column using `stride` and `width` bounds instead of flattening by `size / bpp`.

### Issue 14: EFI loader hardcoded `bytes_per_pixel = 4` without format gate

**Status:** Resolved  
**How verified:** `capture_framebuffer()` now rejects unsupported GOP formats and only accepts `PixelFormat::Bgr` before publishing framebuffer info.

---

## Open

### Issue 5: single shared ELF staging buffer for module loads

**Severity:** High  
**Component:** kernel/init  
**Current anchors:** `kernel/src/init/mod.rs:36`, `kernel/src/init/mod.rs:207-218`

**Current state / impact update:**
`MODULE_IMAGE_STAGING` remains a single global mutable buffer used by `load_module_image()`. Concurrent/overlapping image load paths can still overwrite staged content before dependent consumers finish using it. The stale-image hazards described in the original audit remain relevant for shared-staging semantics.

---

### Issue 12: `FrameBufferConsole::write_line` lacks wrap/scroll bounds

**Severity:** Low  
**Component:** kernel/ui  
**Current anchors:** `kernel/src/ui/framebuffer.rs:486-493`

**Current state / impact update:**
`write_line()` still increments `cursor_x`/`cursor_y` unbounded and then resets only `cursor_x` per line. Rendering is clipped by pixel bounds checks, so memory safety is not the concern here; usability is (text disappears once cursor moves beyond visible region).

---

### Issue 13: `BootModules.entries` const/mut allocation mismatch

**Severity:** Low  
**Component:** shared/abi + boot/efi-loader  
**Current anchors:** `shared/abi/src/boot.rs:55`, `boot/efi-loader/src/main.rs:143`, `boot/efi-loader/src/main.rs:200-205`

**Current state / impact update:**
ABI structs still expose `*const` entry pointers while loader allocates mutable storage (`*mut`) and then passes it through as const in `BootInfo`. This remains mostly a const-correctness/documentation issue rather than a runtime bug.

---

### Issue 15: `MemoryMap.entries` raw pointer lifetime contract undocumented

**Severity:** Low  
**Component:** shared/abi  
**Current anchors:** `shared/abi/src/boot.rs:18-21`

**Current state / impact update:**
`MemoryMap.entries` still has no explicit lifetime/ownership documentation in ABI comments, so consumers must infer validity duration from boot flow conventions.
