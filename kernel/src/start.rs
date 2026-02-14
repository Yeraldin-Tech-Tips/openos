use crate::boot::BootInfo;

#[no_mangle]
#[link_section = ".start"]
pub extern "sysv64" fn openos_kernel_entry(boot_info_ptr: *const BootInfo) -> ! {
    crate::openos_kernel_main(boot_info_ptr)
}
