use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BootEntryPolicy {
    CreateAndKeepFallback,
    ReplaceOpenOSEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartitionAction {
    UseExisting,
    ShrinkAndCreate,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollbackCheckpoint {
    BeforePartitionChange,
    BeforeFilesystemFormat,
    BeforeBootEntryWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallPlan {
    pub target_disk: String,
    pub efi_partition: String,
    pub root_partition: String,
    pub filesystem: String,
    pub boot_entry_policy: BootEntryPolicy,
    pub partition_action: PartitionAction,
    pub rollback_checkpoints: Vec<RollbackCheckpoint>,
}

impl InstallPlan {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.target_disk.is_empty() {
            errors.push("target_disk is required".to_string());
        }
        if self.efi_partition.is_empty() {
            errors.push("efi_partition is required".to_string());
        }
        if self.root_partition.is_empty() {
            errors.push("root_partition is required".to_string());
        }
        if self.filesystem != "ext4" {
            errors.push("filesystem must be ext4 in v1".to_string());
        }

        if !self
            .rollback_checkpoints
            .contains(&RollbackCheckpoint::BeforePartitionChange)
        {
            errors.push("rollback_checkpoints must include BeforePartitionChange".to_string());
        }
        if !self
            .rollback_checkpoints
            .contains(&RollbackCheckpoint::BeforeFilesystemFormat)
        {
            errors.push("rollback_checkpoints must include BeforeFilesystemFormat".to_string());
        }
        if !self
            .rollback_checkpoints
            .contains(&RollbackCheckpoint::BeforeBootEntryWrite)
        {
            errors.push("rollback_checkpoints must include BeforeBootEntryWrite".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_plan() -> InstallPlan {
        InstallPlan {
            target_disk: "/dev/nvme0n1".to_string(),
            efi_partition: "/dev/nvme0n1p1".to_string(),
            root_partition: "/dev/nvme0n1p5".to_string(),
            filesystem: "ext4".to_string(),
            boot_entry_policy: BootEntryPolicy::CreateAndKeepFallback,
            partition_action: PartitionAction::UseExisting,
            rollback_checkpoints: vec![
                RollbackCheckpoint::BeforePartitionChange,
                RollbackCheckpoint::BeforeFilesystemFormat,
                RollbackCheckpoint::BeforeBootEntryWrite,
            ],
        }
    }

    // --- Validation: valid plans ---

    #[test]
    fn valid_plan_passes_validation() {
        assert!(valid_plan().validate().is_ok());
    }

    #[test]
    fn valid_plan_with_replace_policy() {
        let mut plan = valid_plan();
        plan.boot_entry_policy = BootEntryPolicy::ReplaceOpenOSEntry;
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn valid_plan_with_shrink_and_create() {
        let mut plan = valid_plan();
        plan.partition_action = PartitionAction::ShrinkAndCreate;
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn plan_with_single_checkpoint_fails_validation() {
        let mut plan = valid_plan();
        plan.rollback_checkpoints = vec![RollbackCheckpoint::BeforePartitionChange];
        let errors = plan.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("BeforeFilesystemFormat")));
        assert!(errors.iter().any(|e| e.contains("BeforeBootEntryWrite")));
    }

    // --- Validation: individual field errors ---

    #[test]
    fn empty_target_disk_fails() {
        let mut plan = valid_plan();
        plan.target_disk = String::new();
        let errors = plan.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("target_disk")));
    }

    #[test]
    fn empty_efi_partition_fails() {
        let mut plan = valid_plan();
        plan.efi_partition = String::new();
        let errors = plan.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("efi_partition")));
    }

    #[test]
    fn empty_root_partition_fails() {
        let mut plan = valid_plan();
        plan.root_partition = String::new();
        let errors = plan.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("root_partition")));
    }

    #[test]
    fn wrong_filesystem_fails() {
        let mut plan = valid_plan();
        plan.filesystem = "btrfs".to_string();
        let errors = plan.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("ext4")));
    }

    #[test]
    fn missing_before_partition_change_checkpoint_fails() {
        let mut plan = valid_plan();
        plan.rollback_checkpoints = vec![
            RollbackCheckpoint::BeforeFilesystemFormat,
            RollbackCheckpoint::BeforeBootEntryWrite,
        ];
        let errors = plan.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("BeforePartitionChange")));
    }

    #[test]
    fn missing_before_filesystem_format_checkpoint_fails() {
        let mut plan = valid_plan();
        plan.rollback_checkpoints = vec![
            RollbackCheckpoint::BeforePartitionChange,
            RollbackCheckpoint::BeforeBootEntryWrite,
        ];
        let errors = plan.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("BeforeFilesystemFormat")));
    }

    #[test]
    fn missing_before_boot_entry_write_checkpoint_fails() {
        let mut plan = valid_plan();
        plan.rollback_checkpoints = vec![
            RollbackCheckpoint::BeforePartitionChange,
            RollbackCheckpoint::BeforeFilesystemFormat,
        ];
        let errors = plan.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("BeforeBootEntryWrite")));
    }

    // --- Validation: multiple errors at once ---

    #[test]
    fn multiple_errors_reported_together() {
        let plan = InstallPlan {
            target_disk: String::new(),
            efi_partition: String::new(),
            root_partition: String::new(),
            filesystem: "xfs".to_string(),
            boot_entry_policy: BootEntryPolicy::CreateAndKeepFallback,
            partition_action: PartitionAction::UseExisting,
            rollback_checkpoints: vec![],
        };
        let errors = plan.validate().unwrap_err();
        assert_eq!(errors.len(), 7, "expected 7 errors: {errors:?}");
    }

    // --- JSON serialization ---

    #[test]
    fn to_json_pretty_produces_valid_json() {
        let plan = valid_plan();
        let json = plan.to_json_pretty();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("must be valid JSON");
        assert!(parsed.is_object());
    }

    #[test]
    fn json_round_trip() {
        let plan = valid_plan();
        let json = plan.to_json_pretty();
        let restored: InstallPlan = serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(restored.target_disk, plan.target_disk);
        assert_eq!(restored.efi_partition, plan.efi_partition);
        assert_eq!(restored.root_partition, plan.root_partition);
        assert_eq!(restored.filesystem, plan.filesystem);
        assert_eq!(
            restored.rollback_checkpoints.len(),
            plan.rollback_checkpoints.len()
        );
    }

    #[test]
    fn json_contains_expected_fields() {
        let plan = valid_plan();
        let json = plan.to_json_pretty();
        assert!(json.contains("target_disk"));
        assert!(json.contains("efi_partition"));
        assert!(json.contains("root_partition"));
        assert!(json.contains("filesystem"));
        assert!(json.contains("boot_entry_policy"));
        assert!(json.contains("partition_action"));
        assert!(json.contains("rollback_checkpoints"));
    }

    #[test]
    fn json_preserves_disk_paths() {
        let plan = valid_plan();
        let json = plan.to_json_pretty();
        assert!(json.contains("/dev/nvme0n1"));
        assert!(json.contains("/dev/nvme0n1p1"));
        assert!(json.contains("/dev/nvme0n1p5"));
    }

    // --- Enum serialization ---

    #[test]
    fn boot_entry_policy_serialization() {
        let mut plan = valid_plan();
        plan.boot_entry_policy = BootEntryPolicy::ReplaceOpenOSEntry;
        let json = plan.to_json_pretty();
        assert!(json.contains("ReplaceOpenOSEntry"));
    }

    #[test]
    fn partition_action_serialization() {
        let mut plan = valid_plan();
        plan.partition_action = PartitionAction::ShrinkAndCreate;
        let json = plan.to_json_pretty();
        assert!(json.contains("ShrinkAndCreate"));
    }

    #[test]
    fn rollback_checkpoints_serialization() {
        let plan = valid_plan();
        let json = plan.to_json_pretty();
        assert!(json.contains("BeforePartitionChange"));
        assert!(json.contains("BeforeFilesystemFormat"));
        assert!(json.contains("BeforeBootEntryWrite"));
    }
}
