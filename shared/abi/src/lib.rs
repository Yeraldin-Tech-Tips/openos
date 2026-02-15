#![no_std]

pub mod app_manifest;
pub mod boot;
pub mod input;
pub mod ipc;
pub mod syscalls;

pub const ABI_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_version_is_v1() {
        assert_eq!(ABI_VERSION, 1);
    }

    #[test]
    fn all_submodules_accessible() {
        // Verify public modules compile and are accessible
        let _ = boot::BOOTINFO_MAGIC;
        let _ = syscalls::SyscallResult::ok(0);
        let _ = ipc::UiChannel::ShellLifecycle;
        let _ = input::GestureAction::Home;
        let _ = app_manifest::APP_MANIFEST_MAGIC;
    }
}
