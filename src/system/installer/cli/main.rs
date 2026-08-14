//! Permanent first-class MattOS CLI installer.

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use mattos_installer::{
    EncryptionPolicy, Filesystem, GuidedEfi, InstallPlan, InstalledProfile, PLAN_VERSION,
    PartitionAction, PartitionOperation, RootCredentialPolicy, RootFilesystem, StoragePlan, engine,
    execute, render_plan,
};
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "mattos-install",
    about = "Install MattOS to an explicit target disk"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Interactively create and execute a MattOS install plan.
    Guided,
    /// Validate and display a plan without changing disks.
    Plan { plan: PathBuf },
    /// Execute a validated plan after an explicit destructive acknowledgement.
    Install {
        plan: PathBuf,
        #[arg(long)]
        yes_really_erase: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Guided => guided_install(),
        Command::Plan { plan } => {
            print!("{}", render_plan(&InstallPlan::read(&plan)?)?);
            Ok(())
        }
        Command::Install {
            plan,
            yes_really_erase,
        } => {
            let plan = InstallPlan::read(&plan)?;
            print!("{}", render_plan(&plan)?);
            if !yes_really_erase {
                bail!("dry-run only; pass --yes-really-erase to execute the displayed plan");
            }
            execute(&plan)?;
            println!("MattOS installation completed successfully");
            Ok(())
        }
    }
}

fn guided_install() -> Result<()> {
    println!("MattOS guided installer (UEFI/GPT/Btrfs)\nNo disk is selected automatically.");
    let candidates = engine::discover_install_disks()?;
    for candidate in &candidates {
        println!(
            "  {}  {:.1} GiB  {}",
            candidate.device.display(),
            candidate.size_bytes as f64 / 1_073_741_824.0,
            candidate.model
        );
    }
    let profile = match prompt("Installed profile (cli/desktop)")?.as_str() {
        "cli" => InstalledProfile::Cli,
        "desktop" => InstalledProfile::Desktop,
        _ => bail!("profile must be cli or desktop"),
    };
    let mut user_password = prompt_password("User password")?.into_bytes();
    let confirmation = prompt_password("Confirm user password")?;
    if String::from_utf8_lossy(&user_password) != confirmation {
        bail!("password confirmation does not match");
    }
    let user_hash = engine::hash_password_secure(&mut user_password)?;
    let separate_root = prompt_yes_no("Use a different root password", false)?;
    let root_credential = if separate_root {
        let mut password = prompt_password("Root password")?.into_bytes();
        if String::from_utf8_lossy(&password) != prompt_password("Confirm root password")? {
            bail!("root password confirmation does not match");
        }
        RootCredentialPolicy::SeparatePasswordHash(engine::hash_password_secure(&mut password)?)
    } else {
        RootCredentialPolicy::SameAsUser
    };
    let target_disk: PathBuf = prompt("Exact target disk")?.into();
    println!("Discovered partitions on eligible disks:");
    for disk in engine::discover_install_disks()? {
        for partition in engine::discover_partitions(&disk.device)? {
            println!(
                "  {}  parent={}  {:.1} GiB  filesystem={}  type={}  ESP={}  existing_mount_roles={}",
                partition.device.display(),
                partition.parent_disk.display(),
                partition.size_bytes as f64 / 1_073_741_824.0,
                partition.filesystem.as_deref().unwrap_or("unformatted"),
                partition.partition_type.as_deref().unwrap_or("unknown"),
                partition.is_esp,
                if partition.mount_points.is_empty() {
                    "none".into()
                } else {
                    partition
                        .mount_points
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                }
            );
        }
    }
    let storage = prompt_storage(&target_disk)?;
    let plan = InstallPlan {
        version: PLAN_VERSION,
        target_disk,
        storage,
        installed_profile: profile,
        full_name: prompt("Full name")?,
        username: prompt("Username")?,
        hostname: prompt("Computer name")?,
        password_hash: Some(user_hash),
        administrator: prompt_yes_no("Administrator", true)?,
        automatic_login: prompt_yes_no("Log in automatically", false)?,
        root_credential,
        locale: {
            let value = prompt("Locale [en_US.UTF-8]")?;
            if value.is_empty() {
                "en_US.UTF-8".into()
            } else {
                value
            }
        },
        keyboard_layout: {
            let value = prompt("Keyboard layout [us]")?;
            if value.is_empty() { "us".into() } else { value }
        },
        keyboard_variant: prompt("Keyboard variant [default]")?,
        timezone: {
            let value = prompt("Timezone [Etc/UTC]")?;
            if value.is_empty() {
                "Etc/UTC".into()
            } else {
                value
            }
        },
        test_autologin: false,
    };
    print!("{}", render_plan(&plan)?);
    if prompt("Type ERASE to continue")? != "ERASE" {
        bail!("installation cancelled");
    }
    execute(&plan)
}

