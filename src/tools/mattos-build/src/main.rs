use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

mod packaging;

const AUTHORITATIVE_GRUB_CFG: &str = "src/boot/grub/grub.cfg";
const OBSOLETE_GRUB_CFG_PATHS: &[&str] = &["boot/grub/grub.cfg"];
const GRUB_SYSTEMD_ENTRY: &str = "menuentry \"MattOS (systemd)\"";
const GRUB_RESCUE_ENTRY: &str = "menuentry \"MattOS (rescue init)\"";
const GRUB_SYSTEMD_RDINIT: &str = "rdinit=/usr/lib/systemd/systemd";
const GRUB_RESCUE_RDINIT: &str = "rdinit=/usr/libexec/mattos/rescue-init";
const SAFE_IMPORT_PLACEHOLDER_FILES: &[&str] = &[".gitkeep", "README.md"];
const USERLAND_INVENTORY_PATH: &str = "usr/share/mattos/userland-commands.txt";
const INITRAMFS_ARCHIVE_OWNER: &str = "0:0";

const COREUTILS_PROVIDER: &str = "uutils/coreutils";
const GREP_PROVIDER: &str = "uutils/grep";
const SED_PROVIDER: &str = "uutils/sed";
const FINDUTILS_PROVIDER: &str = "uutils/findutils";
const DIFFUTILS_PROVIDER: &str = "uutils/diffutils";
const UTIL_LINUX_PROVIDER: &str = "util-linux";
const LINUX_PAM_PROVIDER: &str = "linux-pam";
const SHADOW_PROVIDER: &str = "shadow";
const SUDO_RS_PROVIDER: &str = "sudo-rs";
const KMOD_PROVIDER: &str = "kmod";
const PROCPS_PROVIDER: &str = "procps-ng";
const NCURSES_PROVIDER: &str = "ncurses";
const IPROUTE2_PROVIDER: &str = "iproute2";
const IPUTILS_PROVIDER: &str = "iputils";
const CURL_PROVIDER: &str = "curl";
const DBUS_BROKER_PROVIDER: &str = "dbus-broker";
const SYSTEMD_PROVIDER: &str = "systemd";
const SYSTEMD_PAM_MODULE_REL: &str = "usr/lib/x86_64-linux-gnu/security/pam_systemd.so";
const REQUIRED_PAM_MODULES: &[&str] = &[
    "pam_unix.so",
    "pam_env.so",
    "pam_nologin.so",
    "pam_rootok.so",
    "pam_permit.so",
    "pam_deny.so",
    "pam_shells.so",
    "pam_securetty.so",
];

const DIFFUTILS_EXPECTED_COMMANDS: &[&str] = &["diff", "cmp", "diff3", "sdiff"];
const DIFFUTILS_AVAILABLE_ALIASES: &[&str] = &["diff", "cmp"];

#[derive(Debug, Clone, Copy)]
struct BinaryInstallSpec {
    provider: &'static str,
    source_rel: &'static str,
    install_name: &'static str,
    command_name: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct ComponentBinarySpec {
    source_rel: &'static str,
    destination_rel: &'static str,
    command_name: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct ComponentInstallManifest {
    provider: &'static str,
    install_root_rel: &'static str,
    binaries: &'static [ComponentBinarySpec],
}

const KMOD_BINARIES: &[ComponentBinarySpec] = &[
    ComponentBinarySpec {
        source_rel: "usr/bin/kmod",
        destination_rel: "usr/bin/kmod",
        command_name: "kmod",
    },
    ComponentBinarySpec {
        source_rel: "usr/sbin/modprobe",
        destination_rel: "usr/sbin/modprobe",
        command_name: "modprobe",
    },
    ComponentBinarySpec {
        source_rel: "usr/sbin/insmod",
        destination_rel: "usr/sbin/insmod",
        command_name: "insmod",
    },
    ComponentBinarySpec {
        source_rel: "usr/sbin/rmmod",
        destination_rel: "usr/sbin/rmmod",
        command_name: "rmmod",
    },
    ComponentBinarySpec {
        source_rel: "usr/sbin/lsmod",
        destination_rel: "usr/sbin/lsmod",
        command_name: "lsmod",
    },
    ComponentBinarySpec {
        source_rel: "usr/sbin/modinfo",
        destination_rel: "usr/sbin/modinfo",
        command_name: "modinfo",
    },
    ComponentBinarySpec {
        source_rel: "usr/sbin/depmod",
        destination_rel: "usr/sbin/depmod",
        command_name: "depmod",
    },
];

const PROCPS_BINARIES: &[ComponentBinarySpec] = &[
    ComponentBinarySpec {
        source_rel: "usr/bin/ps",
        destination_rel: "usr/bin/ps",
        command_name: "ps",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/top",
        destination_rel: "usr/bin/top",
        command_name: "top",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/free",
        destination_rel: "usr/bin/free",
        command_name: "free",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/uptime",
        destination_rel: "usr/bin/uptime",
        command_name: "uptime",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/pgrep",
        destination_rel: "usr/bin/pgrep",
        command_name: "pgrep",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/pkill",
        destination_rel: "usr/bin/pkill",
        command_name: "pkill",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/pidof",
        destination_rel: "usr/bin/pidof",
        command_name: "pidof",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/watch",
        destination_rel: "usr/bin/watch",
        command_name: "watch",
    },
    ComponentBinarySpec {
        source_rel: "usr/sbin/sysctl",
        destination_rel: "usr/sbin/sysctl",
        command_name: "sysctl",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/vmstat",
        destination_rel: "usr/bin/vmstat",
        command_name: "vmstat",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/w",
        destination_rel: "usr/bin/w",
        command_name: "w",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/pmap",
        destination_rel: "usr/bin/pmap",
        command_name: "pmap",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/pwdx",
        destination_rel: "usr/bin/pwdx",
        command_name: "pwdx",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/tload",
        destination_rel: "usr/bin/tload",
        command_name: "tload",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/slabtop",
        destination_rel: "usr/bin/slabtop",
        command_name: "slabtop",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/hugetop",
        destination_rel: "usr/bin/hugetop",
        command_name: "hugetop",
    },
];

const NCURSES_BINARIES: &[ComponentBinarySpec] = &[
    ComponentBinarySpec {
        source_rel: "usr/bin/clear",
        destination_rel: "usr/bin/clear",
        command_name: "clear",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/tput",
        destination_rel: "usr/bin/tput",
        command_name: "tput",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/tic",
        destination_rel: "usr/bin/tic",
        command_name: "tic",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/toe",
        destination_rel: "usr/bin/toe",
        command_name: "toe",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/infocmp",
        destination_rel: "usr/bin/infocmp",
        command_name: "infocmp",
    },
];

const IPROUTE2_BINARIES: &[ComponentBinarySpec] = &[
    ComponentBinarySpec {
        source_rel: "usr/sbin/ip",
        destination_rel: "usr/sbin/ip",
        command_name: "ip",
    },
    ComponentBinarySpec {
        source_rel: "usr/sbin/ss",
        destination_rel: "usr/sbin/ss",
        command_name: "ss",
    },
    ComponentBinarySpec {
        source_rel: "usr/sbin/bridge",
        destination_rel: "usr/sbin/bridge",
        command_name: "bridge",
    },
    ComponentBinarySpec {
        source_rel: "usr/sbin/tc",
        destination_rel: "usr/sbin/tc",
        command_name: "tc",
    },
];

const IPUTILS_BINARIES: &[ComponentBinarySpec] = &[
    ComponentBinarySpec {
        source_rel: "usr/bin/ping",
        destination_rel: "usr/bin/ping",
        command_name: "ping",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/tracepath",
        destination_rel: "usr/bin/tracepath",
        command_name: "tracepath",
    },
];

const CURL_BINARIES: &[ComponentBinarySpec] = &[ComponentBinarySpec {
    source_rel: "usr/bin/curl",
    destination_rel: "usr/bin/curl",
    command_name: "curl",
}];

const DBUS_BROKER_BINARIES: &[ComponentBinarySpec] = &[
    ComponentBinarySpec {
        source_rel: "usr/bin/dbus-broker",
        destination_rel: "usr/bin/dbus-broker",
        command_name: "dbus-broker",
    },
    ComponentBinarySpec {
        source_rel: "usr/bin/dbus-broker-launch",
        destination_rel: "usr/bin/dbus-broker-launch",
        command_name: "dbus-broker-launch",
    },
];

const COMPONENT_INSTALL_MANIFESTS: &[ComponentInstallManifest] = &[
    ComponentInstallManifest {
        provider: KMOD_PROVIDER,
        install_root_rel: "out/build/kmod/install",
        binaries: KMOD_BINARIES,
    },
    ComponentInstallManifest {
        provider: PROCPS_PROVIDER,
        install_root_rel: "out/build/procps-ng/install",
        binaries: PROCPS_BINARIES,
    },
    ComponentInstallManifest {
        provider: NCURSES_PROVIDER,
        install_root_rel: "out/build/ncurses/install",
        binaries: NCURSES_BINARIES,
    },
    ComponentInstallManifest {
        provider: IPROUTE2_PROVIDER,
        install_root_rel: "out/build/iproute2/install",
        binaries: IPROUTE2_BINARIES,
    },
    ComponentInstallManifest {
        provider: IPUTILS_PROVIDER,
        install_root_rel: "out/build/iputils/install",
        binaries: IPUTILS_BINARIES,
    },
    ComponentInstallManifest {
        provider: CURL_PROVIDER,
        install_root_rel: "out/build/curl/install",
        binaries: CURL_BINARIES,
    },
    ComponentInstallManifest {
        provider: DBUS_BROKER_PROVIDER,
        install_root_rel: "out/build/dbus-broker/install",
        binaries: DBUS_BROKER_BINARIES,
    },
];

const TERMINFO_ENTRIES: &[&str] = &[
    "linux",
    "xterm",
    "xterm-256color",
    "screen",
    "screen-256color",
    "vt100",
];

const USERLAND_BINARY_INSTALLS: &[BinaryInstallSpec] = &[
    BinaryInstallSpec {
        provider: GREP_PROVIDER,
        source_rel: "src/userland/grep/target/release/grep",
        install_name: "grep",
        command_name: "grep",
    },
    BinaryInstallSpec {
        provider: SED_PROVIDER,
        source_rel: "src/userland/sed/target/release/sed",
        install_name: "sed",
        command_name: "sed",
    },
    BinaryInstallSpec {
        provider: FINDUTILS_PROVIDER,
        source_rel: "src/userland/findutils/target/release/find",
        install_name: "find",
        command_name: "find",
    },
    BinaryInstallSpec {
        provider: FINDUTILS_PROVIDER,
        source_rel: "src/userland/findutils/target/release/xargs",
        install_name: "xargs",
        command_name: "xargs",
    },
    BinaryInstallSpec {
        provider: FINDUTILS_PROVIDER,
        source_rel: "src/userland/findutils/target/release/locate",
        install_name: "locate",
        command_name: "locate",
    },
    BinaryInstallSpec {
        provider: FINDUTILS_PROVIDER,
        source_rel: "src/userland/findutils/target/release/updatedb",
        install_name: "updatedb",
        command_name: "updatedb",
    },
    BinaryInstallSpec {
        provider: DIFFUTILS_PROVIDER,
        source_rel: "src/userland/diffutils/target/release/diffutils",
        install_name: "diffutils",
        command_name: "diffutils",
    },
];

#[derive(Default)]
struct UserlandInventory {
    implemented_upstream: BTreeSet<String>,
    compiled: BTreeSet<String>,
    installed: BTreeSet<String>,
    intentionally_excluded: BTreeSet<String>,
    failed_compatibility: BTreeSet<String>,
}

impl UserlandInventory {
    fn add_implemented(&mut self, provider: &str, command: &str) {
        self.implemented_upstream
            .insert(format!("{provider}:{command}"));
    }

    fn add_compiled(&mut self, provider: &str, command: &str) {
        self.compiled.insert(format!("{provider}:{command}"));
    }

    fn add_installed(&mut self, provider: &str, command: &str) {
        self.installed.insert(format!("{provider}:{command}"));
    }

    fn add_excluded(&mut self, provider: &str, command: &str) {
        self.intentionally_excluded
            .insert(format!("{provider}:{command}"));
    }

    fn add_failed(&mut self, provider: &str, command: &str, reason: &str) {
        self.failed_compatibility
            .insert(format!("{provider}:{command} ({reason})"));
    }
}

#[derive(Parser, Debug)]
#[command(name = "mattos-build")]
#[command(about = "MattOS build and upstream orchestration tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Doctor,
    Upstream {
        #[command(subcommand)]
        command: UpstreamCommands,
    },
    Package {
        #[command(subcommand)]
        command: packaging::PackageCommands,
    },
    Build {
        #[arg(value_enum)]
        stage: Option<BuildStage>,
    },
    Image,
    Run,
    Clean {
        #[arg(value_enum)]
        target: Option<CleanTarget>,
    },
    #[command(hide = true)]
    BootstrapWsl {
        #[arg(long, default_value = "Ubuntu")]
        distro: String,
        #[arg(long, default_value = "~/src/MattOS")]
        repo_path: String,
        #[arg(long)]
        skip_package_install: bool,
    },
    #[command(hide = true)]
    BuildWslIso {
        #[arg(long, default_value = "Ubuntu")]
        distro: String,
        #[arg(long, default_value = "~/src/MattOS")]
        repo_path: String,
        #[arg(long)]
        skip_boot_test: bool,
    },
    #[command(hide = true)]
    CopyIsoFromWsl {
        #[arg(long, default_value = "Ubuntu")]
        distro: String,
        #[arg(long, default_value = "~/src/MattOS")]
        repo_path: String,
        #[arg(long)]
        windows_destination: Option<String>,
    },
    #[command(hide = true)]
    BootstrapWindows {
        #[arg(long, default_value = "Ubuntu")]
        distro: String,
        #[arg(long)]
        install_distro: bool,
        #[arg(long)]
        skip_package_install: bool,
    },
    #[command(hide = true)]
    Import {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        component: Option<String>,
        #[arg(long)]
        update: bool,
    },
    #[command(hide = true)]
    RunQemu,
}

#[derive(Subcommand, Debug)]
enum UpstreamCommands {
    Status,
    Import {
        #[arg(long)]
        all: bool,
        component: Option<String>,
    },
    Sync {
        #[arg(long)]
        all: bool,
        component: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum BuildStage {
    Kernel,
    Glibc,
    GccRuntime,
    Binutils,
    GccToolchain,
    Make,
    Brush,
    Coreutils,
    Grep,
    Sed,
    Findutils,
    Diffutils,
    Kmod,
    Procps,
    Ncurses,
    Iproute2,
    Iputils,
    Curl,
    Expat,
    Libcap,
    Attr,
    Tar,
    Acl,
    Zlib,
    Bzip2,
    Lz4,
    Xz,
    Xxhash,
    Zstd,
    Openssl,
    Elfutils,
    Pcre2,
    Selinux,
    Libxcrypt,
    Libmd,
    Libbsd,
    Pam,
    Shadow,
    SudoRs,
    UtilLinux,
    Systemd,
    DbusBroker,
    Dpkg,
    Apt,
    Init,
    Rootfs,
    Initramfs,
    Iso,
    All,
}

#[derive(Clone, Debug, ValueEnum)]
enum CleanTarget {
    Artifacts,
    Logs,
    Cargo,
    All,
}

#[derive(Debug, Deserialize)]
struct Sources {
    component: Vec<ComponentDef>,
}

#[derive(Debug, Deserialize, Clone)]
struct ComponentDef {
    name: String,
    repo: String,
    branch: String,
    #[serde(default)]
    revision: Option<String>,
    path: String,
    sync: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SyncState {
    component: String,
    repo: String,
    branch: String,
    imported_commit: String,
    imported_at_utc: String,
    sync_method: String,
    destination_path: String,
}

#[derive(Debug)]
struct WslStatus {
    wsl_installed: bool,
    distros: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = std::env::current_dir().context("unable to determine current directory")?;

    match cli.command {
        Commands::Doctor => doctor(),
        Commands::Upstream { command } => upstream_command(&repo_root, command),
        Commands::Package { command } => packaging::run_package_command(&repo_root, command),
        Commands::Build { stage } => build(&repo_root, stage.unwrap_or(BuildStage::All)),
        Commands::Image => build_image(&repo_root),
        Commands::Run => run_qemu(&repo_root),
        Commands::Clean { target } => clean(&repo_root, target.unwrap_or(CleanTarget::Artifacts)),
        Commands::BootstrapWsl {
            distro,
            repo_path,
            skip_package_install,
        } => bootstrap_wsl(&repo_root, &distro, &repo_path, skip_package_install),
        Commands::BuildWslIso {
            distro,
            repo_path,
            skip_boot_test,
        } => build_wsl_iso(&repo_root, &distro, &repo_path, skip_boot_test),
        Commands::CopyIsoFromWsl {
            distro,
            repo_path,
            windows_destination,
        } => copy_iso_from_wsl(
            &repo_root,
            &distro,
            &repo_path,
            windows_destination.as_deref(),
        ),
        Commands::BootstrapWindows {
            distro,
            install_distro,
            skip_package_install,
        } => bootstrap_windows(&distro, install_distro, skip_package_install),
        Commands::Import {
            all,
            component,
            update,
        } => import_sources(&repo_root, all, component, update),
        Commands::RunQemu => run_qemu(&repo_root),
    }
}

fn doctor() -> Result<()> {
    println!("MattOS doctor");

    if cfg!(windows) {
        bail!("MattOS build is Linux-native for this milestone; run doctor from Linux filesystem")
    }

    let mut missing_required = Vec::new();
    let mut broken_required = Vec::new();
    let mut missing_optional = Vec::new();
    let mut broken_optional = Vec::new();

    println!("\n[Required tools]");
    let local_tools = local_tool_env(&std::env::current_dir().context("cwd")?);
    let local_path_hint = local_tools
        .as_ref()
        .map(|e| e.tool_bin_dir.display().to_string());
    for tool in [
        "git",
        "cargo",
        "rustc",
        "make",
        "gcc",
        "g++",
        "as",
        "autoreconf",
        "autopoint",
        "gnulib-tool",
        "meson",
        "ninja",
        "gperf",
        "gawk",
        "ld",
        "objcopy",
        "objdump",
        "perl",
        "python3",
        "bc",
        "cpio",
        "gzip",
        "mformat",
        "mcopy",
        "grub-mkrescue",
        "xorriso",
        "pkg-config",
        "bash",
        "bison",
        "flex",
        "file",
        "readelf",
        "ldd",
        "rsync",
        "bindgen",
        "cmake",
        "dpkg",
        "dpkg-deb",
        "dpkg-query",
        "dpkg-scanpackages",
        "fakeroot",
        "apt-ftparchive",
        "zstd",
        "xz",
        "tar",
        "triehash",
    ] {
        if !check_host_tool_with_hint(tool, true, local_path_hint.as_deref())? {
            missing_required.push(tool);
        }
    }

    for (tool, args) in [
        ("mformat", vec!["-V"]),
        ("mcopy", vec!["-V"]),
        ("meson", vec!["--version"]),
        ("ninja", vec!["--version"]),
        ("grub-mkrescue", vec!["--version"]),
        ("xorriso", vec!["-version"]),
        ("bindgen", vec!["--version"]),
    ] {
        if missing_required.contains(&tool) {
            continue;
        }
        if let Some(message) = check_tool_runtime(tool, &args)? {
            println!("[broken]  {tool} ({message})");
            broken_required.push(tool);
        }
    }

    if let Some(message) = check_tool_runtime("python3", &["-c", "import jinja2"])? {
        println!("[broken]  python3-jinja2 ({message})");
        broken_required.push("python3-jinja2");
    }

    if let Some(message) = check_tool_runtime("pkg-config", &["--exists", "mount"])? {
        println!("[broken]  libmount-dev ({message})");
        broken_required.push("libmount-dev");
    }
    if let Some(message) = check_tool_runtime("pkg-config", &["--exists", "openssl"])? {
        println!("[broken]  libssl-dev ({message})");
        broken_required.push("libssl-dev");
    }
    if let Some(message) = check_tool_runtime("pkg-config", &["--atleast-version=2.2", "expat"])? {
        println!("[broken]  libexpat1-dev ({message})");
        broken_required.push("libexpat1-dev");
    }
    for (module, package) in [
        ("zlib", "zlib1g-dev"),
        ("liblzma", "liblzma-dev"),
        ("libzstd", "libzstd-dev"),
        ("liblz4", "liblz4-dev"),
        ("libxxhash", "libxxhash-dev"),
    ] {
        if let Some(message) = check_tool_runtime("pkg-config", &["--exists", module])? {
            println!("[broken]  {package} ({message})");
            broken_required.push(package);
        }
    }

    println!("\n[Optional tools]");
    for tool in ["qemu-system-x86_64", "clang"] {
        if !check_host_tool_with_hint(tool, false, local_path_hint.as_deref())? {
            missing_optional.push(tool);
        }
    }

    for (tool, args) in [("qemu-system-x86_64", vec!["--version"])] {
        if missing_optional.contains(&tool) {
            continue;
        }
        if let Some(message) = check_tool_runtime(tool, &args)? {
            println!("[broken]  {tool} ({message})");
            broken_optional.push(tool);
        }
    }

    let mut required_issues: Vec<&str> = Vec::new();
    required_issues.extend(missing_required.iter().copied());
    required_issues.extend(broken_required.iter().copied());
    required_issues.sort_unstable();
    required_issues.dedup();

    let mut optional_issues: Vec<&str> = Vec::new();
    optional_issues.extend(missing_optional.iter().copied());
    optional_issues.extend(broken_optional.iter().copied());
    optional_issues.sort_unstable();
    optional_issues.dedup();

    if !required_issues.is_empty() || !optional_issues.is_empty() {
        println!("\n[Suggested packages]");
        if let Some(cmd) = suggested_package_command(&required_issues, &optional_issues)? {
            println!("{cmd}");
        } else {
            println!("No package manager hint available; install missing tools manually.");
        }
    }

    if !missing_required.is_empty() {
        println!("\n[Required missing tools] {}", missing_required.join(", "));
    }
    if !broken_required.is_empty() {
        println!("[Required broken tools] {}", broken_required.join(", "));
    }

    if !missing_required.is_empty() || !broken_required.is_empty() {
        bail!("doctor detected missing or broken required prerequisites")
    }

    if !missing_optional.is_empty() || !broken_optional.is_empty() {
        println!("doctor completed with optional warnings");
    } else {
        println!("doctor completed successfully");
    }
    Ok(())
}

fn upstream_command(repo_root: &Path, command: UpstreamCommands) -> Result<()> {
    match command {
        UpstreamCommands::Status => upstream_status(repo_root),
        UpstreamCommands::Import { all, component } => {
            import_sources(repo_root, all, component, false)
        }
        UpstreamCommands::Sync { all, component } => {
            import_sources(repo_root, all, component, true)
        }
    }
}

fn upstream_status(repo_root: &Path) -> Result<()> {
    let sources = read_sources(repo_root)?;
    println!("MattOS upstream status");
    for comp in &sources.component {
        let destination = resolve_component_destination(repo_root, &comp.path)?;
        let exists = destination.join(".").exists();
        println!("\ncomponent: {}", comp.name);
        println!("  repo:      {}", comp.repo);
        println!("  branch:    {}", comp.branch);
        println!("  path:      {}", comp.path);
        println!("  present:   {}", if exists { "yes" } else { "no" });

        if let Some(state) = read_sync_state(repo_root, &comp.name)? {
            println!("  commit:    {}", state.imported_commit);
            println!("  imported:  {}", state.imported_at_utc);
        } else {
            println!("  commit:    <not imported>");
        }
    }
    Ok(())
}

fn build_image(repo_root: &Path) -> Result<()> {
    build_rootfs(repo_root)?;
    build_initramfs(repo_root)?;
    build_iso(repo_root)
}

fn clean(repo_root: &Path, target: CleanTarget) -> Result<()> {
    match target {
        CleanTarget::Artifacts => {
            remove_path_if_exists(&repo_root.join("out/build"))?;
            remove_path_if_exists(&repo_root.join("out/images"))?;
        }
        CleanTarget::Logs => {
            remove_path_if_exists(&repo_root.join("out/logs"))?;
        }
        CleanTarget::Cargo => {
            remove_path_if_exists(&repo_root.join("target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/brush/target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/coreutils/target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/grep/target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/sed/target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/findutils/target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/diffutils/target"))?;
            remove_path_if_exists(&repo_root.join("src/system/auth/sudo-rs/target"))?;
        }
        CleanTarget::All => {
            remove_path_if_exists(&repo_root.join("out"))?;
            remove_path_if_exists(&repo_root.join("target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/brush/target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/coreutils/target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/grep/target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/sed/target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/findutils/target"))?;
            remove_path_if_exists(&repo_root.join("src/userland/diffutils/target"))?;
            remove_path_if_exists(&repo_root.join("src/system/auth/sudo-rs/target"))?;
            remove_path_if_exists(&repo_root.join("upstream/.tmp"))?;
        }
    }

    println!("cleaned target: {target:?}");
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                fs::remove_dir_all(path)
                    .with_context(|| format!("failed to remove directory {}", path.display()))?;
            } else {
                fs::remove_file(path)
                    .with_context(|| format!("failed to remove file {}", path.display()))?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    }
    Ok(())
}

fn suggested_package_command(required: &[&str], optional: &[&str]) -> Result<Option<String>> {
    let os_release = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let mut all_tools: Vec<&str> = required.iter().chain(optional.iter()).copied().collect();
    all_tools.sort_unstable();
    all_tools.dedup();

    let mut package_list: Vec<&str> = Vec::new();
    for tool in all_tools {
        for pkg in packages_for_tool(tool, &os_release) {
            if !package_list.contains(&pkg) {
                package_list.push(pkg);
            }
        }
    }

    let package_list = package_list.join(" ");

    if os_release.contains("ID=ubuntu") || os_release.contains("ID=debian") {
        return Ok(Some(format!(
            "sudo apt update && sudo apt install -y {package_list}"
        )));
    }
    if os_release.contains("ID=fedora")
        || os_release.contains("ID=centos")
        || os_release.contains("ID=rhel")
    {
        return Ok(Some(format!("sudo dnf install -y {package_list}")));
    }
    if os_release.contains("ID=arch") || os_release.contains("ID_LIKE=arch") {
        return Ok(Some(format!("sudo pacman -S --needed {package_list}")));
    }

    Ok(None)
}

fn packages_for_tool<'a>(tool: &'a str, os_release: &str) -> Vec<&'a str> {
    if os_release.contains("ID=ubuntu") || os_release.contains("ID=debian") {
        return match tool {
            "grub-mkrescue" => vec!["grub-pc-bin", "grub-common"],
            "mformat" | "mcopy" => vec!["mtools"],
            "qemu-system-x86_64" => vec!["qemu-system-x86"],
            "ninja" => vec!["ninja-build"],
            "autoreconf" => vec!["autoconf", "automake", "libtool"],
            "python3-jinja2" => vec!["python3-jinja2"],
            "libexpat1-dev" => vec!["libexpat1-dev"],
            "dpkg-scanpackages" => vec!["dpkg-dev"],
            "apt-ftparchive" => vec!["apt-utils"],
            "objdump" => vec!["binutils"],
            "xz" => vec!["xz-utils"],
            _ => vec![tool],
        };
    }

    if os_release.contains("ID=fedora")
        || os_release.contains("ID=centos")
        || os_release.contains("ID=rhel")
    {
        return match tool {
            "grub-mkrescue" => vec!["grub2-tools"],
            "mformat" | "mcopy" => vec!["mtools"],
            "qemu-system-x86_64" => vec!["qemu-system-x86"],
            "python3-jinja2" => vec!["python3-jinja2"],
            _ => vec![tool],
        };
    }

    if os_release.contains("ID=arch") || os_release.contains("ID_LIKE=arch") {
        return match tool {
            "grub-mkrescue" => vec!["grub"],
            "mformat" | "mcopy" => vec!["mtools"],
            "python3-jinja2" => vec!["python-jinja"],
            _ => vec![tool],
        };
    }

    vec![tool]
}

fn check_tool_runtime(cmd: &str, args: &[&str]) -> Result<Option<String>> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute tool check: {cmd} {}", args.join(" ")))?;

    if output.status.success() {
        return Ok(None);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };

    Ok(Some(detail))
}

fn bootstrap_windows(distro: &str, install_distro: bool, skip_package_install: bool) -> Result<()> {
    if !cfg!(windows) {
        bail!("bootstrap-windows is intended for Windows hosts")
    }

    println!("MattOS Windows bootstrap");
    println!("Preferred distro: {distro}");
    println!(
        "Repository script: src/tools/bootstrap-wsl.ps1 (run in elevated PowerShell when needed)"
    );

    let status = detect_wsl_status()?;
    if !status.wsl_installed {
        bail!("WSL is not installed. Run: wsl --install")
    }

    let selected = if status.distros.is_empty() {
        println!("No WSL distribution is installed.");
        if install_distro {
            println!("> wsl --install -d {distro}");
            run_cmd(Path::new("."), "wsl", &["--install", "-d", distro])?;
            println!(
                "If installation required admin approval and did not complete, rerun exactly: wsl --install -d {distro}"
            );
            distro.to_string()
        } else {
            bail!(
                "No WSL distro installed. Install one with: wsl --install -d {}",
                distro
            )
        }
    } else {
        preferred_distro(&status.distros).ok_or_else(|| anyhow!("unable to select WSL distro"))?
    };

    println!("Selected distro: {selected}");
    if skip_package_install {
        println!("Skipping Linux package installation (--skip-package-install)");
        println!("Checking expected WSL tools (non-fatal while package install is skipped):");
        for tool in ["bash", "git", "cargo", "make"] {
            check_wsl_tool(&selected, tool, false)?;
        }
        return Ok(());
    }

    let packages = [
        "build-essential",
        "git",
        "cpio",
        "gzip",
        "xorriso",
        "grub-pc-bin",
        "grub-common",
        "qemu-system-x86",
        "curl",
        "ca-certificates",
        "pkg-config",
        "musl-tools",
    ];

    let pkg_cmd = format!(
        "sudo apt-get update && sudo apt-get install -y {}",
        packages.join(" ")
    );
    run_wsl_bash(&selected, None, &pkg_cmd)?;

    let rust_cmd =
        "command -v rustup >/dev/null 2>&1 || curl https://sh.rustup.rs -sSf | sh -s -- -y";
    run_wsl_bash(&selected, None, rust_cmd)?;

    let mut missing_required = Vec::new();
    println!("Checking required WSL tools after bootstrap:");
    for tool in ["bash", "git", "cargo", "make"] {
        if !check_wsl_tool(&selected, tool, true)? {
            missing_required.push(tool);
        }
    }
    if !missing_required.is_empty() {
        bail!(
            "missing required tools in WSL distro {}: {}",
            selected,
            missing_required.join(", ")
        );
    }

    println!("Bootstrap completed. Re-run doctor to verify prerequisites.");
    Ok(())
}

fn bootstrap_wsl(
    repo_root: &Path,
    preferred: &str,
    repo_path: &str,
    skip_package_install: bool,
) -> Result<()> {
    if !cfg!(windows) {
        bail!("bootstrap-wsl is intended to be run from Windows host")
    }

    println!("MattOS WSL bootstrap");
    let distro = require_wsl_ubuntu(preferred)?;
    println!("Using WSL distro: {distro}");

    if !skip_package_install {
        let packages = [
            "build-essential",
            "git",
            "cpio",
            "gzip",
            "xorriso",
            "grub-pc-bin",
            "grub-common",
            "qemu-system-x86",
            "curl",
            "ca-certificates",
            "pkg-config",
            "musl-tools",
            "bc",
            "bison",
            "flex",
            "libssl-dev",
            "libelf-dev",
            "rsync",
        ];
        let pkg_cmd = format!(
            "sudo apt-get update && sudo apt-get install -y {}",
            packages.join(" ")
        );
        run_wsl_bash(&distro, None, &pkg_cmd)?;

        let rust_cmd =
            "command -v rustup >/dev/null 2>&1 || curl https://sh.rustup.rs -sSf | sh -s -- -y";
        run_wsl_bash(&distro, None, rust_cmd)?;
        run_wsl_bash(
            &distro,
            None,
            "bash -lc 'source $HOME/.cargo/env 2>/dev/null || true; rustup target add x86_64-unknown-linux-musl'",
        )?;
    }

    let linux_repo = resolve_wsl_repo_path(&distro, repo_path)?;
    sync_repo_to_wsl(repo_root, &distro, &linux_repo)?;
    println!("WSL repository is ready at {linux_repo}");
    println!(
        "Kernel builds from /mnt/* are blocked by mattos-build to avoid NTFS case-collision issues."
    );
    Ok(())
}

fn build_wsl_iso(
    repo_root: &Path,
    preferred: &str,
    repo_path: &str,
    skip_boot_test: bool,
) -> Result<()> {
    if !cfg!(windows) {
        bail!("build-wsl-iso is intended to be run from Windows host")
    }

    let distro = require_wsl_ubuntu(preferred)?;
    let linux_repo = resolve_wsl_repo_path(&distro, repo_path)?;
    sync_repo_to_wsl(repo_root, &distro, &linux_repo)?;

    let repo_expr = shell_escape(&linux_repo);
    let build_cmd = format!(
        "set -euo pipefail; case {0} in /mnt/*) echo 'Refusing to build from Windows-mounted path: ' {0} >&2; exit 12;; esac; cd {0}; source $HOME/.cargo/env 2>/dev/null || true; rm -rf src/kernel/linux src/userland/brush src/userland/coreutils src/userland/grep src/userland/sed src/userland/findutils src/userland/diffutils upstream/state; mkdir -p src/kernel/linux src/userland/brush src/userland/coreutils src/userland/grep src/userland/sed src/userland/findutils src/userland/diffutils upstream/state; cargo run -p mattos-build -- import --all --update; cargo run -p mattos-build -- build all; test -f out/images/mattos-x86_64.iso",
        repo_expr
    );
    run_wsl_bash(&distro, None, &build_cmd)?;

    if !skip_boot_test {
        let repo_expr = shell_escape(&linux_repo);
        let boot_test = format!(
            "set -euo pipefail; cd {0}; if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then echo 'qemu-system-x86_64 missing in WSL'; exit 22; fi; mkdir -p out/logs; rm -f out/logs/qemu-boot-test.log; (sleep 8; printf 'echo __MATTOS_START__\npwd\nls /\necho MARK_MATTOS\nuname -s\ncat /proc/version\nmkdir -p /tmp/test\ntouch /tmp/test/file\nls /tmp/test\necho __MATTOS_BOOT_OK__\n'; sleep 2) | timeout 180s qemu-system-x86_64 -m 1024 -cdrom out/images/mattos-x86_64.iso -nographic -serial stdio -monitor none -no-reboot -no-shutdown >out/logs/qemu-boot-test.log 2>&1 || true; grep -q '^__MATTOS_START__$' out/logs/qemu-boot-test.log; grep -q '^MARK_MATTOS$' out/logs/qemu-boot-test.log; grep -q '^Linux$' out/logs/qemu-boot-test.log; grep -q '^file$' out/logs/qemu-boot-test.log; grep -q '^__MATTOS_BOOT_OK__$' out/logs/qemu-boot-test.log",
            repo_expr
        );
        run_wsl_bash(&distro, None, &boot_test)?;
    }

    copy_iso_from_wsl(repo_root, &distro, &linux_repo, None)?;
    println!("WSL build complete; ISO copied to Windows out/images/mattos-x86_64.iso");
    Ok(())
}

fn copy_iso_from_wsl(
    repo_root: &Path,
    preferred: &str,
    repo_path: &str,
    windows_destination: Option<&str>,
) -> Result<()> {
    if !cfg!(windows) {
        bail!("copy-iso-from-wsl is intended to be run from Windows host")
    }

    let distro = require_wsl_ubuntu(preferred)?;
    let linux_repo = resolve_wsl_repo_path(&distro, repo_path)?;

    let windows_dst = if let Some(dst) = windows_destination {
        PathBuf::from(dst)
    } else {
        repo_root.join("out/images/mattos-x86_64.iso")
    };

    if let Some(parent) = windows_dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create destination dir {}", parent.display()))?;
    }

    let windows_dst_abs = if windows_dst.is_absolute() {
        windows_dst
    } else {
        repo_root.join(windows_dst)
    };
    let wsl_dst = windows_path_to_wsl(&windows_dst_abs)?;
    let repo_expr = shell_escape(&linux_repo);
    let wsl_dst_expr = shell_escape(&wsl_dst);

    let copy_cmd = format!(
        "set -euo pipefail; test -f {0}/out/images/mattos-x86_64.iso; mkdir -p $(dirname {1}); cp {0}/out/images/mattos-x86_64.iso {1}",
        repo_expr, wsl_dst_expr
    );
    run_wsl_bash(&distro, None, &copy_cmd)?;
    println!("Copied ISO to {}", windows_dst_abs.display());
    Ok(())
}

fn require_wsl_ubuntu(preferred: &str) -> Result<String> {
    let status = detect_wsl_status()?;
    if !status.wsl_installed {
        bail!("WSL is not installed. Run exactly: wsl --install")
    }

    if status.distros.is_empty() {
        bail!("No WSL distro installed. Run exactly (elevated PowerShell): wsl --install -d Ubuntu")
    }

    if status
        .distros
        .iter()
        .any(|d| d.eq_ignore_ascii_case(preferred))
    {
        return Ok(preferred.to_string());
    }

    if let Some(ubuntu) = status
        .distros
        .iter()
        .find(|d| d.to_ascii_lowercase().starts_with("ubuntu"))
    {
        return Ok(ubuntu.clone());
    }

    bail!(
        "Ubuntu WSL distribution not found. Installed distros: {}. Install with: wsl --install -d Ubuntu",
        status.distros.join(", ")
    )
}

fn sync_repo_to_wsl(repo_root: &Path, distro: &str, repo_path: &str) -> Result<()> {
    let source = windows_path_to_wsl(repo_root)?;
    let source_expr = shell_escape(&source);
    let repo_expr = shell_escape(repo_path);
    let cmd = format!(
        "set -euo pipefail; case {0} in /mnt/*) echo 'Refusing Linux worktree on Windows mount: ' {0} >&2; exit 13;; esac; mkdir -p {0}; rsync -a --delete --exclude 'target/' --exclude 'upstream/.tmp/' --exclude 'src/kernel/linux/' --exclude 'src/userland/brush/' --exclude 'src/userland/coreutils/' --exclude 'src/userland/grep/' --exclude 'src/userland/sed/' --exclude 'src/userland/findutils/' --exclude 'src/userland/diffutils/' --exclude 'upstream/state/' {1}/ {0}/",
        repo_expr, source_expr
    );
    run_wsl_bash(distro, None, &cmd)
}

fn resolve_wsl_repo_path(distro: &str, repo_path: &str) -> Result<String> {
    if repo_path == "~" {
        return query_wsl_home(distro);
    }
    if let Some(rest) = repo_path.strip_prefix("~/") {
        let home = query_wsl_home(distro)?;
        return Ok(format!("{home}/{rest}"));
    }
    Ok(repo_path.to_string())
}

fn query_wsl_home(distro: &str) -> Result<String> {
    let output = Command::new("wsl")
        .args(["-d", distro, "--", "bash", "-lc", "printf %s \"$HOME\""])
        .output()
        .with_context(|| format!("failed to query HOME for distro {distro}"))?;
    if !output.status.success() {
        bail!("failed to query WSL HOME for distro {distro}")
    }
    let home = String::from_utf8(output.stdout).context("WSL HOME output was not UTF-8")?;
    if home.trim().is_empty() {
        bail!("WSL HOME resolved to empty path")
    }
    Ok(home.trim().to_string())
}

fn detect_wsl_status() -> Result<WslStatus> {
    let wsl_installed = command_exists_host("wsl")?;
    if !wsl_installed {
        return Ok(WslStatus {
            wsl_installed,
            distros: Vec::new(),
        });
    }

    let output = Command::new("wsl")
        .args(["-l", "-q"])
        .output()
        .context("failed to query WSL distributions")?;

    let mut distros = Vec::new();
    let normalized = decode_wsl_text(&output.stdout).replace('\u{0}', "\n");
    for raw in normalized.lines() {
        let mut line = raw.trim().trim_end_matches('\r').trim().to_string();
        if line.ends_with(" (Default)") {
            line = line.trim_end_matches(" (Default)").to_string();
        }
        if !line.is_empty() {
            distros.push(line);
        }
    }

    Ok(WslStatus {
        wsl_installed,
        distros,
    })
}

fn decode_wsl_text(bytes: &[u8]) -> String {
    let likely_utf16 = bytes.len() >= 2 && bytes.iter().skip(1).step_by(2).any(|b| *b == 0);
    if likely_utf16 && bytes.len() % 2 == 0 {
        let words: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&words)
    } else {
        String::from_utf8_lossy(bytes).to_string()
    }
}

fn preferred_distro(distros: &[String]) -> Option<String> {
    distros
        .iter()
        .find(|d| d.to_ascii_lowercase().starts_with("ubuntu"))
        .cloned()
        .or_else(|| distros.first().cloned())
}

fn check_host_tool_with_hint(
    cmd: &str,
    required: bool,
    local_path_hint: Option<&str>,
) -> Result<bool> {
    let found = command_exists_host(cmd)?;
    if found {
        println!("[ok]      {cmd}");
    } else if required {
        if let Some(path_hint) = local_path_hint {
            println!("[missing] {cmd} (required; also searched rootless fallback at {path_hint})");
        } else {
            println!("[missing] {cmd} (required)");
        }
    } else {
        if let Some(path_hint) = local_path_hint {
            println!("[missing] {cmd} (optional; also searched rootless fallback at {path_hint})");
        } else {
            println!("[missing] {cmd} (optional)");
        }
    }
    Ok(found)
}

fn command_exists_host(cmd: &str) -> Result<bool> {
    let status = if cfg!(windows) {
        Command::new("where").arg(cmd).status()
    } else {
        Command::new("which").arg(cmd).status()
    }
    .with_context(|| format!("failed to probe tool {cmd}"))?;
    Ok(status.success())
}

fn check_wsl_tool(distro: &str, cmd: &str, required: bool) -> Result<bool> {
    let check = format!("command -v {} >/dev/null 2>&1", shell_escape(cmd));
    let ok = run_wsl_bash_status(distro, None, &check)?;
    if ok {
        println!("[ok]      {cmd}");
    } else if required {
        println!("[missing] {cmd} (required)");
    } else {
        println!("[missing] {cmd} (optional)");
    }
    Ok(ok)
}

fn run_wsl_bash(distro: &str, cwd: Option<&Path>, cmd: &str) -> Result<()> {
    let status = run_wsl_bash_status_code(distro, cwd, cmd)?;
    if status == 0 {
        Ok(())
    } else {
        bail!("WSL command failed (exit {status}): {cmd}")
    }
}

fn run_wsl_bash_status(distro: &str, cwd: Option<&Path>, cmd: &str) -> Result<bool> {
    Ok(run_wsl_bash_status_code(distro, cwd, cmd)? == 0)
}

fn run_wsl_bash_status_code(distro: &str, cwd: Option<&Path>, cmd: &str) -> Result<i32> {
    let wrapped = if let Some(cwd_path) = cwd {
        let wsl_path = windows_path_to_wsl(cwd_path)?;
        format!("cd {} && {}", shell_escape(&wsl_path), cmd)
    } else {
        cmd.to_string()
    };

    let status = Command::new("wsl")
        .args(["-d", distro, "--", "bash", "-lc", &wrapped])
        .status()
        .with_context(|| format!("failed to run WSL command: {wrapped}"))?;

    Ok(status.code().unwrap_or(1))
}

fn windows_path_to_wsl(path: &Path) -> Result<String> {
    let s = path.to_string_lossy();
    if s.len() >= 2 && s.as_bytes()[1] == b':' {
        let drive = s
            .chars()
            .next()
            .ok_or_else(|| anyhow!("invalid Windows path"))?
            .to_ascii_lowercase();
        let rest = s[2..].replace('\\', "/");
        return Ok(format!("/mnt/{drive}{rest}"));
    }
    bail!("expected Windows absolute path, got {}", path.display())
}

fn shell_escape(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn import_sources(
    repo_root: &Path,
    all: bool,
    component: Option<String>,
    update: bool,
) -> Result<()> {
    let sources = read_sources(repo_root)?;
    let selected = select_components(&sources.component, all, component)?;

    for comp in selected {
        import_component(repo_root, comp, update)?;
    }

    Ok(())
}

fn read_sources(repo_root: &Path) -> Result<Sources> {
    let path = repo_root.join("upstream/sources.toml");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read sources file: {}", path.display()))?;
    toml::from_str(&text).context("failed to parse upstream/sources.toml")
}

fn select_components<'a>(
    components: &'a [ComponentDef],
    all: bool,
    component: Option<String>,
) -> Result<Vec<&'a ComponentDef>> {
    if all {
        return Ok(components.iter().collect());
    }

    if let Some(name) = component {
        if let Some(found) = components.iter().find(|c| c.name == name) {
            return Ok(vec![found]);
        }
        bail!("unknown component: {name}");
    }

    bail!("pass --all or --component <name>")
}

fn import_component(repo_root: &Path, comp: &ComponentDef, update: bool) -> Result<()> {
    println!(
        "Importing {} from {} ({})",
        comp.name, comp.repo, comp.branch
    );
    validate_component_name(&comp.name)?;
    let destination = resolve_component_destination(repo_root, &comp.path)?;

    fs::create_dir_all(&destination)
        .with_context(|| format!("failed to create destination: {}", destination.display()))?;

    if update {
        if let Some(prior_state) = read_sync_state(repo_root, &comp.name)? {
            if prior_state.repo != comp.repo || prior_state.branch != comp.branch {
                bail!(
                    "state mismatch for {} (repo/branch changed); inspect upstream/state/{}.toml",
                    comp.name,
                    comp.name
                )
            }
            update_component(repo_root, comp, &destination, &prior_state)
        } else if is_scaffold_directory(&destination)? {
            println!(
                "No existing sync state for {}; performing initial import into scaffold directory",
                comp.name
            );
            initial_import_component(repo_root, comp, &destination)
        } else {
            bail!(
                "missing upstream state for {}; run initial import before --update",
                comp.name
            )
        }
    } else {
        initial_import_component(repo_root, comp, &destination)
    }
}

fn is_scaffold_directory(dir: &Path) -> Result<bool> {
    if !dir.exists() {
        return Ok(true);
    }

    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        if is_safe_placeholder_entry(&entry)? {
            continue;
        }
        return Ok(false);
    }
    Ok(true)
}

fn is_safe_placeholder_entry(entry: &fs::DirEntry) -> Result<bool> {
    let name = entry.file_name();
    if !SAFE_IMPORT_PLACEHOLDER_FILES
        .iter()
        .any(|allowed| name == OsStr::new(allowed))
    {
        return Ok(false);
    }
    let meta = entry.file_type().with_context(|| {
        format!(
            "failed to inspect placeholder type for {}",
            entry.path().display()
        )
    })?;
    Ok(meta.is_file())
}

fn initial_import_component(
    repo_root: &Path,
    comp: &ComponentDef,
    destination: &Path,
) -> Result<()> {
    assert_initial_destination_safe(destination)?;

    let tmp = prepare_tmp_clone(repo_root, comp)?;
    let commit = run_cmd_capture(&tmp, "git", &["rev-parse", "HEAD"])?;

    clear_directory_contents(destination)?;
    copy_tree_excluding_dotgit(&tmp, destination)?;

    let state = SyncState {
        component: comp.name.clone(),
        repo: comp.repo.clone(),
        branch: comp.branch.clone(),
        imported_commit: commit.trim().to_owned(),
        imported_at_utc: Utc::now().to_rfc3339(),
        sync_method: comp.sync.clone(),
        destination_path: comp.path.clone(),
    };
    write_sync_state(repo_root, &comp.name, &state)?;

    fs::remove_dir_all(&tmp)
        .with_context(|| format!("failed to remove temporary directory: {}", tmp.display()))?;

    println!("Imported {} at commit {}", comp.name, state.imported_commit);
    Ok(())
}

fn assert_initial_destination_safe(destination: &Path) -> Result<()> {
    if !destination.exists() {
        return Ok(());
    }

    let mut unsafe_entries = Vec::new();
    for entry in fs::read_dir(destination)
        .with_context(|| format!("failed to inspect destination: {}", destination.display()))?
    {
        let entry = entry?;
        if is_safe_placeholder_entry(&entry)? {
            continue;
        }
        unsafe_entries.push(entry.file_name().to_string_lossy().to_string());
    }

    if !unsafe_entries.is_empty() {
        unsafe_entries.sort();
        bail!(
            "initial import refused: destination {} contains non-placeholder files: {}",
            destination.display(),
            unsafe_entries.join(", ")
        )
    }

    Ok(())
}

fn update_component(
    repo_root: &Path,
    comp: &ComponentDef,
    destination: &Path,
    prior_state: &SyncState,
) -> Result<()> {
    let tmp_upstream = prepare_tmp_clone(repo_root, comp)?;
    let new_commit = run_cmd_capture(&tmp_upstream, "git", &["rev-parse", "HEAD"])?;

    let old_commit = prior_state.imported_commit.trim();
    run_cmd(
        &tmp_upstream,
        "git",
        &["fetch", "--depth", "1", "origin", old_commit],
    )?;

    let tmp_root = repo_root.join("upstream/.tmp");
    let tmp_merge = tmp_root.join(format!("{}-merge", comp.name));
    if tmp_merge.exists() {
        fs::remove_dir_all(&tmp_merge)
            .with_context(|| format!("failed to clean {}", tmp_merge.display()))?;
    }
    fs::create_dir_all(&tmp_merge)
        .with_context(|| format!("failed to create {}", tmp_merge.display()))?;

    run_cmd(&tmp_merge, "git", &["init"])?;
    run_cmd(
        &tmp_merge,
        "git",
        &[
            "remote",
            "add",
            "upstream",
            tmp_upstream
                .to_str()
                .ok_or_else(|| anyhow!("invalid path: {}", tmp_upstream.display()))?,
        ],
    )?;
    run_cmd(&tmp_merge, "git", &["fetch", "upstream", old_commit])?;
    run_cmd(&tmp_merge, "git", &["fetch", "upstream", new_commit.trim()])?;
    run_cmd(
        &tmp_merge,
        "git",
        &["checkout", "-q", "-b", "local", old_commit],
    )?;

    clear_directory_contents(&tmp_merge)?;
    copy_tree_excluding_dotgit(destination, &tmp_merge)?;
    run_cmd(&tmp_merge, "git", &["add", "-A"])?;
    run_cmd(
        &tmp_merge,
        "git",
        &[
            "-c",
            "user.name=MattOS Sync Bot",
            "-c",
            "user.email=syncbot@example.invalid",
            "commit",
            "-m",
            "MattOS local snapshot before upstream sync",
        ],
    )?;

    let merge_status = run_cmd_status(
        &tmp_merge,
        "git",
        &["merge", "--no-commit", "--no-ff", new_commit.trim()],
    )?;
    let has_conflicts = merge_status.code() == Some(1);
    if !merge_status.success() && !has_conflicts {
        bail!(
            "sync merge failed unexpectedly with status {}",
            merge_status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
    }

    clear_directory_contents(destination)?;
    copy_tree_excluding_dotgit(&tmp_merge, destination)?;

    fs::remove_dir_all(&tmp_upstream)
        .with_context(|| format!("failed to remove {}", tmp_upstream.display()))?;
    fs::remove_dir_all(&tmp_merge)
        .with_context(|| format!("failed to remove {}", tmp_merge.display()))?;

    if has_conflicts {
        bail!(
            "upstream sync for {} produced merge conflicts under {}; resolve conflicts and rerun --update",
            comp.name,
            comp.path
        );
    }

    let state = SyncState {
        component: comp.name.clone(),
        repo: comp.repo.clone(),
        branch: comp.branch.clone(),
        imported_commit: new_commit.trim().to_owned(),
        imported_at_utc: Utc::now().to_rfc3339(),
        sync_method: comp.sync.clone(),
        destination_path: comp.path.clone(),
    };
    write_sync_state(repo_root, &comp.name, &state)?;

    println!("Updated {} to commit {}", comp.name, state.imported_commit);
    Ok(())
}

fn validate_component_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("component name must not be empty")
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("component name contains unsupported characters: {name}")
    }
    Ok(())
}

fn resolve_component_destination(repo_root: &Path, rel_path: &str) -> Result<PathBuf> {
    if rel_path.contains('\\') {
        bail!("component path must use forward slashes only: {rel_path}")
    }

    let rel = Path::new(rel_path);
    if rel.is_absolute() {
        bail!("component path must be relative: {rel_path}")
    }
    for piece in rel.components() {
        match piece {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => bail!("component path cannot contain '..': {rel_path}"),
            Component::RootDir | Component::Prefix(_) => {
                bail!("component path has invalid prefix/root: {rel_path}")
            }
        }
    }

    let joined = repo_root.join(rel);
    if !joined.starts_with(repo_root) {
        bail!("component path escapes repository root: {rel_path}")
    }
    Ok(joined)
}

fn read_sync_state(repo_root: &Path, name: &str) -> Result<Option<SyncState>> {
    let path = repo_root
        .join("upstream/state")
        .join(format!("{name}.toml"));
    if !path.exists() {
        return Ok(None);
    }
    let body = fs::read_to_string(&path)
        .with_context(|| format!("failed to read sync state: {}", path.display()))?;
    let state = toml::from_str::<SyncState>(&body)
        .with_context(|| format!("failed to parse sync state: {}", path.display()))?;
    Ok(Some(state))
}

fn prepare_tmp_clone(repo_root: &Path, comp: &ComponentDef) -> Result<PathBuf> {
    let tmp_base = repo_root.join("upstream/.tmp");
    fs::create_dir_all(&tmp_base).context("failed to create temporary import directory")?;
    let tmp = tmp_base.join(format!("{}-clone", comp.name));
    if tmp.exists() {
        fs::remove_dir_all(&tmp)
            .with_context(|| format!("failed to remove previous temp dir: {}", tmp.display()))?;
    }

    run_cmd(
        repo_root,
        "git",
        &[
            "clone",
            "--depth",
            "1",
            "--branch",
            &comp.branch,
            &comp.repo,
            tmp.to_str().ok_or_else(|| anyhow!("invalid temp path"))?,
        ],
    )?;
    if let Some(revision) = comp.revision.as_deref() {
        run_cmd(&tmp, "git", &["fetch", "--depth", "1", "origin", revision])?;
        run_cmd(&tmp, "git", &["checkout", "--detach", revision])?;
    }

    Ok(tmp)
}

fn clear_directory_contents(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(dir).with_context(|| format!("failed to read directory: {}", dir.display()))?
    {
        let entry = entry?;
        let p = entry.path();
        if p.file_name() == Some(OsStr::new(".git")) {
            continue;
        }
        if p.is_dir() {
            fs::remove_dir_all(&p)
                .with_context(|| format!("failed to remove directory: {}", p.display()))?;
        } else {
            fs::remove_file(&p)
                .with_context(|| format!("failed to remove file: {}", p.display()))?;
        }
    }
    Ok(())
}

fn copy_tree_excluding_dotgit(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("failed to create copy destination: {}", dst.display()))?;
    for entry in fs::read_dir(src)
        .with_context(|| format!("failed to read source dir: {}", src.display()))?
    {
        let entry = entry?;
        let from = entry.path();
        let name = entry.file_name();
        let metadata = fs::symlink_metadata(&from)
            .with_context(|| format!("failed to read metadata: {}", from.display()))?;

        if name == OsStr::new(".git") {
            continue;
        }

        let to = dst.join(&name);
        if metadata.file_type().is_symlink() {
            copy_symlink(&from, &to)?;
        } else if metadata.is_dir() {
            copy_tree_excluding_dotgit(&from, &to)?;
        } else {
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
            preserve_permissions(&metadata, &to)?;
        }
    }
    Ok(())
}

fn copy_tree_excluding_package_owned(
    src: &Path,
    rootfs: &Path,
    owned: &BTreeSet<PathBuf>,
) -> Result<()> {
    fn copy_inner(src: &Path, dst: &Path, rootfs: &Path, owned: &BTreeSet<PathBuf>) -> Result<()> {
        fs::create_dir_all(dst)
            .with_context(|| format!("failed to create copy destination: {}", dst.display()))?;
        let mut entries = fs::read_dir(src)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let from = entry.path();
            if entry.file_name() == OsStr::new(".git") {
                continue;
            }
            let to = dst.join(entry.file_name());
            let metadata = fs::symlink_metadata(&from)?;
            if metadata.is_dir() {
                copy_inner(&from, &to, rootfs, owned)?;
                continue;
            }
            let rel = to.strip_prefix(rootfs)?;
            if owned.contains(rel) {
                continue;
            }
            if metadata.file_type().is_symlink() {
                copy_symlink(&from, &to)?;
            } else {
                fs::copy(&from, &to)?;
                preserve_permissions(&metadata, &to)?;
            }
        }
        Ok(())
    }

    copy_inner(src, rootfs, rootfs, owned)
}

#[cfg(unix)]
fn copy_symlink(from: &Path, to: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    if to.exists() {
        fs::remove_file(to).with_context(|| format!("failed to remove {}", to.display()))?;
    }
    let target = fs::read_link(from)
        .with_context(|| format!("failed to read symlink {}", from.display()))?;
    symlink(&target, to).with_context(|| format!("failed to create symlink {}", to.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn copy_symlink(from: &Path, to: &Path) -> Result<()> {
    let target = fs::read_link(from)
        .with_context(|| format!("failed to read symlink {}", from.display()))?;
    let parent = to
        .parent()
        .ok_or_else(|| anyhow!("missing parent for {}", to.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create parent {}", parent.display()))?;
    let resolved = from
        .parent()
        .ok_or_else(|| anyhow!("missing parent for {}", from.display()))?
        .join(target);
    fs::copy(&resolved, to)
        .with_context(|| format!("failed to copy symlink fallback {}", resolved.display()))?;
    Ok(())
}

#[cfg(unix)]
fn preserve_permissions(metadata: &fs::Metadata, to: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = metadata.permissions().mode();
    fs::set_permissions(to, fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to set permissions on {}", to.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn preserve_permissions(_metadata: &fs::Metadata, _to: &Path) -> Result<()> {
    Ok(())
}

fn write_sync_state(repo_root: &Path, name: &str, state: &SyncState) -> Result<()> {
    let dir = repo_root.join("upstream/state");
    fs::create_dir_all(&dir).context("failed to create upstream/state")?;
    let path = dir.join(format!("{name}.toml"));
    let temp_path = dir.join(format!("{name}.toml.tmp"));
    let body = toml::to_string_pretty(state).context("failed to serialize sync state")?;
    fs::write(&temp_path, body).with_context(|| {
        format!(
            "failed to write temporary sync state: {}",
            temp_path.display()
        )
    })?;
    fs::rename(&temp_path, &path)
        .with_context(|| format!("failed to publish sync state: {}", path.display()))?;
    Ok(())
}

fn build(repo_root: &Path, stage: BuildStage) -> Result<()> {
    let rebuild_all_consumers = stage == BuildStage::All;
    for next in build_plan(stage) {
        if rebuild_all_consumers && next == BuildStage::Brush {
            reset_native_consumer_outputs(repo_root)?;
        }
        build_stage(repo_root, next)?;
    }
    Ok(())
}

fn reset_native_consumer_outputs(repo_root: &Path) -> Result<()> {
    let output = repo_root.join("out/build");
    if !output.is_dir() {
        return Ok(());
    }
    let mut entries = fs::read_dir(&output)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if matches!(
            entry.file_name().to_str(),
            Some("glibc")
                | Some("gcc-runtime")
                | Some("binutils")
                | Some("gcc-toolchain")
                | Some("make")
        ) {
            continue;
        }
        remove_path_if_exists(&entry.path())?;
    }
    println!(
        "cleared native consumer outputs so every post-glibc stage rebuilds against {}",
        repo_root.join("out/sysroot").display()
    );
    Ok(())
}

fn build_plan(stage: BuildStage) -> Vec<BuildStage> {
    if stage == BuildStage::All {
        return vec![
            BuildStage::Kernel,
            BuildStage::Glibc,
            BuildStage::GccRuntime,
            BuildStage::Binutils,
            BuildStage::GccToolchain,
            BuildStage::Make,
            BuildStage::Brush,
            BuildStage::Coreutils,
            BuildStage::Grep,
            BuildStage::Sed,
            BuildStage::Findutils,
            BuildStage::Diffutils,
            BuildStage::Expat,
            BuildStage::Libcap,
            BuildStage::Attr,
            BuildStage::Acl,
            BuildStage::Zlib,
            BuildStage::Bzip2,
            BuildStage::Lz4,
            BuildStage::Xz,
            BuildStage::Xxhash,
            BuildStage::Zstd,
            BuildStage::Openssl,
            BuildStage::Elfutils,
            BuildStage::Pcre2,
            BuildStage::Selinux,
            BuildStage::Libxcrypt,
            BuildStage::Libmd,
            BuildStage::Libbsd,
            BuildStage::Tar,
            BuildStage::Ncurses,
            BuildStage::Procps,
            BuildStage::Iproute2,
            BuildStage::Iputils,
            BuildStage::Curl,
            BuildStage::Pam,
            BuildStage::UtilLinux,
            BuildStage::Kmod,
            BuildStage::Shadow,
            BuildStage::SudoRs,
            BuildStage::Systemd,
            BuildStage::DbusBroker,
            BuildStage::Dpkg,
            BuildStage::Apt,
            BuildStage::Init,
            BuildStage::Rootfs,
            BuildStage::Initramfs,
            BuildStage::Iso,
        ];
    }

    vec![stage]
}

fn build_stage(repo_root: &Path, stage: BuildStage) -> Result<()> {
    match stage {
        BuildStage::Kernel => build_kernel(repo_root),
        BuildStage::Glibc => build_glibc(repo_root),
        BuildStage::GccRuntime => build_gcc_runtime(repo_root),
        BuildStage::Binutils => build_binutils(repo_root),
        BuildStage::GccToolchain => build_gcc_toolchain(repo_root),
        BuildStage::Make => build_make(repo_root),
        BuildStage::Brush => build_brush(repo_root),
        BuildStage::Coreutils => build_coreutils(repo_root),
        BuildStage::Grep => build_grep(repo_root),
        BuildStage::Sed => build_sed(repo_root),
        BuildStage::Findutils => build_findutils(repo_root),
        BuildStage::Diffutils => build_diffutils(repo_root),
        BuildStage::Kmod => build_kmod(repo_root),
        BuildStage::Procps => build_procps(repo_root),
        BuildStage::Ncurses => build_ncurses(repo_root),
        BuildStage::Iproute2 => build_iproute2(repo_root),
        BuildStage::Iputils => build_iputils(repo_root),
        BuildStage::Curl => build_curl(repo_root),
        BuildStage::Expat => build_expat(repo_root),
        BuildStage::Libcap => build_libcap(repo_root),
        BuildStage::Attr => build_attr(repo_root),
        BuildStage::Tar => build_tar(repo_root),
        BuildStage::Acl => build_acl(repo_root),
        BuildStage::Zlib => build_zlib(repo_root),
        BuildStage::Bzip2 => build_bzip2(repo_root),
        BuildStage::Lz4 => build_lz4(repo_root),
        BuildStage::Xz => build_xz(repo_root),
        BuildStage::Xxhash => build_xxhash(repo_root),
        BuildStage::Zstd => build_zstd(repo_root),
        BuildStage::Openssl => build_openssl(repo_root),
        BuildStage::Elfutils => build_elfutils(repo_root),
        BuildStage::Pcre2 => build_pcre2(repo_root),
        BuildStage::Selinux => build_selinux(repo_root),
        BuildStage::Libxcrypt => build_libxcrypt(repo_root),
        BuildStage::Libmd => build_libmd(repo_root),
        BuildStage::Libbsd => build_libbsd(repo_root),
        BuildStage::Pam => build_linux_pam(repo_root),
        BuildStage::Shadow => build_shadow(repo_root),
        BuildStage::SudoRs => build_sudo_rs(repo_root),
        BuildStage::UtilLinux => build_util_linux(repo_root),
        BuildStage::Systemd => build_systemd(repo_root),
        BuildStage::DbusBroker => build_dbus_broker(repo_root),
        BuildStage::Dpkg => packaging::build_dpkg(repo_root),
        BuildStage::Apt => packaging::build_apt(repo_root),
        BuildStage::Init => build_init(repo_root),
        BuildStage::Rootfs => build_rootfs(repo_root),
        BuildStage::Initramfs => build_initramfs(repo_root),
        BuildStage::Iso => build_iso(repo_root),
        BuildStage::All => {
            bail!("internal error: BuildStage::All should be expanded by build_plan")
        }
    }
}

fn build_kernel(repo_root: &Path) -> Result<()> {
    assert_kernel_build_path_safe(repo_root)?;
    let linux = repo_root.join("src/kernel/linux");
    let config = repo_root.join("src/kernel/config/x86_64_mattos.config");
    if !linux.join("Makefile").exists() {
        bail!(
            "kernel source not found in {}; run import first",
            linux.display()
        );
    }
    if !config.exists() {
        bail!(
            "kernel config missing at {}; add configuration first",
            config.display()
        );
    }

    let config_text = fs::read_to_string(&config)
        .with_context(|| format!("failed to read {}", config.display()))?;
    fs::write(linux.join(".config"), config_text)
        .with_context(|| format!("failed to stage kernel config from {}", config.display()))?;

    let env = local_tool_env(repo_root);
    if let Some(env) = &env {
        println!(
            "Using local rootless toolchain from {}",
            env.tool_root.display()
        );
    }
    run_cmd_with_env(&linux, "make", &["olddefconfig"], env.as_ref())?;
    run_cmd_with_env(&linux, "make", &["-j", "4"], env.as_ref()).context("kernel build failed")?;

    let bz = linux.join("arch/x86/boot/bzImage");
    if !bz.exists() {
        bail!("kernel build finished without bzImage at {}", bz.display())
    }
    Ok(())
}

const GLIBC_MINIMUM_KERNEL: &str = "5.10.0";
const MATTOS_SOURCE_DATE_EPOCH: &str = "1767225600";

fn build_glibc(repo_root: &Path) -> Result<()> {
    let linux = repo_root.join("src/kernel/linux");
    let source = repo_root.join("src/system/libc/glibc");
    let output = repo_root.join("out/build/glibc");
    let build = output.join("build");
    let install = output.join("install");
    let sysroot = repo_root.join("out/sysroot");
    let headers_root = sysroot.join("usr");
    if !linux.join("Makefile").is_file() {
        bail!(
            "Linux source not found at {}; import it first",
            linux.display()
        )
    }
    if !source.join("configure").is_file() {
        bail!(
            "glibc source not found at {}; run `mattos-build upstream import glibc`",
            source.display()
        )
    }

    remove_path_if_exists(&output)?;
    remove_path_if_exists(&sysroot)?;
    fs::create_dir_all(&build)?;
    fs::create_dir_all(&install)?;
    fs::create_dir_all(&headers_root)?;

    let headers_arg = format!("INSTALL_HDR_PATH={}", headers_root.display());
    run_cmd(
        &linux,
        "make",
        &["ARCH=x86", "headers_install", headers_arg.as_str()],
    )
    .context("Linux UAPI header generation failed")?;
    if !sysroot.join("usr/include/linux/version.h").is_file()
        || !sysroot.join("usr/include/asm/unistd.h").is_file()
    {
        bail!("Linux headers_install did not create the required UAPI header tree")
    }
    copy_tree_contents(
        &sysroot.join("usr/include"),
        &output.join("linux-headers/usr/include"),
    )?;
    let mut uapi_files = Vec::new();
    collect_regular_files(&output.join("linux-headers/usr/include"), &mut uapi_files)?;
    let mut uapi_inventory = String::from(
        "revision=f17f39c917cd4aac09db1a6a083ef5ec09b4924d\narchitecture=x86\ncommand=make ARCH=x86 headers_install\n\n",
    );
    for path in uapi_files {
        uapi_inventory.push_str(
            path.strip_prefix(output.join("linux-headers"))?
                .to_string_lossy()
                .as_ref(),
        );
        uapi_inventory.push('\n');
    }
    fs::write(output.join("linux-headers-inventory.txt"), uapi_inventory)?;

    let configure = source.join("configure");
    let headers = sysroot.join("usr/include");
    let glibc_cflags = format!(
        "-O2 -g0 -ffile-prefix-map={}=/usr/src/mattos/glibc -fdebug-prefix-map={}=/usr/src/mattos/glibc",
        repo_root.display(),
        repo_root.display()
    );
    let configure_text = format!(
        "CFLAGS='{}' {} \\\n+  --prefix=/usr \\\n+  --libdir=/usr/lib/x86_64-linux-gnu \\\n+  --libexecdir=/usr/libexec \\\n+  --build=x86_64-pc-linux-gnu \\\n+  --host=x86_64-pc-linux-gnu \\\n+  --enable-kernel={} \\\n+  --with-headers={} \\\n+  --without-selinux \\\n+  --disable-werror \\\n+  --disable-profile \\\n+  --disable-build-nscd \\\n+  --disable-nscd \\\n+  --enable-stack-protector=strong \\\n+  --enable-bind-now\n",
        glibc_cflags,
        configure.display(),
        GLIBC_MINIMUM_KERNEL,
        headers.display()
    );
    fs::write(output.join("configure-invocation.txt"), &configure_text)?;
    fs::write(
        output.join("kernel-headers-source.txt"),
        "source=src/kernel/linux\nrevision=f17f39c917cd4aac09db1a6a083ef5ec09b4924d\nmethod=make ARCH=x86 headers_install\n",
    )?;

    let configure_program = configure
        .to_str()
        .ok_or_else(|| anyhow!("glibc configure path is not UTF-8"))?;
    let headers_option = format!("--with-headers={}", headers.display());
    let kernel_option = format!("--enable-kernel={GLIBC_MINIMUM_KERNEL}");
    let configure_args = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--libexecdir=/usr/libexec",
        "--build=x86_64-pc-linux-gnu",
        "--host=x86_64-pc-linux-gnu",
        kernel_option.as_str(),
        headers_option.as_str(),
        "--without-selinux",
        "--disable-werror",
        "--disable-profile",
        "--disable-build-nscd",
        "--disable-nscd",
        "--enable-stack-protector=strong",
        "--enable-bind-now",
    ];
    let configure_env = [
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("LC_ALL", "C".to_string()),
        ("TZ", "UTC".to_string()),
        ("CFLAGS", glibc_cflags),
        ("libc_cv_slibdir", "/usr/lib/x86_64-linux-gnu".to_string()),
        ("libc_cv_rtlddir", "/lib64".to_string()),
    ];
    run_cmd_with_env_overrides(&build, configure_program, &configure_args, &configure_env)
        .context("glibc configure failed")?;

    let config_make = fs::read_to_string(build.join("config.make"))?;
    if !config_make.contains(&format!("sysheaders = {}", headers.display())) {
        bail!("glibc config.make does not select the controlled MattOS UAPI headers")
    }
    run_cmd_with_env_overrides(
        &build,
        "make",
        &["-j", "4"],
        &[
            ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
            ("LC_ALL", "C".to_string()),
            ("TZ", "UTC".to_string()),
        ],
    )
    .context("glibc build failed")?;
    let install_root = format!("install_root={}", install.display());
    run_cmd_with_env_overrides(
        &build,
        "make",
        &["install", install_root.as_str()],
        &[
            ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
            ("LC_ALL", "C".to_string()),
            ("TZ", "UTC".to_string()),
        ],
    )
    .context("glibc install failed")?;

    for relative in [
        "lib64/ld-linux-x86-64.so.2",
        "usr/lib/x86_64-linux-gnu/libc.so.6",
        "usr/lib/x86_64-linux-gnu/libm.so.6",
        "usr/lib/x86_64-linux-gnu/libnss_files.so.2",
        "usr/lib/x86_64-linux-gnu/libnss_dns.so.2",
        "usr/lib/x86_64-linux-gnu/libresolv.so.2",
        "usr/bin/getent",
    ] {
        if !install.join(relative).is_file() {
            bail!("glibc install is missing required artifact /{relative}")
        }
    }
    copy_tree_contents(&install, &sysroot)?;
    println!(
        "glibc runtime and development sysroot installed in {}",
        sysroot.display()
    );
    Ok(())
}

const GCC_RUNTIME_TARGET: &str = "x86_64-pc-linux-gnu";
const GCC_RUNTIME_LIBSTDCXX_ABI: &str = "libstdc++.so.6.0.34";
const GCC_RUNTIME_REPRESENTATIVE_CONSUMERS: &[&str] = &[
    "usr/bin/apt",
    "usr/bin/apt-get",
    "usr/bin/dpkg",
    "usr/bin/curl",
    "usr/lib/systemd/systemd",
    "usr/bin/dbus-broker",
    "usr/bin/brush",
    "usr/bin/sudo",
    "usr/bin/login",
    "usr/libexec/mattos/rescue-init",
];

fn run_gcc_bootstrap_command(
    cwd: &Path,
    program: &Path,
    args: &[&str],
    env: &[(&str, String)],
) -> Result<()> {
    println!("> {} {}", program.display(), args.join(" "));
    let mut command = Command::new(program);
    command.current_dir(cwd).args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    let status = command.status().with_context(|| {
        format!(
            "failed to spawn GCC bootstrap command {}",
            program.display()
        )
    })?;
    if !status.success() {
        bail!(
            "GCC bootstrap command failed with {status}: {} {}",
            program.display(),
            args.join(" ")
        )
    }
    Ok(())
}

fn find_unique_file_named(root: &Path, name: &str) -> Result<PathBuf> {
    let mut files = Vec::new();
    collect_regular_files(root, &mut files)?;
    let matches = files
        .into_iter()
        .filter(|path| path.file_name().and_then(OsStr::to_str) == Some(name))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        bail!(
            "expected exactly one {name} below {}, found {}",
            root.display(),
            matches.len()
        )
    }
    Ok(matches.into_iter().next().unwrap())
}

fn elf_version_names(path: &Path, prefixes: &[&str]) -> Result<BTreeSet<String>> {
    let output = Command::new("readelf")
        .args(["--version-info"])
        .arg(path)
        .output()
        .with_context(|| format!("failed to inspect symbol versions in {}", path.display()))?;
    if !output.status.success() {
        bail!(
            "readelf cannot inspect symbol versions in {}",
            path.display()
        )
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut versions = BTreeSet::new();
    for word in text.split_whitespace() {
        for prefix in prefixes {
            if let Some(start) = word.find(prefix) {
                versions.insert(
                    word[start..]
                        .trim_matches(|ch: char| {
                            !ch.is_ascii_alphanumeric() && ch != '_' && ch != '.'
                        })
                        .to_string(),
                );
            }
        }
    }
    Ok(versions)
}

fn elf_needed_names(path: &Path) -> Result<BTreeSet<String>> {
    let output = Command::new("readelf")
        .args(["-d"])
        .arg(path)
        .output()
        .with_context(|| {
            format!(
                "failed to inspect dynamic dependencies in {}",
                path.display()
            )
        })?;
    if !output.status.success() {
        bail!(
            "readelf cannot inspect dynamic dependencies in {}",
            path.display()
        )
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("(NEEDED)"))
        .filter_map(|line| {
            line.split('[')
                .nth(1)
                .and_then(|part| part.split(']').next())
                .map(str::to_string)
        })
        .collect())
}

fn validate_gcc_runtime_consumers(repo_root: &Path, sysroot: &Path, runtime: &Path) -> Result<()> {
    let existing_rootfs = repo_root.join("out/build/rootfs");
    if !GCC_RUNTIME_REPRESENTATIVE_CONSUMERS
        .iter()
        .all(|relative| existing_rootfs.join(relative).is_file())
    {
        println!(
            "previous rootfs is unavailable; representative GCC runtime loader checks are deferred to final rootfs validation"
        );
        return Ok(());
    }
    let loader = sysroot.join("lib64/ld-linux-x86-64.so.2");
    let library_path = std::env::join_paths([
        runtime.to_path_buf(),
        existing_rootfs.join("usr/lib/x86_64-linux-gnu"),
        existing_rootfs.join("usr/lib/x86_64-linux-gnu/systemd"),
        existing_rootfs.join("usr/lib"),
    ])?;
    for relative in GCC_RUNTIME_REPRESENTATIVE_CONSUMERS {
        let program = existing_rootfs.join(relative);
        let listed = Command::new(&loader)
            .arg("--library-path")
            .arg(&library_path)
            .arg("--list")
            .arg(&program)
            .output()
            .with_context(|| format!("failed isolated loader validation for /{relative}"))?;
        let output = format!(
            "{}{}",
            String::from_utf8_lossy(&listed.stdout),
            String::from_utf8_lossy(&listed.stderr)
        );
        if !listed.status.success() || output.contains("not found") {
            bail!("isolated GCC runtime loader validation failed for /{relative}: {output}")
        }
        if output.lines().any(|line| {
            line.split("=>")
                .nth(1)
                .and_then(|part| part.split_whitespace().next())
                .is_some_and(|resolved| {
                    resolved.starts_with('/')
                        && !Path::new(resolved).starts_with(runtime)
                        && !Path::new(resolved).starts_with(&existing_rootfs)
                        && !Path::new(resolved).starts_with(sysroot)
                })
        }) {
            bail!(
                "isolated GCC runtime loader validation used a host library for /{relative}: {output}"
            )
        }
    }
    let rescue = existing_rootfs.join("usr/libexec/mattos/rescue-init");
    if !elf_needed_names(&rescue)?.contains("libgcc_s.so.1") {
        bail!("Rust rescue-init no longer preserves its libgcc_s unwind dependency")
    }
    println!(
        "validated {} representative consumers against the MattOS GCC runtime before rootfs replacement",
        GCC_RUNTIME_REPRESENTATIVE_CONSUMERS.len()
    );
    Ok(())
}

fn build_gcc_runtime(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/toolchain/gcc");
    let output = repo_root.join("out/build/gcc-runtime");
    let build = output.join("build");
    let raw_install = output.join("install");
    let runtime = output.join("runtime/usr/lib/x86_64-linux-gnu");
    let sysroot = repo_root.join("out/sysroot");
    if !source.join("configure").is_file() {
        bail!(
            "GCC source not found at {}; run `mattos-build upstream import gcc`",
            source.display()
        )
    }
    if !sysroot.join("usr/lib/x86_64-linux-gnu/libc.so.6").is_file()
        || !sysroot.join("usr/include/stdio.h").is_file()
    {
        bail!("GCC runtime build requires the completed MattOS glibc sysroot")
    }

    remove_path_if_exists(&output)?;
    fs::create_dir_all(&build)?;
    fs::create_dir_all(&raw_install)?;
    fs::create_dir_all(&runtime)?;

    let configure = source.join("configure");
    let sysroot_option = format!("--with-sysroot={}", sysroot.display());
    let build_sysroot_option = format!("--with-build-sysroot={}", sysroot.display());
    let build_triplet = format!("--build={GCC_RUNTIME_TARGET}");
    let host_triplet = format!("--host={GCC_RUNTIME_TARGET}");
    let target_triplet = format!("--target={GCC_RUNTIME_TARGET}");
    let configure_args = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--libexecdir=/usr/libexec",
        "--with-toolexeclibdir=/usr/lib/x86_64-linux-gnu",
        build_triplet.as_str(),
        host_triplet.as_str(),
        target_triplet.as_str(),
        sysroot_option.as_str(),
        build_sysroot_option.as_str(),
        "--enable-languages=c,c++",
        "--disable-bootstrap",
        "--disable-multilib",
        "--disable-nls",
        "--disable-werror",
        "--disable-checking",
        "--disable-analyzer",
        "--enable-shared",
        "--enable-threads=posix",
        "--disable-libsanitizer",
        "--disable-libssp",
        "--disable-libquadmath",
        "--disable-libgomp",
        "--disable-libatomic",
        "--disable-libvtv",
        "--disable-libcc1",
        "--disable-lto",
        "--disable-plugin",
        "--disable-libstdcxx-pch",
        "--without-isl",
        "--with-system-zlib",
    ];
    let prefix_map = format!(
        "-O2 -g0 -ffile-prefix-map={}=/usr/src/mattos/gcc -fdebug-prefix-map={}=/usr/src/mattos/gcc",
        repo_root.display(),
        repo_root.display()
    );
    let env = [
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("LC_ALL", "C".to_string()),
        ("TZ", "UTC".to_string()),
        ("CFLAGS_FOR_TARGET", prefix_map.clone()),
        ("CXXFLAGS_FOR_TARGET", prefix_map),
        ("LDFLAGS_FOR_TARGET", "-Wl,-z,relro -Wl,-z,now".to_string()),
    ];
    fs::write(
        output.join("configure-invocation.txt"),
        format!(
            "SOURCE_DATE_EPOCH={} LC_ALL=C TZ=UTC CFLAGS_FOR_TARGET='{}' CXXFLAGS_FOR_TARGET='{}' LDFLAGS_FOR_TARGET='-Wl,-z,relro -Wl,-z,now' {} {}\nmake -j4 all-target-libgcc all-target-libstdc++-v3\nmake DESTDIR={} install-target-libgcc install-target-libstdc++-v3\n",
            MATTOS_SOURCE_DATE_EPOCH,
            env[3].1,
            env[4].1,
            configure.display(),
            configure_args.join(" "),
            raw_install.display()
        ),
    )?;
    run_gcc_bootstrap_command(&build, &configure, &configure_args, &env)
        .context("GCC runtime configure failed")?;
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &["-j", "4", "all-target-libgcc", "all-target-libstdc++-v3"],
        &env,
    )
    .context("GCC runtime build failed")?;
    let destdir = format!("DESTDIR={}", raw_install.display());
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &[
            destdir.as_str(),
            "install-target-libgcc",
            "install-target-libstdc++-v3",
        ],
        &env,
    )
    .context("GCC runtime install failed")?;

    let libgcc = find_unique_file_named(&raw_install, "libgcc_s.so.1")?;
    let libstdcxx = find_unique_file_named(&raw_install, GCC_RUNTIME_LIBSTDCXX_ABI)?;
    fs::copy(&libgcc, runtime.join("libgcc_s.so.1"))?;
    fs::copy(&libstdcxx, runtime.join(GCC_RUNTIME_LIBSTDCXX_ABI))?;
    std::os::unix::fs::symlink(GCC_RUNTIME_LIBSTDCXX_ABI, runtime.join("libstdc++.so.6"))?;

    let libgcc_needed = elf_needed_names(&runtime.join("libgcc_s.so.1"))?;
    let libstdcxx_needed = elf_needed_names(&runtime.join(GCC_RUNTIME_LIBSTDCXX_ABI))?;
    if !libgcc_needed.is_subset(&BTreeSet::from([
        "libc.so.6".to_string(),
        "ld-linux-x86-64.so.2".to_string(),
    ])) {
        bail!("MattOS libgcc_s has unexpected runtime dependencies: {libgcc_needed:?}")
    }
    if !libstdcxx_needed.is_subset(&BTreeSet::from([
        "libc.so.6".to_string(),
        "libm.so.6".to_string(),
        "libgcc_s.so.1".to_string(),
        "ld-linux-x86-64.so.2".to_string(),
    ])) {
        bail!("MattOS libstdc++ has unexpected runtime dependencies: {libstdcxx_needed:?}")
    }

    let gcc_versions = elf_version_names(&runtime.join("libgcc_s.so.1"), &["GCC_"])?;
    let cxx_versions = elf_version_names(
        &runtime.join(GCC_RUNTIME_LIBSTDCXX_ABI),
        &["GLIBCXX_", "CXXABI_"],
    )?;
    for required in ["GCC_3.0", "GCC_4.2.0", "GCC_14.0.0"] {
        if !gcc_versions.contains(required) {
            bail!("MattOS libgcc_s is missing required ABI node {required}")
        }
    }
    for required in ["GLIBCXX_3.4.34", "CXXABI_1.3.15"] {
        if !cxx_versions.contains(required) {
            bail!("MattOS libstdc++ is missing required ABI node {required}")
        }
    }
    fs::write(
        output.join("runtime-abi.tsv"),
        format!(
            "library\tversion_nodes\nlibgcc_s.so.1\t{}\nlibstdc++.so.6\t{}\n",
            gcc_versions.into_iter().collect::<Vec<_>>().join(","),
            cxx_versions.into_iter().collect::<Vec<_>>().join(",")
        ),
    )?;

    copy_tree_contents(&output.join("runtime"), &sysroot)?;

    let raw_usr = raw_install.join("usr");
    copy_tree_contents(
        &raw_usr.join("include/c++"),
        &sysroot.join("usr/include/c++"),
    )?;
    copy_tree_contents(
        &raw_usr.join("lib/x86_64-linux-gnu/gcc"),
        &sysroot.join("usr/lib/x86_64-linux-gnu/gcc"),
    )?;
    let raw_cxx_libdir = raw_usr.join("lib/lib64");
    let target_libdir = sysroot.join("usr/lib/x86_64-linux-gnu");
    for name in ["libstdc++.a", "libsupc++.a"] {
        fs::copy(raw_cxx_libdir.join(name), target_libdir.join(name))?;
    }
    remove_path_if_exists(&target_libdir.join("libstdc++.so"))?;
    std::os::unix::fs::symlink("libstdc++.so.6", target_libdir.join("libstdc++.so"))?;
    fs::write(
        output.join("development-files.txt"),
        "usr/include/c++/15.3.0\nusr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0\nusr/lib/x86_64-linux-gnu/libstdc++.so\nusr/lib/x86_64-linux-gnu/libstdc++.a\nusr/lib/x86_64-linux-gnu/libsupc++.a\n",
    )?;

    let validation_source = output.join("cxx-unwind-validation.cc");
    let validation_binary = output.join("cxx-unwind-validation");
    fs::write(
        &validation_source,
        "#include <iostream>\n#include <stdexcept>\n#include <string>\nint main() { try { throw std::runtime_error(std::string(\"mattos\")); } catch (const std::exception &e) { std::cout << \"caught:\" << e.what() << '\\n'; return 0; } return 1; }\n",
    )?;
    let sysroot_flag = format!("--sysroot={}", sysroot.display());
    let library_flag = format!("-L{}", runtime.display());
    let rpath_link = format!("-Wl,-rpath-link,{}", runtime.display());
    let validation_source_arg = path_str(&validation_source)?;
    let validation_binary_arg = path_str(&validation_binary)?;
    run_gcc_bootstrap_command(
        repo_root,
        Path::new("g++"),
        &[
            sysroot_flag.as_str(),
            library_flag.as_str(),
            rpath_link.as_str(),
            "-Wl,--dynamic-linker=/lib64/ld-linux-x86-64.so.2",
            validation_source_arg,
            "-o",
            validation_binary_arg,
        ],
        &env,
    )?;
    let loader = sysroot.join("lib64/ld-linux-x86-64.so.2");
    let library_path =
        std::env::join_paths([runtime.clone(), sysroot.join("usr/lib/x86_64-linux-gnu")])?;
    let validation = Command::new(&loader)
        .arg("--library-path")
        .arg(&library_path)
        .arg(&validation_binary)
        .output()?;
    if !validation.status.success()
        || String::from_utf8_lossy(&validation.stdout).trim() != "caught:mattos"
    {
        bail!(
            "MattOS GCC runtime C++ exception validation failed: {}{}",
            String::from_utf8_lossy(&validation.stdout),
            String::from_utf8_lossy(&validation.stderr)
        )
    }
    validate_gcc_runtime_consumers(repo_root, &sysroot, &runtime)?;
    println!(
        "GCC runtime-only build installed libgcc_s.so.1 and {} into {}",
        GCC_RUNTIME_LIBSTDCXX_ABI,
        runtime.display()
    );
    Ok(())
}

const TOOLCHAIN_BUILD: &str = "x86_64-build-linux-gnu";
const TOOLCHAIN_TARGET: &str = "x86_64-pc-linux-gnu";
const GCC_TOOLCHAIN_VERSION: &str = "15.3.0";

fn write_executable_script(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn write_sysroot_compiler_wrappers(
    repo_root: &Path,
    directory: &Path,
    binutils: &Path,
) -> Result<(PathBuf, PathBuf)> {
    let sysroot = repo_root.join("out/sysroot");
    let map = format!(
        "-O2 -g0 -ffile-prefix-map={}=/usr/src/mattos -fdebug-prefix-map={}=/usr/src/mattos",
        repo_root.display(),
        repo_root.display()
    );
    let gcc = directory.join(format!("{TOOLCHAIN_TARGET}-gcc"));
    let gxx = directory.join(format!("{TOOLCHAIN_TARGET}-g++"));
    let target_lib = sysroot.join("usr/lib/x86_64-linux-gnu");
    let target_gcc = target_lib
        .join("gcc")
        .join(TOOLCHAIN_TARGET)
        .join(GCC_TOOLCHAIN_VERSION);
    let common = format!(
        "--sysroot={} -B{}/ -B{}/ -B{}/ -L{} {}",
        shell_escape(path_str(&sysroot)?),
        shell_escape(path_str(binutils)?),
        shell_escape(path_str(&target_gcc)?),
        shell_escape(path_str(&target_lib)?),
        shell_escape(path_str(&target_lib)?),
        map
    );
    write_executable_script(
        &gcc,
        &format!("#!/bin/sh\nexec /usr/bin/gcc {common} \"$@\"\n"),
    )?;
    write_executable_script(
        &gxx,
        &format!("#!/bin/sh\nexec /usr/bin/g++ {common} \"$@\"\n"),
    )?;
    Ok((gcc, gxx))
}

fn toolchain_environment(
    cc: &Path,
    cxx: &Path,
    binutils: &Path,
) -> Result<Vec<(&'static str, String)>> {
    let tool = |name: &str| path_str(&binutils.join(name)).map(str::to_string);
    let mut paths = vec![
        cc.parent()
            .context("toolchain compiler wrapper has no parent directory")?
            .to_path_buf(),
    ];
    if let Some(host_path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&host_path));
    }
    Ok(vec![
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("LC_ALL", "C".to_string()),
        ("TZ", "UTC".to_string()),
        (
            "PATH",
            std::env::join_paths(paths)?.to_string_lossy().into_owned(),
        ),
        ("CC", path_str(cc)?.to_string()),
        ("CXX", path_str(cxx)?.to_string()),
        ("AR", tool("ar")?),
        ("AS", tool("as")?),
        ("LD", tool("ld")?),
        ("NM", tool("nm")?),
        ("RANLIB", tool("ranlib")?),
        ("STRIP", tool("strip")?),
        ("CC_FOR_BUILD", "/usr/bin/gcc".to_string()),
        ("CXX_FOR_BUILD", "/usr/bin/g++".to_string()),
    ])
}

fn build_binutils(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/toolchain/binutils");
    let output = repo_root.join("out/build/binutils");
    let cross_build = output.join("cross-build");
    let cross_install = output.join("cross-install");
    let native_build = output.join("native-build");
    let native_install = output.join("install");
    let wrapper_dir = output.join("bootstrap-bin");
    if !source.join("configure").is_file() {
        bail!("Binutils source is missing at {}", source.display())
    }
    if !repo_root.join("out/sysroot/usr/include/stdio.h").is_file() {
        bail!("Binutils requires the completed MattOS development sysroot")
    }
    remove_path_if_exists(&output)?;
    for directory in [&cross_build, &cross_install, &native_build, &native_install] {
        fs::create_dir_all(directory)?;
    }

    let configure = source.join("configure");
    let sysroot = repo_root.join("out/sysroot");
    let sysroot_arg = format!("--with-sysroot={}", sysroot.display());
    let cross_prefix = format!("--prefix={}", cross_install.join("usr").display());
    let cross_args = [
        cross_prefix.as_str(),
        "--build=x86_64-pc-linux-gnu",
        "--host=x86_64-pc-linux-gnu",
        "--target=x86_64-pc-linux-gnu",
        sysroot_arg.as_str(),
        "--disable-nls",
        "--disable-werror",
        "--disable-gdb",
        "--disable-gdbserver",
        "--disable-gprofng",
        "--disable-gold",
        "--disable-sim",
        "--without-zstd",
        "--enable-deterministic-archives",
    ];
    let reproducible_env = [
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("LC_ALL", "C".to_string()),
        ("TZ", "UTC".to_string()),
        ("CFLAGS", "-O2 -g0".to_string()),
        ("CXXFLAGS", "-O2 -g0".to_string()),
    ];
    run_gcc_bootstrap_command(&cross_build, &configure, &cross_args, &reproducible_env)
        .context("Binutils bootstrap configure failed")?;
    run_gcc_bootstrap_command(
        &cross_build,
        Path::new("make"),
        &["-j", "4", "all-binutils", "all-gas", "all-ld"],
        &reproducible_env,
    )
    .context("Binutils bootstrap build failed")?;
    run_gcc_bootstrap_command(
        &cross_build,
        Path::new("make"),
        &["install-binutils", "install-gas", "install-ld"],
        &reproducible_env,
    )?;

    let cross_bin = cross_install.join("usr/bin");
    let (cc, cxx) = write_sysroot_compiler_wrappers(repo_root, &wrapper_dir, &cross_bin)?;
    let native_env = toolchain_environment(&cc, &cxx, &cross_bin)?;
    let native_args = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--build=x86_64-build-linux-gnu",
        "--host=x86_64-pc-linux-gnu",
        "--target=x86_64-pc-linux-gnu",
        "--with-sysroot=/",
        "--with-build-sysroot=../../sysroot",
        "--disable-nls",
        "--disable-werror",
        "--disable-gdb",
        "--disable-gdbserver",
        "--disable-gprofng",
        "--disable-gold",
        "--disable-sim",
        "--without-zstd",
        "--enable-deterministic-archives",
    ];
    run_gcc_bootstrap_command(&native_build, &configure, &native_args, &native_env)
        .context("MattOS-native Binutils configure failed")?;
    run_gcc_bootstrap_command(
        &native_build,
        Path::new("make"),
        &["-j", "4", "all-binutils", "all-gas", "all-ld"],
        &native_env,
    )
    .context("MattOS-native Binutils build failed")?;
    let destdir = format!("DESTDIR={}", native_install.display());
    run_gcc_bootstrap_command(
        &native_build,
        Path::new("make"),
        &[
            destdir.as_str(),
            "install-binutils",
            "install-gas",
            "install-ld",
        ],
        &native_env,
    )?;
    let tools = [
        "addr2line",
        "ar",
        "as",
        "c++filt",
        "elfedit",
        "ld",
        "nm",
        "objcopy",
        "objdump",
        "ranlib",
        "readelf",
        "size",
        "strings",
        "strip",
    ];
    for tool in tools {
        if !native_install.join("usr/bin").join(tool).is_file() {
            bail!("MattOS-native Binutils did not install /usr/bin/{tool}")
        }
    }
    fs::write(
        output.join("configure-invocation.txt"),
        format!(
            "bootstrap: {} {}\nnative: CC={} CXX={} {} {}\n",
            configure.display(),
            cross_args.join(" "),
            cc.display(),
            cxx.display(),
            configure.display(),
            native_args.join(" ")
        ),
    )?;
    println!("built source-native Binutils for {TOOLCHAIN_TARGET}");
    Ok(())
}

fn prepare_gcc_prerequisite_sources(repo_root: &Path, output: &Path) -> Result<PathBuf> {
    let source = repo_root.join("src/toolchain/gcc");
    let driver = output.join("prerequisite-fetch");
    // Keep checksum-verified prerequisite archives and extracted sources outside
    // the disposable stage directory so a warmed tree remains buildable offline.
    let cache = repo_root.join("out/cache/gcc-prerequisites");
    fs::create_dir_all(driver.join("gcc"))?;
    fs::create_dir_all(driver.join("contrib"))?;
    fs::create_dir_all(&cache)?;
    for relative in [
        "gcc/BASE-VER",
        "contrib/download_prerequisites",
        "contrib/prerequisites.sha512",
    ] {
        let destination = driver.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let metadata = fs::metadata(source.join(relative))?;
        fs::copy(source.join(relative), &destination)?;
        preserve_permissions(&metadata, &destination)?;
    }
    let directory = format!("--directory={}", cache.display());
    run_gcc_bootstrap_command(
        &driver,
        Path::new("./contrib/download_prerequisites"),
        &[directory.as_str(), "--no-isl", "--sha512"],
        &[("LC_ALL", "C".to_string()), ("TZ", "UTC".to_string())],
    )?;
    Ok(cache)
}

fn build_static_prerequisite(
    source: &Path,
    build: &Path,
    install: &Path,
    configure_extra: &[String],
    env: &[(&str, String)],
) -> Result<()> {
    fs::create_dir_all(build)?;
    let prefix = format!("--prefix={}", install.display());
    let mut owned_args = vec![
        prefix,
        format!("--build={TOOLCHAIN_BUILD}"),
        format!("--host={TOOLCHAIN_TARGET}"),
        "--disable-shared".to_string(),
        "--enable-static".to_string(),
    ];
    owned_args.extend_from_slice(configure_extra);
    let args = owned_args.iter().map(String::as_str).collect::<Vec<_>>();
    run_gcc_bootstrap_command(build, &source.join("configure"), &args, env)?;
    run_gcc_bootstrap_command(build, Path::new("make"), &["-j", "4"], env)?;
    run_gcc_bootstrap_command(build, Path::new("make"), &["install"], env)?;
    Ok(())
}

fn build_gcc_toolchain(repo_root: &Path) -> Result<()> {
    let output = repo_root.join("out/build/gcc-toolchain");
    let build = output.join("build");
    let install = output.join("install");
    let prereq_install = output.join("prerequisite-install");
    let binutils = repo_root.join("out/build/binutils/cross-install/usr/bin");
    if !repo_root.join("src/toolchain/gcc/configure").is_file() {
        bail!("GCC source is missing; import the pinned GCC component first")
    }
    if !binutils.join("as").is_file() || !binutils.join("ld").is_file() {
        bail!("GCC toolchain build requires the Binutils bootstrap tools")
    }
    remove_path_if_exists(&output)?;
    fs::create_dir_all(&build)?;
    fs::create_dir_all(&install)?;
    fs::create_dir_all(&prereq_install)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            "../prerequisite-install",
            build.join("prerequisite-install"),
        )?;
        std::os::unix::fs::symlink("../../sysroot", output.join("mattos-sysroot"))?;
        std::os::unix::fs::symlink("../mattos-sysroot", build.join("mattos-sysroot"))?;
    }
    let wrappers = output.join("bootstrap-bin");
    let (cc, cxx) = write_sysroot_compiler_wrappers(repo_root, &wrappers, &binutils)?;
    let env = toolchain_environment(&cc, &cxx, &binutils)?;
    let mut env = env;
    env.extend([
        ("CFLAGS", "-O2 -g0 -std=gnu17".to_string()),
        ("CXXFLAGS", "-O2 -g0 -std=gnu++17".to_string()),
    ]);
    let prereq_sources = prepare_gcc_prerequisite_sources(repo_root, &output)?;
    let gmp_source = prereq_sources.join("gmp-6.2.1");
    let mpfr_source = prereq_sources.join("mpfr-4.1.0");
    let mpc_source = prereq_sources.join("mpc-1.2.1");
    build_static_prerequisite(
        &gmp_source,
        &output.join("prerequisite-build/gmp"),
        &prereq_install,
        &[],
        &env,
    )?;
    let prereq_with_gmp = format!("--with-gmp={}", prereq_install.display());
    build_static_prerequisite(
        &mpfr_source,
        &output.join("prerequisite-build/mpfr"),
        &prereq_install,
        std::slice::from_ref(&prereq_with_gmp),
        &env,
    )?;
    let prereq_with_mpfr = format!("--with-mpfr={}", prereq_install.display());
    build_static_prerequisite(
        &mpc_source,
        &output.join("prerequisite-build/mpc"),
        &prereq_install,
        &[prereq_with_gmp, prereq_with_mpfr],
        &env,
    )?;

    // Invoke GCC through a stable relative path and use stable relative
    // prerequisite prefixes. GCC exposes its configure command in `gcc -v`,
    // so absolute workspace paths here would contaminate the installed driver.
    let configure = PathBuf::from("../../../../src/toolchain/gcc/configure");
    let with_gmp = "--with-gmp=../prerequisite-install".to_string();
    let with_mpfr = "--with-mpfr=../prerequisite-install".to_string();
    let with_mpc = "--with-mpc=../prerequisite-install".to_string();
    let configure_args = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--libexecdir=/usr/libexec",
        "--build=x86_64-build-linux-gnu",
        "--host=x86_64-pc-linux-gnu",
        "--target=x86_64-pc-linux-gnu",
        "--with-sysroot=/",
        "--with-build-sysroot=../mattos-sysroot",
        "--with-native-system-header-dir=/usr/include",
        "--with-as=/usr/bin/as",
        "--with-ld=/usr/bin/ld",
        with_gmp.as_str(),
        with_mpfr.as_str(),
        with_mpc.as_str(),
        "--without-isl",
        "--without-zstd",
        "--enable-languages=c,c++",
        "--enable-default-pie",
        "--disable-bootstrap",
        "--disable-multilib",
        "--disable-nls",
        "--disable-werror",
        "--disable-checking",
        "--disable-analyzer",
        "--disable-libsanitizer",
        "--disable-libssp",
        "--disable-libquadmath",
        "--disable-libgomp",
        "--disable-libatomic",
        "--disable-libvtv",
        "--disable-libcc1",
        "--disable-lto",
        "--disable-plugin",
        "--disable-libstdcxx-pch",
    ];
    let mut gcc_env = env.clone();
    gcc_env.extend([
        ("CFLAGS", "-O2 -g0".to_string()),
        ("CXXFLAGS", "-O2 -g0".to_string()),
        ("LDFLAGS", "-Wl,-z,relro -Wl,-z,now".to_string()),
    ]);
    run_gcc_bootstrap_command(&build, &configure, &configure_args, &gcc_env)
        .context("MattOS-native GCC configure failed")?;
    run_gcc_bootstrap_command(&build, Path::new("make"), &["-j", "4", "all-gcc"], &gcc_env)
        .context("MattOS-native GCC compiler build failed")?;
    let destdir = format!("DESTDIR={}", install.display());
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &[destdir.as_str(), "install-gcc"],
        &gcc_env,
    )?;
    for relative in [
        "usr/bin/gcc",
        "usr/bin/g++",
        "usr/bin/cpp",
        "usr/libexec/gcc/x86_64-pc-linux-gnu/15.3.0/cc1",
        "usr/libexec/gcc/x86_64-pc-linux-gnu/15.3.0/cc1plus",
        "usr/libexec/gcc/x86_64-pc-linux-gnu/15.3.0/collect2",
    ] {
        if !install.join(relative).is_file() {
            bail!("MattOS-native GCC did not install /{relative}")
        }
    }
    for helper in ["cc1", "cc1plus", "collect2"] {
        let needed = elf_needed_names(
            &install
                .join("usr/libexec/gcc")
                .join(TOOLCHAIN_TARGET)
                .join(GCC_TOOLCHAIN_VERSION)
                .join(helper),
        )?;
        if needed.iter().any(|name| {
            name.starts_with("libgmp")
                || name.starts_with("libmpfr")
                || name.starts_with("libmpc")
                || name.starts_with("libzstd")
        }) {
            bail!("installed GCC helper {helper} leaks bootstrap libraries: {needed:?}")
        }
    }
    let mut installed_files = Vec::new();
    collect_regular_files(&install, &mut installed_files)?;
    let build_root = repo_root.to_string_lossy();
    for file in installed_files {
        let header = Command::new("readelf").args(["-h"]).arg(&file).output()?;
        if !header.status.success() {
            continue;
        }
        let bytes = fs::read(&file)?;
        if bytes
            .windows(build_root.len())
            .any(|window| window == build_root.as_bytes())
        {
            bail!(
                "installed GCC ELF {} embeds the host build root",
                file.display()
            )
        }
        let dynamic = Command::new("readelf").args(["-d"]).arg(&file).output()?;
        let dynamic = String::from_utf8_lossy(&dynamic.stdout);
        if dynamic
            .lines()
            .any(|line| line.contains("(RPATH)") || line.contains("(RUNPATH)"))
        {
            bail!(
                "installed GCC ELF {} contains RPATH/RUNPATH",
                file.display()
            )
        }
    }
    fs::write(
        output.join("configure-invocation.txt"),
        format!(
            "CC={} CXX={} CC_FOR_BUILD=/usr/bin/gcc CXX_FOR_BUILD=/usr/bin/g++ {} {}\nmake -j4 all-gcc\nmake DESTDIR={} install-gcc\n",
            cc.display(),
            cxx.display(),
            configure.display(),
            configure_args.join(" "),
            install.display()
        ),
    )?;
    println!("built source-native GCC C/C++ compiler for {TOOLCHAIN_TARGET}");
    Ok(())
}

fn build_make(repo_root: &Path) -> Result<()> {
    let imported = repo_root.join("src/build-tools/make");
    let gnulib = repo_root.join("src/build-support/gnulib");
    let output = repo_root.join("out/build/make");
    let source = output.join("source");
    let build = output.join("build");
    let install = output.join("install");
    let binutils = repo_root.join("out/build/binutils/cross-install/usr/bin");
    if !imported.join("bootstrap").is_file() {
        bail!("GNU Make source is missing at {}", imported.display())
    }
    if !gnulib.join("gnulib-tool").is_file() {
        bail!("pinned Gnulib source is missing at {}", gnulib.display())
    }
    remove_path_if_exists(&output)?;
    fs::create_dir_all(&source)?;
    fs::create_dir_all(&build)?;
    fs::create_dir_all(&install)?;
    copy_tree_contents(&imported, &source)?;
    let gnulib_arg = format!("--gnulib-srcdir={}", gnulib.display());
    run_gcc_bootstrap_command(
        &source,
        Path::new("./bootstrap"),
        &[
            "--gen",
            "--no-git",
            "--no-bootstrap-sync",
            "--copy",
            gnulib_arg.as_str(),
        ],
        &[
            ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
            ("LC_ALL", "C".to_string()),
            ("TZ", "UTC".to_string()),
        ],
    )?;
    let wrappers = output.join("bootstrap-bin");
    let (cc, cxx) = write_sysroot_compiler_wrappers(repo_root, &wrappers, &binutils)?;
    let mut env = toolchain_environment(&cc, &cxx, &binutils)?;
    env.extend([
        ("CC", format!("{TOOLCHAIN_TARGET}-gcc")),
        ("CXX", format!("{TOOLCHAIN_TARGET}-g++")),
        ("CFLAGS", "-O2 -g0".to_string()),
        ("LDFLAGS", "-Wl,-z,relro -Wl,-z,now".to_string()),
    ]);
    let configure_args = [
        "--prefix=/usr",
        "--build=x86_64-build-linux-gnu",
        "--host=x86_64-pc-linux-gnu",
        "--disable-nls",
    ];
    run_gcc_bootstrap_command(&build, &source.join("configure"), &configure_args, &env)?;
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &["-j", "4", "MAKE_MAINTAINER_MODE=", "MAKE_CFLAGS="],
        &env,
    )?;
    let destdir = format!("DESTDIR={}", install.display());
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &[
            destdir.as_str(),
            "MAKE_MAINTAINER_MODE=",
            "MAKE_CFLAGS=",
            "install",
        ],
        &env,
    )?;
    if !install.join("usr/bin/make").is_file() {
        bail!("MattOS-native GNU Make did not install /usr/bin/make")
    }
    fs::write(
        output.join("configure-invocation.txt"),
        format!(
            "gnulib={}\nCC={} {} {}\nmake -j4 MAKE_MAINTAINER_MODE= MAKE_CFLAGS=\nmake DESTDIR={} MAKE_MAINTAINER_MODE= MAKE_CFLAGS= install\n",
            gnulib.display(),
            format!("{TOOLCHAIN_TARGET}-gcc"),
            source.join("configure").display(),
            configure_args.join(" "),
            install.display()
        ),
    )?;
    println!("built source-native GNU Make for {TOOLCHAIN_TARGET}");
    Ok(())
}

fn copy_tree_contents(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&from)?;
        if metadata.is_dir() {
            if fs::symlink_metadata(&to)
                .map(|existing| !existing.is_dir() || existing.file_type().is_symlink())
                .unwrap_or(false)
            {
                remove_path_if_exists(&to)?;
            }
            copy_tree_contents(&from, &to)?;
        } else if metadata.file_type().is_symlink() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            remove_path_if_exists(&to)?;
            std::os::unix::fs::symlink(fs::read_link(&from)?, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            if fs::symlink_metadata(&to)
                .map(|existing| existing.is_dir() || existing.file_type().is_symlink())
                .unwrap_or(false)
            {
                remove_path_if_exists(&to)?;
            }
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
        }
    }
    Ok(())
}

fn hydrate_development_sysroot(repo_root: &Path, installs: &[PathBuf]) -> Result<()> {
    let sysroot = repo_root.join("out/sysroot/usr");
    for install in installs {
        let include = install.join("include");
        if include.is_dir() {
            copy_tree_contents(&include, &sysroot.join("include"))?;
        }
        let library = install.join("lib/x86_64-linux-gnu");
        if library.is_dir() {
            copy_tree_contents(&library, &sysroot.join("lib/x86_64-linux-gnu"))?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct LocalToolEnv {
    tool_root: PathBuf,
    tool_bin_dir: PathBuf,
    tool_lib_dir: PathBuf,
    tool_include_dir: PathBuf,
    bison_pkg_data_dir: PathBuf,
    m4_bin: PathBuf,
}

fn local_tool_env(repo_root: &Path) -> Option<LocalToolEnv> {
    let root = repo_root.join(".tools/rootless/usr");
    let bin = root.join("bin");
    let lib = root.join("lib/x86_64-linux-gnu");
    let include = root.join("include");
    let bison_pkg = root.join("share/bison");
    let m4 = bin.join("m4");
    if bin.exists() && lib.exists() && include.exists() && bison_pkg.exists() && m4.exists() {
        Some(LocalToolEnv {
            tool_root: root,
            tool_bin_dir: bin,
            tool_lib_dir: lib,
            tool_include_dir: include,
            bison_pkg_data_dir: bison_pkg,
            m4_bin: m4,
        })
    } else {
        None
    }
}

fn assert_kernel_build_path_safe(repo_root: &Path) -> Result<()> {
    if cfg!(unix) && std::env::var("WSL_DISTRO_NAME").is_ok() {
        let root = repo_root.to_string_lossy();
        if root.starts_with("/mnt/") {
            bail!(
                "refusing kernel build from Windows-mounted path {}. Use Linux filesystem path like ~/src/MattOS",
                repo_root.display()
            )
        }
    }
    Ok(())
}

fn build_brush(repo_root: &Path) -> Result<()> {
    let brush = repo_root.join("src/userland/brush");
    if !brush.join("Cargo.toml").exists() {
        bail!(
            "brush source not found in {}; run import first",
            brush.display()
        );
    }
    run_cmd(&brush, "cargo", &["build", "--release", "-p", "brush"])
}

fn build_coreutils(repo_root: &Path) -> Result<()> {
    let coreutils = repo_root.join("src/userland/coreutils");
    if !coreutils.join("Cargo.toml").exists() {
        bail!(
            "coreutils source not found in {}; run import first",
            coreutils.display()
        );
    }
    run_cmd(
        &coreutils,
        "cargo",
        &[
            "build",
            "--release",
            "-p",
            "coreutils",
            "--no-default-features",
            "--features",
            "unix",
        ],
    )
}

fn build_grep(repo_root: &Path) -> Result<()> {
    let grep = repo_root.join("src/userland/grep");
    if !grep.join("Cargo.toml").exists() {
        bail!(
            "grep source not found in {}; run import first",
            grep.display()
        );
    }
    run_cmd(
        repo_root,
        "cargo",
        &[
            "build",
            "--release",
            "--manifest-path",
            "src/userland/grep/Cargo.toml",
            "--bin",
            "grep",
        ],
    )
}

fn build_sed(repo_root: &Path) -> Result<()> {
    let sed = repo_root.join("src/userland/sed");
    if !sed.join("Cargo.toml").exists() {
        bail!(
            "sed source not found in {}; run import first",
            sed.display()
        );
    }
    run_cmd(
        repo_root,
        "cargo",
        &[
            "build",
            "--release",
            "--manifest-path",
            "src/userland/sed/Cargo.toml",
            "--bin",
            "sed",
        ],
    )
}

fn build_findutils(repo_root: &Path) -> Result<()> {
    let findutils = repo_root.join("src/userland/findutils");
    if !findutils.join("Cargo.toml").exists() {
        bail!(
            "findutils source not found in {}; run import first",
            findutils.display()
        );
    }
    run_cmd(
        repo_root,
        "cargo",
        &[
            "build",
            "--release",
            "--manifest-path",
            "src/userland/findutils/Cargo.toml",
            "--bins",
        ],
    )
}

fn build_diffutils(repo_root: &Path) -> Result<()> {
    let diffutils = repo_root.join("src/userland/diffutils");
    if !diffutils.join("Cargo.toml").exists() {
        bail!(
            "diffutils source not found in {}; run import first",
            diffutils.display()
        );
    }
    run_cmd(
        repo_root,
        "cargo",
        &[
            "build",
            "--release",
            "--manifest-path",
            "src/userland/diffutils/Cargo.toml",
            "--bin",
            "diffutils",
        ],
    )
}

fn build_init(repo_root: &Path) -> Result<()> {
    run_cmd(
        repo_root,
        "cargo",
        &[
            "build",
            "--release",
            "--manifest-path",
            "src/userland/init/Cargo.toml",
        ],
    )
}

fn build_linux_pam(repo_root: &Path) -> Result<()> {
    let pam_src = repo_root.join("src/system/auth/linux-pam");
    if !pam_src.join("meson.build").exists() {
        bail!(
            "linux-pam source not found in {}; run upstream import linux-pam first",
            pam_src.display()
        );
    }

    let out_root = repo_root.join("out/build/linux-pam");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let options_path = out_root.join("meson-options.txt");
    let libxcrypt = repo_root.join("out/build/libxcrypt/install/usr");
    let libxcrypt_lib = libxcrypt.join("lib/x86_64-linux-gnu");
    if !libxcrypt.join("include/crypt.h").is_file() || !libxcrypt_lib.join("libcrypt.so").exists() {
        bail!("MattOS-built libxcrypt development files are missing; run build libxcrypt first");
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;

    let options = linux_pam_meson_options();
    let env_overrides = [
        (
            "CPPFLAGS",
            format!("-I{}", libxcrypt.join("include").display()),
        ),
        ("LDFLAGS", format!("-L{}", libxcrypt_lib.display())),
        ("LIBRARY_PATH", libxcrypt_lib.display().to_string()),
        ("LD_LIBRARY_PATH", libxcrypt_lib.display().to_string()),
        (
            "PKG_CONFIG_PATH",
            libxcrypt_lib.join("pkgconfig").display().to_string(),
        ),
    ];
    let options_text = format!(
        "{}\n{}\n",
        options.join("\n"),
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let existing_options = fs::read_to_string(&options_path).ok();
    let needs_reconfigure = existing_options.as_deref() != Some(options_text.as_str());
    if needs_reconfigure && build_dir.exists() {
        fs::remove_dir_all(&build_dir)
            .with_context(|| format!("failed to reset {}", build_dir.display()))?;
    }
    let configured = build_dir.join("build.ninja").exists();

    if !configured {
        let mut setup_args = vec![
            "setup".to_string(),
            build_dir.display().to_string(),
            pam_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
    } else if needs_reconfigure {
        let mut setup_args = vec![
            "setup".to_string(),
            "--reconfigure".to_string(),
            build_dir.display().to_string(),
            pam_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
    }

    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &[
            "compile",
            "-C",
            build_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid linux-pam build dir"))?,
        ],
        &env_overrides,
    )?;

    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)
            .with_context(|| format!("failed to clean {}", install_dir.display()))?;
    }
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;

    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            build_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid linux-pam build dir"))?,
            "--no-rebuild",
            "--destdir",
            install_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid linux-pam install dir"))?,
        ],
        &env_overrides,
    )?;

    let pam_lib = install_dir.join("usr/lib/x86_64-linux-gnu/libpam.so.0");
    if !pam_lib.exists() {
        bail!("linux-pam install did not produce {}", pam_lib.display());
    }
    for rel in [
        "usr/lib/x86_64-linux-gnu/security/pam_unix.so",
        "usr/sbin/unix_chkpwd",
    ] {
        validate_dependency_resolves_from(
            &install_dir.join(rel),
            "libcrypt.so.1",
            &libxcrypt_lib,
            &[&libxcrypt_lib],
        )?;
    }
    println!("Linux-PAM libcrypt origin: {}", libxcrypt_lib.display());

    Ok(())
}

fn linux_pam_meson_options() -> Vec<String> {
    vec![
        "--prefix=/usr".to_string(),
        "--sysconfdir=/etc".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        "-Ddocs=disabled".to_string(),
        "-Di18n=disabled".to_string(),
        "-Daudit=disabled".to_string(),
        "-Dselinux=disabled".to_string(),
        "-Dlogind=disabled".to_string(),
        "-Delogind=disabled".to_string(),
        "-Deconf=disabled".to_string(),
        "-Dexamples=false".to_string(),
        "-Dxtests=false".to_string(),
        "-Dsecuredir=/usr/lib/x86_64-linux-gnu/security".to_string(),
    ]
}

fn build_shadow(repo_root: &Path) -> Result<()> {
    let shadow_src = repo_root.join("src/system/auth/shadow");
    if !shadow_src.join("configure.ac").exists() {
        bail!(
            "shadow source not found in {}; run upstream import shadow first",
            shadow_src.display()
        );
    }

    if !shadow_src.join("configure").exists() {
        run_cmd(&shadow_src, "autoreconf", &["-v", "-f", "-i"])?;
    }

    let out_root = repo_root.join("out/build/shadow");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp = build_dir.join("config.stamp");
    let configure_args = [
        "--prefix=/usr",
        "--sysconfdir=/etc",
        "--disable-nls",
        "--with-libpam",
        "--with-libbsd",
        "--without-selinux",
        "--disable-logind",
        "--with-yescrypt",
        "--without-btrfs",
        "--without-nscd",
        "--without-sssd",
    ];
    let pam_install = repo_root.join("out/build/linux-pam/install");
    let pam_include = pam_install.join("usr/include");
    let pam_lib = pam_install.join("usr/lib/x86_64-linux-gnu");
    let pam_pkgconfig = pam_lib.join("pkgconfig");
    let libbsd_install = repo_root.join("out/build/libbsd/install/usr");
    let libbsd_include = libbsd_install.join("include");
    let libbsd_lib = libbsd_install.join("lib/x86_64-linux-gnu");
    let libmd_lib = repo_root.join("out/build/libmd/install/usr/lib/x86_64-linux-gnu");
    let libxcrypt_install = repo_root.join("out/build/libxcrypt/install/usr");
    let libxcrypt_lib = libxcrypt_install.join("lib/x86_64-linux-gnu");
    if !pam_include.join("security/pam_appl.h").exists() || !pam_lib.join("libpam.so").exists() {
        bail!(
            "linux-pam development files missing at {}; run build pam first",
            pam_install.display()
        );
    }
    if !libbsd_include.join("bsd/readpassphrase.h").is_file()
        || !libbsd_lib.join("libbsd.so").exists()
        || !libmd_lib.join("libmd.so").exists()
    {
        bail!(
            "MattOS-built libbsd/libmd development files missing; run build libmd and build libbsd first"
        );
    }
    if !libxcrypt_install.join("include/crypt.h").is_file()
        || !libxcrypt_lib.join("libcrypt.so").exists()
    {
        bail!("MattOS-built libxcrypt development files missing; run build libxcrypt first");
    }
    let library_path = std::env::join_paths([&pam_lib, &libbsd_lib, &libmd_lib, &libxcrypt_lib])?
        .to_string_lossy()
        .to_string();
    let pkgconfig_path = std::env::join_paths([
        &pam_pkgconfig,
        &libbsd_lib.join("pkgconfig"),
        &libmd_lib.join("pkgconfig"),
        &libxcrypt_lib.join("pkgconfig"),
    ])?
    .to_string_lossy()
    .to_string();
    let env_overrides = vec![
        (
            "CPPFLAGS",
            format!(
                "-I{} -I{} -I{} -I{} -DLIBBSD_OVERLAY",
                pam_include.display(),
                libbsd_include.display(),
                libbsd_include.join("bsd").display(),
                libxcrypt_install.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!(
                "-L{} -L{} -L{} -L{}",
                pam_lib.display(),
                libbsd_lib.display(),
                libmd_lib.display(),
                libxcrypt_lib.display()
            ),
        ),
        (
            "LIBBSD_CFLAGS",
            format!(
                "-I{} -DLIBBSD_OVERLAY",
                libbsd_include.join("bsd").display()
            ),
        ),
        ("LIBBSD_LIBS", format!("-L{} -lbsd", libbsd_lib.display())),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        ("PKG_CONFIG_PATH", pkgconfig_path),
    ];
    let config_text = format!(
        "{}\n{}",
        configure_args.join("\n"),
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if stamp.exists() && fs::read_to_string(&stamp).ok().as_deref() != Some(config_text.as_str()) {
        fs::remove_dir_all(&build_dir)
            .with_context(|| format!("failed to reset {}", build_dir.display()))?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;

    if !stamp.exists() {
        run_cmd_with_env_overrides(
            &build_dir,
            shadow_src
                .join("configure")
                .to_str()
                .ok_or_else(|| anyhow!("invalid shadow configure path"))?,
            &configure_args,
            &env_overrides,
        )?;
        fs::write(&stamp, &config_text)
            .with_context(|| format!("failed to write {}", stamp.display()))?;
    }

    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env_overrides)?;

    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)
            .with_context(|| format!("failed to clean {}", install_dir.display()))?;
    }
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;

    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[
            "install",
            &format!(
                "DESTDIR={}",
                install_dir
                    .to_str()
                    .ok_or_else(|| anyhow!("invalid shadow install dir"))?
            ),
        ],
        &env_overrides,
    )?;

    let passwd_bin = install_dir.join("usr/bin/passwd");
    if !passwd_bin.exists() {
        bail!("shadow install did not produce {}", passwd_bin.display());
    }
    let shadow_lib_dirs: [&Path; 3] = [&libbsd_lib, &libmd_lib, &libxcrypt_lib];
    for rel in [
        "usr/bin/chage",
        "usr/bin/newgrp",
        "usr/bin/passwd",
        "usr/sbin/chpasswd",
        "usr/sbin/groupadd",
        "usr/sbin/groupdel",
        "usr/sbin/groupmod",
        "usr/sbin/useradd",
        "usr/sbin/userdel",
        "usr/sbin/usermod",
    ] {
        validate_dependency_resolves_from(
            &install_dir.join(rel),
            "libbsd.so.0",
            &libbsd_lib,
            &shadow_lib_dirs,
        )?;
    }
    for rel in ["usr/bin/newgrp", "usr/bin/passwd", "usr/sbin/chpasswd"] {
        validate_dependency_resolves_from(
            &install_dir.join(rel),
            "libcrypt.so.1",
            &libxcrypt_lib,
            &shadow_lib_dirs,
        )?;
    }
    println!(
        "Shadow origins: libbsd={} transitive-libmd={} libcrypt={}",
        libbsd_lib.display(),
        libmd_lib.display(),
        libxcrypt_lib.display()
    );

    Ok(())
}

fn build_sudo_rs(repo_root: &Path) -> Result<()> {
    let sudo_src = repo_root.join("src/system/auth/sudo-rs");
    if !sudo_src.join("Cargo.toml").exists() {
        bail!(
            "sudo-rs source not found in {}; run upstream import sudo-rs first",
            sudo_src.display()
        );
    }

    let pam_install = repo_root.join("out/build/linux-pam/install");
    let pam_lib = pam_install.join("usr/lib/x86_64-linux-gnu");
    if !pam_lib.join("libpam.so").exists() && !pam_lib.join("libpam.so.0").exists() {
        bail!(
            "linux-pam libraries missing at {}; run build pam first",
            pam_lib.display()
        );
    }
    let current_rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
    let rustflags = if current_rustflags.is_empty() {
        format!("-L native={}", pam_lib.display())
    } else {
        format!("-L native={} {current_rustflags}", pam_lib.display())
    };
    let current_library_path = std::env::var("LIBRARY_PATH").unwrap_or_default();
    let library_path = if current_library_path.is_empty() {
        pam_lib.display().to_string()
    } else {
        format!("{}:{current_library_path}", pam_lib.display())
    };
    let env_overrides = vec![("RUSTFLAGS", rustflags), ("LIBRARY_PATH", library_path)];

    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--release",
            "--manifest-path",
            "src/system/auth/sudo-rs/Cargo.toml",
            "--bin",
            "sudo",
            "--bin",
            "visudo",
        ],
        &env_overrides,
    )?;

    let out_root = repo_root.join("out/build/sudo-rs");
    let install_dir = out_root.join("install");
    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)
            .with_context(|| format!("failed to clean {}", install_dir.display()))?;
    }
    fs::create_dir_all(install_dir.join("usr/bin"))
        .with_context(|| format!("failed to create {}", install_dir.join("usr/bin").display()))?;

    for bin in ["sudo", "visudo"] {
        let src = repo_root.join(format!("src/system/auth/sudo-rs/target/release/{bin}"));
        if !src.exists() {
            bail!("sudo-rs build did not produce {}", src.display());
        }
        let dst = install_dir.join("usr/bin").join(bin);
        fs::copy(&src, &dst).with_context(|| format!("failed to copy {}", src.display()))?;
    }

    Ok(())
}

fn build_util_linux(repo_root: &Path) -> Result<()> {
    let util_linux_src = repo_root.join("src/userland/util-linux");
    if !util_linux_src.join("meson.build").exists() {
        bail!(
            "util-linux source not found in {}; run upstream import util-linux first",
            util_linux_src.display()
        );
    }

    let out_root = repo_root.join("out/build/util-linux");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let options_path = out_root.join("meson-options.txt");
    let env_path = out_root.join("meson-env.txt");
    let pam_install = repo_root.join("out/build/linux-pam/install");
    let pam_pkgconfig = pam_install.join("usr/lib/x86_64-linux-gnu/pkgconfig");
    let pam_include = pam_install.join("usr/include");
    let pam_lib = pam_install.join("usr/lib/x86_64-linux-gnu");
    let selinux_install = repo_root.join("out/build/selinux/install/usr");
    let selinux_pkgconfig = selinux_install.join("lib/x86_64-linux-gnu/pkgconfig");
    let selinux_include = selinux_install.join("include");
    let selinux_lib = selinux_install.join("lib/x86_64-linux-gnu");
    let pcre2_install = repo_root.join("out/build/pcre2/install/usr");
    let pcre2_pkgconfig = pcre2_install.join("lib/x86_64-linux-gnu/pkgconfig");
    let pcre2_include = pcre2_install.join("include");
    let pcre2_lib = pcre2_install.join("lib/x86_64-linux-gnu");
    if !pam_pkgconfig.exists() {
        bail!(
            "linux-pam pkg-config directory missing at {}; run build pam first",
            pam_pkgconfig.display()
        );
    }
    if !selinux_lib.join("libselinux.so.1").exists() || !pcre2_lib.join("libpcre2-8.so.0").exists()
    {
        bail!("staged SELinux/PCRE2 libraries are missing; run build selinux first");
    }

    let current_pkg_config = std::env::var("PKG_CONFIG_PATH").unwrap_or_default();
    let staged_pkg_config =
        std::env::join_paths([&pam_pkgconfig, &selinux_pkgconfig, &pcre2_pkgconfig])?
            .to_string_lossy()
            .to_string();
    let pkg_config_path = if current_pkg_config.is_empty() {
        staged_pkg_config
    } else {
        format!("{staged_pkg_config}:{current_pkg_config}")
    };
    let current_cflags = std::env::var("CFLAGS").unwrap_or_default();
    let staged_cflags = format!(
        "-I{} -I{} -I{}",
        pam_include.display(),
        selinux_include.display(),
        pcre2_include.display()
    );
    let cflags = if current_cflags.is_empty() {
        staged_cflags
    } else {
        format!("{staged_cflags} {current_cflags}")
    };
    let current_ldflags = std::env::var("LDFLAGS").unwrap_or_default();
    let staged_ldflags = format!(
        "-L{} -L{} -L{}",
        pam_lib.display(),
        selinux_lib.display(),
        pcre2_lib.display()
    );
    let ldflags = if current_ldflags.is_empty() {
        staged_ldflags
    } else {
        format!("{staged_ldflags} {current_ldflags}")
    };
    let library_path = std::env::join_paths([&pam_lib, &selinux_lib, &pcre2_lib])?
        .to_string_lossy()
        .to_string();
    let env_overrides = vec![
        ("PKG_CONFIG_PATH", pkg_config_path),
        ("CFLAGS", cflags),
        ("LDFLAGS", ldflags),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
    ];
    let env_text = format!(
        "{}\n",
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let existing_env = fs::read_to_string(&env_path).ok();
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;

    let options = util_linux_meson_options();
    let options_text = format!("{}\n", options.join("\n"));
    let existing_options = fs::read_to_string(&options_path).ok();
    let needs_reconfigure = existing_options.as_deref() != Some(options_text.as_str());
    let env_changed = existing_env.as_deref() != Some(env_text.as_str());
    let mut configured = build_dir.join("build.ninja").exists();

    if configured && env_changed {
        fs::remove_dir_all(&build_dir)
            .with_context(|| format!("failed to reset {}", build_dir.display()))?;
        configured = false;
    }

    if !configured {
        let mut setup_args = vec![
            "setup".to_string(),
            build_dir.display().to_string(),
            util_linux_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
        fs::write(&env_path, &env_text)
            .with_context(|| format!("failed to write {}", env_path.display()))?;
    } else if needs_reconfigure {
        let mut setup_args = vec![
            "setup".to_string(),
            "--reconfigure".to_string(),
            build_dir.display().to_string(),
            util_linux_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
        fs::write(&env_path, &env_text)
            .with_context(|| format!("failed to write {}", env_path.display()))?;
    }

    run_cmd_with_env_overrides(
        repo_root,
        "ninja",
        &[
            "-C",
            build_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid util-linux build dir"))?,
        ],
        &env_overrides,
    )?;

    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)
            .with_context(|| format!("failed to clean {}", install_dir.display()))?;
    }
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;

    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            build_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid util-linux build dir"))?,
            "--no-rebuild",
            "--destdir",
            install_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid util-linux install dir"))?,
        ],
        &env_overrides,
    )?;

    for path in [
        install_dir.join("usr/sbin/agetty"),
        install_dir.join("usr/bin/login"),
        install_dir.join("usr/bin/su"),
        install_dir.join("usr/bin/mount"),
        install_dir.join("usr/bin/umount"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libblkid.so.1"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libmount.so.1"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libsmartcols.so.1"),
    ] {
        if !path.exists() {
            bail!("util-linux install did not produce {}", path.display());
        }
    }
    let util_linux_lib = install_dir.join("usr/lib/x86_64-linux-gnu");
    let runtime_dirs: [&Path; 4] = [&util_linux_lib, &selinux_lib, &pcre2_lib, &pam_lib];
    validate_dependency_resolves_from(
        &install_dir.join("usr/lib/x86_64-linux-gnu/libmount.so.1"),
        "libblkid.so.1",
        &util_linux_lib,
        &runtime_dirs,
    )?;
    validate_dependency_resolves_from(
        &install_dir.join("usr/bin/mount"),
        "libmount.so.1",
        &util_linux_lib,
        &runtime_dirs,
    )?;
    let mount_strings = run_cmd_capture(
        repo_root,
        "strings",
        &[path_str(&install_dir.join("usr/bin/mount"))?],
    )?;
    if !mount_strings.contains("libselinux.so.1") {
        bail!("util-linux mount lost its configured SELinux compatibility loader");
    }

    Ok(())
}

fn util_linux_meson_options() -> Vec<String> {
    vec![
        "--prefix=/usr".to_string(),
        "--sbindir=/usr/sbin".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        "--auto-features=disabled".to_string(),
        "-Dbuild-agetty=enabled".to_string(),
        "-Dbuild-login=enabled".to_string(),
        "-Dbuild-su=enabled".to_string(),
        "-Dbuild-libblkid=enabled".to_string(),
        "-Dbuild-libmount=enabled".to_string(),
        "-Dbuild-libsmartcols=enabled".to_string(),
        "-Dbuild-mount=enabled".to_string(),
        "-Dselinux=enabled".to_string(),
        "-Dsystemd=disabled".to_string(),
        "-Dnls=disabled".to_string(),
        "-Dbuild-bash-completion=disabled".to_string(),
        "-Dbuild-python=disabled".to_string(),
        "-Dbuild-pylibmount=disabled".to_string(),
    ]
}

fn build_kmod(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/kmod");
    if !source.join("meson.build").exists() {
        bail!(
            "kmod source not found in {}; run upstream import kmod first",
            source.display()
        );
    }

    let out_root = repo_root.join("out/build/kmod");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let options_path = out_root.join("meson-options.txt");
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    let options = kmod_meson_options();
    let options_text = format!("{}\n", options.join("\n"));
    let configured = build_dir.join("build.ninja").exists();
    let changed = fs::read_to_string(&options_path).ok().as_deref() != Some(options_text.as_str());

    let mut args = vec!["setup".to_string()];
    if configured && changed {
        args.push("--reconfigure".to_string());
    }
    if !configured || changed {
        args.push(build_dir.display().to_string());
        args.push(source.display().to_string());
        args.extend(options.clone());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_cmd(repo_root, "meson", &refs)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
    }

    run_cmd(
        repo_root,
        "meson",
        &["compile", "-C", path_str(&build_dir)?],
    )?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            path_str(&build_dir)?,
            "--no-rebuild",
            "--destdir",
            path_str(&install_dir)?,
        ],
    )?;
    for command in KMOD_BINARIES {
        let path = install_dir.join(command.source_rel);
        if !path_entry_exists(&path) {
            bail!("kmod install did not produce {}", path.display());
        }
    }
    Ok(())
}

fn kmod_meson_options() -> Vec<String> {
    vec![
        "--prefix=/usr".to_string(),
        "--sbindir=/usr/sbin".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        "--sysconfdir=/etc".to_string(),
        "--auto-features=disabled".to_string(),
        "-Dzstd=disabled".to_string(),
        "-Dxz=disabled".to_string(),
        "-Dzlib=disabled".to_string(),
        "-Dopenssl=disabled".to_string(),
        "-Dmbedtls=disabled".to_string(),
        "-Ddlopen=[]".to_string(),
        "-Dtools=true".to_string(),
        "-Dlogging=true".to_string(),
        "-Dbuild-tests=false".to_string(),
        "-Dmanpages=false".to_string(),
        "-Ddocs=false".to_string(),
    ]
}

fn build_ncurses(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/terminal/ncurses");
    let configure = source.join("configure");
    if !configure.exists() {
        bail!(
            "ncurses source not found in {}; run upstream import ncurses first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/ncurses");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp = out_root.join("configure-options.txt");
    let options = ncurses_configure_options();
    let options_text = format!("{}\n", options.join("\n"));
    if build_dir.join("Makefile").exists()
        && fs::read_to_string(&stamp).ok().as_deref() != Some(options_text.as_str())
    {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").exists() {
        run_cmd(&build_dir, path_str(&configure)?, &options)?;
        fs::write(&stamp, &options_text)
            .with_context(|| format!("failed to write {}", stamp.display()))?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        &build_dir,
        "make",
        &[&format!("DESTDIR={}", install_dir.display()), "install"],
    )?;
    for command in NCURSES_BINARIES {
        let path = install_dir.join(command.source_rel);
        if !path.exists() {
            bail!("ncurses install did not produce {}", path.display());
        }
    }
    verify_terminfo_entries(&install_dir.join("usr/share/terminfo"))?;
    Ok(())
}

fn ncurses_configure_options() -> Vec<&'static str> {
    vec![
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--with-shared",
        "--without-normal",
        "--without-debug",
        "--without-ada",
        "--without-cxx",
        "--without-cxx-binding",
        "--without-tests",
        "--without-manpages",
        "--disable-stripping",
        "--enable-widec",
        "--with-termlib",
        "--enable-pc-files",
        "--with-pkg-config-libdir=/usr/lib/x86_64-linux-gnu/pkgconfig",
    ]
}

fn build_procps(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/procps-ng");
    if !source.join("configure.ac").exists() {
        bail!(
            "procps-ng source not found in {}; run upstream import procps-ng first",
            source.display()
        );
    }
    if !source.join("configure").exists() {
        run_cmd(&source, "./autogen.sh", &[])?;
    }
    let ncurses_install = repo_root.join("out/build/ncurses/install/usr");
    if !ncurses_install
        .join("lib/x86_64-linux-gnu/libncursesw.so.6")
        .exists()
    {
        bail!(
            "ncurses runtime missing at {}; run build ncurses first",
            ncurses_install.display()
        );
    }
    let out_root = repo_root.join("out/build/procps-ng");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp = out_root.join("configure-options.txt");
    let options = procps_configure_options();
    let env = vec![
        (
            "PKG_CONFIG_PATH",
            ncurses_install
                .join("lib/x86_64-linux-gnu/pkgconfig")
                .display()
                .to_string(),
        ),
        (
            "CPPFLAGS",
            format!("-I{}", ncurses_install.join("include").display()),
        ),
        (
            "LDFLAGS",
            format!(
                "-L{}",
                ncurses_install.join("lib/x86_64-linux-gnu").display()
            ),
        ),
        (
            "NCURSES_CFLAGS",
            format!("-I{}", ncurses_install.join("include").display()),
        ),
        (
            "NCURSES_LIBS",
            format!(
                "-L{} -lncursesw -ltinfow",
                ncurses_install.join("lib/x86_64-linux-gnu").display()
            ),
        ),
    ];
    let stamp_text = format!(
        "{}\n{}\n",
        options.join("\n"),
        env.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if build_dir.join("Makefile").exists()
        && fs::read_to_string(&stamp).ok().as_deref() != Some(stamp_text.as_str())
    {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").exists() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source.join("configure"))?,
            &options,
            &env,
        )?;
        fs::write(&stamp, &stamp_text)
            .with_context(|| format!("failed to write {}", stamp.display()))?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[&format!("DESTDIR={}", install_dir.display()), "install"],
        &env,
    )?;
    for command in PROCPS_BINARIES {
        let path = install_dir.join(command.source_rel);
        if !path.exists() {
            bail!("procps-ng install did not produce {}", path.display());
        }
    }
    Ok(())
}

fn procps_configure_options() -> Vec<&'static str> {
    vec![
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--sysconfdir=/etc",
        "--disable-nls",
        "--without-systemd",
        "--without-elogind",
        "--disable-numa",
        "--disable-kill",
        "--disable-pidwait",
        "--disable-examples",
        "--disable-static",
    ]
}

fn sync_build_source(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let source_arg = format!("{}/", source.display());
    let destination_arg = format!("{}/", destination.display());
    run_cmd(
        Path::new("/"),
        "rsync",
        &["-a", "--exclude=.git/", &source_arg, &destination_arg],
    )
}

fn build_expat(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/expat/expat");
    if !source.join("CMakeLists.txt").is_file() {
        bail!(
            "Expat source not found in {}; run upstream import expat first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/expat");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let options = [
        "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_INSTALL_PREFIX=/usr",
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
        "-DEXPAT_SHARED_LIBS=ON",
        "-DEXPAT_BUILD_TOOLS=OFF",
        "-DEXPAT_BUILD_EXAMPLES=OFF",
        "-DEXPAT_BUILD_TESTS=OFF",
        "-DEXPAT_BUILD_DOCS=OFF",
        "-DEXPAT_BUILD_FUZZERS=OFF",
        "-DEXPAT_BUILD_PKGCONFIG=ON",
    ];
    let state = fs::read_to_string(repo_root.join("upstream/state/expat.toml"))
        .context("failed to read Expat upstream state")?;
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    if !build_dir.join("CMakeCache.txt").is_file() {
        let mut args = vec![
            "-S",
            path_str(&source)?,
            "-B",
            path_str(&build_dir)?,
            "-G",
            "Ninja",
        ];
        args.extend(options);
        run_cmd(repo_root, "cmake", &args)?;
    }
    run_cmd(
        repo_root,
        "cmake",
        &["--build", path_str(&build_dir)?, "--parallel", "4"],
    )?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build_dir)?],
        &[("DESTDIR", install_dir.display().to_string())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libexpat.so.1");
    if !soname.exists() {
        bail!("Expat install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_libcap(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/libcap");
    if !source.join("libcap/Makefile").is_file() {
        bail!(
            "libcap source not found in {}; run upstream import libcap first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/libcap");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/libcap.toml"))
        .context("failed to read libcap upstream state")?;
    let make_options = [
        "prefix=/usr",
        "lib=lib/x86_64-linux-gnu",
        "PTHREADS=no",
        "PAM_CAP=no",
        "GOLANG=no",
        "SHARED=yes",
        "USE_GPERF=yes",
    ];
    let stamp = format!("{state}\n{}\n", make_options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    let libcap_dir = source_copy.join("libcap");
    // Upstream's cap_magic.o rule includes cap_names.h indirectly without listing
    // it as a prerequisite, so this focused library build must remain serial.
    let mut build_args = vec!["libcap.so"];
    build_args.extend(make_options);
    run_cmd(&libcap_dir, "make", &build_args)?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    let destdir = format!("DESTDIR={}", install_dir.display());
    let mut install_args = vec!["install-shared-cap", destdir.as_str()];
    install_args.extend(make_options);
    run_cmd(&libcap_dir, "make", &install_args)?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libcap.so.2");
    if !soname.exists() {
        bail!("libcap install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_attr(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/attr");
    if !source.join("configure.ac").is_file() {
        bail!(
            "attr source not found in {}; run upstream import attr first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/attr");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/attr.toml"))
        .context("failed to read attr upstream state")?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--disable-nls",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "./autogen.sh", &[])?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").is_file() {
        let configure = source_copy.join("configure");
        run_cmd(&build_dir, path_str(&configure)?, &options)?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libattr.so.1");
    let headers = install_dir.join("usr/include/attr");
    if !soname.exists() || !headers.join("error_context.h").is_file() {
        bail!(
            "attr install did not produce {} and its development headers",
            soname.display()
        );
    }
    copy_tree_contents(
        &install_dir.join("usr/include"),
        &repo_root.join("out/sysroot/usr/include"),
    )?;
    copy_tree_contents(
        &install_dir.join("usr/lib/x86_64-linux-gnu"),
        &repo_root.join("out/sysroot/usr/lib/x86_64-linux-gnu"),
    )?;
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_acl(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/acl");
    if !source.join("configure.ac").is_file() {
        bail!(
            "ACL source not found in {}; run upstream import acl first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/acl");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/acl.toml"))
        .context("failed to read ACL upstream state")?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--disable-nls",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "./autogen.sh", &[])?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").is_file() {
        let configure = source_copy.join("configure");
        run_cmd(&build_dir, path_str(&configure)?, &options)?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libacl.so.1");
    if !soname.exists() {
        bail!("ACL install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_zlib(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/zlib");
    if !source.join("configure").is_file() {
        bail!(
            "zlib source not found in {}; run upstream import zlib first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/zlib");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/zlib.toml"))
        .context("failed to read zlib upstream state")?;
    let options = ["--prefix=/usr", "--libdir=/usr/lib/x86_64-linux-gnu"];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd(&build_dir, path_str(&source.join("configure"))?, &options)?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libz.so.1");
    if !soname.exists() {
        bail!("zlib install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_bzip2(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/bzip2");
    if !source.join("Makefile-libbz2_so").is_file() {
        bail!(
            "bzip2 source not found in {}; run upstream import bzip2 first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/bzip2");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/bzip2.toml"))
        .context("failed to read bzip2 upstream state")?;
    let stamp = format!("{state}\nMakefile-libbz2_so\n");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    run_cmd(
        &source_copy,
        "make",
        &["-f", "Makefile-libbz2_so", "-j", "4"],
    )?;
    remove_path_if_exists(&install_dir)?;
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    let includedir = install_dir.join("usr/include");
    fs::create_dir_all(&libdir)?;
    fs::create_dir_all(&includedir)?;
    fs::copy(
        source_copy.join("libbz2.so.1.0.8"),
        libdir.join("libbz2.so.1.0.8"),
    )?;
    std::os::unix::fs::symlink("libbz2.so.1.0.8", libdir.join("libbz2.so.1.0"))?;
    std::os::unix::fs::symlink("libbz2.so.1.0", libdir.join("libbz2.so"))?;
    fs::copy(source_copy.join("bzlib.h"), includedir.join("bzlib.h"))?;
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_lz4(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/lz4");
    if !source.join("lib/Makefile").is_file() {
        bail!(
            "LZ4 source not found in {}; run upstream import lz4 first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/lz4");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/lz4.toml"))
        .context("failed to read LZ4 upstream state")?;
    let stamp = format!("{state}\nmake lib\n");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    let library_source = source_copy.join("lib");
    run_cmd(&library_source, "make", &["-j", "4", "lib"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        &library_source,
        "make",
        &[
            "install",
            &format!("DESTDIR={}", install_dir.display()),
            "PREFIX=/usr",
            "LIBDIR=/usr/lib/x86_64-linux-gnu",
        ],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/liblz4.so.1");
    if !soname.exists() {
        bail!("LZ4 install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_xz(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/xz");
    if !source.join("configure.ac").is_file() {
        bail!(
            "XZ Utils source not found in {}; run upstream import xz first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/xz");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/xz.toml"))
        .context("failed to read XZ Utils upstream state")?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--disable-nls",
        "--disable-doc",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "./autogen.sh", &["--no-po4a"])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
        )?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/liblzma.so.5");
    if !soname.exists() {
        bail!("XZ Utils install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_xxhash(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/xxhash");
    if !source.join("Makefile").is_file() {
        bail!(
            "xxHash source not found in {}; run upstream import xxhash first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/xxhash");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/xxhash.toml"))
        .context("failed to read xxHash upstream state")?;
    let stamp = format!("{state}\nmake libxxhash\n");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    run_cmd(&source_copy, "make", &["-j", "4", "libxxhash"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        &source_copy,
        "make",
        &[
            "install_libxxhash",
            "install_libxxhash.includes",
            "install_libxxhash.pc",
            &format!("DESTDIR={}", install_dir.display()),
            "PREFIX=/usr",
            "LIBDIR=/usr/lib/x86_64-linux-gnu",
            "INCLUDEDIR=/usr/include",
            "PKGCONFIGDIR=/usr/lib/x86_64-linux-gnu/pkgconfig",
        ],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libxxhash.so.0");
    if !soname.exists() {
        bail!("xxHash install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_zstd(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/zstd");
    if !source.join("build/cmake/CMakeLists.txt").is_file() {
        bail!(
            "Zstandard source not found in {}; run upstream import zstd first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/zstd");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/zstd.toml"))
        .context("failed to read Zstandard upstream state")?;
    let options = [
        "-G",
        "Ninja",
        "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_INSTALL_PREFIX=/usr",
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
        "-DZSTD_BUILD_PROGRAMS=OFF",
        "-DZSTD_BUILD_TESTS=OFF",
        "-DZSTD_BUILD_STATIC=OFF",
        "-DZSTD_BUILD_SHARED=ON",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    if !build_dir.join("build.ninja").is_file() {
        let cmake_source = source.join("build/cmake");
        let mut args = vec!["-S", path_str(&cmake_source)?, "-B", path_str(&build_dir)?];
        args.extend(options);
        run_cmd(repo_root, "cmake", &args)?;
    }
    run_cmd(
        repo_root,
        "cmake",
        &["--build", path_str(&build_dir)?, "--parallel", "4"],
    )?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build_dir)?],
        &[("DESTDIR", install_dir.display().to_string())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libzstd.so.1");
    if !soname.exists() {
        bail!("Zstandard install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_openssl(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/openssl");
    if !source.join("Configure").is_file() {
        bail!(
            "OpenSSL source not found in {}; run upstream import openssl first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/openssl");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let zlib = repo_root.join("out/build/zlib/install/usr");
    let zstd = repo_root.join("out/build/zstd/install/usr");
    let zlib_lib = zlib.join("lib/x86_64-linux-gnu");
    let zstd_lib = zstd.join("lib/x86_64-linux-gnu");
    if !zlib_lib.join("libz.so").exists() || !zstd_lib.join("libzstd.so").exists() {
        bail!("MattOS OpenSSL dependencies are missing; run build zlib and build zstd first")
    }
    let state = fs::read_to_string(repo_root.join("upstream/state/openssl.toml"))
        .context("failed to read OpenSSL upstream state")?;
    let options = openssl_configure_options(&zlib, &zstd);
    let library_path = std::env::join_paths([&zlib_lib, &zstd_lib])?
        .to_string_lossy()
        .to_string();
    let env = [
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        (
            "CPPFLAGS",
            format!(
                "-I{} -I{}",
                zlib.join("include").display(),
                zstd.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!("-L{} -L{}", zlib_lib.display(), zstd_lib.display()),
        ),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths([zlib_lib.join("pkgconfig"), zstd_lib.join("pkgconfig")])?
                .to_string_lossy()
                .to_string(),
        ),
    ];
    let stamp = format!(
        "{state}\n{}\n{}\n",
        options.join("\n"),
        env.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        let option_refs = options.iter().map(String::as_str).collect::<Vec<_>>();
        run_cmd_with_env_overrides(
            &build_dir,
            "perl",
            &[path_str(&source_copy.join("Configure"))?]
                .into_iter()
                .chain(option_refs)
                .collect::<Vec<_>>()
                .as_slice(),
            &env,
        )?;
    }
    let build_info = Command::new("perl")
        .arg(source_copy.join("util/mkbuildinf.pl"))
        .arg("gcc -O2 -fPIC")
        .arg("linux-x86_64")
        .env("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string())
        .output()
        .context("failed to generate sanitized OpenSSL build information")?;
    if !build_info.status.success() {
        bail!(
            "OpenSSL build-information generator failed: {}",
            String::from_utf8_lossy(&build_info.stderr)
        )
    }
    let build_info_path = build_dir.join("crypto/buildinf.h");
    fs::create_dir_all(
        build_info_path
            .parent()
            .ok_or_else(|| anyhow!("invalid OpenSSL build-information path"))?,
    )?;
    fs::write(&build_info_path, build_info.stdout)
        .with_context(|| format!("failed to write {}", build_info_path.display()))?;
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install_sw", &format!("DESTDIR={}", install_dir.display())],
        &env,
    )?;
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    for soname in ["libcrypto.so.3", "libssl.so.3"] {
        if !libdir.join(soname).exists() {
            bail!(
                "OpenSSL install did not produce {}",
                libdir.join(soname).display()
            )
        }
    }
    let search_dirs: [&Path; 3] = [&libdir, &zlib_lib, &zstd_lib];
    validate_dependency_resolves_from(
        &libdir.join("libcrypto.so.3"),
        "libz.so.1",
        &zlib_lib,
        &search_dirs,
    )?;
    validate_dependency_resolves_from(
        &libdir.join("libcrypto.so.3"),
        "libzstd.so.1",
        &zstd_lib,
        &search_dirs,
    )?;
    validate_dependency_resolves_from(
        &libdir.join("libssl.so.3"),
        "libcrypto.so.3",
        &libdir,
        &search_dirs,
    )?;
    fs::write(&stamp_path, stamp)?;
    println!(
        "OpenSSL origins: zlib={} zstd={}; OPENSSLDIR=/etc/ssl",
        zlib_lib.display(),
        zstd_lib.display()
    );
    Ok(())
}

fn openssl_configure_options(zlib: &Path, zstd: &Path) -> Vec<String> {
    let zlib_lib = zlib.join("lib/x86_64-linux-gnu");
    let zstd_lib = zstd.join("lib/x86_64-linux-gnu");
    vec![
        "linux-x86_64".to_string(),
        "shared".to_string(),
        "zlib".to_string(),
        "enable-zstd".to_string(),
        "no-tests".to_string(),
        "no-docs".to_string(),
        "no-apps".to_string(),
        "no-legacy".to_string(),
        "no-module".to_string(),
        "--prefix=/usr".to_string(),
        "--openssldir=/etc/ssl".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        format!("--with-zlib-include={}", zlib.join("include").display()),
        format!("--with-zlib-lib={}", zlib_lib.display()),
        format!("--with-zstd-include={}", zstd.join("include").display()),
        format!("--with-zstd-lib={}", zstd_lib.display()),
    ]
}

fn build_elfutils(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/elfutils");
    if !source.join("configure.ac").is_file() {
        bail!(
            "elfutils source not found in {}; run upstream import elfutils first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/elfutils");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let zlib = repo_root.join("out/build/zlib/install/usr");
    let zstd = repo_root.join("out/build/zstd/install/usr");
    let zlib_lib = zlib.join("lib/x86_64-linux-gnu");
    let zstd_lib = zstd.join("lib/x86_64-linux-gnu");
    if !zlib_lib.join("libz.so").exists() || !zstd_lib.join("libzstd.so").exists() {
        bail!("MattOS elfutils dependencies are missing; run build zlib and build zstd first")
    }
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--enable-maintainer-mode",
        "--disable-nls",
        "--disable-libdebuginfod",
        "--disable-debuginfod",
        "--disable-demangler",
        "--with-zlib",
        "--with-zstd",
        "--without-bzlib",
        "--without-lzma",
    ];
    let library_path = std::env::join_paths([&zlib_lib, &zstd_lib])?
        .to_string_lossy()
        .to_string();
    let env = [
        (
            "CPPFLAGS",
            format!(
                "-I{} -I{}",
                zlib.join("include").display(),
                zstd.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!("-L{} -L{}", zlib_lib.display(), zstd_lib.display()),
        ),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths([zlib_lib.join("pkgconfig"), zstd_lib.join("pkgconfig")])?
                .to_string_lossy()
                .to_string(),
        ),
    ];
    let state = fs::read_to_string(repo_root.join("upstream/state/elfutils.toml"))
        .context("failed to read elfutils upstream state")?;
    let stamp = format!(
        "{state}\n{}\n{}\n",
        options.join("\n"),
        env.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "autoreconf", &["-fi"])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
            &env,
        )?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-C", "lib", "-j", "4"], &env)?;
    run_cmd_with_env_overrides(&build_dir, "make", &["-C", "libelf", "-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[
            "-C",
            "libelf",
            "install",
            &format!("DESTDIR={}", install_dir.display()),
        ],
        &env,
    )?;
    let pkgconfig = install_dir.join("usr/lib/x86_64-linux-gnu/pkgconfig");
    fs::create_dir_all(&pkgconfig)?;
    fs::copy(
        build_dir.join("config/libelf.pc"),
        pkgconfig.join("libelf.pc"),
    )?;
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    if !libdir.join("libelf.so.1").exists() {
        bail!(
            "elfutils install did not produce {}",
            libdir.join("libelf.so.1").display()
        )
    }
    let search_dirs: [&Path; 3] = [&libdir, &zlib_lib, &zstd_lib];
    validate_dependency_resolves_from(
        &libdir.join("libelf.so.1"),
        "libz.so.1",
        &zlib_lib,
        &search_dirs,
    )?;
    validate_dependency_resolves_from(
        &libdir.join("libelf.so.1"),
        "libzstd.so.1",
        &zstd_lib,
        &search_dirs,
    )?;
    fs::write(&stamp_path, stamp)?;
    println!(
        "libelf origins: zlib={} zstd={}",
        zlib_lib.display(),
        zstd_lib.display()
    );
    Ok(())
}

fn build_pcre2(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/pcre2");
    if !source.join("CMakeLists.txt").is_file() {
        bail!(
            "PCRE2 source not found in {}; run upstream import pcre2 first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/pcre2");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/pcre2.toml"))
        .context("failed to read PCRE2 upstream state")?;
    let sljit = repo_root.join("src/build-support/sljit");
    if !sljit.join("sljit_src/sljitLir.c").is_file() {
        bail!("PCRE2 SLJIT source is missing; run upstream import sljit first");
    }
    let sljit_state = fs::read_to_string(repo_root.join("upstream/state/sljit.toml"))
        .context("failed to read SLJIT upstream state")?;
    let options = [
        "-G",
        "Ninja",
        "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_INSTALL_PREFIX=/usr",
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
        "-DBUILD_SHARED_LIBS=ON",
        "-DBUILD_STATIC_LIBS=OFF",
        "-DPCRE2_BUILD_PCRE2_8=ON",
        "-DPCRE2_BUILD_PCRE2_16=OFF",
        "-DPCRE2_BUILD_PCRE2_32=OFF",
        "-DPCRE2_BUILD_PCRE2GREP=OFF",
        "-DPCRE2_BUILD_TESTS=OFF",
        "-DPCRE2_SUPPORT_JIT=ON",
        "-DPCRE2_SUPPORT_UNICODE=ON",
        "-DPCRE2_SYMVERS=ON",
    ];
    let stamp = format!("{state}\n{sljit_state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    sync_build_source(&sljit, &source_copy.join("deps/sljit"))?;
    if !build_dir.join("build.ninja").is_file() {
        let mut args = vec!["-S", path_str(&source_copy)?, "-B", path_str(&build_dir)?];
        args.extend(options);
        run_cmd(repo_root, "cmake", &args)?;
    }
    run_cmd(
        repo_root,
        "cmake",
        &["--build", path_str(&build_dir)?, "--parallel", "4"],
    )?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build_dir)?],
        &[("DESTDIR", install_dir.display().to_string())],
    )?;
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    let soname = libdir.join("libpcre2-8.so.0");
    if !soname.exists() {
        bail!("PCRE2 install did not produce {}", soname.display());
    }
    for unwanted in ["libpcre2-16.so", "libpcre2-32.so"] {
        if libdir.join(unwanted).exists() {
            bail!("PCRE2 unexpectedly built non-runtime variant {unwanted}");
        }
    }
    fs::write(&stamp_path, stamp)?;
    println!("PCRE2 origin: {}", install_dir.display());
    Ok(())
}

fn build_selinux(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/security/selinux");
    if !source.join("libselinux/src/Makefile").is_file() {
        bail!(
            "SELinux source not found in {}; run upstream import selinux first",
            source.display()
        );
    }
    let pcre2 = repo_root.join("out/build/pcre2/install/usr");
    let pcre2_lib = pcre2.join("lib/x86_64-linux-gnu");
    if !pcre2.join("include/pcre2.h").is_file() || !pcre2_lib.join("libpcre2-8.so").exists() {
        bail!("MattOS-built PCRE2 development files are missing; run build pcre2 first");
    }
    let out_root = repo_root.join("out/build/selinux");
    let source_copy = out_root.join("source");
    let sepol_install = out_root.join("sepol-install");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/selinux.toml"))
        .context("failed to read SELinux upstream state")?;
    let pcre2_state = fs::read_to_string(repo_root.join("upstream/state/pcre2.toml"))
        .context("failed to read PCRE2 upstream state")?;
    let make_vars = [
        "PREFIX=/usr".to_string(),
        "LIBDIR=/usr/lib/x86_64-linux-gnu".to_string(),
        "SHLIBDIR=/usr/lib/x86_64-linux-gnu".to_string(),
        "USE_PCRE2=y".to_string(),
        "DISABLE_SETRANS=y".to_string(),
        "DISABLE_RPM=y".to_string(),
        format!(
            "PCRE_CFLAGS=-DUSE_PCRE2 -DPCRE2_CODE_UNIT_WIDTH=8 -I{}",
            pcre2.join("include").display()
        ),
        format!("PCRE_LDLIBS=-L{} -lpcre2-8", pcre2_lib.display()),
    ];
    let sepol_make_vars = [
        "PREFIX=/usr".to_string(),
        "LIBDIR=/usr/lib/x86_64-linux-gnu".to_string(),
        "SHLIBDIR=/usr/lib/x86_64-linux-gnu".to_string(),
        "DISABLE_CIL=y".to_string(),
        "DISABLE_SHARED=y".to_string(),
    ];
    let library_path = pcre2_lib.display().to_string();
    let env = [
        ("LDFLAGS", format!("-L{}", pcre2_lib.display())),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        (
            "PKG_CONFIG_PATH",
            pcre2_lib.join("pkgconfig").display().to_string(),
        ),
    ];
    let stamp = format!(
        "{state}\n{pcre2_state}\n{}\n{}\n{}\n",
        make_vars.join("\n"),
        sepol_make_vars.join("\n"),
        env.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    let libsepol = source_copy.join("libsepol");
    let mut sepol_build_args = vec!["-C", "src", "-j", "4", "all"];
    sepol_build_args.extend(sepol_make_vars.iter().map(String::as_str));
    run_cmd(&libsepol, "make", &sepol_build_args)?;
    remove_path_if_exists(&sepol_install)?;
    let sepol_destdir = format!("DESTDIR={}", sepol_install.display());
    let mut sepol_install_args = vec!["-C", "src", "install", sepol_destdir.as_str()];
    sepol_install_args.extend(sepol_make_vars.iter().map(String::as_str));
    run_cmd(&libsepol, "make", &sepol_install_args)?;
    run_cmd(
        &libsepol,
        "make",
        &[
            "-C",
            "include",
            "install",
            sepol_destdir.as_str(),
            "PREFIX=/usr",
        ],
    )?;
    let sepol_lib = sepol_install.join("usr/lib/x86_64-linux-gnu");
    if !sepol_install.join("usr/include/sepol/sepol.h").is_file()
        || !sepol_lib.join("libsepol.a").is_file()
    {
        bail!("MattOS-built libsepol development files are incomplete");
    }
    copy_tree_contents(
        &sepol_install.join("usr/include"),
        &repo_root.join("out/sysroot/usr/include"),
    )?;
    copy_tree_contents(
        &sepol_lib,
        &repo_root.join("out/sysroot/usr/lib/x86_64-linux-gnu"),
    )?;
    let libselinux = source_copy.join("libselinux");
    let mut build_args = vec!["-C", "src", "-j", "4", "all"];
    build_args.extend(make_vars.iter().map(String::as_str));
    run_cmd_with_env_overrides(&libselinux, "make", &build_args, &env)?;
    remove_path_if_exists(&install_dir)?;
    let destdir = format!("DESTDIR={}", install_dir.display());
    let mut install_args = vec!["-C", "src", "install", destdir.as_str()];
    install_args.extend(make_vars.iter().map(String::as_str));
    run_cmd_with_env_overrides(&libselinux, "make", &install_args, &env)?;
    run_cmd(
        &libselinux,
        "make",
        &["-C", "include", "install", destdir.as_str(), "PREFIX=/usr"],
    )?;
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    let soname = libdir.join("libselinux.so.1");
    if !soname.exists() {
        bail!("SELinux install did not produce {}", soname.display());
    }
    validate_dependency_resolves_from(&soname, "libpcre2-8.so.0", &pcre2_lib, &[&pcre2_lib])?;
    let dynamic = run_cmd_capture(repo_root, "readelf", &["-d", path_str(&soname)?])?;
    if dynamic.contains("libsepol.so") {
        bail!("libselinux unexpectedly retained a dynamic libsepol dependency");
    }
    fs::write(&stamp_path, stamp)?;
    println!(
        "libselinux origin: {}; PCRE2 origin: {}",
        install_dir.display(),
        pcre2_lib.display()
    );
    Ok(())
}

const LIBXCRYPT_REQUIRED_SYMBOL_VERSIONS: &[&str] =
    &["GLIBC_2.2.5", "XCRYPT_2.0", "XCRYPT_4.3", "XCRYPT_4.4"];

fn libxcrypt_configure_options() -> [&'static str; 7] {
    [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--enable-shared",
        "--enable-hashes=all",
        "--enable-obsolete-api=glibc",
        "--disable-xcrypt-compat-files",
    ]
}

fn build_libxcrypt(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/libxcrypt");
    if !source.join("configure.ac").is_file() {
        bail!(
            "libxcrypt source not found in {}; run upstream import libxcrypt first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/libxcrypt");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/libxcrypt.toml"))
        .context("failed to read libxcrypt upstream state")?;
    let options = libxcrypt_configure_options();
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "./autogen.sh", &[])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
        )?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    run_cmd(&build_dir, "make", &["check", "-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libcrypt.so.1");
    if !soname.exists() {
        bail!("libxcrypt install did not produce {}", soname.display());
    }
    let versions = run_cmd_capture(
        repo_root,
        "readelf",
        &["--version-info", path_str(&soname)?],
    )?;
    for required in LIBXCRYPT_REQUIRED_SYMBOL_VERSIONS {
        if !versions.contains(required) {
            bail!("libxcrypt is missing required symbol version {required}");
        }
    }
    fs::write(&stamp_path, stamp)?;
    println!(
        "libxcrypt origin: {}; yescrypt covered by upstream check suite",
        install_dir.display()
    );
    Ok(())
}

fn build_libmd(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/libmd");
    if !source.join("configure.ac").is_file() {
        bail!(
            "libmd source not found in {}; run upstream import libmd first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/libmd");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/libmd.toml"))
        .context("failed to read libmd upstream state")?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    fs::write(source_copy.join(".dist-version"), "1.2.0\n")?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "./autogen", &[])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
        )?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libmd.so.0");
    if !soname.exists() {
        bail!("libmd install did not produce {}", soname.display());
    }
    remove_path_if_exists(&install_dir.join("usr/lib/x86_64-linux-gnu/libmd.la"))?;
    fs::write(&stamp_path, stamp)?;
    println!("libmd origin: {}", install_dir.display());
    Ok(())
}

fn build_libbsd(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/libbsd");
    if !source.join("configure.ac").is_file() {
        bail!(
            "libbsd source not found in {}; run upstream import libbsd first",
            source.display()
        );
    }
    let libmd_install = repo_root.join("out/build/libmd/install/usr");
    let libmd_lib = libmd_install.join("lib/x86_64-linux-gnu");
    if !libmd_install.join("include/md5.h").is_file() || !libmd_lib.join("libmd.so").exists() {
        bail!(
            "MattOS-built libmd development files missing at {}; run build libmd first",
            libmd_install.display()
        );
    }
    let out_root = repo_root.join("out/build/libbsd");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/libbsd.toml"))
        .context("failed to read libbsd upstream state")?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
    ];
    let env_overrides = [
        (
            "CPPFLAGS",
            format!("-I{}", libmd_install.join("include").display()),
        ),
        ("LDFLAGS", format!("-L{}", libmd_lib.display())),
        ("LIBRARY_PATH", libmd_lib.display().to_string()),
        ("LD_LIBRARY_PATH", libmd_lib.display().to_string()),
        (
            "PKG_CONFIG_PATH",
            libmd_lib.join("pkgconfig").display().to_string(),
        ),
    ];
    let stamp = format!(
        "{state}\n{}\n{}\n",
        options.join("\n"),
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    fs::write(source_copy.join(".dist-version"), "0.12.2\n")?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "./autogen", &[])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
            &env_overrides,
        )?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env_overrides)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env_overrides,
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libbsd.so.0");
    if !soname.exists() {
        bail!("libbsd install did not produce {}", soname.display());
    }
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    let linker_name = libdir.join("libbsd.so");
    let versioned_target = fs::read_link(&soname).context("libbsd SONAME link is not a symlink")?;
    remove_path_if_exists(&linker_name)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&versioned_target, &linker_name)?;
    remove_path_if_exists(&libdir.join("libbsd.la"))?;
    validate_dependency_resolves_from(&soname, "libmd.so.0", &libmd_lib, &[&libmd_lib])?;
    fs::write(&stamp_path, stamp)?;
    println!(
        "libbsd origin: {}; libmd origin: {}",
        install_dir.display(),
        libmd_lib.display()
    );
    Ok(())
}

fn build_tar(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/tar");
    let paxutils = repo_root.join("src/build-support/paxutils");
    if !source.join("bootstrap").is_file() {
        bail!(
            "GNU tar source not found in {}; run upstream import tar first",
            source.display()
        );
    }
    if !paxutils.join("DISTFILES").is_file() {
        bail!(
            "GNU paxutils build support not found in {}; run upstream import paxutils first",
            paxutils.display()
        );
    }
    let acl_install = repo_root.join("out/build/acl/install");
    let acl_libdir = acl_install.join("usr/lib/x86_64-linux-gnu");
    if !acl_install.join("usr/include/sys/acl.h").is_file()
        || !acl_libdir.join("libacl.so").exists()
    {
        bail!(
            "MattOS-built ACL development files missing at {}; run build acl first",
            acl_install.display()
        );
    }
    let out_root = repo_root.join("out/build/tar");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/tar.toml"))
        .context("failed to read GNU tar upstream state")?;
    let paxutils_state = fs::read_to_string(repo_root.join("upstream/state/paxutils.toml"))
        .context("failed to read paxutils upstream state")?;
    let acl_state = fs::read_to_string(repo_root.join("upstream/state/acl.toml"))
        .context("failed to read ACL upstream state")?;
    let options = [
        "--prefix=/usr",
        "--disable-nls",
        "--without-selinux",
        "--with-posix-acls",
    ];
    let stamp = format!(
        "{state}\n{paxutils_state}\n{acl_state}\n{}\n",
        options.join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    sync_build_source(&paxutils, &source_copy.join("paxutils"))?;
    if !source_copy.join("configure").is_file() {
        run_cmd(
            &source_copy,
            "./bootstrap",
            &[
                "--gen",
                "--force",
                "--no-git",
                "--skip-po",
                "--copy",
                "--no-bootstrap-sync",
                "--gnulib-srcdir=/usr/share/gnulib",
            ],
        )?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    let include = acl_install.join("usr/include").display().to_string();
    let lib = acl_libdir.display().to_string();
    let pkgconfig = acl_libdir.join("pkgconfig").display().to_string();
    let configure_env = [
        ("CPPFLAGS", format!("-I{include}")),
        ("LDFLAGS", format!("-L{lib}")),
        ("LD_LIBRARY_PATH", lib.clone()),
        ("PKG_CONFIG_PATH", pkgconfig),
    ];
    if !build_dir.join("Makefile").is_file() {
        let configure = source_copy.join("configure");
        run_cmd_with_env_overrides(&build_dir, path_str(&configure)?, &options, &configure_env)?;
    }
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["-j", "4", "MAKEINFO=true"],
        &configure_env,
    )?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[
            "install",
            "MAKEINFO=true",
            &format!("DESTDIR={}", install_dir.display()),
        ],
        &configure_env,
    )?;
    let tar = install_dir.join("usr/bin/tar");
    if !tar.is_file() {
        bail!("GNU tar install did not produce {}", tar.display());
    }
    validate_dependency_resolves_from(&tar, "libacl.so.1", &acl_libdir, &[&acl_libdir])?;
    let needed = run_cmd_capture(repo_root, "readelf", &["-d", path_str(&tar)?])?;
    if needed.contains("libselinux.so") {
        bail!("MattOS GNU tar unexpectedly links against host SELinux");
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn validate_dependency_resolves_from(
    binary: &Path,
    soname: &str,
    expected_dir: &Path,
    search_dirs: &[&Path],
) -> Result<()> {
    let library_path = std::env::join_paths(search_dirs)?;
    let output = Command::new("ldd")
        .arg(binary)
        .env("LD_LIBRARY_PATH", library_path)
        .output()
        .with_context(|| format!("failed to inspect {} with ldd", binary.display()))?;
    let stdout = String::from_utf8(output.stdout)?;
    if !output.status.success() || stdout.contains("not found") {
        bail!(
            "unresolved runtime dependency for {}:\n{stdout}",
            binary.display()
        );
    }
    let resolved = stdout
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            if fields.next()? != soname || fields.next()? != "=>" {
                return None;
            }
            Some(PathBuf::from(fields.next()?))
        })
        .ok_or_else(|| {
            anyhow!(
                "{} does not resolve required dependency {soname}",
                binary.display()
            )
        })?;
    let canonical_expected = fs::canonicalize(expected_dir)?;
    let canonical_resolved = fs::canonicalize(&resolved).with_context(|| {
        format!(
            "unable to canonicalize {soname} resolution {}",
            resolved.display()
        )
    })?;
    if !canonical_resolved.starts_with(&canonical_expected) {
        bail!(
            "{} unexpectedly resolves {soname} from host path {}; expected {}",
            binary.display(),
            canonical_resolved.display(),
            canonical_expected.display()
        );
    }
    Ok(())
}

fn build_iproute2(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/iproute2");
    if !source.join("Makefile").exists() {
        bail!(
            "iproute2 source not found in {}; run upstream import iproute2 first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/iproute2");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let libcap_install = repo_root.join("out/build/libcap/install/usr");
    let libcap_lib = libcap_install.join("lib/x86_64-linux-gnu");
    let libcap_pc = libcap_lib.join("pkgconfig");
    let libelf_install = repo_root.join("out/build/elfutils/install/usr");
    let libelf_lib = libelf_install.join("lib/x86_64-linux-gnu");
    let zlib_install = repo_root.join("out/build/zlib/install/usr");
    let zlib_lib = zlib_install.join("lib/x86_64-linux-gnu");
    let zstd_install = repo_root.join("out/build/zstd/install/usr");
    let zstd_lib = zstd_install.join("lib/x86_64-linux-gnu");
    let selinux_install = repo_root.join("out/build/selinux/install/usr");
    let selinux_lib = selinux_install.join("lib/x86_64-linux-gnu");
    let pcre2_install = repo_root.join("out/build/pcre2/install/usr");
    let pcre2_lib = pcre2_install.join("lib/x86_64-linux-gnu");
    if !libcap_lib.join("libcap.so").exists()
        || !libcap_pc.join("libcap.pc").is_file()
        || !libelf_lib.join("libelf.so").exists()
        || !libelf_lib.join("pkgconfig/libelf.pc").is_file()
        || !zlib_lib.join("libz.so").exists()
        || !zstd_lib.join("libzstd.so").exists()
        || !selinux_lib.join("libselinux.so").exists()
        || !pcre2_lib.join("libpcre2-8.so").exists()
    {
        bail!(
            "MattOS iproute2 development files are missing; run build libcap, elfutils, zlib, zstd, pcre2, and selinux first"
        );
    }
    let library_path = std::env::join_paths([
        &libcap_lib,
        &libelf_lib,
        &zlib_lib,
        &zstd_lib,
        &selinux_lib,
        &pcre2_lib,
    ])?
    .to_string_lossy()
    .to_string();
    let env = vec![
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths([
                libcap_pc,
                libelf_lib.join("pkgconfig"),
                zlib_lib.join("pkgconfig"),
                zstd_lib.join("pkgconfig"),
                selinux_lib.join("pkgconfig"),
                pcre2_lib.join("pkgconfig"),
            ])?
            .to_string_lossy()
            .to_string(),
        ),
        (
            "CPPFLAGS",
            format!(
                "-I{} -I{} -I{} -I{} -I{} -I{}",
                libcap_install.join("include").display(),
                libelf_install.join("include").display(),
                zlib_install.join("include").display(),
                zstd_install.join("include").display(),
                selinux_install.join("include").display(),
                pcre2_install.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!(
                "-L{} -L{} -L{} -L{} -L{} -L{}",
                libcap_lib.display(),
                libelf_lib.display(),
                zlib_lib.display(),
                zstd_lib.display(),
                selinux_lib.display(),
                pcre2_lib.display()
            ),
        ),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
    ];
    let state = fs::read_to_string(repo_root.join("upstream/state/iproute2.toml"))
        .context("failed to read iproute2 upstream state")?;
    let libcap_state = fs::read_to_string(repo_root.join("upstream/state/libcap.toml"))
        .context("failed to read libcap upstream state")?;
    let libelf_state = fs::read_to_string(repo_root.join("upstream/state/elfutils.toml"))
        .context("failed to read elfutils upstream state")?;
    let zstd_state = fs::read_to_string(repo_root.join("upstream/state/zstd.toml"))
        .context("failed to read Zstandard upstream state")?;
    let selinux_state = fs::read_to_string(repo_root.join("upstream/state/selinux.toml"))
        .context("failed to read SELinux upstream state")?;
    let pcre2_state = fs::read_to_string(repo_root.join("upstream/state/pcre2.toml"))
        .context("failed to read PCRE2 upstream state")?;
    let stamp = format!(
        "{state}\n{libcap_state}\n{libelf_state}\n{zstd_state}\n{selinux_state}\n{pcre2_state}\nPREFIX=/usr\nSBINDIR=/usr/sbin\nLIBDIR=/usr/lib/x86_64-linux-gnu\nSHARED_LIBS=n\n{}\n",
        env.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &build_dir)?;
    if !build_dir.join("config.mk").exists() {
        run_cmd_with_env_overrides(
            &build_dir,
            "./configure",
            &["--prefix=/usr", "--libdir=/usr/lib/x86_64-linux-gnu"],
            &env,
        )?;
    }
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[
            "-j",
            "4",
            "PREFIX=/usr",
            "SBINDIR=/usr/sbin",
            "SHARED_LIBS=n",
        ],
        &env,
    )?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    let destdir = format!("DESTDIR={}", install_dir.display());
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[
            "install",
            &destdir,
            "PREFIX=/usr",
            "SBINDIR=/usr/sbin",
            "SHARED_LIBS=n",
        ],
        &env,
    )?;
    let runtime_dirs: [&Path; 6] = [
        &libcap_lib,
        &libelf_lib,
        &zlib_lib,
        &zstd_lib,
        &selinux_lib,
        &pcre2_lib,
    ];
    for binary in IPROUTE2_BINARIES {
        let installed = install_dir.join(binary.source_rel);
        if !installed.exists() {
            bail!("iproute2 install did not produce {}", binary.source_rel);
        }
        validate_dependency_resolves_from(&installed, "libcap.so.2", &libcap_lib, &runtime_dirs)?;
    }
    for rel in ["usr/sbin/ip", "usr/sbin/tc"] {
        let installed = install_dir.join(rel);
        validate_dependency_resolves_from(&installed, "libelf.so.1", &libelf_lib, &runtime_dirs)?;
        validate_dependency_resolves_from(&installed, "libzstd.so.1", &zstd_lib, &runtime_dirs)?;
    }
    for rel in ["usr/sbin/ip", "usr/sbin/ss"] {
        validate_dependency_resolves_from(
            &install_dir.join(rel),
            "libselinux.so.1",
            &selinux_lib,
            &runtime_dirs,
        )?;
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_iputils(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/iputils");
    if !source.join("meson.build").exists() {
        bail!(
            "iputils source not found in {}; run upstream import iputils first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/iputils");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let options_path = out_root.join("meson-options.txt");
    let options = vec![
        "--prefix=/usr",
        "--bindir=bin",
        "--sbindir=sbin",
        "-DUSE_CAP=false",
        "-DUSE_IDN=false",
        "-DUSE_GETTEXT=false",
        "-DBUILD_ARPING=false",
        "-DBUILD_CLOCKDIFF=false",
        "-DBUILD_PING=true",
        "-DBUILD_TRACEPATH=true",
        "-DBUILD_MANS=false",
        "-DBUILD_HTML_MANS=false",
        "-DNO_SETCAP_OR_SUID=true",
        "-DINSTALL_SYSTEMD_UNITS=false",
        "-DSKIP_TESTS=true",
    ];
    let options_text = format!("{}\n", options.join("\n"));
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    let configured = build_dir.join("build.ninja").exists();
    if !configured {
        let mut args = vec!["setup", path_str(&build_dir)?, path_str(&source)?];
        args.extend(options.iter().copied());
        run_cmd(repo_root, "meson", &args)?;
    } else if fs::read_to_string(&options_path).ok().as_deref() != Some(options_text.as_str()) {
        let mut args = vec![
            "setup",
            "--reconfigure",
            path_str(&build_dir)?,
            path_str(&source)?,
        ];
        args.extend(options.iter().copied());
        run_cmd(repo_root, "meson", &args)?;
    }
    fs::write(&options_path, &options_text)
        .with_context(|| format!("failed to write {}", options_path.display()))?;
    run_cmd(repo_root, "ninja", &["-C", path_str(&build_dir)?])?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            path_str(&build_dir)?,
            "--no-rebuild",
            "--destdir",
            path_str(&install_dir)?,
        ],
    )?;
    for binary in IPUTILS_BINARIES {
        if !install_dir.join(binary.source_rel).exists() {
            bail!("iputils install did not produce {}", binary.source_rel);
        }
    }
    Ok(())
}

fn curl_configure_options() -> Vec<&'static str> {
    vec![
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--sysconfdir=/etc",
        "--with-openssl",
        "--with-ca-bundle=/etc/ssl/certs/ca-certificates.crt",
        "--without-ca-path",
        "--enable-http",
        "--disable-static",
        "--enable-shared",
        "--disable-ipv6",
        "--disable-threaded-resolver",
        "--disable-manual",
        "--disable-docs",
        "--disable-libcurl-option",
        "--disable-ipfs",
        "--disable-websockets",
        "--disable-ftp",
        "--disable-file",
        "--disable-ldap",
        "--disable-ldaps",
        "--disable-rtsp",
        "--disable-dict",
        "--disable-telnet",
        "--disable-tftp",
        "--disable-pop3",
        "--disable-imap",
        "--disable-smb",
        "--disable-smtp",
        "--disable-gopher",
        "--disable-mqtt",
        "--without-libpsl",
        "--without-zlib",
        "--without-brotli",
        "--without-zstd",
        "--without-libidn2",
        "--without-nghttp2",
        "--without-ngtcp2",
        "--without-nghttp3",
        "--without-libssh2",
        "--disable-dependency-tracking",
    ]
}

fn build_curl(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/curl");
    if !source.join("configure.ac").exists() {
        bail!(
            "curl source not found in {}; run upstream import curl first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/curl");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/curl.toml"))
        .context("failed to read curl upstream state")?;
    let openssl = repo_root.join("out/build/openssl/install/usr");
    let openssl_lib = openssl.join("lib/x86_64-linux-gnu");
    let zlib = repo_root.join("out/build/zlib/install/usr");
    let zlib_lib = zlib.join("lib/x86_64-linux-gnu");
    let zstd = repo_root.join("out/build/zstd/install/usr");
    let zstd_lib = zstd.join("lib/x86_64-linux-gnu");
    if !openssl_lib.join("libcrypto.so").exists()
        || !openssl_lib.join("libssl.so").exists()
        || !zlib_lib.join("libz.so").exists()
        || !zstd_lib.join("libzstd.so").exists()
    {
        bail!("MattOS curl TLS dependencies are missing; run build openssl, zlib, and zstd first")
    }
    let options = curl_configure_options();
    let openssl_state = fs::read_to_string(repo_root.join("upstream/state/openssl.toml"))
        .context("failed to read OpenSSL upstream state")?;
    let library_path = std::env::join_paths([&openssl_lib, &zlib_lib, &zstd_lib])?
        .to_string_lossy()
        .to_string();
    let env = [
        (
            "CPPFLAGS",
            format!(
                "-I{} -I{} -I{}",
                openssl.join("include").display(),
                zlib.join("include").display(),
                zstd.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!(
                "-L{} -L{} -L{}",
                openssl_lib.display(),
                zlib_lib.display(),
                zstd_lib.display()
            ),
        ),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths([
                openssl_lib.join("pkgconfig"),
                zlib_lib.join("pkgconfig"),
                zstd_lib.join("pkgconfig"),
            ])?
            .to_string_lossy()
            .to_string(),
        ),
    ];
    let stamp = format!(
        "{state}\n{openssl_state}\n{}\n{}\n",
        options.join("\n"),
        env.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").exists() {
        run_cmd(&source_copy, "autoreconf", &["-fi"])?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").exists() {
        let configure = source_copy.join("configure");
        run_cmd_with_env_overrides(&build_dir, path_str(&configure)?, &options, &env)?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    let destdir = format!("DESTDIR={}", install_dir.display());
    run_cmd_with_env_overrides(&build_dir, "make", &["install", &destdir], &env)?;
    for binary in CURL_BINARIES {
        if !install_dir.join(binary.source_rel).exists() {
            bail!("curl install did not produce {}", binary.source_rel);
        }
    }
    let runtime_dirs: [&Path; 3] = [&openssl_lib, &zlib_lib, &zstd_lib];
    let libcurl = install_dir.join("usr/lib/x86_64-linux-gnu/libcurl.so.4.8.0");
    validate_dependency_resolves_from(&libcurl, "libssl.so.3", &openssl_lib, &runtime_dirs)?;
    validate_dependency_resolves_from(&libcurl, "libcrypto.so.3", &openssl_lib, &runtime_dirs)?;
    validate_dependency_resolves_from(&libcurl, "libzstd.so.1", &zstd_lib, &runtime_dirs)?;
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("invalid path {}", path.display()))
}

fn build_systemd(repo_root: &Path) -> Result<()> {
    let systemd_src = repo_root.join("src/system/systemd");
    if !systemd_src.join("meson.build").exists() {
        bail!(
            "systemd source not found in {}; run upstream import systemd first",
            systemd_src.display()
        );
    }

    let out_root = repo_root.join("out/build/systemd");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let options_path = out_root.join("meson-options.txt");
    let env_path = out_root.join("meson-env.txt");
    let kmod_install = repo_root.join("out/build/kmod/install/usr");
    if !kmod_install
        .join("lib/x86_64-linux-gnu/libkmod.so.2")
        .exists()
    {
        bail!(
            "kmod development files missing at {}; run build kmod first",
            kmod_install.display()
        );
    }
    let util_linux_install = repo_root.join("out/build/util-linux/install/usr");
    let util_linux_lib = util_linux_install.join("lib/x86_64-linux-gnu");
    if !util_linux_lib.join("libmount.so.1").exists()
        || !util_linux_lib.join("pkgconfig/mount.pc").exists()
    {
        bail!(
            "util-linux libmount development files missing at {}; run build util-linux first",
            util_linux_install.display()
        );
    }
    let dependency_installs = [
        repo_root.join("out/build/zlib/install/usr"),
        repo_root.join("out/build/bzip2/install/usr"),
        repo_root.join("out/build/lz4/install/usr"),
        repo_root.join("out/build/xz/install/usr"),
        repo_root.join("out/build/zstd/install/usr"),
        repo_root.join("out/build/elfutils/install/usr"),
        repo_root.join("out/build/pcre2/install/usr"),
        repo_root.join("out/build/selinux/install/usr"),
        repo_root.join("out/build/selinux/sepol-install/usr"),
        repo_root.join("out/build/libxcrypt/install/usr"),
        repo_root.join("out/build/linux-pam/install/usr"),
    ];
    for install in &dependency_installs {
        if !install.join("include").is_dir() || !install.join("lib/x86_64-linux-gnu").is_dir() {
            bail!(
                "systemd source-built dependency is incomplete at {}",
                install.display()
            );
        }
    }
    let mut include_dirs = vec![
        kmod_install.join("include"),
        util_linux_install.join("include"),
    ];
    include_dirs.extend(
        dependency_installs
            .iter()
            .map(|install| install.join("include")),
    );
    let mut library_dirs = vec![
        kmod_install.join("lib/x86_64-linux-gnu"),
        util_linux_lib.clone(),
    ];
    library_dirs.extend(
        dependency_installs
            .iter()
            .map(|install| install.join("lib/x86_64-linux-gnu")),
    );
    let mut sysroot_installs = dependency_installs.to_vec();
    sysroot_installs.push(kmod_install.clone());
    sysroot_installs.push(util_linux_install.clone());
    hydrate_development_sysroot(repo_root, &sysroot_installs)?;
    let pkgconfig_dirs = library_dirs
        .iter()
        .map(|library| library.join("pkgconfig"))
        .filter(|directory| directory.is_dir())
        .collect::<Vec<_>>();
    let system_library_path = std::env::join_paths(library_dirs.iter())?
        .to_string_lossy()
        .to_string();
    let pkgconfig_path = std::env::join_paths(pkgconfig_dirs.iter())?
        .to_string_lossy()
        .to_string();
    let mut cflags = include_dirs
        .iter()
        .map(|include| format!("-I{}", include.display()))
        .collect::<Vec<_>>()
        .join(" ");
    cflags.push_str(&format!(
        " -ffile-prefix-map={}=/usr/src/mattos -fdebug-prefix-map={}=/usr/src/mattos -fmacro-prefix-map={}=/usr/src/mattos",
        repo_root.display(),
        repo_root.display(),
        repo_root.display()
    ));
    let ldflags = library_dirs
        .iter()
        .flat_map(|library| {
            [
                format!("-L{}", library.display()),
                format!("-Wl,-rpath-link,{}", library.display()),
            ]
        })
        .collect::<Vec<_>>()
        .join(" ");
    let env_overrides = vec![
        ("PKG_CONFIG_PATH", pkgconfig_path.clone()),
        ("PKG_CONFIG_LIBDIR", pkgconfig_path),
        (
            "PKG_CONFIG_SYSROOT_DIR",
            repo_root.join("out/sysroot").display().to_string(),
        ),
        ("CFLAGS", cflags),
        ("LDFLAGS", ldflags),
        ("LIBRARY_PATH", system_library_path.clone()),
        ("LD_LIBRARY_PATH", system_library_path),
    ];
    let env_text = format!(
        "{}\n",
        env_overrides
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;

    let options = systemd_meson_options();
    let options_text = format!("{}\n", options.join("\n"));
    let existing_options = fs::read_to_string(&options_path).ok();
    let needs_reconfigure = existing_options.as_deref() != Some(options_text.as_str());
    let mut configured = build_dir.join("build.ninja").exists();
    if configured && fs::read_to_string(&env_path).ok().as_deref() != Some(env_text.as_str()) {
        remove_path_if_exists(&build_dir)?;
        configured = false;
    }

    if !configured {
        let mut setup_args = vec![
            "setup".to_string(),
            build_dir.display().to_string(),
            systemd_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
        fs::write(&env_path, &env_text)
            .with_context(|| format!("failed to write {}", env_path.display()))?;
    } else if needs_reconfigure {
        let mut setup_args = vec![
            "setup".to_string(),
            "--reconfigure".to_string(),
            build_dir.display().to_string(),
            systemd_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
        fs::write(&env_path, &env_text)
            .with_context(|| format!("failed to write {}", env_path.display()))?;
    }

    let ninja_args = vec![
        "-C",
        build_dir
            .to_str()
            .ok_or_else(|| anyhow!("invalid build dir"))?,
    ];
    run_cmd_with_env_overrides(repo_root, "ninja", &ninja_args, &env_overrides)?;

    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)
            .with_context(|| format!("failed to clean {}", install_dir.display()))?;
    }
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;

    let install_args = vec![
        "install",
        "-C",
        build_dir
            .to_str()
            .ok_or_else(|| anyhow!("invalid build dir"))?,
        "--no-rebuild",
        "--destdir",
        install_dir
            .to_str()
            .ok_or_else(|| anyhow!("invalid install dir"))?,
    ];
    run_cmd_with_env_overrides(repo_root, "meson", &install_args, &env_overrides)?;

    let pid1 = install_dir.join("usr/lib/systemd/systemd");
    if !pid1.exists() {
        bail!("systemd install did not produce {}", pid1.display());
    }

    Ok(())
}

fn systemd_meson_options() -> Vec<String> {
    vec![
        "--prefix=/usr".to_string(),
        "--sysconfdir=/etc".to_string(),
        "--localstatedir=/var".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        "-Dmode=release".to_string(),
        "-Dtests=false".to_string(),
        "-Dman=disabled".to_string(),
        "-Dhtml=disabled".to_string(),
        "-Dtranslations=false".to_string(),
        "-Dnetworkd=true".to_string(),
        "-Dresolve=true".to_string(),
        "-Dtimesyncd=true".to_string(),
        "-Dsystemd-network-uid=192".to_string(),
        "-Dsystemd-resolve-uid=193".to_string(),
        "-Dsystemd-timesync-uid=194".to_string(),
        "-Dhomed=disabled".to_string(),
        "-Dportabled=false".to_string(),
        "-Dnspawn=disabled".to_string(),
        "-Dbootloader=disabled".to_string(),
        "-Dfirstboot=false".to_string(),
        "-Drepart=disabled".to_string(),
        "-Doomd=false".to_string(),
        "-Duserdb=false".to_string(),
        "-Dremote=disabled".to_string(),
        "-Dsysupdate=disabled".to_string(),
        "-Dsysupdated=disabled".to_string(),
        "-Dsysinstall=false".to_string(),
        "-Dimportd=disabled".to_string(),
        "-Dvmspawn=disabled".to_string(),
        "-Dcoredump=false".to_string(),
        "-Dpstore=false".to_string(),
        "-Dmachined=false".to_string(),
        "-Dhostnamed=false".to_string(),
        "-Dlocaled=false".to_string(),
        "-Dtimedated=true".to_string(),
        "-Dnsresourced=false".to_string(),
        "-Ddefault-network=false".to_string(),
        "-Ddbus=disabled".to_string(),
        "-Dglib=disabled".to_string(),
        "-Dseccomp=disabled".to_string(),
        "-Dselinux=enabled".to_string(),
        "-Dacl=disabled".to_string(),
        "-Daudit=disabled".to_string(),
        "-Dblkid=disabled".to_string(),
        "-Dkmod=enabled".to_string(),
        "-Dlibmount=enabled".to_string(),
        "-Dpam=enabled".to_string(),
        "-Dlibcrypt=enabled".to_string(),
        "-Dlibcryptsetup=disabled".to_string(),
        "-Dopenssl=disabled".to_string(),
        "-Dlibidn2=disabled".to_string(),
        "-Dgnutls=disabled".to_string(),
        "-Dlibfido2=disabled".to_string(),
        "-Dtpm=false".to_string(),
        "-Dtpm2=disabled".to_string(),
        "-Dqrencode=disabled".to_string(),
        "-Delfutils=disabled".to_string(),
        "-Dzlib=enabled".to_string(),
        "-Dbzip2=enabled".to_string(),
        "-Dxz=enabled".to_string(),
        "-Dlz4=enabled".to_string(),
        "-Dzstd=enabled".to_string(),
        "-Dxkbcommon=disabled".to_string(),
        "-Dpcre2=enabled".to_string(),
        "-Dbpf-framework=disabled".to_string(),
        "-Dvmlinux-h=disabled".to_string(),
        "-Dkernel-install=false".to_string(),
        "-Danalyze=false".to_string(),
        "-Dcreate-log-dirs=false".to_string(),
        "-Djournal-storage-default=volatile".to_string(),
    ]
}

fn build_dbus_broker(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/dbus/dbus-broker");
    if !source.join("meson.build").exists() {
        bail!(
            "dbus-broker source not found in {}; run upstream import dbus-broker first",
            source.display()
        );
    }

    let systemd_install = repo_root.join("out/build/systemd/install/usr");
    let systemd_lib = systemd_install.join("lib/x86_64-linux-gnu");
    let systemd_lib_pc = systemd_lib.join("pkgconfig");
    let systemd_share_pc = systemd_install.join("share/pkgconfig");
    if !systemd_lib.join("libsystemd.so").exists()
        || !systemd_lib_pc.join("libsystemd.pc").exists()
        || !systemd_share_pc.join("systemd.pc").exists()
    {
        bail!(
            "systemd development files missing at {}; run build systemd first",
            systemd_install.display()
        );
    }
    let expat_install = repo_root.join("out/build/expat/install/usr");
    let expat_lib = expat_install.join("lib/x86_64-linux-gnu");
    let expat_pc = expat_lib.join("pkgconfig");
    if !expat_lib.join("libexpat.so").exists() || !expat_pc.join("expat.pc").is_file() {
        bail!(
            "MattOS-built Expat development files missing at {}; run build expat first",
            expat_install.display()
        );
    }

    let out_root = repo_root.join("out/build/dbus-broker");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let options = vec![
        "--prefix=/usr".to_string(),
        "--bindir=bin".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        "--buildtype=release".to_string(),
        "--wrap-mode=forcefallback".to_string(),
        "-Dlauncher=true".to_string(),
        "-Dtests=false".to_string(),
        "-Ddocs=false".to_string(),
        "-Ddoctest=false".to_string(),
        "-Dreference-test=false".to_string(),
        "-Daudit=false".to_string(),
        "-Dapparmor=false".to_string(),
        "-Dselinux=false".to_string(),
        "-Dunstable=false".to_string(),
    ];
    let pkg_config_path = std::env::join_paths([&expat_pc, &systemd_lib_pc, &systemd_share_pc])
        .context("failed to construct dbus-broker PKG_CONFIG_PATH")?
        .to_string_lossy()
        .to_string();
    hydrate_development_sysroot(repo_root, &[expat_install.clone(), systemd_install.clone()])?;
    let env_overrides = vec![
        ("PKG_CONFIG_PATH", pkg_config_path.clone()),
        ("PKG_CONFIG_LIBDIR", pkg_config_path),
        (
            "PKG_CONFIG_SYSROOT_DIR",
            repo_root.join("out/sysroot").display().to_string(),
        ),
        (
            "CFLAGS",
            format!(
                "-I{} -I{}",
                expat_install.join("include").display(),
                systemd_install.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!("-L{} -L{}", expat_lib.display(), systemd_lib.display()),
        ),
        (
            "LIBRARY_PATH",
            std::env::join_paths([&expat_lib, &systemd_lib])?
                .to_string_lossy()
                .to_string(),
        ),
        (
            "LD_LIBRARY_PATH",
            std::env::join_paths([&expat_lib, &systemd_lib])?
                .to_string_lossy()
                .to_string(),
        ),
    ];
    let state = fs::read_to_string(repo_root.join("upstream/state/dbus-broker.toml"))
        .context("failed to read dbus-broker upstream state")?;
    let expat_state = fs::read_to_string(repo_root.join("upstream/state/expat.toml"))
        .context("failed to read Expat upstream state")?;
    let stamp = format!(
        "{state}\n{expat_state}\n{}\n{}\n",
        options.join("\n"),
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }

    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    if !build_dir.join("build.ninja").exists() {
        let mut setup_args = vec![
            "setup".to_string(),
            build_dir.display().to_string(),
            source_copy.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
    }

    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &["compile", "-C", path_str(&build_dir)?],
        &env_overrides,
    )?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            path_str(&build_dir)?,
            "--no-rebuild",
            "--destdir",
            path_str(&install_dir)?,
        ],
        &env_overrides,
    )?;

    for rel in [
        "usr/bin/dbus-broker",
        "usr/bin/dbus-broker-launch",
        "usr/lib/systemd/system/dbus-broker.service",
    ] {
        if !install_dir.join(rel).exists() {
            bail!("dbus-broker install did not produce {rel}");
        }
    }
    validate_dependency_resolves_from(
        &install_dir.join("usr/bin/dbus-broker-launch"),
        "libexpat.so.1",
        &expat_lib,
        &[&expat_lib, &systemd_lib],
    )?;
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_rootfs(repo_root: &Path) -> Result<()> {
    let skeleton = repo_root.join("src/rootfs/skeleton");
    let out = repo_root.join("out/build/rootfs");

    if out.exists() {
        fs::remove_dir_all(&out).with_context(|| format!("failed to clean {}", out.display()))?;
    }
    fs::create_dir_all(&out).with_context(|| format!("failed to create {}", out.display()))?;
    packaging::install_prototype_packages(repo_root, &out)?;
    let package_owned = packaging::package_owned_paths(&out)?;
    let package_snapshot = packaging::snapshot_package_files(&out, &package_owned)?;
    for rel in [
        "README.md",
        "etc/group",
        "etc/inittab",
        "etc/passwd",
        "usr/libexec/mattos/brush-login",
        "usr/libexec/mattos/validate-shell-env",
    ] {
        packaging::reject_legacy_collision(&package_owned, Path::new(rel))?;
        let source = skeleton.join(rel);
        let destination = out.join(rel);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "failed to install legacy skeleton file {}",
                source.display()
            )
        })?;
    }
    set_mode(out.join("usr/libexec/mattos/brush-login"), 0o755)?;
    set_mode(out.join("usr/libexec/mattos/validate-shell-env"), 0o755)?;
    fs::create_dir_all(out.join("root")).context("failed to create /root in rootfs")?;
    set_mode(out.join("root"), 0o700)?;
    fs::create_dir_all(out.join("home")).context("failed to create /home in rootfs")?;
    fs::create_dir_all(out.join("run")).context("failed to create /run in rootfs")?;
    fs::create_dir_all(out.join("var/log")).context("failed to create /var/log in rootfs")?;
    fs::create_dir_all(out.join("var/tmp")).context("failed to create /var/tmp in rootfs")?;
    fs::create_dir_all(out.join("etc/systemd/system"))
        .context("failed to create /etc/systemd/system")?;
    fs::create_dir_all(out.join("usr/libexec/mattos"))
        .context("failed to create rescue init dir")?;
    fs::write(out.join("etc/machine-id"), "").context("failed to create /etc/machine-id")?;

    let systemd_install = repo_root.join("out/build/systemd/install");
    let systemd_pid1 = systemd_install.join("usr/lib/systemd/systemd");
    if !systemd_pid1.exists() {
        bail!(
            "systemd install output missing at {}; run build systemd first",
            systemd_pid1.display()
        );
    }
    copy_tree_excluding_package_owned(&systemd_install, &out, &package_owned)?;
    copy_systemd_runtime_dependencies(&out)?;
    let pam_systemd = out.join(SYSTEMD_PAM_MODULE_REL);
    if !pam_systemd.is_file() {
        bail!(
            "systemd PAM module missing at {}; ensure the imported systemd build has PAM enabled",
            pam_systemd.display()
        );
    }
    copy_runtime_dependencies(&pam_systemd, &out)?;
    verify_required_pam_modules(&out)?;
    apply_live_profile(repo_root, &out)?;
    validate_account_database(&out)?;
    enforce_auth_file_modes(&out)?;
    validate_auth_file_modes(&out)?;
    install_mattos_system_units(repo_root, &out)?;
    install_network_configuration(repo_root, &out)?;

    let init_bin = repo_root.join("target/release/mattos-init");
    if !init_bin.exists() {
        bail!(
            "init binary missing at {}; run build init first",
            init_bin.display()
        );
    }

    let rescue_init = out.join("usr/libexec/mattos/rescue-init");
    fs::copy(&init_bin, &rescue_init).with_context(|| {
        format!(
            "failed to copy rescue init binary from {} into rootfs",
            init_bin.display()
        )
    })?;
    copy_runtime_dependencies(&rescue_init, &out)?;
    let mut inventory = UserlandInventory::default();
    inventory.add_implemented(UTIL_LINUX_PROVIDER, "agetty");
    inventory.add_implemented(UTIL_LINUX_PROVIDER, "login");
    inventory.add_implemented(UTIL_LINUX_PROVIDER, "su");
    inventory.add_compiled(UTIL_LINUX_PROVIDER, "agetty");
    inventory.add_compiled(UTIL_LINUX_PROVIDER, "login");
    inventory.add_compiled(UTIL_LINUX_PROVIDER, "su");
    inventory.add_installed(UTIL_LINUX_PROVIDER, "agetty");
    inventory.add_installed(UTIL_LINUX_PROVIDER, "login");
    inventory.add_installed(UTIL_LINUX_PROVIDER, "su");

    for module in [
        "libpam",
        "pam_unix",
        "pam_env",
        "pam_nologin",
        "pam_rootok",
        "pam_permit",
        "pam_deny",
        "pam_shells",
        "pam_securetty",
        "pam_systemd",
    ] {
        inventory.add_implemented(LINUX_PAM_PROVIDER, module);
        inventory.add_compiled(LINUX_PAM_PROVIDER, module);
        inventory.add_installed(LINUX_PAM_PROVIDER, module);
    }

    for cmd in [
        "passwd", "useradd", "usermod", "userdel", "groupadd", "groupmod", "groupdel", "chpasswd",
        "chage", "newgrp",
    ] {
        inventory.add_implemented(SHADOW_PROVIDER, cmd);
        inventory.add_compiled(SHADOW_PROVIDER, cmd);
        inventory.add_installed(SHADOW_PROVIDER, cmd);
    }
    inventory.add_implemented(SUDO_RS_PROVIDER, "sudo");
    inventory.add_compiled(SUDO_RS_PROVIDER, "sudo");
    inventory.add_installed(SUDO_RS_PROVIDER, "sudo");

    let brush_dst = out.join("usr/bin/brush");
    if !brush_dst.is_file() {
        bail!("mattos-brush package did not install /usr/bin/brush")
    }
    copy_runtime_dependencies(&brush_dst, &out)?;
    inventory.add_implemented("brush", "brush");
    inventory.add_compiled("brush", "brush");
    inventory.add_installed("brush", "brush");

    let coreutils_multicall = resolve_coreutils_multicall(repo_root)?;
    let coreutils_dst = out.join("usr/bin/coreutils");
    if !coreutils_dst.is_file() {
        bail!("mattos-coreutils package did not install /usr/bin/coreutils")
    }
    copy_runtime_dependencies(&coreutils_dst, &out)?;

    let coreutils_applets = list_coreutils_applets(&coreutils_multicall)?;
    for applet in &coreutils_applets {
        inventory.add_implemented(COREUTILS_PROVIDER, applet);
        inventory.add_compiled(COREUTILS_PROVIDER, applet);
    }
    let component_commands: BTreeSet<&str> = COMPONENT_INSTALL_MANIFESTS
        .iter()
        .flat_map(|manifest| manifest.binaries.iter().map(|binary| binary.command_name))
        .collect();
    let installed_coreutils_applets: Vec<String> = coreutils_applets
        .iter()
        .filter(|applet| !component_commands.contains(applet.as_str()))
        .cloned()
        .collect();
    for applet in &installed_coreutils_applets {
        if !path_entry_exists(&out.join("usr/bin").join(applet)) {
            bail!("mattos-coreutils package did not install alias /usr/bin/{applet}")
        }
        inventory.add_installed(COREUTILS_PROVIDER, applet);
    }
    for applet in coreutils_applets
        .iter()
        .filter(|applet| component_commands.contains(applet.as_str()))
    {
        inventory.add_excluded(COREUTILS_PROVIDER, applet);
    }

    for spec in USERLAND_BINARY_INSTALLS {
        install_userland_binary(repo_root, &out, spec)?;
        inventory.add_implemented(spec.provider, spec.command_name);
        inventory.add_compiled(spec.provider, spec.command_name);
        inventory.add_installed(spec.provider, spec.command_name);
    }

    create_command_aliases(&out, "diffutils", DIFFUTILS_AVAILABLE_ALIASES)?;
    for alias in DIFFUTILS_AVAILABLE_ALIASES {
        inventory.add_implemented(DIFFUTILS_PROVIDER, alias);
        inventory.add_installed(DIFFUTILS_PROVIDER, alias);
    }
    for expected in DIFFUTILS_EXPECTED_COMMANDS {
        if !DIFFUTILS_AVAILABLE_ALIASES.contains(expected) {
            inventory.add_failed(DIFFUTILS_PROVIDER, expected, "not implemented upstream");
        }
    }

    let component_provider_commands = install_component_manifests(repo_root, &out, &mut inventory)?;
    let curl_dst = out.join("usr/bin/curl");
    if !curl_dst.is_file() {
        bail!("mattos-curl package did not install /usr/bin/curl")
    }
    copy_runtime_dependencies(&curl_dst, &out)?;
    inventory.add_implemented(CURL_PROVIDER, "curl");
    inventory.add_compiled(CURL_PROVIDER, "curl");
    inventory.add_installed(CURL_PROVIDER, "curl");
    install_component_configuration(repo_root, &out)?;
    install_user_session_configuration(repo_root, &out)?;
    install_dbus_configuration(repo_root, &out)?;
    for command in [
        "busctl",
        "loginctl",
        "networkctl",
        "resolvectl",
        "timedatectl",
    ] {
        inventory.add_implemented(SYSTEMD_PROVIDER, command);
        inventory.add_compiled(SYSTEMD_PROVIDER, command);
        inventory.add_installed(SYSTEMD_PROVIDER, command);
    }

    let mut provider_commands = BTreeMap::<&str, Vec<String>>::new();
    provider_commands.insert(COREUTILS_PROVIDER, installed_coreutils_applets.clone());
    for spec in USERLAND_BINARY_INSTALLS {
        provider_commands
            .entry(spec.provider)
            .or_default()
            .push(spec.command_name.to_string());
    }
    provider_commands
        .entry(DIFFUTILS_PROVIDER)
        .or_default()
        .extend(DIFFUTILS_AVAILABLE_ALIASES.iter().map(|s| s.to_string()));
    for (provider, commands) in component_provider_commands {
        provider_commands.insert(provider, commands);
    }
    provider_commands.insert(CURL_PROVIDER, vec!["curl".to_string()]);
    provider_commands.insert(
        SYSTEMD_PROVIDER,
        vec![
            "busctl".to_string(),
            "loginctl".to_string(),
            "networkctl".to_string(),
            "resolvectl".to_string(),
            "timedatectl".to_string(),
        ],
    );
    validate_no_duplicate_commands(&provider_commands)?;

    for expected in [
        "grep",
        "sed",
        "find",
        "xargs",
        "diff",
        "cmp",
        "login",
        "su",
        "passwd",
        "sudo",
        "useradd",
        "usermod",
        "userdel",
        "groupadd",
        "groupmod",
        "groupdel",
        "chpasswd",
        "getent",
        "modprobe",
        "lsmod",
        "ps",
        "top",
        "free",
        "uptime",
        "pgrep",
        "pkill",
        "clear",
        "tput",
        "infocmp",
        "ip",
        "ss",
        "bridge",
        "tc",
        "ping",
        "tracepath",
        "curl",
        "sh",
        "bash",
        "dbus-broker",
        "dbus-broker-launch",
        "busctl",
        "loginctl",
        "networkctl",
        "resolvectl",
        "timedatectl",
    ] {
        let path = out.join("usr/bin").join(expected);
        let alt = out.join("usr/sbin").join(expected);
        if !path_entry_exists(&path) && !path_entry_exists(&alt) {
            bail!(
                "required command {} missing from rootfs at {}",
                expected,
                path.display()
            )
        }
    }

    inventory.add_installed("brush", "sh");
    inventory.add_installed("brush", "bash");
    inventory.add_excluded(DIFFUTILS_PROVIDER, "diff3");
    inventory.add_excluded(DIFFUTILS_PROVIDER, "sdiff");
    write_userland_inventory(&out, &inventory)?;
    packaging::embed_repository(repo_root, &out)?;
    packaging::validate_dpkg_database(&out)?;
    packaging::validate_package_snapshot(&out, &package_snapshot)?;
    validate_glibc_rootfs(repo_root, &out)?;

    Ok(())
}

fn validate_glibc_rootfs(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let expected_loader = "/lib64/ld-linux-x86-64.so.2";
    let loader = rootfs.join("usr/lib64/ld-linux-x86-64.so.2");
    let libc = rootfs.join("usr/lib/x86_64-linux-gnu/libc.so.6");
    let libm = rootfs.join("usr/lib/x86_64-linux-gnu/libm.so.6");
    for (installed, built) in [
        (
            &loader,
            repo_root.join("out/build/glibc/install/lib64/ld-linux-x86-64.so.2"),
        ),
        (
            &libc,
            repo_root.join("out/build/glibc/install/usr/lib/x86_64-linux-gnu/libc.so.6"),
        ),
        (
            &libm,
            repo_root.join("out/build/glibc/install/usr/lib/x86_64-linux-gnu/libm.so.6"),
        ),
    ] {
        if !installed.is_file() || fs::read(installed)? != fs::read(&built)? {
            bail!(
                "rootfs glibc artifact {} does not exactly match MattOS build output {}",
                installed.display(),
                built.display()
            )
        }
    }
    for (installed, built) in [
        (
            rootfs.join("usr/lib/x86_64-linux-gnu/libgcc_s.so.1"),
            repo_root.join("out/build/gcc-runtime/runtime/usr/lib/x86_64-linux-gnu/libgcc_s.so.1"),
        ),
        (
            rootfs.join("usr/lib/x86_64-linux-gnu/libstdc++.so.6"),
            repo_root.join("out/build/gcc-runtime/runtime/usr/lib/x86_64-linux-gnu/libstdc++.so.6"),
        ),
    ] {
        if !installed.is_file() || !built.is_file() || fs::read(&installed)? != fs::read(&built)? {
            bail!(
                "rootfs GCC runtime {} does not exactly match MattOS build output {}",
                installed.display(),
                built.display()
            )
        }
    }

    let mut files = Vec::new();
    collect_regular_files(rootfs, &mut files)?;
    let mut elf_files = Vec::new();
    let mut provided = BTreeSet::new();
    let mut soname_providers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in files {
        let header = Command::new("readelf").args(["-h"]).arg(&path).output()?;
        if !header.status.success() {
            continue;
        }
        let bytes = fs::read(&path)?;
        let build_root = repo_root.to_string_lossy();
        if bytes
            .windows(build_root.len())
            .any(|window| window == build_root.as_bytes())
        {
            bail!(
                "ELF object /{} embeds the host build root {}",
                path.strip_prefix(rootfs)?.display(),
                repo_root.display()
            )
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            provided.insert(name.to_string());
        }
        let dynamic = Command::new("readelf").args(["-d"]).arg(&path).output()?;
        if dynamic.status.success() {
            let text = String::from_utf8_lossy(&dynamic.stdout);
            for line in text.lines().filter(|line| line.contains("(SONAME)")) {
                if let Some(value) = line
                    .split('[')
                    .nth(1)
                    .and_then(|part| part.split(']').next())
                {
                    provided.insert(value.to_string());
                    soname_providers
                        .entry(value.to_string())
                        .or_default()
                        .push(format!("/{}", path.strip_prefix(rootfs)?.display()));
                }
            }
        }
        elf_files.push(path);
    }
    provided.insert("linux-vdso.so.1".to_string());
    provided.insert("ld-linux-x86-64.so.2".to_string());
    for (soname, paths) in &soname_providers {
        if paths.len() > 1 {
            bail!("duplicate SONAME provider {soname}: {}", paths.join(", "))
        }
    }

    let mut package_owners = BTreeMap::new();
    let info = rootfs.join("var/lib/dpkg/info");
    if info.is_dir() {
        for entry in fs::read_dir(&info)? {
            let path = entry?.path();
            if path.extension().and_then(|part| part.to_str()) != Some("list") {
                continue;
            }
            let package = path
                .file_stem()
                .and_then(|part| part.to_str())
                .unwrap_or("unknown")
                .to_string();
            for installed in fs::read_to_string(&path)?.lines() {
                package_owners.insert(installed.to_string(), package.clone());
            }
        }
    }

    let library_path = std::env::join_paths([
        rootfs.join("usr/lib/x86_64-linux-gnu"),
        rootfs.join("usr/lib/x86_64-linux-gnu/systemd"),
        rootfs.join("usr/lib"),
    ])?;
    let mut rows = Vec::new();
    let mut gcc_runtime_consumers = Vec::new();
    let mut executable_count = 0usize;
    for path in &elf_files {
        let relative = format!("/{}", path.strip_prefix(rootfs)?.display());
        let program_headers = Command::new("readelf").args(["-l"]).arg(path).output()?;
        let program_text = String::from_utf8_lossy(&program_headers.stdout);
        let interpreter = program_text.lines().find_map(|line| {
            line.split_once("Requesting program interpreter:")
                .map(|(_, value)| value.trim().trim_end_matches(']').to_string())
        });
        if let Some(actual) = &interpreter {
            executable_count += 1;
            if actual != expected_loader {
                bail!("ELF executable {relative} uses unexpected interpreter {actual}")
            }
            let listed = Command::new(&loader)
                .arg("--library-path")
                .arg(&library_path)
                .arg("--list")
                .arg(path)
                .output()
                .with_context(|| format!("failed to invoke MattOS loader for {relative}"))?;
            if !listed.status.success() {
                bail!(
                    "MattOS loader cannot resolve {relative}: {}",
                    String::from_utf8_lossy(&listed.stderr)
                )
            }
            let listing = String::from_utf8_lossy(&listed.stdout);
            if listing.contains("not found") {
                bail!("MattOS loader reports an unresolved library for {relative}: {listing}")
            }
            for line in listing.lines().filter(|line| line.contains("=>")) {
                let resolved = line
                    .split("=>")
                    .nth(1)
                    .and_then(|part| part.split_whitespace().next())
                    .unwrap_or_default();
                if resolved.starts_with('/') && !Path::new(resolved).starts_with(rootfs) {
                    bail!("{relative} resolves a runtime library from host path {resolved}")
                }
            }
        }

        let dynamic = Command::new("readelf").args(["-d"]).arg(path).output()?;
        let dynamic_text = String::from_utf8_lossy(&dynamic.stdout);
        let mut runtime_needs = Vec::new();
        for line in dynamic_text
            .lines()
            .filter(|line| line.contains("(NEEDED)"))
        {
            let needed = line
                .split('[')
                .nth(1)
                .and_then(|part| part.split(']').next())
                .unwrap_or_default();
            if !provided.contains(needed) {
                bail!(
                    "ELF object {relative} needs {needed}, which is absent from the MattOS rootfs"
                )
            }
            if needed == "libgcc_s.so.1" || needed == "libstdc++.so.6" {
                runtime_needs.push(needed.to_string());
            }
        }
        for line in dynamic_text
            .lines()
            .filter(|line| line.contains("(RPATH)") || line.contains("(RUNPATH)"))
        {
            if line.contains("/home/")
                || line.contains("/tmp/")
                || line.contains("/usr/local/")
                || line.contains("/opt/")
            {
                bail!(
                    "ELF object {relative} embeds a host-style absolute library search path: {line}"
                )
            }
        }

        let glibc_versions = elf_version_names(path, &["GLIBC_"])?;
        let glibcxx_versions = elf_version_names(path, &["GLIBCXX_"])?;
        let cxxabi_versions = elf_version_names(path, &["CXXABI_"])?;
        let gcc_versions = elf_version_names(path, &["GCC_"])?;
        let owner = package_owners
            .get(&relative)
            .cloned()
            .unwrap_or_else(|| "legacy-source-stage".to_string());
        let build_stage = if relative == "/usr/libexec/mattos/rescue-init" {
            "init"
        } else if relative.starts_with("/usr/lib/systemd/")
            || relative.contains("libnss_systemd")
            || relative.contains("libnss_resolve")
        {
            "systemd"
        } else {
            owner.trim_start_matches("mattos-")
        };
        let glibc_versions = glibc_versions.into_iter().collect::<Vec<_>>().join(",");
        let glibcxx_versions = glibcxx_versions.into_iter().collect::<Vec<_>>().join(",");
        let cxxabi_versions = cxxabi_versions.into_iter().collect::<Vec<_>>().join(",");
        let gcc_versions = gcc_versions.into_iter().collect::<Vec<_>>().join(",");
        for needed in runtime_needs {
            gcc_runtime_consumers.push(format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                relative, owner, needed, glibcxx_versions, cxxabi_versions, gcc_versions
            ));
        }
        rows.push(format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\tvalidated",
            relative,
            owner,
            build_stage,
            interpreter.as_deref().unwrap_or("-"),
            glibc_versions,
            glibcxx_versions,
            cxxabi_versions,
            gcc_versions
        ));
    }
    rows.sort();
    let reports = repo_root.join("out/reports");
    fs::create_dir_all(&reports)?;
    fs::write(
        reports.join("elf-runtime-inventory.tsv"),
        format!(
            "path\towner\tbuild_stage\tinterpreter\tglibc_versions\tglibcxx_versions\tcxxabi_versions\tgcc_versions\trebuild_status\n{}\n",
            rows.join("\n")
        ),
    )?;
    gcc_runtime_consumers.sort();
    fs::write(
        reports.join("gcc-runtime-consumers.tsv"),
        format!(
            "path\towner\tneeded_runtime\tglibcxx_versions\tcxxabi_versions\tgcc_versions\n{}\n",
            gcc_runtime_consumers.join("\n")
        ),
    )?;
    println!(
        "validated {} ELF objects ({} dynamic executables) with MattOS glibc",
        elf_files.len(),
        executable_count
    );
    Ok(())
}

fn collect_regular_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            collect_regular_files(&path, files)?;
        } else if metadata.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn install_component_manifests(
    repo_root: &Path,
    rootfs: &Path,
    inventory: &mut UserlandInventory,
) -> Result<BTreeMap<&'static str, Vec<String>>> {
    let mut providers = BTreeMap::new();

    for manifest in COMPONENT_INSTALL_MANIFESTS {
        if manifest.provider == CURL_PROVIDER {
            continue;
        }
        let install_root = repo_root.join(manifest.install_root_rel);
        let mut commands = Vec::new();
        for binary in manifest.binaries {
            let source = install_root.join(binary.source_rel);
            let destination = rootfs.join(binary.destination_rel);
            if !source.is_file() {
                bail!("component executable missing at {}", source.display());
            }
            if !destination.is_file() {
                bail!(
                    "package did not install required component executable /{}",
                    binary.destination_rel
                );
            }
            inventory.add_implemented(manifest.provider, binary.command_name);
            inventory.add_compiled(manifest.provider, binary.command_name);
            inventory.add_installed(manifest.provider, binary.command_name);
            commands.push(binary.command_name.to_string());
        }
        providers.insert(manifest.provider, commands);
    }

    Ok(providers)
}

#[cfg(test)]
fn inspect_and_stage_executable(
    source: &Path,
    destination: &Path,
    rootfs: &Path,
    install_roots: &[PathBuf],
    library_dirs: &[PathBuf],
) -> Result<()> {
    if !source.exists() {
        bail!("component executable missing at {}", source.display());
    }
    let file_output = Command::new("file")
        .arg("-L")
        .arg(source)
        .output()
        .with_context(|| format!("failed to inspect {} with file", source.display()))?;
    if !file_output.status.success() {
        bail!("file inspection failed for {}", source.display());
    }
    let file_text = String::from_utf8_lossy(&file_output.stdout);
    if !file_text.contains("ELF") {
        bail!(
            "expected an ELF executable, file reported: {}",
            file_text.trim()
        );
    }
    let readelf = Command::new("readelf")
        .args(["-d"])
        .arg(source)
        .output()
        .with_context(|| format!("failed to inspect {} with readelf", source.display()))?;
    if !readelf.status.success() {
        bail!("readelf inspection failed for {}", source.display());
    }

    let library_path = std::env::join_paths(library_dirs)
        .context("failed to construct component LD_LIBRARY_PATH")?;
    let ldd = Command::new("ldd")
        .arg(source)
        .env("LD_LIBRARY_PATH", library_path)
        .output()
        .with_context(|| format!("failed to inspect {} with ldd", source.display()))?;
    if !ldd.status.success() {
        bail!("ldd inspection failed for {}", source.display());
    }
    let ldd_text = String::from_utf8(ldd.stdout).context("ldd output was not UTF-8")?;
    if ldd_text.contains("not found") {
        bail!(
            "unresolved runtime dependency for {}:\n{}",
            source.display(),
            ldd_text
        );
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(source, destination)
        .with_context(|| format!("failed to stage {}", source.display()))?;
    for token in ldd_text
        .split_whitespace()
        .filter(|token| token.starts_with('/'))
    {
        let dependency = Path::new(token);
        if dependency.exists() {
            stage_resolved_dependency(dependency, rootfs, install_roots)?;
        }
    }
    println!("inspected and staged {}", destination.display());
    Ok(())
}

#[cfg(test)]
fn stage_resolved_dependency(
    source: &Path,
    rootfs: &Path,
    install_roots: &[PathBuf],
) -> Result<()> {
    let relative = install_roots
        .iter()
        .find_map(|root| source.strip_prefix(root).ok().map(Path::to_path_buf))
        .or_else(|| source.strip_prefix("/").ok().map(Path::to_path_buf))
        .ok_or_else(|| {
            anyhow!(
                "cannot map runtime dependency {} into rootfs",
                source.display()
            )
        })?;
    let destination = rootfs.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(source, &destination)
        .with_context(|| format!("failed to stage runtime dependency {}", source.display()))?;
    Ok(())
}

fn install_component_configuration(repo_root: &Path, rootfs: &Path) -> Result<()> {
    for directory in [
        "etc/depmod.d",
        "etc/modprobe.d",
        "etc/modules-load.d",
        "usr/lib/depmod.d",
        "usr/lib/modprobe.d",
        "usr/lib/modules-load.d",
        "etc/sysctl.d",
    ] {
        fs::create_dir_all(rootfs.join(directory))
            .with_context(|| format!("failed to create /{directory}"))?;
    }
    let sysctl_source = repo_root.join("src/userland/procps-ng/sysctl.conf");
    if fs::read(&sysctl_source)? != fs::read(rootfs.join("etc/sysctl.conf"))? {
        bail!("mattos-procps did not install the authoritative /etc/sysctl.conf");
    }

    let source_db = repo_root.join("out/build/ncurses/install/usr/share/terminfo");
    verify_terminfo_entries(&source_db)?;
    verify_terminfo_entries(&rootfs.join("usr/share/terminfo"))?;
    Ok(())
}

fn verify_terminfo_entries(database: &Path) -> Result<()> {
    for terminal in TERMINFO_ENTRIES {
        if terminfo_entry_path(database, terminal).is_none() {
            bail!(
                "terminfo database {} lacks required entry {terminal}",
                database.display()
            );
        }
    }
    Ok(())
}

fn terminfo_entry_path(database: &Path, terminal: &str) -> Option<PathBuf> {
    let first = terminal.as_bytes().first().copied()?;
    let candidates = [
        database.join(char::from(first).to_string()).join(terminal),
        database.join(format!("{first:x}")).join(terminal),
    ];
    candidates.into_iter().find(|path| path.exists())
}

fn install_mattos_system_units(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let units_src = repo_root.join("src/system/units");
    if !units_src.exists() {
        bail!(
            "MattOS systemd units missing at {}; expected MattOS-owned units",
            units_src.display()
        );
    }
    let units_dst = rootfs.join("usr/lib/systemd/system");
    fs::create_dir_all(&units_dst)
        .with_context(|| format!("failed to create {}", units_dst.display()))?;
    copy_tree_excluding_dotgit(&units_src, &units_dst)?;

    let default_target = rootfs.join("etc/systemd/system/default.target");
    if default_target.exists() {
        fs::remove_file(&default_target)
            .with_context(|| format!("failed to remove {}", default_target.display()))?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("/usr/lib/systemd/system/mattos.target", &default_target)
        .with_context(|| format!("failed to create {}", default_target.display()))?;

    let getty_wants = rootfs.join("etc/systemd/system/getty.target.wants");
    fs::create_dir_all(&getty_wants)
        .with_context(|| format!("failed to create {}", getty_wants.display()))?;
    let tty1_getty = getty_wants.join("getty@tty1.service");
    if tty1_getty.exists() {
        fs::remove_file(&tty1_getty)
            .with_context(|| format!("failed to remove {}", tty1_getty.display()))?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("/usr/lib/systemd/system/getty@.service", &tty1_getty)
        .with_context(|| format!("failed to create {}", tty1_getty.display()))?;

    for masked in ["ldconfig.service", "mattos-shell.service"] {
        let mask = rootfs.join("etc/systemd/system").join(masked);
        if mask.exists() {
            fs::remove_file(&mask)
                .with_context(|| format!("failed to remove {}", mask.display()))?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink("/dev/null", &mask)
            .with_context(|| format!("failed to create {}", mask.display()))?;
    }

    Ok(())
}

fn install_network_configuration(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let source = repo_root.join("src/system/network");
    if !source.join("network/20-mattos-wired.network").exists() {
        bail!(
            "MattOS network configuration missing at {}",
            source.display()
        );
    }
    copy_tree_excluding_dotgit(&source.join("network"), &rootfs.join("etc/systemd/network"))?;
    for (source_name, destination) in [
        ("resolved.conf", "etc/systemd/resolved.conf"),
        ("timesyncd.conf", "etc/systemd/timesyncd.conf"),
        ("nsswitch.conf", "etc/nsswitch.conf"),
        ("hosts", "etc/hosts"),
        ("networks", "etc/networks"),
        (
            "99-mattos-network.conf",
            "etc/sysctl.d/99-mattos-network.conf",
        ),
    ] {
        let target = rootfs.join(destination);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::copy(source.join(source_name), &target)
            .with_context(|| format!("failed to install network configuration {source_name}"))?;
    }

    fs::create_dir_all(rootfs.join("run/systemd/resolve"))
        .context("failed to create /run/systemd/resolve")?;
    let resolv_conf = rootfs.join("etc/resolv.conf");
    if path_entry_exists(&resolv_conf) {
        remove_path_if_exists(&resolv_conf)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("/run/systemd/resolve/stub-resolv.conf", &resolv_conf)
        .context("failed to create resolved-managed /etc/resolv.conf")?;

    let wants = rootfs.join("etc/systemd/system/multi-user.target.wants");
    fs::create_dir_all(&wants).with_context(|| format!("failed to create {}", wants.display()))?;
    for service in [
        "systemd-networkd.service",
        "systemd-resolved.service",
        "systemd-timesyncd.service",
    ] {
        let unit = rootfs.join("usr/lib/systemd/system").join(service);
        if !unit.exists() {
            bail!("required networking unit missing at {}", unit.display());
        }
        let link = wants.join(service);
        if path_entry_exists(&link) {
            remove_path_if_exists(&link)?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(format!("/usr/lib/systemd/system/{service}"), &link)
            .with_context(|| format!("failed to enable {service}"))?;
    }

    validate_network_configuration(rootfs)
}

fn install_user_session_configuration(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let source = repo_root.join("src/system/session");
    let units_source = source.join("user-units");
    let dbus_config = source.join("dbus/session.conf");
    for required in [
        units_source.join("dbus.socket"),
        units_source.join("dbus-broker.service"),
        dbus_config.clone(),
    ] {
        if !required.is_file() {
            bail!("MattOS user-session unit missing at {}", required.display());
        }
    }

    let user_units = rootfs.join("usr/lib/systemd/user");
    fs::create_dir_all(&user_units)
        .with_context(|| format!("failed to create {}", user_units.display()))?;
    for rel in ["dbus.socket", "dbus-broker.service"] {
        if fs::read(units_source.join(rel))? != fs::read(user_units.join(rel))? {
            bail!("mattos-dbus-broker did not install authoritative user unit {rel}");
        }
    }
    for rel in ["dbus.socket", "dbus-broker.service"] {
        set_mode(user_units.join(rel), 0o644)?;
    }

    let dbus_alias = user_units.join("dbus.service");
    if path_entry_exists(&dbus_alias) {
        remove_path_if_exists(&dbus_alias)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("dbus-broker.service", &dbus_alias)
        .context("failed to create user dbus.service alias")?;

    let sockets_wants = user_units.join("sockets.target.wants");
    fs::create_dir_all(&sockets_wants)
        .with_context(|| format!("failed to create {}", sockets_wants.display()))?;
    let socket_link = sockets_wants.join("dbus.socket");
    if path_entry_exists(&socket_link) {
        remove_path_if_exists(&socket_link)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("../dbus.socket", &socket_link)
        .context("failed to enable the per-user D-Bus socket")?;

    for directory in [
        "usr/share/dbus-1/session.d",
        "usr/share/dbus-1/services",
        "etc/dbus-1/session.d",
    ] {
        fs::create_dir_all(rootfs.join(directory))
            .with_context(|| format!("failed to create /{directory}"))?;
    }
    if fs::read(&dbus_config)? != fs::read(rootfs.join("usr/share/dbus-1/session.conf"))? {
        bail!("mattos-dbus-broker did not install authoritative session bus policy");
    }
    set_mode(rootfs.join("usr/share/dbus-1/session.conf"), 0o644)?;

    // MattOS supplies a deliberately small effective systemd-user PAM stack in /etc.
    // Remove the imported vendor fallback, which references optional PAM modules that
    // are outside the current image's authentication scope.
    let vendor_systemd_user = rootfs.join("usr/lib/pam.d/systemd-user");
    if path_entry_exists(&vendor_systemd_user) {
        remove_path_if_exists(&vendor_systemd_user)?;
    }

    validate_user_session_configuration(rootfs)
}

fn validate_user_session_configuration(rootfs: &Path) -> Result<()> {
    for rel in [
        SYSTEMD_PAM_MODULE_REL,
        "etc/pam.d/login",
        "etc/pam.d/su-l",
        "etc/pam.d/systemd-user",
        "usr/lib/systemd/system/systemd-logind.service",
        "usr/lib/systemd/system/user@.service",
        "usr/lib/systemd/system/user-runtime-dir@.service",
        "usr/lib/systemd/systemd-user-runtime-dir",
        "usr/lib/systemd/user/basic.target",
        "usr/lib/systemd/user/default.target",
        "usr/lib/systemd/user/sockets.target",
        "usr/lib/systemd/user/dbus.socket",
        "usr/lib/systemd/user/dbus-broker.service",
        "usr/lib/systemd/user/dbus.service",
        "usr/lib/systemd/user/sockets.target.wants/dbus.socket",
        "usr/share/dbus-1/session.conf",
        "usr/share/dbus-1/session.d",
        "usr/share/dbus-1/services",
        "etc/dbus-1/session.d",
        "usr/lib/systemd/user-environment-generators/30-systemd-environment-d-generator",
    ] {
        if !path_entry_exists(&rootfs.join(rel)) {
            bail!("required user-session rootfs path missing: /{rel}");
        }
    }

    let expected_hook = "session    optional     pam_systemd.so";
    for stack in ["login", "su-l", "systemd-user"] {
        let body = fs::read_to_string(rootfs.join("etc/pam.d").join(stack))
            .with_context(|| format!("failed to read effective PAM stack {stack}"))?;
        if body.matches(expected_hook).count() != 1 {
            bail!("PAM stack {stack} must contain exactly one optional pam_systemd session hook");
        }
    }
    for entry in fs::read_dir(rootfs.join("etc/pam.d"))? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
        if matches!(name, "login" | "su-l" | "systemd-user") {
            continue;
        }
        if fs::read_to_string(&path)?.contains("pam_systemd.so") {
            bail!("pam_systemd is present in inappropriate PAM stack {name}");
        }
    }

    let user_socket = fs::read_to_string(rootfs.join("usr/lib/systemd/user/dbus.socket"))?;
    if user_socket.matches("ListenStream=%t/bus").count() != 1
        || user_socket.contains("/run/dbus/system_bus_socket")
    {
        bail!("user dbus.socket must own only the per-user %t/bus endpoint");
    }
    let user_broker = fs::read_to_string(rootfs.join("usr/lib/systemd/user/dbus-broker.service"))?;
    if user_broker
        .matches("ExecStart=/usr/bin/dbus-broker-launch --scope user")
        .count()
        != 1
        || user_broker.contains("--scope system")
    {
        bail!("user dbus-broker.service must launch exactly one user-scope broker");
    }
    let session_policy = fs::read_to_string(rootfs.join("usr/share/dbus-1/session.conf"))?;
    for required in [
        "<type>session</type>",
        "<auth>EXTERNAL</auth>",
        "<standard_session_servicedirs/>",
        "<allow own=\"*\"/>",
    ] {
        if !session_policy.contains(required) {
            bail!("per-user D-Bus policy is missing required contract: {required}");
        }
    }
    if session_policy.contains("<type>system</type>")
        || session_policy.contains("<user>messagebus</user>")
        || session_policy.contains("/run/dbus/system_bus_socket")
    {
        bail!("per-user D-Bus policy must remain separate from the system bus");
    }

    for rel in [
        "etc/pam.d/login",
        "etc/pam.d/su-l",
        "etc/pam.d/systemd-user",
        "usr/lib/systemd/user/dbus.socket",
        "usr/lib/systemd/user/dbus-broker.service",
        "usr/share/dbus-1/session.conf",
    ] {
        let body = fs::read_to_string(rootfs.join(rel))?;
        if body.contains("/run/user/1000") || body.contains("user@1000") {
            bail!("generic user-session configuration hardcodes the live UID in /{rel}");
        }
    }
    if path_entry_exists(&rootfs.join("run/user")) {
        bail!("stale /run/user content must not be baked into the staged rootfs");
    }

    validate_executable_runtime_closure(&rootfs.join(SYSTEMD_PAM_MODULE_REL), rootfs)?;
    validate_executable_runtime_closure(
        &rootfs.join("usr/lib/systemd/systemd-user-runtime-dir"),
        rootfs,
    )?;
    Ok(())
}

fn install_dbus_configuration(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let source = repo_root.join("src/system/dbus");
    let config_source = source.join("config/system.conf");
    let sysusers_source = source.join("config/dbus.conf");
    let units_source = source.join("units");
    for required in [
        &config_source,
        &sysusers_source,
        &units_source.join("dbus.socket"),
        &units_source.join("dbus-broker.service"),
    ] {
        if !required.exists() {
            bail!(
                "MattOS D-Bus integration file missing at {}",
                required.display()
            );
        }
    }

    for directory in [
        "etc/dbus-1/system.d",
        "usr/share/dbus-1/system-services",
        "usr/share/dbus-1/system.d",
        "usr/lib/sysusers.d",
        "usr/lib/systemd/system",
    ] {
        fs::create_dir_all(rootfs.join(directory))
            .with_context(|| format!("failed to create /{directory}"))?;
    }
    for (source, destination) in [
        (&config_source, rootfs.join("etc/dbus-1/system.conf")),
        (
            &sysusers_source,
            rootfs.join("usr/lib/sysusers.d/dbus.conf"),
        ),
        (
            &units_source.join("dbus.socket"),
            rootfs.join("usr/lib/systemd/system/dbus.socket"),
        ),
        (
            &units_source.join("dbus-broker.service"),
            rootfs.join("usr/lib/systemd/system/dbus-broker.service"),
        ),
    ] {
        if fs::read(source)? != fs::read(&destination)? {
            bail!(
                "mattos-dbus-broker did not install authoritative /{}",
                destination.strip_prefix(rootfs)?.display()
            );
        }
    }
    for rel in [
        "etc/dbus-1/system.conf",
        "usr/lib/sysusers.d/dbus.conf",
        "usr/lib/systemd/system/dbus.socket",
        "usr/lib/systemd/system/dbus-broker.service",
    ] {
        set_mode(rootfs.join(rel), 0o644)?;
    }

    let aliases = [
        ("dbus.service", "dbus-broker.service"),
        (
            "dbus-org.freedesktop.network1.service",
            "systemd-networkd.service",
        ),
        (
            "dbus-org.freedesktop.resolve1.service",
            "systemd-resolved.service",
        ),
        (
            "dbus-org.freedesktop.timesync1.service",
            "systemd-timesyncd.service",
        ),
        (
            "dbus-org.freedesktop.timedate1.service",
            "systemd-timedated.service",
        ),
        (
            "dbus-org.freedesktop.login1.service",
            "systemd-logind.service",
        ),
    ];
    for (alias, target) in aliases {
        install_systemd_service_alias(rootfs, alias, target)?;
    }

    let sockets_wants = rootfs.join("etc/systemd/system/sockets.target.wants");
    fs::create_dir_all(&sockets_wants)
        .with_context(|| format!("failed to create {}", sockets_wants.display()))?;
    let socket_link = sockets_wants.join("dbus.socket");
    if path_entry_exists(&socket_link) {
        remove_path_if_exists(&socket_link)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("/usr/lib/systemd/system/dbus.socket", &socket_link)
        .context("failed to enable dbus.socket")?;

    validate_dbus_configuration(rootfs)
}

#[cfg(unix)]
fn install_systemd_service_alias(rootfs: &Path, alias: &str, target: &str) -> Result<()> {
    use std::os::unix::fs::symlink;

    let unit_dir = rootfs.join("usr/lib/systemd/system");
    let target_path = unit_dir.join(target);
    if !target_path.is_file() {
        bail!("refusing D-Bus alias {alias}: target unit {target} is missing");
    }
    let alias_path = unit_dir.join(alias);
    if path_entry_exists(&alias_path) {
        remove_path_if_exists(&alias_path)?;
    }
    symlink(target, &alias_path)
        .with_context(|| format!("failed to create D-Bus service alias {alias} -> {target}"))?;
    Ok(())
}

#[cfg(not(unix))]
fn install_systemd_service_alias(_rootfs: &Path, _alias: &str, _target: &str) -> Result<()> {
    bail!("systemd service alias installation requires a Unix host")
}

fn validate_dbus_configuration(rootfs: &Path) -> Result<()> {
    for rel in [
        "usr/bin/dbus-broker",
        "usr/bin/dbus-broker-launch",
        "usr/bin/busctl",
        "etc/dbus-1/system.conf",
        "etc/dbus-1/system.d",
        "usr/share/dbus-1/system-services",
        "usr/share/dbus-1/system.d",
        "usr/lib/sysusers.d/dbus.conf",
        "usr/lib/systemd/system/dbus.socket",
        "usr/lib/systemd/system/dbus-broker.service",
        "etc/systemd/system/sockets.target.wants/dbus.socket",
    ] {
        if !path_entry_exists(&rootfs.join(rel)) {
            bail!("required D-Bus rootfs path missing: /{rel}");
        }
    }

    let system_conf = fs::read_to_string(rootfs.join("etc/dbus-1/system.conf"))
        .context("failed to read installed system.conf")?;
    for required in [
        "<user>messagebus</user>",
        "<deny own=\"*\"/>",
        "<deny send_type=\"method_call\"/>",
        "<includedir>/usr/share/dbus-1/system.d</includedir>",
        "<includedir>/etc/dbus-1/system.d</includedir>",
    ] {
        if !system_conf.contains(required) {
            bail!("system-bus policy is missing required boundary: {required}");
        }
    }
    if system_conf.contains("<allow own=\"*\"/>") {
        bail!("system-bus policy must not allow arbitrary name ownership");
    }

    let socket_unit = fs::read_to_string(rootfs.join("usr/lib/systemd/system/dbus.socket"))
        .context("failed to read dbus.socket")?;
    if socket_unit
        .matches("ListenStream=/run/dbus/system_bus_socket")
        .count()
        != 1
    {
        bail!("dbus.socket must own exactly one conventional system-bus socket");
    }
    if path_entry_exists(&rootfs.join("run/dbus/system_bus_socket")) {
        bail!("stale system-bus socket must not be present in rootfs staging");
    }
    if rootfs.join("usr/bin/dbus-daemon").exists() || rootfs.join("usr/sbin/dbus-daemon").exists() {
        bail!("competing dbus-daemon binary found in rootfs");
    }
    for binary in ["usr/bin/dbus-broker", "usr/bin/dbus-broker-launch"] {
        validate_executable_runtime_closure(&rootfs.join(binary), rootfs)?;
    }

    let broker_unit = fs::read_to_string(rootfs.join("usr/lib/systemd/system/dbus-broker.service"))
        .context("failed to read dbus-broker.service")?;
    if broker_unit
        .matches("ExecStart=/usr/bin/dbus-broker-launch")
        .count()
        != 1
        || broker_unit.contains("dbus-daemon")
    {
        bail!("dbus-broker.service must launch exactly one system-bus implementation");
    }

    let sysusers = fs::read_to_string(rootfs.join("usr/lib/sysusers.d/dbus.conf"))
        .context("failed to read dbus sysusers definition")?;
    let fields: Vec<&str> = sysusers.split_whitespace().collect();
    if fields.get(0) != Some(&"u!")
        || fields.get(1) != Some(&"messagebus")
        || fields.get(2) != Some(&"195")
    {
        bail!("messagebus sysusers definition must pin UID/GID 195");
    }
    for entry in fs::read_dir(rootfs.join("usr/lib/sysusers.d"))? {
        let path = entry?.path();
        if path.ends_with("dbus.conf") || !path.is_file() {
            continue;
        }
        let body = fs::read_to_string(&path).unwrap_or_default();
        for line in body.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.get(2) == Some(&"195") {
                bail!("messagebus UID/GID 195 collides with {}", path.display());
            }
        }
    }

    for name in [
        "systemd1",
        "network1",
        "resolve1",
        "timesync1",
        "timedate1",
        "login1",
    ] {
        let policy = rootfs.join(format!(
            "usr/share/dbus-1/system.d/org.freedesktop.{name}.conf"
        ));
        let service = rootfs.join(format!(
            "usr/share/dbus-1/system-services/org.freedesktop.{name}.service"
        ));
        if !policy.is_file() || !service.is_file() {
            bail!("D-Bus policy/service descriptor missing for org.freedesktop.{name}");
        }
    }

    for (alias, target) in [
        ("dbus.service", "dbus-broker.service"),
        (
            "dbus-org.freedesktop.network1.service",
            "systemd-networkd.service",
        ),
        (
            "dbus-org.freedesktop.resolve1.service",
            "systemd-resolved.service",
        ),
        (
            "dbus-org.freedesktop.timesync1.service",
            "systemd-timesyncd.service",
        ),
        (
            "dbus-org.freedesktop.timedate1.service",
            "systemd-timedated.service",
        ),
        (
            "dbus-org.freedesktop.login1.service",
            "systemd-logind.service",
        ),
    ] {
        let path = rootfs.join("usr/lib/systemd/system").join(alias);
        let actual =
            fs::read_link(&path).with_context(|| format!("missing D-Bus service alias {alias}"))?;
        if actual != Path::new(target) {
            bail!(
                "invalid D-Bus alias {alias}: expected {target}, got {}",
                actual.display()
            );
        }
    }

    Ok(())
}

fn validate_executable_runtime_closure(binary: &Path, rootfs: &Path) -> Result<()> {
    let file = Command::new("file")
        .args(["-L", path_str(binary)?])
        .output()
        .with_context(|| format!("failed to inspect {} with file", binary.display()))?;
    if !file.status.success() || !String::from_utf8_lossy(&file.stdout).contains("ELF") {
        bail!(
            "runtime closure target is not an ELF executable: {}",
            binary.display()
        );
    }
    let readelf = Command::new("readelf")
        .args(["-d", path_str(binary)?])
        .output()
        .with_context(|| format!("failed to inspect {} with readelf", binary.display()))?;
    if !readelf.status.success() {
        bail!(
            "readelf failed for runtime closure target {}",
            binary.display()
        );
    }

    let library_dirs = [
        rootfs.join("usr/lib/x86_64-linux-gnu"),
        rootfs.join("usr/lib/x86_64-linux-gnu/systemd"),
        rootfs.join("lib/x86_64-linux-gnu"),
        rootfs.join("usr/lib"),
        rootfs.join("lib"),
    ];
    let library_path = std::env::join_paths(library_dirs.iter())
        .context("failed to construct rootfs runtime library path")?;
    let ldd = Command::new("ldd")
        .arg(binary)
        .env("LD_LIBRARY_PATH", library_path)
        .output()
        .with_context(|| format!("failed to inspect {} with ldd", binary.display()))?;
    let ldd_text = String::from_utf8(ldd.stdout).context("ldd output was not UTF-8")?;
    if !ldd.status.success() || ldd_text.contains("not found") {
        bail!(
            "unresolved runtime dependency for {}:\n{}",
            binary.display(),
            ldd_text
        );
    }
    for token in ldd_text
        .split_whitespace()
        .filter(|token| token.starts_with('/'))
    {
        let dependency = Path::new(token);
        let staged = if dependency.starts_with(rootfs) {
            dependency.to_path_buf()
        } else {
            rootfs.join(dependency.strip_prefix("/").unwrap_or(dependency))
        };
        if !staged.exists() {
            bail!(
                "runtime dependency {} for {} is not staged at {}",
                dependency.display(),
                binary.display(),
                staged.display()
            );
        }
    }
    Ok(())
}

fn validate_network_configuration(rootfs: &Path) -> Result<()> {
    for rel in [
        "etc/systemd/network/20-mattos-wired.network",
        "etc/systemd/resolved.conf",
        "etc/systemd/timesyncd.conf",
        "etc/nsswitch.conf",
        "etc/ssl/certs/ca-certificates.crt",
        "run/systemd/resolve",
        "usr/lib/systemd/systemd-networkd",
        "usr/lib/systemd/systemd-resolved",
        "usr/lib/systemd/systemd-timesyncd",
        "usr/lib/x86_64-linux-gnu/libnss_resolve.so.2",
        "etc/systemd/system/multi-user.target.wants/systemd-networkd.service",
        "etc/systemd/system/multi-user.target.wants/systemd-resolved.service",
        "etc/systemd/system/multi-user.target.wants/systemd-timesyncd.service",
    ] {
        if !path_entry_exists(&rootfs.join(rel)) {
            bail!("required network runtime path missing: /{rel}");
        }
    }
    let network = fs::read_to_string(rootfs.join("etc/systemd/network/20-mattos-wired.network"))?;
    if !network.contains("Type=ether") || !network.contains("DHCP=ipv4") {
        bail!("wired network configuration must match Ethernet by type and enable IPv4 DHCP");
    }
    let nsswitch = fs::read_to_string(rootfs.join("etc/nsswitch.conf"))?;
    for database in ["passwd:", "group:", "shadow:", "hosts:", "networks:"] {
        if !nsswitch.lines().any(|line| line.starts_with(database)) {
            bail!("nsswitch configuration lacks {database}");
        }
    }
    if !nsswitch
        .lines()
        .any(|line| line.starts_with("hosts:") && line.contains("resolve"))
    {
        bail!("nsswitch hosts database does not use systemd-resolved");
    }
    let ca_bundle = fs::read(rootfs.join("etc/ssl/certs/ca-certificates.crt"))?;
    if ca_bundle.len() < 100_000
        || !ca_bundle
            .windows(27)
            .any(|window| window == b"-----BEGIN CERTIFICATE-----")
    {
        bail!("CA bundle is missing or does not contain PEM certificates");
    }
    #[cfg(unix)]
    {
        let target = fs::read_link(rootfs.join("etc/resolv.conf"))?;
        if target != Path::new("/run/systemd/resolve/stub-resolv.conf") {
            bail!(
                "/etc/resolv.conf has unexpected target {}",
                target.display()
            );
        }
    }

    let account_ids = [
        ("systemd-network", 192_u32),
        ("systemd-resolve", 193_u32),
        ("systemd-timesync", 194_u32),
    ];
    let passwd = fs::read_to_string(rootfs.join("etc/passwd"))?;
    let group = fs::read_to_string(rootfs.join("etc/group"))?;
    for (name, id) in account_ids {
        let id_field = format!(":{id}:");
        if passwd.lines().any(|line| line.contains(&id_field))
            || group.lines().any(|line| line.contains(&id_field))
        {
            bail!("network service account {name} ID {id} collides with a static account");
        }
        let sysusers = rootfs
            .join("usr/lib/sysusers.d")
            .join(format!("{name}.conf"));
        let body = fs::read_to_string(&sysusers)
            .with_context(|| format!("missing sysusers definition for {name}"))?;
        if !body.lines().any(|line| {
            line.contains(name) && line.split_whitespace().any(|field| field == id.to_string())
        }) {
            bail!("sysusers definition for {name} does not pin ID {id}");
        }
    }
    Ok(())
}

fn apply_live_profile(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let live_src = repo_root.join("src/system/profiles/live");
    if !live_src.exists() {
        bail!(
            "MattOS live profile missing at {}; expected live profile overlay",
            live_src.display()
        );
    }
    copy_tree_excluding_dotgit(&live_src, rootfs)?;

    let notice_script = rootfs.join("etc/profile.d/10-mattos-live-notice.sh");
    if notice_script.exists() {
        set_mode(notice_script, 0o755)?;
    }

    Ok(())
}

fn verify_required_pam_modules(rootfs: &Path) -> Result<()> {
    let security_dirs = [
        rootfs.join("usr/lib/x86_64-linux-gnu/security"),
        rootfs.join("usr/lib/security"),
    ];
    for module in REQUIRED_PAM_MODULES {
        let mut found = false;
        for dir in &security_dirs {
            if dir.join(module).exists() {
                found = true;
                break;
            }
        }
        if !found {
            bail!(
                "required PAM module {} missing from rootfs security dirs",
                module
            );
        }
    }

    Ok(())
}

fn enforce_auth_file_modes(rootfs: &Path) -> Result<()> {
    for (rel, mode) in [
        ("etc/shadow", 0o600),
        ("etc/gshadow", 0o600),
        ("etc/passwd", 0o644),
        ("etc/group", 0o644),
        ("etc/sudoers", 0o440),
        ("usr/bin/login", 0o4755),
        ("usr/bin/su", 0o4755),
        ("usr/bin/passwd", 0o4755),
        ("usr/bin/sudo", 0o4755),
    ] {
        let path = rootfs.join(rel);
        if !path.exists() {
            bail!("expected auth file missing at {}", path.display());
        }
        set_mode(path, mode)?;
    }

    let sudoers_dir = rootfs.join("etc/sudoers.d");
    if !sudoers_dir.exists() {
        bail!(
            "expected sudoers include dir missing at {}",
            sudoers_dir.display()
        );
    }
    set_mode(sudoers_dir, 0o750)?;

    for rel in ["etc/sudoers.d/00-mattos-live", "etc/sudoers.d/README"] {
        let path = rootfs.join(rel);
        if path.exists() {
            set_mode(path, 0o440)?;
        }
    }

    let root_home = rootfs.join("root");
    if root_home.exists() {
        set_mode(root_home, 0o700)?;
    }
    let live_home = rootfs.join("home/mattos");
    if live_home.exists() {
        set_mode(live_home, 0o750)?;
    }

    Ok(())
}

#[cfg(unix)]
fn validate_auth_file_modes(rootfs: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    for (rel, expected_mode) in [
        ("etc/shadow", 0o600),
        ("etc/gshadow", 0o600),
        ("etc/passwd", 0o644),
        ("etc/group", 0o644),
        ("etc/sudoers", 0o440),
        ("etc/sudoers.d", 0o750),
        ("usr/bin/login", 0o4755),
        ("usr/bin/su", 0o4755),
        ("usr/bin/passwd", 0o4755),
        ("usr/bin/sudo", 0o4755),
        ("root", 0o700),
        ("home/mattos", 0o750),
    ] {
        let path = rootfs.join(rel);
        let actual_mode = fs::metadata(&path)
            .with_context(|| format!("failed to stat security-sensitive path {}", path.display()))?
            .permissions()
            .mode()
            & 0o7777;
        if actual_mode != expected_mode {
            bail!(
                "unsafe mode {:04o} on {}; expected {:04o}",
                actual_mode,
                path.display(),
                expected_mode
            );
        }
    }

    for rel in ["etc/sudoers.d/00-mattos-live", "etc/sudoers.d/README"] {
        let path = rootfs.join(rel);
        if path.exists() {
            let actual_mode = fs::metadata(&path)
                .with_context(|| format!("failed to stat {}", path.display()))?
                .permissions()
                .mode()
                & 0o7777;
            if actual_mode != 0o440 {
                bail!(
                    "unsafe mode {:04o} on {}; expected 0440",
                    actual_mode,
                    path.display()
                );
            }
        }
    }

    Ok(())
}

#[cfg(not(unix))]
fn validate_auth_file_modes(_rootfs: &Path) -> Result<()> {
    bail!("authentication file-mode validation requires a Unix host")
}

fn validate_account_database(rootfs: &Path) -> Result<()> {
    let passwd_path = rootfs.join("etc/passwd");
    let group_path = rootfs.join("etc/group");
    let shadow_path = rootfs.join("etc/shadow");
    let gshadow_path = rootfs.join("etc/gshadow");

    for path in [&passwd_path, &group_path, &shadow_path, &gshadow_path] {
        if !path.exists() {
            bail!(
                "required account database file missing at {}",
                path.display()
            );
        }
    }

    let passwd_body = fs::read_to_string(&passwd_path)
        .with_context(|| format!("failed to read {}", passwd_path.display()))?;
    let group_body = fs::read_to_string(&group_path)
        .with_context(|| format!("failed to read {}", group_path.display()))?;

    if passwd_body.contains("matt-alienware") || passwd_body.contains("matt:") {
        bail!("passwd file appears to contain host developer username leakage")
    }

    let mut seen_uids = BTreeSet::<u32>::new();
    let mut seen_gids = BTreeSet::<u32>::new();
    let mut saw_root = false;
    let mut saw_live = false;

    for line in passwd_body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() != 7 {
            bail!("invalid passwd entry format: {line}");
        }
        let user = parts[0];
        let uid = parts[2]
            .parse::<u32>()
            .with_context(|| format!("invalid uid in passwd entry: {line}"))?;
        let gid = parts[3]
            .parse::<u32>()
            .with_context(|| format!("invalid gid in passwd entry: {line}"))?;

        if !seen_uids.insert(uid) {
            bail!("duplicate uid detected in passwd: {uid}")
        }

        if user == "root" {
            saw_root = true;
            if uid != 0 || gid != 0 || parts[5] != "/root" || parts[6] != "/bin/brush" {
                bail!("root account entry does not match expected MattOS policy")
            }
        }

        if user == "mattos" {
            saw_live = true;
            if uid != 1000 || gid != 1000 || parts[5] != "/home/mattos" || parts[6] != "/bin/brush"
            {
                bail!("live user mattos entry does not match expected MattOS policy")
            }
        }
    }

    if !saw_root {
        bail!("root account missing from passwd")
    }
    if !saw_live {
        bail!("live user mattos missing from passwd")
    }

    let mut saw_sudo_group = false;
    for line in group_body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() != 4 {
            bail!("invalid group entry format: {line}");
        }
        let name = parts[0];
        let gid = parts[2]
            .parse::<u32>()
            .with_context(|| format!("invalid gid in group entry: {line}"))?;
        if !seen_gids.insert(gid) {
            bail!("duplicate gid detected in group: {gid}")
        }
        if name == "sudo" {
            saw_sudo_group = true;
            if !parts[3].split(',').any(|m| m == "mattos") {
                bail!("sudo group exists but mattos is not a member")
            }
        }
    }

    if !saw_sudo_group {
        bail!("sudo administrative group missing from group database")
    }

    Ok(())
}

fn set_mode(path: PathBuf, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        let perms = std::os::unix::fs::PermissionsExt::from_mode(mode);
        fs::set_permissions(&path, perms)
            .with_context(|| format!("failed to set mode {:o} on {}", mode, path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}

fn copy_systemd_runtime_dependencies(rootfs: &Path) -> Result<()> {
    let mut binaries = Vec::new();
    for rel in [
        "usr/lib/systemd/systemd",
        "usr/lib/systemd/systemd-journald",
        "usr/lib/systemd/systemd-udevd",
        "usr/lib/systemd/systemd-networkd",
        "usr/lib/systemd/systemd-resolved",
        "usr/lib/systemd/systemd-timesyncd",
        "usr/lib/systemd/systemd-timedated",
        "usr/lib/systemd/systemd-logind",
        "usr/lib/systemd/systemd-user-runtime-dir",
        "usr/bin/systemctl",
        "usr/bin/journalctl",
        "usr/bin/busctl",
        "usr/bin/loginctl",
        "usr/bin/networkctl",
        "usr/bin/resolvectl",
        "usr/bin/timedatectl",
    ] {
        let p = rootfs.join(rel);
        if p.exists() {
            binaries.push(p);
        }
    }

    for bin in binaries {
        copy_runtime_dependencies(&bin, rootfs)?;
    }
    Ok(())
}

fn resolve_coreutils_multicall(repo_root: &Path) -> Result<PathBuf> {
    let candidates = [
        repo_root.join("src/userland/coreutils/target/release/coreutils"),
        repo_root.join("src/userland/coreutils/target/release/uutils"),
    ];
    candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .ok_or_else(|| anyhow!("coreutils multicall binary not found; run build coreutils first"))
}

fn list_coreutils_applets(coreutils_multicall: &Path) -> Result<Vec<String>> {
    let output = Command::new(coreutils_multicall)
        .arg("--list")
        .output()
        .with_context(|| format!("failed to run {} --list", coreutils_multicall.display()))?;
    if !output.status.success() {
        bail!("coreutils --list failed with status {}", output.status)
    }

    let raw = String::from_utf8(output.stdout).context("coreutils --list output was not UTF-8")?;
    let mut applets: Vec<String> = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('<') && *line != "uutils")
        .map(ToOwned::to_owned)
        .collect();
    applets.sort();
    applets.dedup();
    if applets.is_empty() {
        bail!("coreutils --list returned no applets")
    }
    Ok(applets)
}

fn install_userland_binary(
    repo_root: &Path,
    rootfs: &Path,
    spec: &BinaryInstallSpec,
) -> Result<()> {
    let source = repo_root.join(spec.source_rel);
    if !source.exists() {
        bail!(
            "{} binary missing at {}; run the matching build stage first",
            spec.command_name,
            source.display()
        )
    }

    let dst = rootfs.join("usr/bin").join(spec.install_name);
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(&source, &dst)
        .with_context(|| format!("failed to copy {} into rootfs", source.display()))?;
    copy_runtime_dependencies(&dst, rootfs)?;
    Ok(())
}

#[cfg(unix)]
fn create_command_aliases(rootfs: &Path, target_binary: &str, aliases: &[&str]) -> Result<()> {
    use std::os::unix::fs::symlink;

    let usr_bin = rootfs.join("usr/bin");
    for alias in aliases {
        let link = usr_bin.join(alias);
        if path_entry_exists(&link) {
            fs::remove_file(&link)
                .with_context(|| format!("failed to remove existing alias {}", link.display()))?;
        }
        symlink(format!("/bin/{target_binary}"), &link)
            .with_context(|| format!("failed to create alias {}", link.display()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn create_command_aliases(_rootfs: &Path, _target_binary: &str, _aliases: &[&str]) -> Result<()> {
    bail!("command alias generation requires Unix symlink support")
}

fn validate_no_duplicate_commands(provider_commands: &BTreeMap<&str, Vec<String>>) -> Result<()> {
    let mut owners = BTreeMap::<String, Vec<&str>>::new();
    for (provider, commands) in provider_commands {
        for command in commands {
            owners.entry(command.clone()).or_default().push(provider);
        }
    }

    let duplicates: Vec<String> = owners
        .iter()
        .filter_map(|(cmd, providers)| {
            if providers.len() > 1 {
                Some(format!("{} [{}]", cmd, providers.join(", ")))
            } else {
                None
            }
        })
        .collect();

    if !duplicates.is_empty() {
        bail!(
            "duplicate command ownership detected: {}",
            duplicates.join("; ")
        )
    }

    Ok(())
}

fn path_entry_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn write_userland_inventory(rootfs: &Path, inventory: &UserlandInventory) -> Result<()> {
    let path = rootfs.join(USERLAND_INVENTORY_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut lines = Vec::new();
    lines.push("# MattOS userland command inventory".to_string());
    lines.push("# format: provider:command".to_string());
    lines.push(String::new());
    lines.push("[implemented_upstream]".to_string());
    for entry in &inventory.implemented_upstream {
        lines.push(entry.clone());
    }
    lines.push(String::new());
    lines.push("[compiled]".to_string());
    for entry in &inventory.compiled {
        lines.push(entry.clone());
    }
    lines.push(String::new());
    lines.push("[installed]".to_string());
    for entry in &inventory.installed {
        lines.push(entry.clone());
    }
    lines.push(String::new());
    lines.push("[intentionally_excluded]".to_string());
    for entry in &inventory.intentionally_excluded {
        lines.push(entry.clone());
    }
    lines.push(String::new());
    lines.push("[failed_compatibility]".to_string());
    for entry in &inventory.failed_compatibility {
        lines.push(entry.clone());
    }

    fs::write(&path, lines.join("\n") + "\n")
        .with_context(|| format!("failed to write {}", path.display()))
}

fn build_initramfs(repo_root: &Path) -> Result<()> {
    let rootfs = repo_root.join("out/build/rootfs");
    if !rootfs.exists() {
        bail!("rootfs not found; run build rootfs first");
    }

    let out_build = repo_root.join("out/build");
    fs::create_dir_all(&out_build).context("failed to create out/build directory")?;
    validate_initramfs_archive_owner(INITRAMFS_ARCHIVE_OWNER)?;
    let archive_command = format!(
        "find . -exec touch -h -d @{MATTOS_SOURCE_DATE_EPOCH} {{}} + && find . -print0 | sort -z | cpio --null -o --quiet --reproducible --owner={INITRAMFS_ARCHIVE_OWNER} --format=newc | gzip -9n > ../initramfs.cpio.gz"
    );

    run_cmd(&rootfs, "bash", &["-lc", &archive_command])
}

fn validate_initramfs_archive_owner(owner: &str) -> Result<()> {
    if owner != "0:0" {
        bail!("unsafe initramfs archive owner {owner}; expected root ownership 0:0")
    }
    Ok(())
}

fn build_iso(repo_root: &Path) -> Result<()> {
    let grub_src = validate_grub_config_source(repo_root)?;

    let kernel = repo_root.join("src/kernel/linux/arch/x86/boot/bzImage");
    if !kernel.exists() {
        bail!(
            "kernel image missing at {}; build kernel first",
            kernel.display()
        );
    }

    let initramfs = repo_root.join("out/build/initramfs.cpio.gz");
    if !initramfs.exists() {
        bail!(
            "initramfs missing at {}; run build initramfs",
            initramfs.display()
        );
    }

    let iso_root = repo_root.join("out/build/iso");
    if iso_root.exists() {
        fs::remove_dir_all(&iso_root)
            .with_context(|| format!("failed to clean {}", iso_root.display()))?;
    }
    let grub_dir = iso_root.join("boot/grub");
    fs::create_dir_all(&grub_dir).context("failed to create ISO directory layout")?;

    fs::copy(&kernel, iso_root.join("boot/vmlinuz"))
        .context("failed to stage kernel into ISO tree")?;
    fs::copy(&initramfs, iso_root.join("boot/initramfs.cpio.gz"))
        .context("failed to stage initramfs into ISO tree")?;
    let staged_grub_cfg = grub_dir.join("grub.cfg");
    fs::copy(&grub_src, &staged_grub_cfg).context("failed to copy grub config")?;
    validate_staged_grub_config(&staged_grub_cfg)?;

    let src_grub_text = fs::read_to_string(&grub_src)
        .with_context(|| format!("failed to read {}", grub_src.display()))?;
    let staged_grub_text = fs::read_to_string(&staged_grub_cfg)
        .with_context(|| format!("failed to read {}", staged_grub_cfg.display()))?;
    if src_grub_text != staged_grub_text {
        bail!(
            "staged GRUB config at {} differs from authoritative source {}",
            staged_grub_cfg.display(),
            grub_src.display()
        );
    }

    let out_images = repo_root.join("out/images");
    fs::create_dir_all(&out_images).context("failed to create out/images")?;
    run_cmd_with_env_overrides(
        repo_root,
        "grub-mkrescue",
        &[
            "-o",
            "out/images/mattos-x86_64.iso",
            "--directory=/usr/lib/grub/i386-pc",
            "out/build/iso",
            "--modification-date=2026010100000000",
            "--set_all_file_dates",
            "2026010100000000",
        ],
        &[("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string())],
    )
}

fn validate_grub_config_source(repo_root: &Path) -> Result<PathBuf> {
    let authoritative = repo_root.join(AUTHORITATIVE_GRUB_CFG);
    if !authoritative.exists() {
        bail!(
            "authoritative GRUB config missing at {}; expected single source at {}",
            authoritative.display(),
            AUTHORITATIVE_GRUB_CFG
        );
    }

    for obsolete in OBSOLETE_GRUB_CFG_PATHS {
        let obsolete_path = repo_root.join(obsolete);
        if obsolete_path.exists() {
            bail!(
                "obsolete GRUB config path detected at {}; remove stale duplicate and keep only {}",
                obsolete_path.display(),
                AUTHORITATIVE_GRUB_CFG
            );
        }
    }

    Ok(authoritative)
}

fn validate_staged_grub_config(path: &Path) -> Result<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read staged grub config {}", path.display()))?;

    for needle in [
        GRUB_SYSTEMD_ENTRY,
        GRUB_RESCUE_ENTRY,
        GRUB_SYSTEMD_RDINIT,
        GRUB_RESCUE_RDINIT,
    ] {
        if !content.contains(needle) {
            bail!(
                "staged GRUB config {} is missing required marker: {}",
                path.display(),
                needle
            );
        }
    }

    Ok(())
}

fn run_qemu(repo_root: &Path) -> Result<()> {
    let iso = repo_root.join("out/images/mattos-x86_64.iso");
    if !iso.exists() {
        bail!("ISO missing at {}; run build iso first", iso.display());
    }
    let logs = repo_root.join("out/logs");
    fs::create_dir_all(&logs).context("failed to create out/logs")?;
    let log_path = logs.join("qemu-boot.log");
    let serial_log_path = logs.join("qemu-serial.log");
    let serial_arg = format!(
        "file:{}",
        serial_log_path
            .to_str()
            .ok_or_else(|| anyhow!("invalid qemu serial log path"))?
    );

    run_cmd(
        repo_root,
        "qemu-system-x86_64",
        &[
            "-m",
            "1024",
            "-cdrom",
            iso.to_str().ok_or_else(|| anyhow!("invalid ISO path"))?,
            "-boot",
            "d",
            "-serial",
            serial_arg.as_str(),
            "-D",
            log_path
                .to_str()
                .ok_or_else(|| anyhow!("invalid qemu log path"))?,
        ],
    )
}

fn copy_runtime_dependencies(binary: &Path, rootfs: &Path) -> Result<()> {
    let library_path = std::env::join_paths([
        rootfs.join("usr/lib/x86_64-linux-gnu"),
        rootfs.join("usr/lib/x86_64-linux-gnu/systemd"),
        rootfs.join("lib/x86_64-linux-gnu"),
        rootfs.join("usr/lib"),
        rootfs.join("lib"),
    ])
    .context("failed to construct rootfs runtime library path")?;
    let output = Command::new("ldd")
        .arg(binary)
        .env("LD_LIBRARY_PATH", library_path)
        .output()
        .with_context(|| {
            format!(
                "failed to inspect runtime dependencies for {}",
                binary.display()
            )
        })?;
    if !output.status.success() {
        return Ok(());
    }
    let text = String::from_utf8(output.stdout).context("ldd output was not UTF-8")?;

    for token in text.split_whitespace() {
        if !token.starts_with('/') {
            continue;
        }
        let src = Path::new(token);
        if !src.exists() {
            continue;
        }
        if src.starts_with(rootfs) {
            continue;
        }
        let rel = src.strip_prefix("/").unwrap_or(src);
        let dst = rootfs.join(rel);
        if dst.exists() {
            continue;
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::copy(src, &dst)
            .with_context(|| format!("failed to copy runtime dependency {}", src.display()))?;
    }

    Ok(())
}

fn run_cmd(cwd: &Path, program: &str, args: &[&str]) -> Result<()> {
    println!("> {} {}", program, args.join(" "));
    let status = run_cmd_status(cwd, program, args)?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "command failed with status {status}: {} {}",
            program,
            args.join(" ")
        )
    }
}

fn run_cmd_status(cwd: &Path, program: &str, args: &[&str]) -> Result<std::process::ExitStatus> {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    apply_mattos_sysroot_environment(&mut command, cwd, program, &[])?;
    command
        .status()
        .with_context(|| format!("failed to spawn command: {program}"))
}

fn run_cmd_with_env(
    cwd: &Path,
    program: &str,
    args: &[&str],
    tool_env: Option<&LocalToolEnv>,
) -> Result<()> {
    println!("> {} {}", program, args.join(" "));
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd);

    if let Some(env) = tool_env {
        let current_path = std::env::var("PATH").unwrap_or_default();
        let composed_path = format!("{}:{}", env.tool_bin_dir.display(), current_path);
        let current_ld = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let composed_ld = if current_ld.is_empty() {
            env.tool_lib_dir.display().to_string()
        } else {
            format!("{}:{current_ld}", env.tool_lib_dir.display())
        };
        let include = env.tool_include_dir.display().to_string();
        let lib = env.tool_lib_dir.display().to_string();

        cmd.env("PATH", composed_path)
            .env("LD_LIBRARY_PATH", composed_ld)
            .env(
                "BISON_PKGDATADIR",
                env.bison_pkg_data_dir.display().to_string(),
            )
            .env("M4", env.m4_bin.display().to_string())
            .env("CFLAGS", format!("-I{include}"))
            .env("HOSTCFLAGS", format!("-I{include}"))
            .env("LDFLAGS", format!("-L{lib}"))
            .env("HOSTLDFLAGS", format!("-L{lib}"));
    }
    apply_mattos_sysroot_environment(&mut cmd, cwd, program, &[])?;

    let status = cmd
        .status()
        .with_context(|| format!("failed to spawn command: {program}"))?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "command failed with status {status}: {} {}",
            program,
            args.join(" ")
        )
    }
}

fn run_cmd_with_env_overrides(
    cwd: &Path,
    program: &str,
    args: &[&str],
    env_overrides: &[(&str, String)],
) -> Result<()> {
    println!("> {} {}", program, args.join(" "));
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd);
    for (key, value) in env_overrides {
        cmd.env(key, value);
    }
    apply_mattos_sysroot_environment(&mut cmd, cwd, program, env_overrides)?;

    let status = cmd
        .status()
        .with_context(|| format!("failed to spawn command: {program}"))?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "command failed with status {status}: {} {}",
            program,
            args.join(" ")
        )
    }
}

fn apply_mattos_sysroot_environment(
    command: &mut Command,
    cwd: &Path,
    program: &str,
    overrides: &[(&str, String)],
) -> Result<()> {
    let Some(repo_root) = cwd.ancestors().find(|candidate| {
        candidate
            .join("src/tools/mattos-build/Cargo.toml")
            .is_file()
    }) else {
        return Ok(());
    };
    let sysroot = repo_root.join("out/sysroot");
    if !sysroot.join("usr/include/stdio.h").is_file()
        || cwd.starts_with(repo_root.join("src/kernel/linux"))
        || cwd.starts_with(repo_root.join("out/build/glibc"))
        || cwd.starts_with(repo_root.join("src/system/libc/glibc"))
    {
        return Ok(());
    }
    let sysroot_flag = format!("--sysroot={}", sysroot.display());
    let value_for = |key: &str| {
        overrides
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var(key).ok())
            .unwrap_or_default()
    };
    for key in ["CPPFLAGS", "CFLAGS", "CXXFLAGS", "LDFLAGS"] {
        let current = value_for(key);
        let mut value = if current.split_whitespace().any(|flag| flag == sysroot_flag) {
            current
        } else if current.is_empty() {
            sysroot_flag.clone()
        } else {
            format!("{current} {sysroot_flag}")
        };
        if matches!(key, "CFLAGS" | "CXXFLAGS") {
            let prefix_map = format!("-ffile-prefix-map={}=/usr/src/mattos", repo_root.display());
            if !value.split_whitespace().any(|flag| flag == prefix_map) {
                value.push_str(&format!(
                    " {prefix_map} -fdebug-prefix-map={}=/usr/src/mattos -fmacro-prefix-map={}=/usr/src/mattos",
                    repo_root.display(),
                    repo_root.display()
                ));
            }
        }
        command.env(key, value);
    }
    if program == "cargo" {
        let current = value_for("RUSTFLAGS");
        let rust_sysroot = format!("-C link-arg={sysroot_flag}");
        let value = if current.contains(&rust_sysroot) {
            current
        } else if current.is_empty() {
            rust_sysroot
        } else {
            format!("{current} {rust_sysroot}")
        };
        command.env("RUSTFLAGS", value);
    }
    command.env("MATTOS_SYSROOT", &sysroot);
    Ok(())
}

fn run_cmd_output(cwd: &Path, program: &str, args: &[&str]) -> Result<Output> {
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to spawn command: {program}"))
}

fn run_cmd_capture(cwd: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = run_cmd_output(cwd, program, args)?;
    if !output.status.success() {
        bail!(
            "command failed with status {}: {} {}",
            output.status,
            program,
            args.join(" ")
        );
    }
    let text = String::from_utf8(output.stdout).context("stdout was not valid UTF-8")?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ok(cwd: &Path, program: &str, args: &[&str]) {
        let status = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("spawn test command");
        assert!(
            status.success(),
            "command failed: {program} {}",
            args.join(" ")
        );
    }

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(path, body).expect("write file");
    }

    fn init_git_repo(path: &Path) {
        run_ok(path, "git", &["init", "-b", "main"]);
        run_ok(path, "git", &["config", "user.name", "Test User"]);
        run_ok(
            path,
            "git",
            &["config", "user.email", "test@example.invalid"],
        );
    }

    fn make_upstream_component_repo(name: &str, file_name: &str, body: &str) -> tempfile::TempDir {
        let upstream = tempfile::tempdir().expect("upstream tempdir");
        let root = upstream.path();
        init_git_repo(root);
        write(&root.join(file_name), body);
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", &format!("init {name}")]);
        upstream
    }

    #[test]
    fn path_safety_rejects_parent_dir() {
        let root = std::env::temp_dir().join("mattos-path-safety");
        let result = resolve_component_destination(&root, "../escape");
        assert!(result.is_err());
    }

    #[test]
    fn initial_import_refuses_meaningful_preexisting_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let destination = root.join("src/userland/grep");
        write(&destination.join("real.rs"), "fn main() {}\n");
        let result = assert_initial_destination_safe(&destination);
        assert!(result.is_err());
    }

    #[test]
    fn initial_import_allows_placeholder_only_destination() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let destination = root.join("src/userland/grep");
        write(&destination.join(".gitkeep"), "");
        write(&destination.join("README.md"), "placeholder\n");
        assert_initial_destination_safe(&destination)
            .expect("placeholder-only destination should pass");
    }

    #[test]
    fn metadata_roundtrip_written_to_state_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let state = SyncState {
            component: "linux".to_string(),
            repo: "https://github.com/torvalds/linux.git".to_string(),
            branch: "master".to_string(),
            imported_commit: "abc123".to_string(),
            imported_at_utc: "2026-01-01T00:00:00Z".to_string(),
            sync_method: "copy".to_string(),
            destination_path: "src/kernel/linux".to_string(),
        };

        write_sync_state(root, "linux", &state).expect("write state");
        let loaded = read_sync_state(root, "linux")
            .expect("read state")
            .expect("present");
        assert_eq!(loaded.repo, state.repo);
        assert_eq!(loaded.branch, state.branch);
        assert_eq!(loaded.imported_commit, state.imported_commit);
    }

    #[test]
    fn sync_update_produces_conflict_markers() {
        let upstream = tempfile::tempdir().expect("upstream tempdir");
        let upstream_root = upstream.path();
        run_ok(upstream_root, "git", &["init", "-b", "main"]);
        run_ok(
            upstream_root,
            "git",
            &["config", "user.name", "Upstream User"],
        );
        run_ok(
            upstream_root,
            "git",
            &["config", "user.email", "upstream@example.invalid"],
        );
        write(&upstream_root.join("README"), "base\n");
        run_ok(upstream_root, "git", &["add", "."]);
        run_ok(upstream_root, "git", &["commit", "-m", "base"]);

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        run_ok(root, "git", &["init"]);
        run_ok(root, "git", &["config", "user.name", "MattOS User"]);
        run_ok(
            root,
            "git",
            &["config", "user.email", "mattos@example.invalid"],
        );
        write(&root.join("README.md"), "repo\n");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "init"]);

        let comp = ComponentDef {
            name: "linux".to_string(),
            repo: upstream_root.to_string_lossy().to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/kernel/linux".to_string(),
            sync: "copy".to_string(),
        };
        import_component(root, &comp, false).expect("initial import");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "import"]);

        write(&root.join("src/kernel/linux/README"), "local\n");
        run_ok(root, "git", &["add", "src/kernel/linux/README"]);
        run_ok(root, "git", &["commit", "-m", "local edit"]);

        write(&upstream_root.join("README"), "upstream\n");
        run_ok(upstream_root, "git", &["add", "README"]);
        run_ok(upstream_root, "git", &["commit", "-m", "upstream edit"]);

        let result = import_component(root, &comp, true);
        assert!(result.is_err());

        let merged =
            fs::read_to_string(root.join("src/kernel/linux/README")).expect("read merged file");
        assert!(merged.contains("<<<<<<<"));
        assert!(merged.contains(">>>>>>>"));
    }

    #[test]
    fn unrelated_dirty_files_do_not_block_component_import() {
        let grep_upstream = make_upstream_component_repo(
            "grep",
            "Cargo.toml",
            "[package]\nname='uu_grep'\nversion='0.1.0'\n",
        );

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        init_git_repo(root);
        write(&root.join("README.md"), "base\n");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "init"]);

        write(
            &root.join("upstream/sources.toml"),
            &format!(
                "[[component]]\nname='grep'\nrepo='{}'\nbranch='main'\npath='src/userland/grep'\nsync='copy'\n",
                grep_upstream.path().display()
            ),
        );
        write(&root.join("docs/dirty-note.md"), "unrelated dirty file\n");

        import_sources(root, false, Some("grep".to_string()), false)
            .expect("import should succeed");
        assert!(root.join("src/userland/grep/Cargo.toml").exists());
    }

    #[test]
    fn dirty_other_component_does_not_block_selected_component_import() {
        let grep_upstream = make_upstream_component_repo(
            "grep",
            "Cargo.toml",
            "[package]\nname='uu_grep'\nversion='0.1.0'\n",
        );
        let sed_upstream = make_upstream_component_repo(
            "sed",
            "Cargo.toml",
            "[package]\nname='sed'\nversion='0.1.0'\n",
        );

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        init_git_repo(root);
        write(&root.join("README.md"), "repo\n");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "init"]);

        write(
            &root.join("upstream/sources.toml"),
            &format!(
                "[[component]]\nname='grep'\nrepo='{}'\nbranch='main'\npath='src/userland/grep'\nsync='copy'\n\n[[component]]\nname='sed'\nrepo='{}'\nbranch='main'\npath='src/userland/sed'\nsync='copy'\n",
                grep_upstream.path().display(),
                sed_upstream.path().display()
            ),
        );

        write(&root.join("src/userland/sed/local.txt"), "dirty sed tree\n");
        import_sources(root, false, Some("grep".to_string()), false)
            .expect("grep import should succeed");
        assert!(root.join("src/userland/grep/Cargo.toml").exists());
    }

    #[test]
    fn failed_initial_import_does_not_write_state_metadata() {
        let upstream = make_upstream_component_repo(
            "grep",
            "Cargo.toml",
            "[package]\nname='uu_grep'\nversion='0.1.0'\n",
        );

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        init_git_repo(root);
        write(&root.join("README.md"), "repo\n");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "init"]);

        let comp = ComponentDef {
            name: "grep".to_string(),
            repo: upstream.path().to_string_lossy().to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/userland/grep".to_string(),
            sync: "copy".to_string(),
        };

        write(
            &root.join("src/userland/grep/not-placeholder.txt"),
            "data\n",
        );
        let result = import_component(root, &comp, false);
        assert!(result.is_err());
        assert!(read_sync_state(root, "grep").expect("read state").is_none());
    }

    #[test]
    fn failed_sync_conflict_does_not_advance_state_commit() {
        let upstream = tempfile::tempdir().expect("upstream tempdir");
        let upstream_root = upstream.path();
        init_git_repo(upstream_root);
        write(&upstream_root.join("README"), "base\n");
        run_ok(upstream_root, "git", &["add", "."]);
        run_ok(upstream_root, "git", &["commit", "-m", "base"]);

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        init_git_repo(root);
        write(&root.join("README.md"), "repo\n");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "init"]);

        let comp = ComponentDef {
            name: "grep".to_string(),
            repo: upstream_root.to_string_lossy().to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/userland/grep".to_string(),
            sync: "copy".to_string(),
        };

        import_component(root, &comp, false).expect("initial import");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "import"]);
        let before = read_sync_state(root, "grep")
            .expect("read state")
            .expect("present")
            .imported_commit;

        write(&root.join("src/userland/grep/README"), "local\n");
        run_ok(root, "git", &["add", "src/userland/grep/README"]);
        run_ok(root, "git", &["commit", "-m", "local"]);

        write(&upstream_root.join("README"), "upstream\n");
        run_ok(upstream_root, "git", &["add", "README"]);
        run_ok(upstream_root, "git", &["commit", "-m", "upstream"]);

        let result = import_component(root, &comp, true);
        assert!(result.is_err());
        let after = read_sync_state(root, "grep")
            .expect("read state")
            .expect("present")
            .imported_commit;
        assert_eq!(before, after);
    }

    #[test]
    fn sync_preserves_uncommitted_local_component_changes() {
        let upstream = tempfile::tempdir().expect("upstream tempdir");
        let upstream_root = upstream.path();
        init_git_repo(upstream_root);
        write(&upstream_root.join("README"), "base\n");
        run_ok(upstream_root, "git", &["add", "."]);
        run_ok(upstream_root, "git", &["commit", "-m", "base"]);

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        init_git_repo(root);
        write(&root.join("README.md"), "repo\n");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "init"]);

        let comp = ComponentDef {
            name: "grep".to_string(),
            repo: upstream_root.to_string_lossy().to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/userland/grep".to_string(),
            sync: "copy".to_string(),
        };

        import_component(root, &comp, false).expect("initial import");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "import"]);

        write(&upstream_root.join("NEWS"), "upstream change\n");
        run_ok(upstream_root, "git", &["add", "NEWS"]);
        run_ok(upstream_root, "git", &["commit", "-m", "news"]);

        write(
            &root.join("src/userland/grep/local-only.txt"),
            "local edit\n",
        );
        import_component(root, &comp, true).expect("update should include local edits");

        assert_eq!(
            fs::read_to_string(root.join("src/userland/grep/local-only.txt"))
                .expect("read local file"),
            "local edit\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("src/userland/grep/NEWS")).expect("read upstream news"),
            "upstream change\n"
        );
    }

    #[test]
    fn path_safety_accepts_normal_relative_path() {
        let root = std::env::temp_dir().join("mattos-path-ok");
        let result = resolve_component_destination(&root, "src/kernel/linux").expect("valid path");
        assert!(result.ends_with(Path::new("src/kernel/linux")));
    }

    #[test]
    fn component_name_validation_rejects_separators() {
        assert!(validate_component_name("linux").is_ok());
        assert!(validate_component_name("bad/name").is_err());
    }

    #[test]
    fn preferred_distro_chooses_ubuntu_first() {
        let distros = vec!["Debian".to_string(), "Ubuntu-24.04".to_string()];
        let selected = preferred_distro(&distros).expect("selected distro");
        assert_eq!(selected, "Ubuntu-24.04");
    }

    #[test]
    fn shell_escape_quotes_spaces() {
        let escaped = shell_escape("hello world");
        assert_eq!(escaped, "'hello world'");
    }

    #[test]
    fn source_selection_requires_flag() {
        let components = vec![ComponentDef {
            name: "linux".to_string(),
            repo: "x".to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/kernel/linux".to_string(),
            sync: "copy".to_string(),
        }];
        let result = select_components(&components, false, None);
        assert!(result.is_err());
    }

    #[test]
    fn clear_directory_keeps_git_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        fs::create_dir_all(root.join(".git")).expect("create .git dir");
        write(&root.join("file.txt"), "x");
        clear_directory_contents(root).expect("clear");
        assert!(root.join(".git").exists());
        assert!(!root.join("file.txt").exists());
    }

    #[test]
    fn copy_tree_ignores_dotgit() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        fs::create_dir_all(src.join(".git")).expect("create .git");
        write(&src.join("a.txt"), "a");
        copy_tree_excluding_dotgit(&src, &dst).expect("copy tree");
        assert!(dst.join("a.txt").exists());
        assert!(!dst.join(".git").exists());
    }

    #[test]
    fn sync_state_absent_returns_none() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let state = read_sync_state(root, "missing").expect("read state");
        assert!(state.is_none());
    }

    #[test]
    fn no_distro_if_list_empty() {
        let selected = preferred_distro(&[]);
        assert!(selected.is_none());
    }

    #[test]
    fn source_selection_by_component() {
        let components = vec![
            ComponentDef {
                name: "linux".to_string(),
                repo: "x".to_string(),
                branch: "main".to_string(),
                revision: None,
                path: "src/kernel/linux".to_string(),
                sync: "copy".to_string(),
            },
            ComponentDef {
                name: "brush".to_string(),
                repo: "y".to_string(),
                branch: "main".to_string(),
                revision: None,
                path: "src/userland/brush".to_string(),
                sync: "copy".to_string(),
            },
        ];
        let selected = select_components(&components, false, Some("brush".to_string()))
            .expect("select component");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "brush");
    }

    #[test]
    fn path_safety_rejects_absolute() {
        let root = std::env::temp_dir().join("mattos-path-absolute");
        let absolute = if cfg!(windows) {
            "C:/absolute/path"
        } else {
            "/absolute/path"
        };
        assert!(resolve_component_destination(&root, absolute).is_err());
    }

    #[test]
    fn validate_component_name_accepts_dash_and_underscore() {
        assert!(validate_component_name("core-utils_1").is_ok());
    }

    #[test]
    fn run_cmd_capture_reads_stdout() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let text = if cfg!(windows) {
            run_cmd_capture(root, "cmd", &["/C", "echo", "hello"]).expect("capture")
        } else {
            run_cmd_capture(root, "sh", &["-c", "echo hello"]).expect("capture")
        };
        assert!(text.to_ascii_lowercase().contains("hello"));
    }

    #[test]
    fn selected_all_returns_everything() {
        let components = vec![
            ComponentDef {
                name: "linux".to_string(),
                repo: "x".to_string(),
                branch: "main".to_string(),
                revision: None,
                path: "src/kernel/linux".to_string(),
                sync: "copy".to_string(),
            },
            ComponentDef {
                name: "brush".to_string(),
                repo: "y".to_string(),
                branch: "main".to_string(),
                revision: None,
                path: "src/userland/brush".to_string(),
                sync: "copy".to_string(),
            },
        ];
        let selected = select_components(&components, true, None).expect("select all");
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn shell_escape_leaves_safe_text() {
        let escaped = shell_escape("src/kernel/linux");
        assert_eq!(escaped, "src/kernel/linux");
    }

    #[test]
    fn path_safety_rejects_parent_in_middle() {
        let root = std::env::temp_dir().join("mattos-path-middle");
        assert!(resolve_component_destination(&root, "kernel/../linux").is_err());
    }

    #[test]
    fn clear_directory_on_missing_dir_is_ok() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("missing");
        clear_directory_contents(&path).expect("clear missing");
    }

    #[test]
    fn no_duplicate_component_names_required_for_selection_logic() {
        let components = vec![ComponentDef {
            name: "linux".to_string(),
            repo: "x".to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/kernel/linux".to_string(),
            sync: "copy".to_string(),
        }];
        let selected =
            select_components(&components, false, Some("linux".to_string())).expect("select linux");
        assert_eq!(selected[0].path, "src/kernel/linux");
    }

    #[test]
    fn read_sources_parses_components() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write(
            &root.join("upstream/sources.toml"),
            "[[component]]\nname='linux'\nrepo='https://example.invalid/linux.git'\nbranch='main'\npath='src/kernel/linux'\nsync='copy'\n",
        );
        let sources = read_sources(root).expect("read sources");
        assert_eq!(sources.component.len(), 1);
        assert_eq!(sources.component[0].name, "linux");
    }

    #[test]
    fn grub_source_validation_requires_authoritative_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let result = validate_grub_config_source(tmp.path());
        assert!(result.is_err());
        let err = result.expect_err("missing source should fail").to_string();
        assert!(err.contains(AUTHORITATIVE_GRUB_CFG));
    }

    #[test]
    fn grub_source_validation_rejects_obsolete_duplicate_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write(
            &root.join(AUTHORITATIVE_GRUB_CFG),
            "menuentry \"MattOS (systemd)\" {}\nmenuentry \"MattOS (rescue init)\" {}\n",
        );
        write(&root.join(OBSOLETE_GRUB_CFG_PATHS[0]), "legacy duplicate\n");

        let result = validate_grub_config_source(root);
        assert!(result.is_err());
        let err = result.expect_err("duplicate should fail").to_string();
        assert!(err.contains(OBSOLETE_GRUB_CFG_PATHS[0]));
    }

    #[test]
    fn grub_source_validation_accepts_single_authoritative_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write(
            &root.join(AUTHORITATIVE_GRUB_CFG),
            "menuentry \"MattOS (systemd)\" {}\nmenuentry \"MattOS (rescue init)\" {}\n",
        );

        let source = validate_grub_config_source(root).expect("authoritative source should pass");
        assert!(source.ends_with(AUTHORITATIVE_GRUB_CFG));
    }

    #[test]
    fn staged_grub_validation_requires_normal_and_rescue_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("grub.cfg");
        write(
            &path,
            "set default=0\nmenuentry \"MattOS (systemd)\" { linux /boot/vmlinuz rdinit=/usr/lib/systemd/systemd }\n",
        );

        let result = validate_staged_grub_config(&path);
        assert!(result.is_err());
        let err = result.expect_err("missing rescue should fail").to_string();
        assert!(err.contains(GRUB_RESCUE_ENTRY));
    }

    #[test]
    fn staged_grub_validation_accepts_required_markers() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("grub.cfg");
        write(
            &path,
            "menuentry \"MattOS (systemd)\" { linux /boot/vmlinuz rdinit=/usr/lib/systemd/systemd }\nmenuentry \"MattOS (rescue init)\" { linux /boot/vmlinuz rdinit=/usr/libexec/mattos/rescue-init }\n",
        );

        validate_staged_grub_config(&path).expect("valid staged config should pass");
    }

    #[test]
    fn write_sync_state_creates_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let state = SyncState {
            component: "brush".to_string(),
            repo: "https://example.invalid/brush.git".to_string(),
            branch: "main".to_string(),
            imported_commit: "def456".to_string(),
            imported_at_utc: "2026-01-01T00:00:00Z".to_string(),
            sync_method: "copy".to_string(),
            destination_path: "src/userland/brush".to_string(),
        };
        write_sync_state(root, "brush", &state).expect("write state");
        assert!(root.join("upstream/state/brush.toml").exists());
    }

    #[test]
    fn check_name_rejects_empty() {
        assert!(validate_component_name("").is_err());
    }

    #[test]
    fn source_selection_unknown_component_fails() {
        let components = vec![ComponentDef {
            name: "linux".to_string(),
            repo: "x".to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/kernel/linux".to_string(),
            sync: "copy".to_string(),
        }];
        let result = select_components(&components, false, Some("missing".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn shell_escape_handles_quotes() {
        let escaped = shell_escape("a'b");
        assert_eq!(escaped, "'a'\\''b'");
    }

    #[test]
    fn preferred_distro_falls_back_to_first() {
        let distros = vec!["Debian".to_string(), "Arch".to_string()];
        let selected = preferred_distro(&distros).expect("selected distro");
        assert_eq!(selected, "Debian");
    }

    #[test]
    fn resolve_component_destination_joins_path() {
        let root = std::env::temp_dir().join("mattos-path-join");
        let resolved = resolve_component_destination(&root, "src/userland/brush").expect("resolve");
        assert!(resolved.ends_with("src/userland/brush"));
    }

    #[test]
    fn source_selection_all_ignores_component_flag() {
        let components = vec![ComponentDef {
            name: "linux".to_string(),
            repo: "x".to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/kernel/linux".to_string(),
            sync: "copy".to_string(),
        }];
        let selected =
            select_components(&components, true, Some("missing".to_string())).expect("select all");
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn copy_tree_copies_nested_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        write(&src.join("dir/nested.txt"), "nested");
        copy_tree_excluding_dotgit(&src, &dst).expect("copy");
        assert_eq!(
            fs::read_to_string(dst.join("dir/nested.txt")).expect("read nested"),
            "nested"
        );
    }

    #[test]
    fn development_sysroot_overlay_replaces_existing_symlinks() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        write(&src.join("lib/libexample.so.1.0"), "runtime");
        std::os::unix::fs::symlink("libexample.so.1.0", src.join("lib/libexample.so.1"))
            .expect("source symlink");

        copy_tree_contents(&src, &dst).expect("initial overlay");
        copy_tree_contents(&src, &dst).expect("idempotent overlay");

        assert_eq!(
            fs::read_link(dst.join("lib/libexample.so.1")).expect("destination symlink"),
            Path::new("libexample.so.1.0")
        );
    }

    #[test]
    fn validate_component_name_rejects_space() {
        assert!(validate_component_name("bad name").is_err());
    }

    #[test]
    fn path_safety_disallows_dotdot_prefix() {
        let root = std::env::temp_dir().join("mattos-path-prefix");
        assert!(resolve_component_destination(&root, "..\\escape").is_err());
    }

    #[test]
    fn read_sync_state_invalid_toml_errors() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write(&root.join("upstream/state/linux.toml"), "not=toml=");
        let result = read_sync_state(root, "linux");
        assert!(result.is_err());
    }

    #[test]
    fn source_file_missing_is_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let result = read_sources(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn kernel_path_guard_allows_non_mnt_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let result = assert_kernel_build_path_safe(tmp.path());
        assert!(result.is_ok());
    }

    #[test]
    fn require_wsl_ubuntu_errors_without_wsl_install() {
        if cfg!(windows) {
            let status = detect_wsl_status().expect("status");
            if !status.wsl_installed {
                let result = require_wsl_ubuntu("Ubuntu");
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn read_sources_parses_systemd_component() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write(
            &root.join("upstream/sources.toml"),
            "[[component]]\nname='systemd'\nrepo='https://github.com/systemd/systemd.git'\nbranch='main'\npath='src/system/systemd'\nsync='copy'\n",
        );
        let sources = read_sources(root).expect("read sources");
        assert_eq!(sources.component.len(), 1);
        assert_eq!(sources.component[0].name, "systemd");
        assert_eq!(sources.component[0].path, "src/system/systemd");
    }

    #[test]
    fn systemd_import_destination_is_safe() {
        let root = std::env::temp_dir().join("mattos-systemd-path");
        let safe = resolve_component_destination(&root, "src/system/systemd").expect("resolve");
        assert!(safe.ends_with("src/system/systemd"));
        assert!(resolve_component_destination(&root, "src/system/../escape").is_err());
    }

    #[test]
    fn systemd_initial_import_writes_state() {
        let upstream = tempfile::tempdir().expect("upstream tempdir");
        let upstream_root = upstream.path();
        run_ok(upstream_root, "git", &["init", "-b", "main"]);
        run_ok(
            upstream_root,
            "git",
            &["config", "user.name", "Upstream User"],
        );
        run_ok(
            upstream_root,
            "git",
            &["config", "user.email", "upstream@example.invalid"],
        );
        write(
            &upstream_root.join("meson.build"),
            "project('systemd', 'c')\n",
        );
        run_ok(upstream_root, "git", &["add", "."]);
        run_ok(upstream_root, "git", &["commit", "-m", "init"]);

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        run_ok(root, "git", &["init"]);
        run_ok(root, "git", &["config", "user.name", "MattOS User"]);
        run_ok(
            root,
            "git",
            &["config", "user.email", "mattos@example.invalid"],
        );
        write(&root.join("README.md"), "repo\n");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "init"]);

        let comp = ComponentDef {
            name: "systemd".to_string(),
            repo: upstream_root.to_string_lossy().to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/system/systemd".to_string(),
            sync: "copy".to_string(),
        };
        import_component(root, &comp, false).expect("initial import");
        assert!(root.join("src/system/systemd/meson.build").exists());

        let state = read_sync_state(root, "systemd")
            .expect("read state")
            .expect("state exists");
        assert_eq!(state.component, "systemd");
        assert_eq!(state.repo, comp.repo);
        assert_eq!(state.destination_path, "src/system/systemd");
    }

    #[test]
    fn systemd_sync_preserves_local_modifications() {
        let upstream = tempfile::tempdir().expect("upstream tempdir");
        let upstream_root = upstream.path();
        run_ok(upstream_root, "git", &["init", "-b", "main"]);
        run_ok(
            upstream_root,
            "git",
            &["config", "user.name", "Upstream User"],
        );
        run_ok(
            upstream_root,
            "git",
            &["config", "user.email", "upstream@example.invalid"],
        );
        write(&upstream_root.join("meson.build"), "base\n");
        run_ok(upstream_root, "git", &["add", "."]);
        run_ok(upstream_root, "git", &["commit", "-m", "base"]);

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        run_ok(root, "git", &["init"]);
        run_ok(root, "git", &["config", "user.name", "MattOS User"]);
        run_ok(
            root,
            "git",
            &["config", "user.email", "mattos@example.invalid"],
        );
        write(&root.join("README.md"), "repo\n");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "init"]);

        let comp = ComponentDef {
            name: "systemd".to_string(),
            repo: upstream_root.to_string_lossy().to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/system/systemd".to_string(),
            sync: "copy".to_string(),
        };
        import_component(root, &comp, false).expect("initial import");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "import"]);

        write(
            &root.join("src/system/systemd/meson.build"),
            "local change\n",
        );
        run_ok(root, "git", &["add", "src/system/systemd/meson.build"]);
        run_ok(root, "git", &["commit", "-m", "local"]);

        write(&upstream_root.join("README"), "upstream only\n");
        run_ok(upstream_root, "git", &["add", "README"]);
        run_ok(upstream_root, "git", &["commit", "-m", "upstream"]);

        import_component(root, &comp, true).expect("update without conflict");
        let local = fs::read_to_string(root.join("src/system/systemd/meson.build"))
            .expect("read local file");
        assert_eq!(local, "local change\n");
    }

    #[test]
    fn systemd_sync_conflict_behavior_surfaces_markers() {
        let upstream = tempfile::tempdir().expect("upstream tempdir");
        let upstream_root = upstream.path();
        run_ok(upstream_root, "git", &["init", "-b", "main"]);
        run_ok(
            upstream_root,
            "git",
            &["config", "user.name", "Upstream User"],
        );
        run_ok(
            upstream_root,
            "git",
            &["config", "user.email", "upstream@example.invalid"],
        );
        write(&upstream_root.join("meson.build"), "base\n");
        run_ok(upstream_root, "git", &["add", "."]);
        run_ok(upstream_root, "git", &["commit", "-m", "base"]);

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        run_ok(root, "git", &["init"]);
        run_ok(root, "git", &["config", "user.name", "MattOS User"]);
        run_ok(
            root,
            "git",
            &["config", "user.email", "mattos@example.invalid"],
        );
        write(&root.join("README.md"), "repo\n");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "init"]);

        let comp = ComponentDef {
            name: "systemd".to_string(),
            repo: upstream_root.to_string_lossy().to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/system/systemd".to_string(),
            sync: "copy".to_string(),
        };
        import_component(root, &comp, false).expect("initial import");
        run_ok(root, "git", &["add", "."]);
        run_ok(root, "git", &["commit", "-m", "import"]);

        write(&root.join("src/system/systemd/meson.build"), "local\n");
        run_ok(root, "git", &["add", "src/system/systemd/meson.build"]);
        run_ok(root, "git", &["commit", "-m", "local"]);

        write(&upstream_root.join("meson.build"), "upstream\n");
        run_ok(upstream_root, "git", &["add", "meson.build"]);
        run_ok(upstream_root, "git", &["commit", "-m", "upstream"]);

        let result = import_component(root, &comp, true);
        assert!(result.is_err());
        let merged =
            fs::read_to_string(root.join("src/system/systemd/meson.build")).expect("read merged");
        assert!(merged.contains("<<<<<<<"));
        assert!(merged.contains(">>>>>>>"));
    }

    #[test]
    fn build_plan_all_includes_uutils_stages() {
        let plan = build_plan(BuildStage::All);
        assert_eq!(plan[0], BuildStage::Kernel);
        assert_eq!(plan[1], BuildStage::Glibc);
        assert_eq!(plan[2], BuildStage::GccRuntime);
        assert!(plan.contains(&BuildStage::Grep));
        assert!(plan.contains(&BuildStage::Sed));
        assert!(plan.contains(&BuildStage::Findutils));
        assert!(plan.contains(&BuildStage::Diffutils));
        assert!(plan.contains(&BuildStage::Kmod));
        assert!(plan.contains(&BuildStage::Ncurses));
        assert!(plan.contains(&BuildStage::Procps));
        assert!(plan.contains(&BuildStage::Iproute2));
        assert!(plan.contains(&BuildStage::Iputils));
        assert!(plan.contains(&BuildStage::Curl));
        assert!(plan.contains(&BuildStage::Expat));
        assert!(plan.contains(&BuildStage::Libcap));
        assert!(plan.contains(&BuildStage::Attr));
        assert!(plan.contains(&BuildStage::Acl));
        assert!(plan.contains(&BuildStage::Zlib));
        assert!(plan.contains(&BuildStage::Bzip2));
        assert!(plan.contains(&BuildStage::Lz4));
        assert!(plan.contains(&BuildStage::Xz));
        assert!(plan.contains(&BuildStage::Xxhash));
        assert!(plan.contains(&BuildStage::Zstd));
        assert!(plan.contains(&BuildStage::Openssl));
        assert!(plan.contains(&BuildStage::Elfutils));
        assert!(plan.contains(&BuildStage::Pcre2));
        assert!(plan.contains(&BuildStage::Selinux));
        assert!(plan.contains(&BuildStage::Libxcrypt));
        assert!(plan.contains(&BuildStage::Libmd));
        assert!(plan.contains(&BuildStage::Libbsd));
        assert!(plan.contains(&BuildStage::Tar));
        let ncurses = plan
            .iter()
            .position(|stage| *stage == BuildStage::Ncurses)
            .unwrap();
        let procps = plan
            .iter()
            .position(|stage| *stage == BuildStage::Procps)
            .unwrap();
        let kmod = plan
            .iter()
            .position(|stage| *stage == BuildStage::Kmod)
            .unwrap();
        let systemd = plan
            .iter()
            .position(|stage| *stage == BuildStage::Systemd)
            .unwrap();
        let expat = plan
            .iter()
            .position(|stage| *stage == BuildStage::Expat)
            .unwrap();
        let libcap = plan
            .iter()
            .position(|stage| *stage == BuildStage::Libcap)
            .unwrap();
        let attr = plan
            .iter()
            .position(|stage| *stage == BuildStage::Attr)
            .unwrap();
        let acl = plan
            .iter()
            .position(|stage| *stage == BuildStage::Acl)
            .unwrap();
        let zlib = plan
            .iter()
            .position(|stage| *stage == BuildStage::Zlib)
            .unwrap();
        let bzip2 = plan
            .iter()
            .position(|stage| *stage == BuildStage::Bzip2)
            .unwrap();
        let lz4 = plan
            .iter()
            .position(|stage| *stage == BuildStage::Lz4)
            .unwrap();
        let xz = plan
            .iter()
            .position(|stage| *stage == BuildStage::Xz)
            .unwrap();
        let xxhash = plan
            .iter()
            .position(|stage| *stage == BuildStage::Xxhash)
            .unwrap();
        let zstd = plan
            .iter()
            .position(|stage| *stage == BuildStage::Zstd)
            .unwrap();
        let openssl = plan
            .iter()
            .position(|stage| *stage == BuildStage::Openssl)
            .unwrap();
        let elfutils = plan
            .iter()
            .position(|stage| *stage == BuildStage::Elfutils)
            .unwrap();
        let pcre2 = plan
            .iter()
            .position(|stage| *stage == BuildStage::Pcre2)
            .unwrap();
        let selinux = plan
            .iter()
            .position(|stage| *stage == BuildStage::Selinux)
            .unwrap();
        let libxcrypt = plan
            .iter()
            .position(|stage| *stage == BuildStage::Libxcrypt)
            .unwrap();
        let libmd = plan
            .iter()
            .position(|stage| *stage == BuildStage::Libmd)
            .unwrap();
        let libbsd = plan
            .iter()
            .position(|stage| *stage == BuildStage::Libbsd)
            .unwrap();
        let tar = plan
            .iter()
            .position(|stage| *stage == BuildStage::Tar)
            .unwrap();
        let dpkg = plan
            .iter()
            .position(|stage| *stage == BuildStage::Dpkg)
            .unwrap();
        let apt = plan
            .iter()
            .position(|stage| *stage == BuildStage::Apt)
            .unwrap();
        let dbus_broker = plan
            .iter()
            .position(|stage| *stage == BuildStage::DbusBroker)
            .unwrap();
        let iproute2 = plan
            .iter()
            .position(|stage| *stage == BuildStage::Iproute2)
            .unwrap();
        assert!(ncurses < procps);
        assert!(kmod < systemd);
        assert!(expat < dbus_broker);
        assert!(libcap < iproute2);
        assert!(attr < acl);
        assert!(acl < tar);
        assert!(zlib < dpkg && bzip2 < dpkg && tar < dpkg);
        assert!(xz < dpkg);
        assert!(zlib < apt && bzip2 < apt && lz4 < apt && xz < apt && xxhash < apt);
        assert!(zstd < dpkg && zstd < apt);
        assert!(zstd < openssl && zstd < elfutils);
        assert!(
            openssl
                < plan
                    .iter()
                    .position(|stage| *stage == BuildStage::Curl)
                    .unwrap()
        );
        assert!(openssl < apt);
        assert!(elfutils < iproute2);
        assert!(pcre2 < selinux && selinux < iproute2 && selinux < dpkg);
        assert!(
            libxcrypt
                < plan
                    .iter()
                    .position(|stage| *stage == BuildStage::Pam)
                    .unwrap()
        );
        assert!(
            libxcrypt
                < plan
                    .iter()
                    .position(|stage| *stage == BuildStage::Shadow)
                    .unwrap()
        );
        assert!(
            libmd < libbsd
                && libbsd
                    < plan
                        .iter()
                        .position(|stage| *stage == BuildStage::Shadow)
                        .unwrap()
        );
        assert!(libmd < dpkg);
        assert!(plan.contains(&BuildStage::Pam));
        assert!(plan.contains(&BuildStage::Shadow));
        assert!(plan.contains(&BuildStage::SudoRs));
        assert_eq!(plan.last().copied(), Some(BuildStage::Iso));
    }

    #[test]
    fn glibc_build_stage_and_upstream_metadata_are_pinned() {
        assert_eq!(
            BuildStage::from_str("glibc", true).unwrap(),
            BuildStage::Glibc
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let sources = read_sources(&root).expect("read MattOS upstream metadata");
        let glibc = sources
            .component
            .iter()
            .find(|component| component.name == "glibc")
            .expect("glibc source metadata");
        assert_eq!(glibc.repo, "git://sourceware.org/git/glibc.git");
        assert_eq!(glibc.branch, "glibc-2.43");
        assert_eq!(
            glibc.revision.as_deref(),
            Some("f762ccf84f122d1354f103a151cba8bde797d521")
        );
        assert_eq!(glibc.path, "src/system/libc/glibc");
        assert_eq!(glibc.sync, "copy");
        assert_eq!(GLIBC_MINIMUM_KERNEL, "5.10.0");
    }

    #[test]
    fn gcc_runtime_build_stage_and_upstream_metadata_are_pinned() {
        assert_eq!(
            BuildStage::from_str("gcc-runtime", true).unwrap(),
            BuildStage::GccRuntime
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let sources = read_sources(&root).expect("read MattOS upstream metadata");
        let gcc = sources
            .component
            .iter()
            .find(|component| component.name == "gcc")
            .expect("GCC source metadata");
        assert_eq!(gcc.repo, "https://gcc.gnu.org/git/gcc.git");
        assert_eq!(gcc.branch, "releases/gcc-15.3.0");
        assert_eq!(
            gcc.revision.as_deref(),
            Some("4db0e8df15bef836558857c291c323add11d035c")
        );
        assert_eq!(gcc.path, "src/toolchain/gcc");
        assert_eq!(gcc.sync, "copy");
        assert!(!root.join("src/toolchain/gcc/.git").exists());
    }

    #[test]
    fn native_toolchain_upstreams_and_stages_are_pinned() {
        assert_eq!(
            BuildStage::from_str("binutils", true).unwrap(),
            BuildStage::Binutils
        );
        assert_eq!(
            BuildStage::from_str("gcc-toolchain", true).unwrap(),
            BuildStage::GccToolchain
        );
        assert_eq!(
            BuildStage::from_str("make", true).unwrap(),
            BuildStage::Make
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let sources = read_sources(&root).expect("read MattOS upstream metadata");
        let component = |name| {
            sources
                .component
                .iter()
                .find(|component| component.name == name)
                .unwrap()
        };
        let binutils = component("binutils");
        assert_eq!(binutils.repo, "https://sourceware.org/git/binutils-gdb.git");
        assert_eq!(binutils.branch, "binutils-2_46_1");
        assert_eq!(
            binutils.revision.as_deref(),
            Some("5e56594815854de5eca35c7c04b11705d0f19c02")
        );
        assert_eq!(binutils.path, "src/toolchain/binutils");
        assert_eq!(binutils.sync, "copy");
        let make = component("make");
        assert_eq!(make.repo, "https://git.savannah.gnu.org/git/make.git");
        assert_eq!(make.branch, "4.4.1");
        assert_eq!(
            make.revision.as_deref(),
            Some("d66a65ad5a0e31b287f53930b0f09e31801f1613")
        );
        assert_eq!(make.path, "src/build-tools/make");
        assert_eq!(make.sync, "copy");
        assert!(!root.join("src/toolchain/binutils/.git").exists());
        assert!(!root.join("src/build-tools/make/.git").exists());
    }

    #[test]
    fn native_compiler_configuration_is_guest_default_and_minimal() {
        let source = include_str!("main.rs");
        let start = source.find("fn build_gcc_toolchain").unwrap();
        let end = source[start..].find("fn build_make").unwrap() + start;
        let build = &source[start..end];
        for required in [
            "--host=x86_64-pc-linux-gnu",
            "--target=x86_64-pc-linux-gnu",
            "--with-sysroot=/",
            "--with-build-sysroot=../mattos-sysroot",
            "--with-native-system-header-dir=/usr/include",
            "--with-as=/usr/bin/as",
            "--with-ld=/usr/bin/ld",
            "--enable-languages=c,c++",
            "--enable-default-pie",
            "--disable-multilib",
            "--disable-libsanitizer",
            "--disable-libgomp",
            "--disable-lto",
            "all-gcc",
            "install-gcc",
        ] {
            assert!(
                build.contains(required),
                "missing native GCC setting {required}"
            );
        }
        for forbidden in [
            "enable-languages=all",
            "install-target-libgfortran",
            "install-target-libgo",
        ] {
            assert!(
                !build.contains(forbidden),
                "unexpected compiler content {forbidden}"
            );
        }
    }

    #[test]
    fn gcc_runtime_configuration_is_target_only_and_sysrooted() {
        let source = include_str!("main.rs");
        let start = source.find("fn build_gcc_runtime").unwrap();
        let end = source[start..].find("const TOOLCHAIN_BUILD").unwrap() + start;
        let build = &source[start..end];
        for required in [
            "--with-sysroot=",
            "--with-build-sysroot=",
            "--enable-languages=c,c++",
            "--disable-multilib",
            "--disable-analyzer",
            "--disable-libsanitizer",
            "--disable-libquadmath",
            "--disable-libstdcxx-pch",
            "all-target-libgcc",
            "all-target-libstdc++-v3",
            "install-target-libgcc",
            "install-target-libstdc++-v3",
            "CFLAGS_FOR_TARGET",
            "CXXFLAGS_FOR_TARGET",
        ] {
            assert!(
                build.contains(required),
                "missing GCC runtime setting {required}"
            );
        }
        for forbidden in [
            "install-gcc",
            "install-g++",
            "-I/usr/include",
            "-L/usr/lib ",
        ] {
            assert!(
                !build.contains(forbidden),
                "forbidden GCC target install/input {forbidden}"
            );
        }
    }

    #[test]
    fn gcc_symbol_version_inventory_parser_covers_all_runtime_namespaces() {
        let temp = tempfile::tempdir().unwrap();
        write(
            &temp.path().join("runtime.c"),
            "int mattos_runtime(void) { return 0; }\n",
        );
        write(
            &temp.path().join("runtime.map"),
            "GCC_14.0.0 { global: mattos_runtime; };\nGLIBCXX_3.4.34 { } GCC_14.0.0;\nCXXABI_1.3.15 { } GLIBCXX_3.4.34;\n",
        );
        run_ok(
            temp.path(),
            "gcc",
            &[
                "-shared",
                "-fPIC",
                "runtime.c",
                "-Wl,--version-script=runtime.map",
                "-o",
                "runtime.so",
            ],
        );
        let versions = elf_version_names(
            &temp.path().join("runtime.so"),
            &["GCC_", "GLIBCXX_", "CXXABI_"],
        )
        .unwrap();
        for expected in ["GCC_14.0.0", "GLIBCXX_3.4.34", "CXXABI_1.3.15"] {
            assert!(versions.contains(expected));
        }
    }

    #[test]
    fn representative_consumers_include_cpp_and_rust_unwind_paths() {
        for consumer in [
            "usr/bin/apt",
            "usr/bin/apt-get",
            "usr/bin/dpkg",
            "usr/bin/curl",
            "usr/lib/systemd/systemd",
            "usr/bin/dbus-broker",
            "usr/bin/brush",
            "usr/bin/sudo",
            "usr/bin/login",
            "usr/libexec/mattos/rescue-init",
        ] {
            assert!(GCC_RUNTIME_REPRESENTATIVE_CONSUMERS.contains(&consumer));
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let workspace = fs::read_to_string(root.join("Cargo.toml")).unwrap();
        let rescue = fs::read_to_string(root.join("src/userland/init/Cargo.toml")).unwrap();
        assert!(!workspace.contains("panic = \"abort\""));
        assert!(!rescue.contains("panic = \"abort\""));
    }

    #[test]
    fn attr_sysroot_prerequisite_is_pinned() {
        assert_eq!(
            BuildStage::from_str("attr", true).unwrap(),
            BuildStage::Attr
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let sources = read_sources(&root).expect("read MattOS upstream metadata");
        let attr = sources
            .component
            .iter()
            .find(|component| component.name == "attr")
            .expect("attr source metadata");
        assert_eq!(attr.repo, "https://git.savannah.nongnu.org/git/attr.git");
        assert_eq!(attr.branch, "v2.6.0");
        assert_eq!(
            attr.revision.as_deref(),
            Some("c440855d6b33446edf4b5eb1a2d892281f15a99b")
        );
    }

    #[test]
    fn small_library_build_stage_names_dispatch() {
        assert_eq!(
            BuildStage::from_str("expat", true).unwrap(),
            BuildStage::Expat
        );
        assert_eq!(
            BuildStage::from_str("libcap", true).unwrap(),
            BuildStage::Libcap
        );
        assert_eq!(BuildStage::from_str("acl", true).unwrap(), BuildStage::Acl);
        assert_eq!(
            BuildStage::from_str("zlib", true).unwrap(),
            BuildStage::Zlib
        );
        assert_eq!(
            BuildStage::from_str("bzip2", true).unwrap(),
            BuildStage::Bzip2
        );
        assert_eq!(BuildStage::from_str("lz4", true).unwrap(), BuildStage::Lz4);
        assert_eq!(BuildStage::from_str("xz", true).unwrap(), BuildStage::Xz);
        assert_eq!(
            BuildStage::from_str("xxhash", true).unwrap(),
            BuildStage::Xxhash
        );
        assert_eq!(
            BuildStage::from_str("zstd", true).unwrap(),
            BuildStage::Zstd
        );
        assert_eq!(
            BuildStage::from_str("openssl", true).unwrap(),
            BuildStage::Openssl
        );
        assert_eq!(
            BuildStage::from_str("elfutils", true).unwrap(),
            BuildStage::Elfutils
        );
        assert_eq!(
            BuildStage::from_str("pcre2", true).unwrap(),
            BuildStage::Pcre2
        );
        assert_eq!(
            BuildStage::from_str("selinux", true).unwrap(),
            BuildStage::Selinux
        );
        assert_eq!(
            BuildStage::from_str("libxcrypt", true).unwrap(),
            BuildStage::Libxcrypt
        );
        assert_eq!(
            BuildStage::from_str("libmd", true).unwrap(),
            BuildStage::Libmd
        );
        assert_eq!(
            BuildStage::from_str("libbsd", true).unwrap(),
            BuildStage::Libbsd
        );
        assert_eq!(BuildStage::from_str("tar", true).unwrap(), BuildStage::Tar);
    }

    #[test]
    fn libxcrypt_preserves_yescrypt_and_required_compatibility_versions() {
        let options = libxcrypt_configure_options();
        assert!(options.contains(&"--enable-hashes=all"));
        assert!(options.contains(&"--enable-obsolete-api=glibc"));
        assert!(options.contains(&"--disable-xcrypt-compat-files"));
        assert_eq!(
            LIBXCRYPT_REQUIRED_SYMBOL_VERSIONS,
            ["GLIBC_2.2.5", "XCRYPT_2.0", "XCRYPT_4.3", "XCRYPT_4.4"]
        );
    }

    #[test]
    fn util_linux_mount_closure_keeps_selinux_compatibility_enabled() {
        let options = util_linux_meson_options();
        for required in [
            "-Dbuild-libblkid=enabled",
            "-Dbuild-libmount=enabled",
            "-Dbuild-libsmartcols=enabled",
            "-Dbuild-mount=enabled",
            "-Dselinux=enabled",
        ] {
            assert!(options.contains(&required.to_string()));
        }
    }

    #[test]
    fn openssl_runtime_configuration_is_minimal_and_explicit() {
        let zlib = Path::new("/mattos/zlib/usr");
        let zstd = Path::new("/mattos/zstd/usr");
        let options = openssl_configure_options(zlib, zstd);
        assert!(options.contains(&"shared".to_string()));
        assert!(options.contains(&"--openssldir=/etc/ssl".to_string()));
        assert_eq!(MATTOS_SOURCE_DATE_EPOCH, "1767225600");
        assert!(options.contains(&"no-module".to_string()));
        assert!(options.contains(&"no-legacy".to_string()));
        assert!(options.contains(&"enable-zstd".to_string()));
    }

    #[test]
    fn curl_preserves_mattos_ca_bundle_and_openssl_backend() {
        let options = curl_configure_options();
        assert!(options.contains(&"--with-openssl"));
        assert!(options.contains(&"--with-ca-bundle=/etc/ssl/certs/ca-certificates.crt"));
        assert!(options.contains(&"--without-ca-path"));
    }

    #[test]
    fn migrated_consumer_rejects_host_library_resolution() {
        let expected = tempfile::tempdir().expect("expected library directory");
        let error = validate_dependency_resolves_from(
            Path::new("/usr/bin/tar"),
            "libc.so.6",
            expected.path(),
            &[expected.path()],
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("unexpectedly resolves libc.so.6 from host path"));
    }

    #[test]
    fn systemd_build_enables_imported_pam_module() {
        let options = systemd_meson_options();
        assert!(options.iter().any(|option| option == "-Dpam=enabled"));
        assert!(options.iter().any(|option| option == "-Dselinux=enabled"));
        assert!(!options.iter().any(|option| option == "-Dpam=disabled"));
        assert_eq!(
            SYSTEMD_PAM_MODULE_REL,
            "usr/lib/x86_64-linux-gnu/security/pam_systemd.so"
        );
    }

    #[cfg(unix)]
    fn make_user_session_test_trees() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        let rootfs = tmp.path().join("rootfs");
        write(
            &repo.join("src/system/session/user-units/dbus.socket"),
            "[Socket]\nListenStream=%t/bus\nExecStartPost=-/usr/bin/systemctl --user set-environment DBUS_SESSION_BUS_ADDRESS=unix:path=%t/bus\n",
        );
        write(
            &repo.join("src/system/session/user-units/dbus-broker.service"),
            "[Service]\nExecStart=/usr/bin/dbus-broker-launch --scope user\n",
        );
        write(
            &repo.join("src/system/session/dbus/session.conf"),
            "<busconfig>\n<type>session</type>\n<auth>EXTERNAL</auth>\n<standard_session_servicedirs/>\n<allow own=\"*\"/>\n</busconfig>\n",
        );
        for (source, destination) in [
            (
                "src/system/session/user-units/dbus.socket",
                "usr/lib/systemd/user/dbus.socket",
            ),
            (
                "src/system/session/user-units/dbus-broker.service",
                "usr/lib/systemd/user/dbus-broker.service",
            ),
            (
                "src/system/session/dbus/session.conf",
                "usr/share/dbus-1/session.conf",
            ),
        ] {
            let body = fs::read_to_string(repo.join(source)).expect("packaged source");
            write(&rootfs.join(destination), &body);
        }
        for (stack, body) in [
            ("login", "session    optional     pam_systemd.so\n"),
            ("su-l", "session    optional     pam_systemd.so\n"),
            ("su", "session    required     pam_unix.so\n"),
            ("sudo", "session    required     pam_unix.so\n"),
            ("passwd", "password   required     pam_unix.so\n"),
            (
                "systemd-user",
                "account    required     pam_unix.so\nsession    required     pam_unix.so\nsession    optional     pam_systemd.so\n",
            ),
        ] {
            write(&rootfs.join("etc/pam.d").join(stack), body);
        }
        for rel in [
            "usr/lib/systemd/system/systemd-logind.service",
            "usr/lib/systemd/system/user@.service",
            "usr/lib/systemd/system/user-runtime-dir@.service",
            "usr/lib/systemd/user/basic.target",
            "usr/lib/systemd/user/default.target",
            "usr/lib/systemd/user/sockets.target",
            "usr/lib/systemd/user-environment-generators/30-systemd-environment-d-generator",
            "usr/lib/pam.d/systemd-user",
        ] {
            write(&rootfs.join(rel), "installed\n");
        }
        for rel in [
            SYSTEMD_PAM_MODULE_REL,
            "usr/lib/systemd/systemd-user-runtime-dir",
        ] {
            let destination = rootfs.join(rel);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).expect("runtime parent");
            }
            fs::copy("/bin/true", &destination).expect("test ELF");
            copy_runtime_dependencies(&destination, &rootfs).expect("test dependency closure");
        }
        fs::create_dir_all(rootfs.join("run")).expect("empty runtime root");
        (tmp, repo, rootfs)
    }

    #[cfg(unix)]
    #[test]
    fn user_session_installation_is_generic_complete_and_bus_scoped() {
        let (_tmp, repo, rootfs) = make_user_session_test_trees();
        install_user_session_configuration(&repo, &rootfs).expect("install user session");
        assert!(rootfs.join(SYSTEMD_PAM_MODULE_REL).is_file());
        assert!(
            rootfs
                .join("usr/lib/systemd/system/user@.service")
                .is_file()
        );
        assert!(
            rootfs
                .join("usr/lib/systemd/system/user-runtime-dir@.service")
                .is_file()
        );
        assert_eq!(
            fs::read_link(rootfs.join("usr/lib/systemd/user/dbus.service")).unwrap(),
            Path::new("dbus-broker.service")
        );
        assert_eq!(
            fs::read_link(rootfs.join("usr/lib/systemd/user/sockets.target.wants/dbus.socket"))
                .unwrap(),
            Path::new("../dbus.socket")
        );
        assert!(!path_entry_exists(&rootfs.join("run/user")));
        assert!(!path_entry_exists(
            &rootfs.join("usr/lib/pam.d/systemd-user")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn user_session_validation_rejects_inappropriate_pam_hook_and_stale_runtime() {
        let (_tmp, repo, rootfs) = make_user_session_test_trees();
        install_user_session_configuration(&repo, &rootfs).expect("install user session");
        write(
            &rootfs.join("etc/pam.d/sudo"),
            "session    optional     pam_systemd.so\n",
        );
        assert!(
            validate_user_session_configuration(&rootfs)
                .expect_err("sudo session hook must fail")
                .to_string()
                .contains("inappropriate PAM stack sudo")
        );
        write(
            &rootfs.join("etc/pam.d/sudo"),
            "session required pam_unix.so\n",
        );
        fs::create_dir_all(rootfs.join("run/user/4242")).expect("stale runtime directory");
        assert!(
            validate_user_session_configuration(&rootfs)
                .expect_err("stale runtime content must fail")
                .to_string()
                .contains("stale /run/user")
        );
    }

    #[test]
    fn account_database_validation_accepts_live_profile_shape() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write(
            &root.join("etc/passwd"),
            "root:x:0:0:root:/root:/bin/brush\nmattos:x:1000:1000:MattOS Live User:/home/mattos:/bin/brush\n",
        );
        write(
            &root.join("etc/group"),
            "root:x:0:\nsudo:x:27:mattos\nmattos:x:1000:\n",
        );
        write(&root.join("etc/shadow"), "root:!:::::::\nmattos:!:::::::\n");
        write(
            &root.join("etc/gshadow"),
            "root:!::\nsudo:!::mattos\nmattos:!::\n",
        );

        validate_account_database(root).expect("valid live account database should pass");
    }

    #[test]
    fn account_database_validation_rejects_duplicate_uid() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write(
            &root.join("etc/passwd"),
            "root:x:0:0:root:/root:/bin/brush\nmattos:x:0:1000:MattOS Live User:/home/mattos:/bin/brush\n",
        );
        write(
            &root.join("etc/group"),
            "root:x:0:\nsudo:x:27:mattos\nmattos:x:1000:\n",
        );
        write(&root.join("etc/shadow"), "root:!:::::::\nmattos:!:::::::\n");
        write(
            &root.join("etc/gshadow"),
            "root:!::\nsudo:!::mattos\nmattos:!::\n",
        );

        let result = validate_account_database(root);
        assert!(result.is_err());
    }

    #[test]
    #[cfg(unix)]
    fn enforce_auth_file_modes_sets_secure_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        for rel in [
            "etc/shadow",
            "etc/gshadow",
            "etc/passwd",
            "etc/group",
            "etc/sudoers",
            "etc/sudoers.d/00-mattos-live",
            "etc/sudoers.d/README",
            "usr/bin/login",
            "usr/bin/su",
            "usr/bin/passwd",
            "usr/bin/sudo",
        ] {
            write(&root.join(rel), "x\n");
        }
        fs::create_dir_all(root.join("root")).expect("root dir");
        fs::create_dir_all(root.join("home/mattos")).expect("home dir");

        enforce_auth_file_modes(root).expect("set modes");

        let sudo_mode = fs::metadata(root.join("usr/bin/sudo"))
            .expect("sudo metadata")
            .permissions()
            .mode()
            & 0o7777;
        assert_eq!(sudo_mode, 0o4755);

        let shadow_mode = fs::metadata(root.join("etc/shadow"))
            .expect("shadow metadata")
            .permissions()
            .mode()
            & 0o7777;
        assert_eq!(shadow_mode, 0o600);
        validate_auth_file_modes(root).expect("secure modes should validate");
    }

    #[test]
    #[cfg(unix)]
    fn auth_file_mode_validation_rejects_unsafe_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        for rel in [
            "etc/shadow",
            "etc/gshadow",
            "etc/passwd",
            "etc/group",
            "etc/sudoers",
            "etc/sudoers.d/00-mattos-live",
            "usr/bin/login",
            "usr/bin/su",
            "usr/bin/passwd",
            "usr/bin/sudo",
        ] {
            write(&root.join(rel), "x\n");
        }
        fs::create_dir_all(root.join("root")).expect("root dir");
        fs::create_dir_all(root.join("home/mattos")).expect("home dir");
        enforce_auth_file_modes(root).expect("set modes");

        fs::set_permissions(root.join("etc/shadow"), fs::Permissions::from_mode(0o644))
            .expect("make shadow unsafe");
        assert!(validate_auth_file_modes(root).is_err());
    }

    #[test]
    fn initramfs_owner_validation_rejects_non_root_ownership() {
        validate_initramfs_archive_owner("0:0").expect("root ownership should pass");
        assert!(validate_initramfs_archive_owner("1000:1000").is_err());
    }

    #[test]
    fn duplicate_command_detection_flags_conflicts() {
        let mut providers = BTreeMap::<&str, Vec<String>>::new();
        providers.insert(COREUTILS_PROVIDER, vec!["cat".to_string()]);
        providers.insert(GREP_PROVIDER, vec!["cat".to_string()]);
        let result = validate_no_duplicate_commands(&providers);
        assert!(result.is_err());
    }

    #[test]
    fn duplicate_command_detection_allows_unique_set() {
        let mut providers = BTreeMap::<&str, Vec<String>>::new();
        providers.insert(COREUTILS_PROVIDER, vec!["cat".to_string()]);
        providers.insert(GREP_PROVIDER, vec!["grep".to_string()]);
        validate_no_duplicate_commands(&providers).expect("unique set should pass");
    }

    #[test]
    fn install_userland_binary_reports_missing_executable() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let rootfs = root.join("rootfs");
        fs::create_dir_all(root.join("src/userland/grep/target/release")).expect("mkdir");

        let spec = BinaryInstallSpec {
            provider: GREP_PROVIDER,
            source_rel: "src/userland/grep/target/release/grep",
            install_name: "grep",
            command_name: "grep",
        };
        let result = install_userland_binary(root, &rootfs, &spec);
        assert!(result.is_err());
    }

    #[test]
    fn userland_inventory_writer_emits_sections() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let mut inventory = UserlandInventory::default();
        inventory.add_implemented(COREUTILS_PROVIDER, "cat");
        inventory.add_compiled(COREUTILS_PROVIDER, "cat");
        inventory.add_installed(COREUTILS_PROVIDER, "cat");
        inventory.add_excluded(DIFFUTILS_PROVIDER, "sdiff");
        inventory.add_failed(DIFFUTILS_PROVIDER, "diff3", "not implemented upstream");

        write_userland_inventory(root, &inventory).expect("write inventory");
        let body = fs::read_to_string(root.join(USERLAND_INVENTORY_PATH)).expect("read inventory");
        assert!(body.contains("[implemented_upstream]"));
        assert!(body.contains("uutils/coreutils:cat"));
        assert!(body.contains("[failed_compatibility]"));
    }

    #[test]
    fn read_sources_parses_uutils_component_set() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write(
            &root.join("upstream/sources.toml"),
            "[[component]]\nname='grep'\nrepo='https://github.com/uutils/grep.git'\nbranch='main'\npath='src/userland/grep'\nsync='copy'\n\n[[component]]\nname='sed'\nrepo='https://github.com/uutils/sed.git'\nbranch='main'\npath='src/userland/sed'\nsync='copy'\n",
        );
        let sources = read_sources(root).expect("read sources");
        assert_eq!(sources.component.len(), 2);
        assert_eq!(sources.component[0].name, "grep");
        assert_eq!(sources.component[1].name, "sed");
    }

    #[test]
    fn read_sources_parses_administration_components() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write(
            &root.join("upstream/sources.toml"),
            "[[component]]\nname='kmod'\nrepo='https://github.com/kmod-project/kmod.git'\nbranch='master'\npath='src/system/kmod'\nsync='copy'\n\n[[component]]\nname='procps-ng'\nrepo='https://gitlab.com/procps-ng/procps.git'\nbranch='master'\npath='src/userland/procps-ng'\nsync='copy'\n\n[[component]]\nname='ncurses'\nrepo='https://github.com/ThomasDickey/ncurses-snapshots.git'\nbranch='master'\npath='src/system/terminal/ncurses'\nsync='copy'\n",
        );
        let sources = read_sources(root).expect("read sources");
        assert_eq!(sources.component.len(), 3);
        assert_eq!(sources.component[0].path, "src/system/kmod");
        assert_eq!(sources.component[1].path, "src/userland/procps-ng");
        assert_eq!(sources.component[2].path, "src/system/terminal/ncurses");
        for component in sources.component {
            resolve_component_destination(root, &component.path).expect("safe component path");
        }
    }

    #[test]
    fn administration_build_stage_names_dispatch() {
        assert_eq!(
            BuildStage::from_str("kmod", true).unwrap(),
            BuildStage::Kmod
        );
        assert_eq!(
            BuildStage::from_str("procps", true).unwrap(),
            BuildStage::Procps
        );
        assert_eq!(
            BuildStage::from_str("ncurses", true).unwrap(),
            BuildStage::Ncurses
        );
    }

    #[test]
    fn read_sources_parses_networking_components_and_safe_destinations() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write(
            &root.join("upstream/sources.toml"),
            "[[component]]\nname='iproute2'\nrepo='https://git.kernel.org/pub/scm/network/iproute2/iproute2.git'\nbranch='main'\npath='src/userland/iproute2'\nsync='copy'\n\n[[component]]\nname='iputils'\nrepo='https://github.com/iputils/iputils.git'\nbranch='master'\npath='src/userland/iputils'\nsync='copy'\n\n[[component]]\nname='curl'\nrepo='https://github.com/curl/curl.git'\nbranch='master'\npath='src/userland/curl'\nsync='copy'\n",
        );
        let sources = read_sources(root).expect("read networking sources");
        assert_eq!(sources.component.len(), 3);
        for component in sources.component {
            let destination =
                resolve_component_destination(root, &component.path).expect("safe destination");
            assert!(destination.starts_with(root.join("src/userland")));
        }
    }

    #[test]
    fn networking_build_stage_names_dispatch() {
        assert_eq!(
            BuildStage::from_str("iproute2", true).unwrap(),
            BuildStage::Iproute2
        );
        assert_eq!(
            BuildStage::from_str("iputils", true).unwrap(),
            BuildStage::Iputils
        );
        assert_eq!(
            BuildStage::from_str("curl", true).unwrap(),
            BuildStage::Curl
        );
    }

    #[test]
    fn dbus_broker_upstream_metadata_has_safe_system_destination() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write(
            &root.join("upstream/sources.toml"),
            "[[component]]\nname='dbus-broker'\nrepo='https://github.com/bus1/dbus-broker.git'\nbranch='main'\npath='src/system/dbus/dbus-broker'\nsync='copy'\n",
        );
        let sources = read_sources(root).expect("read D-Bus source metadata");
        let component = &sources.component[0];
        assert_eq!(component.name, "dbus-broker");
        assert_eq!(component.repo, "https://github.com/bus1/dbus-broker.git");
        assert_eq!(component.branch, "main");
        assert_eq!(component.sync, "copy");
        let destination =
            resolve_component_destination(root, &component.path).expect("safe D-Bus destination");
        assert_eq!(destination, root.join("src/system/dbus/dbus-broker"));
    }

    #[test]
    fn dbus_broker_build_stage_name_dispatches() {
        assert_eq!(
            BuildStage::from_str("dbus-broker", true).unwrap(),
            BuildStage::DbusBroker
        );
        assert!(
            build_plan(BuildStage::All)
                .windows(2)
                .any(|pair| pair == [BuildStage::Systemd, BuildStage::DbusBroker])
        );
    }

    #[test]
    fn dbus_broker_manifest_requires_broker_and_launcher() {
        let manifest = COMPONENT_INSTALL_MANIFESTS
            .iter()
            .find(|manifest| manifest.provider == DBUS_BROKER_PROVIDER)
            .expect("dbus-broker install manifest");
        assert_eq!(manifest.install_root_rel, "out/build/dbus-broker/install");
        assert!(
            manifest
                .binaries
                .iter()
                .any(|binary| binary.command_name == "dbus-broker")
        );
        assert!(
            manifest
                .binaries
                .iter()
                .any(|binary| binary.command_name == "dbus-broker-launch")
        );
    }

    #[cfg(unix)]
    fn make_dbus_test_trees() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        let rootfs = tmp.path().join("rootfs");
        let source = repo.join("src/system/dbus");
        write(
            &source.join("config/system.conf"),
            "<busconfig>\n<user>messagebus</user>\n<deny own=\"*\"/>\n<deny send_type=\"method_call\"/>\n<includedir>/usr/share/dbus-1/system.d</includedir>\n<includedir>/etc/dbus-1/system.d</includedir>\n</busconfig>\n",
        );
        write(
            &source.join("config/dbus.conf"),
            "u! messagebus 195 \"D-Bus System Message Bus\"\n",
        );
        write(
            &source.join("units/dbus.socket"),
            "[Socket]\nListenStream=/run/dbus/system_bus_socket\nSocketMode=0666\n",
        );
        write(
            &source.join("units/dbus-broker.service"),
            "[Service]\nExecStart=/usr/bin/dbus-broker-launch --scope system --config-file=/etc/dbus-1/system.conf\n",
        );
        for (source_rel, destination_rel) in [
            ("config/system.conf", "etc/dbus-1/system.conf"),
            ("config/dbus.conf", "usr/lib/sysusers.d/dbus.conf"),
            ("units/dbus.socket", "usr/lib/systemd/system/dbus.socket"),
            (
                "units/dbus-broker.service",
                "usr/lib/systemd/system/dbus-broker.service",
            ),
        ] {
            let body = fs::read_to_string(source.join(source_rel)).expect("packaged D-Bus fixture");
            write(&rootfs.join(destination_rel), &body);
        }

        for target in [
            "systemd-networkd.service",
            "systemd-resolved.service",
            "systemd-timesyncd.service",
            "systemd-timedated.service",
            "systemd-logind.service",
        ] {
            write(
                &rootfs.join("usr/lib/systemd/system").join(target),
                "[Service]\nExecStart=/bin/true\n",
            );
        }
        for name in [
            "systemd1",
            "network1",
            "resolve1",
            "timesync1",
            "timedate1",
            "login1",
        ] {
            write(
                &rootfs.join(format!(
                    "usr/share/dbus-1/system.d/org.freedesktop.{name}.conf"
                )),
                "<busconfig/>\n",
            );
            write(
                &rootfs.join(format!(
                    "usr/share/dbus-1/system-services/org.freedesktop.{name}.service"
                )),
                &format!("[D-BUS Service]\nName=org.freedesktop.{name}\n"),
            );
        }
        fs::create_dir_all(rootfs.join("run")).expect("runtime staging directory");
        let roots = vec![PathBuf::from("/")];
        let libraries = vec![
            PathBuf::from("/lib/x86_64-linux-gnu"),
            PathBuf::from("/usr/lib/x86_64-linux-gnu"),
        ];
        for binary in ["dbus-broker", "dbus-broker-launch"] {
            inspect_and_stage_executable(
                Path::new("/bin/true"),
                &rootfs.join("usr/bin").join(binary),
                &rootfs,
                &roots,
                &libraries,
            )
            .expect("stage test ELF and dependency closure");
        }
        write(&rootfs.join("usr/bin/busctl"), "present\n");
        (tmp, repo, rootfs)
    }

    #[cfg(unix)]
    #[test]
    fn dbus_installation_has_units_socket_policy_paths_and_aliases() {
        let (_tmp, repo, rootfs) = make_dbus_test_trees();
        install_dbus_configuration(&repo, &rootfs).expect("install D-Bus integration");
        assert!(rootfs.join("etc/dbus-1/system.conf").is_file());
        assert!(rootfs.join("usr/lib/systemd/system/dbus.socket").is_file());
        assert_eq!(
            fs::read_link(rootfs.join("usr/lib/systemd/system/dbus.service")).unwrap(),
            Path::new("dbus-broker.service")
        );
        assert_eq!(
            fs::read_link(
                rootfs.join("usr/lib/systemd/system/dbus-org.freedesktop.network1.service")
            )
            .unwrap(),
            Path::new("systemd-networkd.service")
        );
        assert!(!path_entry_exists(
            &rootfs.join("run/dbus/system_bus_socket")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn dbus_alias_installation_rejects_missing_service() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let error = install_systemd_service_alias(
            tmp.path(),
            "dbus-org.example.service",
            "missing.service",
        )
        .expect_err("missing alias target must be rejected");
        assert!(
            error
                .to_string()
                .contains("target unit missing.service is missing")
        );
    }

    #[cfg(unix)]
    #[test]
    fn dbus_validation_rejects_competing_owner_and_stale_socket() {
        let (_tmp, repo, rootfs) = make_dbus_test_trees();
        install_dbus_configuration(&repo, &rootfs).expect("install D-Bus integration");
        write(&rootfs.join("usr/bin/dbus-daemon"), "competing daemon\n");
        assert!(
            validate_dbus_configuration(&rootfs)
                .expect_err("competing owner must fail")
                .to_string()
                .contains("competing dbus-daemon")
        );
        fs::remove_file(rootfs.join("usr/bin/dbus-daemon")).unwrap();
        write(&rootfs.join("run/dbus/system_bus_socket"), "stale\n");
        assert!(
            validate_dbus_configuration(&rootfs)
                .expect_err("stale socket must fail")
                .to_string()
                .contains("stale system-bus socket")
        );
    }

    #[cfg(unix)]
    #[test]
    fn dbus_runtime_dependency_closure_is_complete() {
        let (_tmp, repo, rootfs) = make_dbus_test_trees();
        install_dbus_configuration(&repo, &rootfs).expect("install D-Bus integration");
        validate_executable_runtime_closure(&rootfs.join("usr/bin/dbus-broker"), &rootfs)
            .expect("broker runtime closure");
        validate_executable_runtime_closure(&rootfs.join("usr/bin/dbus-broker-launch"), &rootfs)
            .expect("launcher runtime closure");
    }

    #[test]
    fn component_manifests_have_required_commands_and_unique_paths() {
        let mut commands = BTreeSet::new();
        let mut destinations = BTreeSet::new();
        for manifest in COMPONENT_INSTALL_MANIFESTS {
            for binary in manifest.binaries {
                assert!(
                    commands.insert(binary.command_name),
                    "duplicate command {}",
                    binary.command_name
                );
                assert!(
                    destinations.insert(binary.destination_rel),
                    "duplicate path {}",
                    binary.destination_rel
                );
                assert!(
                    binary.destination_rel.starts_with("usr/bin/")
                        || binary.destination_rel.starts_with("usr/sbin/")
                );
            }
        }
        for required in [
            "modprobe",
            "insmod",
            "rmmod",
            "lsmod",
            "modinfo",
            "depmod",
            "ps",
            "top",
            "free",
            "uptime",
            "pgrep",
            "pkill",
            "pidof",
            "watch",
            "sysctl",
            "vmstat",
            "w",
            "clear",
            "tput",
            "tic",
            "toe",
            "infocmp",
            "ip",
            "ss",
            "bridge",
            "tc",
            "ping",
            "tracepath",
            "curl",
            "dbus-broker",
            "dbus-broker-launch",
        ] {
            assert!(commands.contains(required), "missing {required}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn network_configuration_validation_covers_resolver_services_accounts_and_ca() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().expect("tempdir");
        let rootfs = tmp.path();
        write(
            &rootfs.join("etc/systemd/network/20-mattos-wired.network"),
            "[Match]\nType=ether\n[Network]\nDHCP=ipv4\n",
        );
        write(
            &rootfs.join("etc/systemd/resolved.conf"),
            "[Resolve]\nDNSStubListener=yes\n",
        );
        write(
            &rootfs.join("etc/systemd/timesyncd.conf"),
            "[Time]\nNTP=time.example\n",
        );
        write(
            &rootfs.join("etc/nsswitch.conf"),
            "passwd: files systemd\ngroup: files systemd\nshadow: files systemd\nhosts: files resolve dns\nnetworks: files dns\n",
        );
        write(
            &rootfs.join("etc/ssl/certs/ca-certificates.crt"),
            &"-----BEGIN CERTIFICATE-----\ncertificate\n-----END CERTIFICATE-----\n".repeat(2_000),
        );
        for rel in [
            "usr/lib/systemd/systemd-networkd",
            "usr/lib/systemd/systemd-resolved",
            "usr/lib/systemd/systemd-timesyncd",
            "usr/lib/x86_64-linux-gnu/libnss_resolve.so.2",
            "etc/systemd/system/multi-user.target.wants/systemd-networkd.service",
            "etc/systemd/system/multi-user.target.wants/systemd-resolved.service",
            "etc/systemd/system/multi-user.target.wants/systemd-timesyncd.service",
        ] {
            write(&rootfs.join(rel), "present\n");
        }
        fs::create_dir_all(rootfs.join("run/systemd/resolve")).expect("resolve runtime dir");
        fs::create_dir_all(rootfs.join("etc")).expect("etc dir");
        symlink(
            "/run/systemd/resolve/stub-resolv.conf",
            rootfs.join("etc/resolv.conf"),
        )
        .expect("resolv.conf symlink");
        write(
            &rootfs.join("etc/passwd"),
            "root:x:0:0:root:/root:/bin/brush\nmattos:x:1000:1000:MattOS:/home/mattos:/bin/brush\n",
        );
        write(&rootfs.join("etc/group"), "root:x:0:\nmattos:x:1000:\n");
        for (name, id) in [
            ("systemd-network", 192),
            ("systemd-resolve", 193),
            ("systemd-timesync", 194),
        ] {
            write(
                &rootfs
                    .join("usr/lib/sysusers.d")
                    .join(format!("{name}.conf")),
                &format!("u! {name} {id} \"service account\"\n"),
            );
        }
        validate_network_configuration(rootfs).expect("valid network configuration");
    }

    #[test]
    fn terminfo_validation_requires_every_selected_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        for terminal in TERMINFO_ENTRIES {
            let first = terminal.chars().next().unwrap().to_string();
            write(
                &tmp.path().join(first).join(terminal),
                "compiled terminfo\n",
            );
        }
        verify_terminfo_entries(tmp.path()).expect("complete terminfo set");
        fs::remove_file(tmp.path().join("l/linux")).expect("remove linux entry");
        assert!(verify_terminfo_entries(tmp.path()).is_err());
    }

    #[test]
    fn local_runtime_dependency_maps_to_rootfs_usr_lib() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let install = tmp.path().join("install");
        let rootfs = tmp.path().join("rootfs");
        let library = install.join("usr/lib/x86_64-linux-gnu/libexample.so.1");
        write(&library, "library\n");
        stage_resolved_dependency(&library, &rootfs, &[install]).expect("stage local dependency");
        assert!(
            rootfs
                .join("usr/lib/x86_64-linux-gnu/libexample.so.1")
                .exists()
        );
        assert!(!rootfs.join("home").exists());
    }
}
