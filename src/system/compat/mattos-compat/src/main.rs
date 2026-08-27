use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

const COMPAT_ROOT: &str = "/compat";

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Distro {
    Debian,
    Fedora,
    Popos,
}

impl Distro {
    fn id(self) -> &'static str {
        match self {
            Self::Debian => "debian-trixie",
            Self::Fedora => "fedora",
            Self::Popos => "popos",
        }
    }

    fn root(self) -> PathBuf {
        Path::new(COMPAT_ROOT).join(self.id())
    }

    fn command_name(self) -> &'static str {
        match self {
            Self::Debian => "debian",
            Self::Fedora => "fedora",
            Self::Popos => "popos",
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "mattos-compat",
    about = "Run applications from isolated distribution userlands"
)]
struct Cli {
    #[command(subcommand)]
    distro: DistroCommand,
}

#[derive(Subcommand, Debug)]
enum DistroCommand {
    Debian {
        #[command(subcommand)]
        command: PackageCommand,
    },
    Fedora {
        #[command(subcommand)]
        command: PackageCommand,
    },
    Popos {
        #[command(subcommand)]
        command: PackageCommand,
    },
}

#[derive(Subcommand, Debug)]
enum PackageCommand {
    Install {
        packages: Vec<String>,
        #[arg(long)]
        yes: bool,
    },
    Remove {
        packages: Vec<String>,
        #[arg(long)]
        yes: bool,
    },
    Purge {
        packages: Vec<String>,
        #[arg(long)]
        yes: bool,
    },
    Update,
    Upgrade,
    FullUpgrade,
    Autoremove,
    Search {
        query: String,
    },
    Show {
        package: String,
    },
    List,
    Run {
        command: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.distro {
        DistroCommand::Debian { command } => dispatch(Distro::Debian, command),
        DistroCommand::Fedora { command } => dispatch(Distro::Fedora, command),
        DistroCommand::Popos { command } => dispatch(Distro::Popos, command),
    }
}

fn dispatch(distro: Distro, command: PackageCommand) -> Result<()> {
    let root = distro.root();
    let _lock = CompatLock::acquire(distro)?;
    let setup_yes = matches!(&command, PackageCommand::Install { yes: true, .. });
    ensure_environment(
        distro,
        &root,
        matches!(command, PackageCommand::Install { .. }),
        setup_yes,
    )?;
    match command {
        PackageCommand::Install { packages, yes } => {
            package_command(distro, &root, "install", packages, yes)
        }
        PackageCommand::Remove { packages, yes } => {
            package_command(distro, &root, "remove", packages, yes)
        }
        PackageCommand::Purge { packages, yes } => {
            package_command(distro, &root, "purge", packages, yes)
        }
        PackageCommand::Update => run_in_root(
            distro,
            &root,
            &package_args(distro, "update", false, &[]),
            false,
        ),
        PackageCommand::Upgrade => run_in_root(
            distro,
            &root,
            &package_args(distro, "upgrade", false, &[]),
            false,
        ),
        PackageCommand::FullUpgrade => run_in_root(
            distro,
            &root,
            &package_args(distro, "full-upgrade", false, &[]),
            false,
        ),
        PackageCommand::Autoremove => run_in_root(
            distro,
            &root,
            &package_args(distro, "autoremove", false, &[]),
            false,
        ),
        PackageCommand::Search { query } => run_in_root(
            distro,
            &root,
            &package_args(distro, "search", false, &[query]),
            false,
        ),
        PackageCommand::Show { package } => run_in_root(
            distro,
            &root,
            &package_args(distro, "show", false, &[package]),
            false,
        ),
        PackageCommand::List => run_in_root(
            distro,
            &root,
            &package_args(distro, "list", false, &[]),
            false,
        ),
        PackageCommand::Run { command } => run_application(distro, &root, &command),
    }
}

fn package_command(
    distro: Distro,
    root: &Path,
    action: &str,
    packages: Vec<String>,
    yes: bool,
) -> Result<()> {
    if packages.is_empty() {
        bail!("at least one package is required");
    }
    let args = package_args(distro, action, yes, &packages);
    let result = run_in_root(distro, root, &args, false);
    if result.is_ok() {
        generate_launchers(distro, root)?;
    }
    result
}

fn package_args(distro: Distro, action: &str, yes: bool, packages: &[String]) -> Vec<String> {
    let mut args = match distro {
        Distro::Fedora => vec!["dnf".into(), "--assumeyes".into(), action.into()],
        Distro::Debian | Distro::Popos => vec!["apt-get".into(), action.into()],
    };
    if yes && !matches!(distro, Distro::Fedora) {
        args.push("--yes".into());
    }
    args.extend(packages.iter().cloned());
    args
}

fn ensure_environment(distro: Distro, root: &Path, installing: bool, yes: bool) -> Result<()> {
    if root.join("etc/os-release").is_file() {
        return Ok(());
    }
    if !installing {
        bail!(
            "{} compatibility environment is not installed; run install first",
            distro.id()
        );
    }
    if !yes && !is_interactive() {
        bail!(
            "{} compatibility environment is missing; refusing non-interactive bootstrap",
            distro.id()
        );
    }
    if !yes {
        println!(
            "The {} compatibility environment is not installed. Create it now? [Y/n]",
            distro.id()
        );
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .context("read bootstrap confirmation")?;
        if !answer.trim().is_empty() && !answer.trim().eq_ignore_ascii_case("y") {
            bail!("compatibility environment setup cancelled");
        }
    }
    bootstrap(distro, root)?;
    run_in_root(distro, root, &bootstrap_refresh_args(distro), false)
}

fn bootstrap_refresh_args(distro: Distro) -> Vec<String> {
    match distro {
        Distro::Fedora => vec![
            "dnf".to_string(),
            "makecache".to_string(),
            "--refresh".to_string(),
        ],
        Distro::Debian | Distro::Popos => package_args(distro, "update", false, &[]),
    }
}

fn bootstrap(distro: Distro, root: &Path) -> Result<()> {
    let (url, checksum) = bootstrap_archive(distro)?;
    let temp = env::temp_dir().join(format!(
        "mattos-compat-{}-{}",
        distro.id(),
        std::process::id()
    ));
    if temp.exists() {
        bail!("bootstrap staging path already exists: {}", temp.display());
    }
    fs::create_dir(&temp).context("create bootstrap staging directory")?;
    let archive = temp.join("rootfs.archive");
    let result = (|| {
        download_verified(&url, &checksum, &archive)?;
        let unpack = temp.join("root");
        fs::create_dir(&unpack)?;
        extract_bootstrap_archive(&archive, &unpack)?;
        let extracted = find_rootfs(&unpack)?;
        let published = root
            .parent()
            .ok_or_else(|| anyhow!("compatibility root has no parent"))?
            .join(format!(
                ".{}.installing-{}",
                distro.id(),
                std::process::id()
            ));
        let status = Command::new("sudo")
            .args(["install", "-d", "-m", "0755"])
            .arg(root.parent().unwrap())
            .status()
            .context("create compatibility root")?;
        if !status.success() {
            bail!("sudo could not create compatibility root parent");
        }
        let _ = Command::new("sudo")
            .args(["rm", "-rf"])
            .arg(&published)
            .status();
        let status = Command::new("sudo")
            .args(["install", "-d", "-m", "0755"])
            .arg(&published)
            .status()
            .context("create compatibility publication staging root")?;
        if !status.success() {
            bail!("sudo could not create {}", published.display());
        }
        let status = Command::new("sudo")
            .args(["cp", "-a"])
            .arg(format!("{}/.", extracted.display()))
            .arg(&published)
            .status()
            .context("stage compatibility rootfs")?;
        if !status.success() {
            bail!("failed to stage {}", root.display());
        }
        let status = Command::new("sudo")
            .args(["mv"])
            .arg(&published)
            .arg(root)
            .status()
            .context("atomically publish compatibility rootfs")?;
        if !status.success() {
            bail!("failed to publish {}", root.display());
        }
        if matches!(distro, Distro::Popos) {
            configure_popos_sources(root)?;
        }
        Ok::<(), anyhow::Error>(())
    })();
    let _ = fs::remove_dir_all(&temp);
    result
}

fn extract_bootstrap_archive(archive: &Path, unpack: &Path) -> Result<()> {
    let status = Command::new("sudo")
        .args(["tar", "--extract", "--file"])
        .arg(archive)
        .args(["--directory"])
        .arg(unpack)
        .status()
        .context("extract compatibility rootfs")?;
    if !status.success() {
        bail!("failed to extract compatibility rootfs");
    }
    if unpack.join("etc/os-release").exists() {
        return Ok(());
    }

    // Fedora's official container artifact is an OCI image archive. Extract
    // only the manifest-declared layer; never guess from arbitrary blobs.
    let index = tar_output(archive, "index.json")?;
    let index: serde_json::Value =
        serde_json::from_slice(&index).context("parse OCI image index")?;
    let manifest_digest = index["manifests"][0]["digest"]
        .as_str()
        .ok_or_else(|| anyhow!("OCI image index has no manifest digest"))?;
    let manifest_path = format!(
        "blobs/sha256/{}",
        manifest_digest
            .strip_prefix("sha256:")
            .ok_or_else(|| anyhow!("unsupported OCI manifest digest"))?
    );
    let manifest = tar_output(archive, &manifest_path)?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&manifest).context("parse OCI image manifest")?;
    let layers = manifest["layers"]
        .as_array()
        .ok_or_else(|| anyhow!("OCI image manifest has no layers"))?;
    for layer in layers {
        let digest = layer["digest"]
            .as_str()
            .ok_or_else(|| anyhow!("OCI layer has no digest"))?;
        let path = format!(
            "blobs/sha256/{}",
            digest
                .strip_prefix("sha256:")
                .ok_or_else(|| anyhow!("unsupported OCI layer digest"))?
        );
        let layer_bytes = tar_output(archive, &path)?;
        extract_layer_bytes(unpack, &layer_bytes)?;
    }
    for metadata in ["blobs", "index.json", "oci-layout"] {
        let _ = Command::new("sudo")
            .args(["rm", "-rf"])
            .arg(unpack.join(metadata))
            .status();
    }
    Ok(())
}

