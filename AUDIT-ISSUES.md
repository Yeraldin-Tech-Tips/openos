# OpenOS Code Audit - Issues Found

> Update (post-merge verification): Issues 1, 8, 9, and 11 are now resolved on `work` via merged PRs #4, #5, #7, and #8. Issue 7 is functionally mitigated by scheduler fallback reaping when queue delivery cannot occur; remaining audit items below are still open unless explicitly marked.

Each section below is a discrete issue ready to be filed as a GitHub issue.
Severity scale: Critical > High > Medium > Low.

---

## Issue 1: Memory allocator free-list has TOCTOU race between atomic load and store

**Severity:** Critical
**Component:** kernel/memory-management
**Files:** `kernel/src/mm/mod.rs:648-688`

### Description

The page table and frame recycling functions use separate `load()` and `store()` operations on atomic counters instead of `compare_exchange` (CAS). If a timer interrupt fires between the `load` and `store`, and triggers a context switch to another task that also pops from the same free-list, both tasks will pop the **same index**, leading to two address spaces sharing the same physical page table or frame.

**Affected functions:**
- `pop_recycled_page_table_idx()` (line 648): loads `FREE_PAGE_TABLE_COUNT`, then stores decremented value
- `pop_recycled_frame_idx()` (line 659): same pattern with `FREE_USER_FRAME_COUNT`
- `recycle_page_table_idx()` (line 670): loads count, writes to stack, stores incremented count
- `recycle_frame_idx()` (line 680): same pattern

### Impact

Two tasks sharing the same physical page leads to memory corruption and potential cross-task data leaks or privilege escalation.

### Suggested Fix

Replace `load()`/`store()` pairs with `compare_exchange` loops, or disable interrupts around these critical sections.

---

## Issue 2: `get_or_create_next_table` does not check PTE_HUGE flag, corrupts memory for user page mappings

**Severity:** Critical
**Component:** kernel/memory-management
**Files:** `kernel/src/mm/mod.rs:521-544`, `kernel/src/mm/mod.rs:429-451`

### Description

`get_or_create_next_table()` (line 521) only checks `PTE_PRESENT` when deciding whether an entry already points to a next-level table. It does **not** check `PTE_HUGE`, which indicates a 2MB huge page mapping.

The kernel identity map (`init_low_identity_kernel_map()`, line 429) creates 2MB huge pages covering 0-4GB at the Page Directory level with `PTE_PRESENT | PTE_WRITABLE | PTE_HUGE`.

When `map_user_page()` is called for a user address below 4GB (e.g., the default userspace base at `0x400000`), the walk reaches a PD entry that is a huge page. `get_or_create_next_table()` sees `PTE_PRESENT`, misinterprets the huge page's physical address as a page table pointer, and writes page table entries to arbitrary physical memory.

Note: `find_leaf_pte_mut()` at lines 507 and 513 correctly checks `PTE_HUGE`, but `get_or_create_next_table()` does not.

### Impact

Every process launch with an image base below 4GB (the default for all userspace images per linker scripts) silently corrupts physical memory. This is the default code path.

### Suggested Fix

Add a `PTE_HUGE` check in `get_or_create_next_table()`. If `PTE_PRESENT | PTE_HUGE` is set, either return an error or split the huge page into 4K entries. Alternatively, restructure the identity map to not overlap with user address ranges.

---

## Issue 3: PID allocator can reassign PID 1 after `NEXT_PID` wraps around

**Severity:** High
**Component:** kernel/scheduler
**Files:** `kernel/src/sched/mod.rs:640-647`

### Description

`allocate_pid()` uses `NEXT_PID.fetch_add(1)` on an `AtomicUsize` (64-bit), then truncates to `u32` via `as u32` for `TaskId`. It only skips PID 0.

When `NEXT_PID` reaches `0x1_0000_0000`, the `as u32` truncation produces `0` (skipped by the loop). The next value `0x1_0000_0001` truncates to `1` -- the PID reserved for PID1 (init). A newly spawned task could then receive PID 1, breaking parent-child relationships and the lifecycle manager.

### Suggested Fix

