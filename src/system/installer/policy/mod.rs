//! Authoritative MattOS installation policy.

use crate::engine::{self, InstallPartition, MountStack};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub const PLAN_VERSION: u32 = 5;
pub const MINIMUM_DISK_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const EFI_MIB: u64 = 512;
pub const LIVE_SOURCE: &str = "/run/mattos/lower";
pub const TARGET_ROOT: &str = "/run/mattos-installer/target";
const INSTALLED_APT_POLICY: &str = "/usr/share/mattos/apt/installed";
pub const BTRFS_MOUNT_OPTIONS: &str = "compress=zstd:3,noatime";
pub const BTRFS_SUBVOLUMES: &[(&str, &str)] = &[
    ("@", "/"),
    ("@home", "/home"),
    ("@snapshots", "/.snapshots"),
];

/// Stable, presentation-neutral installation phases.  Frontends must not
/// infer progress from command output: both the CLI and COSMIC installer
/// consume these events from the one policy implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallStage {
    Preparing,
    Partitioning,
    Formatting,
    CreatingSubvolumes,
    DeployingSystem,
    ConfiguringSystem,
    CreatingUser,
    InstallingGrub,
    Finalizing,
    Complete,
}

impl InstallStage {
    /// The complete, ordered set of stages emitted by the installer policy.
    /// Presentation layers use this rather than carrying their own stage list.
    pub const ALL: [Self; INSTALL_STAGE_COUNT] = [
        Self::Preparing,
        Self::Partitioning,
        Self::Formatting,
        Self::CreatingSubvolumes,
        Self::DeployingSystem,
        Self::ConfiguringSystem,
        Self::CreatingUser,
        Self::InstallingGrub,
        Self::Finalizing,
        Self::Complete,
    ];

    /// Stable user-facing name shared by installer frontends.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Preparing => "Preparing",
            Self::Partitioning => "Partitioning",
            Self::Formatting => "Formatting",
            Self::CreatingSubvolumes => "Preparing filesystems",
            Self::DeployingSystem => "Deploying system",
            Self::ConfiguringSystem => "Configuring system",
            Self::CreatingUser => "Creating user",
            Self::InstallingGrub => "Installing GRUB",
            Self::Finalizing => "Finalizing",
            Self::Complete => "Complete",
        }
    }
}

pub const INSTALL_STAGE_COUNT: usize = 10;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallProgress {
    pub stage: InstallStage,
    pub completed_stages: usize,
    pub total_stages: usize,
    pub detail: String,
}

impl InstallProgress {
    fn new(stage: InstallStage, completed_stages: usize, detail: impl Into<String>) -> Self {
        Self {
            stage,
            completed_stages,
            total_stages: INSTALL_STAGE_COUNT,
            detail: detail.into(),
        }
    }

