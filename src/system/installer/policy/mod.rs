//! Authoritative MattOS installation policy.

use crate::engine::{self, MountStack, PartitionPaths};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PLAN_VERSION: u32 = 3;
pub const MINIMUM_DISK_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const EFI_MIB: u64 = 512;
pub const LIVE_SOURCE: &str = "/run/mattos/lower";
pub const TARGET_ROOT: &str = "/run/mattos-installer/target";
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
        Self::Preparing, Self::Partitioning, Self::Formatting,
        Self::CreatingSubvolumes, Self::DeployingSystem, Self::ConfiguringSystem,
        Self::CreatingUser, Self::InstallingGrub, Self::Finalizing, Self::Complete,
    ];

    /// Stable user-facing name shared by installer frontends.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Preparing => "Preparing", Self::Partitioning => "Partitioning",
            Self::Formatting => "Formatting", Self::CreatingSubvolumes => "Creating Btrfs subvolumes",
            Self::DeployingSystem => "Deploying system", Self::ConfiguringSystem => "Configuring system",
            Self::CreatingUser => "Creating user", Self::InstallingGrub => "Installing GRUB",
            Self::Finalizing => "Finalizing", Self::Complete => "Complete",
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
        Self { stage, completed_stages, total_stages: INSTALL_STAGE_COUNT, detail: detail.into() }
    }

    /// Overall determinate progress, safely bounded for presentation.
    pub fn fraction(&self) -> f32 {
        if self.total_stages == 0 { return 0.0; }
        (self.completed_stages.min(self.total_stages) as f32) / self.total_stages as f32
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StorageIdentity {
    root_uuid: String,
    root_partuuid: String,
    efi_partuuid: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstalledProfile {
    Cli,
    Desktop,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "mode", content = "password_hash")]
pub enum RootCredentialPolicy { SameAsUser, SeparatePasswordHash(String) }

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallPlan {
    pub version: u32,
    pub target_disk: PathBuf,
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
    #[serde(default)] pub keyboard_variant: String,
    pub timezone: String,
    #[serde(default)] pub test_autologin: bool,
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
            bail!("unsupported install plan version {}; expected {PLAN_VERSION}", self.version);
        }
        let disk = self.target_disk.to_string_lossy();
        if !disk.starts_with("/dev/") || disk.contains("..") {
            bail!("target_disk must be an explicit absolute /dev path");
        }
        validate_identifier(&self.hostname, "hostname")?;
        validate_identifier(&self.username, "username")?;
        if self.username == "root" {
            bail!("the installed user may not be root");
        }
        if self.full_name.trim().is_empty() || self.full_name.contains(['\n', ':']) { bail!("full_name must be a non-empty single-line GECOS value"); }
        if let Some(hash) = &self.password_hash
            && (!hash.starts_with('$') || hash.contains(['\n', ':']))
        {
            bail!("password_hash must be a crypt-style hash without separators");
        }
        if let RootCredentialPolicy::SeparatePasswordHash(hash) = &self.root_credential
            && (!hash.starts_with('$') || hash.contains(['\n', ':'])) { bail!("root password hash must be a crypt-style hash without separators"); }
        if !locale_supported(&self.locale) { bail!("unsupported locale {}; install image locale data does not provide it", self.locale); }
        if !xkb_layout_supported(&self.keyboard_layout, &self.keyboard_variant) { bail!("unsupported keyboard layout/variant {}({})", self.keyboard_layout, self.keyboard_variant); }
        if !timezone_supported(&self.timezone) { bail!("unsupported timezone {}", self.timezone); }
        Ok(())
    }
}

fn runtime_path(relative: &str) -> PathBuf {
    let live = Path::new(LIVE_SOURCE).join(relative);
    if live.exists() { live } else { Path::new("/").join(relative) }
}

pub fn locale_supported(locale: &str) -> bool {
    let Some((name, encoding)) = locale.split_once('.') else { return false; };
    encoding.eq_ignore_ascii_case("UTF-8") && runtime_path(&format!("usr/share/i18n/locales/{name}")).is_file()
}

