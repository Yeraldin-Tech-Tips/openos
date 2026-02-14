use abi::ipc::{UiChannel, UiMessageKind};

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

static mut IPC_QUEUE: [IpcMessage; MAX_IPC_MESSAGES] = [EMPTY_MESSAGE; MAX_IPC_MESSAGES];

pub fn init() {
    unsafe {
        let queue_ptr = core::ptr::addr_of_mut!(IPC_QUEUE).cast::<IpcMessage>();
        let mut i = 0usize;
        while i < MAX_IPC_MESSAGES {
            queue_ptr.add(i).write(EMPTY_MESSAGE);
            i += 1;
        }
    }
}

pub fn send(header: UiMessageHeaderRaw, payload: &[u8]) -> Result<(), IpcError> {
    if !valid_header(header) || payload.len() > MAX_IPC_PAYLOAD {
        return Err(IpcError::InvalidMessage);
    }

    if payload.len() != header.payload_len as usize {
        return Err(IpcError::InvalidMessage);
    }

    unsafe {
        let mut i = 0usize;
        while i < MAX_IPC_MESSAGES {
            if !IPC_QUEUE[i].in_use {
                let mut slot = EMPTY_MESSAGE;
                slot.header = header;
                if !payload.is_empty() {
                    slot.payload[..payload.len()].copy_from_slice(payload);
                }
                slot.in_use = true;
                IPC_QUEUE[i] = slot;
                return Ok(());
            }
            i += 1;
        }
    }

    Err(IpcError::QueueFull)
}

pub fn recv(out: &mut [u8]) -> Result<(UiMessageHeaderRaw, usize), IpcError> {
    unsafe {
        let mut i = 0usize;
        while i < MAX_IPC_MESSAGES {
            let slot = IPC_QUEUE[i];
            if slot.in_use {
                let payload_len = slot.header.payload_len as usize;
                if payload_len > out.len() {
                    return Err(IpcError::PayloadTooLarge);
                }

                if payload_len != 0 {
                    out[..payload_len].copy_from_slice(&slot.payload[..payload_len]);
                }

                IPC_QUEUE[i] = EMPTY_MESSAGE;
                return Ok((slot.header, payload_len));
            }
            i += 1;
        }
    }

    Err(IpcError::QueueEmpty)
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
