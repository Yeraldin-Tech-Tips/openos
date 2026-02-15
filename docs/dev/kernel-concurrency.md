# Kernel concurrency model

## Primitive

Kernel mutable globals are synchronized with `sync::IrqSafeLock<T>`.

- Lock acquisition disables interrupts (`cli`) and restores the previous interrupt-enable state on drop.
- The lock is a spin lock and is intended for short critical sections only.
- Do not perform serial logging or device I/O while holding the lock whenever possible.

## Current protected state

- `fs`: open file table.
- `net`: socket table.
- `ipc`: IPC message queue.
- `input`: gesture queue, modifier state, and mouse packet assembly.
- `lifecycle`: app registry and launch history.
- `sched`: task/exit-event table accesses are serialized under scheduler state lock.

## Lock ordering

When multiple locks are needed, acquire in this order:

1. `sched` lock
2. `lifecycle` lock
3. `fs` lock
4. `net` lock
5. `ipc` lock
6. `input` lock

Rules:

- Never acquire an earlier lock while holding a later lock.
- Prefer extracting required state under lock, releasing lock, then calling out to other subsystems.
- Keep lock hold times bounded and deterministic.
