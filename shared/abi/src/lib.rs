#![no_std]

pub mod app_manifest;
pub mod boot;
pub mod input;
pub mod ipc;
pub mod syscalls;

pub const ABI_VERSION: u32 = 1;
