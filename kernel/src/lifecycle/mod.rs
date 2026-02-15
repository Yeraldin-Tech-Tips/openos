use crate::{arch::x86_64::serial, sched::TaskId, sync::IrqSafeLock};

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

struct LifecycleState {
    app_records: [AppRecord; MAX_APP_RECORDS],
    foreground_pid: TaskId,
    next_record_seq: u64,
    spawn_total: u64,
    evict_total: u64,
    launch_history: [LaunchHistoryEntry; MAX_LAUNCH_HISTORY],
    launch_history_head: usize,
    launch_history_len: usize,
}

const EMPTY_STATE: LifecycleState = LifecycleState {
    app_records: [EMPTY_APP_RECORD; MAX_APP_RECORDS],
    foreground_pid: INVALID_PID,
    next_record_seq: 1,
    spawn_total: 0,
    evict_total: 0,
    launch_history: [EMPTY_LAUNCH_HISTORY_ENTRY; MAX_LAUNCH_HISTORY],
    launch_history_head: 0,
    launch_history_len: 0,
};

static LIFECYCLE_STATE: IrqSafeLock<LifecycleState> = IrqSafeLock::new(EMPTY_STATE);

pub fn init() {
    let mut state = LIFECYCLE_STATE.lock();
    *state = EMPTY_STATE;
}

pub fn on_spawn(parent_pid: TaskId, pid: TaskId, spawn_arg: u64) {
    let kind = kind_for_spawn_arg(spawn_arg);
    {
        let mut state = LIFECYCLE_STATE.lock();
        state.spawn_total = state.spawn_total.wrapping_add(1);
        clear_foreground_locked(&mut state);
        let slot = find_or_alloc_slot_locked(&mut state, pid);
        let Some(idx) = slot else {
            drop(state);
            serial::write_line("[openos-kernel] app registry full");
            return;
        };

        let seq = next_seq_locked(&mut state);
        state.app_records[idx] = AppRecord {
            in_use: true,
            pid,
            parent_pid,
            kind,
            state: AppState::Foreground,
            exit_status: 0,
            seq,
        };
        state.foreground_pid = pid;
        push_launch_history_locked(&mut state, seq, pid, kind);
    }

    serial::write_hex_u64("[openos-kernel] app.spawn.pid=", pid.0 as u64);
    serial::write_hex_u64("[openos-kernel] app.spawn.kind=", kind as u8 as u64);
}

pub fn on_task_running(pid: TaskId) {
    let mut state = LIFECYCLE_STATE.lock();
    if !set_foreground_locked(&mut state, pid) {
        clear_foreground_locked(&mut state);
        state.foreground_pid = INVALID_PID;
    }
}

pub fn on_exit(pid: TaskId, exit_status: i64) {
    let mut should_log = false;
    {
        let mut state = LIFECYCLE_STATE.lock();
        if state.foreground_pid == pid {
            state.foreground_pid = INVALID_PID;
        }

        if let Some(idx) = find_slot_locked(&state, pid) {
            let mut record = state.app_records[idx];
            record.state = AppState::Exited;
            record.exit_status = exit_status;
            record.seq = next_seq_locked(&mut state);
            state.app_records[idx] = record;
            should_log = true;
        }
    }

    if should_log {
        serial::write_hex_u64("[openos-kernel] app.exit.pid=", pid.0 as u64);
        serial::write_hex_u64("[openos-kernel] app.exit.status=", exit_status as u64);
    }
}

