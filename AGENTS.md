# Repository Guidelines

## Project Structure & Module Organization
- `boot/efi-loader/`: UEFI loader and boot handoff.
- `kernel/`: monolithic Rust kernel (`arch`, `mm`, `sched`, `syscall`, `fs`, `ui`, `drivers`).
- `shared/abi/`: stable ABI contracts shared by kernel and userspace.
- `userspace/`: syscall library, PID1/init, shell, apps, and per-app payload crates.
- `installer/gui/`: host-side installer planning and GUI logic.
- `tools/`: build/image/signing/dev scripts. Build outputs go to `out/`; intermediate targets to `.build/`.
- `docs/`, `ci/`, `specs/`: technical contracts, checklists, and hardware profile notes.

## Build, Test, and Development Commands
- `./tools/image/install-deps-ubuntu.sh`: install required Ubuntu packages.
- `make build`: build loader, kernel, userspace, and staged artifacts (`./tools/image/build.sh`).
- `make qemu`: run headless boot validation in QEMU (`./tools/image/run_qemu.sh`).
- `./tools/image/run_qemu_ui.sh`: run with SDL UI output.
- `make image` / `make usb-image`: create live ISO or raw USB image.
- `make check`: run formatting, clippy, and tests in one pass.
- `./tools/dev/parallel-lanes.sh`: quick multi-lane contract checks during parallel work.

## Coding Style & Naming Conventions
- Rust 2021 workspace with nightly toolchain (`rust-toolchain.toml`).
- Format with `cargo fmt --all`; CI enforces `cargo fmt --all -- --check`.
- Run clippy with warnings denied before submitting changes.
- Rust naming defaults: `snake_case` for modules/functions, `CamelCase` for types/traits, `kebab-case` for crate names.
- For ABI-crossing data, keep layout explicit and stable (`#[repr(C)]`, fixed enum reprs).

## Testing Guidelines
- Standard unit tests are primarily in `shared/abi` and `installer/gui` (`make test`).
- Add/expand `#[test]` coverage when changing ABI numbers, serialization, planner logic, or parsing.
- For kernel/payload changes, include QEMU boot evidence (serial log summary) in your PR.
- Use `ci/hardware-smoke-checklist.md` for hardware-oriented validation steps when relevant.

## Commit & Pull Request Guidelines
- Use concise, imperative commit subjects; scope prefixes are encouraged (for example: `kernel:`, `docs:`, `sched:`, `feat:`).
- Keep each commit focused; avoid mixing refactors with behavior changes.
- PRs should include a short problem statement, approach summary, linked issue/task, and validation notes (`make check`, QEMU run, or both).
- Attach screenshots or log snippets for visible UI/boot/installer behavior changes.
