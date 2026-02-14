#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syscall {
    ProcSpawn = 0x0001,
    ProcExit = 0x0002,
    ProcWait = 0x0003,

    VmMap = 0x0101,
    VmUnmap = 0x0102,

    FsOpen = 0x0201,
    FsRead = 0x0202,
    FsWrite = 0x0203,
    FsClose = 0x0204,

    NetSocket = 0x0301,
    NetConnect = 0x0302,
    NetSend = 0x0303,
    NetRecv = 0x0304,

    IpcSend = 0x0401,
    IpcRecv = 0x0402,

    GfxSubmitScene = 0x0501,
    GfxPresent = 0x0502,

    InputSubscribe = 0x0601,
    InputRead = 0x0602,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SyscallResult {
    pub code: i64,
    pub value: u64,
}

impl SyscallResult {
    pub const fn ok(value: u64) -> Self {
        Self { code: 0, value }
    }

    pub const fn err(code: i64) -> Self {
        Self { code, value: 0 }
    }

    pub const fn is_ok(&self) -> bool {
        self.code == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syscall_discriminants_process_group() {
        assert_eq!(Syscall::ProcSpawn as u16, 0x0001);
        assert_eq!(Syscall::ProcExit as u16, 0x0002);
        assert_eq!(Syscall::ProcWait as u16, 0x0003);
    }

    #[test]
    fn syscall_discriminants_vm_group() {
        assert_eq!(Syscall::VmMap as u16, 0x0101);
        assert_eq!(Syscall::VmUnmap as u16, 0x0102);
    }

    #[test]
    fn syscall_discriminants_fs_group() {
        assert_eq!(Syscall::FsOpen as u16, 0x0201);
        assert_eq!(Syscall::FsRead as u16, 0x0202);
        assert_eq!(Syscall::FsWrite as u16, 0x0203);
        assert_eq!(Syscall::FsClose as u16, 0x0204);
    }

    #[test]
    fn syscall_discriminants_net_group() {
        assert_eq!(Syscall::NetSocket as u16, 0x0301);
        assert_eq!(Syscall::NetConnect as u16, 0x0302);
        assert_eq!(Syscall::NetSend as u16, 0x0303);
        assert_eq!(Syscall::NetRecv as u16, 0x0304);
    }

    #[test]
    fn syscall_discriminants_ipc_group() {
        assert_eq!(Syscall::IpcSend as u16, 0x0401);
        assert_eq!(Syscall::IpcRecv as u16, 0x0402);
    }

    #[test]
    fn syscall_discriminants_gfx_group() {
        assert_eq!(Syscall::GfxSubmitScene as u16, 0x0501);
        assert_eq!(Syscall::GfxPresent as u16, 0x0502);
    }

    #[test]
    fn syscall_discriminants_input_group() {
        assert_eq!(Syscall::InputSubscribe as u16, 0x0601);
        assert_eq!(Syscall::InputRead as u16, 0x0602);
    }

    #[test]
    fn syscall_groups_do_not_overlap() {
        let all_vals: [u16; 18] = [
            Syscall::ProcSpawn as u16,
            Syscall::ProcExit as u16,
            Syscall::ProcWait as u16,
            Syscall::VmMap as u16,
            Syscall::VmUnmap as u16,
            Syscall::FsOpen as u16,
            Syscall::FsRead as u16,
            Syscall::FsWrite as u16,
            Syscall::FsClose as u16,
            Syscall::NetSocket as u16,
            Syscall::NetConnect as u16,
            Syscall::NetSend as u16,
            Syscall::NetRecv as u16,
            Syscall::IpcSend as u16,
            Syscall::IpcRecv as u16,
            Syscall::GfxSubmitScene as u16,
            Syscall::GfxPresent as u16,
            Syscall::InputSubscribe as u16,
        ];
        // All discriminants must be unique
        for i in 0..all_vals.len() {
            for j in (i + 1)..all_vals.len() {
                assert_ne!(
                    all_vals[i], all_vals[j],
                    "syscall discriminants must be unique"
                );
            }
        }
    }

    #[test]
    fn syscall_result_ok() {
        let r = SyscallResult::ok(42);
        assert_eq!(r.code, 0);
        assert_eq!(r.value, 42);
        assert!(r.is_ok());
    }

    #[test]
    fn syscall_result_err() {
        let r = SyscallResult::err(-1);
        assert_eq!(r.code, -1);
        assert_eq!(r.value, 0);
        assert!(!r.is_ok());
    }

    #[test]
    fn syscall_result_ok_zero() {
        let r = SyscallResult::ok(0);
        assert!(r.is_ok());
        assert_eq!(r.value, 0);
    }

    #[test]
    fn syscall_result_layout() {
        assert_eq!(
            core::mem::size_of::<SyscallResult>(),
            16, // i64 + u64
            "SyscallResult must be 16 bytes for ABI stability"
        );
    }
}