fn prompt_storage(target_disk: &PathBuf) -> Result<StoragePlan> {
    match prompt("Storage mode [guided/manual, default guided]")?.as_str() {
        "" | "guided" => {
            let filesystem =
                match prompt("Root filesystem [btrfs/ext4, default btrfs Recommended]")?.as_str() {
                    "" | "btrfs" => RootFilesystem::Btrfs,
                    "ext4" => RootFilesystem::Ext4,
                    _ => bail!("filesystem must be btrfs or ext4"),
                };
            let reuse = prompt("Existing EFI partition to reuse [blank creates a new ESP]")?;
            let efi = if reuse.is_empty() {
                GuidedEfi::Create
            } else {
                GuidedEfi::Reuse {
                    device: reuse.into(),
                    format: prompt_yes_no("Format the reused EFI partition", false)?,
                }
            };
            Ok(StoragePlan::GuidedWholeDisk { filesystem, efi })
        }
        "manual" => {
            let mut partitions = Vec::new();
            println!(
                "Enter one explicit operation per partition. Every mounted role must be unique; / and /boot/efi are required."
            );
            while prompt_yes_no("Add storage operation", partitions.is_empty())? {
                let action = match prompt("Action (create/delete/preserve/reuse/format)")?.as_str()
                {
                    "create" => PartitionAction::Create,
                    "delete" => PartitionAction::Delete,
                    "preserve" => PartitionAction::Preserve,
                    "reuse" => PartitionAction::Reuse,
                    "format" => PartitionAction::Format,
                    _ => bail!("unsupported storage action"),
                };
                let device: PathBuf = prompt("Partition device")?.into();
                let filesystem =
                    if matches!(action, PartitionAction::Create | PartitionAction::Format) {
                        Some(match prompt("Filesystem (btrfs/ext4/fat32)")?.as_str() {
                            "btrfs" => Filesystem::Btrfs,
                            "ext4" => Filesystem::Ext4,
                            "fat32" => Filesystem::Fat32,
                            _ => bail!("unsupported filesystem"),
                        })
                    } else {
                        None
                    };
                let mount = prompt("Mount point [blank, /, /home, /boot/efi]")?;
                let (partition_number, start_mib, size_mib) = if action == PartitionAction::Create {
                    (
                        Some(prompt("GPT partition number")?.parse()?),
                        Some(prompt("Start MiB")?.parse()?),
                        Some(prompt("Size MiB")?.parse()?),
                    )
                } else {
                    (None, None, None)
                };
                partitions.push(PartitionOperation {
                    device,
                    action,
                    encryption: EncryptionPolicy::None,
                    filesystem,
                    mount_point: (!mount.is_empty()).then_some(mount),
                    partition_number,
                    start_mib,
                    size_mib,
                });
            }
            let storage = StoragePlan::Manual { partitions };
            storage.validate(target_disk)?;
            Ok(storage)
        }
        _ => bail!("storage mode must be guided or manual"),
    }
}

fn prompt_yes_no(label: &str, default: bool) -> Result<bool> {
    let value = prompt(&format!(
        "{label} [{}]",
        if default { "Y/n" } else { "y/N" }
    ))?;
    Ok(if value.is_empty() {
        default
    } else {
        matches!(value.as_str(), "y" | "Y" | "yes" | "YES")
    })
}

fn prompt_password(label: &str) -> Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    unsafe {
        let mut term: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut term) != 0 {
            bail!("cannot read terminal settings");
        }
        let saved = term;
        term.c_lflag &= !libc::ECHO;
        if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &term) != 0 {
            bail!("cannot disable terminal echo");
        }
        let mut value = String::new();
        let read = io::stdin().read_line(&mut value);
        let _ = libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &saved);
        println!();
        read?;
        Ok(value.trim().to_string())
    }
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    Ok(value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_exposes_permanent_guided_plan_and_install_modes() {
        use clap::CommandFactory;
        let command = Cli::command();
        let names = command
            .get_subcommands()
            .map(|item| item.get_name())
            .collect::<Vec<_>>();
        assert_eq!(names, ["guided", "plan", "install"]);
    }

    #[test]
    fn guided_cli_uses_shared_hardened_disk_discovery() {
        let source = include_str!("main.rs");
        assert!(source.contains("engine::discover_install_disks()"));
        assert!(!source.contains("read_dir(\"/sys/class/block\")"));
    }
}