    /// Overall determinate progress, safely bounded for presentation.
    pub fn fraction(&self) -> f32 {
        if self.total_stages == 0 {
            return 0.0;
        }
        (self.completed_stages.min(self.total_stages) as f32) / self.total_stages as f32
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StorageIdentity {
    root_uuid: String,
    root_partuuid: String,
    efi_partuuid: String,
    home_partuuid: Option<String>,
    home_filesystem: Option<Filesystem>,
    root_filesystem: Filesystem,
}

#[derive(Clone, Debug)]
struct ResolvedStorage {
    root: PathBuf,
    root_filesystem: Filesystem,
    home: Option<(PathBuf, Filesystem)>,
    efi: PathBuf,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstalledProfile {
    Cli,
    Desktop,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "mode", content = "password_hash")]
pub enum RootCredentialPolicy {
    SameAsUser,
    SeparatePasswordHash(String),
}

/// Explicit storage policy.  Future encrypted volumes can be introduced as a
/// new operation without changing the mount-assignment representation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "mode", deny_unknown_fields)]
pub enum StoragePlan {
    GuidedWholeDisk {
        filesystem: RootFilesystem,
        efi: GuidedEfi,
    },
    Manual {
        partitions: Vec<PartitionOperation>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RootFilesystem {
    Btrfs,
    Ext4,
}

impl RootFilesystem {
    fn filesystem(self) -> Filesystem {
        match self {
            Self::Btrfs => Filesystem::Btrfs,
            Self::Ext4 => Filesystem::Ext4,
        }
    }
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Btrfs => "Btrfs (Recommended)",
            Self::Ext4 => "ext4",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "policy", deny_unknown_fields)]
pub enum GuidedEfi {
    Create,
    Reuse { device: PathBuf, format: bool },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Filesystem {
    Btrfs,
    Ext4,
    Fat32,
}

impl Filesystem {
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Btrfs => "Btrfs",
            Self::Ext4 => "ext4",
            Self::Fat32 => "FAT32",
        }
    }
    fn lsblk_names(self) -> &'static [&'static str] {
        match self {
            Self::Btrfs => &["btrfs"],
            Self::Ext4 => &["ext4"],
            Self::Fat32 => &["vfat", "fat", "fat32"],
        }
    }

    const fn mount_name(self) -> &'static str {
        match self {
            Self::Btrfs => "btrfs",
            Self::Ext4 => "ext4",
            Self::Fat32 => "vfat",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionOperation {
    pub device: PathBuf,
    pub action: PartitionAction,
    /// Filesystem content is currently plain. A future LUKS variant can wrap
    /// the same operation and mount assignment without changing plan shape.
    #[serde(default)]
    pub encryption: EncryptionPolicy,
    pub filesystem: Option<Filesystem>,
    pub mount_point: Option<String>,
    #[serde(default)]
    pub partition_number: Option<u32>,
    #[serde(default)]
    pub start_mib: Option<u64>,
    #[serde(default)]
    pub size_mib: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EncryptionPolicy {
    #[default]
    None,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionAction {
    Create,
    Delete,
    Preserve,
    Reuse,
    Format,
}

impl PartitionAction {
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Create => "CREATE",
            Self::Delete => "DELETE",
            Self::Preserve => "PRESERVE",
            Self::Reuse => "REUSE",
            Self::Format => "FORMAT",
        }
    }
}

impl StoragePlan {
    pub fn guided_btrfs() -> Self {
        Self::GuidedWholeDisk {
            filesystem: RootFilesystem::Btrfs,
            efi: GuidedEfi::Create,
        }
    }
    pub fn guided_ext4() -> Self {
        Self::GuidedWholeDisk {
            filesystem: RootFilesystem::Ext4,
            efi: GuidedEfi::Create,
        }
    }
    pub fn validate(&self, disk: &Path) -> Result<()> {
        match self {
            Self::GuidedWholeDisk {
                efi: GuidedEfi::Reuse { device, .. },
                ..
            } => {
                if device == disk || !valid_device_path(device) {
                    bail!("reused EFI partition must be an explicit /dev partition")
                }
            }
            Self::GuidedWholeDisk { .. } => {}
            Self::Manual { partitions } => {
                let mut mounts = std::collections::BTreeSet::new();
                let mut devices = std::collections::BTreeMap::new();
                let mut extents = Vec::new();
                let mut root = false;
                let mut efi = false;
                for operation in partitions {
                    if !valid_device_path(&operation.device) {
                        bail!("manual storage operation requires an explicit /dev device")
                    }
                    let device_actions = devices.entry(&operation.device).or_insert_with(Vec::new);
                    if !device_actions.is_empty()
                        && (device_actions.len() != 1
                            || !matches!(
                                (device_actions[0], operation.action),
                                (PartitionAction::Delete, PartitionAction::Create)
                                    | (PartitionAction::Create, PartitionAction::Delete)
                            ))
                    {
                        bail!(
                            "only a DELETE plus CREATE replacement may share device {}",
                            operation.device.display()
                        )
                    }
                    device_actions.push(operation.action);
                    if let Some(mount) = &operation.mount_point {
                        if !matches!(mount.as_str(), "/" | "/home" | "/boot/efi") {
                            bail!("unsupported manual mount point {mount}")
                        }
                        if !mounts.insert(mount) {
                            bail!("storage plan assigns {mount} more than once")
                        }
                        root |= mount == "/";
                        efi |= mount == "/boot/efi";
                        if mount == "/boot/efi"
                            && operation
                                .filesystem
                                .is_some_and(|filesystem| filesystem != Filesystem::Fat32)
                        {
                            bail!("EFI must use FAT32")
                        }
                        if matches!(mount.as_str(), "/" | "/home")
                            && operation.filesystem == Some(Filesystem::Fat32)
                        {
                            bail!("{mount} does not support FAT32")
                        }
                    }
                    match operation.action {
                        PartitionAction::Delete => {
                            if operation.mount_point.is_some() || operation.filesystem.is_some() {
                                bail!("deleted partition cannot be formatted or mounted")
                            }
                        }
                        PartitionAction::Preserve => {
                            if operation.filesystem.is_some() || operation.mount_point.is_some() {
                                bail!(
                                    "preserved partition cannot be formatted or mounted; use reuse to mount it"
                                )
                            }
                        }
                        PartitionAction::Reuse => {
                            if operation.filesystem.is_some() {
                                bail!("reused partition must not request formatting")
                            }
                            if operation.mount_point.is_none() {
                                bail!("reused partition requires a mount point")
                            }
                        }
                        PartitionAction::Format => {
                            if operation.filesystem.is_none() {
                                bail!("formatted partition requires a filesystem")
                            }
                        }
                        PartitionAction::Create => {
                            if operation.filesystem.is_none()
                                || operation.partition_number.is_none()
                                || operation.start_mib.is_none()
                                || operation.size_mib.is_none_or(|size| size == 0)
                            {
                                bail!("created partition requires number, extent, and filesystem")
                            }
                            let start = operation.start_mib.unwrap();
                            let end = start
                                .checked_add(operation.size_mib.unwrap())
                                .ok_or_else(|| anyhow::anyhow!("partition extent overflows"))?;
                            if start < 1 {
                                bail!("created partition must start at or after 1 MiB")
                            }
                            if extents.iter().any(|(other_start, other_end)| {
                                start < *other_end && *other_start < end
                            }) {
                                bail!("created partition extents overlap")
                            }
                            extents.push((start, end));
                        }
                    }
                }
                if !root {
                    bail!("manual storage plan requires a root mount point")
                }
                if !efi {
                    bail!("manual storage plan requires an EFI mount point")
                }
            }
        }
        Ok(())
    }
}

fn valid_device_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with("/dev/") && !text.contains("..") && text.len() > 5
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallPlan {
    pub version: u32,
    pub target_disk: PathBuf,
    pub storage: StoragePlan,
    pub installed_profile: InstalledProfile,
    pub hostname: String,
    pub full_name: String,
    pub username: String,
    #[serde(default)]
    pub password_hash: Option<String>,
    pub administrator: bool,
    pub automatic_login: bool,
    pub root_credential: RootCredentialPolicy,
    pub locale: String,
    pub keyboard_layout: String,
    #[serde(default)]
    pub keyboard_variant: String,
    pub timezone: String,
    #[serde(default)]
    pub test_autologin: bool,
}

impl InstallPlan {
    pub fn read(path: &Path) -> Result<Self> {
        let body = fs::read_to_string(path)
            .with_context(|| format!("failed to read install plan {}", path.display()))?;
        let plan: Self = toml::from_str(&body)
            .with_context(|| format!("invalid install plan {}", path.display()))?;
        plan.validate_policy()?;
        Ok(plan)
    }

    pub fn validate_policy(&self) -> Result<()> {
        if self.version != PLAN_VERSION {
            bail!(
                "unsupported install plan version {}; expected {PLAN_VERSION}",
                self.version
            );
        }
        let disk = self.target_disk.to_string_lossy();
        if !disk.starts_with("/dev/") || disk.contains("..") {
            bail!("target_disk must be an explicit absolute /dev path");
        }
        self.storage.validate(&self.target_disk)?;
        validate_identifier(&self.hostname, "hostname")?;
        validate_identifier(&self.username, "username")?;
        if self.username == "root" {
            bail!("the installed user may not be root");
        }
        if self.full_name.trim().is_empty() || self.full_name.contains(['\n', ':']) {
            bail!("full_name must be a non-empty single-line GECOS value");
        }
        if let Some(hash) = &self.password_hash
            && (!hash.starts_with('$') || hash.contains(['\n', ':']))
        {
            bail!("password_hash must be a crypt-style hash without separators");
        }
        if let RootCredentialPolicy::SeparatePasswordHash(hash) = &self.root_credential
            && (!hash.starts_with('$') || hash.contains(['\n', ':']))
        {
            bail!("root password hash must be a crypt-style hash without separators");
        }
        if !locale_supported(&self.locale) {
            bail!(
                "unsupported locale {}; install image locale data does not provide it",
                self.locale
            );
        }
        if !xkb_layout_supported(&self.keyboard_layout, &self.keyboard_variant) {
            bail!(
                "unsupported keyboard layout/variant {}({})",
                self.keyboard_layout,
                self.keyboard_variant
            );
        }
        if !timezone_supported(&self.timezone) {
            bail!("unsupported timezone {}", self.timezone);
        }
        Ok(())
    }
}

fn runtime_path(relative: &str) -> PathBuf {
    let live = Path::new(LIVE_SOURCE).join(relative);
    if live.exists() {
        live
    } else {
        Path::new("/").join(relative)
    }
}

pub fn locale_supported(locale: &str) -> bool {
    let Some((name, encoding)) = locale.split_once('.') else {
        return false;
    };
    encoding.eq_ignore_ascii_case("UTF-8")
        && runtime_path(&format!("usr/share/i18n/locales/{name}")).is_file()
}

pub fn timezone_supported(timezone: &str) -> bool {
    !timezone.is_empty()
        && !timezone.starts_with('/')
        && !timezone.contains("..")
        && runtime_path(&format!("usr/share/zoneinfo/{timezone}")).is_file()
}

pub fn xkb_layout_supported(layout: &str, variant: &str) -> bool {
    crate::discover_keyboard_layouts().is_ok_and(|layouts| {
        layouts.iter().any(|candidate| {
            candidate.id == layout
                && candidate
                    .variants
                    .iter()
                    .any(|candidate| candidate.id == variant)
        })
    })
}

fn validate_identifier(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 63
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || value.starts_with('-')
        || value.ends_with('-')
    {
        bail!("{label} must use lowercase ASCII letters, digits, and interior hyphens");
    }
    Ok(())
}

pub fn render_plan(plan: &InstallPlan) -> Result<String> {
    plan.validate_policy()?;
    let storage = render_storage_plan(&plan.storage, &plan.target_disk)?;
    Ok(format!(
        "MattOS install plan\n  target: {}\n{storage}  profile: {:?}\n  locale: {}\n  keyboard: {} ({})\n  timezone: {}\n  hostname: {}\n  full name: {}\n  user: {}\n  account type: {}\n  automatic login: {}\n  root credential: {}\n",
        plan.target_disk.display(),
        plan.installed_profile,
        plan.locale,
        plan.keyboard_layout,
        if plan.keyboard_variant.is_empty() {
            "default"
        } else {
            &plan.keyboard_variant
        },
        plan.timezone,
        plan.hostname,
        plan.full_name,
        plan.username,
        if plan.administrator {
            "Administrator"
        } else {
            "Standard user"
        },
        if plan.automatic_login {
            "enabled"
        } else {
            "disabled"
        },
        match plan.root_credential {
            RootCredentialPolicy::SameAsUser => "same as user",
            RootCredentialPolicy::SeparatePasswordHash(_) => "separate password",
        }
    ))
}

pub fn render_storage_plan(storage: &StoragePlan, disk: &Path) -> Result<String> {
    storage.validate(disk)?;
    let mut output = String::from("  storage operations:\n");
    match storage {
        StoragePlan::GuidedWholeDisk { filesystem, efi } => {
            let reused = match efi {
                GuidedEfi::Reuse { device, .. } => Some(device),
                GuidedEfi::Create => None,
            };
            let discovered = engine::discover_partitions(disk).unwrap_or_default();
            if discovered.is_empty() {
                output.push_str(&format!(
                    "    DELETE (ERASED): all non-reused partitions on {}\n",
                    disk.display()
                ));
            } else {
                for partition in discovered
                    .iter()
                    .filter(|partition| reused != Some(&partition.device))
                {
                    output.push_str(&format!(
                        "    DELETE (ERASED): {} ({:.1} MiB, {})\n",
                        partition.device.display(),
                        partition.size_bytes as f64 / 1_048_576.0,
                        partition.filesystem.as_deref().unwrap_or("unformatted")
                    ));
                }
            }
            match efi {
                GuidedEfi::Create => output.push_str(&format!(
                    "    CREATE+FORMAT: {} — 512 MiB FAT32 mounted at /boot/efi\n",
                    engine::partition_path(disk, 1)?.display()
                )),
                GuidedEfi::Reuse { device, format } => output.push_str(&format!(
                    "    {}: {} as /boot/efi\n",
                    if *format {
                        "FORMAT FAT32"
                    } else {
                        "REUSE without formatting"
                    },
                    device.display()
                )),
            }
            let root_number = match reused.and_then(|device| {
                discovered
                    .iter()
                    .find(|partition| &partition.device == device)
            }) {
                Some(partition) if partition.parent_disk == disk && partition.number == 1 => 2,
                _ => 1 + u32::from(matches!(efi, GuidedEfi::Create)),
            };
            output.push_str(&format!(
                "    CREATE+FORMAT: {} — available guided space as {} mounted at /\n",
                engine::partition_path(disk, root_number)?.display(),
                filesystem.display_name(),
            ));
            if *filesystem == RootFilesystem::Btrfs {
                output.push_str("    CREATE: Btrfs subvolumes @, @home, @snapshots\n");
            }
        }
        StoragePlan::Manual { partitions } => {
            for operation in partitions {
                output.push_str(&format!(
                    "    {}: {}",
                    operation.action.display_name(),
                    operation.device.display()
                ));
                if let Some(filesystem) = operation.filesystem {
                    output.push_str(&format!(" as {}", filesystem.display_name()));
                }
                if let Some(mount) = &operation.mount_point {
                    output.push_str(&format!(" mounted at {mount}"));
                }
                if operation.action == PartitionAction::Create {
                    output.push_str(&format!(
                        " ({} MiB at {} MiB)",
                        operation.size_mib.unwrap(),
                        operation.start_mib.unwrap()
                    ));
                }
                output.push('\n');
            }
        }
    }
    Ok(output)
}

pub fn validate_target(plan: &InstallPlan) -> Result<()> {
    plan.validate_policy()?;
    resolve_storage(plan)
        .map(|_| ())
        .context("MattOS target safety policy rejected the storage plan")
}

fn partition_for(device: &Path, partitions: &[InstallPartition]) -> Option<InstallPartition> {
    partitions
        .iter()
        .find(|partition| partition.device == device)
        .cloned()
}

fn existing_filesystem(partition: &InstallPartition) -> Option<Filesystem> {
    let name = partition.filesystem.as_deref()?;
    [Filesystem::Btrfs, Filesystem::Ext4, Filesystem::Fat32]
        .into_iter()
        .find(|filesystem| filesystem.lsblk_names().contains(&name))
}

fn validate_esp(partition: &InstallPartition, require_fat: bool) -> Result<()> {
    if !partition.is_esp {
        bail!(
            "{} is not an EFI System Partition",
            partition.device.display()
        );
    }
    if require_fat && existing_filesystem(partition) != Some(Filesystem::Fat32) {
        bail!(
            "{} EFI System Partition is not FAT",
            partition.device.display()
        );
    }
    if partition.size_bytes < 100 * 1024 * 1024 {
        bail!(
            "{} EFI System Partition is smaller than 100 MiB",
            partition.device.display()
        );
    }
    if !partition.mount_points.is_empty() {
        bail!(
            "{} is mounted and cannot be reused safely",
            partition.device.display()
        );
    }
    Ok(())
}

fn target_disk_size(disk: &Path) -> Result<u64> {
    Ok(fs::read_to_string(
        Path::new("/sys/class/block")
            .join(
                disk.file_name()
                    .ok_or_else(|| anyhow::anyhow!("target has no name"))?,
            )
            .join("size"),
    )?
    .trim()
    .parse::<u64>()?
    .saturating_mul(512))
}

/// Choose the larger aligned region around a retained ESP. Values are sectors.
fn guided_root_geometry(disk_bytes: u64, efi: &InstallPartition) -> Result<(u64, u64)> {
    const ALIGN: u64 = 2048;
    const GPT_HEAD: u64 = ALIGN;
    const GPT_TAIL: u64 = 34;
    let disk_sectors = disk_bytes / 512;
    let efi_start = efi.start_bytes / 512;
    let efi_end = efi_start.saturating_add(efi.size_bytes / 512);
    let before_size = efi_start.saturating_sub(GPT_HEAD);
    let after_start = efi_end.saturating_add(ALIGN - 1) / ALIGN * ALIGN;
    let after_size = disk_sectors
        .saturating_sub(GPT_TAIL)
        .saturating_sub(after_start);
    let geometry = if before_size >= after_size {
        (GPT_HEAD, before_size)
    } else {
        (after_start, after_size)
    };
    if geometry.1.saturating_mul(512) < MINIMUM_DISK_BYTES {
        bail!(
            "retaining {} leaves less than 8 GiB for the guided root filesystem",
            efi.device.display()
        );
    }
    Ok(geometry)
}

fn resolve_storage(plan: &InstallPlan) -> Result<ResolvedStorage> {
    engine::validate_install_disk(&plan.target_disk, MINIMUM_DISK_BYTES, true)?;
    let selected = engine::discover_partitions(&plan.target_disk)?;
    match &plan.storage {
        StoragePlan::GuidedWholeDisk { filesystem, efi } => {
            let root_filesystem = filesystem.filesystem();
            match efi {
                GuidedEfi::Create => Ok(ResolvedStorage {
                    root: engine::partition_path(&plan.target_disk, 2)?,
                    root_filesystem,
                    home: None,
                    efi: engine::partition_path(&plan.target_disk, 1)?,
                }),
                GuidedEfi::Reuse { device, format } => {
                    let discovered = engine::discover_partitions(device)?;
                    let partition = partition_for(device, &discovered).ok_or_else(|| {
                        anyhow::anyhow!(
                            "reused EFI partition {} was not discovered",
                            device.display()
                        )
                    })?;
                    if partition.parent_disk != plan.target_disk {
                        engine::validate_install_disk(&partition.parent_disk, 0, false)
                            .context("validate the disk containing the reused EFI partition")?;
                    }
                    validate_esp(&partition, !format)?;
                    if partition.parent_disk == plan.target_disk {
                        guided_root_geometry(target_disk_size(&plan.target_disk)?, &partition)?;
                    }
                    let root_number =
                        if partition.parent_disk == plan.target_disk && partition.number == 1 {
                            2
                        } else {
                            1
                        };
                    Ok(ResolvedStorage {
                        root: engine::partition_path(&plan.target_disk, root_number)?,
                        root_filesystem,
                        home: None,
                        efi: device.clone(),
                    })
                }
            }
        }
        StoragePlan::Manual { partitions } => {
            for existing in &selected {
                if !partitions
                    .iter()
                    .any(|operation| operation.device == existing.device)
                {
                    bail!(
                        "manual storage plan must explicitly preserve, reuse, format, or delete {}",
                        existing.device.display()
                    );
                }
            }
            let deleted = partitions
                .iter()
                .filter(|operation| operation.action == PartitionAction::Delete)
                .map(|operation| operation.device.clone())
                .collect::<std::collections::BTreeSet<_>>();
            let mut occupied_numbers = selected
                .iter()
                .filter(|partition| !deleted.contains(&partition.device))
                .map(|partition| partition.number)
                .collect::<std::collections::BTreeSet<_>>();
            let mut creates = partitions
                .iter()
                .filter(|operation| operation.action == PartitionAction::Create)
                .collect::<Vec<_>>();
            creates.sort_by_key(|operation| operation.partition_number.unwrap());
            for operation in creates {
                let expected = (1..=128)
                    .find(|number| !occupied_numbers.contains(number))
                    .ok_or_else(|| anyhow::anyhow!("GPT has no free partition number"))?;
                if operation.partition_number != Some(expected) {
                    bail!(
                        "created partition {} must use next available GPT number {expected}",
                        operation.device.display()
                    );
                }
                occupied_numbers.insert(expected);
            }
            let disk_size = target_disk_size(&plan.target_disk)?;
            let mut root = None;
            let mut home = None;
            let mut efi = None;
            for operation in partitions {
                let existing = partition_for(&operation.device, &selected).or_else(|| {
                    engine::discover_partitions(&operation.device)
                        .ok()
                        .and_then(|items| partition_for(&operation.device, &items))
                });
                let filesystem = match operation.action {
                    PartitionAction::Create => {
                        if existing.is_some() && !deleted.contains(&operation.device) {
                            bail!(
                                "created partition {} already exists",
                                operation.device.display()
                            );
                        }
                        let expected = engine::partition_path(
                            &plan.target_disk,
                            operation.partition_number.unwrap(),
                        )?;
                        if expected != operation.device {
                            bail!(
                                "created partition path {} does not match partition number",
                                operation.device.display()
                            );
                        }
                        let start = operation.start_mib.unwrap().saturating_mul(1024 * 1024);
                        let end = start.saturating_add(
                            operation.size_mib.unwrap().saturating_mul(1024 * 1024),
                        );
                        if end > disk_size {
                            bail!(
                                "created partition {} extends beyond the target disk",
                                operation.device.display()
                            );
                        }
                        for current in &selected {
                            if deleted.contains(&current.device) {
                                continue;
                            }
                            let current_end =
                                current.start_bytes.saturating_add(current.size_bytes);
                            if start < current_end && current.start_bytes < end {
                                bail!(
                                    "created partition {} overlaps preserved {}",
                                    operation.device.display(),
                                    current.device.display()
                                );
                            }
                        }
                        operation.filesystem
                    }
                    PartitionAction::Delete => {
                        let current = existing.as_ref().ok_or_else(|| {
                            anyhow::anyhow!(
                                "deleted partition {} does not exist",
                                operation.device.display()
                            )
                        })?;
                        if current.parent_disk != plan.target_disk {
                            bail!("cannot delete partition outside selected disk")
                        }
                        None
                    }
                    PartitionAction::Format => {
                        let current = existing.as_ref().ok_or_else(|| {
                            anyhow::anyhow!(
                                "formatted partition {} does not exist",
                                operation.device.display()
                            )
                        })?;
                        if !current.mount_points.is_empty() {
                            bail!(
                                "formatted partition {} is mounted",
                                operation.device.display()
                            );
                        }
                        operation.filesystem
                    }
                    PartitionAction::Preserve => {
                        let current = existing.as_ref().ok_or_else(|| {
                            anyhow::anyhow!(
                                "preserved partition {} does not exist",
                                operation.device.display()
                            )
                        })?;
                        existing_filesystem(current)
                    }
                    PartitionAction::Reuse => {
                        let current = existing.as_ref().ok_or_else(|| {
                            anyhow::anyhow!(
                                "reused partition {} does not exist",
                                operation.device.display()
                            )
                        })?;
                        if !current.mount_points.is_empty() {
                            bail!("reused partition {} is mounted", operation.device.display());
                        }
                        existing_filesystem(current)
                    }
                };
                if let Some(mount) = operation.mount_point.as_deref() {
                    let filesystem = filesystem.ok_or_else(|| {
                        anyhow::anyhow!(
                            "{} has no supported filesystem",
                            operation.device.display()
                        )
                    })?;
                    match mount {
                        "/" | "/home" => {
                            if filesystem == Filesystem::Fat32 {
                                bail!("{mount} does not support FAT32");
                            }
                            if existing
                                .as_ref()
                                .is_some_and(|partition| partition.parent_disk != plan.target_disk)
                            {
                                bail!("{mount} must reside on the selected target disk");
                            }
                            if mount == "/" {
                                root = Some((operation.device.clone(), filesystem));
                            } else {
                                home = Some((operation.device.clone(), filesystem));
                            }
                        }
                        "/boot/efi" => {
                            if matches!(
                                operation.action,
                                PartitionAction::Reuse | PartitionAction::Format
                            ) {
                                let current =
                                    existing.as_ref().expect("existing EFI partition resolved");
                                if current.parent_disk != plan.target_disk {
                                    engine::validate_install_disk(&current.parent_disk, 0, false)
                                        .context(
                                        "validate the disk containing the reused EFI partition",
                                    )?;
                                }
                                validate_esp(current, operation.action == PartitionAction::Reuse)?;
                            }
                            if filesystem != Filesystem::Fat32 {
                                bail!("EFI mount must use FAT32")
                            }
                            efi = Some(operation.device.clone());
                        }
                        _ => unreachable!(),
                    }
                } else if operation.action == PartitionAction::Format
                    && existing
                        .as_ref()
                        .is_some_and(|partition| partition.parent_disk != plan.target_disk)
                {
                    bail!("cannot format an unassigned partition outside the selected target disk");
                }
            }
            let (root, root_filesystem) =
                root.ok_or_else(|| anyhow::anyhow!("manual storage plan has no root"))?;
            Ok(ResolvedStorage {
                root,
                root_filesystem,
                home,
                efi: efi
                    .ok_or_else(|| anyhow::anyhow!("manual storage plan has no EFI partition"))?,
            })
        }
    }
}

pub fn execute(plan: &InstallPlan) -> Result<()> {
    execute_with_progress(plan, |event| {
        eprintln!("mattos-install: {:?}: {}", event.stage, event.detail)
    })
}

pub fn execute_with_progress<F>(plan: &InstallPlan, mut progress: F) -> Result<()>
where
    F: FnMut(InstallProgress),
{
    progress(InstallProgress::new(
        InstallStage::Preparing,
        0,
        "Validating the selected target disk",
    ));
    plan.validate_policy()?;
    let storage = resolve_storage(plan)
        .context("validate complete storage plan before destructive operations")?;
    engine::require_tools(&[
        "sfdisk", "mkfs.fat", "mount", "umount", "cp", "blkid", "udevadm", "groupadd", "useradd",
        "usermod", "dpkg",
    ])?;
    let uses_btrfs = storage.root_filesystem == Filesystem::Btrfs
        || storage
            .home
            .as_ref()
            .is_some_and(|(_, filesystem)| *filesystem == Filesystem::Btrfs);
    let uses_ext4 = storage.root_filesystem == Filesystem::Ext4
        || storage
            .home
            .as_ref()
            .is_some_and(|(_, filesystem)| *filesystem == Filesystem::Ext4);
    if uses_btrfs {
        engine::require_tools(&["mkfs.btrfs", "btrfs"])?;
    }
    if uses_ext4 {
        engine::require_tools(&["mkfs.ext4"])?;
    }
    progress(InstallProgress::new(
        InstallStage::Partitioning,
        1,
        "Applying the validated partition operation plan",
    ));
    apply_partition_operations(plan, &storage)?;
    engine::run("udevadm", &["settle".as_ref()])?;
    verify_partition_results(plan)?;
    progress(InstallProgress::new(
        InstallStage::Formatting,
        2,
        "Formatting only partitions explicitly marked for formatting",
    ));
    apply_formats(plan, &storage)?;
    let target = Path::new(TARGET_ROOT);
    progress(InstallProgress::new(
        InstallStage::CreatingSubvolumes,
        3,
        "Preparing the selected root filesystem and mount layout",
    ));
    fs::create_dir_all(target)?;
    let mut mounts = MountStack::new();
    mount_storage(&storage, target, &mut mounts)?;

    progress(InstallProgress::new(
        InstallStage::DeployingSystem,
        4,
        "Copying the immutable live system to the target",
    ));
    let install_result = populate_target(plan, &storage, target, &mut progress);
    let sync_result = engine::run("sync", &[]);
    let unmount_result = mounts.unmount_all();
    install_result.and(sync_result).and(unmount_result)?;
    progress(InstallProgress::new(
        InstallStage::Finalizing,
        8,
        "Synchronizing and unmounting target filesystems",
    ));
    progress(InstallProgress::new(
        InstallStage::Complete,
        INSTALL_STAGE_COUNT,
        "Installation complete",
    ));
    Ok(())
}

fn verify_partition_results(plan: &InstallPlan) -> Result<()> {
    if let StoragePlan::Manual { partitions } = &plan.storage {
        for operation in partitions {
            let is_partition = fs::metadata(&operation.device)
                .is_ok_and(|metadata| metadata.file_type().is_block_device());
            match operation.action {
                PartitionAction::Create if !is_partition => {
                    bail!(
                        "created partition {} did not appear",
                        operation.device.display()
                    )
                }
                PartitionAction::Delete
                    if !partitions.iter().any(|candidate| {
                        candidate.device == operation.device
                            && candidate.action == PartitionAction::Create
                    }) && is_partition =>
                {
                    bail!(
                        "deleted partition {} still exists",
                        operation.device.display()
                    )
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn partition_type(filesystem: Filesystem) -> &'static str {
    if filesystem == Filesystem::Fat32 {
        "U"
    } else {
        "L"
    }
}

fn apply_partition_operations(plan: &InstallPlan, storage: &ResolvedStorage) -> Result<()> {
    match &plan.storage {
        StoragePlan::GuidedWholeDisk {
            efi: GuidedEfi::Create,
            ..
        } => {
            let layout = format!("label: gpt\n,{}M,U\n,,L\n", EFI_MIB);
            engine::command_with_input(
                "sfdisk",
                &[plan.target_disk.as_os_str()],
                layout.as_bytes(),
            )?;
        }
        StoragePlan::GuidedWholeDisk {
            efi: GuidedEfi::Reuse { device, .. },
            ..
        } => {
            if engine::discover_partitions(&plan.target_disk)?
                .iter()
                .any(|partition| partition.device == *device)
            {
                let efi = engine::discover_partitions(device)?
                    .into_iter()
                    .find(|partition| partition.device == *device)
                    .ok_or_else(|| anyhow::anyhow!("reused EFI disappeared"))?;
                let start = efi.start_bytes / 512;
                let size = efi.size_bytes / 512;
                let (root_start, root_size) =
                    guided_root_geometry(target_disk_size(&plan.target_disk)?, &efi)?;
                let layout = format!(
                    "label: gpt\nunit: sectors\n{} : start={start}, size={size}, type=U\n{} : start={root_start}, size={root_size}, type=L\n",
                    device.display(),
                    storage.root.display()
                );
                engine::command_with_input(
                    "sfdisk",
                    &[plan.target_disk.as_os_str()],
                    layout.as_bytes(),
                )?;
            } else {
                engine::command_with_input(
                    "sfdisk",
                    &[plan.target_disk.as_os_str()],
                    b"label: gpt\n,,L\n",
                )?;
            }
        }
        StoragePlan::Manual { partitions } => {
            let discovered = engine::discover_partitions(&plan.target_disk)?;
            for operation in partitions
                .iter()
                .filter(|operation| operation.action == PartitionAction::Delete)
            {
                let number = discovered
                    .iter()
                    .find(|partition| partition.device == operation.device)
                    .map(|partition| partition.number)
                    .ok_or_else(|| anyhow::anyhow!("deleted partition disappeared"))?
                    .to_string();
                engine::run(
                    "sfdisk",
                    &[
                        "--delete".as_ref(),
                        plan.target_disk.as_os_str(),
                        number.as_ref(),
                    ],
                )?;
            }
            for operation in partitions
                .iter()
                .filter(|operation| operation.action == PartitionAction::Create)
            {
                let line = format!(
                    "start={}MiB, size={}MiB, type={}\n",
                    operation.start_mib.unwrap(),
                    operation.size_mib.unwrap(),
                    partition_type(operation.filesystem.unwrap())
                );
                engine::command_with_input(
                    "sfdisk",
                    &["--append".as_ref(), plan.target_disk.as_os_str()],
                    line.as_bytes(),
                )?;
            }
        }
    }
    Ok(())
}

fn format_device(device: &Path, filesystem: Filesystem, efi_label: bool) -> Result<()> {
    match filesystem {
        Filesystem::Btrfs => engine::run(
            "mkfs.btrfs",
            &[
                "-f".as_ref(),
                "-L".as_ref(),
                "MattOS".as_ref(),
                device.as_os_str(),
            ],
        ),
        Filesystem::Ext4 => engine::run(
            "mkfs.ext4",
            &[
                "-F".as_ref(),
                "-L".as_ref(),
                "MattOS".as_ref(),
                device.as_os_str(),
            ],
        ),
        Filesystem::Fat32 => engine::run(
            "mkfs.fat",
            &[
                "-F".as_ref(),
                "32".as_ref(),
                "-n".as_ref(),
                if efi_label {
                    "MATTOS_EFI".as_ref()
                } else {
                    "MATTOS".as_ref()
                },
                device.as_os_str(),
            ],
        ),
    }
}

fn apply_formats(plan: &InstallPlan, storage: &ResolvedStorage) -> Result<()> {
    match &plan.storage {
        StoragePlan::GuidedWholeDisk { efi, .. } => {
            if matches!(
                efi,
                GuidedEfi::Create | GuidedEfi::Reuse { format: true, .. }
            ) {
                format_device(&storage.efi, Filesystem::Fat32, true)?;
            }
            format_device(&storage.root, storage.root_filesystem, false)
        }
        StoragePlan::Manual { partitions } => {
            for operation in partitions.iter().filter(|operation| {
                matches!(
                    operation.action,
                    PartitionAction::Create | PartitionAction::Format
                )
            }) {
                format_device(
                    &operation.device,
                    operation.filesystem.unwrap(),
                    operation.mount_point.as_deref() == Some("/boot/efi"),
                )?;
            }
            Ok(())
        }
    }
}

fn mount_storage(storage: &ResolvedStorage, target: &Path, mounts: &mut MountStack) -> Result<()> {
    if storage.root_filesystem == Filesystem::Btrfs {
        mounts.mount(&[storage.root.as_os_str(), target.as_os_str()], target)?;
        for subvolume in if storage.home.is_none() {
            vec!["@", "@home", "@snapshots"]
        } else {
            vec!["@", "@snapshots"]
        } {
            engine::run(
                "btrfs",
                &[
                    "subvolume".as_ref(),
                    "create".as_ref(),
                    target.join(subvolume).as_os_str(),
                ],
            )?;
        }
        mounts.unmount_all()?;
        let options = format!("subvol=@,{BTRFS_MOUNT_OPTIONS}");
        mounts.mount(
            &[
                "-o".as_ref(),
                options.as_ref(),
                storage.root.as_os_str(),
                target.as_os_str(),
            ],
            target,
        )?;
    } else {
        mounts.mount(&[storage.root.as_os_str(), target.as_os_str()], target)?;
    }
    for relative in ["home", ".snapshots", "boot/efi"] {
        fs::create_dir_all(target.join(relative))?;
    }
    if let Some((home, _)) = &storage.home {
        mounts.mount(
            &[home.as_os_str(), target.join("home").as_os_str()],
            &target.join("home"),
        )?;
    } else if storage.root_filesystem == Filesystem::Btrfs {
        for (subvolume, mountpoint) in [("@home", "home"), ("@snapshots", ".snapshots")] {
            let options = format!("subvol={subvolume},{BTRFS_MOUNT_OPTIONS}");
            let path = target.join(mountpoint);
            mounts.mount(
                &[
                    "-o".as_ref(),
                    options.as_ref(),
                    storage.root.as_os_str(),
                    path.as_os_str(),
                ],
                &path,
            )?;
        }
    }
    let efi_path = target.join("boot/efi");
    mounts.mount(&[storage.efi.as_os_str(), efi_path.as_os_str()], &efi_path)?;
    Ok(())
}

fn populate_target<F>(
    plan: &InstallPlan,
    storage: &ResolvedStorage,
    target: &Path,
    progress: &mut F,
) -> Result<()>
where
    F: FnMut(InstallProgress),
{
    if !Path::new(LIVE_SOURCE)
        .join("usr/lib/systemd/systemd")
        .is_file()
    {
        bail!("MattOS immutable live source is unavailable at {LIVE_SOURCE}");
    }
    engine::run(
        "cp",
        &[
            "-a".as_ref(),
            "--reflink=auto".as_ref(),
            format!("{LIVE_SOURCE}/.").as_ref(),
            target.as_os_str(),
        ],
    )?;
    normalize_systemd_unit_permissions(target)?;
    progress(InstallProgress::new(
        InstallStage::ConfiguringSystem,
        5,
        "Configuring the installed system",
    ));
    remove_live_only_state(target)?;
    configure_installed_apt(target)?;
    write_identity(plan, target)?;
    write_regional_identity(plan, target)?;
    let identity = storage_identity(storage)?;
    write_storage_identity(&identity, target)?;
    write_fstab(&identity, target)?;
    progress(InstallProgress::new(
        InstallStage::InstallingGrub,
        6,
        "Installing boot files and GRUB configuration",
    ));
    install_boot_files(&identity, target)?;
    progress(InstallProgress::new(
        InstallStage::ConfiguringSystem,
        6,
        "Removing live-only installer state",
    ));
    purge_live_installer(target)?;
    progress(InstallProgress::new(
        InstallStage::CreatingUser,
        7,
        "Creating the installed user account",
    ));
    create_user(plan, target)?;
    fs::write(
        target.join("etc/mattos-installed-profile"),
        match plan.installed_profile {
            InstalledProfile::Cli => "cli\n",
            InstalledProfile::Desktop => "desktop\n",
        },
    )?;
    configure_installed_profile(plan, target)?;
    Ok(())
}

fn normalize_systemd_unit_permissions(target: &Path) -> Result<()> {
    fn visit(directory: &Path) -> Result<()> {
        if !directory.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                visit(&path)?;
            } else if metadata.is_file() {
                let mode = metadata.permissions().mode();
                if mode & 0o111 != 0 {
                    fs::set_permissions(&path, fs::Permissions::from_mode(mode & !0o111))
                        .with_context(|| {
                            format!(
                                "remove executable bits from systemd unit {}",
                                path.display()
                            )
                        })?;
                }
            }
        }
        Ok(())
    }

    visit(&target.join("usr/lib/systemd/system"))?;
    visit(&target.join("usr/lib/systemd/user"))?;
    Ok(())
}

fn configure_installed_profile(plan: &InstallPlan, target: &Path) -> Result<()> {
    let default_target = target.join("etc/systemd/system/default.target");
    remove_optional_file(&default_target)?;
    let unit = match plan.installed_profile {
        InstalledProfile::Cli => "/usr/lib/systemd/system/multi-user.target",
        InstalledProfile::Desktop => "/usr/lib/systemd/system/graphical.target",
    };
    #[cfg(unix)]
    std::os::unix::fs::symlink(unit, &default_target)?;

    let greetd_config = target.join("etc/greetd/cosmic-greeter.toml");
    if plan.installed_profile == InstalledProfile::Desktop && !greetd_config.is_file() {
        bail!("desktop profile is missing COSMIC greetd configuration")
    }
    if plan.installed_profile == InstalledProfile::Desktop && plan.automatic_login {
        let mut config = fs::read_to_string(&greetd_config)?;
        config.push_str(&format!(
            "\n[initial_session]\ncommand = \"/usr/bin/start-cosmic\"\nuser = \"{}\"\n",
            plan.username
        ));
        fs::write(greetd_config, config)?;
    }
    remove_optional_file(&target.join("etc/mattos-desktop-pending"))?;
    Ok(())
}

fn remove_live_only_state(target: &Path) -> Result<()> {
    for relative in [
        "etc/systemd/system/getty@tty1.service.d/autologin.conf",
        "etc/systemd/system/serial-getty@ttyS0.service.d/autologin.conf",
        "etc/systemd/system/cosmic-greeter.service.d/live.conf",
        "etc/greetd/cosmic-live.toml",
        "etc/sudoers.d/00-mattos-live",
        "etc/tmpfiles.d/mattos-live.conf",
        "etc/motd",
    ] {
        remove_optional_file(&target.join(relative))?;
    }
    match fs::remove_dir_all(target.join("home/mattos")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("remove live user home"),
    }
    for relative in ["etc/passwd", "etc/shadow", "etc/group", "etc/gshadow"] {
        remove_account_from_database(&target.join(relative), "mattos")?;
    }
    Ok(())
}

/// Replace the live ISO's offline APT policy with the installed-system policy.
/// The templates are shipped by the MattOS apt package, so installation never
/// depends on reading the source repository or reaching the network.
fn configure_installed_apt(target: &Path) -> Result<()> {
    let template_root = target.join(INSTALLED_APT_POLICY.trim_start_matches('/'));
    let etc = target.join("etc/apt");
    for (name, destination) in [
        ("01mattos", etc.join("apt.conf.d/01mattos")),
        (
            "00mattos-priority",
            etc.join("preferences.d/00mattos-priority"),
        ),
        ("mattos.sources", etc.join("sources.list.d/mattos.sources")),
        (
            "mattos-hosted.sources",
            etc.join("sources.list.d/mattos-hosted.sources"),
        ),
        (
            "debian-trixie.sources",
            etc.join("sources.list.d/debian-trixie.sources"),
        ),
    ] {
        let source = template_root.join(name);
        if !source.is_file() {
            bail!(
                "installed APT policy template is missing: {}",
                source.display()
            );
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "install APT policy template {} as {}",
                source.display(),
                destination.display()
            )
        })?;
    }

    let local = fs::read_to_string(etc.join("sources.list.d/mattos.sources"))?;
    let hosted = fs::read_to_string(etc.join("sources.list.d/mattos-hosted.sources"))?;
    let debian = fs::read_to_string(etc.join("sources.list.d/debian-trixie.sources"))?;
    let preferences = fs::read_to_string(etc.join("preferences.d/00mattos-priority"))?;
    if !local.contains("Enabled: no")
        || !local.contains("file:/usr/share/mattos/repository")
        || !hosted.contains("Enabled: yes")
        || !hosted.contains("https://packages.mattsherfey.com")
        || !debian.contains("Enabled: yes")
        || !debian.contains("Suites: trixie trixie-updates")
        || !debian.contains("Suites: trixie-security")
        || !preferences.contains("Pin-Priority: 990")
        || !preferences.contains("Pin-Priority: 500")
        || preferences.contains("Pin-Priority: 1001")
        || !preferences.contains("Pin-Priority: -1")
    {
        bail!("installed APT policy failed its fail-closed repository checks");
    }
    for keyring in [
        "usr/share/keyrings/mattos-archive-keyring.asc",
        "usr/share/keyrings/debian-archive-keyring.asc",
    ] {
        if !target.join(keyring).is_file() {
            bail!("installed APT keyring is missing: /{keyring}");
        }
    }
    Ok(())
}

fn remove_optional_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("remove live-only file {}", path.display()))
        }
    }
}

