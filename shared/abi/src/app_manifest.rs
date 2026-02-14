pub const APP_MANIFEST_MAGIC: &[u8; 6] = b"OAPPM1";

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    FilesRead = 1,
    FilesWrite = 2,
    NetworkClient = 3,
    Notifications = 4,
    SettingsRead = 5,
    SettingsWrite = 6,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppManifestHeader {
    pub magic: [u8; 6],
    pub version: u16,
    pub permissions_count: u16,
    pub signature_offset: u32,
    pub signature_len: u32,
}
