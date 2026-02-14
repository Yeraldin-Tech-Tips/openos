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
}
