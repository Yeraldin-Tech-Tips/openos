use core::cmp::min;

use crate::{
    lifecycle::{AppKind, AppSnapshot, AppState, LaunchHistorySnapshot},
    sched::{TaskId, TaskSnapshot, TaskState},
};

pub const MAX_PATH_BYTES: usize = 256;
const MAX_OPEN_FILES: usize = 64;
const MAX_FILE_BYTES: usize = 1024;

const ROOT_NODE: usize = 0;
const ETC_NODE: usize = 1;
const PROC_NODE: usize = 2;
const PROC_SELF_NODE: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsError {
    InvalidPath,
    NotFound,
    TableFull,
    BadFd,
    AccessDenied,
}

#[derive(Clone, Copy)]
enum NodeKind {
    Dir,
    StaticFile(&'static [u8]),
    DynamicFile(fn(TaskId, &mut [u8]) -> usize),
}

#[derive(Clone, Copy)]
struct Node {
    name: &'static [u8],
    parent: Option<usize>,
    kind: NodeKind,
}

const NODES: [Node; 12] = [
    Node {
        name: b"",
        parent: None,
        kind: NodeKind::Dir,
    },
    Node {
        name: b"etc",
        parent: Some(ROOT_NODE),
        kind: NodeKind::Dir,
    },
    Node {
        name: b"proc",
        parent: Some(ROOT_NODE),
        kind: NodeKind::Dir,
    },
    Node {
        name: b"openos-release",
        parent: Some(ETC_NODE),
        kind: NodeKind::StaticFile(b"OpenOS 0.1-dev\n"),
    },
    Node {
        name: b"gesture-map",
        parent: Some(ETC_NODE),
        kind: NodeKind::StaticFile(
            b"Alt+Up=Home\nAlt+Left=AppSwitcherLeft\nAlt+Right=AppSwitcherRight\nAlt+Down=ControlCenter\nAlt+Shift+Down=NotificationCenter\n",
        ),
    },
    Node {
        name: b"boot-state",
        parent: Some(PROC_NODE),
        kind: NodeKind::StaticFile(b"boot=ring3\nipc=on\ngfx=on\ninput=irq1\n"),
    },
    Node {
        name: b"self",
        parent: Some(PROC_NODE),
        kind: NodeKind::Dir,
    },
    Node {
        name: b"status",
        parent: Some(PROC_SELF_NODE),
        kind: NodeKind::DynamicFile(render_self_status),
    },
    Node {
        name: b"tree",
        parent: Some(PROC_NODE),
        kind: NodeKind::DynamicFile(render_tree),
    },
    Node {
        name: b"tasks",
        parent: Some(PROC_NODE),
        kind: NodeKind::DynamicFile(render_tasks),
    },
    Node {
        name: b"apps",
        parent: Some(PROC_NODE),
        kind: NodeKind::DynamicFile(render_apps),
    },
    Node {
        name: b"launcher-history",
        parent: Some(PROC_NODE),
        kind: NodeKind::DynamicFile(render_launcher_history),
    },
];

#[derive(Clone, Copy)]
struct DirIter {
    parent: usize,
    cursor: usize,
}

impl DirIter {
    const fn new(parent: usize) -> Self {
        Self { parent, cursor: 0 }
    }

    fn next(&mut self) -> Option<usize> {
        while self.cursor < NODES.len() {
            let idx = self.cursor;
            self.cursor += 1;
            if NODES[idx].parent == Some(self.parent) {
                return Some(idx);
            }
        }
        None
    }
}

#[derive(Clone, Copy)]
struct OpenFile {
    in_use: bool,
    owner_pid: TaskId,
    offset: usize,
    len: usize,
    data: [u8; MAX_FILE_BYTES],
}

const EMPTY_OPEN_FILE: OpenFile = OpenFile {
    in_use: false,
    owner_pid: TaskId(0),
    offset: 0,
    len: 0,
    data: [0; MAX_FILE_BYTES],
};

const EMPTY_TASK_SNAPSHOT: TaskSnapshot = TaskSnapshot {
    pid: TaskId(0),
    parent_pid: TaskId(0),
    state: TaskState::Unused,
    exit_requested: false,
};

const EMPTY_APP_SNAPSHOT: AppSnapshot = AppSnapshot {
    pid: TaskId(0),
    parent_pid: TaskId(0),
    kind: AppKind::Unknown,
    state: AppState::Queued,
    foreground: false,
    exit_status: 0,
};

const EMPTY_LAUNCH_HISTORY_SNAPSHOT: LaunchHistorySnapshot = LaunchHistorySnapshot {
    seq: 0,
    pid: TaskId(0),
    kind: AppKind::Unknown,
};

struct ByteWriter<'a> {
    out: &'a mut [u8],
    pos: usize,
}