pub fn timezone_supported(timezone: &str) -> bool {
    !timezone.is_empty() && !timezone.starts_with('/') && !timezone.contains("..")
        && runtime_path(&format!("usr/share/zoneinfo/{timezone}")).is_file()
}

pub fn xkb_layout_supported(layout: &str, variant: &str) -> bool {
    let Ok(data) = fs::read_to_string(runtime_path("usr/share/X11/xkb/rules/evdev.lst")) else { return false; };
    let mut in_variants = false;
    let mut layout_found = false;
    for line in data.lines() {
        if line == "! variant" { in_variants = true; continue; }
        if line.starts_with('!') { in_variants = false; }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if !in_variants && fields.first() == Some(&layout) { layout_found = true; }
        if in_variants && !variant.is_empty() && fields.first() == Some(&variant) && fields.get(1) == Some(&layout) { return true; }
    }
    variant.is_empty() && layout_found
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
    let parts = engine::partition_paths(&plan.target_disk)?;
    Ok(format!(
        "MattOS install plan\n  target: {} (ERASED)\n  firmware: UEFI\n  partition table: GPT\n  EFI: {} (FAT32, {EFI_MIB} MiB)\n  system: {} (Btrfs)\n  subvolumes: @, @home, @snapshots\n  mount options: {BTRFS_MOUNT_OPTIONS}\n  profile: {:?}\n  locale: {}\n  keyboard: {} ({})\n  timezone: {}\n  hostname: {}\n  full name: {}\n  user: {}\n  account type: {}\n  automatic login: {}\n  root credential: {}\n",
        plan.target_disk.display(),
        parts.efi.display(),
        parts.system.display(),
        plan.installed_profile, plan.locale, plan.keyboard_layout, if plan.keyboard_variant.is_empty() { "default" } else { &plan.keyboard_variant }, plan.timezone, plan.hostname, plan.full_name, plan.username,
        if plan.administrator { "Administrator" } else { "Standard user" },
        if plan.automatic_login { "enabled" } else { "disabled" },
        match plan.root_credential { RootCredentialPolicy::SameAsUser => "same as user", RootCredentialPolicy::SeparatePasswordHash(_) => "separate password" }
    ))
}

pub fn validate_target(plan: &InstallPlan) -> Result<()> {
    plan.validate_policy()?;
    engine::validate_whole_disk(&plan.target_disk, MINIMUM_DISK_BYTES)
        .context("MattOS target safety policy rejected the selected disk")
}

pub fn execute(plan: &InstallPlan) -> Result<()> {
    execute_with_progress(plan, |event| eprintln!("mattos-install: {:?}: {}", event.stage, event.detail))
}