fn remove_account_from_database(path: &Path, username: &str) -> Result<()> {
    let body = fs::read_to_string(path)?;
    let mut output = String::new();
    for line in body.lines() {
        let mut fields = line.split(':').map(str::to_owned).collect::<Vec<_>>();
        if fields.first().is_some_and(|name| name == username) {
            continue;
        }
        if fields.first().is_some_and(|name| name == "sudo") && fields.len() >= 4 {
            fields[3] = fields[3]
                .split(',')
                .filter(|member| !member.is_empty() && *member != username)
                .collect::<Vec<_>>()
                .join(",");
        }
        output.push_str(&fields.join(":"));
        output.push('\n');
    }
    fs::write(path, output)?;
    Ok(())
}

fn purge_live_installer(target: &Path) -> Result<()> {
    let root = format!("--root={}", target.display());
    let admindir = format!("--admindir={}", target.join("var/lib/dpkg").display());
    engine::run(
        "dpkg",
        &[
            root.as_ref(),
            admindir.as_ref(),
            "--purge".as_ref(),
            "mattos-installer".as_ref(),
        ],
    )
    .context("remove live-only installer package from installed target")
}

fn write_identity(plan: &InstallPlan, target: &Path) -> Result<()> {
    fs::write(target.join("etc/hostname"), format!("{}\n", plan.hostname))?;
    fs::write(
        target.join("etc/hosts"),
        format!(
            "127.0.0.1 localhost\n127.0.1.1 {}\n::1 localhost\n",
            plan.hostname
        ),
    )?;
    fs::write(target.join("etc/machine-id"), "")?;
    Ok(())
}

