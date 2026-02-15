use core::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    arch::x86_64::{
        interrupts::{self, InterruptFrame},
        serial,
        user::{self, UserContext},
    },
    mm,
    sync::IrqSafeLock,
};

pub const MAX_TASKS: usize = 32;

const INVALID_TASK_SLOT: usize = usize::MAX;
const TIME_SLICE_TICKS: usize = 5;
const MAX_EXIT_EVENTS: usize = 64;
const PAGE_SIZE: u64 = 4096;
const VM_DYNAMIC_BASE: u64 = 0x0000_2000_0000_0000;
const VM_DYNAMIC_LIMIT_EXCLUSIVE: u64 = 0x0000_7FFF_0000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskId(pub u32);

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Unused = 0,
    Ready = 1,
    Running = 2,
    Blocked = 3,
    Exited = 4,
}

#[derive(Clone, Copy)]
struct CpuRegisters {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rdi: u64,
    rsi: u64,
    rbp: u64,
    rbx: u64,
    rdx: u64,
    rcx: u64,
    rax: u64,
}

impl CpuRegisters {
    const fn zeroed() -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rdi: 0,
            rsi: 0,
            rbp: 0,
            rbx: 0,
            rdx: 0,
            rcx: 0,
            rax: 0,
        }
    }

    fn save_from_frame(&mut self, frame: &InterruptFrame) {
        self.r15 = frame.r15;
        self.r14 = frame.r14;
        self.r13 = frame.r13;
        self.r12 = frame.r12;
        self.r11 = frame.r11;
        self.r10 = frame.r10;
        self.r9 = frame.r9;
        self.r8 = frame.r8;
        self.rdi = frame.rdi;
        self.rsi = frame.rsi;
        self.rbp = frame.rbp;
        self.rbx = frame.rbx;
        self.rdx = frame.rdx;
        self.rcx = frame.rcx;
        self.rax = frame.rax;
    }

    fn restore_into_frame(&self, frame: &mut InterruptFrame) {
        frame.r15 = self.r15;
        frame.r14 = self.r14;
        frame.r13 = self.r13;
        frame.r12 = self.r12;
        frame.r11 = self.r11;
        frame.r10 = self.r10;
        frame.r9 = self.r9;
        frame.r8 = self.r8;
        frame.rdi = self.rdi;
        frame.rsi = self.rsi;
        frame.rbp = self.rbp;
        frame.rbx = self.rbx;
        frame.rdx = self.rdx;
        frame.rcx = self.rcx;
        frame.rax = self.rax;
    }
}

#[derive(Clone, Copy)]
struct ExitEvent {
    in_use: bool,
    parent_pid: TaskId,
    child_pid: TaskId,
    exit_status: i64,
}

const EMPTY_EXIT_EVENT: ExitEvent = ExitEvent {
    in_use: false,
    parent_pid: TaskId(0),
    child_pid: TaskId(0),
    exit_status: 0,
};

#[derive(Clone, Copy)]
pub struct TaskRegistration {
    pub pid: TaskId,
    pub parent_pid: TaskId,
    pub context: UserContext,
    pub image_base: u64,
    pub image_size: usize,
    pub entry_staging: *const u8,
    pub segment_count: usize,
    pub image_source_id: u32,
}

