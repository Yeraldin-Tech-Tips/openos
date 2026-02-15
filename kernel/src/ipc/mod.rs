use abi::ipc::{UiChannel, UiMessageKind};

use crate::sync::IrqSafeLock;

pub const MAX_IPC_MESSAGES: usize = 64;
pub const MAX_IPC_PAYLOAD: usize = 256;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UiMessageHeaderRaw {
    pub channel: u16,
    pub kind: u16,
    pub payload_len: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpcError {
    InvalidMessage,
    QueueFull,
    QueueEmpty,
    PayloadTooLarge,
}

#[derive(Clone, Copy)]
pub struct IpcStats {
    pub queued_messages: usize,
}

#[derive(Clone, Copy)]
struct IpcMessage {
    in_use: bool,
    header: UiMessageHeaderRaw,
    payload: [u8; MAX_IPC_PAYLOAD],
}

const EMPTY_HEADER: UiMessageHeaderRaw = UiMessageHeaderRaw {
    channel: 0,
    kind: 0,
    payload_len: 0,
};

const EMPTY_MESSAGE: IpcMessage = IpcMessage {
    in_use: false,
    header: EMPTY_HEADER,
    payload: [0; MAX_IPC_PAYLOAD],
};

struct IpcState {
    queue: [IpcMessage; MAX_IPC_MESSAGES],
}

static IPC_STATE: IrqSafeLock<IpcState> = IrqSafeLock::new(IpcState {
    queue: [EMPTY_MESSAGE; MAX_IPC_MESSAGES],
});

pub fn init() {
    let mut state = IPC_STATE.lock();
    state.queue = [EMPTY_MESSAGE; MAX_IPC_MESSAGES];
}

pub fn send(header: UiMessageHeaderRaw, payload: &[u8]) -> Result<(), IpcError> {
    if !valid_header(header) || payload.len() > MAX_IPC_PAYLOAD {
        return Err(IpcError::InvalidMessage);
    }

    if payload.len() != header.payload_len as usize {
        return Err(IpcError::InvalidMessage);
    }

    let mut state = IPC_STATE.lock();
    let mut i = 0usize;
    while i < MAX_IPC_MESSAGES {
        if !state.queue[i].in_use {
            let mut slot = EMPTY_MESSAGE;
            slot.header = header;
            if !payload.is_empty() {
                slot.payload[..payload.len()].copy_from_slice(payload);
            }
            slot.in_use = true;
            state.queue[i] = slot;
            return Ok(());
        }
        i += 1;
    }

    Err(IpcError::QueueFull)
}

pub fn recv(out: &mut [u8]) -> Result<(UiMessageHeaderRaw, usize), IpcError> {
    let mut state = IPC_STATE.lock();
    let mut i = 0usize;
    while i < MAX_IPC_MESSAGES {
        let slot = state.queue[i];
        if slot.in_use {
            let payload_len = slot.header.payload_len as usize;
            if payload_len > out.len() {
                return Err(IpcError::PayloadTooLarge);
            }

            if payload_len != 0 {
                out[..payload_len].copy_from_slice(&slot.payload[..payload_len]);
            }

            state.queue[i] = EMPTY_MESSAGE;
            return Ok((slot.header, payload_len));
        }
        i += 1;
    }

    Err(IpcError::QueueEmpty)
}

pub fn stats() -> IpcStats {
    let state = IPC_STATE.lock();
    let mut queued = 0usize;
    let mut i = 0usize;
    while i < MAX_IPC_MESSAGES {
        if state.queue[i].in_use {
            queued += 1;
        }
        i += 1;
    }
    IpcStats {
        queued_messages: queued,
    }
}

fn valid_header(header: UiMessageHeaderRaw) -> bool {
    valid_channel(header.channel) && valid_kind(header.kind)
}

fn valid_channel(channel: u16) -> bool {
    channel == UiChannel::ShellLifecycle as u16
        || channel == UiChannel::NotificationCenter as u16
        || channel == UiChannel::ControlCenter as u16
        || channel == UiChannel::AppLaunch as u16
}

fn valid_kind(kind: u16) -> bool {
    kind == UiMessageKind::LaunchApp as u16
        || kind == UiMessageKind::CloseApp as u16
        || kind == UiMessageKind::PublishNotification as u16
        || kind == UiMessageKind::ToggleControl as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_header() -> UiMessageHeaderRaw {
        UiMessageHeaderRaw {
            channel: UiChannel::AppLaunch as u16,
            kind: UiMessageKind::LaunchApp as u16,
            payload_len: 1,
        }
    }

    #[test]
    fn ipc_queue_stress_preserves_count() {
        init();
        let header = test_header();
        let mut rounds = 0usize;
        while rounds < 256 {
            assert!(send(header, &[42]).is_ok());
            assert_eq!(stats().queued_messages, 1);
            let mut out = [0u8; MAX_IPC_PAYLOAD];
            let (_, len) = recv(&mut out).expect("message expected");
            assert_eq!(len, 1);
            assert_eq!(out[0], 42);
            assert_eq!(stats().queued_messages, 0);
            rounds += 1;
        }
    }
}
