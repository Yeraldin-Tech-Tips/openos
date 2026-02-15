use core::cmp::min;

use crate::sched::TaskId;

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

static mut SOCKETS: [Socket; MAX_SOCKETS] = [EMPTY_SOCKET; MAX_SOCKETS];

pub fn init() {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(SOCKETS).cast::<Socket>();
        let mut i = 0usize;
        while i < MAX_SOCKETS {
            ptr.add(i).write(EMPTY_SOCKET);
            i += 1;
        }
    }
}

pub fn socket(pid: TaskId, domain: u64, kind: u64, _protocol: u64) -> Result<u64, NetError> {
    if domain != 2 || (kind != 1 && kind != 2) {
        return Err(NetError::InvalidArg);
    }

    unsafe {
        let mut i = 0usize;
        while i < MAX_SOCKETS {
            if !SOCKETS[i].in_use {
                SOCKETS[i] = Socket {
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

    unsafe {
        let mut sock = SOCKETS[slot];
        if !sock.in_use || sock.owner_pid != pid {
            return Err(NetError::BadFd);
        }
        sock.connected = true;
        sock.mode = mode;
        SOCKETS[slot] = sock;
    }

    Ok(())
}

pub fn send(pid: TaskId, fd: u64, payload: &[u8]) -> Result<usize, NetError> {
    if payload.is_empty() {
        return Ok(0);
    }

    let slot = fd_to_slot(fd)?;
    unsafe {
        let mut sock = SOCKETS[slot];
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
                SOCKETS[slot] = sock;
                Ok(count)
            }
            SocketMode::Nic => {
                let sent =
                    crate::drivers::ethernet_transmit(payload).map_err(|_| NetError::WouldBlock)?;
                Ok(sent)
            }
            SocketMode::None => Err(NetError::NotConnected),
        }
    }
}

pub fn recv(pid: TaskId, fd: u64, out: &mut [u8]) -> Result<usize, NetError> {
    if out.is_empty() {
        return Ok(0);
    }

    let slot = fd_to_slot(fd)?;
    unsafe {
        let mut sock = SOCKETS[slot];
        if !sock.in_use || sock.owner_pid != pid {
            return Err(NetError::BadFd);
        }
        if !sock.connected {
            return Err(NetError::NotConnected);
        }
        if sock.mode == SocketMode::Nic {
            return Err(NetError::WouldBlock);
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
        SOCKETS[slot] = sock;
        Ok(count)
    }
}

pub fn close_all_for_pid(pid: TaskId) {
    unsafe {
        let mut i = 0usize;
        while i < MAX_SOCKETS {
            if SOCKETS[i].in_use && SOCKETS[i].owner_pid == pid {
                SOCKETS[i] = EMPTY_SOCKET;
            }
            i += 1;
        }
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
