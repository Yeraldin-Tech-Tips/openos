# Parallel Workstream Workflow

Use `tools/dev/parallel-lanes.sh` to run four independent lanes concurrently:

1. `boot`: shell-level validation of image/signing scripts.
2. `kernel`: ABI + kernel contract scan.
3. `shell`: gesture and input mapping scan.
4. `installer`: install-plan contract scan.

This does not replace full compile/test, but it keeps integration drift visible while multiple contributors work on separate lanes.