Check for both `0` and `1` after truncation, or use `compare_exchange` to wrap `NEXT_PID` back to 2 properly.

---

## Issue 4: All kernel subsystems use unsynchronized `static mut` with no interrupt safety

**Severity:** High
**Component:** kernel/concurrency
**Files:** `kernel/src/fs/mod.rs`, `kernel/src/net/mod.rs`, `kernel/src/ipc/mod.rs`, `kernel/src/lifecycle/mod.rs`, `kernel/src/input/mod.rs`, `kernel/src/sched/mod.rs`

### Description

Every kernel subsystem stores global state in `static mut` variables (e.g., `OPEN_FILES`, `SOCKETS`, `IPC_QUEUE`, `APP_RECORDS`, `TASKS`) with no synchronization primitives. The scheduler's `on_timer_tick()` handler can preempt any syscall handler mid-operation via a timer interrupt. If the interrupted syscall and the interrupt handler (or a context-switched task) both access the same `static mut` data, state corruption occurs.

**Examples:**
- A syscall modifying `OPEN_FILES` is preempted; another task's syscall also modifies `OPEN_FILES`
- `lifecycle::on_spawn()` is called from a syscall, interrupted, and `on_exit()` runs during context switch cleanup for the same record
- `net::send()` copies into a socket's `recv_buf`, is preempted, and another task calls `recv()` on the same socket

### Impact

Data corruption in any kernel subsystem. May manifest as lost IPC messages, corrupted file descriptors, or incorrect task lifecycle state.

### Suggested Fix

Either:
1. Disable interrupts (CLI/STI) around critical sections accessing shared `static mut` state, or
2. Replace `static mut` with structures protected by spinlocks or interrupt-safe wrappers

---

## Issue 5: Single shared ELF staging buffer causes data corruption on concurrent module loads

**Severity:** High
**Component:** kernel/init
**Files:** `kernel/src/init/elf.rs:89`

### Description

`USERSPACE_IMAGE_STAGING` is a single `static mut` 16MB buffer used by `load_elf64_image()` to stage all ELF images before copying them into per-process page frames. If `spawn_from_boot_module()` is called concurrently (e.g., PID1 spawns an app while a timer interrupt context-switches and another task also spawns), the staging buffer is overwritten by the second load, corrupting the first task's image.

The loaded image's `entry_staging` pointer (returned in `LoadedInitImage`) points directly into this buffer, and `register_user_task()` stores this pointer. If a second ELF load happens before the first task's pages are fully populated, the data is overwritten.

### Impact

Process image corruption when multiple boot module loads overlap. May cause crashes or incorrect execution in spawned tasks.

### Suggested Fix

Either:
1. Add a mutex/lock around the staging buffer usage, or
2. Allocate per-load staging buffers from the frame pool, or
3. Copy the staging data into the page frames atomically before returning from `load_elf64_image()`

---

## Issue 6: `InputRead` syscall missing from `syscall_groups_do_not_overlap` test

**Severity:** Medium
**Component:** shared/abi
**Files:** `shared/abi/src/syscalls.rs:104-134`

### Description

The `syscall_groups_do_not_overlap` test declares an array of 18 discriminants but there are 19 syscall variants. `Syscall::InputRead` (0x0602) is missing from the `all_vals` array. This means the test does not verify that `InputRead`'s discriminant is unique, which could allow a future accidental collision to go undetected.

### Suggested Fix

Add `Syscall::InputRead as u16` to the `all_vals` array and update the array length to 19.

---

## Issue 7: Exit event queue overflow silently drops events, causing zombie processes

**Severity:** Medium
**Component:** kernel/scheduler
**Files:** `kernel/src/sched/mod.rs:740-760`

### Description

`enqueue_exit_event()` (line 740) has a fixed-size queue of 64 entries (`MAX_EXIT_EVENTS`). When the queue is full, it logs a message but **drops the event silently**. The parent process will never receive the child's exit notification, causing:

1. `proc_wait()` will always return `-11` (EAGAIN) for that child
2. The parent will never be able to reap the child
3. The child's task slot remains in `Exited` state permanently, wasting a slot from the 32-task limit
4. Under sustained spawning, this can lead to task table exhaustion

