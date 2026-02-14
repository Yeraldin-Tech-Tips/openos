# Syscall ABI v1

This document defines the initial syscall number map shared by kernel and userspace.

## Calling convention

- Syscall number: `u16`
- Arguments: four `u64` slots (`a0..a3`)
- Return: `SyscallResult { code: i64, value: u64 }`
- `code == 0` means success
- `code < 0` maps to errno-like failures

## Implemented argument contracts

- `ProcSpawn(a0=spawn_arg) -> value=child_pid`
- `ProcExit(a0=exit_status) -> value=0`
- `ProcWait(a0=status_out_ptr_or_0) -> value=child_pid`
- `VmMap(a0=addr_hint_or_0, a1=len, a2=flags) -> value=mapped_addr`
- `VmUnmap(a0=addr, a1=len) -> value=unmapped_pages`
- `FsOpen(a0=path_ptr, a1=path_len, a2=open_flags) -> value=fd`
- `FsRead(a0=fd, a1=buf_ptr, a2=len) -> value=bytes_read`
- `FsWrite(a0=fd, a1=buf_ptr, a2=len) -> value=bytes_written`
- `FsClose(a0=fd) -> value=0`
- `NetSocket(a0=domain, a1=kind, a2=protocol) -> value=fd`
- `NetConnect(a0=fd, a1=addr_ptr, a2=addr_len) -> value=0`
- `NetSend(a0=fd, a1=buf_ptr, a2=len) -> value=bytes_sent`
- `NetRecv(a0=fd, a1=buf_ptr, a2=len) -> value=bytes_read`
- `IpcSend(a0=header_ptr, a1=payload_ptr, a2=payload_len) -> value=payload_len`
- `IpcRecv(a0=header_out_ptr, a1=payload_out_ptr, a2=payload_capacity) -> value=payload_len`
- `GfxSubmitScene(a0=top_color, a1=bottom_color, a2=dock_color, a3=dock_height) -> value=0`
- `GfxPresent() -> value=0`
- `InputSubscribe(a0=enable_bool) -> value=0`
- `InputRead() -> value=gesture_action_u8`

`ProcSpawn` spawn argument contract:

- `0`: clone currently running task image.
- `1`: spawn `AppShellExecutable` boot module.
- `2`: spawn `AppSettingsExecutable` boot module.
- `3`: spawn `AppFilesExecutable` boot module.

## Number map

| Subsystem | Symbol | Number |
|---|---|---:|
| Process | `ProcSpawn` | `0x0001` |
| Process | `ProcExit` | `0x0002` |
| Process | `ProcWait` | `0x0003` |
| VM | `VmMap` | `0x0101` |
| VM | `VmUnmap` | `0x0102` |
| FS | `FsOpen` | `0x0201` |
| FS | `FsRead` | `0x0202` |
| FS | `FsWrite` | `0x0203` |
| FS | `FsClose` | `0x0204` |
| NET | `NetSocket` | `0x0301` |
| NET | `NetConnect` | `0x0302` |
| NET | `NetSend` | `0x0303` |
| NET | `NetRecv` | `0x0304` |
| IPC | `IpcSend` | `0x0401` |
| IPC | `IpcRecv` | `0x0402` |
| GFX | `GfxSubmitScene` | `0x0501` |
| GFX | `GfxPresent` | `0x0502` |
| INPUT | `InputSubscribe` | `0x0601` |
| INPUT | `InputRead` | `0x0602` |

## Compatibility rules

1. Existing syscall numbers are immutable once released.
2. New syscalls append to subsystem ranges.
3. ABI major version increments only on incompatible changes.