fn tar_output(archive: &Path, member: &str) -> Result<Vec<u8>> {
    let output = Command::new("tar")
        .args(["--extract", "--to-stdout", "--file"])
        .arg(archive)
        .arg(member)
        .output()
        .with_context(|| format!("read {} from bootstrap archive", member))?;
    if !output.status.success() {
        bail!("bootstrap archive does not contain {}", member);
    }
    Ok(output.stdout)
}

fn extract_layer_bytes(unpack: &Path, layer: &[u8]) -> Result<()> {
    let mut child = Command::new("sudo")
        .args(["tar", "--extract", "--gzip", "--file=-", "--directory"])
        .arg(unpack)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("start OCI layer extraction")?;
    child.stdin.take().unwrap().write_all(layer)?;
    let status = child.wait()?;
    if !status.success() {
        bail!("failed to extract OCI rootfs layer");
    }
    Ok(())
}

fn bootstrap_archive(distro: Distro) -> Result<(String, String)> {
    let (url, checksum, variable) = match distro {
        Distro::Debian => (
            "https://raw.githubusercontent.com/debuerreotype/docker-debian-artifacts/bae6d64d90b4068b09ff9d8b564c2773ef5d8d83/trixie/oci/blobs/rootfs.tar.gz",
            "27ee9a8250487842a26b1ffa1215982ba9ae27010bce1997d52f9f8628578d17",
            "MATTOS_COMPAT_DEBIAN_ROOTFS_URL",
        ),
        Distro::Fedora => (
            "https://download.fedoraproject.org/pub/fedora/linux/releases/44/Container/x86_64/images/Fedora-Container-Base-Generic-44-1.7.x86_64.oci.tar.xz",
            "75200f5752a74a21a616ca9a75e25beb594e2e117a0195c54f87c0b3e3974d1b",
            "MATTOS_COMPAT_FEDORA_ROOTFS_URL",
        ),
        Distro::Popos => (
            "https://cdimage.ubuntu.com/ubuntu-base/releases/24.04/release/ubuntu-base-24.04.3-base-amd64.tar.gz",
            "6bc2cde3930ad088b3bb46fa45279e96d25bc3810f209850ecbe4722711874f9",
            "MATTOS_COMPAT_POPOS_ROOTFS_URL",
        ),
    };
    let selected = env::var(variable).unwrap_or_else(|_| url.to_owned());
    let selected_checksum = match distro {
        _ => checksum.to_owned(),
    };
    if selected_checksum.is_empty() {
        bail!("bootstrap archive has no verification checksum");
    }
    Ok((selected, selected_checksum))
}