impl<'a> ByteWriter<'a> {
    fn new(out: &'a mut [u8]) -> Self {
        Self { out, pos: 0 }
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        if self.pos >= self.out.len() {
            return;
        }
        let count = min(bytes.len(), self.out.len() - self.pos);
        self.out[self.pos..self.pos + count].copy_from_slice(&bytes[..count]);
        self.pos += count;
    }

    fn write_u64(&mut self, mut value: u64) {
        let mut buf = [0u8; 20];
        let mut i = buf.len();
        if value == 0 {
            self.write_bytes(b"0");
            return;
        }
        while value != 0 {
            i -= 1;
            buf[i] = b'0' + (value % 10) as u8;
            value /= 10;
        }
        self.write_bytes(&buf[i..]);
    }

    fn write_i64(&mut self, value: i64) {
        if value < 0 {
            self.write_bytes(b"-");
            self.write_u64(value.unsigned_abs());
            return;
        }
        self.write_u64(value as u64);
    }

    fn len(&self) -> usize {
        self.pos
    }
}

static mut OPEN_FILES: [OpenFile; MAX_OPEN_FILES] = [EMPTY_OPEN_FILE; MAX_OPEN_FILES];

pub fn init() {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(OPEN_FILES).cast::<OpenFile>();
        let mut i = 0usize;
        while i < MAX_OPEN_FILES {
            ptr.add(i).write(EMPTY_OPEN_FILE);
            i += 1;
        }
    }
}

pub fn open(pid: TaskId, path: &[u8], flags: u64) -> Result<u64, FsError> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES {
        return Err(FsError::InvalidPath);
    }
    if !path.starts_with(b"/") {
        return Err(FsError::InvalidPath);
    }
    if (flags & 0x1) != 0 {
        return Err(FsError::AccessDenied);
    }

    let node_idx = resolve_path(path).ok_or(FsError::NotFound)?;
    let mut data = [0u8; MAX_FILE_BYTES];
    let len = materialize_node(pid, node_idx, &mut data)?;

    unsafe {
        let mut slot = 0usize;
        while slot < MAX_OPEN_FILES {
            if !OPEN_FILES[slot].in_use {
                OPEN_FILES[slot] = OpenFile {
                    in_use: true,
                    owner_pid: pid,
                    offset: 0,
                    len,
                    data,
                };
                return Ok((slot as u64) + 3);
            }
            slot += 1;
        }
    }

    Err(FsError::TableFull)
}

pub fn read(pid: TaskId, fd: u64, out: &mut [u8]) -> Result<usize, FsError> {
    if fd < 3 {
        return Err(FsError::BadFd);
    }

    let slot = (fd - 3) as usize;
    if slot >= MAX_OPEN_FILES {
        return Err(FsError::BadFd);
    }

    unsafe {
        let mut entry = OPEN_FILES[slot];
        if !entry.in_use || entry.owner_pid != pid {
            return Err(FsError::BadFd);
        }

        if entry.offset >= entry.len {
            return Ok(0);
        }

        let available = entry.len - entry.offset;
        let count = min(available, out.len());
        out[..count].copy_from_slice(&entry.data[entry.offset..entry.offset + count]);
        entry.offset += count;
        OPEN_FILES[slot] = entry;
        Ok(count)
    }
}

