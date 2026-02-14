use crate::arch::x86_64::tables::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};

pub const DEFAULT_USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_F000;
pub const DEFAULT_USER_STACK_SIZE: usize = 1024 * 1024;
pub const DEFAULT_USER_RFLAGS: u64 = 0x202;

#[derive(Clone, Copy)]
pub struct UserContext {
    pub instruction_pointer: u64,
    pub stack_pointer: u64,
    pub rflags: u64,
}

impl UserContext {
    pub const fn for_entry(entry_virtual: u64) -> Self {
        Self {
            instruction_pointer: entry_virtual,
            stack_pointer: DEFAULT_USER_STACK_TOP,
            rflags: DEFAULT_USER_RFLAGS,
        }
    }
}

pub unsafe fn enter_user_mode(entry_virtual: u64, user_stack_top: u64, arg0: u64) -> ! {
    core::arch::asm!(
        "mov rdi, {arg0}",
        "push {user_ss}",
        "push {user_rsp}",
        "pushfq",
        "or qword ptr [rsp], 0x200",
        "push {user_cs}",
        "push {user_rip}",
        "iretq",
        user_ss = in(reg) USER_DATA_SELECTOR as u64,
        user_rsp = in(reg) user_stack_top,
        user_cs = in(reg) USER_CODE_SELECTOR as u64,
        user_rip = in(reg) entry_virtual,
        arg0 = in(reg) arg0,
        options(noreturn),
    );
}
