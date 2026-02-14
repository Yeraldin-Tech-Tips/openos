#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiChannel {
    ShellLifecycle = 1,
    NotificationCenter = 2,
    ControlCenter = 3,
    AppLaunch = 4,
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiMessageKind {
    LaunchApp = 0x01,
    CloseApp = 0x02,
    PublishNotification = 0x03,
    ToggleControl = 0x04,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UiMessageHeader {
    pub channel: UiChannel,
    pub kind: UiMessageKind,
    pub payload_len: u16,
}