pub fn execute_with_progress<F>(plan: &InstallPlan, mut progress: F) -> Result<()>
where
    F: FnMut(InstallProgress),
{
    progress(InstallProgress::new(InstallStage::Preparing, 0, "Validating the selected target disk"));
    validate_target(plan)?;
    engine::require_tools(&[
        "sfdisk",
        "mkfs.fat",
        "mkfs.btrfs",
        "btrfs",
        "mount",
        "umount",
        "cp",
        "blkid",
        "udevadm",
        "useradd",
        "usermod",
        "dpkg",
    ])?;
    let parts = engine::partition_paths(&plan.target_disk)?;
    progress(InstallProgress::new(InstallStage::Partitioning, 1, "Creating the GPT partition table"));
    let layout = format!("label: gpt\n,{}M,U\n,,L\n", EFI_MIB);
    engine::command_with_input("sfdisk", &[plan.target_disk.as_os_str()], layout.as_bytes())?;
    engine::run("udevadm", &["settle".as_ref()])?;
    progress(InstallProgress::new(InstallStage::Formatting, 2, "Formatting EFI and Btrfs filesystems"));
    engine::run(
        "mkfs.fat",
        &[
            "-F".as_ref(),
            "32".as_ref(),
            "-n".as_ref(),
            "MATTOS_EFI".as_ref(),
            parts.efi.as_os_str(),
        ],
    )?;
    engine::run(
        "mkfs.btrfs",
        &[
            "-f".as_ref(),
            "-L".as_ref(),
            "MattOS".as_ref(),
            parts.system.as_os_str(),
        ],
    )?;

    let target = Path::new(TARGET_ROOT);
    progress(InstallProgress::new(InstallStage::CreatingSubvolumes, 3, "Creating MattOS Btrfs subvolumes"));
    fs::create_dir_all(target)?;
    let mut mounts = MountStack::new();
    mounts.mount(&[parts.system.as_os_str(), target.as_os_str()], target)?;
    for (subvolume, _) in BTRFS_SUBVOLUMES {
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

    let root_options = format!("subvol=@,{BTRFS_MOUNT_OPTIONS}");
    mounts.mount(
        &["-o".as_ref(), root_options.as_ref(), parts.system.as_os_str(), target.as_os_str()],
        target,
    )?;
    for relative in ["home", ".snapshots", "boot/efi"] {
        fs::create_dir_all(target.join(relative))?;
    }
    for (subvolume, mountpoint) in [("@home", "home"), ("@snapshots", ".snapshots")] {
        let options = format!("subvol={subvolume},{BTRFS_MOUNT_OPTIONS}");
        let path = target.join(mountpoint);
        mounts.mount(
            &["-o".as_ref(), options.as_ref(), parts.system.as_os_str(), path.as_os_str()],
            &path,
        )?;
    }
    let efi_path = target.join("boot/efi");
    mounts.mount(&[parts.efi.as_os_str(), efi_path.as_os_str()], &efi_path)?;

    progress(InstallProgress::new(InstallStage::DeployingSystem, 4, "Copying the immutable live system to the target"));
    let install_result = populate_target(plan, &parts, target, &mut progress);
    let sync_result = engine::run("sync", &[]);
    let unmount_result = mounts.unmount_all();
    install_result.and(sync_result).and(unmount_result)?;
    progress(InstallProgress::new(InstallStage::Finalizing, 8, "Synchronizing and unmounting target filesystems"));
    progress(InstallProgress::new(InstallStage::Complete, INSTALL_STAGE_COUNT, "Installation complete"));
    Ok(())
}

fn populate_target<F>(
    plan: &InstallPlan,
    parts: &PartitionPaths,
    target: &Path,
    progress: &mut F,
) -> Result<()>
where
    F: FnMut(InstallProgress),
{
    if !Path::new(LIVE_SOURCE).join("usr/lib/systemd/systemd").is_file() {
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
    progress(InstallProgress::new(InstallStage::ConfiguringSystem, 5, "Configuring the installed system"));
    remove_live_only_state(target)?;
    write_identity(plan, target)?;
    write_regional_identity(plan, target)?;
    let identity = storage_identity(parts)?;
    write_storage_identity(&identity, target)?;
    write_fstab(&identity, target)?;
    progress(InstallProgress::new(InstallStage::InstallingGrub, 6, "Installing boot files and GRUB configuration"));
    install_boot_files(&identity, target)?;
    progress(InstallProgress::new(InstallStage::ConfiguringSystem, 6, "Removing live-only installer state"));
    purge_live_installer(target)?;
    progress(InstallProgress::new(InstallStage::CreatingUser, 7, "Creating the installed user account"));
    create_user(plan, target)?;
    fs::write(
        target.join("etc/mattos-installed-profile"),
        match plan.installed_profile {
            InstalledProfile::Cli => "cli\n",
            InstalledProfile::Desktop => "desktop\n",
        },
    )?;
    if plan.installed_profile == InstalledProfile::Desktop {
        fs::write(
            target.join("etc/mattos-desktop-pending"),
            "COSMIC packages and cosmic-initial-setup are not yet in the base source closure.\n",
        )?;
    }
    Ok(())
}

fn remove_live_only_state(target: &Path) -> Result<()> {
    for relative in [
        "etc/systemd/system/getty@tty1.service.d/autologin.conf",
        "etc/systemd/system/serial-getty@ttyS0.service.d/autologin.conf",
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

fn remove_optional_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("remove live-only file {}", path.display())),
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
        &[root.as_ref(), admindir.as_ref(), "--purge".as_ref(), "mattos-installer".as_ref()],
    )
    .context("remove live-only installer package from installed target")
}

fn write_identity(plan: &InstallPlan, target: &Path) -> Result<()> {
    fs::write(target.join("etc/hostname"), format!("{}\n", plan.hostname))?;
    fs::write(
        target.join("etc/hosts"),
        format!("127.0.0.1 localhost\n127.0.1.1 {}\n::1 localhost\n", plan.hostname),
    )?;
    fs::write(target.join("etc/machine-id"), "")?;
    Ok(())
}

fn write_regional_identity(plan: &InstallPlan, target: &Path) -> Result<()> {
    let locale_dir = target.join("usr/lib/x86_64-linux-gnu/locale");
    fs::create_dir_all(&locale_dir)?;
    let prefix = format!("--prefix={}", target.display());
    let (locale_name, _) = plan.locale.split_once('.').expect("validated locale");
    engine::run("localedef", &[prefix.as_ref(), "-i".as_ref(), locale_name.as_ref(), "-f".as_ref(), "UTF-8".as_ref(), plan.locale.as_ref()])
        .context("generate selected locale in installed target")?;
    fs::write(target.join("etc/locale.conf"), format!("LANG={}\n", plan.locale))?;
    fs::write(target.join("etc/vconsole.conf"), format!("KEYMAP={}\n", plan.keyboard_layout))?;
    let x11 = target.join("etc/X11/xorg.conf.d");
    fs::create_dir_all(&x11)?;
    fs::write(x11.join("00-keyboard.conf"), format!("Section \"InputClass\"\n Identifier \"mattos-keyboard\"\n MatchIsKeyboard \"on\"\n Option \"XkbLayout\" \"{}\"\n Option \"XkbVariant\" \"{}\"\nEndSection\n", plan.keyboard_layout, plan.keyboard_variant))?;
    let localtime = target.join("etc/localtime");
    let zone = format!("/usr/share/zoneinfo/{}", plan.timezone);
    #[cfg(unix)] std::os::unix::fs::symlink(zone, localtime)?;
    fs::write(target.join("etc/timezone"), format!("{}\n", plan.timezone))?;
    Ok(())
}

fn write_fstab(identity: &StorageIdentity, target: &Path) -> Result<()> {
    fs::write(
        target.join("etc/fstab"),
        format!(
            "# MattOS installed-init mounts these stable identities before systemd.\nPARTUUID={} / btrfs noauto,subvol=@,{BTRFS_MOUNT_OPTIONS} 0 0\nPARTUUID={} /home btrfs noauto,subvol=@home,{BTRFS_MOUNT_OPTIONS} 0 0\nPARTUUID={} /.snapshots btrfs noauto,subvol=@snapshots,{BTRFS_MOUNT_OPTIONS} 0 0\nPARTUUID={} /boot/efi vfat noauto,umask=0077 0 2\n",
            identity.root_partuuid,
            identity.root_partuuid,
            identity.root_partuuid,
            identity.efi_partuuid,
        ),
    )?;
    Ok(())
}

fn filesystem_uuid(device: &Path) -> Result<String> {
    engine::capture(
        "blkid",
        &["-s".as_ref(), "UUID".as_ref(), "-o".as_ref(), "value".as_ref(), device.as_os_str()],
    )
}

fn partition_uuid(device: &Path) -> Result<String> {
    engine::capture(
        "blkid",
        &["-s".as_ref(), "PARTUUID".as_ref(), "-o".as_ref(), "value".as_ref(), device.as_os_str()],
    )
}

fn storage_identity(parts: &PartitionPaths) -> Result<StorageIdentity> {
    Ok(StorageIdentity {
        root_uuid: filesystem_uuid(&parts.system)?,
        root_partuuid: partition_uuid(&parts.system)?,
        efi_partuuid: partition_uuid(&parts.efi)?,
    })
}

fn write_storage_identity(identity: &StorageIdentity, target: &Path) -> Result<()> {
    fs::write(
        target.join("etc/mattos-storage.conf"),
        format!(
            "root_uuid={}\nroot_partuuid={}\nefi_partuuid={}\n",
            identity.root_uuid, identity.root_partuuid, identity.efi_partuuid
        ),
    )?;
    Ok(())
}

fn install_boot_files(identity: &StorageIdentity, target: &Path) -> Result<()> {
    for (source, destination) in [
        ("/usr/lib/mattos/installer/vmlinuz", "boot/vmlinuz"),
        ("/usr/lib/mattos/installer/installed-initramfs.cpio.xz", "boot/installed-initramfs.cpio.xz"),
        ("/usr/lib/mattos/installer/BOOTX64.EFI", "boot/efi/EFI/BOOT/BOOTX64.EFI"),
    ] {
        let destination = target.join(destination);
        fs::create_dir_all(destination.parent().expect("boot destination parent"))?;
        fs::copy(source, &destination).with_context(|| format!("install boot asset {source}"))?;
    }
    fs::write(
        target.join("boot/efi/EFI/BOOT/grub.cfg"),
        format!(
            "search --no-floppy --fs-uuid --set=root {}\nset prefix=($root)/@/boot/grub\nconfigfile ($root)/@/boot/grub/grub.cfg\n",
            identity.root_uuid
        ),
    )?;
    fs::create_dir_all(target.join("boot/grub"))?;
    fs::write(
        target.join("boot/grub/grub.cfg"),
        render_installed_grub_config(&identity.root_uuid),
    )?;
    Ok(())
}

fn render_installed_grub_config(root_uuid: &str) -> String {
    format!(
        "set timeout=2\nset default=0\nmenuentry 'MattOS' {{\n linux /@/boot/vmlinuz mattos.root_uuid={root_uuid} rootfstype=btrfs rootflags=subvol=@,{BTRFS_MOUNT_OPTIONS} rw console=tty0 console=ttyS0,115200\n initrd /@/boot/installed-initramfs.cpio.xz\n}}\n"
    )
}

fn create_user(plan: &InstallPlan, target: &Path) -> Result<()> {
    let base = ["--root".as_ref(), target.as_os_str(), "--create-home".as_ref(), "--user-group".as_ref(), "--shell".as_ref(), "/bin/brush".as_ref(), "--comment".as_ref(), plan.full_name.as_ref()];
    if plan.administrator {
        engine::run("useradd", &[base.as_slice(), &["--groups".as_ref(), "wheel".as_ref(), plan.username.as_ref()]].concat())?;
    } else {
        engine::run("useradd", &[base.as_slice(), &[plan.username.as_ref()]].concat())?;
    }
    if let Some(hash) = &plan.password_hash {
        engine::run(
            "usermod",
            &["--root".as_ref(), target.as_os_str(), "--password".as_ref(), hash.as_ref(), plan.username.as_ref()],
        )?;
    }
    let root_hash = match &plan.root_credential { RootCredentialPolicy::SameAsUser => plan.password_hash.as_ref(), RootCredentialPolicy::SeparatePasswordHash(hash) => Some(hash) };
    if let Some(hash) = root_hash { engine::run("usermod", &["--root".as_ref(), target.as_os_str(), "--password".as_ref(), hash.as_ref(), "root".as_ref()])?; }
    if plan.automatic_login || plan.test_autologin {
        for (unit, arguments) in [
            ("getty@tty1.service.d", format!("--autologin {} --noclear %I $TERM", plan.username)),
            ("serial-getty@ttyS0.service.d", format!("--autologin {} --keep-baud 115200,57600,38400,9600 %I $TERM", plan.username)),
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
            installed_profile: profile,
            hostname: "mattos-test".into(),
            full_name: "MattOS Test User".into(),
            username: "mattos-user".into(),
            password_hash: None,
            administrator: true,
            automatic_login: false,
            root_credential: RootCredentialPolicy::SameAsUser,
            locale: "en_US.UTF-8".into(), keyboard_layout: "us".into(), keyboard_variant: String::new(), timezone: "Etc/UTC".into(),
            test_autologin: true,
        }
    }

    #[test]
    fn frontend_and_installed_profile_are_independent() {
        assert_eq!(plan("/dev/vda", InstalledProfile::Desktop).installed_profile, InstalledProfile::Desktop);
        assert_eq!(plan("/dev/vda", InstalledProfile::Cli).installed_profile, InstalledProfile::Cli);
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
        assert!(InstallStage::ALL.iter().all(|stage| !stage.display_name().is_empty()));
    }

    #[test]
    fn progress_fraction_is_safe_for_initial_and_invalid_totals() {
        let initial = InstallProgress::new(InstallStage::Preparing, 0, "starting");
        assert_eq!(initial.fraction(), 0.0);
        let invalid = InstallProgress {
            stage: InstallStage::Preparing, completed_stages: 4, total_stages: 0, detail: String::new(),
        };
        assert_eq!(invalid.fraction(), 0.0);
        let overcomplete = InstallProgress {
            stage: InstallStage::Complete, completed_stages: 99, total_stages: 10, detail: String::new(),
        };
        assert_eq!(overcomplete.fraction(), 1.0);
    }

    #[test]
    fn installed_init_discovers_a_real_btrfs_device_instead_of_mounting_uuid_text() {
        let source = include_str!("../engine/installed-init.c");
        assert!(source.contains("/sys/class/block"));
        assert!(source.contains("try_installed_root"));
        assert!(source.contains("mattos-installed-profile"));
        assert!(source.contains("MS_NOATIME, \"subvol=@,compress=zstd:3\""));
        assert!(source.contains("mattos.root_uuid"));
        assert!(source.contains("mount_installed_filesystems"));
        assert!(source.contains("find_sibling_partition(root, 1"));
        assert!(!source.contains("subvol=@,compress=zstd:3,noatime"));
    }

    #[test]
    fn installed_fstab_uses_stable_partuuids_and_early_mount_ownership() {
        let directory = tempfile::tempdir().unwrap();
        let identity = StorageIdentity {
            root_uuid: "root-fs-uuid".into(),
            root_partuuid: "root-part-uuid".into(),
            efi_partuuid: "efi-part-uuid".into(),
        };
        fs::create_dir_all(directory.path().join("etc")).unwrap();
        write_fstab(&identity, directory.path()).unwrap();
        let fstab = fs::read_to_string(directory.path().join("etc/fstab")).unwrap();
        assert!(fstab.contains("PARTUUID=root-part-uuid / btrfs noauto,subvol=@"));
        assert!(fstab.contains("PARTUUID=root-part-uuid /home btrfs noauto,subvol=@home"));
        assert!(fstab.contains("PARTUUID=root-part-uuid /.snapshots btrfs noauto,subvol=@snapshots"));
        assert!(fstab.contains("PARTUUID=efi-part-uuid /boot/efi vfat noauto"));
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
        };
        write_storage_identity(&identity, directory.path()).unwrap();
        let config = fs::read_to_string(directory.path().join("etc/mattos-storage.conf")).unwrap();
        assert!(config.contains("root_uuid=root-fs-uuid"));
        assert!(!config.contains("/dev/"));
    }

    #[test]
    fn installed_kernel_command_line_does_not_claim_uuid_is_a_device_path() {
        let config = render_installed_grub_config("test-uuid");
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
    fn live_account_removal_preserves_other_sudo_members() {
        let directory = tempfile::tempdir().unwrap();
        let group = directory.path().join("group");
        fs::write(&group, "sudo:x:27:alice,mattos,bob\nmattos:x:1000:\n").unwrap();
        remove_account_from_database(&group, "mattos").unwrap();
        assert_eq!(fs::read_to_string(group).unwrap(), "sudo:x:27:alice,bob\n");
    }

    #[test]
    fn installed_user_policy_does_not_depend_on_a_numeric_default_primary_group() {
        let source = include_str!("mod.rs");
        assert!(source.contains("\"--user-group\".as_ref()"));
        assert!(source.contains("\"--shell\".as_ref(), \"/bin/brush\".as_ref()"));
    }
}