pub fn close(pid: TaskId, fd: u64) -> Result<(), FsError> {
    if fd < 3 {
        return Err(FsError::BadFd);
    }

    let slot = (fd - 3) as usize;
    if slot >= MAX_OPEN_FILES {
        return Err(FsError::BadFd);
    }

    unsafe {
        let entry = OPEN_FILES[slot];
        if !entry.in_use || entry.owner_pid != pid {
            return Err(FsError::BadFd);
        }
        OPEN_FILES[slot] = EMPTY_OPEN_FILE;
    }
    Ok(())
}

pub fn close_all_for_pid(pid: TaskId) {
    unsafe {
        let mut i = 0usize;
        while i < MAX_OPEN_FILES {
            if OPEN_FILES[i].in_use && OPEN_FILES[i].owner_pid == pid {
                OPEN_FILES[i] = EMPTY_OPEN_FILE;
            }
            i += 1;
        }
    }
}

fn resolve_path(path: &[u8]) -> Option<usize> {
    if path == b"/" {
        return Some(ROOT_NODE);
    }

    let mut current = ROOT_NODE;
    let mut i = 1usize;
    while i < path.len() {
        while i < path.len() && path[i] == b'/' {
            i += 1;
        }
        if i >= path.len() {
            break;
        }
        let start = i;
        while i < path.len() && path[i] != b'/' {
            i += 1;
        }
        let component = &path[start..i];
        if component.is_empty() || component == b"." || component == b".." {
            return None;
        }
        current = find_child(current, component)?;
    }

    Some(current)
}

fn find_child(parent: usize, name: &[u8]) -> Option<usize> {
    let mut iter = DirIter::new(parent);
    while let Some(idx) = iter.next() {
        if NODES[idx].name == name {
            return Some(idx);
        }
    }
    None
}

fn materialize_node(pid: TaskId, node_idx: usize, out: &mut [u8]) -> Result<usize, FsError> {
    match NODES[node_idx].kind {
        NodeKind::Dir => Ok(render_directory_listing(node_idx, out)),
        NodeKind::StaticFile(bytes) => {
            let count = min(bytes.len(), out.len());
            out[..count].copy_from_slice(&bytes[..count]);
            Ok(count)
        }
        NodeKind::DynamicFile(f) => Ok(f(pid, out)),
    }
}

fn render_directory_listing(dir_idx: usize, out: &mut [u8]) -> usize {
    let mut writer = ByteWriter::new(out);
    let mut iter = DirIter::new(dir_idx);
    while let Some(idx) = iter.next() {
        writer.write_bytes(NODES[idx].name);
        if let NodeKind::Dir = NODES[idx].kind {
            writer.write_bytes(b"/");
        }
        writer.write_bytes(b"\n");
    }
    writer.len()
}

fn render_self_status(pid: TaskId, out: &mut [u8]) -> usize {
    let mut writer = ByteWriter::new(out);
    writer.write_bytes(b"pid=");
    writer.write_u64(pid.0 as u64);
    writer.write_bytes(b"\nopen_fds=");
    writer.write_u64(count_open_fds(pid) as u64);
    writer.write_bytes(b"\n");
    writer.len()
}

fn render_tree(_pid: TaskId, out: &mut [u8]) -> usize {
    let mut writer = ByteWriter::new(out);
    writer.write_bytes(b"/\n");
    render_tree_children(ROOT_NODE, 1, &mut writer);
    writer.len()
}

fn render_tasks(_pid: TaskId, out: &mut [u8]) -> usize {
    let mut writer = ByteWriter::new(out);
    let mut snapshots = [EMPTY_TASK_SNAPSHOT; crate::sched::MAX_TASKS];
    let snapshot_count = crate::sched::snapshot_tasks(&mut snapshots);
    let mut i = 0usize;
    while i < snapshot_count {
        let task = snapshots[i];
        writer.write_bytes(b"pid=");
        writer.write_u64(task.pid.0 as u64);
        writer.write_bytes(b" parent=");
        writer.write_u64(task.parent_pid.0 as u64);
        writer.write_bytes(b" state=");
        writer.write_bytes(task_state_label(task.state));
        writer.write_bytes(b" exit_requested=");
        writer.write_u64(if task.exit_requested { 1 } else { 0 });
        writer.write_bytes(b"\n");
        i += 1;
    }
    writer.len()
}