fn write_regional_identity(plan: &InstallPlan, target: &Path) -> Result<()> {
    let locale_dir = target.join("usr/lib/x86_64-linux-gnu/locale");
    fs::create_dir_all(&locale_dir)?;
    let prefix = format!("--prefix={}", target.display());
    let (locale_name, _) = plan.locale.split_once('.').expect("validated locale");
    engine::run(
        "localedef",
        &[
            prefix.as_ref(),
            "-i".as_ref(),
            locale_name.as_ref(),
            "-f".as_ref(),
            "UTF-8".as_ref(),
            plan.locale.as_ref(),
        ],
    )
    .context("generate selected locale in installed target")?;
    fs::write(
        target.join("etc/locale.conf"),
        format!("LANG={}\n", plan.locale),
    )?;
    fs::write(
        target.join("etc/vconsole.conf"),
        format!("KEYMAP={}\n", plan.keyboard_layout),
    )?;
    let x11 = target.join("etc/X11/xorg.conf.d");
    fs::create_dir_all(&x11)?;
    fs::write(
        x11.join("00-keyboard.conf"),
        format!(
            "Section \"InputClass\"\n Identifier \"mattos-keyboard\"\n MatchIsKeyboard \"on\"\n Option \"XkbLayout\" \"{}\"\n Option \"XkbVariant\" \"{}\"\nEndSection\n",
            plan.keyboard_layout, plan.keyboard_variant
        ),
    )?;
    let localtime = target.join("etc/localtime");
    let zone = format!("/usr/share/zoneinfo/{}", plan.timezone);
    #[cfg(unix)]
    std::os::unix::fs::symlink(zone, localtime)?;
    fs::write(target.join("etc/timezone"), format!("{}\n", plan.timezone))?;
    Ok(())
}

