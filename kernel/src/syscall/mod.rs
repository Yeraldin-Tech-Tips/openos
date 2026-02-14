use core::{
    arch::global_asm,
    cmp::min,
    mem::size_of,
    sync::atomic::{AtomicUsize, Ordering},
};

use abi::syscalls::{Syscall, SyscallResult};
use crate::fs;
use crate::ipc::{self, UiMessageHeaderRaw};
use crate::ui::compositor::{self, PresentError, SimpleScene};

const USER_VADDR_MIN: u64 = 0x0000_0000_0040_0000;
const USER_VADDR_MAX_EXCLUSIVE: u64 = 0x0000_8000_0000_0000;
const SYSCALL_IO_MAX: usize = 4096;
const VM_FLAG_WRITABLE: u64 = 1 << 0;
const VM_FLAG_EXECUTABLE: u64 = 1 << 1;

pub fn init() {
    INT80_TRAP_COUNT.store(0, Ordering::Release);

    crate::arch::x86_64::tables::install_user_interrupt(
        crate::arch::x86_64::tables::int80_vector(),
        openos_int80_entry as *const () as usize as u64,
    );

    crate::arch::x86_64::serial::write_hex_u64(
        "[openos-kernel] int80.vector=",
        crate::arch::x86_64::tables::int80_vector() as u64,
    );
}

#[repr(C)]
pub struct Int80Frame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,
}

static INT80_TRAP_COUNT: AtomicUsize = AtomicUsize::new(0);

#[no_mangle]
pub extern "C" fn openos_syscall_int80_dispatch(frame: &mut Int80Frame) {
    let trap_count = INT80_TRAP_COUNT.fetch_add(1, Ordering::AcqRel) + 1;
    if trap_count <= 2 {
        crate::arch::x86_64::serial::write_hex_u64(
            "[openos-kernel] int80.trap_count=",
            trap_count as u64,
        );
    }

    let result = dispatch(frame.rax as u16, frame.rdi, frame.rsi, frame.rdx, frame.r10);
    frame.rax = result.value;
    frame.rdx = result.code as u64;
}

