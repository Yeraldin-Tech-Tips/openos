use crate::{arch::x86_64::serial, sched::TaskId};

const MAX_APP_RECORDS: usize = 16;
const MAX_LAUNCH_HISTORY: usize = 16;

pub const MAX_APP_SNAPSHOTS: usize = MAX_APP_RECORDS;
pub const MAX_LAUNCH_HISTORY_SNAPSHOTS: usize = MAX_LAUNCH_HISTORY;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppKind {
    Unknown = 0,
    Shell = 1,
    Settings = 2,
    Files = 3,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppState {
    Queued = 0,
    Foreground = 1,
    Exited = 2,
}

#[derive(Clone, Copy)]
pub struct AppSnapshot {
    pub pid: TaskId,
    pub parent_pid: TaskId,
    pub kind: AppKind,
    pub state: AppState,
    pub foreground: bool,
    pub exit_status: i64,
}

#[derive(Clone, Copy)]
pub struct LaunchHistorySnapshot {
    pub seq: u64,
    pub pid: TaskId,
    pub kind: AppKind,
}

#[derive(Clone, Copy)]
pub struct LifecycleStats {
    pub spawn_total: u64,
    pub evict_total: u64,
    pub record_count: usize,
    pub foreground_pid: TaskId,
}

#[derive(Clone, Copy)]
pub struct KindStatus {
    pub present: bool,
    pub pid: TaskId,
    pub state: AppState,
    pub foreground: bool,
    pub exit_status: i64,
}

#[derive(Clone, Copy)]
struct AppRecord {
    in_use: bool,
    pid: TaskId,
    parent_pid: TaskId,
    kind: AppKind,
    state: AppState,
    exit_status: i64,
    seq: u64,
}

#[derive(Clone, Copy)]
struct LaunchHistoryEntry {
    in_use: bool,
    seq: u64,
    pid: TaskId,
    kind: AppKind,
}

const EMPTY_APP_RECORD: AppRecord = AppRecord {
    in_use: false,
    pid: TaskId(0),
    parent_pid: TaskId(0),
    kind: AppKind::Unknown,
    state: AppState::Queued,
    exit_status: 0,
    seq: 0,
};

const EMPTY_LAUNCH_HISTORY_ENTRY: LaunchHistoryEntry = LaunchHistoryEntry {
    in_use: false,
    seq: 0,
    pid: TaskId(0),
    kind: AppKind::Unknown,
};

const INVALID_PID: TaskId = TaskId(0);

static mut APP_RECORDS: [AppRecord; MAX_APP_RECORDS] = [EMPTY_APP_RECORD; MAX_APP_RECORDS];
static mut FOREGROUND_PID: TaskId = INVALID_PID;
static mut NEXT_RECORD_SEQ: u64 = 1;
static mut SPAWN_TOTAL: u64 = 0;
static mut EVICT_TOTAL: u64 = 0;

static mut LAUNCH_HISTORY: [LaunchHistoryEntry; MAX_LAUNCH_HISTORY] =
    [EMPTY_LAUNCH_HISTORY_ENTRY; MAX_LAUNCH_HISTORY];
static mut LAUNCH_HISTORY_HEAD: usize = 0;
static mut LAUNCH_HISTORY_LEN: usize = 0;

pub fn init() {
    unsafe {
        let records_ptr = core::ptr::addr_of_mut!(APP_RECORDS).cast::<AppRecord>();
        let mut i = 0usize;
        while i < MAX_APP_RECORDS {
            records_ptr.add(i).write(EMPTY_APP_RECORD);
            i += 1;
        }

        let history_ptr = core::ptr::addr_of_mut!(LAUNCH_HISTORY).cast::<LaunchHistoryEntry>();
        let mut j = 0usize;
        while j < MAX_LAUNCH_HISTORY {
            history_ptr.add(j).write(EMPTY_LAUNCH_HISTORY_ENTRY);
            j += 1;
        }

        FOREGROUND_PID = INVALID_PID;
        NEXT_RECORD_SEQ = 1;
        SPAWN_TOTAL = 0;
        EVICT_TOTAL = 0;
        LAUNCH_HISTORY_HEAD = 0;
        LAUNCH_HISTORY_LEN = 0;
    }
}

pub fn on_spawn(parent_pid: TaskId, pid: TaskId, spawn_arg: u64) {
    let kind = kind_for_spawn_arg(spawn_arg);
    unsafe {
        SPAWN_TOTAL = SPAWN_TOTAL.wrapping_add(1);

        clear_foreground_locked();
        let slot = find_or_alloc_slot_locked(pid);
        let Some(idx) = slot else {
            serial::write_line("[openos-kernel] app registry full");
            return;
        };

        let seq = next_seq_locked();
        APP_RECORDS[idx] = AppRecord {
            in_use: true,
            pid,
            parent_pid,
            kind,
            state: AppState::Foreground,
            exit_status: 0,
            seq,
        };
        FOREGROUND_PID = pid;
        push_launch_history_locked(seq, pid, kind);
    }

    serial::write_hex_u64("[openos-kernel] app.spawn.pid=", pid.0 as u64);
    serial::write_hex_u64("[openos-kernel] app.spawn.kind=", kind as u8 as u64);
}

pub fn on_task_running(pid: TaskId) {
    unsafe {
        if !set_foreground_locked(pid) {
            clear_foreground_locked();
            FOREGROUND_PID = INVALID_PID;
        }
    }
}

pub fn on_exit(pid: TaskId, exit_status: i64) {
    unsafe {
        if FOREGROUND_PID == pid {
            FOREGROUND_PID = INVALID_PID;
        }

        if let Some(idx) = find_slot_locked(pid) {
            let mut record = APP_RECORDS[idx];
            record.state = AppState::Exited;
            record.exit_status = exit_status;
            record.seq = next_seq_locked();
            APP_RECORDS[idx] = record;
            serial::write_hex_u64("[openos-kernel] app.exit.pid=", pid.0 as u64);
            serial::write_hex_u64("[openos-kernel] app.exit.status=", exit_status as u64);
        }
    }
}

pub fn snapshot(out: &mut [AppSnapshot]) -> usize {
    if out.is_empty() {
        return 0;
    }

    unsafe {
        let fg = FOREGROUND_PID;
        let mut written = 0usize;
        let mut i = 0usize;
        while i < MAX_APP_RECORDS && written < out.len() {
            let record = APP_RECORDS[i];
            if record.in_use {
                out[written] = AppSnapshot {
                    pid: record.pid,
                    parent_pid: record.parent_pid,
                    kind: record.kind,
                    state: record.state,
                    foreground: fg == record.pid && record.state == AppState::Foreground,
                    exit_status: record.exit_status,
                };
                written += 1;
            }
            i += 1;
        }
        written
    }
}

pub fn snapshot_launch_history(out: &mut [LaunchHistorySnapshot]) -> usize {
    if out.is_empty() {
        return 0;
    }

    unsafe {
        let mut written = 0usize;
        let mut i = 0usize;
        while i < LAUNCH_HISTORY_LEN && written < out.len() {
            let idx = (LAUNCH_HISTORY_HEAD + MAX_LAUNCH_HISTORY - 1 - i) % MAX_LAUNCH_HISTORY;
            let entry = LAUNCH_HISTORY[idx];
            if entry.in_use {
                out[written] = LaunchHistorySnapshot {
                    seq: entry.seq,
                    pid: entry.pid,
                    kind: entry.kind,
                };
                written += 1;
            }
            i += 1;
        }
        written
    }
}

pub fn stats() -> LifecycleStats {
    unsafe {
        let mut record_count = 0usize;
        let mut i = 0usize;
        while i < MAX_APP_RECORDS {
            if APP_RECORDS[i].in_use {
                record_count += 1;
            }
            i += 1;
        }

        LifecycleStats {
            spawn_total: SPAWN_TOTAL,
            evict_total: EVICT_TOTAL,
            record_count,
            foreground_pid: FOREGROUND_PID,
        }
    }
}

pub fn status_for_kind(kind: AppKind) -> KindStatus {
    unsafe {
        let mut best: Option<AppRecord> = None;
        let mut i = 0usize;
        while i < MAX_APP_RECORDS {
            let record = APP_RECORDS[i];
            if record.in_use && record.kind == kind {
                match best {
                    Some(existing) if existing.seq >= record.seq => {}
                    _ => best = Some(record),
                }
            }
            i += 1;
        }

        if let Some(record) = best {
            return KindStatus {
                present: true,
                pid: record.pid,
                state: record.state,
                foreground: FOREGROUND_PID == record.pid && record.state == AppState::Foreground,
                exit_status: record.exit_status,
            };
        }

        KindStatus {
            present: false,
            pid: INVALID_PID,
            state: AppState::Queued,
            foreground: false,
            exit_status: 0,
        }
    }
}

unsafe fn set_foreground_locked(pid: TaskId) -> bool {
    let Some(idx) = find_slot_locked(pid) else {
        return false;
    };

    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        if APP_RECORDS[i].in_use
            && APP_RECORDS[i].state == AppState::Foreground
            && APP_RECORDS[i].pid != pid
        {
            let mut record = APP_RECORDS[i];
            record.state = AppState::Queued;
            APP_RECORDS[i] = record;
        }
        i += 1;
    }

    let mut target = APP_RECORDS[idx];
    if target.state != AppState::Exited {
        target.state = AppState::Foreground;
        target.seq = next_seq_locked();
        APP_RECORDS[idx] = target;
        FOREGROUND_PID = pid;
    }
    true
}

