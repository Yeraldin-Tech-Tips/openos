mod plan;

use std::env;
use std::fs;

use plan::{BootEntryPolicy, InstallPlan, PartitionAction, RollbackCheckpoint};

fn main() {
    let (target_disk, efi_partition, root_partition, output_path) = parse_args();

    let plan = InstallPlan {
        target_disk,
        efi_partition,
        root_partition,
        filesystem: "ext4".to_string(),
        boot_entry_policy: BootEntryPolicy::CreateAndKeepFallback,
        partition_action: PartitionAction::UseExisting,
        rollback_checkpoints: vec![
            RollbackCheckpoint::BeforePartitionChange,
            RollbackCheckpoint::BeforeFilesystemFormat,
            RollbackCheckpoint::BeforeBootEntryWrite,
        ],
    };

    if let Err(errors) = plan.validate() {
        eprintln!("Invalid install plan:");
        for err in errors {
            eprintln!("- {err}");
        }
        std::process::exit(2);
    }

    let plan_json = plan.to_json_pretty();

    println!("OpenOS Installer GUI skeleton");
    println!("Install plan preview:\n{plan_json}");

    if let Some(path) = output_path {
        if let Err(err) = fs::write(&path, &plan_json) {
            eprintln!("Failed to write install plan to {path}: {err}");
            std::process::exit(3);
        }
        println!("Install plan exported to {path}");
    }
}

fn parse_args() -> (String, String, String, Option<String>) {
    let mut target_disk = "/dev/nvme0n1".to_string();
    let mut efi_partition = "/dev/nvme0n1p1".to_string();
    let mut root_partition = "/dev/nvme0n1p5".to_string();
    let mut output_path = None;

    let args: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--disk" if i + 1 < args.len() => {
                target_disk = args[i + 1].clone();
                i += 2;
            }
            "--efi" if i + 1 < args.len() => {
                efi_partition = args[i + 1].clone();
                i += 2;
            }
            "--root" if i + 1 < args.len() => {
                root_partition = args[i + 1].clone();
                i += 2;
            }
            "--output" if i + 1 < args.len() => {
                output_path = Some(args[i + 1].clone());
                i += 2;
            }
            "--help" | "-h" => {
                print_help_and_exit(0);
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_help_and_exit(1);
            }
        }
    }

    (target_disk, efi_partition, root_partition, output_path)
}

fn print_help_and_exit(code: i32) -> ! {
    println!("OpenOS Installer GUI skeleton CLI");
    println!("Usage: openos-installer-gui [--disk /dev/nvme0n1] [--efi /dev/nvme0n1p1] [--root /dev/nvme0n1p5] [--output plan.json]");
    std::process::exit(code);
}