pub fn dispatch(num: u16, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> SyscallResult {
    match num {
        x if x == Syscall::ProcSpawn as u16 => proc_spawn(_a0),
        x if x == Syscall::ProcExit as u16 => proc_exit(_a0),
        x if x == Syscall::ProcWait as u16 => proc_wait(_a0),
        x if x == Syscall::VmMap as u16 => vm_map(_a0, _a1, _a2),
        x if x == Syscall::VmUnmap as u16 => vm_unmap(_a0, _a1),
        x if x == Syscall::FsOpen as u16 => fs_open(_a0, _a1, _a2),
        x if x == Syscall::FsRead as u16 => fs_read(_a0, _a1, _a2),
        x if x == Syscall::FsWrite as u16 => fs_write(_a0, _a1, _a2),
        x if x == Syscall::FsClose as u16 => fs_close(_a0),
        x if x == Syscall::NetSocket as u16 => net_socket(_a0, _a1, _a2),
        x if x == Syscall::NetConnect as u16 => net_connect(_a0, _a1, _a2),
        x if x == Syscall::NetSend as u16 => net_send(_a0, _a1, _a2),
        x if x == Syscall::NetRecv as u16 => net_recv(_a0, _a1, _a2),
        x if x == Syscall::IpcSend as u16 => ipc_send(_a0, _a1, _a2),
        x if x == Syscall::IpcRecv as u16 => ipc_recv(_a0, _a1, _a2),
        x if x == Syscall::GfxSubmitScene as u16 => gfx_submit_scene(_a0, _a1, _a2, _a3),
        x if x == Syscall::GfxPresent as u16 => gfx_present(),
        x if x == Syscall::InputSubscribe as u16 => input_subscribe(_a0),
        x if x == Syscall::InputRead as u16 => input_read(),
        _ => SyscallResult::err(-38),
    }
}

fn proc_spawn(spawn_arg: u64) -> SyscallResult {
    if spawn_arg == 0 {
        return match crate::sched::spawn_from_current() {
            Ok(pid) => {
                crate::arch::x86_64::serial::write_hex_u64("[openos-kernel] proc_spawn.pid=", pid.0 as u64);
                SyscallResult::ok(pid.0 as u64)
            }
            Err(crate::sched::SpawnTaskError::MissingCurrentTask) => SyscallResult::err(-3),
            Err(crate::sched::SpawnTaskError::InvalidParent) => SyscallResult::err(-22),
            Err(crate::sched::SpawnTaskError::TableFull) => SyscallResult::err(-11),
            Err(crate::sched::SpawnTaskError::MemoryMapFailed) => SyscallResult::err(-12),
        };
    }

    let parent_pid = match crate::sched::current_task_id() {
        Some(pid) => pid,
        None => return SyscallResult::err(-3),
    };

    match crate::init::spawn_from_boot_module(spawn_arg, parent_pid) {
        Ok(pid) => {
            crate::lifecycle::on_spawn(parent_pid, pid, spawn_arg);
            crate::arch::x86_64::serial::write_hex_u64("[openos-kernel] proc_spawn.pid=", pid.0 as u64);
            SyscallResult::ok(pid.0 as u64)
        }
        Err(crate::init::SpawnModuleError::UnsupportedSpawnArg) => SyscallResult::err(-22),
        Err(crate::init::SpawnModuleError::MissingModule) => SyscallResult::err(-2),
        Err(crate::init::SpawnModuleError::InvalidModule | crate::init::SpawnModuleError::UnsupportedFormat) => {
            SyscallResult::err(-8)
        }
        Err(crate::init::SpawnModuleError::LoadFailed) => SyscallResult::err(-8),
        Err(crate::init::SpawnModuleError::SchedulerRejected) => SyscallResult::err(-11),
    }
}

fn proc_exit(status: u64) -> SyscallResult {
    match crate::sched::request_current_exit(status as i64) {
        Ok(pid) => {
            crate::arch::x86_64::serial::write_hex_u64("[openos-kernel] proc_exit.pid=", pid.0 as u64);
            SyscallResult::ok(0)
        }
        Err(crate::sched::ExitTaskError::MissingCurrentTask) => SyscallResult::err(-3),
        Err(crate::sched::ExitTaskError::InvalidCurrentTask) => SyscallResult::err(-22),
    }
}

fn proc_wait(status_out_ptr: u64) -> SyscallResult {
    let parent_pid = match crate::sched::current_task_id() {
        Some(pid) => pid,
        None => return SyscallResult::err(-3),
    };

    let Some((child_pid, exit_status)) = crate::sched::collect_child_exit(parent_pid) else {
        return SyscallResult::err(-11);
    };

    if status_out_ptr != 0 {
        if !user_range_valid(status_out_ptr, size_of::<i64>()) {
            return SyscallResult::err(-14);
        }
        unsafe {
            (status_out_ptr as *mut i64).write(exit_status);
        }
    }

    SyscallResult::ok(child_pid.0 as u64)
}

fn fs_write(fd: u64, buf_ptr: u64, len: u64) -> SyscallResult {
    if fd != 1 && fd != 2 {
        return SyscallResult::err(-9);
    }

    let len = len as usize;
    if len == 0 {
        return SyscallResult::ok(0);
    }
    if len > SYSCALL_IO_MAX {
        return SyscallResult::err(-22);
    }

    if !user_range_valid(buf_ptr, len) {
        return SyscallResult::err(-14);
    }

    let bytes = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, len) };
    crate::arch::x86_64::serial::write_bytes(bytes);
    SyscallResult::ok(len as u64)
}

fn vm_map(addr_hint: u64, len: u64, flags: u64) -> SyscallResult {
    let len = len as usize;
    if len == 0 {
        return SyscallResult::err(-22);
    }

    let asid = match crate::sched::current_task_address_space() {
        Some(asid) => asid,
        None => return SyscallResult::err(-3),
    };

    let map_base = if addr_hint == 0 {
        match crate::sched::reserve_current_vm_range(len) {
            Ok(base) => base,
            Err(crate::sched::VmRangeError::MissingCurrentTask | crate::sched::VmRangeError::InvalidCurrentTask) => {
                return SyscallResult::err(-3)
            }
            Err(crate::sched::VmRangeError::InvalidLength) => return SyscallResult::err(-22),
            Err(crate::sched::VmRangeError::RangeOverflow) => return SyscallResult::err(-12),
        }
    } else {
        addr_hint
    };

    let writable = (flags & VM_FLAG_WRITABLE) != 0;
    let executable = (flags & VM_FLAG_EXECUTABLE) != 0;
    match crate::mm::map_user_range(asid, map_base, len, writable, executable) {
        Ok(addr) => SyscallResult::ok(addr),
        Err(crate::mm::UserMapError::InvalidImage
        | crate::mm::UserMapError::InvalidEntry
        | crate::mm::UserMapError::InvalidRange
        | crate::mm::UserMapError::AddressOutOfRange) => SyscallResult::err(-22),
        Err(crate::mm::UserMapError::AlreadyMapped) => SyscallResult::err(-17),
        Err(crate::mm::UserMapError::InvalidAddressSpace | crate::mm::UserMapError::AddressSpaceTableFull) => {
            SyscallResult::err(-3)
        }
        Err(crate::mm::UserMapError::ResourceTrackingOverflow
        | crate::mm::UserMapError::PageTablePoolExhausted
        | crate::mm::UserMapError::UserFramePoolExhausted) => SyscallResult::err(-12),
    }
}