fn write_fstab(identity: &StorageIdentity, target: &Path) -> Result<()> {
    let mut body = String::from("# Generated from the validated MattOS storage plan.\n");
    match identity.root_filesystem {
        Filesystem::Btrfs => {
            body.push_str(&format!(
                "PARTUUID={} / btrfs subvol=@,{BTRFS_MOUNT_OPTIONS} 0 0\n",
                identity.root_partuuid
            ));
            if let Some(home) = &identity.home_partuuid {
                body.push_str(&format!(
                    "PARTUUID={home} /home {} defaults 0 2\n",
                    identity
                        .home_filesystem
                        .expect("home identity")
                        .mount_name()
                ));
            } else {
                body.push_str(&format!(
                    "PARTUUID={} /home btrfs subvol=@home,{BTRFS_MOUNT_OPTIONS} 0 0\n",
                    identity.root_partuuid
                ));
            }
            body.push_str(&format!(
                "PARTUUID={} /.snapshots btrfs subvol=@snapshots,{BTRFS_MOUNT_OPTIONS} 0 0\n",
                identity.root_partuuid
            ));
        }
        Filesystem::Ext4 => {
            body.push_str(&format!(
                "PARTUUID={} / ext4 defaults 0 1\n",
                identity.root_partuuid
            ));
            if let Some(home) = &identity.home_partuuid {
                body.push_str(&format!(
                    "PARTUUID={home} /home {} defaults 0 2\n",
                    identity
                        .home_filesystem
                        .expect("home identity")
                        .mount_name()
                ));
            }
        }
        Filesystem::Fat32 => unreachable!("validated root cannot be FAT32"),
    }
    body.push_str(&format!(
        "PARTUUID={} /boot/efi vfat umask=0077 0 2\n",
        identity.efi_partuuid
    ));
    fs::write(target.join("etc/fstab"), body)?;
    Ok(())
}

