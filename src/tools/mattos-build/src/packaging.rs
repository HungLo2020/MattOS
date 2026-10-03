use super::*;
use clap::Subcommand;
use filetime::{FileTime, set_file_times, set_symlink_file_times};

mod cache;
pub(crate) use cache::{
    PackageCacheInput, PackageCacheManifest, ensure_package_facts, ensure_package_set,
    explain_package_cache, invalidate_package_cache, invalidate_package_facts, package_cache_input,
    package_cache_manifest_path, package_facts_status, print_package_cache_status,
    validate_package_cache, write_package_set_manifest,
};

mod audit;
#[cfg(test)]
pub(crate) use audit::{
    bootstrap_consumers, bootstrap_source_attribution, confirmed_host_package,
    validate_migrated_bootstrap_absent,
};
pub(crate) use audit::{
    detect_staging_collisions, generate_bootstrap_audit, runtime_libraries_for_spec,
    validate_no_mutable_package_state, validate_staged_runtime_ownership,
};

mod builder_image;
mod recipe_digest;
mod snapshot_version;
mod staging;
#[cfg(test)]
pub(crate) use cache::{
    PACKAGE_SET_SCHEMA_VERSION, PackageSetManifest, package_definition_digest,
    package_payload_source_digests, package_recipe_revision, package_set_policy,
    package_stage_dependency_digest,
};
#[cfg(test)]
pub(crate) use staging::{
    GLIBC_RUNTIME_LIBRARIES, imported_soname_library_licenses, copy_path_preserving, copy_preserving, stage_brush,
    stage_ca_certificates, stage_cargo, stage_flatpak_system_remote, stage_gcc_development,
    stage_flatpak_closure_package, stage_iso_codes, stage_rustc, stage_wireless_regdb,
    stage_xdg_desktop_portal,
    validate_no_mutable_system_state, validate_vulkan_icd_manifests,
};
pub(crate) use staging::{
    apply_live_apt_policy, component_install, copy_tree_preserving, stage_package,
    validate_udev_hwdb_payload,
};

mod registry;
pub(crate) use builder_image::build_builder_image;
#[cfg(test)]
pub(crate) use registry::package_install_order_for;
pub(crate) use registry::{PACKAGE_NAMES, PackageSpec, package_install_order, package_specs};
pub(crate) use registry::{live_excluded_packages, live_package_install_order, live_package_names};
#[cfg(test)]
pub(crate) use registry::{MATTOS_TOOLCHAIN_META_PACKAGE, MATTOS_TOOLCHAIN_PACKAGES};

const ARCH: &str = "amd64";
/// The Debian revision prefix: versions are `<upstream>-1mattos<N>`, with N
/// from `src/system/packages/revisions.toml` (1 when no entry applies).
const REVISION: &str = "1mattos";
const REVISION_LEDGER: &str = "src/system/packages/revisions.toml";
const SOURCE_DATE_EPOCH: i64 = 1_767_225_600; // 2026-01-01T00:00:00Z
const DPKG_UPSTREAM_COMMIT: &str = "ff7e9d8bf01379e8b022028a65afaa262e2c25cd";
const DPKG_UPSTREAM_REPOSITORY: &str = "https://git.dpkg.org/git/dpkg/dpkg.git";

struct DpkgMissingSourceInput {
    path: &'static str,
    sha256: &'static str,
}

const DPKG_MISSING_SOURCE_INPUTS: &[DpkgMissingSourceInput] = &[
    DpkgMissingSourceInput {
        path: "dselect/completion/bash/dselect",
        sha256: "c5c26193b15bff4ce6ee3174641d21d39f6e6841396312cb12341b0c2eee638f",
    },
    DpkgMissingSourceInput {
        path: "scripts/completion/bash/dpkg-source",
        sha256: "e76a4b7bfa74cc6cce48dce8345ed132fe0425182507ebc4c80ac1b3c3ffa00d",
    },
    DpkgMissingSourceInput {
        path: "src/completion/bash/dpkg",
        sha256: "2e7512d98773e7f94977a77e2b23bfa15b4a32afacddf62cf4e9c25c88ee6cbc",
    },
    DpkgMissingSourceInput {
        path: "src/completion/bash/dpkg-deb",
        sha256: "d45a9508926145befcafe789c5d2b4977bbaba33502e025a18f30a05e990423b",
    },
    DpkgMissingSourceInput {
        path: "src/completion/bash/dpkg-query",
        sha256: "c31450e165abe23c54ff8a97c39f844d193ffb722e78657a42ffce8dbf65604d",
    },
    DpkgMissingSourceInput {
        path: "utils/completion/bash/start-stop-daemon",
        sha256: "aafbbf3024eec97187898791c408fcbbf5ffad629cd81566b606347ef1270f87",
    },
    DpkgMissingSourceInput {
        path: "utils/completion/bash/update-alternatives",
        sha256: "322de52d50d91ef0cf447e74c2e6cd0719ce645a22980d0fa07666acc6a874e1",
    },
];

const DPKG_RUNTIME_PATHS: &[&str] = &[
    "usr/bin/dpkg",
    "usr/bin/dpkg-deb",
    "usr/bin/dpkg-divert",
    "usr/bin/dpkg-query",
    "usr/bin/dpkg-realpath",
    "usr/bin/dpkg-split",
    "usr/bin/dpkg-statoverride",
    "usr/bin/dpkg-trigger",
    "usr/bin/update-alternatives",
    "usr/sbin/start-stop-daemon",
];
const APT_RUNTIME_PATHS: &[&str] = &[
    "usr/bin/apt",
    "usr/bin/apt-cache",
    "usr/bin/apt-config",
    "usr/bin/apt-get",
    "usr/bin/apt-mark",
    "usr/lib/apt/apt-helper",
    "usr/lib/apt/methods/copy",
    "usr/lib/apt/methods/file",
    "usr/lib/apt/methods/gpgv",
    "usr/lib/apt/methods/http",
    "usr/lib/apt/methods/https",
    "usr/lib/apt/methods/store",
];
const APT_CONFFILES: &[&str] = &[
    "/etc/apt/apt.conf.d/01mattos",
    "/etc/apt/sources.list.d/00-mattos-local.sources",
    "/etc/apt/sources.list.d/mattos-hosted.sources",
    "/etc/apt/preferences.d/00mattos-priority",
];
const PAM_MODULES: &[&str] = &[
    "pam_unix.so",
    "pam_limits.so",
    "pam_env.so",
    "pam_nologin.so",
    "pam_rootok.so",
    "pam_permit.so",
    "pam_deny.so",
    "pam_shells.so",
    "pam_securetty.so",
];
const KMOD_RUNTIME_PATHS: &[&str] = &[
    "usr/bin/kmod",
    "usr/sbin/modprobe",
    "usr/sbin/insmod",
    "usr/sbin/rmmod",
    "usr/sbin/lsmod",
    "usr/sbin/modinfo",
    "usr/sbin/depmod",
];
const PROCPS_RUNTIME_PATHS: &[&str] = &[
    "usr/bin/ps",
    "usr/bin/top",
    "usr/bin/free",
    "usr/bin/uptime",
    "usr/bin/pgrep",
    "usr/bin/pkill",
    "usr/bin/pidof",
    "usr/bin/watch",
    "usr/sbin/sysctl",
    "usr/bin/vmstat",
    "usr/bin/w",
    "usr/bin/pmap",
    "usr/bin/pwdx",
    "usr/bin/tload",
    "usr/bin/slabtop",
    "usr/bin/hugetop",
];
const NCURSES_RUNTIME_PATHS: &[&str] = &[
    "usr/bin/clear",
    "usr/bin/tput",
    "usr/bin/tic",
    "usr/bin/toe",
    "usr/bin/infocmp",
];
const SHADOW_RUNTIME_PATHS: &[&str] = &[
    "usr/bin/passwd",
    "usr/sbin/useradd",
    "usr/sbin/usermod",
    "usr/sbin/userdel",
    "usr/sbin/groupadd",
    "usr/sbin/groupmod",
    "usr/sbin/groupdel",
    "usr/sbin/chpasswd",
    "usr/bin/chage",
    "usr/bin/newgrp",
];
const UTIL_LINUX_AUTH_PATHS: &[&str] = &[
    "usr/sbin/agetty",
    "usr/sbin/sulogin",
    "usr/bin/login",
    "usr/bin/su",
];
const UTIL_LINUX_BASE_PATHS: &[&str] = &[
    "usr/bin/lsblk",
    "usr/bin/dmesg",
    "usr/sbin/fdisk",
    "usr/sbin/cfdisk",
    "usr/sbin/sfdisk",
    "usr/sbin/wipefs",
    "usr/sbin/blkid",
    "usr/bin/findmnt",
    "usr/sbin/losetup",
    "usr/bin/mountpoint",
    "usr/sbin/blockdev",
    "usr/bin/flock",
    "usr/bin/lscpu",
    "usr/bin/lslocks",
    "usr/bin/lsns",
    "usr/bin/nsenter",
    "usr/bin/unshare",
    "usr/bin/taskset",
    "usr/bin/chrt",
    "usr/bin/ionice",
    "usr/bin/prlimit",
    "usr/bin/uuidgen",
];
const IPROUTE2_RUNTIME_PATHS: &[&str] = &[
    "usr/sbin/ip",
    "usr/sbin/ss",
    "usr/sbin/bridge",
    "usr/sbin/tc",
];
const IPUTILS_RUNTIME_PATHS: &[&str] = &["usr/bin/ping", "usr/bin/tracepath"];
const OPENSSH_SERVER_RUNTIME_PATHS: &[&str] = &[
    "usr/sbin/sshd",
    "usr/lib/openssh/sshd-session",
    "usr/lib/openssh/sshd-auth",
    "usr/lib/openssh/sftp-server",
    "usr/lib/openssh/ssh-keysign",
];
const UDEV_HWDB_SOURCE_REL: &str = "usr/lib/udev/hwdb.d";
const UDEV_HWDB_BINARY_REL: &str = "usr/lib/udev/hwdb.bin";
const UDEV_HWDB_UNIT_REL: &str = "usr/lib/systemd/system/systemd-hwdb-update.service";
const UDEV_HWDB_WANTS_REL: &str =
    "usr/lib/systemd/system/sysinit.target.wants/systemd-hwdb-update.service";
const UDEV_HWDB_TEST_MODALIAS: &str = "pci:v00008086d0000100Esv00001AF4sd00001100bc02sc00i00";
#[cfg(test)]
const MIGRATED_BOOTSTRAP_SONAME_PREFIXES: &[&str] = &[
    "libc.so",
    "libm.so",
    "ld-linux-",
    "libexpat.so",
    "libcap.so",
    "libattr.so",
    "libacl.so",
    "libz.so",
    "libbz2.so",
    "liblz4.so",
    "liblzma.so",
    "libxxhash.so",
    "libmd.so",
    "libbsd.so",
    "libcrypto.so",
    "libssl.so",
    "libelf.so",
    "libzstd.so",
    "libpcre2-8.so",
    "libselinux.so",
    "libcrypt.so",
    "libgcc_s.so",
    "libstdc++.so",
];