### Suggested Fix

Either:
1. Increase the queue or make it dynamic, or
2. Evict the oldest event when full, or
3. Block the exiting task until queue space is available

---

## Issue 8: `proc_wait` writes to userspace pointer without verifying page is mapped and writable

**Severity:** Medium
**Component:** kernel/syscall
**Files:** `kernel/src/syscall/mod.rs:150-170`

### Description

The `proc_wait` syscall handler validates the userspace pointer with `user_range_valid()` (line 161), which only checks that the address falls within the valid user virtual address range (`0x400000` to `0x800000000000`). It does **not** verify that the page is actually mapped in the current task's page tables or that it is writable.

A malicious or buggy userspace program could pass a valid-range but unmapped address, causing the kernel to write to a page fault-triggering address in kernel mode (line 165: `(status_out_ptr as *mut i64).write(exit_status)`). Since the kernel writes directly through the pointer without a page fault handler for kernel-mode writes to user pages, this could cause a kernel panic or, worse, write to an unintended physical page if the address happens to map to kernel memory through the identity map.

The same pattern exists in `ipc_recv` (line 495-497) and all syscall handlers that write to userspace buffers.

### Suggested Fix

Walk the page tables to verify the target page is present, user-accessible, and writable before writing. Alternatively, use a `copy_to_user()`-style helper that handles page faults gracefully.

---

## Issue 9: `spawn_from_current` reuses parent's stale `entry_staging` pointer

**Severity:** Medium
**Component:** kernel/scheduler
**Files:** `kernel/src/sched/mod.rs:302-338`

### Description

`spawn_from_current()` (line 302) reads the parent task's `entry_staging` pointer and passes it to `register_user_task()` for the child. This pointer points into the shared `USERSPACE_IMAGE_STAGING` buffer (see Issue 5). If any other ELF load has occurred since the parent was loaded, the staging buffer now contains a **different** image. The child will be loaded with the wrong binary.

This means `ProcSpawn` with `spawn_arg == 0` (fork-like clone) actually loads whatever image was most recently staged, not the parent's image.

### Suggested Fix

Store a copy of the image data per-task, or re-load the parent's module from the boot modules when spawning a child.

---

## Issue 10: Network socket FDs are not closed on task exit if socket was created but never used in a syscall during exit

**Severity:** Low
**Component:** kernel/networking
**Files:** `kernel/src/net/mod.rs:186-196`

### Description

`close_all_for_pid()` correctly iterates all sockets and closes those owned by the exiting PID. However, the socket table uses `static mut` without synchronization (see Issue 4). If a socket is being modified by a syscall on one task while `close_all_for_pid()` is called from the timer interrupt handler during task retirement, the socket state could be partially written -- e.g., the `in_use` flag might be read before it's set, or the owner PID might be stale.

### Impact

Leaked sockets (slots never freed) or use-after-close on sockets that were being actively used at the time of forced exit.

### Suggested Fix

Protect socket table access with interrupt-safe synchronization (see Issue 4).

---

## Issue 11: Framebuffer `fill_solid` can write beyond the framebuffer memory region

**Severity:** Medium
**Component:** kernel/ui
**Files:** `kernel/src/ui/framebuffer.rs:31-39`

### Description

`fill_solid()` calculates `pixels = fb.size / fb.bytes_per_pixel as usize` and writes `pixels` u32 values starting from `fb.base`. However, `fb.size` is the total byte size and `fb.bytes_per_pixel` is 4. If `fb.stride > fb.width` (which is common -- stride includes padding bytes), the actual framebuffer memory extends beyond `fb.height * fb.width * 4` but the usable pixel count is `fb.height * fb.stride`. The calculation `fb.size / 4` could write to padding areas, which is generally harmless but could write **past** the framebuffer if `fb.size` was reported incorrectly or if `fb.size` includes areas beyond the actual mapped memory.

More importantly, the gradient and strip functions use `row_base = y * stride` and write at `ptr.add(row_base + x)`. If `stride > width`, the writes at `row_base + width - 1` are correct, but `fill_solid` uses a flat index `0..pixels` which doesn't account for stride padding -- it fills memory linearly rather than respecting the stride layout. This produces visual artifacts on displays where `stride != width`.

