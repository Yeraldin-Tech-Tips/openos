use core::cmp::min;

use crate::{sched::TaskId, sync::IrqSafeLock};

const MAX_SOCKETS: usize = 64;
const SOCKET_RECV_CAPACITY: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetError {
    InvalidArg,
    BadFd,
    NotConnected,
    WouldBlock,
    TableFull,
    AddressUnsupported,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SocketMode {
    None = 0,
    Loopback = 1,
    Nic = 2,
}

#[derive(Clone, Copy)]
struct Socket {
    in_use: bool,
    owner_pid: TaskId,
    connected: bool,
    mode: SocketMode,
    recv_buf: [u8; SOCKET_RECV_CAPACITY],
    recv_len: usize,
}

const EMPTY_SOCKET: Socket = Socket {
    in_use: false,
    owner_pid: TaskId(0),
    connected: false,
    mode: SocketMode::None,
    recv_buf: [0; SOCKET_RECV_CAPACITY],
    recv_len: 0,
};

struct NetState {
    sockets: [Socket; MAX_SOCKETS],
}

static NET_STATE: IrqSafeLock<NetState> = IrqSafeLock::new(NetState {
    sockets: [EMPTY_SOCKET; MAX_SOCKETS],
});

pub fn init() {
    // NET_STATE is already initialized with empty sockets; skip lock to avoid
    // IrqSafeLock hang during early boot (same pattern as input::init).
}

pub fn socket(pid: TaskId, domain: u64, kind: u64, _protocol: u64) -> Result<u64, NetError> {
    if domain != 2 || (kind != 1 && kind != 2) {
        return Err(NetError::InvalidArg);
    }

    let mut state = NET_STATE.lock();
    let mut i = 0usize;
    while i < MAX_SOCKETS {
        if !state.sockets[i].in_use {
            state.sockets[i] = Socket {
                in_use: true,
                owner_pid: pid,
                connected: false,
                mode: SocketMode::None,
                recv_buf: [0; SOCKET_RECV_CAPACITY],
                recv_len: 0,
            };
            return Ok((i as u64) + 1000);
        }
        i += 1;
    }

    Err(NetError::TableFull)
}

pub fn connect(pid: TaskId, fd: u64, addr: &[u8]) -> Result<(), NetError> {
    let slot = fd_to_slot(fd)?;
    let mode = if is_supported_loopback_addr(addr) {
        SocketMode::Loopback
    } else if is_supported_nic_addr(addr) {
        SocketMode::Nic
    } else {
        return Err(NetError::AddressUnsupported);
    };

    let mut state = NET_STATE.lock();
    let mut sock = state.sockets[slot];
    if !sock.in_use || sock.owner_pid != pid {
        return Err(NetError::BadFd);
    }
    sock.connected = true;
    sock.mode = mode;
    state.sockets[slot] = sock;

    Ok(())
}

pub fn send(pid: TaskId, fd: u64, payload: &[u8]) -> Result<usize, NetError> {
    if payload.is_empty() {
        return Ok(0);
    }

    let slot = fd_to_slot(fd)?;
    let mut state = NET_STATE.lock();
    let mut sock = state.sockets[slot];
    if !sock.in_use || sock.owner_pid != pid {
        return Err(NetError::BadFd);
    }
    if !sock.connected {
        return Err(NetError::NotConnected);
    }

    match sock.mode {
        SocketMode::Loopback => {
            let free = SOCKET_RECV_CAPACITY.saturating_sub(sock.recv_len);
            if free == 0 {
                return Err(NetError::WouldBlock);
            }
            let count = min(free, payload.len());
            let start = sock.recv_len;
            sock.recv_buf[start..start + count].copy_from_slice(&payload[..count]);
            sock.recv_len += count;
            state.sockets[slot] = sock;
            Ok(count)
        }
        SocketMode::Nic => {
            drop(state);
            let sent =
                crate::drivers::ethernet_transmit(payload).map_err(|_| NetError::WouldBlock)?;
            Ok(sent)
        }
        SocketMode::None => Err(NetError::NotConnected),
    }
}

pub fn recv(pid: TaskId, fd: u64, out: &mut [u8]) -> Result<usize, NetError> {
    if out.is_empty() {
        return Ok(0);
    }

    let slot = fd_to_slot(fd)?;
    let mut state = NET_STATE.lock();
    let mut sock = state.sockets[slot];
    if !sock.in_use || sock.owner_pid != pid {
        return Err(NetError::BadFd);
    }
    if !sock.connected {
        return Err(NetError::NotConnected);
    }
    if sock.mode == SocketMode::Nic {
        drop(state);
        return crate::drivers::ethernet_receive(out).map_err(|_| NetError::WouldBlock);
    }
    if sock.recv_len == 0 {
        return Err(NetError::WouldBlock);
    }

    let count = min(sock.recv_len, out.len());
    out[..count].copy_from_slice(&sock.recv_buf[..count]);
    if count < sock.recv_len {
        let remaining = sock.recv_len - count;
        sock.recv_buf.copy_within(count..sock.recv_len, 0);
        let mut i = remaining;
        while i < SOCKET_RECV_CAPACITY {
            sock.recv_buf[i] = 0;
            i += 1;
        }
        sock.recv_len = remaining;
    } else {
        sock.recv_buf = [0; SOCKET_RECV_CAPACITY];
        sock.recv_len = 0;
    }
    state.sockets[slot] = sock;
    Ok(count)
}

pub fn close_all_for_pid(pid: TaskId) {
    let mut state = NET_STATE.lock();
    let mut i = 0usize;
    while i < MAX_SOCKETS {
        if state.sockets[i].in_use && state.sockets[i].owner_pid == pid {
            state.sockets[i] = EMPTY_SOCKET;
        }
        i += 1;
    }
}

fn is_supported_loopback_addr(addr: &[u8]) -> bool {
    addr == b"loopback" || addr == b"127.0.0.1:7"
}

fn is_supported_nic_addr(addr: &[u8]) -> bool {
    addr == b"nic0" || addr == b"10.0.2.15:9"
}

fn fd_to_slot(fd: u64) -> Result<usize, NetError> {
    if fd < 1000 {
        return Err(NetError::BadFd);
    }
    let slot = (fd - 1000) as usize;
    if slot >= MAX_SOCKETS {
        return Err(NetError::BadFd);
    }
    Ok(slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_table_mutation_stress() {
        init();
        let pid = TaskId(9);
        for _ in 0..128 {
            let fd = socket(pid, 2, 1, 0).expect("socket");
            connect(pid, fd, b"loopback").expect("connect");
            assert_eq!(send(pid, fd, b"ping").expect("send"), 4);
            let mut out = [0u8; 8];
            assert_eq!(recv(pid, fd, &mut out).expect("recv"), 4);
            close_all_for_pid(pid);
        }
    }
}