pub fn snapshot(out: &mut [AppSnapshot]) -> usize {
    if out.is_empty() {
        return 0;
    }

    let state = LIFECYCLE_STATE.lock();
    let fg = state.foreground_pid;
    let mut written = 0usize;
    let mut i = 0usize;
    while i < MAX_APP_RECORDS && written < out.len() {
        let record = state.app_records[i];
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

pub fn snapshot_launch_history(out: &mut [LaunchHistorySnapshot]) -> usize {
    if out.is_empty() {
        return 0;
    }

    let state = LIFECYCLE_STATE.lock();
    let mut written = 0usize;
    let mut i = 0usize;
    while i < state.launch_history_len && written < out.len() {
        let idx = (state.launch_history_head + MAX_LAUNCH_HISTORY - 1 - i) % MAX_LAUNCH_HISTORY;
        let entry = state.launch_history[idx];
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

pub fn stats() -> LifecycleStats {
    let state = LIFECYCLE_STATE.lock();
    let mut record_count = 0usize;
    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        if state.app_records[i].in_use {
            record_count += 1;
        }
        i += 1;
    }

    LifecycleStats {
        spawn_total: state.spawn_total,
        evict_total: state.evict_total,
        record_count,
        foreground_pid: state.foreground_pid,
    }
}

pub fn status_for_kind(kind: AppKind) -> KindStatus {
    let state = LIFECYCLE_STATE.lock();
    let mut best: Option<AppRecord> = None;
    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        let record = state.app_records[i];
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
            foreground: state.foreground_pid == record.pid && record.state == AppState::Foreground,
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

fn set_foreground_locked(state: &mut LifecycleState, pid: TaskId) -> bool {
    let Some(idx) = find_slot_locked(state, pid) else {
        return false;
    };

    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        if state.app_records[i].in_use
            && state.app_records[i].state == AppState::Foreground
            && state.app_records[i].pid != pid
        {
            let mut record = state.app_records[i];
            record.state = AppState::Queued;
            state.app_records[i] = record;
        }
        i += 1;
    }

    let mut target = state.app_records[idx];
    if target.state != AppState::Exited {
        target.state = AppState::Foreground;
        target.seq = next_seq_locked(state);
        state.app_records[idx] = target;
        state.foreground_pid = pid;
    }
    true
}

fn clear_foreground_locked(state: &mut LifecycleState) {
    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        if state.app_records[i].in_use && state.app_records[i].state == AppState::Foreground {
            let mut record = state.app_records[i];
            record.state = AppState::Queued;
            state.app_records[i] = record;
        }
        i += 1;
    }
}

fn find_slot_locked(state: &LifecycleState, pid: TaskId) -> Option<usize> {
    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        if state.app_records[i].in_use && state.app_records[i].pid == pid {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_or_alloc_slot_locked(state: &mut LifecycleState, pid: TaskId) -> Option<usize> {
    if let Some(idx) = find_slot_locked(state, pid) {
        return Some(idx);
    }

    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        if !state.app_records[i].in_use {
            return Some(i);
        }
        i += 1;
    }

    evict_slot_locked(state)
}

fn evict_slot_locked(state: &mut LifecycleState) -> Option<usize> {
    let mut candidate = find_oldest_slot_locked(state, AppState::Exited);
    if candidate.is_none() {
        candidate = find_oldest_slot_locked(state, AppState::Queued);
    }
    if candidate.is_none() {
        candidate = find_oldest_non_foreground_slot_locked(state);
    }

    if let Some(idx) = candidate {
        let record = state.app_records[idx];
        state.app_records[idx] = EMPTY_APP_RECORD;
        if state.foreground_pid == record.pid {
            state.foreground_pid = INVALID_PID;
        }
        state.evict_total = state.evict_total.wrapping_add(1);
    }

    candidate
}

fn find_oldest_slot_locked(state: &LifecycleState, state_filter: AppState) -> Option<usize> {
    let mut best_idx: Option<usize> = None;
    let mut best_seq = u64::MAX;
    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        let record = state.app_records[i];
        if record.in_use && record.state == state_filter && record.seq < best_seq {
            best_seq = record.seq;
            best_idx = Some(i);
        }
        i += 1;
    }
    best_idx
}

fn find_oldest_non_foreground_slot_locked(state: &LifecycleState) -> Option<usize> {
    let mut best_idx: Option<usize> = None;
    let mut best_seq = u64::MAX;
    let mut i = 0usize;
    while i < MAX_APP_RECORDS {
        let record = state.app_records[i];
        if record.in_use && record.pid != state.foreground_pid && record.seq < best_seq {
            best_seq = record.seq;
            best_idx = Some(i);
        }
        i += 1;
    }
    best_idx
}

fn push_launch_history_locked(state: &mut LifecycleState, seq: u64, pid: TaskId, kind: AppKind) {
    let head = state.launch_history_head;
    state.launch_history[head] = LaunchHistoryEntry {
        in_use: true,
        seq,
        pid,
        kind,
    };
    state.launch_history_head = (head + 1) % MAX_LAUNCH_HISTORY;
    if state.launch_history_len < MAX_LAUNCH_HISTORY {
        state.launch_history_len += 1;
    }
}

fn next_seq_locked(state: &mut LifecycleState) -> u64 {
    let seq = state.next_record_seq;
    state.next_record_seq = state.next_record_seq.wrapping_add(1);
    if state.next_record_seq == 0 {
        state.next_record_seq = 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_transitions_stress_preserve_latest_state() {
        init();
        for i in 0..128u32 {
            let pid = TaskId(i + 1);
            on_spawn(TaskId(1), pid, 1);
            on_task_running(pid);
            on_exit(pid, i as i64);
        }
        let status = status_for_kind(AppKind::Shell);
        assert!(status.present);
        assert_eq!(status.state, AppState::Exited);
        assert_eq!(status.exit_status, 127);
    }
}
