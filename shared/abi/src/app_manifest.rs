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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_manifest_magic_value() {
        assert_eq!(APP_MANIFEST_MAGIC, b"OAPPM1");
        assert_eq!(APP_MANIFEST_MAGIC.len(), 6);
    }

    #[test]
    fn permission_discriminants() {
        assert_eq!(Permission::FilesRead as u8, 1);
        assert_eq!(Permission::FilesWrite as u8, 2);
        assert_eq!(Permission::NetworkClient as u8, 3);
        assert_eq!(Permission::Notifications as u8, 4);
        assert_eq!(Permission::SettingsRead as u8, 5);
        assert_eq!(Permission::SettingsWrite as u8, 6);
    }

    #[test]
    fn permission_discriminants_unique() {
        let vals = [
            Permission::FilesRead as u8,
            Permission::FilesWrite as u8,
            Permission::NetworkClient as u8,
            Permission::Notifications as u8,
            Permission::SettingsRead as u8,
            Permission::SettingsWrite as u8,
        ];
        for i in 0..vals.len() {
            for j in (i + 1)..vals.len() {
                assert_ne!(vals[i], vals[j], "permission discriminants must be unique");
            }
        }
    }

    #[test]
    fn app_manifest_header_construction() {
        let header = AppManifestHeader {
            magic: *APP_MANIFEST_MAGIC,
            version: 1,
            permissions_count: 3,
            signature_offset: 64,
            signature_len: 256,
        };
        assert_eq!(&header.magic, APP_MANIFEST_MAGIC);
        assert_eq!(header.version, 1);
        assert_eq!(header.permissions_count, 3);
        assert_eq!(header.signature_offset, 64);
        assert_eq!(header.signature_len, 256);
    }

    #[test]
    fn app_manifest_header_layout() {
        // [u8;6] + u16 + u16 + 2 pad + u32 + u32 = 20 (repr(C) alignment)
        assert_eq!(
            core::mem::size_of::<AppManifestHeader>(),
            20,
            "AppManifestHeader must be 20 bytes for ABI stability"
        );
    }
}