fn vm_unmap(addr: u64, len: u64) -> SyscallResult {
    let len = len as usize;
    if len == 0 {
        return SyscallResult::err(-22);
    }

    let asid = match crate::sched::current_task_address_space() {
        Some(asid) => asid,
        None => return SyscallResult::err(-3),
    };

    match crate::mm::unmap_user_range(asid, addr, len) {
        Ok(unmapped_pages) => SyscallResult::ok(unmapped_pages as u64),
        Err(crate::mm::UserMapError::InvalidImage
        | crate::mm::UserMapError::InvalidEntry
        | crate::mm::UserMapError::InvalidRange
        | crate::mm::UserMapError::AddressOutOfRange
        | crate::mm::UserMapError::AlreadyMapped) => SyscallResult::err(-22),
        Err(crate::mm::UserMapError::InvalidAddressSpace | crate::mm::UserMapError::AddressSpaceTableFull) => {
            SyscallResult::err(-3)
        }
        Err(crate::mm::UserMapError::ResourceTrackingOverflow
        | crate::mm::UserMapError::PageTablePoolExhausted
        | crate::mm::UserMapError::UserFramePoolExhausted) => SyscallResult::err(-12),
    }
}

fn fs_open(path_ptr: u64, path_len: u64, flags: u64) -> SyscallResult {
    let path_len = path_len as usize;
    if path_len == 0 || path_len > fs::MAX_PATH_BYTES {
        return SyscallResult::err(-22);
    }
    if !user_range_valid(path_ptr, path_len) {
        return SyscallResult::err(-14);
    }

    let pid = match crate::sched::current_task_id() {
        Some(pid) => pid,
        None => return SyscallResult::err(-3),
    };

    let path = unsafe { core::slice::from_raw_parts(path_ptr as *const u8, path_len) };
    match fs::open(pid, path, flags) {
        Ok(fd) => SyscallResult::ok(fd),
        Err(fs::FsError::InvalidPath) => SyscallResult::err(-22),
        Err(fs::FsError::NotFound) => SyscallResult::err(-2),
        Err(fs::FsError::TableFull) => SyscallResult::err(-24),
        Err(fs::FsError::BadFd) => SyscallResult::err(-9),
        Err(fs::FsError::AccessDenied) => SyscallResult::err(-13),
    }
}

fn fs_read(fd: u64, buf_ptr: u64, len: u64) -> SyscallResult {
    let len = len as usize;
    if len == 0 {
        return SyscallResult::ok(0);
    }
    if len > SYSCALL_IO_MAX {
        return SyscallResult::err(-22);
    }
    if !user_range_valid(buf_ptr, len) {
        return SyscallResult::err(-14);
    }

    let pid = match crate::sched::current_task_id() {
        Some(pid) => pid,
        None => return SyscallResult::err(-3),
    };

    let out = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, len) };
    match fs::read(pid, fd, out) {
        Ok(read) => SyscallResult::ok(read as u64),
        Err(fs::FsError::InvalidPath) => SyscallResult::err(-22),
        Err(fs::FsError::NotFound) => SyscallResult::err(-2),
        Err(fs::FsError::TableFull) => SyscallResult::err(-24),
        Err(fs::FsError::BadFd) => SyscallResult::err(-9),
        Err(fs::FsError::AccessDenied) => SyscallResult::err(-13),
    }
}

fn fs_close(fd: u64) -> SyscallResult {
    let pid = match crate::sched::current_task_id() {
        Some(pid) => pid,
        None => return SyscallResult::err(-3),
    };

    match fs::close(pid, fd) {
        Ok(()) => SyscallResult::ok(0),
        Err(fs::FsError::InvalidPath) => SyscallResult::err(-22),
        Err(fs::FsError::NotFound) => SyscallResult::err(-2),
        Err(fs::FsError::TableFull) => SyscallResult::err(-24),
        Err(fs::FsError::BadFd) => SyscallResult::err(-9),
        Err(fs::FsError::AccessDenied) => SyscallResult::err(-13),
    }
}

