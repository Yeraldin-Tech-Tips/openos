# Installer Plan Contract v1

Installer emits an `InstallPlan` JSON document before execution.

## Required fields

- `target_disk`
- `efi_partition`
- `root_partition`
- `filesystem`
- `boot_entry_policy`
- `partition_action`
- `rollback_checkpoints`

## Invariants

1. `filesystem` must be `ext4` in v1.
2. `efi_partition` must be FAT32 and mounted under `/boot/efi` during install.
3. `rollback_checkpoints` must include steps before partition, format, and boot entry writes.
