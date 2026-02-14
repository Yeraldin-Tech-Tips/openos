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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

        if self.rollback_checkpoints.is_empty() {
            errors.push("rollback_checkpoints must not be empty".to_string());
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