fn net_socket(domain: u64, kind: u64, protocol: u64) -> SyscallResult {
    let pid = match crate::sched::current_task_id() {
        Some(pid) => pid,
        None => return SyscallResult::err(-3),
    };

    match crate::net::socket(pid, domain, kind, protocol) {
        Ok(fd) => SyscallResult::ok(fd),
        Err(crate::net::NetError::InvalidArg) => SyscallResult::err(-22),
        Err(crate::net::NetError::BadFd) => SyscallResult::err(-9),
        Err(crate::net::NetError::NotConnected) => SyscallResult::err(-107),
        Err(crate::net::NetError::WouldBlock) => SyscallResult::err(-11),
        Err(crate::net::NetError::TableFull) => SyscallResult::err(-24),
        Err(crate::net::NetError::AddressUnsupported) => SyscallResult::err(-97),
    }
}

fn net_connect(fd: u64, addr_ptr: u64, addr_len: u64) -> SyscallResult {
    let addr_len = addr_len as usize;
    if addr_len == 0 || addr_len > 64 {
        return SyscallResult::err(-22);
    }
    if !user_range_valid(addr_ptr, addr_len) {
        return SyscallResult::err(-14);
    }

    let pid = match crate::sched::current_task_id() {
        Some(pid) => pid,
        None => return SyscallResult::err(-3),
    };

    let addr = unsafe { core::slice::from_raw_parts(addr_ptr as *const u8, addr_len) };
    match crate::net::connect(pid, fd, addr) {
        Ok(()) => SyscallResult::ok(0),
        Err(crate::net::NetError::InvalidArg) => SyscallResult::err(-22),
        Err(crate::net::NetError::BadFd) => SyscallResult::err(-9),
        Err(crate::net::NetError::NotConnected) => SyscallResult::err(-107),
        Err(crate::net::NetError::WouldBlock) => SyscallResult::err(-11),
        Err(crate::net::NetError::TableFull) => SyscallResult::err(-24),
        Err(crate::net::NetError::AddressUnsupported) => SyscallResult::err(-97),
    }
}

fn net_send(fd: u64, buf_ptr: u64, len: u64) -> SyscallResult {
    let len = len as usize;
    if len == 0 {
        return SyscallResult::ok(0);
    }
    if len > SYSCALL_IO_MAX {
        return SyscallResult::err(-22);
    }
    if !user_range_valid(buf_ptr, len) {
        return SyscallResult::err(-14);
    }

    let pid = match crate::sched::current_task_id() {
        Some(pid) => pid,
        None => return SyscallResult::err(-3),
    };

    let payload = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, len) };
    match crate::net::send(pid, fd, payload) {
        Ok(sent) => SyscallResult::ok(sent as u64),
        Err(crate::net::NetError::InvalidArg) => SyscallResult::err(-22),
        Err(crate::net::NetError::BadFd) => SyscallResult::err(-9),
        Err(crate::net::NetError::NotConnected) => SyscallResult::err(-107),
        Err(crate::net::NetError::WouldBlock) => SyscallResult::err(-11),
        Err(crate::net::NetError::TableFull) => SyscallResult::err(-24),
        Err(crate::net::NetError::AddressUnsupported) => SyscallResult::err(-97),
    }
}

fn net_recv(fd: u64, buf_ptr: u64, len: u64) -> SyscallResult {
    let len = len as usize;
    if len == 0 {
        return SyscallResult::ok(0);
    }
    if len > SYSCALL_IO_MAX {
        return SyscallResult::err(-22);
    }
    if !user_range_valid(buf_ptr, len) {
        return SyscallResult::err(-14);
    }

    let pid = match crate::sched::current_task_id() {
        Some(pid) => pid,
        None => return SyscallResult::err(-3),
    };

    let out = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, len) };
    match crate::net::recv(pid, fd, out) {
        Ok(read) => SyscallResult::ok(read as u64),
        Err(crate::net::NetError::InvalidArg) => SyscallResult::err(-22),
        Err(crate::net::NetError::BadFd) => SyscallResult::err(-9),
        Err(crate::net::NetError::NotConnected) => SyscallResult::err(-107),
        Err(crate::net::NetError::WouldBlock) => SyscallResult::err(-11),
        Err(crate::net::NetError::TableFull) => SyscallResult::err(-24),
        Err(crate::net::NetError::AddressUnsupported) => SyscallResult::err(-97),
    }
}