### Suggested Fix

Use a row-by-row loop that respects stride, similar to the gradient functions: iterate `y in 0..height`, then `x in 0..width`, writing at `y * stride + x`.

---

## Issue 12: `FrameBufferConsole::write_line` has no line wrapping or screen bounds checking

**Severity:** Low
**Component:** kernel/ui
**Files:** `kernel/src/ui/framebuffer.rs:513-520`

### Description

`FrameBufferConsole::write_line()` increments `cursor_x` by 8 for each character and `cursor_y` by 12 for each newline, but never checks whether `cursor_x` exceeds the framebuffer width or `cursor_y` exceeds the height. Characters written past the right edge will be clipped by `put_pixel_fb`'s bounds check, but `cursor_x` will keep increasing, and characters that should wrap to the next line will be invisible. When `cursor_y` exceeds the height, all subsequent text is silently discarded.

### Suggested Fix

Add line wrapping when `cursor_x + 8 > fb.width` and scrolling or reset when `cursor_y + 12 > fb.height`.

---

## Issue 13: `BootModules.entries` pointer is `*const` but allocated storage is `*mut` -- const-correctness mismatch

**Severity:** Low
**Component:** shared/abi
**Files:** `shared/abi/src/boot.rs:53-57`, `boot/efi-loader/src/main.rs:142-144`

### Description

`BootModules` declares `entries: *const BootModule` (line 55 of boot.rs), but the EFI loader allocates it as `*mut BootModule` (line 188 of main.rs) and writes through it. The kernel then reads through the `*const` pointer. While this works in practice (the pointer value is the same), the `*const` type in the struct prevents the kernel from ever needing to modify the module list, which is correct semantics. However, the EFI loader casts `*mut` to assign to `*const` -- the `BootModules` struct should use `*mut BootModule` for consistency with the allocation, or the loader should use a separate mutable pointer for writing before assigning the final const pointer.

Similarly, `MemoryMap.entries` is `*const MemoryMapEntry` but allocated as `*mut`.

### Suggested Fix

This is a minor correctness/clarity issue. Consider using `*mut` in the struct to match the allocation pattern, or add a comment explaining the intentional immutability after construction.

---

## Issue 14: `bytes_per_pixel` is hardcoded to 4 in EFI loader without verifying pixel format

**Severity:** Medium
**Component:** boot/efi-loader
**Files:** `boot/efi-loader/src/main.rs:169`

### Description

`capture_framebuffer()` hardcodes `bytes_per_pixel: 4` (line 169) without checking the actual pixel format from `mode_info`. While most modern UEFI implementations use 32-bit BGRA/RGBA formats, the UEFI spec allows other formats. If the GOP mode uses a non-32bpp format (e.g., 24bpp RGB or 15/16bpp), the framebuffer code will write pixels at incorrect offsets, corrupting the display and potentially writing past the end of the framebuffer.

### Suggested Fix

Read `mode_info.pixel_format()` and either verify it's `PixelBlueGreenRedReserved8BitPerColor` or `PixelRedGreenBlueReserved8BitPerColor`, or reject unsupported formats and fall back to no-framebuffer mode.

---

## Issue 15: `MemoryMap.entries` in `BootInfo` uses raw pointer with no lifetime guarantee

**Severity:** Low
**Component:** shared/abi
**Files:** `shared/abi/src/boot.rs:16-21`

### Description

`MemoryMap` contains `entries: *const MemoryMapEntry` and `count: usize`. This raw pointer has no lifetime or ownership semantics. After the EFI loader exits boot services and constructs `BootInfo`, the pointer is valid because the memory was allocated as `LOADER_DATA`. However, if the kernel ever frees or reuses that memory region, the pointer becomes dangling. There is no safety mechanism or documentation marking this pointer as only valid during early boot.

### Suggested Fix

Add a doc comment to `MemoryMap.entries` specifying that this pointer is only valid during kernel initialization (before the physical memory allocator reclaims LOADER_DATA regions).
