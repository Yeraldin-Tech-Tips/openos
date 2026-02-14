mod elf;

use abi::boot::{BootInfo, BootModule, BootModuleKind};
use core::{
    ptr, slice,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    arch::x86_64::{serial, user::UserContext},
    sched::{self, TaskId, TaskRegistration},
};

const MAX_CACHED_MODULES: usize = 8;
const EMPTY_MODULE: BootModule = BootModule {
    kind: BootModuleKind::InitExecutable,
    _reserved: 0,
    base: ptr::null(),
    size: 0,
};

static mut CACHED_MODULES: [BootModule; MAX_CACHED_MODULES] = [EMPTY_MODULE; MAX_CACHED_MODULES];
static CACHED_MODULE_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModuleTaskError {
    MissingModule,
    InvalidModule,
    UnsupportedFormat,
    LoadFailed,
    SchedulerRejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitLaunchError {
    MissingModule,
    InvalidModule,
    UnsupportedFormat,
    LoadFailed,
    SchedulerRejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnModuleError {
    UnsupportedSpawnArg,
    MissingModule,
    InvalidModule,
    UnsupportedFormat,
    LoadFailed,
    SchedulerRejected,
}

pub fn launch_pid1(boot: &BootInfo) -> TaskId {
    cache_boot_modules(boot);
    match try_launch_pid1_from_module() {
        Ok(pid) => pid,
        Err(err) => {
            serial::write_line(init_error_message(err));
            launch_pid1_stub()
        }
    }
}

pub fn spawn_from_boot_module(
    spawn_arg: u64,
    parent_pid: TaskId,
) -> Result<TaskId, SpawnModuleError> {
    let module_kind = spawn_arg_to_kind(spawn_arg).ok_or(SpawnModuleError::UnsupportedSpawnArg)?;
    let module = find_cached_module(module_kind).ok_or(SpawnModuleError::MissingModule)?;

    let pid = spawn_task_from_module(module, parent_pid, None).map_err(map_spawn_error)?;
    serial::write_hex_u64(
        "[openos-kernel] spawn.module.kind=",
        module.kind as u32 as u64,
    );
    serial::write_hex_u64("[openos-kernel] spawn.module.pid=", pid.0 as u64);
    Ok(pid)
}

fn try_launch_pid1_from_module() -> Result<TaskId, InitLaunchError> {
    let init_module =
        find_cached_module(BootModuleKind::InitExecutable).ok_or(InitLaunchError::MissingModule)?;

    serial::write_line("[openos-kernel] init module discovered");
    serial::write_hex_u64("[openos-kernel] init.base=", init_module.base as u64);
    serial::write_hex_u64("[openos-kernel] init.size=", init_module.size as u64);

    let pid =
        spawn_task_from_module(init_module, TaskId(0), Some(TaskId(1))).map_err(map_init_error)?;
    serial::write_line("[openos-kernel] launch pid1 from init module");
    Ok(pid)
}

fn spawn_task_from_module(
    module: BootModule,
    parent_pid: TaskId,
    pid_override: Option<TaskId>,
) -> Result<TaskId, ModuleTaskError> {
    let loaded = load_module_image(module)?;

    serial::write_line("[openos-kernel] module elf staged");
    serial::write_hex_u64("[openos-kernel] module.image_base=", loaded.image_base);
    serial::write_hex_u64(
        "[openos-kernel] module.image_size=",
        loaded.image_size as u64,
    );
    serial::write_hex_u64(
        "[openos-kernel] module.entry_virtual=",
        loaded.entry_virtual,
    );
    serial::write_hex_u64(
        "[openos-kernel] module.entry_staging=",
        loaded.entry_staging as u64,
    );
    serial::write_hex_u64(
        "[openos-kernel] module.load_segments=",
        loaded.segment_count as u64,
    );

    let mut i = 0usize;
    while i < loaded.segment_count {
        let seg = loaded.segments[i];
        serial::write_hex_u64("[openos-kernel] seg.vaddr=", seg.vaddr);
        serial::write_hex_u64("[openos-kernel] seg.filesz=", seg.file_size);
        serial::write_hex_u64("[openos-kernel] seg.memsz=", seg.mem_size);
        serial::write_hex_u64("[openos-kernel] seg.flags=", seg.flags as u64);
        serial::write_hex_u64(
            "[openos-kernel] seg.staging_off=",
            seg.staging_offset as u64,
        );
        i += 1;
    }

    let pid = pid_override.unwrap_or_else(sched::allocate_task_id);
    let user_context = UserContext::for_entry(loaded.entry_virtual);
    sched::register_user_task(TaskRegistration {
        pid,
        parent_pid,
        context: user_context,
        image_base: loaded.image_base,
        image_size: loaded.image_size,
        entry_staging: loaded.entry_staging,
        segment_count: loaded.segment_count,
    })
    .map_err(|_| ModuleTaskError::SchedulerRejected)?;

    serial::write_hex_u64("[openos-kernel] task.registered.pid=", pid.0 as u64);
    serial::write_hex_u64(
        "[openos-kernel] task.user_stack_top=",
        user_context.stack_pointer,
    );
    Ok(pid)
}

fn load_module_image(module: BootModule) -> Result<elf::LoadedInitImage, ModuleTaskError> {
    if module.base.is_null() || module.size < 4 {
        return Err(ModuleTaskError::InvalidModule);
    }
    let image = unsafe { slice::from_raw_parts(module.base, module.size) };
    if &image[0..4] != b"\x7FELF" {
        return Err(ModuleTaskError::UnsupportedFormat);
    }

    elf::load_elf64_image(image).map_err(|_| ModuleTaskError::LoadFailed)
}

fn cache_boot_modules(boot: &BootInfo) {
    unsafe {
        CACHED_MODULES = [EMPTY_MODULE; MAX_CACHED_MODULES];
    }
    CACHED_MODULE_COUNT.store(0, Ordering::Release);

    if boot.modules.entries.is_null() || boot.modules.count == 0 {
        return;
    }

    let modules = unsafe { slice::from_raw_parts(boot.modules.entries, boot.modules.count) };
    let mut cached = 0usize;
    let mut i = 0usize;
    while i < modules.len() && cached < MAX_CACHED_MODULES {
        let module = modules[i];
        if !module.base.is_null() && module.size >= 4 {
            unsafe {
                CACHED_MODULES[cached] = module;
            }
            cached += 1;
        }
        i += 1;
    }

    CACHED_MODULE_COUNT.store(cached, Ordering::Release);
    serial::write_hex_u64("[openos-kernel] modules.cached=", cached as u64);
}

fn find_cached_module(kind: BootModuleKind) -> Option<BootModule> {
    let count = CACHED_MODULE_COUNT.load(Ordering::Acquire);
    let mut i = 0usize;
    while i < count {
        let module = unsafe { CACHED_MODULES[i] };
        if module.kind == kind {
            return Some(module);
        }
        i += 1;
    }
    None
}

fn spawn_arg_to_kind(spawn_arg: u64) -> Option<BootModuleKind> {
    match spawn_arg {
        1 => Some(BootModuleKind::AppShellExecutable),
        2 => Some(BootModuleKind::AppSettingsExecutable),
        3 => Some(BootModuleKind::AppFilesExecutable),
        _ => None,
    }
}

fn map_init_error(err: ModuleTaskError) -> InitLaunchError {
    match err {
        ModuleTaskError::MissingModule => InitLaunchError::MissingModule,
        ModuleTaskError::InvalidModule => InitLaunchError::InvalidModule,
        ModuleTaskError::UnsupportedFormat => InitLaunchError::UnsupportedFormat,
        ModuleTaskError::LoadFailed => InitLaunchError::LoadFailed,
        ModuleTaskError::SchedulerRejected => InitLaunchError::SchedulerRejected,
    }
}

fn map_spawn_error(err: ModuleTaskError) -> SpawnModuleError {
    match err {
        ModuleTaskError::MissingModule => SpawnModuleError::MissingModule,
        ModuleTaskError::InvalidModule => SpawnModuleError::InvalidModule,
        ModuleTaskError::UnsupportedFormat => SpawnModuleError::UnsupportedFormat,
        ModuleTaskError::LoadFailed => SpawnModuleError::LoadFailed,
        ModuleTaskError::SchedulerRejected => SpawnModuleError::SchedulerRejected,
    }
}

pub fn launch_pid1_stub() -> TaskId {
    let pid = TaskId(1);
    serial::write_line("[openos-kernel] launch pid1 stub: openos-init");
    pid
}

fn init_error_message(err: InitLaunchError) -> &'static str {
    match err {
        InitLaunchError::MissingModule => "[openos-kernel] init module missing",
        InitLaunchError::InvalidModule => "[openos-kernel] init module invalid",
        InitLaunchError::UnsupportedFormat => "[openos-kernel] init module unsupported format",
        InitLaunchError::LoadFailed => "[openos-kernel] init module ELF load failed",
        InitLaunchError::SchedulerRejected => "[openos-kernel] init task registration failed",
    }
}