fn filesystem_uuid(device: &Path) -> Result<String> {
    engine::capture(
        "blkid",
        &[
            "-s".as_ref(),
            "UUID".as_ref(),
            "-o".as_ref(),
            "value".as_ref(),
            device.as_os_str(),
        ],
    )
}

fn partition_uuid(device: &Path) -> Result<String> {
    engine::capture(
        "blkid",
        &[
            "-s".as_ref(),
            "PARTUUID".as_ref(),
            "-o".as_ref(),
            "value".as_ref(),
            device.as_os_str(),
        ],
    )
}

fn storage_identity(storage: &ResolvedStorage) -> Result<StorageIdentity> {
    Ok(StorageIdentity {
        root_uuid: filesystem_uuid(&storage.root)?,
        root_partuuid: partition_uuid(&storage.root)?,
        efi_partuuid: partition_uuid(&storage.efi)?,
        home_partuuid: storage
            .home
            .as_ref()
            .map(|(device, _)| partition_uuid(device))
            .transpose()?,
        home_filesystem: storage.home.as_ref().map(|(_, filesystem)| *filesystem),
        root_filesystem: storage.root_filesystem,
    })
}

fn write_storage_identity(identity: &StorageIdentity, target: &Path) -> Result<()> {
    fs::write(
        target.join("etc/mattos-storage.conf"),
        format!(
            "root_uuid={}\nroot_partuuid={}\nroot_filesystem={}\nefi_partuuid={}\n",
            identity.root_uuid,
            identity.root_partuuid,
            identity.root_filesystem.display_name(),
            identity.efi_partuuid
        ),
    )?;
    Ok(())
}

fn install_boot_files(identity: &StorageIdentity, target: &Path) -> Result<()> {
    for (source, destination) in [
        ("/usr/lib/mattos/installer/vmlinuz", "boot/vmlinuz"),
        (
            "/usr/lib/mattos/installer/installed-initramfs.cpio.xz",
            "boot/installed-initramfs.cpio.xz",
        ),
        (
            "/usr/lib/mattos/installer/BOOTX64.EFI",
            "boot/efi/EFI/BOOT/BOOTX64.EFI",
        ),
    ] {
        let destination = target.join(destination);
        fs::create_dir_all(destination.parent().expect("boot destination parent"))?;
        fs::copy(source, &destination).with_context(|| format!("install boot asset {source}"))?;
    }
    let prefix = if identity.root_filesystem == Filesystem::Btrfs {
        "/@/boot/grub"
    } else {
        "/boot/grub"
    };
    fs::write(
        target.join("boot/efi/EFI/BOOT/grub.cfg"),
        format!(
            "search --no-floppy --fs-uuid --set=root {}\nset prefix=($root){prefix}\nconfigfile ($root){prefix}/grub.cfg\n",
            identity.root_uuid,
        ),
    )?;
    fs::create_dir_all(target.join("boot/grub"))?;
    fs::write(
        target.join("boot/grub/grub.cfg"),
        render_installed_grub_config(identity),
    )?;
    Ok(())
}

fn render_installed_grub_config(identity: &StorageIdentity) -> String {
    let (boot_prefix, root_flags, filesystem) = match identity.root_filesystem {
        Filesystem::Btrfs => (
            "/@",
            format!(" rootflags=subvol=@,{BTRFS_MOUNT_OPTIONS}"),
            "btrfs",
        ),
        Filesystem::Ext4 => ("", String::new(), "ext4"),
        Filesystem::Fat32 => unreachable!(),
    };
    format!(
        "set timeout=2\nset default=0\nmenuentry 'MattOS' {{\n linux {boot_prefix}/boot/vmlinuz mattos.root_uuid={} mattos.root_fstype={filesystem} rootfstype={filesystem}{root_flags} rw console=tty0 console=ttyS0,115200\n initrd {boot_prefix}/boot/installed-initramfs.cpio.xz\n}}\n",
        identity.root_uuid
    )
}