#[derive(Subcommand, Debug)]
pub(crate) enum PackageCommands {
    Build {
        #[arg(long, conflicts_with = "package")]
        all: bool,
        package: Option<String>,
    },
    Repo,
    Inspect {
        package: String,
    },
    Audit,
    Status,
    CompatibilityAudit,
    PublishPlan {
        #[arg(required = true)]
        artifacts: Vec<PathBuf>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PackageInventory {
    package: Vec<PackageInventoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PackageInventoryEntry {
    name: String,
    version: String,
    architecture: String,
    artifact_path: String,
    source_component: String,
    dependencies: Vec<String>,
    runtime_libraries: Vec<String>,
    file_count: u64,
    sha256: String,
}

const PACKAGE_CACHE_SCHEMA_VERSION: u32 = 1;
const PACKAGE_AUDIT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
struct PreparedPackage {
    spec: PackageSpec,
    version: String,
    staging: PathBuf,
    artifact: PathBuf,
    input: PackageCacheInput,
    reused: Option<PackageCacheManifest>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PackageAuditManifest {
    schema_version: u32,
    input_digest: String,
    package_count: usize,
    policy: String,
}

#[derive(Debug, Deserialize)]
struct DebianCompatibilityManifest {
    schema_version: u32,
    suite: String,
    architecture: String,
    policy: String,
    version_policy: String,
    package: Vec<DebianCompatibilityPackage>,
}

#[derive(Debug, Deserialize)]
struct DebianCompatibilityPackage {
    debian_name: String,
    mattos_name: String,
    source_component: String,
    owned_paths: Vec<String>,
    provided_abi_or_commands: Vec<String>,
    current_mattos_version: String,
    expected_debian_role: String,
    classification: String,
    known_gaps: Vec<String>,
    #[serde(default)]
    debian_epoch: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct LinuxScriptsPolicy {
    schema_version: u32,
    component: String,
    authoritative_path: String,
    sha256: String,
    policy: String,
    forbidden_nested_entry: String,
}

#[derive(Serialize)]
struct Provenance<'a> {
    package: &'a str,
    version: &'a str,
    architecture: &'a str,
    mattos_source_path: &'a str,
    upstream_repository: &'a str,
    upstream_commit: &'a str,
    build_configuration: &'a str,
    runtime_libraries: &'a [String],
}

#[derive(Debug, Serialize, Deserialize)]
struct BootstrapAuditReport {
    schema_version: u32,
    package: String,
    snapshot: String,
    entry_count: u64,
    payload_bytes: u64,
    classification_totals: BTreeMap<String, u64>,
    entries: Vec<BootstrapAuditEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BootstrapAuditEntry {
    path: String,
    file_type: String,
    size: u64,
    mode: String,
    symlink_target: Option<String>,
    sha256: String,
    file_description: String,
    elf_type: Option<String>,
    elf_interpreter: Option<String>,
    soname: Option<String>,
    dt_needed: Vec<String>,
    objdump_needed: Vec<String>,
    ldd_resolved: Vec<String>,
    confirmed_host_package: Option<String>,
    upstream_project: Option<String>,
    source_attribution: String,
    source_already_exists_in_mattos: bool,
    consumers: Vec<BootstrapConsumer>,
    reason_in_bootstrap_runtime: String,
    recommended_future_package: String,
    migration_difficulty: String,
    attribution_confidence: String,
    classification: String,
    boundary_group: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct BootstrapConsumer {
    package: String,
    path: String,
}

pub(crate) fn run_package_command(repo_root: &Path, command: PackageCommands) -> Result<()> {
    match command {
        PackageCommands::Build { all, package } => {
            if all {
                build_all_packages(repo_root)?;
            } else if let Some(name) = package {
                build_packages(repo_root, &[name])?;
            } else {
                bail!("package build requires a package name or --all")
            }
            Ok(())
        }
        PackageCommands::Repo => generate_repository(repo_root),
        PackageCommands::Inspect { package } => inspect_package(repo_root, &package),
        PackageCommands::Audit => generate_bootstrap_audit(repo_root),
        PackageCommands::Status => print_inventory(repo_root),
        PackageCommands::CompatibilityAudit => validate_debian_compatibility(repo_root),
        PackageCommands::PublishPlan { artifacts } => print_publish_plan(repo_root, &artifacts),
    }
}

fn validate_debian_compatibility(repo_root: &Path) -> Result<()> {
    let manifest_path = repo_root.join("src/system/packages/debian-compat/trixie.toml");
    let manifest: DebianCompatibilityManifest =
        toml::from_str(&fs::read_to_string(&manifest_path)?)
            .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    if manifest.schema_version != 1
        || manifest.suite != "trixie"
        || manifest.architecture != ARCH
        || manifest.policy.trim().is_empty()
        || manifest.version_policy.trim().is_empty()
    {
        bail!("Debian compatibility manifest header is invalid")
    }
    let expected: BTreeSet<&str> = PACKAGE_NAMES.iter().copied().collect();
    let actual: BTreeSet<&str> = manifest
        .package
        .iter()
        .map(|package| package.mattos_name.as_str())
        .collect();
    if actual != expected || manifest.package.len() != PACKAGE_NAMES.len() {
        bail!("Debian compatibility manifest does not map the complete package inventory")
    }
    for package in &manifest.package {
        validate_package_name(&package.debian_name)?;
        validate_package_name(&package.mattos_name)?;
        validate_debian_version(&package.current_mattos_version)?;
        if package.debian_epoch == Some(0) {
            bail!(
                "compatibility entry {} has an invalid zero Debian epoch",
                package.mattos_name
            )
        }
        if package.source_component.trim().is_empty()
            || package.owned_paths.is_empty()
            || package.provided_abi_or_commands.is_empty()
            || package.expected_debian_role.trim().is_empty()
            || package.known_gaps.is_empty()
        {
            bail!("compatibility entry {} is incomplete", package.mattos_name)
        }
        match package.classification.as_str() {
            "mattos-specific" if package.mattos_name.starts_with("mattos-") => {}
            "debian-compatible" if package.debian_name == package.mattos_name => {}
            "mattos-extension" if package.debian_name == package.mattos_name => {}
            "mattos-alternative" => {}
            _ => bail!("invalid package classification for {}", package.mattos_name),
        }
    }

    validate_apt_compatibility_policy(repo_root)?;
    validate_linuxscripts_upstream(repo_root)?;
    println!(
        "validated Debian {} {} compatibility policy for {} packages",
        manifest.suite,
        manifest.architecture,
        manifest.package.len()
    );
    Ok(())
}

/// MattOS installs only from its own repositories: the embedded media
/// repository and the signed hosted one.  No other archive is configured.
const APT_SOURCE_FILES: &[&str] = &["00-mattos-local.sources", "mattos-hosted.sources"];

fn validate_apt_compatibility_policy(repo_root: &Path) -> Result<()> {
    let config = repo_root.join("src/system/packages/config/apt");
    let preferences = fs::read_to_string(config.join("00mattos-priority"))?;
    for required in [
        "Pin: release o=MattOS,l=MattOS Local,n=trixie\nPin-Priority: 990",
        "Pin: release o=MattOS,l=MattOS,n=trixie\nPin-Priority: 990",
    ] {
        if !preferences.contains(required) {
            bail!("APT preferences lack required policy stanza: {required}")
        }
    }
    for (label, directory) in [("live", config.clone()), ("installed", config.join("installed"))] {
        let mut sources = fs::read_dir(&directory)?
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect::<std::io::Result<Vec<_>>>()?;
        sources.retain(|name| name.ends_with(".sources") || name.ends_with(".list"));
        sources.sort();
        if sources != APT_SOURCE_FILES {
            bail!("{label} APT sources must be exactly {APT_SOURCE_FILES:?}, found {sources:?}")
        }
        let preferences = fs::read_to_string(directory.join("00mattos-priority"))?;
        if preferences.lines().any(|line| line.starts_with("Pin:") && !line.contains("o=MattOS")) {
            bail!("{label} APT preferences pin a non-MattOS origin")
        }
    }
    let local = fs::read_to_string(config.join("00-mattos-local.sources"))?;
    if !local.contains("URIs: file:/usr/share/mattos/repository")
        || !local.contains("Suites: trixie")
        || !local.contains("Trusted: yes")
    {
        bail!("embedded MattOS source is invalid")
    }
    let hosted = fs::read_to_string(config.join("mattos-hosted.sources"))?;
    if !hosted.contains("URIs: https://packages.mattsherfey.com")
        || !hosted.contains("Suites: trixie")
        || !hosted.contains("Enabled: yes")
        || hosted.contains("Trusted: yes")
    {
        bail!("hosted MattOS source scaffold is invalid")
    }
    let installed = config.join("installed");
    let installed_local = fs::read_to_string(installed.join("00-mattos-local.sources"))?;
    let installed_hosted = fs::read_to_string(installed.join("mattos-hosted.sources"))?;
    let installed_preferences = fs::read_to_string(installed.join("00mattos-priority"))?;
    let installed_conf = fs::read_to_string(installed.join("01mattos"))?;
    if !installed_local.contains("Enabled: yes")
        || !installed_local.contains("URIs: file:/usr/share/mattos/repository")
        || !installed_local.contains("Trusted: yes")
        || !installed_hosted.contains("Enabled: yes")
        || !installed_hosted.contains("URIs: https://packages.mattsherfey.com")
        || !installed_hosted.contains("Signed-By: /usr/share/keyrings/mattos-archive-keyring.asc")
        || !installed_conf.contains("Acquire::https::Verify-Peer \"true\";")
        || !installed_conf.contains("Acquire::https::Verify-Host \"true\";")
        || !installed_conf.contains("Acquire::AllowInsecureRepositories \"false\";")
        || !installed_preferences
            .contains("Pin: release o=MattOS,l=MattOS Local,n=trixie\nPin-Priority: 990")
        || !installed_preferences
            .contains("Pin: release o=MattOS,l=MattOS,n=trixie\nPin-Priority: 990")
        || installed_preferences.contains("Pin-Priority: 1001")
    {
        bail!("installed APT policy is invalid")
    }
    if !config.join("keys/mattos-archive-keyring.asc").is_file() {
        bail!("APT keyring source is missing: mattos-archive-keyring.asc")
    }
    Ok(())
}

fn validate_linuxscripts_upstream(repo_root: &Path) -> Result<PathBuf> {
    let policy_path = repo_root.join("upstream/policies/linuxscripts.toml");
    let policy: LinuxScriptsPolicy = toml::from_str(&fs::read_to_string(&policy_path)?)
        .with_context(|| format!("failed to parse {}", policy_path.display()))?;
    if policy.schema_version != 1
        || policy.component != "linuxscripts"
        || policy.policy.trim().is_empty()
        || policy.forbidden_nested_entry != ".git"
    {
        bail!("LinuxScripts read-only policy is invalid")
    }
    let state = read_sync_state(repo_root, "linuxscripts")?
        .ok_or_else(|| anyhow!("LinuxScripts upstream state is missing"))?;
    if state.repo != "https://github.com/HungLo2020/LinuxScripts.git"
        || state.branch != "master"
        || state.destination_path != "src/infrastructure/LinuxScripts"
        || state.sync_method != "copy"
    {
        bail!("LinuxScripts upstream state does not match the approved source")
    }
    let component_root = repo_root.join(&state.destination_path);
    let authoritative = repo_root.join(&policy.authoritative_path);
    if !authoritative.is_file() || !authoritative.starts_with(&component_root) {
        bail!("authoritative repository publisher is missing or outside LinuxScripts")
    }
    let actual = sha256_file(&authoritative)?;
    if actual != policy.sha256 {
        bail!(
            "authoritative LinuxScripts publisher changed locally: expected {}, got {actual}; update upstream and sync instead",
            policy.sha256
        )
    }
    walk_tree(&component_root, &mut |path, _| {
        if path.file_name() == Some(OsStr::new(&policy.forbidden_nested_entry)) {
            bail!(
                "nested Git metadata is forbidden in imported LinuxScripts: {}",
                path.display()
            )
        }
        Ok(())
    })?;
    Ok(authoritative)
}

fn print_publish_plan(repo_root: &Path, artifacts: &[PathBuf]) -> Result<()> {
    let publisher = validate_linuxscripts_upstream(repo_root)?;
    let inventory = read_inventory(repo_root)?;
    let approved_root = repo_root
        .join("out/packages")
        .canonicalize()
        .with_context(|| {
            format!(
                "package output directory is missing at {}",
                repo_root.join("out/packages").display()
            )
        })?;
    let mut approved = Vec::new();
    for supplied in artifacts {
        let candidate = if supplied.is_absolute() {
            supplied.clone()
        } else {
            repo_root.join(supplied)
        };
        let canonical = validate_publication_artifact_location(&approved_root, &candidate)?;
        let relative = relative_display(repo_root, &canonical)?;
        let entry = inventory
            .package
            .iter()
            .find(|entry| entry.artifact_path == relative)
            .ok_or_else(|| {
                anyhow!("artifact is not approved by out/packages/inventory.toml: {relative}")
            })?;
        if sha256_file(&canonical)? != entry.sha256 {
            bail!("artifact checksum differs from approved inventory: {relative}")
        }
        approved.push(canonical);
    }
    approved.sort();
    approved.dedup();
    if approved.len() != artifacts.len() {
        bail!("duplicate publication artifacts are not allowed")
    }
    let command = format_publish_command(&publisher, &approved)?;
    println!("validated non-publishing command (not executed):\n{command}");
    Ok(())
}

fn format_publish_command(publisher: &Path, approved: &[PathBuf]) -> Result<String> {
    Ok(std::iter::once("python3".to_string())
        .chain(std::iter::once(shell_escape(path_str(publisher)?)))
        .chain([
            "--repo".to_string(),
            "mattos".to_string(),
            "upload".to_string(),
        ])
        .chain(
            approved
                .iter()
                .map(|path| shell_escape(path_str(path).unwrap())),
        )
        .collect::<Vec<_>>()
        .join(" "))
}

fn validate_publication_artifact_location(
    approved_root: &Path,
    candidate: &Path,
) -> Result<PathBuf> {
    let canonical = candidate
        .canonicalize()
        .with_context(|| format!("publication artifact is missing: {}", candidate.display()))?;
    if !canonical.starts_with(approved_root)
        || canonical.extension().and_then(OsStr::to_str) != Some("deb")
    {
        bail!(
            "publication artifacts must be .deb files inside out/packages: {}",
            candidate.display()
        )
    }
    Ok(canonical)
}

pub(crate) fn build_all_packages(repo_root: &Path) -> Result<()> {
    validate_debian_compatibility(repo_root)?;
    remove_path_if_exists(&repo_root.join("out/packages/staging/mattos-bootstrap-runtime"))?;
    if let Ok(mut inventory) = read_inventory(repo_root) {
        inventory
            .package
            .retain(|entry| PACKAGE_NAMES.contains(&entry.name.as_str()));
        write_inventory(repo_root, &inventory)?;
    }
    build_packages(
        repo_root,
        &PACKAGE_NAMES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
    )
}

/// Validates one package's cache entry, staging it again on a miss.
fn prepare_package(
    repo_root: &Path,
    spec: PackageSpec,
    version: String,
    staging: PathBuf,
    artifact: PathBuf,
    input: PackageCacheInput,
) -> Result<PreparedPackage> {
    let reused = performance::measure_package_validation(|| {
        validate_package_cache(repo_root, &spec, &version, &staging, &artifact, &input)
    })
    .ok();
    if reused.is_some() {
        performance::timed(
            &format!("package:{}", spec.name),
            "hit",
            "package key, staging inventory, control metadata, and artifact SHA-256 matched",
            &input.cache_key,
            || Ok(()),
        )?;
        println!("package cache hit: {}", spec.name);
    } else {
        performance::invalidate_integrity_paths(repo_root, &[staging.clone(), artifact.clone()]);
        performance::timed(
            &format!("package-staging:{}", spec.name),
            "miss",
            "package inputs or cached artifact validation changed",
            &input.cache_key,
            || stage_package(repo_root, &spec),
        )?;
        println!("package cache miss: {}", spec.name);
    }
    Ok(PreparedPackage {
        spec,
        version,
        staging,
        artifact,
        input,
        reused,
    })
}

/// Maps `items` on up to `workers` threads, returning results in input order
/// (so inventories stay deterministic) or the first error encountered.
fn parallel_in_order<T: Send, R: Send>(
    items: Vec<T>,
    workers: usize,
    action: impl Fn(T) -> Result<R> + Sync,
) -> Result<Vec<R>> {
    let count = items.len();
    let queue = std::sync::Mutex::new(items.into_iter().enumerate());
    let results = std::sync::Mutex::new((0..count).map(|_| None).collect::<Vec<Option<R>>>());
    let failed = std::sync::atomic::AtomicBool::new(false);
    let errors = std::thread::scope(|scope| {
        let handles = (0..workers.clamp(1, count.max(1)))
            .map(|_| {
                scope.spawn(|| -> Result<()> {
                    while !failed.load(std::sync::atomic::Ordering::Relaxed) {
                        let Some((position, item)) = queue.lock().expect("work queue poisoned").next() else {
                            break;
                        };
                        match action(item) {
                            Ok(result) => results.lock().expect("results poisoned")[position] = Some(result),
                            Err(error) => {
                                failed.store(true, std::sync::atomic::Ordering::Relaxed);
                                return Err(error);
                            }
                        }
                    }
                    Ok(())
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("package worker panicked"))
            .filter_map(Result::err)
            .collect::<Vec<_>>()
    });
    if let Some(error) = errors.into_iter().next() {
        return Err(error);
    }
    Ok(results
        .into_inner()
        .expect("results poisoned")
        .into_iter()
        .map(|result| result.expect("every item produced a result"))
        .collect())
}

fn build_packages(repo_root: &Path, names: &[String]) -> Result<()> {
    // An invalid revision ledger would mis-version every package it names.
    validate_revision_ledger(repo_root)?;
    let specs = package_specs();
    let mut selected = Vec::new();
    for name in names {
        validate_package_name(name)?;
        let spec = specs
            .iter()
            .find(|spec| spec.name == *name)
            .ok_or_else(|| anyhow!("unknown MattOS package {name}"))?;
        selected.push(spec.clone());
    }

    let staging_root = repo_root.join("out/packages/staging");
    let artifact_root = repo_root.join("out/packages/amd64");
    fs::create_dir_all(&staging_root)?;
    fs::create_dir_all(&artifact_root)?;
    // Versions and cache keys share a memo of source digests; compute them
    // first.  Validating and staging each package is independent and
    // disk-bound, so those run on parallel workers.
    let mut source_digests = BTreeMap::new();
    let mut keyed = Vec::new();
    for spec in &selected {
        let version = package_version(repo_root, spec)?;
        let staging = staging_root.join(spec.name);
        let artifact = artifact_root.join(format!("{}_{}_{}.deb", spec.name, version, ARCH));
        let input = package_cache_input(repo_root, spec, &version, &mut source_digests)?;
        keyed.push((spec.clone(), version, staging, artifact, input));
    }
    let stage_log = performance::current_stage_log();
    let prepared = parallel_in_order(keyed, package_build_workers(), |(spec, version, staging, artifact, input)| {
        performance::with_inherited_stage_log(stage_log.clone(), || {
            prepare_package(repo_root, spec, version, staging, artifact, input)
        })
    })?;
    // Check the complete prototype set only for a full package build. A
    // targeted package build must not unexpectedly rescan every existing
    // staging tree merely because those trees happen to be present; the full
    // build retains the complete collision/runtime audit.
    let full_selection = PACKAGE_NAMES
        .iter()
        .all(|name| names.iter().any(|selected| selected == name));
    let collision_specs: Vec<PackageSpec> = if full_selection
        && PACKAGE_NAMES
            .iter()
            .all(|name| staging_root.join(name).is_dir())
    {
        specs.clone()
    } else {
        selected.clone()
    };
    let audit_input = performance::digest_value(&(
        PACKAGE_AUDIT_SCHEMA_VERSION,
        prepared
            .iter()
            .map(|package| (&package.spec.name, &package.input.cache_key))
            .collect::<Vec<_>>(),
        "collision-soname-dependency-compatibility-v1",
    ))?;
    let audit_path = repo_root.join("out/state/audits/package-global.json");
    let audit_reusable = fs::read(&audit_path)
        .ok()
        .and_then(|body| serde_json::from_slice::<PackageAuditManifest>(&body).ok())
        .is_some_and(|manifest| {
            manifest.schema_version == PACKAGE_AUDIT_SCHEMA_VERSION
                && manifest.input_digest == audit_input
                && manifest.package_count == collision_specs.len()
                && manifest.policy == "collision-soname-dependency-compatibility-v1"
        });
    if audit_reusable {
        performance::timed(
            "package-audits",
            "hit",
            "all package fact keys and the global validation policy matched",
            &audit_input,
            || Ok(()),
        )?;
    } else {
        performance::timed(
            "package-audits",
            "miss",
            "package fact graph or global validation policy changed",
            &audit_input,
            || {
                detect_staging_collisions(&staging_root, &collision_specs)?;
                if collision_specs.len() == PACKAGE_NAMES.len() {
                    validate_staged_runtime_ownership(repo_root, &collision_specs)?;
                }
                Ok(())
            },
        )?;
        performance::atomic_write_json(
            &audit_path,
            &PackageAuditManifest {
                schema_version: PACKAGE_AUDIT_SCHEMA_VERSION,
                input_digest: audit_input,
                package_count: collision_specs.len(),
                policy: "collision-soname-dependency-compatibility-v1".into(),
            },
        )?;
    }

    let mut inventory = read_inventory(repo_root).unwrap_or(PackageInventory {
        package: Vec::new(),
    });
    // Each changed package is independent: stage-normalize, compress with
    // zstd -19, verify, and record its manifest in parallel.  Results are
    // merged in the original order so the inventory stays deterministic.
    let mut entries: Vec<Option<PackageInventoryEntry>> = Vec::with_capacity(prepared.len());
    let mut pending = Vec::new();
    for (position, package) in prepared.into_iter().enumerate() {
        if let Some(cached) = package.reused {
            entries.push(Some(cached.inventory_entry));
        } else {
            entries.push(None);
            pending.push((position, package));
        }
    }
    let built = parallel_in_order(pending, package_build_workers(), |(position, package)| {
        performance::with_inherited_stage_log(stage_log.clone(), || {
            build_package_artifact(repo_root, package).map(|entry| (position, entry))
        })
    })?;
    for (position, entry) in built {
        entries[position] = Some(entry);
    }
    for entry in entries {
        let entry = entry.expect("every package produced an inventory entry");
        inventory.package.retain(|old| old.name != entry.name);
        inventory.package.push(entry);
    }
    inventory.package.sort_by(|a, b| a.name.cmp(&b.name));
    write_inventory(repo_root, &inventory)?;
    ensure_package_facts(repo_root, &inventory)?;
    print_inventory(repo_root)?;
    if let Some(warning) = stale_compatibility_versions(repo_root, &inventory)? {
        eprint!("{warning}");
    }
    if let Some(warning) = validate_revision_ledger(repo_root)? {
        eprint!("{warning}");
    }
    write_package_set_manifest(repo_root)
}

/// Compatibility entries whose recorded `current_mattos_version` differs from
/// the version just built.  The field documents what MattOS ships and is
/// maintained by hand, so report drift on every build instead of letting it
/// rot silently.
fn stale_compatibility_versions(repo_root: &Path, inventory: &PackageInventory) -> Result<Option<String>> {
    let manifest_path = repo_root.join("src/system/packages/debian-compat/trixie.toml");
    let manifest: DebianCompatibilityManifest = toml::from_str(&fs::read_to_string(&manifest_path)?)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let built = inventory
        .package
        .iter()
        .map(|entry| (entry.name.as_str(), entry.version.as_str()))
        .collect::<BTreeMap<_, _>>();
    let stale = manifest
        .package
        .iter()
        .filter_map(|package| {
            let version = *built.get(package.mattos_name.as_str())?;
            (package.current_mattos_version != version).then(|| {
                format!("  {}: records {}, built {version}", package.mattos_name, package.current_mattos_version)
            })
        })
        .collect::<Vec<_>>();
    Ok((!stale.is_empty()).then(|| {
        format!(
            "warning: {} entr{} in src/system/packages/debian-compat/trixie.toml record an outdated current_mattos_version:\n{}\n",
            stale.len(),
            if stale.len() == 1 { "y" } else { "ies" },
            stale.join("\n")
        )
    }))
}

fn package_stage_dependencies(source_component: &str) -> &'static [&'static str] {
    match source_component {
        "MattOS" | "ca-certificates" | "test" => &[],
        "mattos-profiles" => &["systemd", "init"],
        "mattos-compat" => &["systemd", "rust"],
        "linux-uapi" => &["linux-headers"],
        "kernel-modules" => &["linux"],
        "gcc" => &["gcc-runtime", "gcc-compiler"],
        "glibc" => &["glibc", "formal-sysroot"],
        "make" => &["make"],
        // The installer package embeds both installer-stage assets and the
        // Linux bzImage used by installed systems. Keep the filesystem-tool
        // packages tied only to their shared build stage.
        "installer" => &["installer", "linux"],
        "greetd" => &["greetd"],
        "plasma-login-manager" => &["plasma-login-manager"],
        "aurorae" => &["aurorae"],
        "kscreenlocker" => &["kscreenlocker"],
        "mattos-plasma-live" => &[],
        "mattos-plasma-theme" => &["material-cursors"],
        "btrfs-progs" | "dosfstools" => &["installer"],
        "e2fsprogs" => &["e2fsprogs"],
        "procps-ng" => &["procps-ng"],
        "linux-pam" => &["linux-pam"],
        "sudo-rs" => &["sudo-rs"],
        other => match other {
            "brush" => &["brush"],
            "coreutils" => &["coreutils"],
            "grep" => &["grep"],
            "findutils" => &["findutils"],
            "diffutils" => &["diffutils"],
            "binutils" => &["binutils"],
            "apt" => &["apt"],
            "dpkg" => &["dpkg"],
            "libgpg-error" => &["libgpg-error"],
            "libgcrypt" => &["libgcrypt"],
            "libassuan" => &["libassuan"],
            "libksba" => &["libksba"],
            "npth" => &["npth"],
            "gnupg" => &["gpgv"],
            "systemd" => &["systemd"],
            "dbus-broker" => &["dbus-broker"],
            "dbus" => &["dbus"],
            "dav1d" => &["dav1d"],
            "glib" => &["glib"],
            "pipewire" => &["pipewire"],
            "ffmpeg" => &["ffmpeg"],
            "libva" => &["libva"],
            "opencv" => &["opencv"],
            "util-linux" => &["util-linux"],
            "iproute2" => &["iproute2"],
            "iputils" => &["iputils"],
            "gzip" => &["gzip"],
            "patch" => &["patch"],
            "sed" => &["sed"],
            "dash" => &["dash"],
            "mawk" => &["mawk"],
            "rsync" => &["rsync"],
            "pkgconf" => &["pkgconf"],
            "cmake" => &["cmake"],
            "wayland-protocols" => &["wayland-protocols"],
            "perl" => &["perl"],
            "m4" => &["m4"],
            "autoconf" => &["autoconf"],
            "automake" => &["automake"],
            "libtool" => &["libtool"],
            "ninja" => &["ninja"],
            // Meson is staged straight from its vendored Python source.
            "meson" => &[],
            "file" => &["file"],
            "less" => &["less"],
            "git" => &["git"],
            "openssh" => &["openssh"],
            "libffi" => &["libffi"],
            "icu" => &["icu"],
            "freetype" => &["freetype"],
            "fontconfig" => &["fontconfig"],
            "pop-fonts" => &["pop-fonts"],
            "kdeclarative" => &["kdeclarative"],
            "kirigami-addons" => &["kirigami-addons"],
            "kquickcharts" => &["kquickcharts"],
            "qtbase" => &["qtbase"],
            "qtshadertools" => &["qtshadertools"],
            "qtdeclarative" => &["qtdeclarative"],
            "qtsvg" => &["qtsvg"],
            "qtwayland" => &["qtwayland"],
            "qttools" => &["qttools"],
            "qtmultimedia" => &["qtmultimedia"],
            "qtspeech" => &["qtspeech"],
            "qt5compat" => &["qt5compat"],
            "qca" => &["qca"],
            "qcoro" => &["qcoro"],
            "kwin" => &["kwin"],
            "plasma-framework" => &["plasma-framework"],
            "krunner" => &["krunner"],
            "plasma-activities" => &["plasma-activities"],
            "plasma-activities-stats" => &["plasma-activities-stats"],
            "plasma5support" => &["plasma5support"],
            "ksysguard" => &["ksysguard"],
            "knewstuff" => &["knewstuff"],
            "attica" => &["attica"],
            "sonnet" => &["sonnet"],
            "plasma-workspace" => &["plasma-workspace"],
            "kscreenlocker" => &["kscreenlocker"],
            "plasma-desktop" => &["plasma-desktop"],
            "plasma-login-manager" => &["plasma-login-manager"],
            "breeze" => &["breeze"],
            "breeze-icons" => &["breeze-icons"],
            "lm-sensors" => &["lm-sensors"],
            "highway" => &["highway"],
            "kfilemetadata" => &["kfilemetadata"],
            "kpty" => &["kpty"],
            "networkmanager-qt" => &["networkmanager-qt"],
            "purpose" => &["purpose"],
            "milou" => &["milou"],
            "systemsettings" => &["systemsettings"],
            "ksystemstats" => &["ksystemstats"],
            "plasma-systemmonitor" => &["plasma-systemmonitor"],
            "polkit-kde-agent-1" => &["polkit-kde-agent-1"],
            "kquickimageeditor" => &["kquickimageeditor"],
            "kpipewire" => &["kpipewire"],
            "spectacle" => &["spectacle"],
            "pulseaudio-qt" => &["pulseaudio-qt"],
            "plasma-pa" => &["plasma-pa"],
            "plasma-nm" => &["plasma-nm"],
            "powerdevil" => &["powerdevil"],
            "xdg-desktop-portal-kde" => &["xdg-desktop-portal-kde"],
            "dolphin" => &["dolphin"],
            "konsole" => &["konsole"],
            "kate" => &["kate"],
            "ark" => &["ark"],
            "packagekit-qt" => &["packagekit-qt"],
            "haruna" => &["haruna"],
            "mpvqt" => &["mpvqt"],
            "discover" => &["discover"],
            "gwenview" => &["gwenview"],
            "kimageannotator" => &["kimageannotator"],
            "kcolorpicker" => &["kcolorpicker"],
            "elisa" => &["elisa"],
            "partitionmanager" => &["partitionmanager"],
            "kwalletmanager" => &["kwalletmanager"],
            "kcalc" => &["kcalc"],
            "kparts" => &["kparts"],
            "ktextwidgets" => &["ktextwidgets"],
            "ktexteditor" => &["ktexteditor"],
            "qrencode" => &["qrencode"],
            "zxing-cpp" => &["zxing-cpp"],
            "prison" => &["prison"],
            "libkscreen" => &["libkscreen"],
            "modemmanager" => &["modemmanager"],
            "modemmanager-qt" => &["modemmanager-qt"],
            "libarchive" => &["libarchive"],
            "libsndfile" => &["libsndfile"],
            "pulseaudio" => &["pulseaudio"],
            "libgudev" => &["libgudev"],
            "gmp" => &["gmp"],
            "mpfr" => &["mpfr"],
            "mpc" => &["mpc"],
            "nasm" => &["nasm"],
            "packagekit" => &["packagekit"],
            "sqlite" => &["sqlite"],
            "jansson" => &["jansson"],
            "kdsingleapplication" => &["kdsingleapplication"],
            "mpv" => &["mpv"],
            "libplacebo" => &["libplacebo"],
            "libass" => &["libass"],
            "harfbuzz" => &["harfbuzz"],
            "fribidi" => &["fribidi"],
            "exiv2" => &["exiv2"],
            "libjpeg-turbo" => &["libjpeg-turbo"],
            "libxrandr" => &["libxrandr"],
            "libbytesize" => &["libbytesize"],
            "keyutils" => &["keyutils"],
            "libnvme" => &["libnvme"],
            "popt" => &["popt"],
            "json-c" => &["json-c"],
            "lvm2" => &["lvm2"],
            "cryptsetup" => &["cryptsetup"],
            "libblockdev" => &["libblockdev"],
            "wireplumber" => &["wireplumber"],
            "upower" => &["upower"],
            "udisks2" => &["udisks2"],
            "bluez" => &["bluez"],
            "power-profiles-daemon" => &["power-profiles-daemon"],
            "kcoreaddons" => &["kcoreaddons"],
            "ki18n" => &["ki18n"],
            "kwidgetsaddons" => &["kwidgetsaddons"],
            "kconfig" => &["kconfig"],
            "kcmutils" => &["kcmutils"],
            "kdbusaddons" => &["kdbusaddons"],
            "kcrash" => &["kcrash"],
            "kwindowsystem" => &["kwindowsystem"],
            "kpackage" => &["kpackage"],
            "karchive" => &["karchive"],
            "kio" => &["kio"],
            "kunitconversion" => &["kunitconversion"],
            "ksvg" => &["ksvg"],
            "knotifications" => &["knotifications"],
            "knotifyconfig" => &["knotifyconfig"],
            "kguiaddons" => &["kguiaddons"],
            "kitemmodels" => &["kitemmodels"],
            "kglobalaccel" => &["kglobalaccel"],
            "kiconthemes" => &["kiconthemes"],
            "kcolorscheme" => &["kcolorscheme"],
            "kjobwidgets" => &["kjobwidgets"],
            "kcompletion" => &["kcompletion"],
            "kservice" => &["kservice"],
            "kauth" => &["kauth"],
            "polkit-qt-1" => &["polkit-qt-1"],
            "yaml-cpp" => &["yaml-cpp"],
            "kpmcore" => &["kpmcore"],
            "calamares" => &["calamares"],
            "wayland" => &["wayland"],
            "xkbcommon" => &["xkbcommon"],
            "xkeyboard-config" => &[],
            "iso-codes" => &[],
            "seatd" => &["seatd"],
            "libdisplay-info" => &["libdisplay-info"],
            "libevdev" => &["libevdev"],
            "libinput" => &["libinput"],
            "pixman" => &["pixman"],
            "libdrm" => &["libdrm"],
            "x11-compat" => &["x11-compat"],
            "libglvnd" => &["libglvnd"],
            "vulkan-loader" => &["vulkan-headers", "vulkan-loader"],
            "vulkan-tools" => &["vulkan-tools"],
            "mesa" => &["mesa"],
            "nvidia-driver" => &["nvidia-driver"],
            // Flatpak's runtime closure is packaged separately
            // (`FLATPAK_CLOSURE_PACKAGES`); each package keys on its own stage.
            "flatpak" => &["flatpak"],
            "xwayland" => &[
                "xwayland",
                "libepoxy",
                "libfontenc",
                "libxfont",
                "libxcvt",
                "libxshmfence",
                "libxkbfile",
                "xkbcomp",
            ],
            "xdg-desktop-portal" => &["xdg-desktop-portal"],
            "gstreamer" => &["gstreamer"],
            "gstreamer-base" => &["gstreamer-base"],
            "polkit" => &["polkit"],
            "networkmanager" => &["networkmanager"],
            "libnl" => &["libnl"],
            "wpa-supplicant" => &["wpa-supplicant"],
            "grub" => &["grub"],
            "cozy" => &["cozy"],
            "cpython" => &["cpython"],
            "llvm" => &["llvm"],
            "rust" => &["rust"],
            "ncurses" => &["ncurses"],
            "kmod" => &["kmod"],
            "shadow" => &["shadow"],
            "curl" => &["curl"],
            "tar" => &["tar"],
            "expat" => &["expat"],
            "libcap" => &["libcap"],
            "attr" => &["attr"],
            "acl" => &["acl"],
            "zlib" => &["zlib"],
            "bzip2" => &["bzip2"],
            "lz4" => &["lz4"],
            "xz" => &["xz"],
            "xxhash" => &["xxhash"],
            "zstd" => &["zstd"],
            "nghttp2" => &["nghttp2"],
            "openssl" => &["openssl"],
            "elfutils" => &["elfutils"],
            "pcre2" => &["pcre2"],
            "selinux" => &["selinux"],
            "libxcrypt" => &["libxcrypt"],
            "libmd" => &["libmd"],
            "libbsd" => &["libbsd"],
            _ => &[],
        },
    }
}

fn package_source_roots(source_component: &str) -> &'static [&'static str] {
    match source_component {
        "mattos-profiles" => &[
            "src/system/packages/profiles",
            "src/system/units",
            "src/system/network",
            "src/rootfs/skeleton",
            "src/userland/init",
        ],
        "mattos-plasma-live" => &[
            "src/system/session/plasma",
        ],
        "mattos-plasma-theme" => &[
            "src/system/desktop/branding/MattOS",
            "src/desktop/themes/nordic-kde/widgets",
            "src/desktop/themes/nordic-kde/icons",
            "src/desktop/themes/nordic-kde/dialogs",
            "src/desktop/themes/nordic-kde/metadata.desktop",
            "src/desktop/themes/nordic-kde/colors",
            "src/desktop/themes/nordic-kde/LICENSE",
            "src/desktop/themes/papirus-icon-theme/Papirus-Dark",
            "src/desktop/themes/papirus-icon-theme/Papirus",
            "src/desktop/themes/papirus-icon-theme/LICENSE",
            "src/desktop/themes/material-cursors/src/material_light_cursors",
            "src/desktop/themes/material-cursors/src/config",
            "src/desktop/themes/material-cursors/src/cursorList",
            "src/desktop/themes/material-cursors/LICENSE",
            "src/desktop/themes/material-cursors/build.sh",
            "src/build-tools/xcursorgen",
            "src/desktop/themes/utterly-round-aurorae/Utterly-Round-Dark",
        ],
        "mattos-compat" => &["src/system/compat/mattos-compat"],
        "MattOS" => &[
            "src/rootfs/skeleton",
            "src/system/packages/config",
        ],
        "ca-certificates" => &["src/system/network"],
        "linux-uapi" => &["src/kernel/linux-uapi"],
        "kernel-modules" => &["src/kernel/linux", "src/kernel/config"],
        "glibc" => &["src/system/libc/glibc"],
        "gcc" => &["src/toolchain/gcc"],
        "binutils" => &["src/toolchain/binutils"],
        "make" => &["src/build-tools/make"],
        "installer" => &[
            "src/system/installer",
            "src/system/storage/btrfs-progs",
            "src/system/storage/dosfstools",
            "src/system/storage/e2fsprogs",
        ],
        "greetd" => &["src/system/session/greetd"],
        "btrfs-progs" => &["src/system/storage/btrfs-progs"],
        "dosfstools" => &["src/system/storage/dosfstools"],
        "e2fsprogs" => &["src/system/storage/e2fsprogs"],
        "libsndfile" => &["src/system/multimedia/libsndfile"],
        "pulseaudio" => &["src/system/multimedia/pulseaudio"],
        "libgudev" => &["src/system/libraries/libgudev"],
        "gmp" => &["src/system/libraries/gmp"],
        "mpfr" => &["src/system/libraries/mpfr"],
        "mpc" => &["src/system/libraries/mpc"],
        "libbytesize" => &["src/system/libraries/libbytesize"],
        "keyutils" => &["src/system/security/keyutils"],
        "libnvme" => &["src/system/libraries/libnvme"],
        "popt" => &["src/system/libraries/popt"],
        "json-c" => &["src/system/libraries/json-c"],
        "lvm2" => &["src/system/storage/lvm2"],
        "cryptsetup" => &["src/system/storage/cryptsetup"],
        "libblockdev" => &["src/system/libraries/libblockdev"],
        "wireplumber" => &["src/system/multimedia/wireplumber"],
        "upower" => &["src/system/services/upower"],
        "udisks2" => &["src/system/services/udisks2"],
        "bluez" => &["src/system/services/bluez"],
        "power-profiles-daemon" => &["src/system/services/power-profiles-daemon"],
        "brush" => &["src/userland/brush"],
        "coreutils" => &["src/userland/coreutils"],
        "grep" => &["src/userland/grep"],
        "findutils" => &["src/userland/findutils"],
        "diffutils" => &["src/userland/diffutils"],
        "curl" => &["src/userland/curl"],
        "libmd" => &["src/system/libraries/libmd"],
        "libbsd" => &["src/system/libraries/libbsd"],
        "zstd" => &["src/system/libraries/zstd"],
        "nghttp2" => &["src/system/libraries/nghttp2"],
        "openssl" => &["src/system/libraries/openssl"],
        "elfutils" => &["src/system/libraries/elfutils"],
        "pcre2" => &["src/system/libraries/pcre2"],
        "selinux" => &["src/system/security/selinux"],
        "libxcrypt" => &["src/system/libraries/libxcrypt"],
        "util-linux" => &["src/userland/util-linux"],
        "dpkg" => &["src/system/packages/dpkg"],
        "apt" => &["src/system/packages/apt"],
        "libgpg-error" => &["src/system/security/libgpg-error"],
        "libgcrypt" => &["src/system/security/libgcrypt"],
        "libassuan" => &["src/system/security/libassuan"],
        "libksba" => &["src/system/security/libksba"],
        "npth" => &["src/system/security/npth"],
        "gnupg" => &["src/system/security/gnupg"],
        "ncurses" => &["src/system/terminal/ncurses"],
        "kmod" => &["src/system/kmod"],
        "procps-ng" => &["src/userland/procps-ng"],
        "systemd" => &["src/system/systemd"],
        "iso-codes" => &["src/system/data/iso-codes"],
        "expat" => &["src/system/libraries/expat/expat"],
        "libcap" => &["src/system/libraries/libcap"],
        "attr" => &["src/system/libraries/attr"],
        "acl" => &["src/system/libraries/acl"],
        "zlib" => &["src/system/libraries/zlib"],
        "bzip2" => &["src/system/libraries/bzip2"],
        "lz4" => &["src/system/libraries/lz4"],
        "xz" => &["src/system/libraries/xz"],
        "xxhash" => &["src/system/libraries/xxhash"],
        "tar" => &["src/userland/tar"],
        "dbus-broker" => &["src/system/dbus/dbus-broker"],
        "dbus" => &["src/system/dbus/dbus"],
        "dav1d" => &["src/system/multimedia/dav1d"],
        "glib" => &["src/system/libraries/glib"],
        "pipewire" => &[
            "src/system/multimedia/pipewire",
        ],
        "ffmpeg" => &["src/system/multimedia/ffmpeg"],
        "libva" => &["src/system/graphics/libva"],
        "opencv" => &["src/system/multimedia/opencv"],
        "linux-pam" => &["src/system/auth/linux-pam"],
        "shadow" => &["src/system/auth/shadow"],
        "sudo-rs" => &["src/system/auth/sudo-rs"],
        "iproute2" => &["src/userland/iproute2"],
        "iputils" => &["src/userland/iputils"],
        "gzip" => &["src/userland/gzip"],
        "patch" => &["src/userland/patch"],
        "sed" => &["src/userland/sed", "upstream/policies/release-archives.toml"],
        "dash" => &["src/userland/dash", "upstream/policies/release-archives.toml"],
        "mawk" => &["src/userland/mawk"],
        "rsync" => &["src/userland/rsync", "upstream/policies/release-archives.toml"],
        "pkgconf" => &["src/build-tools/pkgconf"],
        "cmake" => &["src/build-tools/cmake"],
        "wayland-protocols" => &["src/graphics/wayland-protocols"],
        "perl" => &["src/development/perl"],
        "m4" => &["src/build-tools/m4", "upstream/policies/release-archives.toml"],
        "autoconf" => &["src/build-tools/autoconf", "upstream/policies/release-archives.toml"],
        "automake" => &["src/build-tools/automake", "upstream/policies/release-archives.toml"],
        "libtool" => &["src/build-tools/libtool", "upstream/policies/release-archives.toml"],
        "meson" => &["src/build-tools/meson"],
        "ninja" => &["src/build-tools/ninja"],
        "file" => &["src/userland/file"],
        "less" => &["src/userland/less"],
        "git" => &["src/userland/git"],
        "openssh" => &["src/system/network/openssh-portable"],
        "libffi" => &["src/system/libraries/libffi/libffi"],
        "icu" => &["src/system/libraries/icu"],
        "freetype" => &["src/system/libraries/freetype"],
        "fontconfig" => &["src/system/libraries/fontconfig"],
        "pop-fonts" => &["src/desktop/fonts/pop-fonts"],
        "kdeclarative" => &["src/desktop/kde/kdeclarative"],
        "qtbase" => &[
            "src/desktop/qt/qtbase",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        "qtshadertools" => &[
            "src/desktop/qt/qtshadertools",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        "qtdeclarative" => &[
            "src/desktop/qt/qtdeclarative",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        "qtsvg" => &[
            "src/desktop/qt/qtsvg",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        "qtwayland" => &[
            "src/desktop/qt/qtwayland",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        "qttools" => &[
            "src/desktop/qt/qttools",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        "qtmultimedia" => &[
            "src/desktop/qt/qtmultimedia",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        "qtspeech" => &[
            "src/desktop/qt/qtspeech",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        "qt5compat" => &[
            "src/desktop/qt/qt5compat",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        "qca" => &[
            "src/system/security/qca",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "qcoro" => &[
            "src/desktop/kde/qcoro",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kcoreaddons" => &[
            "src/desktop/kde/kcoreaddons",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "ki18n" => &[
            "src/desktop/kde/ki18n",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kwidgetsaddons" => &[
            "src/desktop/kde/kwidgetsaddons",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kconfig" => &[
            "src/desktop/kde/kconfig",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kdbusaddons" => &[
            "src/desktop/kde/kdbusaddons",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kauth" => &[
            "src/desktop/kde/kauth",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kwin" => &[
            "src/desktop/kde/kwin",
            "upstream/patches/kwin",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "aurorae" => &[
            "src/desktop/kde/aurorae",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "layer-shell-qt" => &[
            "src/desktop/kde/layer-shell-qt",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "plasma-workspace" => &[
            "src/desktop/kde/plasma-workspace",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kscreenlocker" => &[
            "src/desktop/kde/kscreenlocker",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "plasma-login-manager" => &[
            "src/desktop/kde/plasma-login-manager",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "plasma-desktop" => &[
            "src/desktop/kde/plasma-desktop",
            "src/system/session/plasma",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "breeze" => &[
            "src/desktop/kde/breeze",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "plasma-framework" => &[
            "src/desktop/kde/plasma-framework",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "krunner" => &[
            "src/desktop/kde/krunner",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kcrash" => &[
            "src/desktop/kde/kcrash",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kwindowsystem" => &[
            "src/desktop/kde/kwindowsystem",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kpackage" => &[
            "src/desktop/kde/kpackage",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-karchive" => &[
            "src/desktop/kde/karchive",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kio" => &[
            "src/desktop/kde/kio",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kunitconversion" => &[
            "src/desktop/kde/kunitconversion",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-ksvg" => &[
            "src/desktop/kde/ksvg",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-knotifications" => &[
            "src/desktop/kde/knotifications",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-knotifyconfig" => &[
            "src/desktop/kde/knotifyconfig",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kguiaddons" => &[
            "src/desktop/kde/kguiaddons",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kitemmodels" => &[
            "src/desktop/kde/kitemmodels",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kglobalaccel" => &[
            "src/desktop/kde/kglobalaccel",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kiconthemes" => &[
            "src/desktop/kde/kiconthemes",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kcolorscheme" => &[
            "src/desktop/kde/kcolorscheme",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-ksyntaxhighlighting" => &[
            "src/desktop/kde/ksyntaxhighlighting",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kcompletion" => &[
            "src/desktop/kde/kcompletion",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kjobwidgets" => &[
            "src/desktop/kde/kjobwidgets",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kservice" => &[
            "src/desktop/kde/kservice",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-solid" => &[
            "src/desktop/kde/solid",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kcodecs" => &[
            "src/desktop/kde/kcodecs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kdecoration" => &[
            "src/desktop/kde/kdecoration",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kidletime" => &[
            "src/desktop/kde/kidletime",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "liblcms2-2" => &[
            "src/system/graphics/lcms2",
        ],
        "kf6-kwayland" => &[
            "src/desktop/kde/kwayland",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-knighttime" => &[
            "src/desktop/kde/knighttime",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kholidays" => &[
            "src/desktop/kde/kholidays",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kstatusnotifieritem" => &[
            "src/desktop/kde/kstatusnotifieritem",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kxmlgui" => &[
            "src/desktop/kde/kxmlgui",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kconfigwidgets" => &[
            "src/desktop/kde/kconfigwidgets",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kitemviews" => &[
            "src/desktop/kde/kitemviews",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kbookmarks" => &[
            "src/desktop/kde/kbookmarks",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "qt6-positioning" => &[
            "src/desktop/qt/qtpositioning",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        "kf6-kirigami" => &[
            "src/desktop/kde/kirigami",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kirigami-addons" => &[
            "src/desktop/kde/kirigami-addons",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kf6-kquickcharts" => &[
            "src/desktop/kde/kquickcharts",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "libcanberra0" => &[
            "src/system/libraries/libcanberra",
        ],
        "breeze-icons" => &[
            "src/desktop/kde/breeze-icons",
            "src/tools/mattos-build/src/stages/plasma.rs",
        ],
        "plasma-activities" => &[
            "src/desktop/kde/plasma-activities",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "plasma-activities-stats" => &[
            "src/desktop/kde/plasma-activities-stats",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "plasma5support" => &[
            "src/desktop/kde/plasma5support",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kcmutils" => &[
            "src/desktop/kde/kcmutils",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "ksysguard" => &[
            "src/desktop/kde/ksysguard",
            "src/tools/mattos-build/src/stages/plasma.rs",
        ],
        "knewstuff" => &[
            "src/desktop/kde/knewstuff",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
            "src/tools/mattos-build/src/stages/plasma.rs",
        ],
        "attica" => &[
            "src/desktop/kde/attica",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "sonnet" => &[
            "src/desktop/kde/sonnet",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
            "src/tools/mattos-build/src/stages/plasma.rs",
        ],
        "polkit-qt-1" => &[
            "src/system/security/polkit-qt-1",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "yaml-cpp" => &[
            "src/system/libraries/yaml-cpp",
            "upstream/patches/yaml-cpp",
            "upstream/state/yaml-cpp.toml",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "kpmcore" => &[
            "src/system/storage/kpmcore",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        "calamares" => &[
            "src/system/installer/calamares/upstream",
            "src/system/installer/calamares/mattos",
            "upstream/patches/calamares",
            "upstream/state/calamares.toml",
            "src/tools/mattos-build/src/stages/calamares.rs",
        ],
        "wayland" => &["src/system/libraries/wayland"],
        "xkbcommon" => &["src/system/libraries/xkbcommon"],
        "xkeyboard-config" => &["src/system/data/xkeyboard-config"],
        "tzdata" => &["src/system/data/tzdata"],
        "linux-firmware" => &["src/system/data/linux-firmware"],
        "wireless-regdb" => &["src/system/data/wireless-regdb"],
        "seatd" => &["src/system/libraries/seatd"],
        "libdisplay-info" => &[
            "src/system/libraries/libdisplay-info",
            "src/system/data/hwdata",
        ],
        "libevdev" => &["src/system/libraries/libevdev"],
        "libinput" => &["src/system/libraries/libinput"],
        "pixman" => &["src/system/libraries/pixman"],
        "libdrm" => &["src/system/libraries/libdrm"],
        "x11-compat" => &[
            "src/system/graphics/xorgproto",
            "src/system/graphics/xorg-util-macros",
            "src/system/graphics/xtrans",
            "src/system/graphics/libxau",
            "src/system/graphics/libxdmcp",
            "src/system/graphics/libice",
            "src/system/graphics/libsm",
            "src/system/graphics/libxi",
            "src/system/graphics/libxcursor",
            "src/system/graphics/libxft",
            "src/system/graphics/xcb-proto",
            "src/system/graphics/libxcb",
            "src/system/graphics/libx11",
            "src/system/graphics/libxext",
            "src/system/graphics/libxfixes",
            "src/system/graphics/libxrender",
            "src/system/graphics/libxtst",
            "src/system/graphics/xcb-util",
            "src/system/graphics/xcb-renderutil",
            "src/system/graphics/xcb-image",
            "src/system/graphics/xcb-cursor",
            "src/system/graphics/xcb-util-wm",
            "src/system/graphics/xcb-keysyms",
            "src/system/graphics/xcb-util-m4",
            // The payload dispatcher owns the selected X11/XCB library split;
            // changes to it must invalidate packages assembled from x11-compat.
        ],
        "libglvnd" => &["src/system/graphics/libglvnd"],
        "vulkan-loader" => &[
            "src/system/graphics/vulkan-headers",
            "src/system/graphics/vulkan-loader",
        ],
        "vulkan-tools" => &["src/system/graphics/vulkan-tools"],
        "mesa" => &["src/system/graphics/mesa"],
        "nvidia-driver" => &[
            "src/system/graphics/nvidia-driver",
            "src/system/graphics/nvidia-open-gpu-kernel-modules",
            "upstream/patches/nvidia-open-gpu-kernel-modules",
        ],
        "flatpak" => &[
            "src/system/packages/flatpak",
            "src/system/installer/flatpak-target-install.c",
        ],
        "xwayland" => &[
            "src/system/graphics/xwayland",
            "src/system/graphics/libepoxy",
            "src/system/graphics/libfontenc",
            "src/system/graphics/libxfont",
            "src/system/graphics/libxcvt",
            "src/system/graphics/libxshmfence",
            "src/system/graphics/libxkbfile",
            "src/system/graphics/xkbcomp",
        ],
        "xdg-desktop-portal" => &[
            "src/system/packages/xdg-desktop-portal",
            "src/system/packages/xdg-desktop-portal-gvdb",
            "src/system/packages/xdg-desktop-portal-libglnx",
        ],
        "gstreamer" | "gstreamer-base" => &["src/system/multimedia/gstreamer"],
        "duktape" => &[
            "src/system/security/duktape",
            "src/tools/mattos-build/src/main.rs",
        ],
        "polkit" => &[
            "src/system/security/polkit",
            "src/tools/mattos-build/src/main.rs",
        ],
        "networkmanager" => &["src/system/network/NetworkManager"],
        "libnl" => &["src/system/network/libnl"],
        "wpa-supplicant" => &[
            "src/system/network/hostap",
            "src/system/network/wpa-supplicant",
        ],
        "grub" => &["src/boot/grub/upstream", "src/build-support/grub-gnulib"],
        "cozy" => &["src/userland/cozy"],
        "cpython" => &["src/development/python/cpython"],
        "llvm" => &[
            "src/toolchain/llvm-project",
        ],
        "rust" => &[
            "src/toolchain/rust",
            "upstream/policies/release-archives.toml",
        ],
        _ => &[],
    }
}

fn package_configuration_roots(package: &str) -> &'static [&'static str] {
    match package {
        "flatpak" => &["src/system/packages/config/flatpak"],
        "grub-efi-amd64" => &["src/boot/grub/config"],
        // APT installs these policy files into the runtime package. Keep the
        // package cache identity tied to their bytes so live/rootfs overlays
        // cannot reuse an artifact containing an older repository policy.
        "apt" => &["src/system/packages/config/apt"],
        "dbus-broker" => &[
            "src/system/dbus/config/system.conf",
            "src/system/dbus/config/dbus.conf",
            "src/system/dbus/units",
            "src/system/session/dbus/session.conf",
            "src/system/session/user-units",
        ],
        "libpam-runtime" => &["src/system/auth/config/pam.d"],
        "plasma-login-manager" => &[
            "src/system/session/plasma-login-manager",
            "src/system/session/plasma/start-plasma",
        ],
        "passwd" => &[
            "src/system/auth/config/login.defs",
            "src/system/auth/config/default/useradd",
        ],
        "mattos-sudo-rs" => &[
            "src/system/auth/config/sudoers",
            "src/system/auth/config/sudoers.d/README",
        ],
        "openssh-client" | "openssh-server" => &["src/system/network/openssh"],
        "mattos-installer" => &[
            "src/system/installer/policy/example-plan.toml",
            "src/system/installer/PROVENANCE.md",
            "src/system/units/mattos-install-cli.service",
            "src/system/units/mattos-install-cli.target",
        ],
        _ => &[],
    }
}

fn package_dependencies(repo_root: &Path, spec: &PackageSpec) -> Result<Vec<String>> {
    let specs = package_specs();
    let names: BTreeSet<&str> = specs.iter().map(|candidate| candidate.name).collect();
    effective_dependencies(spec)
        .into_iter()
        .map(|dependency| {
            if names.contains(dependency) {
                let target = specs
                    .iter()
                    .find(|candidate| candidate.name == dependency)
                    .ok_or_else(|| anyhow!("unknown dependency {dependency}"))?;
                Ok(format!(
                    "{dependency} (= {})",
                    package_version(repo_root, target)?
                ))
            } else {
                Ok(dependency.to_string())
            }
        })
        .collect()
}

fn effective_dependencies(spec: &PackageSpec) -> Vec<&'static str> {
    let mut dependencies = spec.depends.to_vec();
    if spec.name != "mattos-filesystem" && spec.name != "libc6" && !dependencies.contains(&"libc6")
    {
        dependencies.insert(0, "libc6");
    }
    dependencies
}

pub(crate) fn package_coreutils_applets(binary: &Path) -> Result<Vec<String>> {
    let applets = list_coreutils_applets(binary)?;
    let component_commands: BTreeSet<&str> = COMPONENT_INSTALL_MANIFESTS
        .iter()
        .flat_map(|manifest| manifest.binaries.iter().map(|binary| binary.command_name))
        .filter(|command| *command != "curl")
        .collect();
    Ok(applets
        .into_iter()
        .filter(|applet| !component_commands.contains(applet.as_str()))
        .collect())
}

fn package_version(repo_root: &Path, spec: &PackageSpec) -> Result<String> {
    let upstream = match spec.name {
        "mattos-filesystem" | "mattos-base-files" => "0.1".to_string(),
        "mattos-compat" => "0.1.0".to_string(),
        "libc6" | "libc6-dev" | "libc-bin" | "locales" => {
            component_snapshot_version(repo_root, "glibc")?
        }
        "linux-libc-dev" => component_snapshot_version(repo_root, "linux-uapi")?,
        LINUX_MODULES_PACKAGE => component_snapshot_version(repo_root, "linux")?,
        "libgcc-s1"
        | "libgomp1"
        | "libstdc++6"
        | "mattos-libgcc-dev"
        | "mattos-libstdc++-dev"
        | "mattos-gcc-common"
        | "cpp"
        | "gcc"
        | "g++" => component_snapshot_version(repo_root, "gcc")?,
        "binutils" => component_snapshot_version(repo_root, "binutils")?,
        "make" => component_snapshot_version(repo_root, "make")?,
        "ca-certificates" => "2026.07.16".to_string(),
        "iso-codes" => "4.20.1".to_string(),
        "mattos-brush" => {
            cargo_package_version(&repo_root.join("src/userland/brush/brush/Cargo.toml"))?
        }
        "coreutils" => {
            cargo_workspace_version(&repo_root.join("src/userland/coreutils/Cargo.toml"))?
        }
        name @ ("grep" | "findutils" | "diffutils") => {
            cargo_package_version(&repo_root.join("src/userland").join(name).join("Cargo.toml"))?
        }
        "curl" => curl_version(&repo_root.join("src/userland/curl/include/curl/curlver.h"))?,
        "dpkg" => fs::read_to_string(repo_root.join("out/build/dpkg/source/.dist-version"))?
            .trim()
            .to_string(),
        "libgpg-error0" => component_snapshot_version(repo_root, "libgpg-error")?,
        "libgcrypt20" => component_snapshot_version(repo_root, "libgcrypt")?,
        "libassuan9" => component_snapshot_version(repo_root, "libassuan")?,
        "libksba8" => component_snapshot_version(repo_root, "libksba")?,
        "libnpth0" => component_snapshot_version(repo_root, "npth")?,
        "gpgv" | "gnupg" => component_snapshot_version(repo_root, "gnupg")?,
        "libapt-pkg7.0" | "apt" => apt_version(repo_root)?,
        "mattos-libtinfow6" | "libncursesw6" | "ncurses-base" | "ncurses-bin"
        | "libncurses-dev" => {
            component_snapshot_version(repo_root, "ncurses")?
        }
        "libreadline8" => component_snapshot_version(repo_root, "readline")?,
        "libndp0" => component_snapshot_version(repo_root, "libndp")?,
        "libkmod2" | "kmod" => component_snapshot_version(repo_root, "kmod")?,
        "mattos-libproc2" | "procps" => component_snapshot_version(repo_root, "procps-ng")?,
        "libsystemd0" | "libudev1" | "udev" | "libsystemd-dev" => {
            component_snapshot_version(repo_root, "systemd")?
        }
        "libexpat1" => component_snapshot_version(repo_root, "expat")?,
        "libfreetype6" => component_snapshot_version(repo_root, "freetype")?,
        "shared-mime-info" => component_snapshot_version(repo_root, "shared-mime-info")?,
        "libfontconfig1" | "fontconfig" | "fontconfig-config" => {
            component_snapshot_version(repo_root, "fontconfig")?
        }
        "fonts-fira" => component_snapshot_version(repo_root, "pop-fonts")?,
        "libcap2" | "libcap-dev" => component_snapshot_version(repo_root, "libcap")?,
        "libattr1" | "libattr1-dev" => component_snapshot_version(repo_root, "attr")?,
        "libacl1" | "libacl1-dev" => component_snapshot_version(repo_root, "acl")?,
        "zlib1g" => component_snapshot_version(repo_root, "zlib")?,
        "libbz2-1.0" | "bzip2" => component_snapshot_version(repo_root, "bzip2")?,
        "liblz4-1" => component_snapshot_version(repo_root, "lz4")?,
        "liblzma5" | "xz-utils" => component_snapshot_version(repo_root, "xz")?,
        "libxxhash0" => component_snapshot_version(repo_root, "xxhash")?,
        "libmd0" => component_snapshot_version(repo_root, "libmd")?,
        "libbsd0" => component_snapshot_version(repo_root, "libbsd")?,
        "libzstd1" | "zstd" => component_snapshot_version(repo_root, "zstd")?,
        "libnghttp2-14" => component_snapshot_version(repo_root, "nghttp2")?,
        "mattos-libcrypto3" | "libssl3t64" => component_snapshot_version(repo_root, "openssl")?,
        "libelf1t64" => component_snapshot_version(repo_root, "elfutils")?,
        "libpcre2-8-0" => component_snapshot_version(repo_root, "pcre2")?,
        "libselinux1" => component_snapshot_version(repo_root, "selinux")?,
        "libcrypt1" => component_snapshot_version(repo_root, "libxcrypt")?,
        "tar" => component_snapshot_version(repo_root, "tar")?,
        "dbus-broker" => component_snapshot_version(repo_root, "dbus-broker")?,
        "libpam0g" | "mattos-libpam-misc0" | "libpam-modules" | "libpam-runtime" => {
            component_snapshot_version(repo_root, "linux-pam")?
        }
        "passwd" | "uidmap" => component_snapshot_version(repo_root, "shadow")?,
        "mattos-sudo-rs" => {
            cargo_package_version(&repo_root.join("src/system/auth/sudo-rs/Cargo.toml"))?
        }
        "libblkid1" | "libmount1" | "libsmartcols1" | "libuuid1" | "libfdisk1" | "mount"
        | "util-linux" | "login" => component_snapshot_version(repo_root, "util-linux")?,
        "gzip" => component_snapshot_version(repo_root, "gzip")?,
        "patch" => component_snapshot_version(repo_root, "patch")?,
        "sed" => component_snapshot_version(repo_root, "sed")?,
        "dash" => component_snapshot_version(repo_root, "dash")?,
        "mawk" => mawk_version(repo_root)?,
        "rsync" => component_snapshot_version(repo_root, "rsync")?,
        "pkgconf" => component_snapshot_version(repo_root, "pkgconf")?,
        "cmake" => component_snapshot_version(repo_root, "cmake")?,
        name @ ("perl" | "m4" | "autoconf" | "automake" | "libtool" | "meson") => {
            component_snapshot_version(repo_root, name)?
        }
        "ninja-build" => component_snapshot_version(repo_root, "ninja")?,
        "libmagic1" | "file" => component_snapshot_version(repo_root, "file")?,
        "less" => component_snapshot_version(repo_root, "less")?,
        "git" => component_snapshot_version(repo_root, "git")?,
        "openssh-client" | "openssh-server" => component_snapshot_version(repo_root, "openssh")?,
        "libffi8" | "libffi-dev" => component_snapshot_version(repo_root, "libffi")?,
        "qt6-base" => component_snapshot_version(repo_root, "qtbase")?,
        "qt6-shadertools" => component_snapshot_version(repo_root, "qtshadertools")?,
        "qt6-declarative" => component_snapshot_version(repo_root, "qtdeclarative")?,
        "qt6-svg" => component_snapshot_version(repo_root, "qtsvg")?,
        "qt6-wayland" => component_snapshot_version(repo_root, "qtwayland")?,
        "qt6-tools" => component_snapshot_version(repo_root, "qttools")?,
        "qt6-multimedia" => component_snapshot_version(repo_root, "qtmultimedia")?,
        "qt6-speech" => component_snapshot_version(repo_root, "qtspeech")?,
        "qt6-core5compat" => component_snapshot_version(repo_root, "qt5compat")?,
        "qca-qt6" => component_snapshot_version(repo_root, "qca")?,
        "qcoro-qt6" => component_snapshot_version(repo_root, "qcoro")?,
        "kf6-kwallet" => component_snapshot_version(repo_root, "kwallet")?,
        "kf6-kcoreaddons" => component_snapshot_version(repo_root, "kcoreaddons")?,
        "kf6-ki18n" => component_snapshot_version(repo_root, "ki18n")?,
        "kf6-kwidgetsaddons" => component_snapshot_version(repo_root, "kwidgetsaddons")?,
        "kf6-kconfig" => component_snapshot_version(repo_root, "kconfig")?,
        "kf6-kdbusaddons" => component_snapshot_version(repo_root, "kdbusaddons")?,
        "kf6-kcrash" => component_snapshot_version(repo_root, "kcrash")?,
        "kf6-kwindowsystem" => component_snapshot_version(repo_root, "kwindowsystem")?,
        "kf6-kpackage" => component_snapshot_version(repo_root, "kpackage")?,
        "kf6-karchive" => component_snapshot_version(repo_root, "karchive")?,
        "kf6-kio" => component_snapshot_version(repo_root, "kio")?,
        "kf6-kunitconversion" => component_snapshot_version(repo_root, "kunitconversion")?,
        "kf6-ksvg" => component_snapshot_version(repo_root, "ksvg")?,
        "kf6-knotifications" => component_snapshot_version(repo_root, "knotifications")?,
        "kf6-knotifyconfig" => component_snapshot_version(repo_root, "knotifyconfig")?,
        "kf6-kguiaddons" => component_snapshot_version(repo_root, "kguiaddons")?,
        "kf6-kitemmodels" => component_snapshot_version(repo_root, "kitemmodels")?,
        "kf6-kglobalaccel" => component_snapshot_version(repo_root, "kglobalaccel")?,
        "kf6-kiconthemes" => component_snapshot_version(repo_root, "kiconthemes")?,
        "kf6-kcolorscheme" => component_snapshot_version(repo_root, "kcolorscheme")?,
        "kf6-ksyntaxhighlighting" => component_snapshot_version(repo_root, "ksyntaxhighlighting")?,
        "kf6-kjobwidgets" => component_snapshot_version(repo_root, "kjobwidgets")?,
        "kf6-kcompletion" => component_snapshot_version(repo_root, "kcompletion")?,
        "kf6-kservice" => component_snapshot_version(repo_root, "kservice")?,
        "kf6-solid" => component_snapshot_version(repo_root, "solid")?,
        "kf6-kcodecs" => component_snapshot_version(repo_root, "kcodecs")?,
        "kf6-kdecoration" => component_snapshot_version(repo_root, "kdecoration")?,
        "kf6-kidletime" => component_snapshot_version(repo_root, "kidletime")?,
        "liblcms2-2" => component_snapshot_version(repo_root, "lcms2")?,
        "kf6-kwayland" => component_snapshot_version(repo_root, "kwayland")?,
        "kf6-knighttime" => component_snapshot_version(repo_root, "knighttime")?,
        "kf6-kholidays" => component_snapshot_version(repo_root, "kholidays")?,
        "kf6-kstatusnotifieritem" => component_snapshot_version(repo_root, "kstatusnotifieritem")?,
        "kf6-kxmlgui" => component_snapshot_version(repo_root, "kxmlgui")?,
        "kf6-kconfigwidgets" => component_snapshot_version(repo_root, "kconfigwidgets")?,
        "kf6-kitemviews" => component_snapshot_version(repo_root, "kitemviews")?,
        "kf6-kbookmarks" => component_snapshot_version(repo_root, "kbookmarks")?,
        "qt6-positioning" => component_snapshot_version(repo_root, "qtpositioning")?,
        "kf6-kirigami" => component_snapshot_version(repo_root, "kirigami")?,
        "kf6-qqc2-desktop-style" => component_snapshot_version(repo_root, "qqc2-desktop-style")?,
        "kf6-kirigami-addons" => component_snapshot_version(repo_root, "kirigami-addons")?,
        "kf6-kquickcharts" => component_snapshot_version(repo_root, "kquickcharts")?,
        "libcanberra0" => component_snapshot_version(repo_root, "libcanberra")?,
        "breeze-icons" => component_snapshot_version(repo_root, "breeze-icons")?,
        "plasma-activities" => component_snapshot_version(repo_root, "plasma-activities")?,
        "plasma-activities-stats" => {
            component_snapshot_version(repo_root, "plasma-activities-stats")?
        }
        "plasma5support" => component_snapshot_version(repo_root, "plasma5support")?,
        "kf6-kcmutils" => component_snapshot_version(repo_root, "kcmutils")?,
        "libprocesscore10" => component_snapshot_version(repo_root, "ksysguard")?,
        "kf6-knewstuffcore" => component_snapshot_version(repo_root, "knewstuff")?,
        "kf6-attica" => component_snapshot_version(repo_root, "attica")?,
        "kf6-sonnet" => component_snapshot_version(repo_root, "sonnet")?,
        "kf6-kauth" => component_snapshot_version(repo_root, "kauth")?,
        "polkit-qt6-1" => component_snapshot_version(repo_root, "polkit-qt-1")?,
        "libyaml-cpp0.8" => component_snapshot_version(repo_root, "yaml-cpp")?,
        "libkpmcore13" => component_snapshot_version(repo_root, "kpmcore")?,
        "calamares" => component_snapshot_version(repo_root, "calamares")?,
        "libwayland-client0" | "libwayland-cursor0" | "libwayland-server0" | "libwayland-egl1" => {
            component_snapshot_version(repo_root, "wayland")?
        }
        "libxkbcommon0" => component_snapshot_version(repo_root, "xkbcommon")?,
        "libxml2-16" => component_snapshot_version(repo_root, "libxml2")?,
        "libpng16-16t64" => component_snapshot_version(repo_root, "libpng")?,
        "libxkbfile1" => component_snapshot_version(repo_root, "libxkbfile")?,
        "xkb-data" => component_snapshot_version(repo_root, "xkeyboard-config")?,
        "tzdata" => component_snapshot_version(repo_root, "tzdata")?,
        "linux-firmware" => component_snapshot_version(repo_root, "linux-firmware")?,
        "wireless-regdb" => component_snapshot_version(repo_root, "wireless-regdb")?,
        "libseat1" => component_snapshot_version(repo_root, "seatd")?,
        "libdisplay-info3" => component_snapshot_version(repo_root, "libdisplay-info")?,
        "libevdev2" => component_snapshot_version(repo_root, "libevdev")?,
        "libinput10" => component_snapshot_version(repo_root, "libinput")?,
        "libpixman-1-0" => component_snapshot_version(repo_root, "pixman")?,
        "libdrm2" | "libdrm-amdgpu1" | "libdrm-nouveau2" => {
            component_snapshot_version(repo_root, "libdrm")?
        }
        "libxau6" => component_snapshot_version(repo_root, "libxau")?,
        "libxdmcp6" => component_snapshot_version(repo_root, "libxdmcp")?,
        "libice6" => component_snapshot_version(repo_root, "libice")?,
        "libsm6" => component_snapshot_version(repo_root, "libsm")?,
        "libxi6" => component_snapshot_version(repo_root, "libxi")?,
        "libxrender1" => component_snapshot_version(repo_root, "libxrender")?,
        "libxtst6" => component_snapshot_version(repo_root, "libxtst")?,
        "libxcursor1" => component_snapshot_version(repo_root, "libxcursor")?,
        "libxft2" => component_snapshot_version(repo_root, "libxft")?,
        "libxcb1" => component_snapshot_version(repo_root, "libxcb")?,
        "libx11-6" => component_snapshot_version(repo_root, "libx11")?,
        "libxext6" => component_snapshot_version(repo_root, "libxext")?,
        "libxfixes3" => component_snapshot_version(repo_root, "libxfixes")?,
        "libglvnd0" | "libglvnd-dev" | "libglx0" | "libgl1" | "libopengl0" | "libegl1"
        | "libgles1" | "libgles2" => component_snapshot_version(repo_root, "libglvnd")?,
        "libgbm1" | "libegl-mesa0" | "libgl1-mesa-dri" | "mesa-vulkan-drivers" => {
            component_snapshot_version(repo_root, "mesa")?
        }
        "libvulkan1" | "libvulkan-dev" | "vulkan-tools" => "1.4.357".to_string(),
        NVIDIA_OPEN_MODULES_PACKAGE
        | "nvidia-firmware-595"
        | "libnvidia-gl-595"
        | "libnvidia-compute-595"
        | "libnvidia-encode-595"
        | "libnvidia-decode-595"
        | "nvidia-utils-595"
        | "nvidia-driver-595-open" => "595.84".to_string(),
        "flatpak" => component_snapshot_version(repo_root, "flatpak")?,
        "libostree-1-1" => component_snapshot_version(repo_root, "ostree")?,
        "ostree" => component_snapshot_version(repo_root, "ostree")?,
        "libgpgme45" => component_snapshot_version(repo_root, "gpgme")?,
        "libgdk-pixbuf-2.0-0" => component_snapshot_version(repo_root, "gdk-pixbuf")?,
        "libgdk-pixbuf2.0-bin" => component_snapshot_version(repo_root, "gdk-pixbuf")?,
        "libappstream5" => component_snapshot_version(repo_root, "appstream")?,
        "libappstreamqt3" => component_snapshot_version(repo_root, "appstream")?,
        "appstream" => component_snapshot_version(repo_root, "appstream")?,
        "libjson-glib-1.0-0" => component_snapshot_version(repo_root, "json-glib")?,
        "libxmlb2" => component_snapshot_version(repo_root, "libxmlb")?,
        "libfyaml0" => component_snapshot_version(repo_root, "libfyaml")?,
        "libfuse3-4" => component_snapshot_version(repo_root, "fuse3")?,
        "fuse3" => component_snapshot_version(repo_root, "fuse3")?,
        "bubblewrap" => component_snapshot_version(repo_root, "bubblewrap")?,
        "xdg-dbus-proxy" => component_snapshot_version(repo_root, "xdg-dbus-proxy")?,
        "xwayland" => component_snapshot_version(repo_root, "xwayland")?,
        "xdg-desktop-portal" => component_snapshot_version(repo_root, "xdg-desktop-portal")?,
        // gst-plugins-base is a subproject of the single GStreamer import.
        "libgstreamer1.0-0" | "libgstreamer-plugins-base1.0-0" => component_snapshot_version(repo_root, "gstreamer")?,
        "libduktape207" => component_snapshot_version(repo_root, "duktape")?,
        "polkit" => component_snapshot_version(repo_root, "polkit")?,
        "network-manager" => component_snapshot_version(repo_root, "networkmanager")?,
        "libnl-3-200" | "libnl-genl-3-200" | "libnl-route-3-200" | "libnl-3-dev"
        | "libnl-genl-3-dev" => {
            component_snapshot_version(repo_root, "libnl")?
        }
        "wpasupplicant" => component_snapshot_version(repo_root, "wpa-supplicant")?,
        "grub-efi-amd64" => component_snapshot_version(repo_root, "grub")?,
        "mattos-cozy" => cargo_package_version(&repo_root.join("src/userland/cozy/Cargo.toml"))?,
        "greetd" => component_snapshot_version(repo_root, "greetd")?,
        "plasma-login-manager" => component_snapshot_version(repo_root, "plasma-login-manager")?,
        "kwin" => component_snapshot_version(repo_root, "kwin")?,
        "kwin-aurorae" => component_snapshot_version(repo_root, "aurorae")?,
        "layer-shell-qt" => component_snapshot_version(repo_root, "layer-shell-qt")?,
        "plasma-framework" => component_snapshot_version(repo_root, "plasma-framework")?,
        "krunner" => component_snapshot_version(repo_root, "krunner")?,
        "kactivitymanagerd" => component_snapshot_version(repo_root, "kactivitymanagerd")?,
        "kglobalacceld" => component_snapshot_version(repo_root, "kglobalacceld")?,
        "plasma-workspace" => component_snapshot_version(repo_root, "plasma-workspace")?,
        "kscreenlocker" => component_snapshot_version(repo_root, "kscreenlocker")?,
        "plasma-desktop" => component_snapshot_version(repo_root, "plasma-desktop")?,
        "breeze" => component_snapshot_version(repo_root, "breeze")?,
        "lm-sensors" => component_snapshot_version(repo_root, "lm-sensors")?,
        "libhwy1" => component_snapshot_version(repo_root, "highway")?,
        "kf6-kfilemetadata" => component_snapshot_version(repo_root, "kfilemetadata")?,
        "kf6-kpty" => component_snapshot_version(repo_root, "kpty")?,
        "kf6-networkmanager-qt" => component_snapshot_version(repo_root, "networkmanager-qt")?,
        "kf6-purpose" => component_snapshot_version(repo_root, "purpose")?,
        "milou" => component_snapshot_version(repo_root, "milou")?,
        "systemsettings" => component_snapshot_version(repo_root, "systemsettings")?,
        "ksystemstats" => component_snapshot_version(repo_root, "ksystemstats")?,
        "plasma-systemmonitor" => component_snapshot_version(repo_root, "plasma-systemmonitor")?,
        "polkit-kde-agent-1" => component_snapshot_version(repo_root, "polkit-kde-agent-1")?,
        "kquickimageeditor" => component_snapshot_version(repo_root, "kquickimageeditor")?,
        "ffmpeg-libs" => component_snapshot_version(repo_root, "ffmpeg")?,
        "libva2" => component_snapshot_version(repo_root, "libva")?,
        "libopencv4" => component_snapshot_version(repo_root, "opencv")?,
        "kpipewire" => component_snapshot_version(repo_root, "kpipewire")?,
        "spectacle" => component_snapshot_version(repo_root, "spectacle")?,
        "pulseaudio-qt" => component_snapshot_version(repo_root, "pulseaudio-qt")?,
        "plasma-pa" => component_snapshot_version(repo_root, "plasma-pa")?,
        "plasma-nm" => component_snapshot_version(repo_root, "plasma-nm")?,
        "powerdevil" => component_snapshot_version(repo_root, "powerdevil")?,
        "xdg-desktop-portal-kde" => {
            component_snapshot_version(repo_root, "xdg-desktop-portal-kde")?
        }
        "dolphin" => component_snapshot_version(repo_root, "dolphin")?,
        "konsole" => component_snapshot_version(repo_root, "konsole")?,
        "kate" => component_snapshot_version(repo_root, "kate")?,
        "ark" => component_snapshot_version(repo_root, "ark")?,
        "libpackagekitqt6-2" => component_snapshot_version(repo_root, "packagekit-qt")?,
        "haruna" => component_snapshot_version(repo_root, "haruna")?,
        "libmpvqt3" => component_snapshot_version(repo_root, "mpvqt")?,
        "plasma-discover" => component_snapshot_version(repo_root, "discover")?,
        "gwenview" => component_snapshot_version(repo_root, "gwenview")?,
        "libkimageannotator-qt6-0" => component_snapshot_version(repo_root, "kimageannotator")?,
        "libkcolorpicker-qt6-0" => component_snapshot_version(repo_root, "kcolorpicker")?,
        "elisa" => component_snapshot_version(repo_root, "elisa")?,
        "partitionmanager" => component_snapshot_version(repo_root, "partitionmanager")?,
        "kwalletmanager" => component_snapshot_version(repo_root, "kwalletmanager")?,
        "kcalc" => component_snapshot_version(repo_root, "kcalc")?,
        "kf6-kparts" => component_snapshot_version(repo_root, "kparts")?,
        "kf6-ktextwidgets" => component_snapshot_version(repo_root, "ktextwidgets")?,
        "kf6-ktexteditor" => component_snapshot_version(repo_root, "ktexteditor")?,
        "libqrencode4" => component_snapshot_version(repo_root, "qrencode")?,
        "libzxing4" => component_snapshot_version(repo_root, "zxing-cpp")?,
        "kf6-prison" => component_snapshot_version(repo_root, "prison")?,
        "kf6-libkscreen" => component_snapshot_version(repo_root, "libkscreen")?,
        "modemmanager" => component_snapshot_version(repo_root, "modemmanager")?,
        "kf6-modemmanager-qt" => component_snapshot_version(repo_root, "modemmanager-qt")?,
        "libarchive13" => component_snapshot_version(repo_root, "libarchive")?,
        "libsndfile1" => component_snapshot_version(repo_root, "libsndfile")?,
        "libpulse0" => component_snapshot_version(repo_root, "pulseaudio")?,
        "libgudev-1.0-0" => component_snapshot_version(repo_root, "libgudev")?,
        "libgmp10" => component_snapshot_version(repo_root, "gmp")?,
        "libmpfr6" => component_snapshot_version(repo_root, "mpfr")?,
        "libmpc3" => component_snapshot_version(repo_root, "mpc")?,
        "nasm" => component_snapshot_version(repo_root, "nasm")?,
        "packagekit" => component_snapshot_version(repo_root, "packagekit")?,
        "libsqlite3-0" => component_snapshot_version(repo_root, "sqlite")?,
        "libjansson4" => component_snapshot_version(repo_root, "jansson")?,
        "libkdsingleapplication-qt6-1.2" => component_snapshot_version(repo_root, "kdsingleapplication")?,
        "libmpv2" => component_snapshot_version(repo_root, "mpv")?,
        "libplacebo360" => component_snapshot_version(repo_root, "libplacebo")?,
        "libass9" => component_snapshot_version(repo_root, "libass")?,
        "libharfbuzz0b" => component_snapshot_version(repo_root, "harfbuzz")?,
        "libfribidi0" => component_snapshot_version(repo_root, "fribidi")?,
        "libexiv2-28" => component_snapshot_version(repo_root, "exiv2")?,
        "libjpeg62-turbo" => component_snapshot_version(repo_root, "libjpeg-turbo")?,
        "libxrandr2" => component_snapshot_version(repo_root, "libxrandr")?,
        "libbytesize1" => component_snapshot_version(repo_root, "libbytesize")?,
        "libkeyutils1" => component_snapshot_version(repo_root, "keyutils")?,
        "libnvme1" => component_snapshot_version(repo_root, "libnvme")?,
        "libpopt0" => component_snapshot_version(repo_root, "popt")?,
        "libjson-c5" => component_snapshot_version(repo_root, "json-c")?,
        "libdevmapper1.02.1" => component_snapshot_version(repo_root, "lvm2")?,
        "libcryptsetup12" => component_snapshot_version(repo_root, "cryptsetup")?,
        "libblockdev3" => component_snapshot_version(repo_root, "libblockdev")?,
        "wireplumber" => component_snapshot_version(repo_root, "wireplumber")?,
        "upower" => component_snapshot_version(repo_root, "upower")?,
        "udisks2" => component_snapshot_version(repo_root, "udisks2")?,
        "bluez" => component_snapshot_version(repo_root, "bluez")?,
        "power-profiles-daemon" => component_snapshot_version(repo_root, "power-profiles-daemon")?,
        "libdbus-1-3" => component_snapshot_version(repo_root, "dbus")?,
        "libdav1d7" => component_snapshot_version(repo_root, "dav1d")?,
        "libglib2.0-0t64" => component_snapshot_version(repo_root, "glib")?,
        "libicu78" => component_snapshot_version(repo_root, "icu")?,
        "kf6-kdeclarative" => component_snapshot_version(repo_root, "kdeclarative")?,
        "pipewire" => component_snapshot_version(repo_root, "pipewire")?,
        "libpython3.14" | "python3" | "python3-venv" | "python3-dev" => {
            component_snapshot_version(repo_root, "cpython")?
        }
        "libllvm22" | "llvm" | "llvm-dev" | "clang" | "lld" => {
            component_snapshot_version(repo_root, "llvm")?
        }
        "rustc" | "cargo" => component_snapshot_version(repo_root, "rust")?,
        "iproute2" => component_snapshot_version(repo_root, "iproute2")?,
        "iputils-ping" => component_snapshot_version(repo_root, "iputils")?,
        "btrfs-progs" => "6.17".to_string(),
        "dosfstools" => "4.2".to_string(),
        "e2fsprogs" => "1.47.2".to_string(),
        "mattos-installer" => "0.1".to_string(),
        "systemd" => component_snapshot_version(repo_root, "systemd")?,
        "mattos-base-runtime"
        | "mattos-base"
        | "mattos-cli"
        | "mattos-build-essential"
        | "mattos-plasma"
        | "mattos-toolchain"
        | "mattos-plasma-live"
        | "mattos-plasma-theme" => "0.1".to_string(),
        name => match staging::development_package_component(name) {
            // A table `-dev` package shares its library's version.
            Some(component) => component_snapshot_version(repo_root, component)?,
            None => bail!("unknown package {}", spec.name),
        },
    };
    let epoch = compatibility_epoch(repo_root, &spec.name)?;
    let upstream = match epoch {
        Some(epoch) => format!("{epoch}:{upstream}"),
        None => upstream,
    };
    let revision = packaging_revision(repo_root, spec.name, &upstream)?;
    Ok(format!("{upstream}-{REVISION}{revision}"))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionLedger {
    schema_version: u32,
    #[serde(default)]
    package: BTreeMap<String, RevisionEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionEntry {
    upstream: String,
    revision: u32,
}

fn read_revision_ledger(repo_root: &Path) -> Result<RevisionLedger> {
    let path = repo_root.join(REVISION_LEDGER);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RevisionLedger { schema_version: 1, package: BTreeMap::new() });
        }
        Err(error) => return Err(error).with_context(|| format!("failed to read {}", path.display())),
    };
    let ledger: RevisionLedger =
        toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
    if ledger.schema_version != 1 {
        bail!("{} has unsupported schema_version {}", path.display(), ledger.schema_version);
    }
    Ok(ledger)
}

/// The packaging revision of `package` at `upstream`: the ledger's, while its
/// entry names this upstream version, otherwise 1.
fn packaging_revision(repo_root: &Path, package: &str, upstream: &str) -> Result<u32> {
    Ok(read_revision_ledger(repo_root)?
        .package
        .get(package)
        .filter(|entry| entry.upstream == upstream)
        .map_or(1, |entry| entry.revision))
}

/// Reject ledger entries for unknown packages or with a meaningless revision,
/// and report entries a newer upstream version has made obsolete.
pub(crate) fn validate_revision_ledger(repo_root: &Path) -> Result<Option<String>> {
    let ledger = read_revision_ledger(repo_root)?;
    let specs = package_specs();
    let mut stale = Vec::new();
    for (name, entry) in &ledger.package {
        let spec = specs
            .iter()
            .find(|spec| spec.name == name)
            .ok_or_else(|| anyhow!("{REVISION_LEDGER} names unknown package {name}"))?;
        if entry.revision < 2 {
            bail!("{REVISION_LEDGER}: {name} revision {} must be at least 2 (1 is the default)", entry.revision);
        }
        let version = package_version(repo_root, spec)?;
        let current = version.rsplit_once('-').map_or(version.as_str(), |(upstream, _)| upstream);
        if entry.upstream != current {
            stale.push(format!("  {name}: entry for {}, now {current}", entry.upstream));
        }
    }
    Ok((!stale.is_empty()).then(|| {
        format!(
            "warning: {} entr{} in {REVISION_LEDGER} name an older upstream version and no longer apply:\n{}\n",
            stale.len(),
            if stale.len() == 1 { "y" } else { "ies" },
            stale.join("\n")
        )
    }))
}

fn compatibility_epoch(repo_root: &Path, package_name: &str) -> Result<Option<u64>> {
    let manifest_path = repo_root.join("src/system/packages/debian-compat/trixie.toml");
    let manifest: DebianCompatibilityManifest =
        toml::from_str(&fs::read_to_string(&manifest_path)?)
            .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    Ok(manifest
        .package
        .into_iter()
        .find(|package| package.mattos_name == package_name)
        .and_then(|package| package.debian_epoch))
}

fn component_snapshot_version(repo_root: &Path, component: &str) -> Result<String> {
    let state = read_sync_state(repo_root, component)?
        .ok_or_else(|| anyhow!("upstream state missing for {component}"))?;
    if let Some(version) = release_version_from_branch(&state.branch) {
        return Ok(version);
    }
    snapshot_version::snapshot_upstream_version(
        component,
        &repo_root.join(&state.destination_path),
        state.imported_commit.trim(),
        state.upstream_committed_at_utc.as_deref(),
    )
}

/// The Debian upstream version named by a release tag (`branch` in
/// `upstream/sources.toml`), or `None` for a moving branch such as `main`.
///
/// Tags name versions in many styles: `openssl-3.5.8`, `v6.26.0`,
/// `curl-8_22_0`, `R_2_8_5`, `FILE5_48`, `libXfont2-2.0.9`, `VER-2-14-3`,
/// `V_10_5_P1` (OpenSSH), `2026d` (tzdata), `20260916` and
/// `master-2026-09-03` (dated data releases), `v7.2-rc5`.  The version is
/// the last `-`-separated field that holds a dotted or underscored number,
/// else the trailing run of numeric fields, with any project-name letters
/// stripped and `_` read as `.`.  A tag that yields no version falls back to
/// a snapshot version, which sorts below every release.
fn release_version_from_branch(branch: &str) -> Option<String> {
    let mut tag = branch.rsplit('/').next()?;
    if let Some(version) = openssh_release_version(tag) {
        return Some(version);
    }
    // tzdata releases: a year and a letter.
    if tag.len() == 5
        && tag[..4].bytes().all(|byte| byte.is_ascii_digit())
        && tag.as_bytes()[4].is_ascii_lowercase()
    {
        return Some(tag.to_string());
    }
    let mut release_candidate = None;
    if let Some(at) = tag.rfind("-rc")
        && !tag[at + 3..].is_empty()
        && tag[at + 3..].bytes().all(|byte| byte.is_ascii_digit())
    {
        release_candidate = Some(&tag[at + 3..]);
        tag = &tag[..at];
    }
    let fields = tag.split('-').collect::<Vec<_>>();
    let is_number_list = |field: &str| {
        field.bytes().enumerate().any(|(at, byte)| {
            matches!(byte, b'.' | b'_')
                && at > 0
                && field.as_bytes()[at - 1].is_ascii_digit()
                && field.as_bytes().get(at + 1).is_some_and(u8::is_ascii_digit)
        })
    };
    let strip_name = |field: &str| field.trim_start_matches(|ch: char| ch.is_ascii_alphabetic()).to_string();
    let core = if let Some(field) = fields.iter().rev().find(|field| is_number_list(field)) {
        strip_name(field)
    } else {
        let mut trailing = Vec::new();
        for field in fields.iter().rev() {
            let digits = strip_name(field);
            if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                break;
            }
            trailing.insert(0, digits.clone());
            if digits != *field {
                break;
            }
        }
        trailing.join(".")
    };
    let core = core.trim_matches(['.', '_']).replace('_', ".");
    if !core.starts_with(|ch: char| ch.is_ascii_digit())
        || !core.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '~' | '+'))
    {
        return None;
    }
    Some(match release_candidate {
        Some(number) => format!("{core}~rc{number}"),
        None => core,
    })
}

/// OpenSSH portable tags: `V_10_5_P1` is release 10.5p1.
fn openssh_release_version(tag: &str) -> Option<String> {
    let rest = tag.strip_prefix(['V', 'v'])?.strip_prefix('_')?;
    let mut parts = rest.split('_');
    let (major, minor, portable) = (parts.next()?, parts.next()?, parts.next()?);
    let portable = portable.strip_prefix(['P', 'p'])?;
    let numeric = |value: &str| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit());
    (parts.next().is_none() && numeric(major) && numeric(minor) && numeric(portable))
        .then(|| format!("{major}.{minor}p{portable}"))
}

fn apt_version(repo_root: &Path) -> Result<String> {
    let output = Command::new(repo_root.join("out/build/apt/install/usr/bin/apt"))
        .arg("--version")
        .env(
            "LD_LIBRARY_PATH",
            repo_root.join("out/build/apt/install/usr/lib/x86_64-linux-gnu"),
        )
        .output()
        .context("failed to obtain the built APT version")?;
    if !output.status.success() {
        bail!("built apt --version failed")
    }
    String::from_utf8(output.stdout)?
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("unable to parse built APT version"))
}

fn cargo_package_version(path: &Path) -> Result<String> {
    let value: toml::Value = toml::from_str(&fs::read_to_string(path)?)?;
    value
        .get("package")
        .and_then(|v| v.get("version"))
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("version missing from {}", path.display()))
}

fn cargo_workspace_version(path: &Path) -> Result<String> {
    let value: toml::Value = toml::from_str(&fs::read_to_string(path)?)?;
    value
        .get("workspace")
        .and_then(|v| v.get("package"))
        .and_then(|v| v.get("version"))
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("workspace.package.version missing from {}", path.display()))
}

/// mawk versions its snapshots by date on top of 1.3.4, as Debian does:
/// `1.3.4.<YYYYMMDD>` from the tree's `patchlev.h`.
fn mawk_version(repo_root: &Path) -> Result<String> {
    let path = repo_root.join("src/userland/mawk/patchlev.h");
    let body = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let define = |name: &str| {
        body.lines().find_map(|line| {
            let mut words = line.split_whitespace();
            (words.next() == Some("#define") && words.next() == Some(name))
                .then(|| words.next().map(|value| value.trim_matches('"').to_string()))?
        })
    };
    match (define("PATCH_BASE"), define("PATCH_LEVEL"), define("PATCH_STRING"), define("DATE_STRING")) {
        (Some(base), Some(level), Some(patch), Some(date)) => {
            Ok(format!("{base}.{level}{patch}.{date}"))
        }
        _ => bail!("{} lacks the mawk version defines", path.display()),
    }
}

fn curl_version(path: &Path) -> Result<String> {
    let body = fs::read_to_string(path)?;
    for line in body.lines() {
        if let Some(value) = line
            .trim()
            .strip_prefix("#define LIBCURL_VERSION \"")
            .and_then(|s| s.strip_suffix('"'))
        {
            return Ok(value.trim_end_matches("-DEV").to_string());
        }
    }
    bail!("LIBCURL_VERSION missing from {}", path.display())
}

fn validate_package_name(name: &str) -> Result<()> {
    let bytes = name.as_bytes();
    if bytes.len() < 2
        || !bytes[0].is_ascii_lowercase()
        || !bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.')
        })
    {
        bail!("invalid Debian package name {name:?}")
    }
    Ok(())
}

fn validate_debian_version(version: &str) -> Result<()> {
    let upstream = version
        .rsplit_once('-')
        .map(|(left, _)| left)
        .unwrap_or(version);
    if version.is_empty()
        || !upstream.as_bytes().first().is_some_and(u8::is_ascii_digit)
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'~' | b'-' | b':'))
    {
        bail!("invalid Debian version {version:?}")
    }
    Ok(())
}

fn render_control(
    spec: &PackageSpec,
    version: &str,
    installed_size: u64,
    dependencies: &[String],
    runtime_libraries: &[String],
) -> Result<String> {
    validate_package_name(spec.name)?;
    validate_debian_version(version)?;
    let mut fields = vec![
        format!("Package: {}", spec.name),
        format!("Version: {version}"),
        format!("Architecture: {ARCH}"),
        format!("Priority: {}", spec.priority),
        "Maintainer: MattOS Project <packages@mattos.invalid>".to_string(),
        format!("Installed-Size: {installed_size}"),
        format!(
            "Depends: {}",
            if dependencies.is_empty() {
                "".to_string()
            } else {
                dependencies.join(", ")
            }
        ),
    ];
    if spec.essential {
        fields.push("Essential: yes".to_string());
    }
    let provides = spec
        .provides
        .iter()
        .copied()
        .filter(|relation| *relation != spec.name)
        .collect::<Vec<_>>();
    let conflicts = spec
        .conflicts
        .iter()
        .copied()
        .filter(|relation| *relation != spec.name)
        .collect::<Vec<_>>();
    let replaces = spec
        .replaces
        .iter()
        .copied()
        .filter(|relation| *relation != spec.name)
        .collect::<Vec<_>>();
    if !provides.is_empty() {
        fields.push(format!("Provides: {}", provides.join(", ")));
    }
    if !conflicts.is_empty() {
        fields.push(format!("Conflicts: {}", conflicts.join(", ")));
    }
    if !replaces.is_empty() {
        fields.push(format!("Replaces: {}", replaces.join(", ")));
    }
    if !runtime_libraries.is_empty() {
        fields.push(format!(
            "X-MattOS-ELF-Dependencies: {}",
            runtime_libraries.join(", ")
        ));
    }
    fields.push(format!("Description: {}", spec.description));
    Ok(format!("{}\n", fields.join("\n")))
}

fn write_provenance(
    repo_root: &Path,
    staging: &Path,
    spec: &PackageSpec,
    version: &str,
    runtime_libraries: &[String],
) -> Result<()> {
    let (source_path, repository, commit, configuration) = match spec.source_component {
        "brush" => component_provenance(
            repo_root,
            "brush",
            "src/userland/brush",
            "cargo build --release",
        )?,
        "coreutils" => component_provenance(
            repo_root,
            "coreutils",
            "src/userland/coreutils",
            "cargo build --release",
        )?,
        component @ ("grep" | "findutils" | "diffutils") => component_provenance(
            repo_root,
            component,
            &format!("src/userland/{component}"),
            "cargo build --release",
        )?,
        "curl" => component_provenance(
            repo_root,
            "curl",
            "src/userland/curl",
            &curl_configure_options().join(" "),
        )?,
        "dpkg" => component_provenance(
            repo_root,
            "dpkg",
            "src/system/packages/dpkg",
            "./configure --prefix=/usr --sysconfdir=/etc --localstatedir=/var --libexecdir=/usr/libexec --disable-dselect --disable-nls; make; make install",
        )?,
        "apt" => component_provenance(
            repo_root,
            "apt",
            "src/system/packages/apt",
            "cmake Release CURRENT_VENDOR=mattos COMMON_ARCH=amd64 WITH_DOC=OFF WITH_TESTS=OFF USE_NLS=OFF",
        )?,
        "ca-certificates" => (
            "src/system/network/ca-certificates.crt".to_string(),
            "https://curl.se/ca/cacert-2026-07-16.pem".to_string(),
            "sha256:3ff344e30b9b1ed2971044eabb438a08f2e2245ddb5f8ab1a3ad8b63ab4eaf91".to_string(),
            "pinned Mozilla-derived curl CA Extract; 119 certificates; MPL-2.0".to_string(),
        ),
        "gcc" => {
            let state = read_sync_state(repo_root, "gcc")?
                .ok_or_else(|| anyhow!("upstream state missing for gcc"))?;
            let invocation = if matches!(spec.name, "mattos-gcc-common" | "cpp" | "gcc" | "g++") {
                "out/build/gcc-toolchain/configure-invocation.txt"
            } else {
                "out/build/gcc-runtime/configure-invocation.txt"
            };
            let configuration = fs::read_to_string(repo_root.join(invocation))?
                .trim()
                .to_string();
            (
                state.destination_path,
                state.repo,
                state.imported_commit,
                configuration,
            )
        }
        component @ ("binutils" | "make") => {
            let state = read_sync_state(repo_root, component)?
                .ok_or_else(|| anyhow!("upstream state missing for {component}"))?;
            let configuration = fs::read_to_string(
                repo_root.join(format!("out/build/{component}/configure-invocation.txt")),
            )?
            .trim()
            .to_string();
            (
                state.destination_path,
                state.repo,
                state.imported_commit,
                configuration,
            )
        }
        "linux-uapi" => {
            let state = read_sync_state(repo_root, "linux-uapi")?
                .ok_or_else(|| anyhow!("upstream state missing for linux-uapi"))?;
            let configuration =
                fs::read_to_string(repo_root.join("out/build/glibc/kernel-headers-source.txt"))?
                    .trim()
                    .to_string();
            (
                state.destination_path,
                state.repo,
                state.imported_commit,
                configuration,
            )
        }
        "x11-compat" => {
            let libx11 = read_sync_state(repo_root, "libx11")?
                .ok_or_else(|| anyhow!("upstream state missing for libx11"))?;
            let libice = read_sync_state(repo_root, "libice")?
                .ok_or_else(|| anyhow!("upstream state missing for libice"))?;
            let libsm = read_sync_state(repo_root, "libsm")?
                .ok_or_else(|| anyhow!("upstream state missing for libsm"))?;
            let libxi = read_sync_state(repo_root, "libxi")?
                .ok_or_else(|| anyhow!("upstream state missing for libxi"))?;
            let libxrender = read_sync_state(repo_root, "libxrender")?
                .ok_or_else(|| anyhow!("upstream state missing for libxrender"))?;
            let libxtst = read_sync_state(repo_root, "libxtst")?
                .ok_or_else(|| anyhow!("upstream state missing for libxtst"))?;
            let libxcursor = read_sync_state(repo_root, "libxcursor")?
                .ok_or_else(|| anyhow!("upstream state missing for libxcursor"))?;
            let libxft = read_sync_state(repo_root, "libxft")?
                .ok_or_else(|| anyhow!("upstream state missing for libxft"))?;
            (
                "src/system/graphics/{libxau,libxdmcp,libice,libsm,libxi,libxcb,libx11,libxext,libxrender,libxtst,libxcursor,libxft}".to_string(),
                "https://gitlab.freedesktop.org/xorg".to_string(),
                format!(
                    "libx11:{}; libICE:{}; libSM:{}; libXi:{}; libXrender:{}; libXtst:{}; libXcursor:{}; libXft:{}",
                    libx11.imported_commit, libice.imported_commit, libsm.imported_commit,
                    libxi.imported_commit, libxrender.imported_commit, libxtst.imported_commit,
                    libxcursor.imported_commit, libxft.imported_commit
                ),
                "source-built X11 client/session ABI for Wayland session management, Xwayland, cursor and font support; no X server or X11 desktop session".to_string(),
            )
        }
        "nvidia-driver" => {
            let open = read_sync_state(repo_root, "nvidia-open-gpu-kernel-modules")?
                .ok_or_else(|| anyhow!("upstream state missing for NVIDIA open modules"))?;
            (
                "src/system/graphics/nvidia-driver/manifest.toml + src/system/graphics/nvidia-open-gpu-kernel-modules".to_string(),
                "https://download.nvidia.com/XFree86/Linux-x86_64/595.84/ + https://github.com/NVIDIA/open-gpu-kernel-modules".to_string(),
                format!("runfile-sha256:9e4f5d56e74e1ec12a05b2b0afda893c3187da71cbd8fb14c1a394bbeeeb4148; open:{}", open.imported_commit),
                concat!("NVIDIA 595.84 production stack; proprietary files extracted verbatim without stripping; open modules built for ", mattos_kernel_release!()).to_string(),
            )
        }
        "mattos-plasma-theme" => {
            let components = [
                "nordic-kde",
                "papirus-icon-theme",
                "material-cursors",
                "utterly-round-aurorae",
                "xcursorgen",
            ];
            let mut repositories = Vec::new();
            let mut commits = Vec::new();
            for component in components {
                let state = read_sync_state(repo_root, component)?
                    .ok_or_else(|| anyhow!("upstream state missing for {component}"))?;
                repositories.push(format!("{component}={}", state.repo));
                commits.push(format!("{component}={}", state.imported_commit));
            }
            (
                "src/system/desktop/branding/MattOS; four pinned runtime theme sources plus pinned host-only xcursorgen asset compiler".to_string(),
                repositories.join("; "),
                commits.join("; "),
                "MattOS-owned appearance defaults/layout; panel-defaults.conf is validated and rendered into Plasma's supported layout scripting API; external slideshow paths retained verbatim; wallpaper images excluded; xcursorgen is build-only and excluded from runtime payload".to_string(),
            )
        }
        component @ ("glibc" | "ncurses" | "kmod" | "procps-ng" | "systemd" | "dbus-broker"
        | "linux-pam" | "shadow" | "sudo-rs" | "util-linux" | "iproute2"
        | "iputils" | "expat" | "libcap" | "acl" | "zlib" | "bzip2" | "lz4" | "xz"
        | "xxhash" | "zstd" | "nghttp2" | "openssl" | "elfutils" | "pcre2" | "selinux"
        | "libxcrypt" | "libmd" | "libbsd" | "tar" | "gzip" | "patch" | "file"
        | "libgpg-error" | "libgcrypt" | "libassuan" | "libksba" | "npth"
        | "gnupg" | "less" | "git" | "openssh" | "libffi" | "wayland"
        | "xkbcommon" | "libglvnd" | "xkeyboard-config" | "cpython" | "llvm"
        | "rust" | "libnl" | "wpa-supplicant" | "grub" | "greetd" | "sed" | "dash"
        | "mawk" | "rsync" | "pkgconf" | "cmake" | "attr" | "perl" | "m4" | "autoconf"
        | "automake" | "libtool" | "meson" | "ninja") => {
            let state = read_sync_state(repo_root, component)?
                .ok_or_else(|| anyhow!("upstream state missing for {component}"))?;
            (
                state.destination_path,
                state.repo,
                state.imported_commit,
                if component == "xkeyboard-config" {
                    "pinned XKB runtime-data subset staged under /usr/share/X11/xkb".to_string()
                } else {
                    format!("MattOS source build output in out/build/{component}/install")
                },
            )
        }
        _ => (
            "src/rootfs/skeleton".to_string(),
            "MattOS monorepo".to_string(),
            "working-tree".to_string(),
            "mattos package staging".to_string(),
        ),
    };
    let configuration = configuration.replace(repo_root.to_string_lossy().as_ref(), "<repo>");
    let info = Provenance {
        package: spec.name,
        version,
        architecture: ARCH,
        mattos_source_path: &source_path,
        upstream_repository: &repository,
        upstream_commit: &commit,
        build_configuration: &configuration,
        runtime_libraries,
    };
    let destination = staging
        .join("usr/share/doc")
        .join(spec.name)
        .join("mattos-build-info.toml");
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(destination, toml::to_string_pretty(&info)?)?;
    Ok(())
}

fn component_provenance(
    repo_root: &Path,
    component: &str,
    path: &str,
    config: &str,
) -> Result<(String, String, String, String)> {
    let state = read_sync_state(repo_root, component)?
        .ok_or_else(|| anyhow!("upstream state missing for {component}"))?;
    Ok((
        path.to_string(),
        state.repo,
        state.imported_commit,
        config.to_string(),
    ))
}

fn installed_size_kib(root: &Path) -> Result<u64> {
    let mut bytes = 0u64;
    #[cfg(unix)]
    let mut seen_inodes = BTreeSet::new();
    walk_tree(root, &mut |path, meta| {
        if meta.is_file() && !path.starts_with(root.join("DEBIAN")) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if !seen_inodes.insert((meta.dev(), meta.ino())) {
                    return Ok(());
                }
            }
            bytes += meta.len();
        }
        Ok(())
    })?;
    Ok(bytes.div_ceil(1024))
}

fn count_package_entries(root: &Path) -> Result<u64> {
    let mut count = 0;
    walk_tree(root, &mut |path, _| {
        if !path.starts_with(root.join("DEBIAN")) {
            count += 1;
        }
        Ok(())
    })?;
    Ok(count)
}

fn walk_tree(
    root: &Path,
    callback: &mut dyn FnMut(&Path, &fs::Metadata) -> Result<()>,
) -> Result<()> {
    if !root.is_dir() {
        bail!("tree missing at {}", root.display());
    }
    let mut entries: Vec<_> = fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        callback(&path, &meta)?;
        if meta.is_dir() {
            walk_tree(&path, callback)?;
        }
    }
    Ok(())
}

fn normalize_tree_timestamps(root: &Path) -> Result<()> {
    let time = FileTime::from_unix_time(SOURCE_DATE_EPOCH, 0);
    walk_tree(root, &mut |path, meta| {
        if meta.file_type().is_symlink() {
            set_symlink_file_times(path, time, time)?;
        } else {
            set_file_times(path, time, time)?;
        }
        Ok(())
    })?;
    set_file_times(root, time, time)?;
    Ok(())
}

fn normalize_package_modes(root: &Path) -> Result<()> {
    walk_tree(root, &mut |path, meta| {
        if meta.file_type().is_symlink() {
            return Ok(());
        }
        let rel = path.strip_prefix(root)?;
        let mode = if meta.is_dir() {
            if rel == Path::new("root") {
                0o700
            } else if rel == Path::new("tmp") {
                0o1777
            } else if rel == Path::new("etc/sudoers.d") {
                0o750
            } else {
                0o755
            }
        } else if matches!(
            rel.to_str(),
            Some(
                "usr/bin/passwd"
                    | "usr/bin/newgrp"
                    | "usr/bin/sudo"
                    | "usr/bin/login"
                    | "usr/bin/su"
                    | "usr/bin/pkexec"
                    | "usr/bin/fusermount3"
                    | "usr/bin/newuidmap"
                    | "usr/bin/newgidmap"
                    | "usr/lib/polkit-1/polkit-agent-helper-1"
            )
        ) {
            0o4755
        } else if matches!(rel.to_str(), Some("etc/sudoers" | "etc/sudoers.d/README")) {
            0o440
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o111 != 0 {
                    0o755
                } else {
                    0o644
                }
            }
            #[cfg(not(unix))]
            {
                0o644
            }
        };
        set_mode(path.to_path_buf(), mode)
    })?;
    set_mode(root.to_path_buf(), 0o755)
}

fn sha256_file(path: &Path) -> Result<String> {
    performance::sha256_file(path)
}

fn verify_deb(path: &Path, expected_name: &str, expected_version: &str) -> Result<()> {
    let info = Command::new("dpkg-deb")
        .args(["--field", path_str(path)?])
        .output()
        .context("failed to inspect package metadata")?;
    if !info.status.success() {
        bail!("dpkg-deb --field failed for {}", path.display());
    }
    let fields = String::from_utf8(info.stdout)?;
    let paragraphs = repository::parse_control_paragraphs(&fields)?;
    let paragraph = paragraphs
        .first()
        .ok_or_else(|| anyhow!("package {} has no control fields", path.display()))?;
    for (field, expected) in [
        ("Package", expected_name),
        ("Version", expected_version),
        ("Architecture", ARCH),
    ] {
        if repository::control_field(paragraph, field)? != expected {
            bail!(
                "package {} has invalid {field}; expected {expected}",
                path.display()
            );
        }
    }
    let contents = Command::new("dpkg-deb")
        .args(["--contents", path_str(path)?])
        .output()?;
    if !contents.status.success() {
        bail!("dpkg-deb --contents failed for {}", path.display());
    }
    let listing = String::from_utf8(contents.stdout)?;
    if listing.lines().any(|line| {
        line.split(" -> ")
            .next()
            .unwrap_or(line)
            .split_whitespace()
            .last()
            .is_some_and(|entry| entry.contains("../"))
    }) {
        bail!("unsafe parent path leaked into {}", path.display());
    }
    Ok(())
}

fn write_inventory(repo_root: &Path, inventory: &PackageInventory) -> Result<()> {
    let path = repo_root.join("out/packages/inventory.toml");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = toml::to_string_pretty(inventory)?;
    if !fs::read_to_string(&path).is_ok_and(|existing| existing == body) {
        fs::write(path, body)?;
    }
    Ok(())
}

fn read_inventory(repo_root: &Path) -> Result<PackageInventory> {
    let path = repo_root.join("out/packages/inventory.toml");
    toml::from_str(&fs::read_to_string(&path)?)
        .with_context(|| format!("failed to read {}", path.display()))
}

fn print_inventory(repo_root: &Path) -> Result<()> {
    let inventory = read_inventory(repo_root)?;
    println!(
        "{:<22} {:<19} {:<6} {:<10} {}",
        "PACKAGE", "VERSION", "ARCH", "FILES", "SHA256 / ARTIFACT"
    );
    for package in inventory.package {
        println!(
            "{:<22} {:<19} {:<6} {:<10} {}  {}",
            package.name,
            package.version,
            package.architecture,
            package.file_count,
            package.sha256,
            package.artifact_path
        );
        println!(
            "  source={} depends={} runtime-libraries={}",
            package.source_component,
            if package.dependencies.is_empty() {
                "<none>".to_string()
            } else {
                package.dependencies.join(",")
            },
            if package.runtime_libraries.is_empty() {
                "<none>".to_string()
            } else {
                package.runtime_libraries.join(",")
            }
        );
    }
    Ok(())
}

fn inspect_package(repo_root: &Path, name: &str) -> Result<()> {
    validate_package_name(name)?;
    let inventory = read_inventory(repo_root)?;
    let entry = inventory
        .package
        .iter()
        .find(|entry| entry.name == name)
        .ok_or_else(|| anyhow!("package {name} is not in the built inventory"))?;
    let spec = package_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| anyhow!("package definition for {name} is missing"))?;
    let staging = repo_root.join("out/packages/staging").join(name);
    let conffiles_path = staging.join("DEBIAN/conffiles");
    let conffiles = if conffiles_path.is_file() {
        fs::read_to_string(&conffiles_path)?
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut shared_libraries = Vec::new();
    walk_tree(&staging, &mut |path, metadata| {
        if !path.starts_with(staging.join("DEBIAN"))
            && !metadata.is_dir()
            && path
                .file_name()
                .and_then(|part| part.to_str())
                .is_some_and(|part| part.starts_with("lib") && part.contains(".so"))
        {
            shared_libraries.push(format!("/{}", path.strip_prefix(&staging)?.display()));
        }
        Ok(())
    })?;
    let repository_packages =
        repo_root.join("out/repository/dists/trixie/main/binary-amd64/Packages");
    let repository_resolution = if repository_packages.is_file() {
        repository::validate_repository_packages(&fs::read_to_string(repository_packages)?)?;
        "valid"
    } else {
        "not generated"
    };
    println!(
        "package: {}\nversion: {}\narchitecture: {}\nessential: {}\npriority: {}\nartifact: {}\nsource: {}\ndepends: {}\nprovides: {}\nconflicts: {}\nreplaces: {}\nconffiles: {}\nELF dependencies: {}\npackage-owned shared libraries: {}\nrepository dependency resolution: {}\nfiles: {}\nsha256: {}",
        entry.name,
        entry.version,
        entry.architecture,
        if spec.essential { "yes" } else { "no" },
        spec.priority,
        entry.artifact_path,
        entry.source_component,
        if entry.dependencies.is_empty() {
            "<none>".to_string()
        } else {
            entry.dependencies.join(", ")
        },
        if spec.provides.is_empty() {
            "<none>".to_string()
        } else {
            spec.provides.join(", ")
        },
        if spec.conflicts.is_empty() {
            "<none>".to_string()
        } else {
            spec.conflicts.join(", ")
        },
        if spec.replaces.is_empty() {
            "<none>".to_string()
        } else {
            spec.replaces.join(", ")
        },
        if conffiles.is_empty() {
            "<none>".to_string()
        } else {
            conffiles.join(", ")
        },
        if entry.runtime_libraries.is_empty() {
            "<none>".to_string()
        } else {
            entry.runtime_libraries.join(", ")
        },
        if shared_libraries.is_empty() {
            "<none>".to_string()
        } else {
            shared_libraries.join(", ")
        },
        repository_resolution,
        entry.file_count,
        entry.sha256
    );
    let artifact = repo_root.join(&entry.artifact_path);
    run_cmd(repo_root, "dpkg-deb", &["--info", path_str(&artifact)?])?;
    run_cmd(repo_root, "dpkg-deb", &["--contents", path_str(&artifact)?])
}

mod repository;
pub(crate) use repository::{generate_repository, repository_stage_spec};

pub(crate) fn install_prototype_packages(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let inventory = read_inventory(repo_root)?;
    repository::validate_repository_against_inventory(
        &repo_root.join("out/repository"),
        &inventory,
    )?;
    install_package_set(repo_root, rootfs, &inventory, &live_package_install_order()?)?;
    validate_dpkg_database(rootfs)?;
    remove_dpkg_transaction_locks(rootfs)
}

/// Installs `order` (dependencies first) into the empty root `rootfs` with
/// the build host's dpkg, chrootless, as an unprivileged user.
fn install_package_set(
    repo_root: &Path,
    rootfs: &Path,
    inventory: &PackageInventory,
    order: &[&str],
) -> Result<()> {
    let admindir = rootfs.join("var/lib/dpkg");
    for rel in ["info", "updates", "triggers", "parts"] {
        fs::create_dir_all(admindir.join(rel))?;
    }
    fs::create_dir_all(rootfs.join("var/log"))?;
    fs::write(admindir.join("status"), "")?;
    fs::write(admindir.join("available"), "")?;
    // fakeroot lets the unprivileged image builder exercise normal dpkg mode and
    // ownership semantics, including unpacking read-only security conffiles.
    let mut command = Command::new("fakeroot");
    command
        .args(["--", "dpkg"])
        .arg(format!("--root={}", rootfs.display()))
        .arg(format!("--admindir={}", admindir.display()))
        .arg(format!(
            "--log={}",
            rootfs.join("var/log/dpkg.log").display()
        ))
        .args(["--force-bad-path", "--force-script-chrootless"])
        // The rootfs is a disposable build output that is validated and then
        // copied into images; per-file fsync (dpkg's default) only adds time
        // and SSD writes.  Image builders such as mmdebstrap do the same.
        .arg("--force-unsafe-io")
        .arg("--install");
    for name in order {
        let entry = inventory
            .package
            .iter()
            .find(|entry| entry.name == *name)
            .ok_or_else(|| anyhow!("package {name} is not in the package inventory"))?;
        command.arg(repo_root.join(&entry.artifact_path));
    }
    let status = command.status().context("failed to run dpkg")?;
    if !status.success() {
        bail!("dpkg package installation into {} failed with {status}", rootfs.display());
    }
    Ok(())
}

fn remove_dpkg_transaction_locks(rootfs: &Path) -> Result<()> {
    // dpkg creates empty advisory lock files even for an offline target root.
    // They are transaction state, not image payload, and a cached rootfs must
    // never retain them.
    for rel in [
        "var/lib/dpkg/lock",
        "var/lib/dpkg/lock-frontend",
        "var/lib/apt/lists/lock",
        "var/cache/apt/archives/lock",
    ] {
        remove_path_if_exists(&rootfs.join(rel))?;
    }
    // dpkg records wall-clock installation timestamps. Preserve that log when
    // installation fails for diagnostics, but initialize successful images
    // with empty mutable log state so rootfs and image bytes are reproducible.
    fs::write(rootfs.join("var/log/dpkg.log"), "")
        .context("failed to initialize deterministic dpkg log state")?;
    Ok(())
}

pub(crate) fn validate_dpkg_database(rootfs: &Path) -> Result<()> {
    let admindir = rootfs.join("var/lib/dpkg");
    for name in live_excluded_packages() {
        let output = Command::new("dpkg-query")
            .arg(format!("--admindir={}", admindir.display()))
            .args(["-W", "-f=${db:Status-Status}", name])
            .output()?;
        if String::from_utf8_lossy(&output.stdout) == "installed" {
            bail!("development package {name} was unpacked into the live root");
        }
    }
    for name in live_package_names() {
        let output = Command::new("dpkg-query")
            .arg(format!("--admindir={}", admindir.display()))
            .args(["-W", "-f=${db:Status-Status}", name])
            .output()?;
        if !output.status.success() || String::from_utf8_lossy(&output.stdout) != "installed" {
            bail!("dpkg database does not report {name} installed");
        }
    }
    for (path, owner) in [
        ("/usr/bin/brush", "mattos-brush"),
        ("/usr/bin/sh", "dash"),
        ("/usr/bin/dash", "dash"),
        ("/usr/bin/sed", "sed"),
        ("/usr/bin/grep", "grep"),
        ("/usr/bin/find", "findutils"),
        ("/usr/bin/diff", "diffutils"),
        ("/usr/libexec/mattos/rescue-init", "mattos-base-runtime"),
        ("/usr/bin/awk", "mawk"),
        ("/usr/bin/rsync", "rsync"),
        ("/usr/bin/bash", "mattos-brush"),
        ("/usr/bin/curl", "curl"),
        ("/usr/bin/ls", "coreutils"),
        ("/usr/bin/tar", "tar"),
        ("/usr/bin/dpkg", "dpkg"),
        ("/usr/bin/apt", "apt"),
        ("/usr/bin/apt-get", "apt"),
        ("/usr/bin/ldd", "libc-bin"),
        ("/usr/lib/apt/methods/file", "apt"),
        (
            "/usr/lib/x86_64-linux-gnu/libapt-pkg.so.7.0",
            "libapt-pkg7.0",
        ),
        ("/usr/lib/x86_64-linux-gnu/libgcc_s.so.1", "libgcc-s1"),
        ("/usr/lib/x86_64-linux-gnu/libstdc++.so.6", "libstdc++6"),
        ("/etc/ssl/certs/ca-certificates.crt", "ca-certificates"),
        ("/etc/ssl/cert.pem", "ca-certificates"),
        ("/usr/lib/x86_64-linux-gnu/libpam.so.0", "libpam0g"),
        ("/usr/lib/x86_64-linux-gnu/libncursesw.so.6", "libncursesw6"),
        ("/usr/lib/x86_64-linux-gnu/libpanelw.so.6", "libncursesw6"),
        ("/usr/lib/x86_64-linux-gnu/libkmod.so.2", "libkmod2"),
        ("/usr/lib/x86_64-linux-gnu/libproc2.so.1", "mattos-libproc2"),
        ("/usr/lib/udev/hwdb.bin", "udev"),
        (
            "/usr/lib/systemd/system/systemd-hwdb-update.service",
            "udev",
        ),
        ("/usr/lib/x86_64-linux-gnu/libexpat.so.1", "libexpat1"),
        ("/usr/lib/x86_64-linux-gnu/libfreetype.so.6", "libfreetype6"),
        (
            "/usr/lib/x86_64-linux-gnu/libfontconfig.so.1",
            "libfontconfig1",
        ),
        ("/usr/bin/fc-match", "fontconfig"),
        ("/usr/lib/x86_64-linux-gnu/libcap.so.2", "libcap2"),
        ("/usr/lib/x86_64-linux-gnu/libattr.so.1", "libattr1"),
        ("/usr/lib/x86_64-linux-gnu/libpcre2-8.so.0", "libpcre2-8-0"),
        ("/usr/lib/x86_64-linux-gnu/libselinux.so.1", "libselinux1"),
        ("/usr/lib/x86_64-linux-gnu/libcrypt.so.1", "libcrypt1"),
        ("/usr/lib/x86_64-linux-gnu/libacl.so.1", "libacl1"),
        ("/usr/lib/x86_64-linux-gnu/libz.so.1", "zlib1g"),
        ("/usr/lib/x86_64-linux-gnu/libbz2.so.1.0", "libbz2-1.0"),
        ("/usr/lib/x86_64-linux-gnu/liblz4.so.1", "liblz4-1"),
        ("/usr/lib/x86_64-linux-gnu/liblzma.so.5", "liblzma5"),
        ("/usr/lib/x86_64-linux-gnu/libxxhash.so.0", "libxxhash0"),
        ("/usr/lib/x86_64-linux-gnu/libmd.so.0", "libmd0"),
        ("/usr/lib/x86_64-linux-gnu/libbsd.so.0", "libbsd0"),
        ("/usr/bin/dbus-broker", "dbus-broker"),
        ("/usr/bin/sudo", "mattos-sudo-rs"),
        ("/usr/bin/passwd", "passwd"),
        ("/usr/bin/login", "login"),
        ("/usr/sbin/ip", "iproute2"),
        ("/usr/bin/ping", "iputils-ping"),
    ] {
        let output = Command::new("dpkg-query")
            .arg(format!("--admindir={}", admindir.display()))
            .args(["-S", path])
            .output()?;
        if !output.status.success() || !String::from_utf8_lossy(&output.stdout).starts_with(owner) {
            bail!("dpkg ownership query failed for {path}");
        }
    }
    Ok(())
}

pub(crate) fn package_owned_paths(rootfs: &Path) -> Result<BTreeSet<PathBuf>> {
    let admindir = rootfs.join("var/lib/dpkg/info");
    let mut owned = BTreeSet::new();
    for name in live_package_names() {
        let list = fs::read_to_string(admindir.join(format!("{name}.list")))?;
        for line in list.lines() {
            let rel = line.trim_start_matches('/');
            if !rel.is_empty() {
                owned.insert(PathBuf::from(rel));
            }
        }
    }
    Ok(owned)
}

pub(crate) fn reject_legacy_collision(
    owned: &BTreeSet<PathBuf>,
    destination_rel: &Path,
) -> Result<()> {
    let normalized = destination_rel.strip_prefix("/").unwrap_or(destination_rel);
    if owned.contains(normalized) {
        bail!(
            "legacy rootfs install would overwrite package-owned /{}",
            normalized.display()
        );
    }
    Ok(())
}

pub(crate) fn snapshot_package_files(
    rootfs: &Path,
    owned: &BTreeSet<PathBuf>,
) -> Result<BTreeMap<PathBuf, String>> {
    let mut snapshot = BTreeMap::new();
    for rel in owned {
        let path = rootfs.join(rel);
        let meta = fs::symlink_metadata(&path)
            .with_context(|| format!("package-owned path disappeared: /{}", rel.display()))?;
        let identity = if meta.file_type().is_symlink() {
            format!("symlink:{}", fs::read_link(&path)?.display())
        } else if meta.is_file() {
            format!("file:{}", sha256_file(&path)?)
        } else if meta.is_dir() {
            "directory".to_string()
        } else {
            format!("special:{:?}", meta.file_type())
        };
        snapshot.insert(rel.clone(), identity);
    }
    Ok(snapshot)
}

pub(crate) fn validate_package_snapshot(
    rootfs: &Path,
    expected: &BTreeMap<PathBuf, String>,
) -> Result<()> {
    let owned: BTreeSet<PathBuf> = expected.keys().cloned().collect();
    let actual = snapshot_package_files(rootfs, &owned)?;
    if actual != *expected {
        let changed = expected
            .iter()
            .find(|(path, identity)| actual.get(*path) != Some(*identity))
            .map(|(path, _)| path.display().to_string())
            .unwrap_or_else(|| "<unknown>".into());
        bail!("legacy rootfs assembly changed package-owned /{changed}")
    }
    Ok(())
}

/// Where the live root finds the offline package repository: on the live
/// medium beside the SquashFS (see `build_iso_atomic`), which `live-init`
/// keeps mounted at /run/mattos/medium.  The .debs are already compressed,
/// so packing them into the SquashFS only cost build time.
pub(crate) const LIVE_REPOSITORY_LINK_TARGET: &str = "/run/mattos/medium/mattos/repository";

pub(crate) fn embed_repository(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let source = repo_root.join("out/repository");
    if !source.join("dists/trixie/Release").is_file() {
        bail!("local repository has not been generated");
    }
    let link = rootfs.join("usr/share/mattos/repository");
    fs::create_dir_all(link.parent().context("repository link has no parent")?)?;
    remove_path_if_exists(&link)?;
    std::os::unix::fs::symlink(LIVE_REPOSITORY_LINK_TARGET, &link)?;
    Ok(())
}

pub(crate) fn build_dpkg(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/packages/dpkg");
    if !source.join("configure.ac").is_file() {
        bail!("dpkg source missing; run upstream import dpkg");
    }
    let out = repo_root.join("out/build/dpkg");
    let zlib = repo_root.join("out/build/zlib/install/usr");
    let bzip2 = repo_root.join("out/build/bzip2/install/usr");
    let xz = repo_root.join("out/build/xz/install/usr");
    let zstd = repo_root.join("out/build/zstd/install/usr");
    let libmd = repo_root.join("out/build/libmd/install/usr");
    let selinux = repo_root.join("out/build/selinux/install/usr");
    let pcre2 = repo_root.join("out/build/pcre2/install/usr");
    let zlib_lib = zlib.join("lib/x86_64-linux-gnu");
    let bzip2_lib = bzip2.join("lib/x86_64-linux-gnu");
    let xz_lib = xz.join("lib/x86_64-linux-gnu");
    let zstd_lib = zstd.join("lib/x86_64-linux-gnu");
    let libmd_lib = libmd.join("lib/x86_64-linux-gnu");
    let selinux_lib = selinux.join("lib/x86_64-linux-gnu");
    let pcre2_lib = pcre2.join("lib/x86_64-linux-gnu");
    let sysroot_pkgconfig = repo_root.join("out/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig");
    if !zlib_lib.join("libz.so").exists()
        || !bzip2_lib.join("libbz2.so").exists()
        || !xz_lib.join("liblzma.so").exists()
        || !zstd_lib.join("libzstd.so").exists()
        || !libmd_lib.join("libmd.so").exists()
        || !selinux_lib.join("libselinux.so").exists()
        || !pcre2_lib.join("libpcre2-8.so").exists()
    {
        bail!(
            "MattOS dpkg development libraries are missing; run build zlib, bzip2, xz, zstd, libmd, pcre2, and selinux first"
        )
    }
    hydrate_development_sysroot(
        repo_root,
        &[
            zlib.clone(),
            bzip2.clone(),
            xz.clone(),
            zstd.clone(),
            libmd.clone(),
            selinux.clone(),
            pcre2.clone(),
            repo_root.join("out/build/selinux/sepol-install/usr"),
        ],
    )?;
    let include_flags = format!(
        "-I{} -I{} -I{} -I{} -I{} -I{} -I{}",
        zlib.join("include").display(),
        bzip2.join("include").display(),
        xz.join("include").display(),
        zstd.join("include").display(),
        libmd.join("include").display(),
        selinux.join("include").display(),
        pcre2.join("include").display()
    );
    let link_flags = format!(
        "-L{} -L{} -L{} -L{} -L{} -L{} -L{}",
        zlib_lib.display(),
        bzip2_lib.display(),
        xz_lib.display(),
        zstd_lib.display(),
        libmd_lib.display(),
        selinux_lib.display(),
        pcre2_lib.display()
    );
    let library_path = std::env::join_paths([
        &zlib_lib,
        &bzip2_lib,
        &xz_lib,
        &zstd_lib,
        &libmd_lib,
        &selinux_lib,
        &pcre2_lib,
    ])?
    .to_string_lossy()
    .to_string();
    let pkgconfig_path = std::env::join_paths([
        zlib_lib.join("pkgconfig"),
        xz_lib.join("pkgconfig"),
        zstd_lib.join("pkgconfig"),
        libmd_lib.join("pkgconfig"),
        selinux_lib.join("pkgconfig"),
        pcre2_lib.join("pkgconfig"),
        sysroot_pkgconfig,
    ])?
    .to_string_lossy()
    .to_string();
    let dependency_env = [
        ("CPPFLAGS", include_flags),
        ("LDFLAGS", link_flags),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        ("PKG_CONFIG_PATH", pkgconfig_path.clone()),
        ("PKG_CONFIG_LIBDIR", pkgconfig_path),
        (
            "PKG_CONFIG_SYSROOT_DIR",
            repo_root.join("out/sysroot").display().to_string(),
        ),
    ];
    let source_copy = out.join("source");
    let build = out.join("build");
    let install = out.join("install");
    remove_path_if_exists(&source_copy)?;
    remove_path_if_exists(&build)?;
    remove_path_if_exists(&install)?;
    fs::create_dir_all(&out)?;
    sync_build_source(&source, &source_copy)?;
    stage_missing_dpkg_source_inputs(repo_root, &source_copy)?;
    let state = read_sync_state(repo_root, "dpkg")?
        .ok_or_else(|| anyhow!("upstream state missing for dpkg"))?;
    let changelog = fs::read_to_string(source_copy.join("debian/changelog"))?;
    let upstream_version = changelog
        .lines()
        .next()
        .and_then(|line| line.split_once('('))
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(version, _)| version)
        .ok_or_else(|| anyhow!("unable to derive dpkg version from debian/changelog"))?;
    let short_commit = state
        .imported_commit
        .get(..8)
        .unwrap_or(&state.imported_commit);
    fs::write(
        source_copy.join(".dist-version"),
        format!("{upstream_version}+git.{short_commit}\n"),
    )?;
    fs::write(
        source_copy.join(".dist-vcs-id"),
        format!("{}\n", state.imported_commit),
    )?;
    run_cmd(&source_copy, "./autogen", &[])?;
    fs::create_dir_all(&build)?;
    let configure = source_copy.join("configure");
    run_cmd_with_env_overrides(
        &build,
        path_str(&configure)?,
        &[
            "--prefix=/usr",
            "--sysconfdir=/etc",
            "--localstatedir=/var",
            "--libexecdir=/usr/libexec",
            "--disable-dselect",
            "--disable-nls",
            "--with-libselinux",
        ],
        &dependency_env,
    )?;
    run_cmd_with_env_overrides(&build, "make", &["-j", "4"], &dependency_env)?;
    fs::create_dir_all(&install)?;
    run_cmd_with_env_overrides(
        &build,
        "make",
        &["install", &format!("DESTDIR={}", install.display())],
        &dependency_env,
    )?;
    for rel in [
        "usr/bin/dpkg",
        "usr/bin/dpkg-query",
        "usr/bin/dpkg-deb",
        "usr/sbin/start-stop-daemon",
        "usr/bin/update-alternatives",
    ] {
        if !install.join(rel).is_file() {
            bail!("dpkg build did not produce {rel}");
        }
    }
    let dpkg_deb = install.join("usr/bin/dpkg-deb");
    let compression_libs: [&Path; 4] = [&zlib_lib, &bzip2_lib, &xz_lib, &zstd_lib];
    validate_dependency_resolves_from(&dpkg_deb, "libz.so.1", &zlib_lib, &compression_libs)?;
    validate_dependency_resolves_from(&dpkg_deb, "libbz2.so.1.0", &bzip2_lib, &compression_libs)?;
    validate_dependency_resolves_from(&dpkg_deb, "liblzma.so.5", &xz_lib, &compression_libs)?;
    validate_dependency_resolves_from(&dpkg_deb, "libzstd.so.1", &zstd_lib, &compression_libs)?;
    let dpkg_lib_dirs: [&Path; 7] = [
        &zlib_lib,
        &bzip2_lib,
        &xz_lib,
        &zstd_lib,
        &libmd_lib,
        &selinux_lib,
        &pcre2_lib,
    ];
    for rel in [
        "usr/bin/dpkg",
        "usr/bin/dpkg-deb",
        "usr/bin/dpkg-divert",
        "usr/bin/dpkg-query",
        "usr/bin/dpkg-realpath",
        "usr/bin/dpkg-split",
        "usr/bin/dpkg-statoverride",
        "usr/bin/dpkg-trigger",
    ] {
        validate_dependency_resolves_from(
            &install.join(rel),
            "libmd.so.0",
            &libmd_lib,
            &dpkg_lib_dirs,
        )?;
    }
    for rel in ["usr/bin/dpkg", "usr/bin/dpkg-statoverride"] {
        validate_dependency_resolves_from(
            &install.join(rel),
            "libselinux.so.1",
            &selinux_lib,
            &dpkg_lib_dirs,
        )?;
    }
    println!(
        "dpkg origins: zlib={} bzip2={} liblzma={} libzstd={} libmd={} libselinux={} pcre2={}",
        zlib_lib.display(),
        bzip2_lib.display(),
        xz_lib.display(),
        zstd_lib.display(),
        libmd_lib.display(),
        selinux_lib.display(),
        pcre2_lib.display()
    );
    println!("built imported dpkg into {}", install.display());
    Ok(())
}

fn stage_missing_dpkg_source_inputs(repo_root: &Path, source_copy: &Path) -> Result<()> {
    let cache = repo_root.join("out/cache/dpkg").join(DPKG_UPSTREAM_COMMIT);
    fs::create_dir_all(&cache).with_context(|| format!("failed to create {}", cache.display()))?;

    let mut fetch_required = false;
    for input in DPKG_MISSING_SOURCE_INPUTS {
        let cached = cache.join(input.path);
        if cached.is_file() {
            let actual = sha256_file(&cached)?;
            if actual != input.sha256 {
                bail!(
                    "cached dpkg source input checksum mismatch for {}: expected {}, got {}",
                    input.path,
                    input.sha256,
                    actual
                );
            }
        } else {
            fetch_required = true;
        }
    }

    let git_dir = repo_root.join("out/cache/dpkg/upstream.git");
    let git_dir_arg = format!("--git-dir={}", git_dir.display());
    if fetch_required {
        if !git_dir.is_dir() {
            run_cmd(repo_root, "git", &["init", "--bare", path_str(&git_dir)?])?;
        }
        run_cmd(
            repo_root,
            "git",
            &[
                git_dir_arg.as_str(),
                "fetch",
                "--depth=1",
                DPKG_UPSTREAM_REPOSITORY,
                DPKG_UPSTREAM_COMMIT,
            ],
        )?;
    }

    for input in DPKG_MISSING_SOURCE_INPUTS {
        let cached = cache.join(input.path);
        if !cached.is_file() {
            let object = format!("{DPKG_UPSTREAM_COMMIT}:{}", input.path);
            let output = Command::new("git")
                .args([git_dir_arg.as_str(), "show", object.as_str()])
                .output()
                .with_context(|| {
                    format!("failed to read {} from pinned dpkg commit", input.path)
                })?;
            if !output.status.success() {
                bail!(
                    "pinned dpkg commit did not provide {}: {}",
                    input.path,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            if let Some(parent) = cached.parent() {
                fs::create_dir_all(parent)?;
            }
            let temp = cached.with_extension("tmp");
            fs::write(&temp, &output.stdout)
                .with_context(|| format!("failed to write {}", temp.display()))?;
            let actual = sha256_file(&temp)?;
            if actual != input.sha256 {
                let _ = fs::remove_file(&temp);
                bail!(
                    "downloaded dpkg source input checksum mismatch for {}: expected {}, got {}",
                    input.path,
                    input.sha256,
                    actual
                );
            }
            fs::rename(&temp, &cached)
                .with_context(|| format!("failed to publish {}", cached.display()))?;
        }

        let destination = source_copy.join(input.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&cached, &destination).with_context(|| {
            format!(
                "failed to stage pinned dpkg source input {} into output-owned source mirror",
                input.path
            )
        })?;
    }
    Ok(())
}

pub(crate) fn build_apt(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/packages/apt");
    if !source.join("CMakeLists.txt").is_file() {
        bail!("APT source missing; run upstream import apt");
    }
    let out = repo_root.join("out/build/apt");
    let zlib = repo_root.join("out/build/zlib/install/usr");
    let bzip2 = repo_root.join("out/build/bzip2/install/usr");
    let lz4 = repo_root.join("out/build/lz4/install/usr");
    let xz = repo_root.join("out/build/xz/install/usr");
    let xxhash = repo_root.join("out/build/xxhash/install/usr");
    let zstd = repo_root.join("out/build/zstd/install/usr");
    let openssl = repo_root.join("out/build/openssl/install/usr");
    let systemd = repo_root.join("out/build/systemd/install/usr");
    let zlib_lib = zlib.join("lib/x86_64-linux-gnu");
    let bzip2_lib = bzip2.join("lib/x86_64-linux-gnu");
    let lz4_lib = lz4.join("lib/x86_64-linux-gnu");
    let xz_lib = xz.join("lib/x86_64-linux-gnu");
    let xxhash_lib = xxhash.join("lib/x86_64-linux-gnu");
    let zstd_lib = zstd.join("lib/x86_64-linux-gnu");
    let openssl_lib = openssl.join("lib/x86_64-linux-gnu");
    let systemd_lib = systemd.join("lib/x86_64-linux-gnu");
    if !zlib_lib.join("libz.so").exists()
        || !bzip2_lib.join("libbz2.so").exists()
        || !lz4_lib.join("liblz4.so").exists()
        || !xz_lib.join("liblzma.so").exists()
        || !xxhash_lib.join("libxxhash.so").exists()
        || !zstd_lib.join("libzstd.so").exists()
        || !openssl_lib.join("libcrypto.so").exists()
        || !openssl_lib.join("libssl.so").exists()
        || !systemd.join("include/libudev.h").is_file()
        || !systemd_lib.join("libudev.so").exists()
    {
        bail!(
            "MattOS APT development files are missing; run build zlib, bzip2, lz4, xz, xxhash, zstd, openssl, and systemd first"
        )
    }
    hydrate_development_sysroot(
        repo_root,
        &[
            zlib.clone(),
            bzip2.clone(),
            lz4.clone(),
            xz.clone(),
            xxhash.clone(),
            zstd.clone(),
            openssl.clone(),
            systemd.clone(),
        ],
    )?;
    let source_copy = out.join("source");
    let build = out.join("build");
    let install = out.join("install");
    remove_path_if_exists(&source_copy)?;
    remove_path_if_exists(&build)?;
    remove_path_if_exists(&install)?;
    fs::create_dir_all(&out)?;
    copy_imported_working_tree(
        repo_root,
        Path::new("src/system/packages/apt"),
        &source_copy,
    )?;
    apply_component_patches(repo_root, "apt", &source_copy)?;
    let zlib_root = format!("-DZLIB_ROOT={}", zlib.display());
    let bzip2_include = format!("-DBZIP2_INCLUDE_DIR={}", bzip2.join("include").display());
    let bzip2_library = format!(
        "-DBZIP2_LIBRARY_RELEASE={}",
        bzip2_lib.join("libbz2.so").display()
    );
    let lz4_include = format!("-DLZ4_INCLUDE_DIRS={}", lz4.join("include").display());
    let lz4_library = format!("-DLZ4_LIBRARIES={}", lz4_lib.join("liblz4.so").display());
    let lzma_include = format!("-DLZMA_INCLUDE_DIRS={}", xz.join("include").display());
    let lzma_library = format!("-DLZMA_LIBRARIES={}", xz_lib.join("liblzma.so").display());
    let xxhash_include = format!("-DXXHASH_INCLUDE_DIRS={}", xxhash.join("include").display());
    let xxhash_library = format!(
        "-DXXHASH_LIBRARIES={}",
        xxhash_lib.join("libxxhash.so").display()
    );
    let zstd_include = format!("-DZSTD_INCLUDE_DIRS={}", zstd.join("include").display());
    let zstd_library = format!("-DZSTD_LIBRARIES={}", zstd_lib.join("libzstd.so").display());
    let openssl_include = format!(
        "-DOPENSSL_INCLUDE_DIR={}",
        openssl.join("include").display()
    );
    let openssl_crypto = format!(
        "-DOPENSSL_CRYPTO_LIBRARY={}",
        openssl_lib.join("libcrypto.so").display()
    );
    let openssl_ssl = format!(
        "-DOPENSSL_SSL_LIBRARY={}",
        openssl_lib.join("libssl.so").display()
    );
    let udev_include = format!("-DUDEV_INCLUDE_DIRS={}", systemd.join("include").display());
    let udev_library = format!(
        "-DUDEV_LIBRARIES={}",
        systemd_lib.join("libudev.so").display()
    );
    let iconv_include = format!(
        "-DICONV_INCLUDE_DIR={}",
        repo_root.join("out/sysroot/usr/include").display()
    );
    let library_path = std::env::join_paths([
        &zlib_lib,
        &bzip2_lib,
        &lz4_lib,
        &xz_lib,
        &xxhash_lib,
        &zstd_lib,
        &openssl_lib,
        &systemd_lib,
    ])?
    .to_string_lossy()
    .to_string();
    let pkgconfig_path = std::env::join_paths([
        zlib_lib.join("pkgconfig"),
        lz4_lib.join("pkgconfig"),
        xz_lib.join("pkgconfig"),
        xxhash_lib.join("pkgconfig"),
        zstd_lib.join("pkgconfig"),
        openssl_lib.join("pkgconfig"),
        systemd_lib.join("pkgconfig"),
    ])?
    .to_string_lossy()
    .to_string();
    let include_flags = [&zlib, &bzip2, &lz4, &xz, &xxhash, &zstd, &openssl, &systemd]
        .iter()
        .map(|install| format!("-I{}", install.join("include").display()))
        .collect::<Vec<_>>()
        .join(" ");
    let link_flags = [
        &zlib_lib,
        &bzip2_lib,
        &lz4_lib,
        &xz_lib,
        &xxhash_lib,
        &zstd_lib,
        &openssl_lib,
        &systemd_lib,
    ]
    .iter()
    .flat_map(|library| {
        [
            format!("-L{}", library.display()),
            format!("-Wl,-rpath-link,{}", library.display()),
        ]
    })
    .collect::<Vec<_>>()
    .join(" ");
    let dependency_env = [
        ("CPPFLAGS", include_flags.clone()),
        ("CFLAGS", include_flags.clone()),
        ("CXXFLAGS", include_flags),
        ("LDFLAGS", link_flags),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path.clone()),
        ("PKG_CONFIG_PATH", pkgconfig_path.clone()),
        ("PKG_CONFIG_LIBDIR", pkgconfig_path),
        (
            "PKG_CONFIG_SYSROOT_DIR",
            repo_root.join("out/sysroot").display().to_string(),
        ),
    ];
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &[
            "-S",
            path_str(&source_copy)?,
            "-B",
            path_str(&build)?,
            "-G",
            "Ninja",
            "-DCMAKE_BUILD_TYPE=Release",
            // CMake otherwise reserves a checkout-path-sized build RPATH and
            // replaces it with NUL padding during install.  The installed code
            // is the same, but section offsets and GNU build IDs then vary with
            // the length of the checkout path.
            "-DCMAKE_SKIP_RPATH=ON",
            "-DCMAKE_INSTALL_PREFIX=/usr",
            "-DCMAKE_INSTALL_SYSCONFDIR=/etc",
            "-DCURRENT_VENDOR=mattos",
            "-DCOMMON_ARCH=amd64",
            "-DDPKG_DATADIR=/usr/share/dpkg",
            "-DWITH_DOC=OFF",
            "-DWITH_TESTS=OFF",
            "-DWITH_FTPARCHIVE=OFF",
            "-DUSE_NLS=OFF",
            // Do not let CMake discover the host libseccomp while compiling
            // against the MattOS sysroot.  MattOS does not publish a
            // target-owned libseccomp development interface yet, and APT's
            // seccomp sandbox is optional.
            "-DCMAKE_DISABLE_FIND_PACKAGE_SECCOMP=TRUE",
            &zlib_root,
            &bzip2_include,
            &bzip2_library,
            &lz4_include,
            &lz4_library,
            &lzma_include,
            &lzma_library,
            &xxhash_include,
            &xxhash_library,
            &zstd_include,
            &zstd_library,
            &openssl_include,
            &openssl_crypto,
            &openssl_ssl,
            &udev_include,
            &udev_library,
            &iconv_include,
        ],
        &dependency_env,
    )?;
    let cache = fs::read_to_string(build.join("CMakeCache.txt"))?;
    if !cache.lines().any(|line| line == "CMAKE_SKIP_RPATH:BOOL=ON") {
        bail!("APT build did not disable checkout-dependent CMake RPATH padding")
    }
    for expected in [
        format!("ZLIB_INCLUDE_DIR:PATH={}", zlib.join("include").display()),
        format!(
            "ZLIB_LIBRARY_RELEASE:FILEPATH={}",
            zlib_lib.join("libz.so").display()
        ),
        format!("BZIP2_INCLUDE_DIR:PATH={}", bzip2.join("include").display()),
        format!(
            "BZIP2_LIBRARY_RELEASE:FILEPATH={}",
            bzip2_lib.join("libbz2.so").display()
        ),
        format!("LZ4_INCLUDE_DIRS:PATH={}", lz4.join("include").display()),
        format!(
            "LZ4_LIBRARIES:FILEPATH={}",
            lz4_lib.join("liblz4.so").display()
        ),
        format!("LZMA_INCLUDE_DIRS:PATH={}", xz.join("include").display()),
        format!(
            "LZMA_LIBRARIES:FILEPATH={}",
            xz_lib.join("liblzma.so").display()
        ),
        format!(
            "XXHASH_INCLUDE_DIRS:PATH={}",
            xxhash.join("include").display()
        ),
        format!(
            "XXHASH_LIBRARIES:FILEPATH={}",
            xxhash_lib.join("libxxhash.so").display()
        ),
        format!("ZSTD_INCLUDE_DIRS:PATH={}", zstd.join("include").display()),
        format!(
            "ZSTD_LIBRARIES:FILEPATH={}",
            zstd_lib.join("libzstd.so").display()
        ),
        format!(
            "OPENSSL_INCLUDE_DIR:PATH={}",
            openssl.join("include").display()
        ),
        format!(
            "OPENSSL_CRYPTO_LIBRARY:FILEPATH={}",
            openssl_lib.join("libcrypto.so").display()
        ),
        format!(
            "OPENSSL_SSL_LIBRARY:FILEPATH={}",
            openssl_lib.join("libssl.so").display()
        ),
        format!(
            "UDEV_INCLUDE_DIRS:PATH={}",
            systemd.join("include").display()
        ),
        format!(
            "UDEV_LIBRARIES:FILEPATH={}",
            systemd_lib.join("libudev.so").display()
        ),
        format!(
            "ICONV_INCLUDE_DIR:PATH={}",
            repo_root.join("out/sysroot/usr/include").display()
        ),
    ] {
        if !cache.lines().any(|line| line == expected) {
            bail!(
                "APT resolved an unexpected host compression dependency; missing cache entry {expected}"
            )
        }
    }
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--build", path_str(&build)?, "--parallel", "4"],
        &dependency_env,
    )?;
    fs::create_dir_all(&install)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build)?],
        &[
            ("DESTDIR", install.display().to_string()),
            ("LD_LIBRARY_PATH", library_path.clone()),
        ],
    )?;
    for rel in ["usr/bin/apt", "usr/bin/apt-cache", "usr/bin/apt-get"] {
        if !install.join(rel).is_file() {
            bail!("APT build did not produce {rel}");
        }
    }
    let libapt_pkg = install.join("usr/lib/x86_64-linux-gnu/libapt-pkg.so.7.0.0");
    let dependency_libs: [&Path; 8] = [
        &zlib_lib,
        &bzip2_lib,
        &lz4_lib,
        &xz_lib,
        &xxhash_lib,
        &zstd_lib,
        &openssl_lib,
        &systemd_lib,
    ];
    validate_dependency_resolves_from(&libapt_pkg, "libz.so.1", &zlib_lib, &dependency_libs)?;
    validate_dependency_resolves_from(&libapt_pkg, "libbz2.so.1.0", &bzip2_lib, &dependency_libs)?;
    validate_dependency_resolves_from(&libapt_pkg, "liblz4.so.1", &lz4_lib, &dependency_libs)?;
    validate_dependency_resolves_from(&libapt_pkg, "liblzma.so.5", &xz_lib, &dependency_libs)?;
    validate_dependency_resolves_from(
        &libapt_pkg,
        "libxxhash.so.0",
        &xxhash_lib,
        &dependency_libs,
    )?;
    validate_dependency_resolves_from(&libapt_pkg, "libzstd.so.1", &zstd_lib, &dependency_libs)?;
    validate_dependency_resolves_from(
        &libapt_pkg,
        "libcrypto.so.3",
        &openssl_lib,
        &dependency_libs,
    )?;
    validate_dependency_resolves_from(&libapt_pkg, "libudev.so.1", &systemd_lib, &dependency_libs)?;
    println!(
        "APT dependency origins: zlib={} bzip2={} lz4={} liblzma={} xxhash={} zstd={} OpenSSL={} libudev={}",
        zlib_lib.display(),
        bzip2_lib.display(),
        lz4_lib.display(),
        xz_lib.display(),
        xxhash_lib.display(),
        zstd_lib.display(),
        openssl_lib.display(),
        systemd_lib.display()
    );
    println!("built imported APT into {}", install.display());
    Ok(())
}

fn relative_display(root: &Path, path: &Path) -> Result<String> {
    Ok(path.strip_prefix(root)?.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests;

/// Builds, verifies, and records one changed package artifact.
fn build_package_artifact(repo_root: &Path, package: PreparedPackage) -> Result<PackageInventoryEntry> {
            normalize_tree_timestamps(&package.staging)?;
    let staging_arg = path_str(&package.staging)?;
    let artifact_arg = path_str(&package.artifact)?;
    performance::timed(
        &format!("deb:{}", package.spec.name),
        "miss",
        "package payload changed; creating deterministic zstd level 19 archive",
        &package.input.cache_key,
        || {
            let status = Command::new("dpkg-deb")
                .args([
                    "--root-owner-group",
                    "-Zzstd",
                    "-z19",
                    "--build",
                    staging_arg,
                    artifact_arg,
                ])
                .env("SOURCE_DATE_EPOCH", SOURCE_DATE_EPOCH.to_string())
                .status()
                .context("failed to run dpkg-deb")?;
            if !status.success() {
                bail!("dpkg-deb failed for {} with {status}", package.spec.name)
            }
            Ok(())
        },
    )?;
    verify_deb(&package.artifact, package.spec.name, &package.version)?;
    let entry = PackageInventoryEntry {
        name: package.spec.name.to_string(),
        version: package.version.clone(),
        architecture: ARCH.to_string(),
        artifact_path: relative_display(repo_root, &package.artifact)?,
        source_component: package.spec.source_component.to_string(),
        dependencies: package_dependencies(repo_root, &package.spec)?,
        runtime_libraries: runtime_libraries_for_spec(repo_root, &package.spec)?,
        file_count: count_package_entries(&package.staging)?,
        sha256: sha256_file(&package.artifact)?,
    };
    let manifest = PackageCacheManifest {
        schema_version: PACKAGE_CACHE_SCHEMA_VERSION,
        package: package.spec.name.to_string(),
        cache_key: package.input.cache_key,
        definition_digest: package.input.definition_digest,
        payload_source_digest: package.input.payload_source_digest,
        payload_configuration_digest: package.input.payload_configuration_digest,
        dependency_digest: package.input.dependency_digest,
        payload_inventory_digest: performance::output_path_digest(
            repo_root,
            &package.staging,
        )?,
        artifact_sha256: entry.sha256.clone(),
        artifact_path: entry.artifact_path.clone(),
        inventory_entry: entry.clone(),
    };
    performance::atomic_write_json(
        &package_cache_manifest_path(repo_root, package.spec.name),
        &manifest,
    )?;
    Ok(entry)
}

/// Parallel `dpkg-deb` workers: one per CPU, but bounded by available memory.
/// zstd level 19 on the largest packages (LLVM, Qt, kernel modules) peaks at
/// about 1.3 GiB per multithreaded `dpkg-deb`; unbounded parallelism drove
/// the host into critical memory pressure.
fn package_build_workers() -> usize {
    const MEMORY_PER_WORKER_BYTES: u64 = 1536 * 1024 * 1024;
    let cpus = std::thread::available_parallelism().map_or(1, usize::from);
    let available = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|meminfo| {
            meminfo.lines().find_map(|line| {
                line.strip_prefix("MemAvailable:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
        })
        .map(|kib| kib * 1024);
    package_workers_for(cpus, available, MEMORY_PER_WORKER_BYTES)
}

fn package_workers_for(cpus: usize, available_memory: Option<u64>, per_worker: u64) -> usize {
    let by_memory = available_memory
        .map_or(cpus, |bytes| usize::try_from(bytes / per_worker.max(1)).unwrap_or(cpus));
    cpus.min(by_memory).max(1)
}

#[cfg(test)]
mod package_worker_tests;
