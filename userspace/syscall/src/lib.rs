#![no_std]

use core::arch::asm;

use abi::ipc::UiMessageHeader;
use abi::syscalls::{Syscall, SyscallResult};

#[inline(always)]
pub fn invoke(syscall: Syscall, a0: u64, a1: u64, a2: u64, a3: u64) -> SyscallResult {
    let value: u64;
    let code_raw: u64;

    unsafe {
        asm!(
            "int 0x80",
            inlateout("rax") syscall as u16 as u64 => value,
            in("rdi") a0,
            in("rsi") a1,
            inlateout("rdx") a2 => code_raw,
            in("r10") a3,
            out("rcx") _,
            out("r11") _,
            options(nostack)
        );
    }

    SyscallResult {
        code: code_raw as i64,
        value,
    }
}

#[inline(always)]
pub fn proc_spawn(spawn_arg: u64) -> SyscallResult {
    invoke(Syscall::ProcSpawn, spawn_arg, 0, 0, 0)
}

#[inline(always)]
pub fn proc_exit(status: u64) -> SyscallResult {
    invoke(Syscall::ProcExit, status, 0, 0, 0)
}

#[inline(always)]
pub fn proc_wait(status_out: Option<&mut i64>) -> SyscallResult {
    let status_ptr = match status_out {
        Some(slot) => slot as *mut i64 as u64,
        None => 0,
    };
    invoke(Syscall::ProcWait, status_ptr, 0, 0, 0)
}

#[inline(always)]
pub fn vm_map(addr_hint: u64, len: usize, flags: u64) -> SyscallResult {
    invoke(Syscall::VmMap, addr_hint, len as u64, flags, 0)
}

#[inline(always)]
pub fn vm_unmap(addr: u64, len: usize) -> SyscallResult {
    invoke(Syscall::VmUnmap, addr, len as u64, 0, 0)
}

#[inline(always)]
pub fn fs_open(path: &[u8], flags: u64) -> SyscallResult {
    invoke(
        Syscall::FsOpen,
        path.as_ptr() as u64,
        path.len() as u64,
        flags,
        0,
    )
}

#[inline(always)]
pub fn fs_read(fd: u64, out: &mut [u8]) -> SyscallResult {
    invoke(
        Syscall::FsRead,
        fd,
        out.as_mut_ptr() as u64,
        out.len() as u64,
        0,
    )
}

#[inline(always)]
pub fn fs_write(fd: u64, bytes: &[u8]) -> SyscallResult {
    invoke(
        Syscall::FsWrite,
        fd,
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        0,
    )
}

#[inline(always)]
pub fn fs_close(fd: u64) -> SyscallResult {
    invoke(Syscall::FsClose, fd, 0, 0, 0)
}

#[inline(always)]
pub fn net_socket(domain: u64, kind: u64, protocol: u64) -> SyscallResult {
    invoke(Syscall::NetSocket, domain, kind, protocol, 0)
}

#[inline(always)]
pub fn net_connect(fd: u64, addr: &[u8]) -> SyscallResult {
    invoke(
        Syscall::NetConnect,
        fd,
        addr.as_ptr() as u64,
        addr.len() as u64,
        0,
    )
}

#[inline(always)]
pub fn net_send(fd: u64, payload: &[u8]) -> SyscallResult {
    invoke(
        Syscall::NetSend,
        fd,
        payload.as_ptr() as u64,
        payload.len() as u64,
        0,
    )
}

#[inline(always)]
pub fn net_recv(fd: u64, out: &mut [u8]) -> SyscallResult {
    invoke(
        Syscall::NetRecv,
        fd,
        out.as_mut_ptr() as u64,
        out.len() as u64,
        0,
    )
}

#[inline(always)]
pub fn ipc_send(header: &UiMessageHeader, payload: &[u8]) -> SyscallResult {
    invoke(
        Syscall::IpcSend,
        header as *const UiMessageHeader as u64,
        payload.as_ptr() as u64,
        payload.len() as u64,
        0,
    )
}

#[inline(always)]
pub fn ipc_recv(header_out: &mut UiMessageHeader, payload_out: &mut [u8]) -> SyscallResult {
    invoke(
        Syscall::IpcRecv,
        header_out as *mut UiMessageHeader as u64,
        payload_out.as_mut_ptr() as u64,
        payload_out.len() as u64,
        0,
    )
}

#[inline(always)]
pub fn gfx_submit_scene(
    top_color: u32,
    bottom_color: u32,
    dock_color: u32,
    dock_height: u32,
) -> SyscallResult {
    invoke(
        Syscall::GfxSubmitScene,
        top_color as u64,
        bottom_color as u64,
        dock_color as u64,
        dock_height as u64,
    )
}

#[inline(always)]
pub fn gfx_present() -> SyscallResult {
    invoke(Syscall::GfxPresent, 0, 0, 0, 0)
}

#[inline(always)]
pub fn input_subscribe(enable: bool) -> SyscallResult {
    invoke(Syscall::InputSubscribe, if enable { 1 } else { 0 }, 0, 0, 0)
}

#[inline(always)]
pub fn input_read() -> SyscallResult {
    invoke(Syscall::InputRead, 0, 0, 0, 0)
}