fn download_verified(url: &str, checksum: &str, destination: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--output",
        ])
        .arg(destination)
        .arg(url)
        .status()
        .with_context(|| format!("download {url}"))?;
    if !status.success() {
        bail!("failed to download bootstrap archive");
    }
    let expected = format!("{checksum}  {}\n", destination.display());
    let mut verifier = Command::new("sha256sum");
    verifier.args(["--check", "--strict", "-"]);
    let mut child = verifier.stdin(std::process::Stdio::piped()).spawn()?;
    use std::io::Write;
    child.stdin.take().unwrap().write_all(expected.as_bytes())?;
    let status = child.wait()?;
    if !status.success() {
        bail!("bootstrap archive checksum verification failed");
    }
    Ok(())
}

fn find_rootfs(unpack: &Path) -> Result<PathBuf> {
    if unpack.join("etc/os-release").is_file() {
        return Ok(unpack.to_owned());
    }
    let mut layers = fs::read_dir(unpack)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .map(|path| path.join("layer.tar"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    layers.sort();
    if layers.is_empty() {
        bail!("bootstrap archive did not contain a root filesystem");
    }
    for layer in layers {
        let status = Command::new("sudo")
            .args(["tar", "--extract", "--file"])
            .arg(layer)
            .args(["--directory"])
            .arg(unpack)
            .status()?;
        if !status.success() {
            bail!("failed to extract OCI layer");
        }
    }
    if !unpack.join("etc/os-release").is_file() {
        bail!("bootstrap archive did not produce etc/os-release");
    }
    Ok(unpack.to_owned())
}

fn configure_popos_sources(root: &Path) -> Result<()> {
    let sources = root.join("etc/apt/sources.list.d/pop-os.list");
    let content = "deb https://apt.pop-os.org/release noble main\n";
    let parent = sources.parent().unwrap();
    let status = Command::new("sudo")
        .args(["install", "-d", "-m", "0755"])
        .arg(parent)
        .status()
        .context("create Pop!_OS sources directory")?;
    if !status.success() {
        bail!("failed to create Pop!_OS sources directory");
    }
    let mut child = Command::new("sudo")
        .args(["install", "-m", "0644", "/dev/stdin"])
        .arg(&sources)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("start sudo install for Pop!_OS sources")?;
    child.stdin.take().unwrap().write_all(content.as_bytes())?;
    let status = child.wait()?;
    if !status.success() {
        bail!("failed to configure Pop!_OS package sources");
    }
    Ok(())
}

fn run_in_root(_distro: Distro, root: &Path, args: &[String], as_user: bool) -> Result<()> {
    if args.is_empty() {
        bail!("empty compatibility command");
    }
    let home = env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
    let uid = command_output("id", &["-u"])?;
    let gid = command_output("id", &["-g"])?;
    let mut command = Command::new("sudo");
    command
        .args(["systemd-nspawn", "-D"])
        .arg(root)
        .args(["--quiet", "--as-pid2", "--private-users=no"])
        .arg("--bind-ro=/etc/resolv.conf")
        .arg("--bind=/run/user")
        .arg("--bind=/home")
        .arg(format!("--setenv=HOME={}", Path::new(&home).display()))
        .arg(format!("--setenv=XDG_RUNTIME_DIR=/run/user/{}", uid.trim()));
    for path in ["/dev/dri", "/dev/snd"] {
        if Path::new(path).exists() {
            command.arg(format!("--bind={path}"));
        }
    }
    for variable in [
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "PULSE_SERVER",
        "XAUTHORITY",
    ] {
        if let Some(value) = env::var_os(variable) {
            command.arg(format!("--setenv={variable}={}", value.to_string_lossy()));
        }
    }
    let payload = payload_args(args, as_user, uid.trim(), gid.trim());
    let status = command
        .arg("--")
        .args(&payload)
        .status()
        .context("run compatibility command with systemd-nspawn")?;
    if !status.success() {
        bail!("compatibility command failed with {status}");
    }
    Ok(())
}

fn payload_args(args: &[String], as_user: bool, uid: &str, gid: &str) -> Vec<String> {
    if !as_user {
        return args.to_vec();
    }

    // nspawn's --uid expects a user that exists in the container.  Host user
    // IDs generally do not, so drop privileges inside the container using
    // numeric IDs instead.  This also preserves the host user's supplementary
    // access boundary without requiring a synthetic passwd entry.
    let mut payload = vec![
        "/usr/bin/setpriv".to_owned(),
        format!("--reuid={uid}"),
        format!("--regid={gid}"),
        "--clear-groups".to_owned(),
        "--".to_owned(),
    ];
    payload.extend(args.iter().cloned());
    payload
}

struct CompatLock {
    _file: File,
    path: PathBuf,
}

impl CompatLock {
    fn acquire(distro: Distro) -> Result<Self> {
        let path = PathBuf::from("/tmp").join(format!("mattos-compat-{}.lock", distro.id()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("compatibility environment is busy: {}", path.display()))?;
        use std::io::Write;
        writeln!(&file, "pid={}", std::process::id())?;
        Ok(Self { _file: file, path })
    }
}

impl Drop for CompatLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn run_application(distro: Distro, root: &Path, command: &[String]) -> Result<()> {
    if command.is_empty() {
        bail!("a command is required");
    }
    run_in_root(distro, root, command, true)
}

fn generate_launchers(distro: Distro, root: &Path) -> Result<()> {
    let Some(home) = launcher_home()? else {
        return Ok(());
    };
    let launcher_dir = PathBuf::from(home).join(".local/share/applications");
    fs::create_dir_all(&launcher_dir)?;
    let applications = root.join("usr/share/applications");
    if !applications.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(applications)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("desktop") {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let Some(wrapper) = compat_desktop_entry(distro, &text) else {
            continue;
        };
        let id = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("application");
        let launcher = launcher_dir.join(format!("mattos-compat-{id}"));
        fs::write(&launcher, wrapper)?;
        restore_launcher_owner(&launcher)?;
    }
    Ok(())
}

fn launcher_home() -> Result<Option<PathBuf>> {
    if let Some(user) = env::var_os("SUDO_USER") {
        let user = user.to_string_lossy();
        if !user.is_empty() && user != "root" {
            let passwd = command_output("getent", &["passwd", &user])?;
            let home = passwd
                .split(':')
                .nth(5)
                .filter(|home| !home.is_empty())
                .map(PathBuf::from);
            if home.is_some() {
                return Ok(home);
            }
        }
    }
    Ok(env::var_os("HOME").map(PathBuf::from))
}

fn restore_launcher_owner(path: &Path) -> Result<()> {
    let (Some(uid), Some(gid)) = (env::var_os("SUDO_UID"), env::var_os("SUDO_GID")) else {
        return Ok(());
    };
    let status = Command::new("chown")
        .arg(format!(
            "{}:{}",
            uid.to_string_lossy(),
            gid.to_string_lossy()
        ))
        .arg(path)
        .status()
        .context("restore desktop launcher ownership")?;
    if !status.success() {
        bail!("failed to restore ownership of {}", path.display());
    }
    Ok(())
}

fn compat_desktop_entry(distro: Distro, source: &str) -> Option<String> {
    let mut found_exec = false;
    let mut output = Vec::new();
    for line in source.lines() {
        if let Some(exec) = line.strip_prefix("Exec=") {
            output.push(format!(
                "Exec=mattos-compat {} run -- {}",
                distro.command_name(),
                exec
            ));
            found_exec = true;
        } else {
            output.push(line.to_owned());
        }
    }
    if !found_exec {
        return None;
    }
    output.push(format!("X-MattOS-Compat-Distro={}", distro.command_name()));
    Some(format!("{}\n", output.join("\n")))
}

fn is_interactive() -> bool {
    env::var_os("MATTOS_COMPAT_NONINTERACTIVE").is_none()
        && std::io::IsTerminal::is_terminal(&std::io::stdin())
}

fn command_output(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("run {program}"))?;
    if !output.status.success() {
        bail!("{program} failed");
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distro_roots_are_isolated_and_stable() {
        assert_eq!(Distro::Debian.id(), "debian-trixie");
        assert_ne!(Distro::Debian.root(), Distro::Fedora.root());
        assert_eq!(Distro::Popos.command_name(), "popos");
    }

    #[test]
    fn package_arguments_match_each_native_manager() {
        assert_eq!(
            package_args(Distro::Debian, "install", true, &["firefox-esr".into()]),
            vec!["apt-get", "install", "--yes", "firefox-esr"]
        );
        assert_eq!(
            package_args(Distro::Fedora, "install", false, &["nano".into()]),
            vec!["dnf", "--assumeyes", "install", "nano"]
        );
    }

    #[test]
    fn bootstrap_overrides_keep_verification_required() {
        let (url, checksum) = bootstrap_archive(Distro::Debian).unwrap();
        assert!(url.starts_with("https://"));
        assert_eq!(checksum.len(), 64);
    }

    #[test]
    fn nspawn_runs_payload_as_user_without_unsupported_group_option() {
        let source = compat_desktop_entry(
            Distro::Debian,
            "[Desktop Entry]\nType=Application\nName=Firefox\nExec=firefox-esr %u\nIcon=firefox\n",
        )
        .unwrap();
        assert!(source.contains("Exec=mattos-compat debian run -- firefox-esr %u"));
        assert!(source.contains("Name=Firefox"));
        assert!(source.contains("Icon=firefox"));
        assert!(source.contains("X-MattOS-Compat-Distro=debian"));
        assert!(!source.contains("--gid"));
        let payload = payload_args(&["firefox-esr".into(), "%u".into()], true, "1000", "1000");
        assert_eq!(
            payload,
            vec![
                "/usr/bin/setpriv",
                "--reuid=1000",
                "--regid=1000",
                "--clear-groups",
                "--",
                "firefox-esr",
                "%u"
            ]
        );
    }

    #[test]
    fn desktop_entries_without_exec_are_not_published() {
        assert!(compat_desktop_entry(Distro::Fedora, "[Desktop Entry]\nName=Hidden\n").is_none());
    }
}