fn ipc_send(header_ptr: u64, payload_ptr: u64, payload_len: u64) -> SyscallResult {
    let payload_len = payload_len as usize;
    if payload_len > ipc::MAX_IPC_PAYLOAD {
        return SyscallResult::err(-22);
    }

    if !user_range_valid(header_ptr, size_of::<UiMessageHeaderRaw>()) {
        return SyscallResult::err(-14);
    }

    if payload_len != 0 && !user_range_valid(payload_ptr, payload_len) {
        return SyscallResult::err(-14);
    }

    let header = unsafe { (header_ptr as *const UiMessageHeaderRaw).read_unaligned() };
    let payload = if payload_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(payload_ptr as *const u8, payload_len) }
    };

    match ipc::send(header, payload) {
        Ok(()) => SyscallResult::ok(payload_len as u64),
        Err(ipc::IpcError::InvalidMessage) => SyscallResult::err(-22),
        Err(ipc::IpcError::QueueFull) => SyscallResult::err(-11),
        Err(ipc::IpcError::QueueEmpty | ipc::IpcError::PayloadTooLarge) => SyscallResult::err(-22),
    }
}

fn ipc_recv(header_out_ptr: u64, payload_out_ptr: u64, payload_capacity: u64) -> SyscallResult {
    if header_out_ptr == 0 || !user_range_valid(header_out_ptr, size_of::<UiMessageHeaderRaw>()) {
        return SyscallResult::err(-14);
    }

    let payload_capacity = payload_capacity as usize;
    if payload_capacity != 0 && !user_range_valid(payload_out_ptr, payload_capacity) {
        return SyscallResult::err(-14);
    }

    let mut payload = [0u8; ipc::MAX_IPC_PAYLOAD];
    let recv_capacity = min(payload_capacity, ipc::MAX_IPC_PAYLOAD);
    let (header, len) = match ipc::recv(&mut payload[..recv_capacity]) {
        Ok(value) => value,
        Err(ipc::IpcError::QueueEmpty) => return SyscallResult::err(-11),
        Err(ipc::IpcError::PayloadTooLarge) => return SyscallResult::err(-90),
        Err(ipc::IpcError::InvalidMessage | ipc::IpcError::QueueFull) => return SyscallResult::err(-22),
    };

    unsafe {
        (header_out_ptr as *mut UiMessageHeaderRaw).write_unaligned(header);
        if len != 0 {
            core::ptr::copy_nonoverlapping(payload.as_ptr(), payload_out_ptr as *mut u8, len);
        }
    }

    SyscallResult::ok(len as u64)
}

fn gfx_submit_scene(top_color: u64, bottom_color: u64, dock_color: u64, dock_height: u64) -> SyscallResult {
    let scene = SimpleScene {
        top_color: top_color as u32,
        bottom_color: bottom_color as u32,
        dock_color: dock_color as u32,
        dock_height: dock_height as u32,
    };
    compositor::submit_simple_scene(scene);
    SyscallResult::ok(0)
}

fn gfx_present() -> SyscallResult {
    match compositor::present_simple_scene() {
        Ok(()) => SyscallResult::ok(0),
        Err(PresentError::MissingScene) => SyscallResult::err(-61),
        Err(PresentError::FramebufferUnavailable) => SyscallResult::err(-19),
    }
}

fn input_subscribe(enable: u64) -> SyscallResult {
    crate::input::subscribe(enable != 0);
    SyscallResult::ok(0)
}

fn input_read() -> SyscallResult {
    let Some(action) = crate::input::read_action() else {
        return SyscallResult::err(-11);
    };
    SyscallResult::ok(action as u8 as u64)
}

fn user_range_valid(ptr: u64, len: usize) -> bool {
    if ptr < USER_VADDR_MIN {
        return false;
    }
    let end = match ptr.checked_add(len as u64) {
        Some(v) => v,
        None => return false,
    };
    end > ptr && end <= USER_VADDR_MAX_EXCLUSIVE
}

global_asm!(
    r#"
    .global openos_int80_entry
openos_int80_entry:
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rdi, rsp
    call openos_syscall_int80_dispatch

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    iretq
"#
);

unsafe extern "C" {
    fn openos_int80_entry();
}
