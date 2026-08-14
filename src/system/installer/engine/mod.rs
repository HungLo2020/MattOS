//! Reusable installer mechanics without MattOS layout or profile decisions.

use anyhow::{Context, Result, anyhow, bail};
use std::ffi::{CStr, CString, OsStr};
use std::fs;
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[link(name = "crypt")]
unsafe extern "C" {
    fn crypt(key: *const libc::c_char, salt: *const libc::c_char) -> *mut libc::c_char;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallDisk {
    pub device: PathBuf,
    pub size_bytes: u64,
    pub model: String,
}

pub const EFI_SYSTEM_PARTITION_GUID: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallPartition {
    pub device: PathBuf,
    pub parent_disk: PathBuf,
    pub number: u32,
    pub start_bytes: u64,
    pub size_bytes: u64,
    pub filesystem: Option<String>,
    pub partition_type: Option<String>,
    pub is_esp: bool,
    pub mount_points: Vec<PathBuf>,
}

/// Discover the current partition table through lsblk's machine-readable
/// interface. This is read-only and shared by policy, CLI, and graphical UI.
pub fn discover_partitions(disk: &Path) -> Result<Vec<InstallPartition>> {
    let output = Command::new("lsblk")
        .args([
            "--json",
            "--bytes",
            "--paths",
            "-o",
            "PATH,PKNAME,TYPE,SIZE,FSTYPE,PARTTYPE,MOUNTPOINTS,START,PARTN",
        ])
        .arg(disk)
        .output()
        .context("launch lsblk partition discovery")?;
    if !output.status.success() {
        bail!("lsblk failed while discovering {}", disk.display());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parse lsblk JSON")?;
    let mut result = Vec::new();
    let devices = value
        .get("blockdevices")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow!("lsblk JSON has no blockdevices"))?;
    for device in devices {
        collect_partitions(device, disk, &mut result)?;
    }
    result.sort_by_key(|partition| partition.number);
    Ok(result)
}

fn json_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    value.and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
}

fn collect_partitions(
    value: &serde_json::Value,
    requested_disk: &Path,
    output: &mut Vec<InstallPartition>,
) -> Result<()> {
    if value.get("type").and_then(|value| value.as_str()) == Some("part") {
        let path = PathBuf::from(
            value
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or_else(|| anyhow!("partition has no path"))?,
        );
        let parent = value
            .get("pkname")
            .and_then(|value| value.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| requested_disk.to_path_buf());
        let partition_type = value
            .get("parttype")
            .and_then(|value| value.as_str())
            .map(|value| value.to_ascii_lowercase());
        let filesystem = value
            .get("fstype")
            .and_then(|value| value.as_str())
            .map(str::to_owned);
        let mount_points = value
            .get("mountpoints")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .collect();
        output.push(InstallPartition {
            device: path,
            parent_disk: parent,
            number: json_u64(value.get("partn")).unwrap_or(0) as u32,
            start_bytes: json_u64(value.get("start"))
                .unwrap_or(0)
                .saturating_mul(512),
            size_bytes: json_u64(value.get("size")).unwrap_or(0),
            is_esp: partition_type.as_deref() == Some(EFI_SYSTEM_PARTITION_GUID),
            filesystem,
            partition_type,
            mount_points,
        });
    }
    if let Some(children) = value.get("children").and_then(|value| value.as_array()) {
        for child in children {
            collect_partitions(child, requested_disk, output)?;
        }
    }
    Ok(())
}

pub fn discover_install_disks() -> Result<Vec<InstallDisk>> {
    discover_install_disks_in(Path::new("/sys/class/block"), Path::new("/dev"))
}

fn discover_install_disks_in(sys_block: &Path, dev_root: &Path) -> Result<Vec<InstallDisk>> {
    let mut disks = Vec::new();
    for entry in fs::read_dir(sys_block)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        if path.join("partition").exists() || name.starts_with("loop") || name.starts_with("ram") {
            continue;
        }
        // Optical media (SCSI type 5) and kernel read-only devices are not
        // whole-disk installation targets. In particular, this prevents the
        // live ISO's /dev/sr0 from appearing before the actual target disk in
        // the graphical and guided CLI frontends.
        let read_only = fs::read_to_string(path.join("ro")).is_ok_and(|value| value.trim() == "1");
        let optical =
            fs::read_to_string(path.join("device/type")).is_ok_and(|value| value.trim() == "5");
        if read_only || optical {
            continue;
        }
        let sectors = fs::read_to_string(path.join("size"))
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0);
        if sectors == 0 {
            continue;
        }
        let model = fs::read_to_string(path.join("device/model"))
            .unwrap_or_else(|_| "block device".into())
            .trim()
            .to_string();
        disks.push(InstallDisk {
            device: dev_root.join(name),
            size_bytes: sectors.saturating_mul(512),
            model,
        });
    }
    disks.sort_by(|left, right| left.device.cmp(&right.device));
    Ok(disks)
}

