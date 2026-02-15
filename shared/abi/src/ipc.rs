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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_channel_discriminants() {
        assert_eq!(UiChannel::ShellLifecycle as u16, 1);
        assert_eq!(UiChannel::NotificationCenter as u16, 2);
        assert_eq!(UiChannel::ControlCenter as u16, 3);
        assert_eq!(UiChannel::AppLaunch as u16, 4);
    }

    #[test]
    fn ui_channel_uniqueness() {
        let vals = [
            UiChannel::ShellLifecycle as u16,
            UiChannel::NotificationCenter as u16,
            UiChannel::ControlCenter as u16,
            UiChannel::AppLaunch as u16,
        ];
        for i in 0..vals.len() {
            for j in (i + 1)..vals.len() {
                assert_ne!(vals[i], vals[j], "channel discriminants must be unique");
            }
        }
    }

    #[test]
    fn ui_message_kind_discriminants() {
        assert_eq!(UiMessageKind::LaunchApp as u16, 0x01);
        assert_eq!(UiMessageKind::CloseApp as u16, 0x02);
        assert_eq!(UiMessageKind::PublishNotification as u16, 0x03);
        assert_eq!(UiMessageKind::ToggleControl as u16, 0x04);
    }

    #[test]
    fn ui_message_header_construction() {
        let header = UiMessageHeader {
            channel: UiChannel::AppLaunch,
            kind: UiMessageKind::LaunchApp,
            payload_len: 128,
        };
        assert_eq!(header.channel, UiChannel::AppLaunch);
        assert_eq!(header.kind, UiMessageKind::LaunchApp);
        assert_eq!(header.payload_len, 128);
    }

    #[test]
    fn ui_message_header_zero_payload() {
        let header = UiMessageHeader {
            channel: UiChannel::ControlCenter,
            kind: UiMessageKind::ToggleControl,
            payload_len: 0,
        };
        assert_eq!(header.payload_len, 0);
    }

    #[test]
    fn ui_message_header_max_payload() {
        let header = UiMessageHeader {
            channel: UiChannel::NotificationCenter,
            kind: UiMessageKind::PublishNotification,
            payload_len: u16::MAX,
        };
        assert_eq!(header.payload_len, u16::MAX);
    }
}