fn render_apps(_pid: TaskId, out: &mut [u8]) -> usize {
    let mut writer = ByteWriter::new(out);
    let stats = crate::lifecycle::stats();
    writer.write_bytes(b"spawn_total=");
    writer.write_u64(stats.spawn_total);
    writer.write_bytes(b" evict_total=");
    writer.write_u64(stats.evict_total);
    writer.write_bytes(b" records=");
    writer.write_u64(stats.record_count as u64);
    writer.write_bytes(b" foreground_pid=");
    writer.write_u64(stats.foreground_pid.0 as u64);
    writer.write_bytes(b"\n");

    let mut snapshots = [EMPTY_APP_SNAPSHOT; crate::lifecycle::MAX_APP_SNAPSHOTS];
    let snapshot_count = crate::lifecycle::snapshot(&mut snapshots);
    let mut i = 0usize;
    while i < snapshot_count {
        let app = snapshots[i];
        writer.write_bytes(b"pid=");
        writer.write_u64(app.pid.0 as u64);
        writer.write_bytes(b" parent=");
        writer.write_u64(app.parent_pid.0 as u64);
        writer.write_bytes(b" app=");
        writer.write_bytes(app_kind_label(app.kind));
        writer.write_bytes(b" state=");
        writer.write_bytes(app_state_label(app.state));
        writer.write_bytes(b" fg=");
        writer.write_u64(if app.foreground { 1 } else { 0 });
        writer.write_bytes(b" exit_status=");
        writer.write_i64(app.exit_status);
        writer.write_bytes(b"\n");
        i += 1;
    }
    writer.len()
}

fn render_launcher_history(_pid: TaskId, out: &mut [u8]) -> usize {
    let mut writer = ByteWriter::new(out);
    let mut snapshots =
        [EMPTY_LAUNCH_HISTORY_SNAPSHOT; crate::lifecycle::MAX_LAUNCH_HISTORY_SNAPSHOTS];
    let snapshot_count = crate::lifecycle::snapshot_launch_history(&mut snapshots);
    let mut i = 0usize;
    while i < snapshot_count {
        let entry = snapshots[i];
        writer.write_bytes(b"seq=");
        writer.write_u64(entry.seq);
        writer.write_bytes(b" pid=");
        writer.write_u64(entry.pid.0 as u64);
        writer.write_bytes(b" app=");
        writer.write_bytes(app_kind_label(entry.kind));
        writer.write_bytes(b"\n");
        i += 1;
    }
    writer.len()
}

fn app_kind_label(kind: AppKind) -> &'static [u8] {
    match kind {
        AppKind::Unknown => b"unknown",
        AppKind::Shell => b"shell",
        AppKind::Settings => b"settings",
        AppKind::Files => b"files",
    }
}

fn app_state_label(state: AppState) -> &'static [u8] {
    match state {
        AppState::Queued => b"queued",
        AppState::Foreground => b"foreground",
        AppState::Exited => b"exited",
    }
}

fn task_state_label(state: TaskState) -> &'static [u8] {
    match state {
        TaskState::Unused => b"unused",
        TaskState::Ready => b"ready",
        TaskState::Running => b"running",
        TaskState::Blocked => b"blocked",
        TaskState::Exited => b"exited",
    }
}

fn render_tree_children(parent: usize, depth: usize, writer: &mut ByteWriter<'_>) {
    let mut iter = DirIter::new(parent);
    while let Some(idx) = iter.next() {
        let mut i = 0usize;
        while i < depth {
            writer.write_bytes(b"  ");
            i += 1;
        }
        writer.write_bytes(NODES[idx].name);
        if let NodeKind::Dir = NODES[idx].kind {
            writer.write_bytes(b"/");
        }
        writer.write_bytes(b"\n");

        if let NodeKind::Dir = NODES[idx].kind {
            render_tree_children(idx, depth + 1, writer);
        }
    }
}

fn count_open_fds(pid: TaskId) -> usize {
    unsafe {
        let mut count = 0usize;
        let mut i = 0usize;
        while i < MAX_OPEN_FILES {
            if OPEN_FILES[i].in_use && OPEN_FILES[i].owner_pid == pid {
                count += 1;
            }
            i += 1;
        }
        count
    }
}