/// Hash a GUI-entered password without placing plaintext in argv or a plan.
/// MattOS libxcrypt supplies SHA-512 crypt; caller-owned and temporary
/// plaintext buffers are cleared whether hashing succeeds or fails.
pub fn hash_password_secure(password: &mut Vec<u8>) -> Result<String> {
    let result = (|| {
        let mut random = [0u8; 12];
        let count = unsafe { libc::getrandom(random.as_mut_ptr().cast(), random.len(), 0) };
        if count != random.len() as isize {
            bail!("secure password salt generation failed");
        }
        const ALPHABET: &[u8; 64] =
            b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let salt_body = random
            .iter()
            .map(|byte| ALPHABET[(*byte & 63) as usize] as char)
            .collect::<String>();
        let salt = CString::new(format!("$6${salt_body}$"))?;
        let key = CString::new(password.as_slice())
            .context("password contains an unsupported NUL byte")?;
        let result = unsafe { crypt(key.as_ptr(), salt.as_ptr()) };
        if result.is_null() {
            bail!("MattOS libxcrypt rejected SHA-512 password hashing");
        }
        let hash = unsafe { CStr::from_ptr(result) }.to_str()?.to_string();
        let mut temporary_plaintext = key.into_bytes_with_nul();
        temporary_plaintext.fill(0);
        if !hash.starts_with("$6$") || hash.contains(['\n', ':']) {
            bail!("password hasher returned an invalid SHA-512 crypt value");
        }
        Ok(hash)
    })();
    password.fill(0);
    password.clear();
    result
}

pub fn partition_path(disk: &Path, number: u32) -> Result<PathBuf> {
    if number == 0 {
        bail!("partition number must be positive");
    }
    let text = disk
        .to_str()
        .ok_or_else(|| anyhow!("target disk path is not UTF-8"))?;
    let separator = if text.as_bytes().last().is_some_and(u8::is_ascii_digit) {
        "p"
    } else {
        ""
    };
    Ok(PathBuf::from(format!("{text}{separator}{number}")))
}

/// Validate mechanics common to any supported whole-disk installation.
pub fn validate_whole_disk(target: &Path, minimum_bytes: u64) -> Result<()> {
    validate_install_disk(target, minimum_bytes, true)
}

pub fn validate_install_disk(
    target: &Path,
    minimum_bytes: u64,
    reject_mounted: bool,
) -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        bail!("installation requires root privileges");
    }
    let metadata = fs::metadata(target)
        .with_context(|| format!("target disk {} does not exist", target.display()))?;
    if !metadata.file_type().is_block_device() {
        bail!("target {} is not a block device", target.display());
    }
    let canonical = fs::canonicalize(target)?;
    let name = canonical
        .file_name()
        .ok_or_else(|| anyhow!("target has no block-device name"))?;
    let sectors: u64 = fs::read_to_string(Path::new("/sys/class/block").join(name).join("size"))?
        .trim()
        .parse()
        .context("invalid block size from sysfs")?;
    if sectors.saturating_mul(512) < minimum_bytes {
        bail!("target is smaller than the required minimum size");
    }

    // Compare block-device ancestry, not string prefixes such as vda/vdaa.
    let root = capture(
        "findmnt",
        &[
            "-rn".as_ref(),
            "-o".as_ref(),
            "SOURCE".as_ref(),
            "/".as_ref(),
        ],
    )?;
    if let Some(root_device) = root.strip_prefix("/dev/") {
        let root_name = Path::new(root_device)
            .file_name()
            .ok_or_else(|| anyhow!("running root has no block-device name"))?;
        let mut current = fs::canonicalize(Path::new("/sys/class/block").join(root_name))?;
        loop {
            if current.file_name() == Some(name) {
                bail!("refusing to erase the disk containing the running root");
            }
            let Some(parent) = current.parent() else {
                break;
            };
            if parent == current || parent.ends_with("block") {
                break;
            }
            current = parent.to_path_buf();
        }
    }

    let mounted = Command::new("lsblk")
        .args(["-nr", "-o", "MOUNTPOINTS"])
        .arg(&canonical)
        .output()?;
    if !mounted.status.success() {
        bail!("lsblk failed while validating {}", canonical.display());
    }
    if reject_mounted
        && String::from_utf8_lossy(&mounted.stdout)
            .lines()
            .any(|line| !line.trim().is_empty())
    {
        bail!("refusing to erase a target with mounted filesystems");
    }
    Ok(())
}

pub fn require_tools(tools: &[&str]) -> Result<()> {
    for tool in tools {
        let status = Command::new("/usr/bin/env")
            .args(["sh", "-c", &format!("command -v -- {tool} >/dev/null")])
            .status()?;
        if !status.success() {
            bail!("required installer tool is unavailable: {tool}");
        }
    }
    Ok(())
}