#[derive(Clone, Copy)]
pub struct TaskDescriptor {
    pub pid: TaskId,
    pub parent_pid: TaskId,
    pub state: TaskState,
    pub address_space: mm::AddressSpaceId,
    pub exit_requested: bool,
    pub entry_point: u64,
    pub context: UserContext,
    pub image_base: u64,
    pub image_size: usize,
    pub segment_count: usize,
    pub image_source_id: u32,
    next_vm_base: u64,
    pending_exit_status: i64,
    exit_collected: bool,
    regs: CpuRegisters,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExitEventMetrics {
    pub dropped: usize,
    pub evicted: usize,
    pub recovered: usize,
}

#[derive(Clone, Copy)]
pub struct TaskSnapshot {
    pub pid: TaskId,
    pub parent_pid: TaskId,
    pub state: TaskState,
    pub exit_requested: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterTaskError {
    InvalidPid,
    InvalidImage,
    MemoryMapFailed,
    TableFull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchTaskError {
    MissingTask,
    InvalidContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnTaskError {
    MissingCurrentTask,
    InvalidParent,
    TableFull,
    MemoryMapFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitTaskError {
    MissingCurrentTask,
    InvalidCurrentTask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmRangeError {
    MissingCurrentTask,
    InvalidCurrentTask,
    InvalidLength,
    RangeOverflow,
}

const EMPTY_TASK: TaskDescriptor = TaskDescriptor {
    pid: TaskId(0),
    parent_pid: TaskId(0),
    state: TaskState::Unused,
    address_space: mm::AddressSpaceId::INVALID,
    exit_requested: false,
    entry_point: 0,
    context: UserContext {
        instruction_pointer: 0,
        stack_pointer: 0,
        rflags: 0,
    },
    image_base: 0,
    image_size: 0,
    segment_count: 0,
    image_source_id: 0,
    next_vm_base: VM_DYNAMIC_BASE,
    pending_exit_status: 0,
    exit_collected: false,
    regs: CpuRegisters::zeroed(),
};

static TASK_COUNT: AtomicUsize = AtomicUsize::new(0);
static NEXT_PID: AtomicUsize = AtomicUsize::new(2);
static CURRENT_TASK_SLOT: AtomicUsize = AtomicUsize::new(INVALID_TASK_SLOT);
static TICKS_IN_SLICE: AtomicUsize = AtomicUsize::new(0);
static CONTEXT_SWITCH_COUNT: AtomicUsize = AtomicUsize::new(0);
static EXIT_EVENT_DROPPED: AtomicUsize = AtomicUsize::new(0);
static EXIT_EVENT_EVICTED: AtomicUsize = AtomicUsize::new(0);
static EXIT_EVENT_RECOVERED: AtomicUsize = AtomicUsize::new(0);
static EXIT_EVENT_HEAD: AtomicUsize = AtomicUsize::new(0);
static EXIT_EVENT_COUNT: AtomicUsize = AtomicUsize::new(0);

static mut TASKS: [TaskDescriptor; MAX_TASKS] = [EMPTY_TASK; MAX_TASKS];
static mut EXIT_EVENTS: [ExitEvent; MAX_EXIT_EVENTS] = [EMPTY_EXIT_EVENT; MAX_EXIT_EVENTS];
static SCHED_STATE_LOCK: IrqSafeLock<()> = IrqSafeLock::new(());

pub fn init() {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    unsafe {
        let task_ptr = core::ptr::addr_of_mut!(TASKS).cast::<TaskDescriptor>();
        let mut i = 0usize;
        while i < MAX_TASKS {
            task_ptr.add(i).write(EMPTY_TASK);
            i += 1;
        }

        let event_ptr = core::ptr::addr_of_mut!(EXIT_EVENTS).cast::<ExitEvent>();
        let mut j = 0usize;
        while j < MAX_EXIT_EVENTS {
            event_ptr.add(j).write(EMPTY_EXIT_EVENT);
            j += 1;
        }
    }

    TASK_COUNT.store(0, Ordering::Release);
    NEXT_PID.store(2, Ordering::Release);
    CURRENT_TASK_SLOT.store(INVALID_TASK_SLOT, Ordering::Release);
    TICKS_IN_SLICE.store(0, Ordering::Release);
    CONTEXT_SWITCH_COUNT.store(0, Ordering::Release);
    EXIT_EVENT_DROPPED.store(0, Ordering::Release);
    EXIT_EVENT_EVICTED.store(0, Ordering::Release);
    EXIT_EVENT_RECOVERED.store(0, Ordering::Release);
    EXIT_EVENT_HEAD.store(0, Ordering::Release);
    EXIT_EVENT_COUNT.store(0, Ordering::Release);
}

pub fn register_user_task(reg: TaskRegistration) -> Result<TaskId, RegisterTaskError> {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    if reg.pid.0 == 0 {
        return Err(RegisterTaskError::InvalidPid);
    }
    if reg.image_size == 0
        || reg.entry_staging.is_null()
        || reg.context.instruction_pointer == 0
        || reg.image_source_id == 0
    {
        return Err(RegisterTaskError::InvalidImage);
    }

    let slot = find_registration_slot().ok_or(RegisterTaskError::TableFull)?;
    let count = TASK_COUNT.load(Ordering::Acquire);

    let address_space = mm::map_user_task_image(
        reg.context.instruction_pointer,
        reg.entry_staging,
        reg.image_base,
        reg.image_size,
        reg.context.stack_pointer,
        user::DEFAULT_USER_STACK_SIZE,
    )
    .map_err(|_| RegisterTaskError::MemoryMapFailed)?;

    unsafe {
        let mut regs = CpuRegisters::zeroed();
        regs.rdi = reg.pid.0 as u64;
        task_ptr_mut(slot).write(TaskDescriptor {
            pid: reg.pid,
            parent_pid: reg.parent_pid,
            state: TaskState::Ready,
            address_space,
            exit_requested: false,
            entry_point: reg.context.instruction_pointer,
            context: reg.context,
            image_base: reg.image_base,
            image_size: reg.image_size,
            segment_count: reg.segment_count,
            image_source_id: reg.image_source_id,
            next_vm_base: VM_DYNAMIC_BASE,
            pending_exit_status: 0,
            exit_collected: false,
            regs,
        });
    }

    if slot >= count {
        TASK_COUNT.store(slot + 1, Ordering::Release);
    }

    Ok(reg.pid)
}

pub fn spawn_from_current() -> Result<TaskId, SpawnTaskError> {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    let current_slot = CURRENT_TASK_SLOT.load(Ordering::Acquire);
    let count = TASK_COUNT.load(Ordering::Acquire);
    if current_slot >= count {
        return Err(SpawnTaskError::MissingCurrentTask);
    }

    let parent = unsafe { task_ptr_const(current_slot).read() };
    if !matches!(parent.state, TaskState::Running | TaskState::Ready)
        || parent.entry_point == 0
        || parent.image_size == 0
        || parent.image_source_id == 0
    {
        return Err(SpawnTaskError::InvalidParent);
    }

    let loaded = crate::init::resolve_task_image(parent.image_source_id)
        .map_err(|_| SpawnTaskError::InvalidParent)?;

    let pid = allocate_pid();
    let child_context = UserContext::for_entry(loaded.entry_virtual);
    register_user_task(TaskRegistration {
        pid,
        parent_pid: parent.pid,
        context: child_context,
        image_base: loaded.image_base,
        image_size: loaded.image_size,
        entry_staging: loaded.entry_staging,
        segment_count: loaded.segment_count,
        image_source_id: parent.image_source_id,
    })
    .map_err(|err| match err {
        RegisterTaskError::TableFull => SpawnTaskError::TableFull,
        RegisterTaskError::MemoryMapFailed => SpawnTaskError::MemoryMapFailed,
        RegisterTaskError::InvalidPid | RegisterTaskError::InvalidImage => {
            SpawnTaskError::InvalidParent
        }
    })?;

    Ok(pid)
}

pub fn request_current_exit(status: i64) -> Result<TaskId, ExitTaskError> {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    let current_slot = CURRENT_TASK_SLOT.load(Ordering::Acquire);
    let count = TASK_COUNT.load(Ordering::Acquire);
    if current_slot >= count {
        return Err(ExitTaskError::MissingCurrentTask);
    }

    unsafe {
        let task_ptr = task_ptr_mut(current_slot);
        let mut task = task_ptr.read();
        if !matches!(task.state, TaskState::Running | TaskState::Ready) {
            return Err(ExitTaskError::InvalidCurrentTask);
        }
        task.exit_requested = true;
        task.pending_exit_status = status;
        task_ptr.write(task);
        Ok(task.pid)
    }
}

pub fn current_task_id() -> Option<TaskId> {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    let slot = CURRENT_TASK_SLOT.load(Ordering::Acquire);
    let count = TASK_COUNT.load(Ordering::Acquire);
    if slot >= count {
        return None;
    }

    let task = unsafe { task_ptr_const(slot).read() };
    if task.state == TaskState::Unused {
        None
    } else {
        Some(task.pid)
    }
}

pub fn collect_child_exit(parent_pid: TaskId) -> Option<(TaskId, i64)> {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    unsafe { dequeue_exit_event(parent_pid).or_else(|| recover_child_exit(parent_pid)) }
}

pub fn exit_event_metrics() -> ExitEventMetrics {
    ExitEventMetrics {
        dropped: EXIT_EVENT_DROPPED.load(Ordering::Acquire),
        evicted: EXIT_EVENT_EVICTED.load(Ordering::Acquire),
        recovered: EXIT_EVENT_RECOVERED.load(Ordering::Acquire),
    }
}

pub fn allocate_task_id() -> TaskId {
    allocate_pid()
}

pub fn current_task_address_space() -> Option<mm::AddressSpaceId> {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    let slot = CURRENT_TASK_SLOT.load(Ordering::Acquire);
    let count = TASK_COUNT.load(Ordering::Acquire);
    if slot >= count {
        return None;
    }

    let task = unsafe { task_ptr_const(slot).read() };
    if task.state == TaskState::Unused {
        None
    } else {
        Some(task.address_space)
    }
}

pub fn reserve_current_vm_range(len: usize) -> Result<u64, VmRangeError> {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    if len == 0 {
        return Err(VmRangeError::InvalidLength);
    }

    let current_slot = CURRENT_TASK_SLOT.load(Ordering::Acquire);
    let count = TASK_COUNT.load(Ordering::Acquire);
    if current_slot >= count {
        return Err(VmRangeError::MissingCurrentTask);
    }

    unsafe {
        let task_ptr = task_ptr_mut(current_slot);
        let mut task = task_ptr.read();
        if !matches!(task.state, TaskState::Running | TaskState::Ready) {
            return Err(VmRangeError::InvalidCurrentTask);
        }

        let aligned_len = align_up_u64(len as u64).ok_or(VmRangeError::RangeOverflow)?;
        let start = align_up_u64(task.next_vm_base).ok_or(VmRangeError::RangeOverflow)?;
        let end = start
            .checked_add(aligned_len)
            .ok_or(VmRangeError::RangeOverflow)?;
        if end == 0 || end > VM_DYNAMIC_LIMIT_EXCLUSIVE {
            return Err(VmRangeError::RangeOverflow);
        }

        task.next_vm_base = end
            .checked_add(PAGE_SIZE)
            .ok_or(VmRangeError::RangeOverflow)?;
        task_ptr.write(task);
        Ok(start)
    }
}

pub fn task_descriptor(pid: TaskId) -> Option<TaskDescriptor> {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    let count = TASK_COUNT.load(Ordering::Acquire);
    let mut i = 0usize;
    while i < count {
        let task = unsafe { task_ptr_const(i).read() };
        if task.state != TaskState::Unused && task.pid == pid {
            return Some(task);
        }
        i += 1;
    }
    None
}

pub fn snapshot_tasks(out: &mut [TaskSnapshot]) -> usize {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    if out.is_empty() {
        return 0;
    }

    let count = TASK_COUNT.load(Ordering::Acquire);
    let mut written = 0usize;
    let mut i = 0usize;
    while i < count && written < out.len() {
        let task = unsafe { task_ptr_const(i).read() };
        if task.state != TaskState::Unused {
            out[written] = TaskSnapshot {
                pid: task.pid,
                parent_pid: task.parent_pid,
                state: task.state,
                exit_requested: task.exit_requested,
            };
            written += 1;
        }
        i += 1;
    }

    written
}

pub fn dispatch_task(pid: TaskId) -> Result<(), DispatchTaskError> {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    let count = TASK_COUNT.load(Ordering::Acquire);
    let mut i = 0usize;

    while i < count {
        let task_ptr = unsafe { task_ptr_mut(i) };
        let mut task = unsafe { task_ptr.read() };

        if task.state != TaskState::Unused && task.pid == pid {
            if task.context.instruction_pointer == 0 || task.context.stack_pointer == 0 {
                return Err(DispatchTaskError::InvalidContext);
            }

            task.state = TaskState::Running;
            unsafe {
                task_ptr.write(task);
            }
            crate::lifecycle::on_task_running(task.pid);
            CURRENT_TASK_SLOT.store(i, Ordering::Release);
            TICKS_IN_SLICE.store(0, Ordering::Release);
            interrupts::enable_timer_irq();

            unsafe {
                run_user_entry(
                    task.pid.0 as u64,
                    task.address_space,
                    task.context.instruction_pointer,
                    task.context.stack_pointer,
                );
            }
        }

        i += 1;
    }

    Err(DispatchTaskError::MissingTask)
}

pub fn on_timer_tick(frame: &mut InterruptFrame, tick_count: u64) {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    if (frame.cs & 0x3) != 0x3 {
        return;
    }

    let count = TASK_COUNT.load(Ordering::Acquire);
    if count == 0 {
        return;
    }

    let current_slot = CURRENT_TASK_SLOT.load(Ordering::Acquire);
    if current_slot >= count {
        return;
    }

    let current_exit_requested = unsafe { task_ptr_const(current_slot).read().exit_requested };
    if !current_exit_requested {
        unsafe {
            save_slot_context(current_slot, frame);
        }

        let elapsed = TICKS_IN_SLICE.fetch_add(1, Ordering::AcqRel) + 1;
        if elapsed < TIME_SLICE_TICKS {
            return;
        }
        TICKS_IN_SLICE.store(0, Ordering::Release);
    } else {
        TICKS_IN_SLICE.store(0, Ordering::Release);
    }

    let Some(next_slot) = next_runnable_slot(current_slot, count) else {
        return;
    };
    if next_slot == current_slot {
        return;
    }

    let mut retired_asid = mm::AddressSpaceId::INVALID;
    let mut exit_event: Option<(TaskId, TaskId, i64)> = None;
    unsafe {
        if current_exit_requested {
            let (pid, asid, parent_pid, status) = retire_task_slot(current_slot);
            retired_asid = asid;
            exit_event = Some((parent_pid, pid, status));
        } else {
            set_task_state(current_slot, TaskState::Ready);
        }
        set_task_state(next_slot, TaskState::Running);
        restore_slot_context(next_slot, frame);
    }
    CURRENT_TASK_SLOT.store(next_slot, Ordering::Release);

    if let Some((parent_pid, child_pid, status)) = exit_event {
        unsafe {
            enqueue_exit_event(parent_pid, child_pid, status);
        }
        release_address_space(retired_asid);
    }

    let switch_count = CONTEXT_SWITCH_COUNT.fetch_add(1, Ordering::AcqRel) + 1;
    if switch_count <= 8 || (switch_count % 128) == 0 {
        serial::write_hex_u64("[openos-kernel] sched.switch_count=", switch_count as u64);
        serial::write_hex_u64("[openos-kernel] sched.tick=", tick_count);
    }
}

pub fn handle_user_fault(frame: &mut InterruptFrame, vector: u8, error_code: u64) -> bool {
    let _sched_guard = SCHED_STATE_LOCK.lock();
    if (frame.cs & 0x3) != 0x3 {
        return false;
    }

    let count = TASK_COUNT.load(Ordering::Acquire);
    if count == 0 {
        return false;
    }

    let current_slot = CURRENT_TASK_SLOT.load(Ordering::Acquire);
    if current_slot >= count {
        return false;
    }

    let fault_status = 0x100 + vector as i64;
    let (fault_pid, retired_asid, parent_pid, exit_status) = unsafe {
        save_slot_context(current_slot, frame);
        retire_task_slot_with_status(current_slot, fault_status)
    };

    serial::write_hex_u64("[openos-kernel] task.fault.pid=", fault_pid.0 as u64);
    serial::write_hex_u64("[openos-kernel] task.fault.vector=", vector as u64);
    serial::write_hex_u64("[openos-kernel] task.fault.error=", error_code);

    let Some(next_slot) = next_runnable_slot(current_slot, count) else {
        CURRENT_TASK_SLOT.store(INVALID_TASK_SLOT, Ordering::Release);
        interrupts::disable_timer_irq();
        return false;
    };
    if next_slot == current_slot {
        CURRENT_TASK_SLOT.store(INVALID_TASK_SLOT, Ordering::Release);
        interrupts::disable_timer_irq();
        return false;
    }

    unsafe {
        set_task_state(next_slot, TaskState::Running);
        restore_slot_context(next_slot, frame);
        enqueue_exit_event(parent_pid, fault_pid, exit_status);
    }
    CURRENT_TASK_SLOT.store(next_slot, Ordering::Release);
    TICKS_IN_SLICE.store(0, Ordering::Release);
    release_address_space(retired_asid);
    true
}

fn find_registration_slot() -> Option<usize> {
    let count = TASK_COUNT.load(Ordering::Acquire);
    let mut i = 0usize;
    while i < count {
        let task = unsafe { task_ptr_const(i).read() };
        if task.state == TaskState::Unused
            || (task.state == TaskState::Exited && task.exit_collected)
        {
            return Some(i);
        }
        i += 1;
    }

    if count < MAX_TASKS {
        Some(count)
    } else {
        None
    }
}

fn parent_can_reap_child(parent_pid: TaskId) -> bool {
    if parent_pid.0 == 0 {
        return false;
    }

    let count = TASK_COUNT.load(Ordering::Acquire);
    let mut i = 0usize;
    while i < count {
        let task = unsafe { task_ptr_const(i).read() };
        if task.pid == parent_pid {
            return matches!(task.state, TaskState::Ready | TaskState::Running);
        }
        i += 1;
    }

    false
}

fn allocate_pid() -> TaskId {
    allocate_pid_from_counter(&NEXT_PID)
}

fn allocate_pid_from_counter(counter: &AtomicUsize) -> TaskId {
    loop {
        let next = counter.fetch_add(1, Ordering::AcqRel) as u32;
        if next >= 2 {
            return TaskId(next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_pid_skips_reserved_ids_after_u32_wrap() {
        let counter = AtomicUsize::new(u32::MAX as usize);

        let max_pid = allocate_pid_from_counter(&counter);
        assert_eq!(max_pid, TaskId(u32::MAX));

        let wrapped_pid = allocate_pid_from_counter(&counter);
        assert_eq!(wrapped_pid, TaskId(2));
        assert_ne!(wrapped_pid, TaskId(0));
        assert_ne!(wrapped_pid, TaskId(1));
    }
}

fn release_address_space(asid: mm::AddressSpaceId) {
    if asid == mm::AddressSpaceId::INVALID {
        return;
    }

    if mm::release_user_address_space(asid).is_err() {
        serial::write_hex_u64("[openos-kernel] mm.release_failed.asid=", asid.0 as u64);
    } else {
        serial::write_hex_u64("[openos-kernel] mm.released.asid=", asid.0 as u64);
    }
}

fn next_runnable_slot(current_slot: usize, count: usize) -> Option<usize> {
    let mut offset = 1usize;
    while offset <= count {
        let slot = (current_slot + offset) % count;
        let task = unsafe { task_ptr_const(slot).read() };
        if matches!(task.state, TaskState::Ready | TaskState::Running) && !task.exit_requested {
            return Some(slot);
        }
        offset += 1;
    }
    None
}

unsafe fn save_slot_context(slot: usize, frame: &InterruptFrame) {
    let task_ptr = task_ptr_mut(slot);
    let mut task = task_ptr.read();
    if matches!(task.state, TaskState::Unused | TaskState::Exited) {
        return;
    }

    task.context.instruction_pointer = frame.rip;
    task.context.stack_pointer = frame.rsp;
    task.context.rflags = frame.rflags;
    task.regs.save_from_frame(frame);
    task_ptr.write(task);
}

unsafe fn restore_slot_context(slot: usize, frame: &mut InterruptFrame) {
    let task = task_ptr_const(slot).read();
    mm::activate_user_address_space(task.address_space);
    task.regs.restore_into_frame(frame);
    frame.rip = task.context.instruction_pointer;
    frame.rsp = task.context.stack_pointer;
    frame.rflags = task.context.rflags;
}

unsafe fn set_task_state(slot: usize, state: TaskState) {
    let task_ptr = task_ptr_mut(slot);
    let mut task = task_ptr.read();
    if task.state == TaskState::Unused || task.state == TaskState::Exited {
        return;
    }
    let pid = task.pid;
    task.state = state;
    task_ptr.write(task);
    if state == TaskState::Running {
        crate::lifecycle::on_task_running(pid);
    }
}

unsafe fn retire_task_slot(slot: usize) -> (TaskId, mm::AddressSpaceId, TaskId, i64) {
    let task = task_ptr_const(slot).read();
    retire_task_slot_with_status(slot, task.pending_exit_status)
}

unsafe fn retire_task_slot_with_status(
    slot: usize,
    status: i64,
) -> (TaskId, mm::AddressSpaceId, TaskId, i64) {
    let task_ptr = task_ptr_mut(slot);
    let mut task = task_ptr.read();

    let pid = task.pid;
    let asid = task.address_space;
    let parent_pid = task.parent_pid;

    crate::fs::close_all_for_pid(pid);
    crate::net::close_all_for_pid(pid);
    crate::lifecycle::on_exit(pid, status);

    task.state = TaskState::Exited;
    task.address_space = mm::AddressSpaceId::INVALID;
    task.exit_requested = false;
    task.pending_exit_status = status;
    task.exit_collected = false;
    task_ptr.write(task);

    (pid, asid, parent_pid, status)
}

unsafe fn enqueue_exit_event(parent_pid: TaskId, child_pid: TaskId, exit_status: i64) {
    if !parent_can_reap_child(parent_pid) {
        EXIT_EVENT_DROPPED.fetch_add(1, Ordering::AcqRel);
        mark_exit_collected(child_pid);
        return;
    }

    let head = EXIT_EVENT_HEAD.load(Ordering::Acquire);
    let count = EXIT_EVENT_COUNT.load(Ordering::Acquire);

    if count < MAX_EXIT_EVENTS {
        let slot = (head + count) % MAX_EXIT_EVENTS;
        EXIT_EVENTS[slot] = ExitEvent {
            in_use: true,
            parent_pid,
            child_pid,
            exit_status,
        };
        EXIT_EVENT_COUNT.store(count + 1, Ordering::Release);
        return;
    }

    EXIT_EVENT_EVICTED.fetch_add(1, Ordering::AcqRel);
    EXIT_EVENTS[head] = ExitEvent {
        in_use: true,
        parent_pid,
        child_pid,
        exit_status,
    };
    EXIT_EVENT_HEAD.store((head + 1) % MAX_EXIT_EVENTS, Ordering::Release);
    serial::write_line("[openos-kernel] wait queue full, evicting oldest exit event");
}

unsafe fn dequeue_exit_event(parent_pid: TaskId) -> Option<(TaskId, i64)> {
    let head = EXIT_EVENT_HEAD.load(Ordering::Acquire);
    let count = EXIT_EVENT_COUNT.load(Ordering::Acquire);

    let mut offset = 0usize;
    while offset < count {
        let slot = (head + offset) % MAX_EXIT_EVENTS;
        let event = EXIT_EVENTS[slot];
        if event.parent_pid == parent_pid {
            let child_pid = event.child_pid;
            let status = event.exit_status;

            let mut shift = offset;
            while shift + 1 < count {
                let from = (head + shift + 1) % MAX_EXIT_EVENTS;
                let to = (head + shift) % MAX_EXIT_EVENTS;
                EXIT_EVENTS[to] = EXIT_EVENTS[from];
                shift += 1;
            }
            let tail = (head + count - 1) % MAX_EXIT_EVENTS;
            EXIT_EVENTS[tail] = EMPTY_EXIT_EVENT;
            EXIT_EVENT_COUNT.store(count - 1, Ordering::Release);

            mark_exit_collected(child_pid);
            return Some((child_pid, status));
        }
        offset += 1;
    }
    None
}

unsafe fn recover_child_exit(parent_pid: TaskId) -> Option<(TaskId, i64)> {
    let count = TASK_COUNT.load(Ordering::Acquire);
    let mut i = 0usize;
    while i < count {
        let task_ptr = task_ptr_mut(i);
        let mut task = task_ptr.read();
        if task.parent_pid == parent_pid && task.state == TaskState::Exited && !task.exit_collected
        {
            task.exit_collected = true;
            let child_pid = task.pid;
            let exit_status = task.pending_exit_status;
            task.pending_exit_status = 0;
            task_ptr.write(task);
            EXIT_EVENT_RECOVERED.fetch_add(1, Ordering::AcqRel);
            return Some((child_pid, exit_status));
        }
        i += 1;
    }

    None
}

unsafe fn mark_exit_collected(child_pid: TaskId) {
    let count = TASK_COUNT.load(Ordering::Acquire);
    let mut i = 0usize;
    while i < count {
        let task_ptr = task_ptr_mut(i);
        let mut task = task_ptr.read();
        if task.pid == child_pid && task.state == TaskState::Exited && !task.exit_collected {
            task.exit_collected = true;
            task.pending_exit_status = 0;
            task_ptr.write(task);
            return;
        }
        i += 1;
    }
}

unsafe fn task_ptr_const(slot: usize) -> *const TaskDescriptor {
    core::ptr::addr_of!(TASKS)
        .cast::<TaskDescriptor>()
        .add(slot)
}

unsafe fn task_ptr_mut(slot: usize) -> *mut TaskDescriptor {
    core::ptr::addr_of_mut!(TASKS)
        .cast::<TaskDescriptor>()
        .add(slot)
}

unsafe fn run_user_entry(
    entry_arg0: u64,
    address_space: mm::AddressSpaceId,
    entry_virtual: u64,
    user_stack_top: u64,
) -> ! {
    mm::activate_user_address_space(address_space);
    user::enter_user_mode(entry_virtual, user_stack_top, entry_arg0)
}

fn align_up_u64(value: u64) -> Option<u64> {
    value
        .checked_add(PAGE_SIZE - 1)
        .map(|v| v & !(PAGE_SIZE - 1))
}