unsafe fn clear_foreground_locked() {
    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        if APP_RECORDS[i].in_use && APP_RECORDS[i].state == AppState::Foreground {
            let mut record = APP_RECORDS[i];
            record.state = AppState::Queued;
            APP_RECORDS[i] = record;
        }
        i += 1;
    }
}

unsafe fn find_slot_locked(pid: TaskId) -> Option<usize> {
    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        if APP_RECORDS[i].in_use && APP_RECORDS[i].pid == pid {
            return Some(i);
        }
        i += 1;
    }
    None
}

unsafe fn find_or_alloc_slot_locked(pid: TaskId) -> Option<usize> {
    if let Some(idx) = find_slot_locked(pid) {
        return Some(idx);
    }

    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        if !APP_RECORDS[i].in_use {
            return Some(i);
        }
        i += 1;
    }

    evict_slot_locked()
}

unsafe fn evict_slot_locked() -> Option<usize> {
    let mut candidate = find_oldest_slot_locked(AppState::Exited);
    if candidate.is_none() {
        candidate = find_oldest_slot_locked(AppState::Queued);
    }
    if candidate.is_none() {
        candidate = find_oldest_non_foreground_slot_locked();
    }

    if let Some(idx) = candidate {
        let record = APP_RECORDS[idx];
        serial::write_hex_u64("[openos-kernel] app.evict.pid=", record.pid.0 as u64);
        APP_RECORDS[idx] = EMPTY_APP_RECORD;
        if FOREGROUND_PID == record.pid {
            FOREGROUND_PID = INVALID_PID;
        }
        EVICT_TOTAL = EVICT_TOTAL.wrapping_add(1);
    }

    candidate
}