pub fn run(program: &str, arguments: &[&OsStr]) -> Result<()> {
    let status = Command::new(program)
        .args(arguments)
        .status()
        .with_context(|| format!("launch {program}"))?;
    if !status.success() {
        bail!("{program} failed with {status}");
    }
    Ok(())
}

pub fn capture(program: &str, arguments: &[&OsStr]) -> Result<String> {
    let output = Command::new(program).args(arguments).output()?;
    if !output.status.success() {
        bail!("{program} failed with {}", output.status);
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

pub fn command_with_input(program: &str, arguments: &[&OsStr], input: &[u8]) -> Result<()> {
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .spawn()?;
    child.stdin.take().expect("piped stdin").write_all(input)?;
    let status = child.wait()?;
    if !status.success() {
        bail!("{program} failed with {status}");
    }
    Ok(())
}

/// Tracks mounts and always unmounts them in reverse order.
pub struct MountStack {
    mounts: Vec<PathBuf>,
}

impl MountStack {
    pub fn new() -> Self {
        Self { mounts: Vec::new() }
    }

    pub fn mount(&mut self, arguments: &[&OsStr], mountpoint: &Path) -> Result<()> {
        run("mount", arguments)?;
        self.mounts.push(mountpoint.to_path_buf());
        Ok(())
    }

    pub fn unmount_all(&mut self) -> Result<()> {
        let mut first_error = None;
        while let Some(path) = self.mounts.pop() {
            if let Err(error) = run("umount", &[path.as_os_str()])
                && first_error.is_none()
            {
                first_error = Some(error.context(format!("unmount {}", path.display())));
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for MountStack {
    fn drop(&mut self) {
        let _ = self.unmount_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn partition_names_cover_virtio_nvme_and_loop() {
        assert_eq!(
            partition_path(Path::new("/dev/vda"), 2).unwrap(),
            Path::new("/dev/vda2")
        );
        assert_eq!(
            partition_path(Path::new("/dev/nvme0n1"), 1).unwrap(),
            Path::new("/dev/nvme0n1p1")
        );
        assert_eq!(
            partition_path(Path::new("/dev/loop7"), 2).unwrap(),
            Path::new("/dev/loop7p2")
        );
    }

    #[test]
    fn lsblk_json_discovery_preserves_storage_safety_fields() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{"path":"/dev/nvme0n1p1","pkname":"/dev/nvme0n1","type":"part","size":536870912,"fstype":"vfat","parttype":"C12A7328-F81F-11D2-BA4B-00A0C93EC93B","mountpoints":["/boot/efi",null],"start":2048,"partn":1}"#,
        )
        .unwrap();
        let mut partitions = Vec::new();
        collect_partitions(&value, Path::new("/dev/nvme0n1"), &mut partitions).unwrap();
        assert_eq!(partitions.len(), 1);
        let partition = &partitions[0];
        assert_eq!(partition.device, Path::new("/dev/nvme0n1p1"));
        assert_eq!(partition.parent_disk, Path::new("/dev/nvme0n1"));
        assert_eq!(partition.number, 1);
        assert_eq!(partition.start_bytes, 2048 * 512);
        assert_eq!(partition.size_bytes, 512 * 1024 * 1024);
        assert_eq!(partition.filesystem.as_deref(), Some("vfat"));
        assert!(partition.is_esp);
        assert_eq!(partition.mount_points, [PathBuf::from("/boot/efi")]);
    }

    #[test]
    fn password_hashing_is_sha512_salted_and_clears_plaintext() {
        let mut password = b"not-a-real-password".to_vec();
        let hash = hash_password_secure(&mut password).unwrap();
        assert!(hash.starts_with("$6$"));
        assert!(!hash.contains("not-a-real-password"));
        assert!(password.is_empty());
    }

    #[test]
    fn disk_discovery_excludes_live_optical_and_read_only_devices() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mattos-installer-disks-{}-{nonce}",
            std::process::id()
        ));
        let sys = root.join("sys");
        let dev = root.join("dev");
        for (name, sectors, read_only, device_type) in [
            ("vda", "25165824", "0", "0"),
            ("sr0", "1563648", "1", "5"),
            ("readonly-disk", "25165824", "1", "0"),
        ] {
            let path = sys.join(name);
            fs::create_dir_all(path.join("device")).unwrap();
            fs::write(path.join("size"), sectors).unwrap();
            fs::write(path.join("ro"), read_only).unwrap();
            fs::write(path.join("device/type"), device_type).unwrap();
            fs::write(path.join("device/model"), format!("{name} model")).unwrap();
        }
        fs::create_dir_all(&dev).unwrap();

        let disks = discover_install_disks_in(&sys, &dev).unwrap();
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].device, dev.join("vda"));
        assert_eq!(disks[0].size_bytes, 12 * 1024 * 1024 * 1024);

        fs::remove_dir_all(root).unwrap();
    }
}