fn create_user(plan: &InstallPlan, target: &Path) -> Result<()> {
    // Removing the live account also removes its private GID 1000.  Do not let
    // useradd consult the now-stale GROUP=1000 default before recreating it;
    // materialize the installed user's private group first and select both IDs
    // explicitly.  This also keeps the first installed account deterministic.
    engine::run(
        "groupadd",
        &[
            "--root".as_ref(),
            target.as_os_str(),
            "--gid".as_ref(),
            "1000".as_ref(),
            plan.username.as_ref(),
        ],
    )?;
    let base = [
        "--root".as_ref(),
        target.as_os_str(),
        "--uid".as_ref(),
        "1000".as_ref(),
        "--gid".as_ref(),
        plan.username.as_ref(),
        "--create-home".as_ref(),
        "--shell".as_ref(),
        "/bin/brush".as_ref(),
        "--comment".as_ref(),
        plan.full_name.as_ref(),
    ];
    if plan.administrator {
        engine::run(
            "useradd",
            &[
                base.as_slice(),
                &["--groups".as_ref(), "sudo".as_ref(), plan.username.as_ref()],
            ]
            .concat(),
        )?;
    } else {
        engine::run(
            "useradd",
            &[base.as_slice(), &[plan.username.as_ref()]].concat(),
        )?;
    }
    if let Some(hash) = &plan.password_hash {
        engine::run(
            "usermod",
            &[
                "--root".as_ref(),
                target.as_os_str(),
                "--password".as_ref(),
                hash.as_ref(),
                plan.username.as_ref(),
            ],
        )?;
    }
    let root_hash = match &plan.root_credential {
        RootCredentialPolicy::SameAsUser => plan.password_hash.as_ref(),
        RootCredentialPolicy::SeparatePasswordHash(hash) => Some(hash),
    };
    if let Some(hash) = root_hash {
        engine::run(
            "usermod",
            &[
                "--root".as_ref(),
                target.as_os_str(),
                "--password".as_ref(),
                hash.as_ref(),
                "root".as_ref(),
            ],
        )?;
    }
    if plan.automatic_login || plan.test_autologin {
        for (unit, arguments) in [
            (
                "getty@tty1.service.d",
                format!("--autologin {} --noclear %I $TERM", plan.username),
            ),
            (
                "serial-getty@ttyS0.service.d",
                format!(
                    "--autologin {} --keep-baud 115200,57600,38400,9600 %I $TERM",
                    plan.username
                ),
            ),
        ] {
            let directory = target.join("etc/systemd/system").join(unit);
            fs::create_dir_all(&directory)?;
            fs::write(
                directory.join("autologin.conf"),
                format!("[Service]\nExecStart=\nExecStart=-/usr/sbin/agetty {arguments}\n"),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(disk: &str, profile: InstalledProfile) -> InstallPlan {
        InstallPlan {
            version: PLAN_VERSION,
            target_disk: disk.into(),
            storage: StoragePlan::guided_btrfs(),
            installed_profile: profile,
            hostname: "mattos-test".into(),
            full_name: "MattOS Test User".into(),
            username: "mattos-user".into(),
            password_hash: None,
            administrator: true,
            automatic_login: false,
            root_credential: RootCredentialPolicy::SameAsUser,
            locale: "en_US.UTF-8".into(),
            keyboard_layout: "us".into(),
            keyboard_variant: String::new(),
            timezone: "Etc/UTC".into(),
            test_autologin: true,
        }
    }

    #[test]
    fn frontend_and_installed_profile_are_independent() {
        assert_eq!(
            plan("/dev/vda", InstalledProfile::Desktop).installed_profile,
            InstalledProfile::Desktop
        );
        assert_eq!(
            plan("/dev/vda", InstalledProfile::Cli).installed_profile,
            InstalledProfile::Cli
        );
    }

    #[test]
    fn plan_validation_is_fail_closed() {
        let mut candidate = plan("/dev/vda", InstalledProfile::Cli);
        candidate.target_disk = "/tmp/disk".into();
        assert!(candidate.validate_policy().is_err());
        candidate.target_disk = "/dev/vda".into();
        candidate.username = "Root User".into();
        assert!(candidate.validate_policy().is_err());
    }

    #[test]
    fn plan_round_trip_is_machine_readable() {
        let original = plan("/dev/vda", InstalledProfile::Desktop);
        let body = toml::to_string(&original).unwrap();
        let decoded: InstallPlan = toml::from_str(&body).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn installed_apt_transition_disables_local_and_enables_signed_remotes() {
        let target = tempfile::tempdir().unwrap();
        let target = target.path();
        let template_root = target.join("usr/share/mattos/apt/installed");
        fs::create_dir_all(&template_root).unwrap();
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let source_root = repo_root.join("src/system/packages/config/apt/installed");
        for name in [
            "01mattos",
            "00mattos-priority",
            "mattos.sources",
            "mattos-hosted.sources",
            "debian-trixie.sources",
        ] {
            fs::copy(source_root.join(name), template_root.join(name)).unwrap();
        }
        for name in [
            "mattos-archive-keyring.asc",
            "debian-archive-keyring.asc",
        ] {
            let destination = target.join("usr/share/keyrings").join(name);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(
                repo_root.join("src/system/packages/config/apt/keys").join(name),
                destination,
            )
            .unwrap();
        }

        configure_installed_apt(target).unwrap();
        let local = fs::read_to_string(target.join("etc/apt/sources.list.d/mattos.sources"))
            .unwrap();
        let hosted =
            fs::read_to_string(target.join("etc/apt/sources.list.d/mattos-hosted.sources"))
                .unwrap();
        assert!(local.contains("Enabled: no"));
        assert!(hosted.contains("Enabled: yes"));
        assert!(
            fs::read_to_string(target.join("etc/apt/apt.conf.d/01mattos"))
                .unwrap()
                .contains("Verify-Peer \"true\"")
        );
    }

    #[test]
    fn rendered_plan_warns_about_erasure_and_btrfs_policy() {
        let output = render_plan(&plan("/dev/vda", InstalledProfile::Cli)).unwrap();
        assert!(output.contains("ERASED"));
        assert!(output.contains("@home"));
        assert!(output.contains("Btrfs"));
    }

    #[test]
    fn structured_progress_has_a_stable_complete_terminal_event() {
        let event = InstallProgress::new(InstallStage::Complete, INSTALL_STAGE_COUNT, "done");
        assert_eq!(event.total_stages, INSTALL_STAGE_COUNT);
        assert_eq!(event.completed_stages, event.total_stages);
        assert_eq!(event.stage, InstallStage::Complete);
        assert_eq!(event.detail, "done");
    }

    #[test]
    fn stage_display_names_cover_the_authoritative_stage_set() {
        assert_eq!(InstallStage::ALL.len(), INSTALL_STAGE_COUNT);
        assert!(
            InstallStage::ALL
                .iter()
                .all(|stage| !stage.display_name().is_empty())
        );
    }

    #[test]
    fn progress_fraction_is_safe_for_initial_and_invalid_totals() {
        let initial = InstallProgress::new(InstallStage::Preparing, 0, "starting");
        assert_eq!(initial.fraction(), 0.0);
        let invalid = InstallProgress {
            stage: InstallStage::Preparing,
            completed_stages: 4,
            total_stages: 0,
            detail: String::new(),
        };
        assert_eq!(invalid.fraction(), 0.0);
        let overcomplete = InstallProgress {
            stage: InstallStage::Complete,
            completed_stages: 99,
            total_stages: 10,
            detail: String::new(),
        };
        assert_eq!(overcomplete.fraction(), 1.0);
    }

    #[test]
    fn installed_init_discovers_btrfs_or_ext4_by_stable_uuid() {
        let source = include_str!("../engine/installed-init.c");
        assert!(source.contains("/sys/class/block"));
        assert!(source.contains("try_installed_root"));
        assert!(source.contains("mattos-installed-profile"));
        assert!(source.contains("mattos.root_fstype"));
        assert!(source.contains("strcmp(filesystem, \"ext4\")"));
        assert!(source.contains("mattos.root_uuid"));
        assert!(!source.contains("find_sibling_partition(root, 1"));
    }

    #[test]
    fn installed_fstab_uses_stable_partuuids_and_early_mount_ownership() {
        let directory = tempfile::tempdir().unwrap();
        let identity = StorageIdentity {
            root_uuid: "root-fs-uuid".into(),
            root_partuuid: "root-part-uuid".into(),
            efi_partuuid: "efi-part-uuid".into(),
            home_partuuid: None,
            home_filesystem: None,
            root_filesystem: Filesystem::Btrfs,
        };
        fs::create_dir_all(directory.path().join("etc")).unwrap();
        write_fstab(&identity, directory.path()).unwrap();
        let fstab = fs::read_to_string(directory.path().join("etc/fstab")).unwrap();
        assert!(fstab.contains("PARTUUID=root-part-uuid / btrfs subvol=@"));
        assert!(fstab.contains("PARTUUID=root-part-uuid /home btrfs subvol=@home"));
        assert!(fstab.contains("PARTUUID=root-part-uuid /.snapshots btrfs subvol=@snapshots"));
        assert!(fstab.contains("PARTUUID=efi-part-uuid /boot/efi vfat umask=0077"));
        assert!(!fstab.contains("/dev/vda"));
    }

    #[test]
    fn storage_identity_record_is_device_name_independent() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("etc")).unwrap();
        let identity = StorageIdentity {
            root_uuid: "root-fs-uuid".into(),
            root_partuuid: "root-part-uuid".into(),
            efi_partuuid: "efi-part-uuid".into(),
            home_partuuid: None,
            home_filesystem: None,
            root_filesystem: Filesystem::Btrfs,
        };
        write_storage_identity(&identity, directory.path()).unwrap();
        let config = fs::read_to_string(directory.path().join("etc/mattos-storage.conf")).unwrap();
        assert!(config.contains("root_uuid=root-fs-uuid"));
        assert!(!config.contains("/dev/"));
    }

    #[test]
    fn installed_kernel_command_line_does_not_claim_uuid_is_a_device_path() {
        let identity = StorageIdentity {
            root_uuid: "test-uuid".into(),
            root_partuuid: "root".into(),
            efi_partuuid: "efi".into(),
            home_partuuid: None,
            home_filesystem: None,
            root_filesystem: Filesystem::Btrfs,
        };
        let config = render_installed_grub_config(&identity);
        assert!(config.contains("mattos.root_uuid=test-uuid"));
        assert!(config.contains("linux /@/boot/vmlinuz"));
        assert!(config.contains("initrd /@/boot/installed-initramfs.cpio.xz"));
        assert!(!config.contains("root=UUID="));
    }

    #[test]
    fn installed_efi_config_addresses_the_btrfs_root_subvolume_from_the_top_level() {
        let source = include_str!("mod.rs");
        assert!(source.contains("set prefix=($root)/@/boot/grub"));
        assert!(source.contains("configfile ($root)/@/boot/grub/grub.cfg"));
    }

    #[test]
    fn ext4_boot_and_fstab_have_no_btrfs_assumptions() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("etc")).unwrap();
        let identity = StorageIdentity {
            root_uuid: "ext4-uuid".into(),
            root_partuuid: "root-part".into(),
            efi_partuuid: "efi-part".into(),
            home_partuuid: None,
            home_filesystem: None,
            root_filesystem: Filesystem::Ext4,
        };
        write_fstab(&identity, directory.path()).unwrap();
        let fstab = fs::read_to_string(directory.path().join("etc/fstab")).unwrap();
        assert!(fstab.contains("PARTUUID=root-part / ext4 defaults"));
        assert!(!fstab.contains("subvol="));
        let grub = render_installed_grub_config(&identity);
        assert!(grub.contains("mattos.root_fstype=ext4"));
        assert!(grub.contains("linux /boot/vmlinuz"));
        assert!(!grub.contains("/@/"));
    }

    #[test]
    fn separate_home_partition_is_emitted_from_the_plan_identity() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("etc")).unwrap();
        let identity = StorageIdentity {
            root_uuid: "root-uuid".into(),
            root_partuuid: "root-part".into(),
            efi_partuuid: "efi-part".into(),
            home_partuuid: Some("home-part".into()),
            home_filesystem: Some(Filesystem::Ext4),
            root_filesystem: Filesystem::Btrfs,
        };
        write_fstab(&identity, directory.path()).unwrap();
        let fstab = fs::read_to_string(directory.path().join("etc/fstab")).unwrap();
        assert!(fstab.contains("PARTUUID=home-part /home ext4 defaults"));
        assert!(!fstab.contains("/home btrfs subvol=@home"));
        assert!(fstab.contains("/.snapshots btrfs subvol=@snapshots"));
    }

    #[test]
    fn guided_efi_reuse_chooses_a_non_overlapping_root_region() {
        let gib = 1024 * 1024 * 1024;
        let efi = InstallPartition {
            device: "/dev/vda1".into(),
            parent_disk: "/dev/vda".into(),
            number: 1,
            start_bytes: 1024 * 1024,
            size_bytes: 512 * 1024 * 1024,
            filesystem: Some("vfat".into()),
            partition_type: Some(engine::EFI_SYSTEM_PARTITION_GUID.into()),
            is_esp: true,
            mount_points: Vec::new(),
        };
        let (start, size) = guided_root_geometry(16 * gib, &efi).unwrap();
        assert!(start * 512 >= efi.start_bytes + efi.size_bytes);
        assert!(size * 512 >= MINIMUM_DISK_BYTES);
    }

    #[test]
    fn review_distinguishes_preserve_reuse_and_destructive_operations() {
        let storage = manual(vec![
            operation("/dev/vda4", PartitionAction::Preserve, None, None),
            operation(
                "/dev/vda2",
                PartitionAction::Format,
                Some(Filesystem::Ext4),
                Some("/"),
            ),
            operation("/dev/vda1", PartitionAction::Reuse, None, Some("/boot/efi")),
        ]);
        let review = render_storage_plan(&storage, Path::new("/dev/vda")).unwrap();
        assert!(review.contains("PRESERVE: /dev/vda4"));
        assert!(review.contains("FORMAT: /dev/vda2 as ext4 mounted at /"));
        assert!(review.contains("REUSE: /dev/vda1 mounted at /boot/efi"));
    }

    fn manual(operations: Vec<PartitionOperation>) -> StoragePlan {
        StoragePlan::Manual {
            partitions: operations,
        }
    }
    fn operation(
        device: &str,
        action: PartitionAction,
        filesystem: Option<Filesystem>,
        mount: Option<&str>,
    ) -> PartitionOperation {
        PartitionOperation {
            device: device.into(),
            action,
            encryption: EncryptionPolicy::None,
            filesystem,
            mount_point: mount.map(str::to_owned),
            partition_number: None,
            start_mib: None,
            size_mib: None,
        }
    }

    #[test]
    fn manual_storage_requires_unique_root_and_efi() {
        let missing_efi = manual(vec![operation(
            "/dev/vda2",
            PartitionAction::Format,
            Some(Filesystem::Ext4),
            Some("/"),
        )]);
        assert!(
            missing_efi
                .validate(Path::new("/dev/vda"))
                .unwrap_err()
                .to_string()
                .contains("EFI")
        );
        let duplicate_root = manual(vec![
            operation(
                "/dev/vda2",
                PartitionAction::Format,
                Some(Filesystem::Ext4),
                Some("/"),
            ),
            operation(
                "/dev/vda3",
                PartitionAction::Format,
                Some(Filesystem::Btrfs),
                Some("/"),
            ),
            operation("/dev/vda1", PartitionAction::Reuse, None, Some("/boot/efi")),
        ]);
        assert!(
            duplicate_root
                .validate(Path::new("/dev/vda"))
                .unwrap_err()
                .to_string()
                .contains("more than once")
        );
    }

    #[test]
    fn preserved_partitions_cannot_be_accidentally_formatted() {
        let invalid = manual(vec![
            operation(
                "/dev/vda2",
                PartitionAction::Reuse,
                Some(Filesystem::Ext4),
                Some("/"),
            ),
            operation("/dev/vda1", PartitionAction::Reuse, None, Some("/boot/efi")),
        ]);
        assert!(
            invalid
                .validate(Path::new("/dev/vda"))
                .unwrap_err()
                .to_string()
                .contains("must not request formatting")
        );
    }

    #[test]
    fn manual_create_extents_reject_overlap() {
        let mut root = operation(
            "/dev/vda2",
            PartitionAction::Create,
            Some(Filesystem::Ext4),
            Some("/"),
        );
        root.partition_number = Some(2);
        root.start_mib = Some(512);
        root.size_mib = Some(4096);
        let mut home = operation(
            "/dev/vda3",
            PartitionAction::Create,
            Some(Filesystem::Ext4),
            Some("/home"),
        );
        home.partition_number = Some(3);
        home.start_mib = Some(4000);
        home.size_mib = Some(4096);
        let mut efi = operation(
            "/dev/vda1",
            PartitionAction::Create,
            Some(Filesystem::Fat32),
            Some("/boot/efi"),
        );
        efi.partition_number = Some(1);
        efi.start_mib = Some(1);
        efi.size_mib = Some(511);
        assert!(
            manual(vec![efi, root, home])
                .validate(Path::new("/dev/vda"))
                .unwrap_err()
                .to_string()
                .contains("overlap")
        );
    }

    #[test]
    fn manual_plan_can_replace_a_deleted_partition_at_the_same_device() {
        let deleted = operation("/dev/vda2", PartitionAction::Delete, None, None);
        let mut root = operation(
            "/dev/vda2",
            PartitionAction::Create,
            Some(Filesystem::Ext4),
            Some("/"),
        );
        root.partition_number = Some(2);
        root.start_mib = Some(512);
        root.size_mib = Some(8192);
        let efi = operation("/dev/vda1", PartitionAction::Reuse, None, Some("/boot/efi"));
        manual(vec![deleted, root, efi])
            .validate(Path::new("/dev/vda"))
            .unwrap();
    }

    #[test]
    fn live_account_removal_preserves_other_sudo_members() {
        let directory = tempfile::tempdir().unwrap();
        let group = directory.path().join("group");
        fs::write(&group, "sudo:x:27:alice,mattos,bob\nmattos:x:1000:\n").unwrap();
        remove_account_from_database(&group, "mattos").unwrap();
        assert_eq!(fs::read_to_string(group).unwrap(), "sudo:x:27:alice,bob\n");
    }

    #[test]
    fn installed_target_removes_graphical_live_autologin_state() {
        let directory = tempfile::tempdir().unwrap();
        for relative in [
            "etc/systemd/system/getty@tty1.service.d/autologin.conf",
            "etc/systemd/system/serial-getty@ttyS0.service.d/autologin.conf",
            "etc/systemd/system/cosmic-greeter.service.d/live.conf",
            "etc/greetd/cosmic-live.toml",
            "etc/sudoers.d/00-mattos-live",
            "etc/tmpfiles.d/mattos-live.conf",
            "etc/motd",
        ] {
            let path = directory.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "live only\n").unwrap();
        }
        fs::create_dir_all(directory.path().join("home/mattos")).unwrap();
        for (relative, contents) in [
            (
                "etc/passwd",
                "root:x:0:0:root:/root:/bin/brush\nmattos:x:1000:1000:MattOS Live User:/home/mattos:/bin/brush\n",
            ),
            ("etc/shadow", "root:!:::::::\nmattos:!:::::::\n"),
            ("etc/group", "root:x:0:\nsudo:x:27:mattos\nmattos:x:1000:\n"),
            ("etc/gshadow", "root:!::\nsudo:!::mattos\nmattos:!::\n"),
        ] {
            fs::write(directory.path().join(relative), contents).unwrap();
        }

        remove_live_only_state(directory.path()).unwrap();

        assert!(
            !directory
                .path()
                .join("etc/systemd/system/cosmic-greeter.service.d/live.conf")
                .exists()
        );
        assert!(
            !directory
                .path()
                .join("etc/greetd/cosmic-live.toml")
                .exists()
        );
        assert!(!directory.path().join("home/mattos").exists());
        assert!(
            !fs::read_to_string(directory.path().join("etc/passwd"))
                .unwrap()
                .contains("mattos:")
        );
    }

    #[test]
    fn installed_systemd_units_are_never_executable() {
        let target = tempfile::tempdir().unwrap();
        let system = target.path().join("usr/lib/systemd/system");
        let user = target.path().join("usr/lib/systemd/user");
        fs::create_dir_all(&system).unwrap();
        fs::create_dir_all(&user).unwrap();
        let system_unit = system.join("graphical.target");
        let user_unit = user.join("cosmic-session.target");
        fs::write(&system_unit, "[Unit]\n").unwrap();
        fs::write(&user_unit, "[Unit]\n").unwrap();
        fs::set_permissions(&system_unit, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&user_unit, fs::Permissions::from_mode(0o775)).unwrap();

        normalize_systemd_unit_permissions(target.path()).unwrap();

        assert_eq!(
            fs::metadata(system_unit).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert_eq!(
            fs::metadata(user_unit).unwrap().permissions().mode() & 0o777,
            0o664
        );
    }

    #[test]
    fn installed_user_policy_does_not_depend_on_a_numeric_default_primary_group() {
        let source = include_str!("mod.rs");
        assert!(source.contains("engine::run(\n        \"groupadd\""));
        assert!(source.contains("\"--uid\".as_ref()"));
        assert!(source.contains("\"--gid\".as_ref()"));
        assert!(source.contains("\"sudo\".as_ref()"));
        assert!(!source.contains("\"--user-group\".as_ref()"));
        assert!(!source.contains("\"wheel\".as_ref()"));
        assert!(source.contains("\"--shell\".as_ref()"));
        assert!(source.contains("\"/bin/brush\".as_ref()"));
    }

    #[test]
    fn installed_profiles_select_recoverable_systemd_targets() {
        for (profile, expected) in [
            (
                InstalledProfile::Desktop,
                "/usr/lib/systemd/system/graphical.target",
            ),
            (
                InstalledProfile::Cli,
                "/usr/lib/systemd/system/multi-user.target",
            ),
        ] {
            let directory = tempfile::tempdir().unwrap();
            fs::create_dir_all(directory.path().join("etc/systemd/system")).unwrap();
            fs::create_dir_all(directory.path().join("etc/greetd")).unwrap();
            fs::write(
                directory.path().join("etc/greetd/cosmic-greeter.toml"),
                "[default_session]\ncommand = \"/usr/bin/cosmic-greeter-start\"\nuser = \"cosmic-greeter\"\n",
            )
            .unwrap();
            let mut candidate = plan("/dev/vda", profile);
            candidate.automatic_login = false;
            configure_installed_profile(&candidate, directory.path()).unwrap();
            assert_eq!(
                fs::read_link(directory.path().join("etc/systemd/system/default.target")).unwrap(),
                PathBuf::from(expected)
            );
        }
    }

    #[test]
    fn desktop_autologin_uses_greetd_initial_session() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("etc/systemd/system")).unwrap();
        fs::create_dir_all(directory.path().join("etc/greetd")).unwrap();
        fs::write(
            directory.path().join("etc/greetd/cosmic-greeter.toml"),
            "[default_session]\ncommand = \"/usr/bin/cosmic-greeter-start\"\nuser = \"cosmic-greeter\"\n",
        )
        .unwrap();
        let mut candidate = plan("/dev/vda", InstalledProfile::Desktop);
        candidate.automatic_login = true;
        configure_installed_profile(&candidate, directory.path()).unwrap();
        let config =
            fs::read_to_string(directory.path().join("etc/greetd/cosmic-greeter.toml")).unwrap();
        assert!(config.contains("[initial_session]"));
        assert!(config.contains("command = \"/usr/bin/start-cosmic\""));
        assert!(config.contains(&format!("user = \"{}\"", candidate.username)));
    }
}