unsafe fn find_oldest_slot_locked(state: AppState) -> Option<usize> {
    let mut best_idx: Option<usize> = None;
    let mut best_seq = u64::MAX;
    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        let record = APP_RECORDS[i];
        if record.in_use && record.state == state && record.seq < best_seq {
            best_seq = record.seq;
            best_idx = Some(i);
        }
        i += 1;
    }
    best_idx
}

unsafe fn find_oldest_non_foreground_slot_locked() -> Option<usize> {
    let mut best_idx: Option<usize> = None;
    let mut best_seq = u64::MAX;
    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        let record = APP_RECORDS[i];
        if record.in_use && record.pid != FOREGROUND_PID && record.seq < best_seq {
            best_seq = record.seq;
            best_idx = Some(i);
        }
        i += 1;
    }
    best_idx
}

unsafe fn push_launch_history_locked(seq: u64, pid: TaskId, kind: AppKind) {
    LAUNCH_HISTORY[LAUNCH_HISTORY_HEAD] = LaunchHistoryEntry {
        in_use: true,
        seq,
        pid,
        kind,
    };
    LAUNCH_HISTORY_HEAD = (LAUNCH_HISTORY_HEAD + 1) % MAX_LAUNCH_HISTORY;
    if LAUNCH_HISTORY_LEN < MAX_LAUNCH_HISTORY {
        LAUNCH_HISTORY_LEN += 1;
    }
}

unsafe fn next_seq_locked() -> u64 {
    let seq = NEXT_RECORD_SEQ;
    NEXT_RECORD_SEQ = NEXT_RECORD_SEQ.wrapping_add(1);
    if NEXT_RECORD_SEQ == 0 {
        NEXT_RECORD_SEQ = 1;
    }
    if seq == 0 {
        1
    } else {
        seq
    }
}

fn kind_for_spawn_arg(spawn_arg: u64) -> AppKind {
    match spawn_arg {
        1 => AppKind::Shell,
        2 => AppKind::Settings,
        3 => AppKind::Files,
        _ => AppKind::Unknown,
    }
}
