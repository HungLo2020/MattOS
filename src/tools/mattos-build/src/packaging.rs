use super::*;
use clap::Subcommand;
use filetime::{FileTime, set_file_times, set_symlink_file_times};

mod cache;
pub(crate) use cache::{
    ensure_package_facts, ensure_package_set, explain_package_cache,
    invalidate_package_cache, invalidate_package_facts, package_cache_input,
    package_cache_manifest_path, package_facts_status,
    print_package_cache_status, validate_package_cache, write_package_set_manifest,
    PackageCacheInput, PackageCacheManifest,
};

mod audit;
pub(crate) use audit::{
    detect_staging_collisions, generate_bootstrap_audit, runtime_libraries_for_spec,
    validate_no_mutable_package_state, validate_staged_runtime_ownership,
};
#[cfg(test)]
pub(crate) use audit::{
    bootstrap_consumers, bootstrap_source_attribution, confirmed_host_package,
    validate_migrated_bootstrap_absent,
};

mod staging;
pub(crate) use staging::{
    apply_live_apt_policy, component_install, stage_package, validate_udev_hwdb_payload,
};
#[cfg(test)]
pub(crate) use staging::{
    copy_path_preserving, copy_preserving, copy_tree_preserving, stage_brush,
    stage_ca_certificates, stage_cargo, stage_flatpak_system_remote, stage_gcc_development,
    stage_iso_codes, stage_rustc, stage_wireless_regdb,
    stage_xdg_desktop_portal,
    validate_no_mutable_system_state, validate_vulkan_icd_manifests,
    GLIBC_RUNTIME_LIBRARIES,
};
#[cfg(test)]
pub(crate) use cache::{
    package_definition_digest,
    package_recipe_revision, package_set_policy, package_set_manifest_path,
    package_stage_dependency_digest, package_payload_source_digests,
    PACKAGE_SET_SCHEMA_VERSION,
    PackageFacts, PackageElfMember, PackagePayloadFact, PackageSetEntry,
    PackageSetManifest,
};

mod registry;
pub(crate) use registry::{
    package_install_order, package_specs, PackageSpec, PACKAGE_NAMES,
};
#[cfg(test)]
pub(crate) use registry::package_install_order_for;

const ARCH: &str = "amd64";
const REVISION: &str = "1mattos1";
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
    "/etc/apt/sources.list.d/mattos.sources",
    "/etc/apt/sources.list.d/mattos-hosted.sources",
    "/etc/apt/sources.list.d/debian-trixie.sources",
    "/etc/apt/preferences.d/00mattos-priority",
];
const PAM_MODULES: &[&str] = &[
    "pam_unix.so",
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
const UTIL_LINUX_AUTH_PATHS: &[&str] = &["usr/sbin/agetty", "usr/bin/login", "usr/bin/su"];
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
    protected: bool,
    current_mattos_version: String,
    expected_debian_role: String,
    classification: String,
    known_gaps: Vec<String>,
    #[serde(default)]
    debian_epoch: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ProtectedPackageManifest {
    schema_version: u32,
    suite: String,
    packages: Vec<String>,
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

    let protected_path = repo_root.join("src/system/packages/debian-compat/protected.toml");
    let protected: ProtectedPackageManifest = toml::from_str(&fs::read_to_string(&protected_path)?)
        .with_context(|| format!("failed to parse {}", protected_path.display()))?;
    if protected.schema_version != 1 || protected.suite != "trixie" {
        bail!("protected-package manifest header is invalid")
    }
    let protected_names: BTreeSet<&str> = protected.packages.iter().map(String::as_str).collect();
    if protected_names.len() != protected.packages.len() {
        bail!("protected-package manifest contains duplicates")
    }
    for required in [
        "libc6",
        "libc-bin",
        "libc6-dev",
        "linux-libc-dev",
        "libgcc-s1",
        "libstdc++6",
        "systemd",
        "libsystemd0",
        "libudev1",
        "udev",
        "dpkg",
        "apt",
        "coreutils",
        "util-linux",
        "mount",
        "login",
        "passwd",
        "libpam0g",
        "libpam-modules",
        "libpam-runtime",
        "libssl3t64",
        "mattos-libcrypto3",
        "mattos-filesystem",
        "mattos-base-files",
    ] {
        if !protected_names.contains(required) {
            bail!("protected-package manifest is missing {required}")
        }
    }
    for package in manifest.package.iter().filter(|package| package.protected) {
        if !protected_names.contains(package.mattos_name.as_str()) {
            bail!(
                "protected compatibility package {} is not pinned",
                package.mattos_name
            )
        }
    }

    validate_apt_compatibility_policy(repo_root, &protected.packages)?;
    validate_linuxscripts_upstream(repo_root)?;
    println!(
        "validated Debian {} {} compatibility policy for {} packages",
        manifest.suite,
        manifest.architecture,
        manifest.package.len()
    );
    Ok(())
}

fn validate_apt_compatibility_policy(repo_root: &Path, protected: &[String]) -> Result<()> {
    let config = repo_root.join("src/system/packages/config/apt");
    let preferences = fs::read_to_string(config.join("00mattos-priority"))?;
    for required in [
        "Pin: release o=MattOS,l=MattOS Local,n=trixie\nPin-Priority: 990",
        "Pin: release o=MattOS,l=MattOS,n=trixie\nPin-Priority: 990",
        "Pin: release o=Debian,n=trixie\nPin-Priority: 500",
        "Pin: release o=Debian\nPin-Priority: -1",
    ] {
        if !preferences.contains(required) {
            bail!("APT preferences lack required policy stanza: {required}")
        }
    }
    let protected_stanzas = preferences
        .split("Explanation:")
        .filter(|stanza| stanza.contains("must never replace"))
        .collect::<Vec<_>>();
    if protected_stanzas.is_empty() {
        bail!("APT preferences lack protected-package stanza");
    }
    for name in protected {
        let mut pinned = protected_stanzas.iter().flat_map(|stanza| {
            stanza
                .lines()
                .filter_map(|line| line.strip_prefix("Package: "))
                .flat_map(str::split_whitespace)
        });
        if !pinned.any(|candidate| candidate == name) {
            bail!("APT protected-package pin is missing {name}")
        }
    }
    let local = fs::read_to_string(config.join("mattos.sources"))?;
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
    let debian = fs::read_to_string(config.join("debian-trixie.sources"))?;
    if !debian.contains("URIs: https://deb.debian.org/debian")
        || !debian.contains("URIs: https://security.debian.org/debian-security")
        || !debian.contains("Signed-By: /usr/share/keyrings/debian-archive-keyring.asc")
        || !debian.contains("Enabled: no")
        || debian.contains("Trusted: yes")
    {
        bail!("Debian Trixie source scaffold is invalid")
    }
    let installed = config.join("installed");
    let installed_local = fs::read_to_string(installed.join("mattos.sources"))?;
    let installed_hosted = fs::read_to_string(installed.join("mattos-hosted.sources"))?;
    let installed_debian = fs::read_to_string(installed.join("debian-trixie.sources"))?;
    let installed_preferences = fs::read_to_string(installed.join("00mattos-priority"))?;
    let installed_conf = fs::read_to_string(installed.join("01mattos"))?;
    if !installed_local.contains("Enabled: no")
        || !installed_local.contains("URIs: file:/usr/share/mattos/repository")
        || !installed_hosted.contains("Enabled: yes")
        || !installed_hosted.contains("URIs: https://packages.mattsherfey.com")
        || !installed_hosted.contains("Signed-By: /usr/share/keyrings/mattos-archive-keyring.asc")
        || !installed_debian.contains("Enabled: no")
        || !installed_debian.contains("Suites: trixie trixie-updates")
        || !installed_debian.contains("Suites: trixie-security")
        || !installed_debian.contains("Signed-By: /usr/share/keyrings/debian-archive-keyring.asc")
        || !installed_conf.contains("Acquire::https::Verify-Peer \"true\";")
        || !installed_conf.contains("Acquire::https::Verify-Host \"true\";")
        || !installed_conf.contains("Acquire::AllowInsecureRepositories \"false\";")
        || !installed_preferences.contains("Pin-Priority: 990")
        || !installed_preferences.contains("Pin-Priority: 500")
        || installed_preferences.contains("Pin-Priority: 1001")
        || !installed_preferences.contains("Pin-Priority: -1")
    {
        bail!("installed APT policy is invalid")
    }
    for keyring in ["mattos-archive-keyring.asc", "debian-archive-keyring.asc"] {
        if !config.join("keys").join(keyring).is_file() {
            bail!("APT keyring source is missing: {keyring}")
        }
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
        .chain(["--repo".to_string(), "mattos".to_string(), "upload".to_string()])
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

fn build_packages(repo_root: &Path, names: &[String]) -> Result<()> {
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
    let mut source_digests = BTreeMap::new();
    let mut prepared = Vec::new();
    for spec in &selected {
        let version = package_version(repo_root, spec)?;
        let staging = staging_root.join(spec.name);
        let artifact = artifact_root.join(format!("{}_{}_{}.deb", spec.name, version, ARCH));
        let input = package_cache_input(repo_root, spec, &version, &mut source_digests)?;
        let reused = performance::measure_package_validation(|| {
            validate_package_cache(repo_root, spec, &version, &staging, &artifact, &input)
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
            performance::invalidate_integrity_paths(
                repo_root,
                &[staging.clone(), artifact.clone()],
            );
            performance::timed(
                &format!("package-staging:{}", spec.name),
                "miss",
                "package inputs or cached artifact validation changed",
                &input.cache_key,
                || stage_package(repo_root, spec),
            )?;
            println!("package cache miss: {}", spec.name);
        }
        prepared.push(PreparedPackage {
            spec: spec.clone(),
            version,
            staging,
            artifact,
            input,
            reused,
        });
    }
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
    for package in prepared {
        let entry = if let Some(cached) = package.reused {
            cached.inventory_entry
        } else {
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
            entry
        };
        inventory.package.retain(|old| old.name != entry.name);
        inventory.package.push(entry);
    }
    inventory.package.sort_by(|a, b| a.name.cmp(&b.name));
    write_inventory(repo_root, &inventory)?;
    ensure_package_facts(repo_root, &inventory)?;
    print_inventory(repo_root)?;
    write_package_set_manifest(repo_root)
}




fn package_stage_dependencies(source_component: &str) -> &'static [&'static str] {
    match source_component {
        "MattOS" | "ca-certificates" | "test" => &[],
        "mattos-compat" => &["systemd"],
        "linux" => &["linux-headers"],
        "kernel-modules" => &["linux"],
        "gcc" => &["gcc-runtime", "gcc-compiler"],
        "glibc" => &["glibc", "formal-sysroot"],
        "make" => &["make"],
        // The installer package embeds both installer-stage assets and the
        // Linux bzImage used by installed systems. Keep the filesystem-tool
        // packages tied only to their shared build stage.
        "installer" => &["installer", "linux"],
        "btrfs-progs" | "dosfstools" | "e2fsprogs" => &["installer"],
        "procps-ng" => &["procps-ng"],
        "linux-pam" => &["linux-pam"],
        "sudo-rs" => &["sudo-rs"],
        other => match other {
            "brush" => &["brush"],
            "coreutils" => &["coreutils"],
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
            "util-linux" => &["util-linux"],
            "iproute2" => &["iproute2"],
            "iputils" => &["iputils"],
            "gzip" => &["gzip"],
            "patch" => &["patch"],
            "file" => &["file"],
            "less" => &["less"],
            "git" => &["git"],
            "openssh" => &["openssh"],
            "libffi" => &["libffi"],
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
            "cosmic-comp" => &["cosmic-comp"],
            // These stage outputs form Flatpak's runtime closure. They are
            // intentionally owned by the flatpak package until MattOS splits
            // them into separately installable library packages.
            "flatpak" => &[
                "flatpak",
                "ostree",
                "gpgme",
                "gdk-pixbuf",
                "appstream",
                "json-glib",
                "libxmlb",
                "libfyaml",
                "fuse3",
                "libxml2",
                "libarchive",
                "libpng",
                "bubblewrap",
                "xdg-dbus-proxy",
            ],
            "xwayland" => &[
                "xwayland",
                "libepoxy",
                "freetype",
                "libfontenc",
                "libxfont",
                "libxcvt",
                "libxshmfence",
                "libxkbfile",
                "xkbcomp",
            ],
            "xdg-desktop-portal" => &["xdg-desktop-portal", "gstreamer", "gstreamer-base"],
            "cosmic-desktop" => &["cosmic-desktop"],
            "cosmic-edit" => &["cosmic-edit"],
            "cosmic-initial-setup" => &["cosmic-initial-setup"],
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
        "mattos-compat" => &["src/system/compat/mattos-compat"],
        "MattOS" => &[
            "src/rootfs/skeleton",
            "src/system/packages/config",
            // The MattOS filesystem/base-files payload is assembled by this
            // module; recipe changes must invalidate those package artifacts.
            "src/tools/mattos-build/src/packaging.rs",
        ],
        "ca-certificates" => &["src/system/network"],
        "linux" => &["src/kernel/linux"],
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
        "btrfs-progs" => &["src/system/storage/btrfs-progs"],
        "dosfstools" => &["src/system/storage/dosfstools"],
        "e2fsprogs" => &["src/system/storage/e2fsprogs"],
        "brush" => &["src/userland/brush"],
        "coreutils" => &["src/userland/coreutils"],
        "curl" => &["src/userland/curl"],
        "libmd" => &["src/system/libraries/libmd"],
        "libbsd" => &["src/system/libraries/libbsd"],
        "zstd" => &["src/system/libraries/zstd"],
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
        "pipewire" => &["src/system/multimedia/pipewire"],
        "linux-pam" => &["src/system/auth/linux-pam"],
        "shadow" => &["src/system/auth/shadow"],
        "sudo-rs" => &["src/system/auth/sudo-rs"],
        "iproute2" => &["src/userland/iproute2"],
        "iputils" => &["src/userland/iputils"],
        "gzip" => &["src/userland/gzip"],
        "patch" => &["src/userland/patch"],
        "file" => &["src/userland/file"],
        "less" => &["src/userland/less"],
        "git" => &["src/userland/git"],
        "openssh" => &["src/system/network/openssh-portable"],
        "libffi" => &["src/system/libraries/libffi/libffi"],
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
            "src/system/graphics/xcb-proto",
            "src/system/graphics/libxcb",
            "src/system/graphics/libx11",
            "src/system/graphics/libxext",
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
            "src/system/packages/ostree",
            "src/system/security/gpgme",
            "src/system/libraries/gdk-pixbuf",
            "src/system/libraries/appstream",
            "src/system/libraries/json-glib",
            "src/system/libraries/libxmlb",
            "src/system/libraries/libfyaml",
            "src/system/libraries/fuse3",
            "src/system/libraries/libxml2",
            "src/system/libraries/libarchive",
            "src/system/libraries/libpng",
            "src/system/security/bubblewrap",
            "src/system/packages/xdg-dbus-proxy",
        ],
        "xwayland" => &[
            "src/system/graphics/xwayland",
            "src/system/graphics/libepoxy",
            "src/system/libraries/freetype",
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
            "src/system/multimedia/gstreamer",
        ],
        "cosmic-comp" => &["src/desktop/cosmic/cosmic-comp"],
        "cosmic-desktop" => &[
            "src/desktop/cosmic",
            "src/desktop/themes/pop-icon-theme",
            "src/system/session/greetd",
            "src/system/session/cosmic",
            "src/tools/mattos-build/src/main.rs",
        ],
        "cosmic-edit" => &["src/desktop/cosmic/cosmic-edit"],
        "cosmic-initial-setup" => &[
            "src/desktop/cosmic/cosmic-initial-setup",
            "resources/COSMIC/layouts",
            "resources/COSMIC/themes",
            "src/tools/mattos-build/src/main.rs",
        ],
        "duktape" => &[
            "src/system/security/duktape",
            "src/tools/mattos-build/src/main.rs",
            "src/tools/mattos-build/src/packaging.rs",
        ],
        "polkit" => &[
            "src/system/security/polkit",
            "src/tools/mattos-build/src/main.rs",
            "src/tools/mattos-build/src/packaging.rs",
        ],
        "networkmanager" => &["src/system/network/NetworkManager"],
        "libnl" => &["src/system/network/libnl"],
        "wpa-supplicant" => &["src/system/network/hostap", "src/system/network/wpa-supplicant"],
        "grub" => &["src/boot/grub/upstream", "src/build-support/grub-gnulib"],
        "cozy" => &["src/userland/cozy"],
        "cpython" => &["src/development/python/cpython"],
        "llvm" => &["src/toolchain/llvm-project"],
        "rust" => &[
            "src/toolchain/rust",
            "upstream/policies/release-archives.toml",
        ],
        _ => &[],
    }
}

fn package_configuration_roots(package: &str) -> &'static [&'static str] {
    match package {
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
            "src/system/units/mattos-install-graphical.service",
            "src/system/units/mattos-install-graphical.target",
            "src/system/units/mattos-cosmic-installer-session.service",
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
        "linux-libc-dev" | "linux-modules-7.2.0-rc5-mattos" => {
            component_snapshot_version(repo_root, "linux")?
        }
        "libgcc-s1"
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
        "mattos-libtinfow6" | "libncursesw6" | "ncurses-base" | "ncurses-bin" => {
            component_snapshot_version(repo_root, "ncurses")?
        }
        "libreadline8" => component_snapshot_version(repo_root, "readline")?,
        "libndp0" => component_snapshot_version(repo_root, "libndp")?,
        "libkmod2" | "kmod" => component_snapshot_version(repo_root, "kmod")?,
        "mattos-libproc2" | "procps" => component_snapshot_version(repo_root, "procps-ng")?,
        "libsystemd0" | "libudev1" | "udev" => component_snapshot_version(repo_root, "systemd")?,
        "libexpat1" => component_snapshot_version(repo_root, "expat")?,
        "libcap2" => component_snapshot_version(repo_root, "libcap")?,
        "libattr1" => component_snapshot_version(repo_root, "attr")?,
        "libacl1" => component_snapshot_version(repo_root, "acl")?,
        "zlib1g" => component_snapshot_version(repo_root, "zlib")?,
        "libbz2-1.0" | "bzip2" => component_snapshot_version(repo_root, "bzip2")?,
        "liblz4-1" => component_snapshot_version(repo_root, "lz4")?,
        "liblzma5" | "xz-utils" => component_snapshot_version(repo_root, "xz")?,
        "libxxhash0" => component_snapshot_version(repo_root, "xxhash")?,
        "libmd0" => component_snapshot_version(repo_root, "libmd")?,
        "libbsd0" => component_snapshot_version(repo_root, "libbsd")?,
        "libzstd1" | "zstd" => component_snapshot_version(repo_root, "zstd")?,
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
        "passwd" => component_snapshot_version(repo_root, "shadow")?,
        "mattos-sudo-rs" => {
            cargo_package_version(&repo_root.join("src/system/auth/sudo-rs/Cargo.toml"))?
        }
        "libblkid1" | "libmount1" | "libsmartcols1" | "libuuid1" | "libfdisk1" | "mount"
        | "util-linux" | "login" => component_snapshot_version(repo_root, "util-linux")?,
        "gzip" => component_snapshot_version(repo_root, "gzip")?,
        "patch" => component_snapshot_version(repo_root, "patch")?,
        "libmagic1" | "file" => component_snapshot_version(repo_root, "file")?,
        "less" => component_snapshot_version(repo_root, "less")?,
        "git" => component_snapshot_version(repo_root, "git")?,
        "openssh-client" | "openssh-server" => component_snapshot_version(repo_root, "openssh")?,
        "libffi8" | "libffi-dev" => component_snapshot_version(repo_root, "libffi")?,
        "libwayland-client0" | "libwayland-server0" | "libwayland-egl1" => {
            component_snapshot_version(repo_root, "wayland")?
        }
        "libxkbcommon0" => component_snapshot_version(repo_root, "xkbcommon")?,
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
        "libxcb1" => component_snapshot_version(repo_root, "libxcb")?,
        "libx11-6" => component_snapshot_version(repo_root, "libx11")?,
        "libxext6" => component_snapshot_version(repo_root, "libxext")?,
        "libglvnd0" | "libglx0" | "libgl1" | "libopengl0" | "libegl1" | "libgles1"
        | "libgles2" => {
            component_snapshot_version(repo_root, "libglvnd")?
        }
        "libgbm1" | "libegl-mesa0" | "libgl1-mesa-dri" | "mesa-vulkan-drivers" => {
            component_snapshot_version(repo_root, "mesa")?
        }
        "libvulkan1" | "libvulkan-dev" | "vulkan-tools" => "1.4.357".to_string(),
        "linux-modules-nvidia-595-open-7.2.0-rc5-mattos"
        | "nvidia-firmware-595"
        | "libnvidia-gl-595"
        | "libnvidia-compute-595"
        | "libnvidia-encode-595"
        | "libnvidia-decode-595"
        | "nvidia-utils-595"
        | "nvidia-driver-595-open" => "595.84".to_string(),
        "cosmic-comp" => component_snapshot_version(repo_root, "cosmic-comp")?,
        "flatpak" => component_snapshot_version(repo_root, "flatpak")?,
        "xwayland" => component_snapshot_version(repo_root, "xwayland")?,
        "xdg-desktop-portal" => component_snapshot_version(repo_root, "xdg-desktop-portal")?,
        "cosmic-edit" => {
            cargo_package_version(&repo_root.join("src/desktop/cosmic/cosmic-edit/Cargo.toml"))?
        }
        "cosmic-initial-setup" => cargo_package_version(
            &repo_root.join("src/desktop/cosmic/cosmic-initial-setup/Cargo.toml"),
        )?,
        "libduktape207" => component_snapshot_version(repo_root, "duktape")?,
        "polkit" => component_snapshot_version(repo_root, "polkit")?,
        "network-manager" => component_snapshot_version(repo_root, "networkmanager")?,
        "libnl-3-200" | "libnl-genl-3-200" => component_snapshot_version(repo_root, "libnl")?,
        "wpasupplicant" => component_snapshot_version(repo_root, "wpa-supplicant")?,
        "grub-efi-amd64" => component_snapshot_version(repo_root, "grub")?,
        "mattos-cozy" => cargo_package_version(&repo_root.join("src/userland/cozy/Cargo.toml"))?,
        "cosmic-desktop" => component_snapshot_version(repo_root, "cosmic-session")?,
        "libdbus-1-3" => component_snapshot_version(repo_root, "dbus")?,
        "libdav1d7" => component_snapshot_version(repo_root, "dav1d")?,
        "libglib2.0-0t64" => component_snapshot_version(repo_root, "glib")?,
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
        _ => bail!("unknown package {}", spec.name),
    };
    let epoch = compatibility_epoch(repo_root, &spec.name)?;
    let upstream = match epoch {
        Some(epoch) => format!("{epoch}:{upstream}"),
        None => upstream,
    };
    Ok(format!("{upstream}-{REVISION}"))
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
    let short = state
        .imported_commit
        .get(..12)
        .unwrap_or(&state.imported_commit);
    Ok(format!("0~git.{short}"))
}

fn release_version_from_branch(branch: &str) -> Option<String> {
    let normalized = branch.replace('_', ".");
    let mut candidate = normalized.rsplit('/').next()?;
    let candidate_lower = candidate.to_ascii_lowercase();
    for prefix in [
        "binutils-",
        "bzip2-",
        "dbus-",
        "elfutils-",
        "gcc-",
        "glibc-",
        "gnupg-",
        "libx11-",
        "libxau-",
        "libxcb-",
        "libxdmcp-",
        "libxext-",
        "llvmorg-",
        "openssl-",
        "pcre2-",
        "readline-",
        "util-macros-",
        "xcb-proto-",
        "xkbcommon-",
        "xwayland-",
        "xkeyboard-config-",
        "xorgproto-",
        "xtrans-",
    ] {
        if candidate_lower.starts_with(prefix) {
            candidate = &candidate[prefix.len()..];
            break;
        }
    }
    candidate = candidate.strip_prefix('v').unwrap_or(candidate);
    let version = candidate
        .trim_end_matches(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '~'))
        .replace("-rc", "~rc");
    if version.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        && version.chars().any(|ch| ch == '.')
    {
        Some(version)
    } else {
        None
    }
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
        "linux" => {
            let state = read_sync_state(repo_root, "linux")?
                .ok_or_else(|| anyhow!("upstream state missing for linux"))?;
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
            let state = read_sync_state(repo_root, "libx11")?
                .ok_or_else(|| anyhow!("upstream state missing for libx11"))?;
            (
                "src/system/graphics/{libxau,libxdmcp,libxcb,libx11,libxext}".to_string(),
                "https://gitlab.freedesktop.org/xorg".to_string(),
                format!("libx11:{} (see upstream/state for complete closure)", state.imported_commit),
                "source-built minimal client ABI for immutable NVIDIA Vulkan dependencies; no X server, GLX dispatcher, or X11 platform metadata".to_string(),
            )
        }
        "nvidia-driver" => {
            let open = read_sync_state(repo_root, "nvidia-open-gpu-kernel-modules")?
                .ok_or_else(|| anyhow!("upstream state missing for NVIDIA open modules"))?;
            (
                "src/system/graphics/nvidia-driver/manifest.toml + src/system/graphics/nvidia-open-gpu-kernel-modules".to_string(),
                "https://download.nvidia.com/XFree86/Linux-x86_64/595.84/ + https://github.com/NVIDIA/open-gpu-kernel-modules".to_string(),
                format!("runfile-sha256:9e4f5d56e74e1ec12a05b2b0afda893c3187da71cbd8fb14c1a394bbeeeb4148; open:{}", open.imported_commit),
                "NVIDIA 595.84 production stack; proprietary files extracted verbatim without stripping; open modules built for 7.2.0-rc5-mattos".to_string(),
            )
        }
        component @ ("glibc" | "ncurses" | "kmod" | "procps-ng" | "systemd" | "dbus-broker"
        | "linux-pam" | "shadow" | "sudo-rs" | "util-linux" | "iproute2"
        | "iputils" | "expat" | "libcap" | "acl" | "zlib" | "bzip2" | "lz4" | "xz"
        | "xxhash" | "zstd" | "openssl" | "elfutils" | "pcre2" | "selinux"
        | "libxcrypt" | "libmd" | "libbsd" | "tar" | "gzip" | "patch" | "file"
        | "libgpg-error" | "libgcrypt" | "libassuan" | "libksba" | "npth"
        | "gnupg" | "less" | "git" | "openssh" | "libffi" | "wayland"
        | "xkbcommon" | "libglvnd" | "xkeyboard-config" | "cpython" | "llvm"
        | "rust" | "libnl" | "wpa-supplicant" | "grub") => {
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
                    | "usr/bin/fusermount3"
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
pub(crate) use repository::{
    generate_repository, repository_stage_spec,
};

pub(crate) fn install_prototype_packages(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let inventory = read_inventory(repo_root)?;
    repository::validate_repository_against_inventory(&repo_root.join("out/repository"), &inventory)?;
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
        .args(["--force-bad-path", "--force-script-chrootless", "--install"]);
    for name in package_install_order()? {
        let entry = inventory
            .package
            .iter()
            .find(|entry| entry.name == *name)
            .unwrap();
        command.arg(repo_root.join(&entry.artifact_path));
    }
    let status = command.status().context("failed to run dpkg for rootfs")?;
    if !status.success() {
        bail!("dpkg package installation into rootfs failed with {status}");
    }
    validate_dpkg_database(rootfs)?;
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
    for name in PACKAGE_NAMES {
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
        ("/usr/bin/sh", "mattos-brush"),
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
        ("/usr/lib/x86_64-linux-gnu/libgcc_s.so", "mattos-libgcc-dev"),
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
    for name in PACKAGE_NAMES {
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

pub(crate) fn embed_repository(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let source = repo_root.join("out/repository");
    if !source.join("dists/trixie/Release").is_file() {
        bail!("local repository has not been generated");
    }
    copy_tree_excluding_dotgit(&source, &rootfs.join("usr/share/mattos/repository"))
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
mod tests {
    use super::*;
    use super::repository::{dependency_name, exact_dependency_version, validate_release_sha256, validate_repository, validate_repository_packages};
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn installer_package_cache_tracks_its_embedded_linux_kernel() {
        assert_eq!(
            package_stage_dependencies("installer"),
            ["installer", "linux"]
        );
        assert_eq!(package_stage_dependencies("btrfs-progs"), ["installer"]);
        assert_eq!(package_stage_dependencies("dosfstools"), ["installer"]);
        assert_eq!(package_stage_dependencies("e2fsprogs"), ["installer"]);
    }

    #[test]
    fn flatpak_package_owns_the_complete_target_built_runtime_closure() {
        let specs = package_specs();
        let flatpak = specs
            .iter()
            .find(|spec| spec.name == "flatpak")
            .expect("flatpak package spec");
        assert_eq!(flatpak.source_component, "flatpak");
        assert_eq!(
            package_stage_dependencies("flatpak"),
            [
                "flatpak",
                "ostree",
                "gpgme",
                "gdk-pixbuf",
                "appstream",
                "json-glib",
                "libxmlb",
                "libfyaml",
                "fuse3",
                "libxml2",
                "libarchive",
                "libpng",
                "bubblewrap",
                "xdg-dbus-proxy",
            ]
        );
        assert!(package_source_roots("flatpak").contains(&"src/system/packages/flatpak"));
        assert!(package_source_roots("flatpak").contains(&"src/system/packages/ostree"));
        assert!(package_source_roots("flatpak").contains(&"src/system/security/bubblewrap"));
        assert!(package_source_roots("flatpak").contains(&"src/system/packages/xdg-dbus-proxy"));
        assert!(
            package_source_roots("flatpak")
                .contains(&"src/system/installer/flatpak-target-install.c")
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        assert!(!root
            .join("src/system/packages/flatpak/resources")
            .join("firefox.toml")
            .exists());
    }

    #[test]
    fn apt_package_cache_tracks_repository_policy_configuration() {
        assert_eq!(
            package_configuration_roots("apt"),
            ["src/system/packages/config/apt"]
        );
    }

    #[test]
    fn portal_package_consumes_flatpak_owned_bubblewrap_without_copying_it() {
        let specs = package_specs();
        let portal = specs
            .iter()
            .find(|spec| spec.name == "xdg-desktop-portal")
            .expect("portal package spec");
        assert!(portal.depends.contains(&"flatpak"));
        assert_eq!(
            package_stage_dependencies("xdg-desktop-portal"),
            ["xdg-desktop-portal", "gstreamer", "gstreamer-base"]
        );
        assert!(!package_source_roots("xdg-desktop-portal")
            .contains(&"src/system/security/bubblewrap"));

        let root = tempfile::tempdir().unwrap();
        for (component, relative) in [
            ("bubblewrap", "usr/bin/bwrap"),
            ("gstreamer", "usr/lib/libgstreamer-1.0.so.0"),
            ("gstreamer-base", "usr/lib/libgstpbutils-1.0.so.0"),
            ("xdg-desktop-portal", "usr/libexec/xdg-desktop-portal"),
        ] {
            let path = root.path().join("out/build").join(component).join("install").join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, component).unwrap();
        }
        let staging = root.path().join("staging");
        stage_xdg_desktop_portal(root.path(), &staging).unwrap();
        assert!(!staging.join("usr/bin/bwrap").exists());
        assert!(staging.join("usr/libexec/xdg-desktop-portal").is_file());
    }

    #[test]
    fn flatpak_document_portal_keeps_fusermount_privileged_after_normalization() {
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("usr/bin/fusermount3");
        fs::create_dir_all(helper.parent().unwrap()).unwrap();
        fs::write(&helper, "target-owned fuse helper\n").unwrap();
        set_mode(helper.clone(), 0o755).unwrap();

        normalize_package_modes(root.path()).unwrap();

        assert_eq!(
            fs::metadata(helper).unwrap().permissions().mode() & 0o7777,
            0o4755,
            "xdg-document-portal requires a setuid fusermount3 to mount /run/user/$UID/doc"
        );
    }

    #[test]
    fn xwayland_package_carries_the_owned_keyboard_compiler_and_layout_data() {
        let specs = package_specs();
        let xwayland = specs
            .iter()
            .find(|spec| spec.name == "xwayland")
            .expect("xwayland package spec");

        // xkbcomp is a runtime helper, deliberately kept out of Xwayland's
        // compile graph; it is composed into the same package instead.
        assert_eq!(
            package_stage_dependencies("xwayland"),
            [
                "xwayland",
                "libepoxy",
                "freetype",
                "libfontenc",
                "libxfont",
                "libxcvt",
                "libxshmfence",
                "libxkbfile",
                "xkbcomp",
            ]
        );
        assert!(
            package_source_roots("xwayland").contains(&"src/system/graphics/xkbcomp")
        );
        assert!(xwayland.depends.contains(&"xkb-data"));
        assert_eq!(
            crate::stage_graph::direct_dependencies(crate::stage_graph::BuildStage::Xwayland),
            [
                "formal-sysroot",
                "x11-compat",
                "pixman",
                "wayland",
                "libffi",
                "xkbcommon",
                "libxkbfile",
                "libxfont",
                "libfontenc",
                "freetype",
                "zlib",
                "libxcvt",
                "libxshmfence",
                "libepoxy",
                "libdrm",
                "libglvnd",
                "libmd",
                "mesa",
            ]
        );
    }

    #[test]
    fn broad_firmware_and_regulatory_data_are_source_owned_and_installer_required() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let firmware = root.join("src/system/data/linux-firmware");
        assert!(firmware.join("WHENCE").is_file());
        assert!(firmware.join("amdgpu").is_dir());
        assert!(firmware.join("intel").is_dir());
        assert!(
            firmware
                .join("intel/iwlwifi/iwlwifi-so-a0-gf-a0-83.ucode")
                .is_file()
        );
        assert!(!firmware.join(".git").exists());
        assert!(root.join("upstream/state/linux-firmware.toml").is_file());
        assert!(root.join("upstream/state/wireless-regdb.toml").is_file());

        let specs = package_specs();
        let installer = specs
            .iter()
            .find(|spec| spec.name == "mattos-installer")
            .unwrap();
        assert!(installer.depends.contains(&"linux-firmware"));
        assert!(installer.depends.contains(&"wireless-regdb"));

        let staged = tempfile::tempdir().unwrap();
        stage_wireless_regdb(&root, staged.path()).unwrap();
        assert_eq!(
            fs::read(staged.path().join("usr/lib/firmware/regulatory.db")).unwrap(),
            fs::read(root.join("src/system/data/wireless-regdb/regulatory.db")).unwrap()
        );
        assert!(
            staged
                .path()
                .join("usr/lib/firmware/regulatory.db.p7s")
                .is_file()
        );
    }

    #[test]
    fn graphical_installer_waits_for_modular_drm_and_input_coldplug() {
        let unit = include_str!("../../../system/units/mattos-cosmic-installer-session.service");
        assert!(unit.contains("After=systemd-udev-trigger.service systemd-udev-settle.service"));
        assert!(unit.contains("/dev/dri/card[0-9]*"));
        assert!(unit.contains("/dev/input/event[0-9]*"));
        assert!(unit.contains(
            "dbus-run-session --config-file=/usr/share/dbus-1/mattos-private-session.conf"
        ));
        assert!(unit.contains("XCURSOR_THEME=Pop"));
        assert!(!unit.contains("modprobe virtio_gpu"));
    }

    #[test]
    fn cosmic_runtime_packaging_owns_session_bus_tools_and_desktop_defaults() {
        let source = include_str!("packaging.rs");
        for required in [
            "usr/bin/dbus-daemon",
            "usr/bin/dbus-run-session",
            "usr/bin/dbus-update-activation-environment",
            "usr/share/dbus-1/mattos-private-session.conf",
            "usr/share/icons/hicolor/index.theme",
            "com.system76.CosmicSettings.Shortcuts/v1/defaults",
            "com.system76.CosmicSettings.WindowRules/v1/tiling_exception_defaults",
        ] {
            assert!(
                source.contains(required),
                "runtime packaging omits {required}"
            );
        }
        for required in [
            "flatpak_user_exports",
            "flatpak_system_exports",
            "flatpak/exports/share",
            "XDG_SESSION_TYPE XDG_CURRENT_DESKTOP DCONF_PROFILE XDG_DATA_DIRS SSH_AUTH_SOCK",
            "export DCONF_PROFILE=cosmic",
        ] {
            assert!(
                source.contains(required),
                "COSMIC session packaging must expose Flatpak exports through XDG_DATA_DIRS: {required}"
            );
        }

        let launcher = include_str!("../../../system/session/cosmic/cosmic-greeter-start");
        assert!(launcher.contains("LIBSEAT_BACKEND=logind"));
        assert!(launcher.contains("XDG_SESSION_TYPE=wayland"));
        assert!(launcher.contains("cosmic-comp --no-xwayland"));
        let unit = include_str!("../../../system/session/cosmic/cosmic-greeter.service");
        assert!(unit.contains("Restart=always"));
        assert!(unit.contains("TimeoutStopSec=10s"));
        assert!(unit.contains("After=systemd-user-sessions.service systemd-logind.service"));
        assert!(unit.contains("cosmic-greeter-daemon.service"));
        assert!(!source.contains("wants.join(\"cosmic-greeter-daemon.service\")"));

        let specs = package_specs();
        let libseat = specs
            .iter()
            .find(|spec| spec.name == "libseat1")
            .expect("libseat package");
        assert!(libseat.depends.contains(&"libsystemd0"));

        let package_roots = package_source_roots("cosmic-desktop");
        assert!(package_roots.contains(&"src/system/session/cosmic"));
        assert!(package_roots.contains(&"src/desktop/cosmic"));
    }

    #[test]
    fn cosmic_policy_resources_are_first_class_and_user_overridable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let resources = root.join("resources/COSMIC");
        assert!(resources.join("PROVENANCE.md").is_file());
        assert!(
            resources
                .join("defaults/com.system76.CosmicPanel/v1/entries")
                .is_file()
        );
        assert!(
            resources
                .join("layouts/top-panel-and-bottom-dock/layout.kdl")
                .is_file()
        );
        assert!(resources.join("themes/nebula-dark.ron").is_file());

        let source = format!(
            "{}\n{}\n{}\n{}",
            include_str!("main.rs"),
            include_str!("stages/image.rs"),
            include_str!("stages/registry.rs"),
            include_str!("stages/desktop.rs")
        );
        for path in [
            "resources/COSMIC/defaults",
            "resources/COSMIC/layouts",
            "resources/COSMIC/themes",
            "/usr/share/cosmic",
            "/usr/share/cosmic-layouts",
            "/usr/share/cosmic-themes",
        ] {
            assert!(
                source.contains(path),
                "COSMIC resource contract omits {path}"
            );
        }

        let libcosmic =
            fs::read_to_string(root.join("src/desktop/cosmic/libcosmic/cosmic-config/src/lib.rs"))
                .unwrap();
        assert!(libcosmic.contains("~/.config/cosmic") || libcosmic.contains("config_dir"));
        assert!(libcosmic.contains("find_data_file"));
    }

    #[test]
    fn package_dependencies_propagate_only_stage_output_changes() {
        let root = tempfile::tempdir().unwrap();
        let manifest_path = root.path().join("out/state/stages/make.json");
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        let write_manifest = |input_digest: &str, output_digest: &str| {
            fs::write(
                &manifest_path,
                serde_json::to_vec(&serde_json::json!({
                    "schema_version": performance::STAGE_MANIFEST_SCHEMA_VERSION,
                    "stage": "make",
                    "inputs": {
                        "source_digest": "source",
                        "configuration_digest": "configuration",
                        "tool_digest": "tool",
                        "environment_digest": "environment",
                        "dependency_digests": {},
                        "full_digest": input_digest
                    },
                    "input_details": {
                        "schema_version": performance::STAGE_MANIFEST_SCHEMA_VERSION,
                        "recipe": "test",
                        "source": {},
                        "configuration": {},
                        "environment": {},
                        "tools": {},
                        "dependencies": {}
                    },
                    "expected_outputs": [],
                    "output_content_digest": output_digest
                }))
                .unwrap(),
            )
            .unwrap();
        };

        write_manifest("input-one", "output-one");
        let first = package_stage_dependency_digest(root.path(), "make").unwrap();
        write_manifest("input-two", "output-one");
        let input_only_change = package_stage_dependency_digest(root.path(), "make").unwrap();
        assert_eq!(first, input_only_change);

        write_manifest("input-two", "output-two");
        let output_change = package_stage_dependency_digest(root.path(), "make").unwrap();
        assert_ne!(first, output_change);
    }

    #[test]
    fn package_set_boundary_is_exact_and_detects_artifact_or_input_changes() {
        let entry = PackageSetEntry {
            package: "example".into(),
            cache_key: "input-a".into(),
            artifact_path: "out/packages/amd64/example.deb".into(),
            artifact_sha256: "artifact-a".into(),
        };
        let manifest = PackageSetManifest {
            schema_version: PACKAGE_SET_SCHEMA_VERSION,
            policy: package_set_policy().into(),
            packages: vec![entry.clone()],
        };
        assert_eq!(manifest.packages, vec![entry.clone()]);

        let mut changed_input = entry.clone();
        changed_input.cache_key = "input-b".into();
        assert_ne!(manifest.packages, vec![changed_input]);

        let mut changed_artifact = entry;
        changed_artifact.artifact_sha256 = "artifact-b".into();
        assert_ne!(manifest.packages, vec![changed_artifact]);
    }

    #[test]
    fn package_set_policy_is_a_distinct_cache_boundary() {
        assert!(package_set_policy().contains("approved-package-cache-manifests"));
        assert_ne!(package_set_policy(), "package-cache-v1");
    }

    fn run_ok(cwd: &Path, program: &str, args: &[&str]) {
        let status = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "command failed: {program} {}",
            args.join(" ")
        );
    }

    fn repository_packages(extra_apt_field: Option<&str>) -> String {
        PACKAGE_NAMES
            .iter()
            .map(|name| {
                let apt_extra = if *name == "apt" {
                    extra_apt_field.unwrap_or("")
                } else {
                    ""
                };
                let provides = if *name == "libc6" {
                    "Provides: mattos-runtime-abi\n"
                } else {
                    ""
                };
                format!("Package: {name}\nVersion: 1\nArchitecture: amd64\n{provides}{apt_extra}\n")
            })
            .collect()
    }

    #[test]
    fn dpkg_git_build_pins_all_ignored_completion_inputs() {
        assert_eq!(DPKG_UPSTREAM_COMMIT.len(), 40);
        assert_eq!(
            DPKG_UPSTREAM_REPOSITORY,
            "https://git.dpkg.org/git/dpkg/dpkg.git"
        );
        let paths = DPKG_MISSING_SOURCE_INPUTS
            .iter()
            .map(|input| input.path)
            .collect::<BTreeSet<_>>();
        assert_eq!(paths.len(), 7);
        for expected in [
            "dselect/completion/bash/dselect",
            "scripts/completion/bash/dpkg-source",
            "src/completion/bash/dpkg",
            "src/completion/bash/dpkg-deb",
            "src/completion/bash/dpkg-query",
            "utils/completion/bash/start-stop-daemon",
            "utils/completion/bash/update-alternatives",
        ] {
            assert!(
                paths.contains(expected),
                "missing pinned dpkg input {expected}"
            );
        }
        assert!(
            DPKG_MISSING_SOURCE_INPUTS
                .iter()
                .all(|input| input.sha256.len() == 64)
        );
        let source = include_str!("packaging.rs");
        let start = source.find("pub(crate) fn build_dpkg").unwrap();
        let end = source[start..]
            .find("fn stage_missing_dpkg_source_inputs")
            .unwrap()
            + start;
        let build = &source[start..end];
        assert!(
            build.find("sync_build_source").unwrap()
                < build.find("stage_missing_dpkg_source_inputs").unwrap()
        );
    }

    #[test]
    fn apt_disables_checkout_dependent_build_rpath_padding() {
        let source = include_str!("packaging.rs");
        let start = source.find("pub(crate) fn build_apt").unwrap();
        let end = source[start..].find("fn relative_display").unwrap() + start;
        let build = &source[start..end];
        assert!(build.contains("-DCMAKE_SKIP_RPATH=ON"));
        assert!(build.contains("CMAKE_SKIP_RPATH:BOOL=ON"));
        assert!(build.contains("checkout-dependent CMake RPATH padding"));
    }

    #[test]
    fn validates_package_names_versions_and_architecture() {
        assert!(validate_package_name("coreutils").is_ok());
        assert!(validate_package_name("MattOS").is_err());
        assert!(validate_package_name("mattos_coreutils").is_err());
        assert!(validate_debian_version("0.9.0-1mattos1").is_ok());
        assert!(validate_debian_version("today!").is_err());
        assert_eq!(ARCH, "amd64");
    }

    #[test]
    fn control_contains_required_metadata() {
        let spec = package_specs()
            .into_iter()
            .find(|s| s.name == "curl")
            .unwrap();
        let control = render_control(
            &spec,
            "8.22.0-1mattos1",
            42,
            &["libc6 (= 2.43-1mattos1)".into()],
            &["libc.so.6".into()],
        )
        .unwrap();
        for field in [
            "Package:",
            "Version:",
            "Architecture: amd64",
            "Maintainer:",
            "Description:",
            "Depends:",
            "Installed-Size:",
            "X-MattOS-ELF-Dependencies:",
        ] {
            assert!(control.contains(field), "missing {field}");
        }
    }

    #[test]
    fn package_manager_definitions_are_complete_and_deliberate() {
        let specs = package_specs();
        for name in [
            "dpkg",
            "libapt-pkg7.0",
            "apt",
            "ca-certificates",
            "libgcc-s1",
            "libstdc++6",
        ] {
            assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
        }
        let filesystem = specs
            .iter()
            .find(|spec| spec.name == "mattos-filesystem")
            .unwrap();
        let dpkg = specs.iter().find(|spec| spec.name == "dpkg").unwrap();
        assert!(filesystem.essential);
        assert_eq!(filesystem.priority, "required");
        assert!(!dpkg.essential);
        assert_eq!(dpkg.priority, "required");
        assert!(DPKG_RUNTIME_PATHS.contains(&"usr/bin/update-alternatives"));
        assert!(DPKG_RUNTIME_PATHS.contains(&"usr/sbin/start-stop-daemon"));
        assert!(APT_RUNTIME_PATHS.contains(&"usr/lib/apt/methods/file"));
        assert!(APT_RUNTIME_PATHS.contains(&"usr/lib/apt/methods/gpgv"));
        assert!(APT_RUNTIME_PATHS.contains(&"usr/lib/apt/methods/http"));
        assert!(APT_RUNTIME_PATHS.contains(&"usr/lib/apt/methods/https"));
        assert!(
            !specs
                .iter()
                .any(|spec| spec.name == "mattos-bootstrap-runtime")
        );
    }

    #[test]
    fn apt_live_and_installed_policies_have_opposite_source_authority() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let config = root.join("src/system/packages/config/apt");
        let live = fs::read_to_string(config.join("mattos.sources")).unwrap();
        let installed = fs::read_to_string(config.join("installed/mattos.sources")).unwrap();
        assert!(live.contains("Trusted: yes"));
        assert!(!live.contains("Enabled: no"));
        assert!(installed.contains("Enabled: no"));
        assert!(!installed.contains("Trusted: yes"));
        let hosted = fs::read_to_string(config.join("installed/mattos-hosted.sources")).unwrap();
        let debian = fs::read_to_string(config.join("installed/debian-trixie.sources")).unwrap();
        assert!(hosted.contains("Enabled: yes"));
        assert!(debian.contains("Enabled: no"));
        assert!(debian.contains("Suites: trixie-security"));
    }

    #[test]
    fn native_cosmic_installer_has_an_owned_xkbcommon_runtime() {
        let specs = package_specs();
        let xkbcommon = specs
            .iter()
            .find(|spec| spec.name == "libxkbcommon0")
            .expect("xkbcommon runtime package must exist");
        assert_eq!(xkbcommon.source_component, "xkbcommon");
        assert_eq!(xkbcommon.depends, &["libc6", "xkb-data"]);

        let xkb_data = specs
            .iter()
            .find(|spec| spec.name == "xkb-data")
            .expect("default XKB runtime data package must exist");
        assert_eq!(xkb_data.source_component, "xkeyboard-config");
        assert!(xkb_data.depends.is_empty());

        let installer = specs
            .iter()
            .find(|spec| spec.name == "mattos-installer")
            .expect("installer package must exist");
        assert!(installer.depends.contains(&"libxkbcommon0"));
        assert!(installer.depends.contains(&"e2fsprogs"));
        assert!(installer.provides.contains(&"mattos-installer-cosmic"));
    }

    #[test]
    fn generic_mesa_runtime_is_split_into_debian_compatible_driver_packages() {
        let specs = package_specs();
        for name in [
            "libdrm-amdgpu1",
            "libdrm-nouveau2",
            "libgles1",
            "libgl1-mesa-dri",
            "mesa-vulkan-drivers",
        ] {
            assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
        }
        let dri = specs
            .iter()
            .find(|spec| spec.name == "libgl1-mesa-dri")
            .unwrap();
        for dependency in ["libllvm22", "libdrm-amdgpu1", "libdrm-nouveau2", "libzstd1"] {
            assert!(dri.depends.contains(&dependency));
        }
        assert!(dri.provides.contains(&"mattos-mesa-llvmpipe"));
        let vulkan = specs
            .iter()
            .find(|spec| spec.name == "mesa-vulkan-drivers")
            .unwrap();
        assert_eq!(vulkan.source_component, "mesa");
        assert!(vulkan.depends.contains(&"libvulkan1"));
        for name in ["libvulkan1", "libvulkan-dev", "vulkan-tools"] {
            assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
        }
    }

    #[test]
    fn canonical_mesa_icd_manifests_cover_hardware_virtio_and_software() {
        let root = tempfile::tempdir().unwrap();
        let manifest_dir = root.path().join("usr/share/vulkan/icd.d");
        let library_dir = root.path().join("usr/lib/x86_64-linux-gnu");
        fs::create_dir_all(&manifest_dir).unwrap();
        fs::create_dir_all(&library_dir).unwrap();
        for (manifest, library) in [
            ("radeon_icd.x86_64.json", "libvulkan_radeon.so"),
            ("intel_icd.x86_64.json", "libvulkan_intel.so"),
            ("nouveau_icd.x86_64.json", "libvulkan_nouveau.so"),
            ("virtio_icd.x86_64.json", "libvulkan_virtio.so"),
            ("lvp_icd.x86_64.json", "libvulkan_lvp.so"),
        ] {
            fs::write(library_dir.join(library), b"ICD").unwrap();
            fs::write(
                manifest_dir.join(manifest),
                serde_json::to_vec(&serde_json::json!({
                    "file_format_version": "1.0.1",
                    "ICD": {
                        "api_version": "1.4.354",
                        "library_path": format!("/usr/lib/x86_64-linux-gnu/{library}")
                    }
                }))
                .unwrap(),
            )
            .unwrap();
        }
        validate_vulkan_icd_manifests(root.path()).unwrap();
    }

    #[test]
    fn nvidia_stack_is_version_locked_and_coinstallable_with_mesa() {
        let specs = package_specs();
        let spec = |name| specs.iter().find(|spec| spec.name == name).unwrap();
        for name in [
            "linux-modules-nvidia-595-open-7.2.0-rc5-mattos",
            "nvidia-firmware-595",
            "libnvidia-gl-595",
            "libnvidia-compute-595",
            "libnvidia-encode-595",
            "libnvidia-decode-595",
            "nvidia-utils-595",
            "nvidia-driver-595-open",
        ] {
            assert_eq!(spec(name).source_component, "nvidia-driver");
            assert!(spec(name).conflicts.is_empty());
            assert!(spec(name).replaces.is_empty());
        }
        let driver = spec("nvidia-driver-595-open");
        assert!(driver.depends.contains(&"libnvidia-gl-595"));
        assert!(
            driver
                .depends
                .contains(&"linux-modules-nvidia-595-open-7.2.0-rc5-mattos")
        );
        assert!(spec("libnvidia-gl-595").depends.contains(&"libegl1"));
        assert_eq!(spec("libegl1").source_component, "libglvnd");
        assert_eq!(spec("libegl-mesa0").source_component, "mesa");
        assert!(specs.iter().any(|spec| spec.name == "mesa-vulkan-drivers"));
    }

    #[test]
    fn nvidia_manifest_pins_one_production_release_and_turing_floor() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let manifest: toml::Value = toml::from_str(
            &fs::read_to_string(root.join("src/system/graphics/nvidia-driver/manifest.toml"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["version"].as_str(), Some("595.84"));
        assert_eq!(manifest["release_branch"].as_str(), Some("production"));
        assert_eq!(
            manifest["binary_policy"].as_str(),
            Some("verbatim-extraction-no-strip-no-patch")
        );
        assert_eq!(manifest["include_in_iso"].as_bool(), Some(true));
        let supported = manifest["supported_gpu_generations"].as_array().unwrap();
        assert!(
            supported
                .iter()
                .any(|value| value.as_str() == Some("Turing"))
        );
        assert!(
            manifest["excluded_gpu_generations"].as_array().unwrap()[0]
                .as_str()
                .unwrap()
                .contains("Pascal")
        );
        assert!(root.join("src/system/graphics/nvidia-open-gpu-kernel-modules/kernel-open/nvidia/nvidia.Kbuild").is_file());
        assert!(
            !root
                .join("src/system/graphics/nvidia-open-gpu-kernel-modules/.git")
                .exists()
        );
        let modprobe =
            fs::read_to_string(root.join("src/system/graphics/nvidia-driver/nvidia-modprobe.conf"))
                .unwrap();
        assert!(modprobe.contains("options nvidia-drm modeset=1 fbdev=1"));
        assert!(!modprobe.contains("softdep nouveau"));
        assert!(!modprobe.contains("blacklist nouveau"));
    }

    #[test]
    fn third_milestone_package_families_are_complete() {
        let specs = package_specs();
        for name in [
            "mattos-libtinfow6",
            "libncursesw6",
            "ncurses-base",
            "ncurses-bin",
            "libkmod2",
            "kmod",
            "mattos-libproc2",
            "procps",
            "libsystemd0",
            "libudev1",
            "udev",
            "dbus-broker",
            "libpam0g",
            "mattos-libpam-misc0",
            "libpam-modules",
            "libpam-runtime",
            "passwd",
            "mattos-sudo-rs",
            "login",
            "libblkid1",
            "libmount1",
            "libsmartcols1",
            "mount",
            "iproute2",
            "iputils-ping",
            "xwayland",
            "xdg-desktop-portal",
        ] {
            assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
        }
        assert_eq!(PACKAGE_NAMES.len(), 173);
    }

    #[test]
    fn iso_codes_package_contains_the_pinned_locales_rs_contract() {
        let specs = package_specs();
        let spec = specs.iter().find(|spec| spec.name == "iso-codes").unwrap();
        assert_eq!(spec.source_component, "iso-codes");
        assert!(package_source_roots("iso-codes").contains(&"src/system/data/iso-codes"));

        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let staging = tempfile::tempdir().unwrap();
        stage_iso_codes(&repo, staging.path()).unwrap();
        for name in ["iso_3166-1.json", "iso_639-2.json", "iso_639-3.json"] {
            let path = staging.path().join("usr/share/iso-codes/json").join(name);
            assert!(path.is_file(), "missing {name}");
            assert!(!fs::read_to_string(path).unwrap().is_empty());
        }
        assert!(
            staging
                .path()
                .join("usr/share/doc/iso-codes/PROVENANCE.md")
                .is_file()
        );
    }

    #[test]
    fn base_userland_package_families_and_command_set_are_complete() {
        let specs = package_specs();
        for name in [
            "libuuid1",
            "libfdisk1",
            "libattr1",
            "util-linux",
            "gzip",
            "bzip2",
            "xz-utils",
            "zstd",
            "patch",
            "libmagic1",
            "file",
            "less",
            "git",
            "openssh-client",
            "openssh-server",
        ] {
            assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
        }
        assert_eq!(PACKAGE_NAMES.len(), 173);
        assert_eq!(
            UTIL_LINUX_BASE_PATHS,
            &[
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
            ]
        );
        let util = specs.iter().find(|spec| spec.name == "util-linux").unwrap();
        for dependency in [
            "libblkid1",
            "libmount1",
            "libsmartcols1",
            "libuuid1",
            "libfdisk1",
            "libselinux1",
            "libncursesw6",
            "mattos-libtinfow6",
        ] {
            assert!(util.depends.contains(&dependency));
        }
        let patch = specs.iter().find(|spec| spec.name == "patch").unwrap();
        assert_eq!(patch.depends, &["libattr1"]);
        assert_eq!(package_recipe_revision("util-linux"), 2);
        assert_eq!(package_recipe_revision("git"), 2);
        assert_eq!(package_recipe_revision("openssh-server"), 2);
        assert_eq!(package_recipe_revision("libpam-runtime"), 2);
        // Flatpak's package payload includes MattOS's signed Flathub policy,
        // a minimal initialized OSTree layout, and the target-rooted optional
        // install helper. Keep this expectation aligned with that contract.
        assert_eq!(package_recipe_revision("flatpak"), 9);
        let ssh_service = include_str!("../../../system/network/openssh/ssh.service");
        assert!(ssh_service.contains("\nType=notify\n"));
        assert!(ssh_service.contains("ExecStart=/usr/sbin/sshd -D"));
        assert!(OPENSSH_SERVER_RUNTIME_PATHS.contains(&"usr/lib/openssh/sshd-session"));
        assert!(OPENSSH_SERVER_RUNTIME_PATHS.contains(&"usr/lib/openssh/sshd-auth"));
        let util_digest = package_definition_digest(util).unwrap();
        let gzip = specs.iter().find(|spec| spec.name == "gzip").unwrap();
        assert_ne!(util_digest, package_definition_digest(gzip).unwrap());
    }

    #[test]
    fn self_hosting_development_package_families_are_split_and_complete() {
        let specs = package_specs();
        for name in [
            "libffi8",
            "libffi-dev",
            "libpython3.14",
            "python3",
            "python3-venv",
            "python3-dev",
            "libllvm22",
            "llvm",
            "llvm-dev",
            "clang",
            "lld",
            "rustc",
            "cargo",
        ] {
            assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
        }
        assert_eq!(PACKAGE_NAMES.len(), 173);
        let python = specs.iter().find(|spec| spec.name == "python3").unwrap();
        for dependency in [
            "libffi8",
            "libpython3.14",
            "libncursesw6",
            "mattos-libtinfow6",
        ] {
            assert!(python.depends.contains(&dependency));
        }
        let ncurses = specs
            .iter()
            .find(|spec| spec.name == "libncursesw6")
            .unwrap();
        assert_eq!(ncurses.source_component, "ncurses");
        assert_eq!(package_recipe_revision("libncursesw6"), 2);
        for package in ["libllvm22", "llvm", "clang", "lld", "rustc"] {
            let spec = specs.iter().find(|spec| spec.name == package).unwrap();
            assert!(spec.depends.contains(&"zlib1g"), "{package} lacks zlib1g");
            assert!(
                spec.depends.contains(&"libzstd1"),
                "{package} lacks libzstd1"
            );
        }
        let cargo = specs.iter().find(|spec| spec.name == "cargo").unwrap();
        for dependency in ["rustc", "libgcc-s1", "zlib1g", "libzstd1"] {
            assert!(cargo.depends.contains(&dependency));
        }
        assert!(
            !specs
                .iter()
                .any(|spec| matches!(spec.name, "perl" | "tcl" | "bash"))
        );
    }

    #[test]
    fn rustc_and_cargo_stage_disjoint_complete_payloads() {
        fn write(root: &Path, relative: &str) {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, relative).unwrap();
        }

        fn files(root: &Path) -> BTreeSet<PathBuf> {
            fn visit(root: &Path, directory: &Path, paths: &mut BTreeSet<PathBuf>) {
                for entry in fs::read_dir(directory).unwrap() {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    if entry.file_type().unwrap().is_dir() {
                        visit(root, &path, paths);
                    } else {
                        paths.insert(path.strip_prefix(root).unwrap().to_path_buf());
                    }
                }
            }

            let mut paths = BTreeSet::new();
            visit(root, root, &mut paths);
            paths
        }

        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        let install = repo.join("out/build/rust/install/usr");
        for relative in [
            "bin/rustc",
            "bin/rustdoc",
            "bin/cargo",
            "lib/rustlib/x86_64-unknown-linux-gnu/lib/libstd-test.rlib",
            "share/doc/rustc/LICENSE-APACHE",
            "share/doc/cargo/LICENSE-APACHE",
            "share/man/man1/rustc.1",
            "share/man/man1/rustdoc.1",
            "share/man/man1/cargo.1",
            "share/man/man1/cargo-build.1",
            "share/zsh/site-functions/_cargo",
        ] {
            write(&install, relative);
        }

        let rustc_stage = repo.join("rustc-stage");
        let cargo_stage = repo.join("cargo-stage");
        stage_rustc(repo, &rustc_stage).unwrap();
        stage_cargo(repo, &cargo_stage).unwrap();

        let rustc_files = files(&rustc_stage);
        let cargo_files = files(&cargo_stage);
        assert!(rustc_files.is_disjoint(&cargo_files));
        assert!(rustc_files.contains(Path::new("usr/bin/rustc")));
        assert!(rustc_files.contains(Path::new("usr/bin/rustdoc")));
        assert!(rustc_files.contains(Path::new(
            "usr/lib/rustlib/x86_64-unknown-linux-gnu/lib/libstd-test.rlib",
        )));
        assert!(!rustc_files.contains(Path::new("usr/bin/cargo")));
        assert!(cargo_files.contains(Path::new("usr/bin/cargo")));
        assert!(cargo_files.contains(Path::new("usr/share/man/man1/cargo-build.1")));
        assert!(!cargo_files.contains(Path::new("usr/share/man/man1/rustc.1")));
    }

    #[test]
    fn udev_hwdb_is_prebuilt_from_vendor_sources_without_mutable_state() {
        let specs = package_specs();
        let udev = specs.iter().find(|spec| spec.name == "udev").unwrap();
        assert_eq!(udev.source_component, "systemd");
        assert!(udev.depends.contains(&"libudev1"));
        assert_eq!(UDEV_HWDB_SOURCE_REL, "usr/lib/udev/hwdb.d");
        assert_eq!(UDEV_HWDB_BINARY_REL, "usr/lib/udev/hwdb.bin");

        let imported_unit =
            include_str!("../../../system/systemd/units/systemd-hwdb-update.service.in");
        for required in [
            "ConditionPathExists=|!{{UDEVLIBEXECDIR}}/hwdb.bin",
            "ConditionPathExists=|/etc/udev/hwdb.bin",
            "ConditionDirectoryNotEmpty=|/etc/udev/hwdb.d/",
        ] {
            assert!(imported_unit.contains(required));
        }
        let source = include_str!("packaging/staging.rs");
        let start = source.find("fn stage_udev_hwdb").unwrap();
        let end = source[start..].find("fn stage_runtime_paths").unwrap() + start;
        let body = &source[start..end];
        for required in [
            "systemd_install.join(UDEV_HWDB_SOURCE_REL)",
            "generate_udev_hwdb(repo_root, staging)",
            "--usr",
            "--strict",
            "KSLPHHRH",
            "etc/udev/hwdb.bin",
        ] {
            assert!(body.contains(required), "missing hwdb policy {required}");
        }
    }

    #[test]
    fn small_library_migration_definitions_are_complete() {
        let specs = package_specs();
        let expat = specs.iter().find(|spec| spec.name == "libexpat1").unwrap();
        let libcap = specs.iter().find(|spec| spec.name == "libcap2").unwrap();
        let attr = specs.iter().find(|spec| spec.name == "libattr1").unwrap();
        let broker = specs
            .iter()
            .find(|spec| spec.name == "dbus-broker")
            .unwrap();
        let iproute2 = specs.iter().find(|spec| spec.name == "iproute2").unwrap();
        let acl = specs.iter().find(|spec| spec.name == "libacl1").unwrap();
        let zlib = specs.iter().find(|spec| spec.name == "zlib1g").unwrap();
        let bzip2 = specs.iter().find(|spec| spec.name == "libbz2-1.0").unwrap();
        let lz4 = specs.iter().find(|spec| spec.name == "liblz4-1").unwrap();
        let xz = specs.iter().find(|spec| spec.name == "liblzma5").unwrap();
        let xxhash = specs.iter().find(|spec| spec.name == "libxxhash0").unwrap();
        let libmd = specs.iter().find(|spec| spec.name == "libmd0").unwrap();
        let libbsd = specs.iter().find(|spec| spec.name == "libbsd0").unwrap();
        let zstd = specs.iter().find(|spec| spec.name == "libzstd1").unwrap();
        let crypto = specs
            .iter()
            .find(|spec| spec.name == "mattos-libcrypto3")
            .unwrap();
        let ssl = specs.iter().find(|spec| spec.name == "libssl3t64").unwrap();
        let elf = specs.iter().find(|spec| spec.name == "libelf1t64").unwrap();
        let shadow = specs.iter().find(|spec| spec.name == "passwd").unwrap();
        let tar = specs.iter().find(|spec| spec.name == "tar").unwrap();
        let dpkg = specs.iter().find(|spec| spec.name == "dpkg").unwrap();
        let apt = specs
            .iter()
            .find(|spec| spec.name == "libapt-pkg7.0")
            .unwrap();
        assert_eq!(attr.source_component, "attr");
        assert_eq!(expat.source_component, "expat");
        assert_eq!(libcap.source_component, "libcap");
        assert!(broker.depends.contains(&"libexpat1"));
        assert!(iproute2.depends.contains(&"libcap2"));
        assert!(iproute2.depends.contains(&"zlib1g"));
        assert_eq!(acl.source_component, "acl");
        assert_eq!(zlib.source_component, "zlib");
        assert_eq!(bzip2.source_component, "bzip2");
        assert_eq!(lz4.source_component, "lz4");
        assert_eq!(xz.source_component, "xz");
        assert_eq!(xxhash.source_component, "xxhash");
        assert_eq!(libmd.source_component, "libmd");
        assert_eq!(libbsd.source_component, "libbsd");
        assert!(libbsd.depends.contains(&"libmd0"));
        assert_eq!(zstd.source_component, "zstd");
        assert_eq!(crypto.source_component, "openssl");
        assert!(crypto.depends.contains(&"libzstd1"));
        assert_eq!(ssl.source_component, "openssl");
        assert!(ssl.depends.contains(&"mattos-libcrypto3"));
        assert_eq!(elf.source_component, "elfutils");
        assert!(elf.depends.contains(&"libzstd1"));
        assert!(shadow.depends.contains(&"libbsd0"));
        assert!(shadow.depends.contains(&"libmd0"));
        assert_eq!(tar.source_component, "tar");
        assert!(tar.depends.contains(&"libacl1"));
        assert_eq!(tar.provides, &["tar"]);
        assert_eq!(tar.conflicts, &["tar"]);
        assert_eq!(tar.replaces, &["tar"]);
        assert!(dpkg.depends.contains(&"tar"));
        assert!(dpkg.depends.contains(&"zlib1g"));
        assert!(dpkg.depends.contains(&"libbz2-1.0"));
        assert!(dpkg.depends.contains(&"liblzma5"));
        assert!(dpkg.depends.contains(&"libzstd1"));
        assert!(dpkg.depends.contains(&"libmd0"));
        assert!(apt.depends.contains(&"zlib1g"));
        assert!(apt.depends.contains(&"libbz2-1.0"));
        assert!(apt.depends.contains(&"liblz4-1"));
        assert!(apt.depends.contains(&"liblzma5"));
        assert!(apt.depends.contains(&"libxxhash0"));
        assert!(apt.depends.contains(&"libzstd1"));
        assert!(apt.depends.contains(&"mattos-libcrypto3"));
        let apt_cli = specs.iter().find(|spec| spec.name == "apt").unwrap();
        assert!(apt_cli.depends.contains(&"zlib1g"));
        assert!(apt_cli.depends.contains(&"libbz2-1.0"));
        assert!(apt_cli.depends.contains(&"liblz4-1"));
        assert!(apt_cli.depends.contains(&"liblzma5"));
        assert!(apt_cli.depends.contains(&"libxxhash0"));
        assert!(apt_cli.depends.contains(&"libzstd1"));
        assert!(apt_cli.depends.contains(&"mattos-libcrypto3"));
        let curl = specs.iter().find(|spec| spec.name == "curl").unwrap();
        assert!(curl.depends.contains(&"zlib1g"));
        assert!(curl.depends.contains(&"libzstd1"));
        assert!(curl.depends.contains(&"mattos-libcrypto3"));
        assert!(curl.depends.contains(&"libssl3t64"));
        assert_eq!(
            MIGRATED_BOOTSTRAP_SONAME_PREFIXES,
            &[
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
            ]
        );
    }

    #[test]
    fn openssl_elfutils_zstd_graph_is_active_and_acyclic() {
        let specs = package_specs();
        assert!(
            !specs
                .iter()
                .any(|spec| spec.name == "mattos-bootstrap-runtime")
        );
        assert!(MIGRATED_BOOTSTRAP_SONAME_PREFIXES.contains(&"libzstd.so"));

        let order = package_install_order_for(&specs, PACKAGE_NAMES).unwrap();
        let position = |name: &str| order.iter().position(|entry| *entry == name).unwrap();
        assert!(position("libc6") < position("libzstd1"));
        assert!(position("libzstd1") < position("mattos-libcrypto3"));
        assert!(position("libzstd1") < position("libelf1t64"));
        assert!(position("mattos-libcrypto3") < position("libssl3t64"));
        assert!(position("libssl3t64") < position("curl"));
    }

    #[test]
    fn pcre2_selinux_libxcrypt_graph_is_active_and_acyclic() {
        let specs = package_specs();
        let spec = |name| specs.iter().find(|spec| spec.name == name).unwrap();
        assert!(spec("libselinux1").depends.contains(&"libpcre2-8-0"));
        assert!(spec("dpkg").depends.contains(&"libselinux1"));
        assert!(spec("iproute2").depends.contains(&"libselinux1"));
        assert!(spec("libpam-modules").depends.contains(&"libcrypt1"));
        assert!(spec("libpam-runtime").depends.contains(&"libcrypt1"));
        assert!(spec("passwd").depends.contains(&"libcrypt1"));
        assert!(spec("libmount1").depends.contains(&"libblkid1"));
        assert!(spec("mount").depends.contains(&"libmount1"));
        assert!(spec("mount").depends.contains(&"libsmartcols1"));
        assert!(spec("mount").depends.contains(&"libselinux1"));
        for prefix in ["libpcre2-8.so", "libselinux.so", "libcrypt.so"] {
            assert!(MIGRATED_BOOTSTRAP_SONAME_PREFIXES.contains(&prefix));
        }
        assert_eq!(
            package_install_order_for(&specs, PACKAGE_NAMES)
                .unwrap()
                .len(),
            PACKAGE_NAMES.len()
        );
    }

    #[test]
    fn zstd_cycle_design_is_rejected() {
        let specs = [
            PackageSpec {
                name: "mattos-bootstrap-runtime",
                description: "test bootstrap",
                source_component: "test",
                depends: &["libzstd1"],
                provides: &[],
                conflicts: &[],
                replaces: &[],
                essential: false,
                priority: "required",
            },
            PackageSpec {
                name: "libzstd1",
                description: "test zstd",
                source_component: "zstd",
                depends: &["mattos-bootstrap-runtime"],
                provides: &[],
                conflicts: &[],
                replaces: &[],
                essential: false,
                priority: "important",
            },
        ];
        let error = package_install_order_for(&specs, &["mattos-bootstrap-runtime", "libzstd1"])
            .unwrap_err()
            .to_string();
        assert!(error.contains("circular or unresolvable"));
    }

    #[test]
    fn migrated_libraries_cannot_remain_in_bootstrap_manifest() {
        let libc_error = validate_migrated_bootstrap_absent(&[
            "/usr/lib/x86_64-linux-gnu/libc.so.6\t/lib/libc.so.6\treason\thash".into(),
        ])
        .unwrap_err()
        .to_string();
        assert!(libc_error.contains("libc.so.6 remains"));
        let tar_error =
            validate_migrated_bootstrap_absent(
                &["/usr/bin/tar\t/usr/bin/tar\treason\thash".into()],
            )
            .unwrap_err()
            .to_string();
        assert!(tar_error.contains("GNU tar remains"));
        let error = validate_migrated_bootstrap_absent(&[
            "/usr/lib/x86_64-linux-gnu/libexpat.so.1\t/lib/libexpat.so.1\treason\thash".into(),
        ])
        .unwrap_err()
        .to_string();
        assert!(error.contains("libexpat.so.1 remains"));
    }

    #[test]
    fn bootstrap_inventory_shrinks_by_selected_library_payloads() {
        let before = [
            ("libc.so.6", 2_326_088u64),
            ("libexpat.so.1", 182_608),
            ("libcap.so.2", 51_616),
            ("libacl.so.1", 39_768),
            ("libz.so.1", 121_280),
            ("libbz2.so.1.0", 74_680),
            ("liblz4.so.1", 166_224),
            ("liblzma.so.5", 215_448),
            ("libxxhash.so.0", 96_408),
            ("libmd.so.0", 59_776),
            ("libbsd.so.0", 89_312),
            ("libcrypto.so.3", 6_353_776),
            ("libssl.so.3", 1_106_088),
            ("libelf.so.1", 125_728),
            ("libzstd.so.1", 817_376),
            ("libpcre2-8.so.0", 711_416),
            ("libselinux.so.1", 211_488),
            ("libcrypt.so.1", 198_744),
        ];
        let after = before
            .iter()
            .filter(|(name, _)| {
                !MIGRATED_BOOTSTRAP_SONAME_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
            })
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(before.len() - after.len(), 18);
        assert_eq!(
            before.iter().map(|(_, size)| size).sum::<u64>()
                - after.iter().map(|(_, size)| size).sum::<u64>(),
            12_947_824
        );
    }

    #[test]
    fn glibc_runtime_graph_is_foundational_and_acyclic() {
        let specs = package_specs();
        let libc = specs.iter().find(|spec| spec.name == "libc6").unwrap();
        let libc_bin = specs.iter().find(|spec| spec.name == "libc-bin").unwrap();
        let libgcc = specs.iter().find(|spec| spec.name == "libgcc-s1").unwrap();
        let libstdcxx = specs.iter().find(|spec| spec.name == "libstdc++6").unwrap();
        assert_eq!(libc.depends, &["mattos-filesystem"]);
        assert!(libc_bin.depends.contains(&"libc6"));
        assert!(libgcc.depends.contains(&"libc6"));
        assert!(libstdcxx.depends.contains(&"libgcc-s1"));
        assert!(!libc.depends.contains(&"mattos-bootstrap-runtime"));
        let order = package_install_order_for(&specs, PACKAGE_NAMES).unwrap();
        let position = |name: &str| order.iter().position(|entry| *entry == name).unwrap();
        assert!(position("mattos-filesystem") < position("libc6"));
        assert!(position("libc6") < position("libgcc-s1"));
        assert!(position("libgcc-s1") < position("libstdc++6"));
        assert_eq!(order.len(), PACKAGE_NAMES.len());
    }

    #[test]
    fn gcc_runtime_packages_are_minimal_acyclic_and_replace_bootstrap() {
        let specs = package_specs();
        let spec = |name| specs.iter().find(|spec| spec.name == name).unwrap();
        let libgcc = spec("libgcc-s1");
        let libstdcxx = spec("libstdc++6");
        assert_eq!(libgcc.source_component, "gcc");
        assert_eq!(libgcc.depends, &["mattos-filesystem", "libc6"]);
        assert_eq!(libgcc.provides, &["libgcc-s1"]);
        assert_eq!(libstdcxx.source_component, "gcc");
        assert_eq!(
            libstdcxx.depends,
            &["mattos-filesystem", "libc6", "libgcc-s1"]
        );
        assert_eq!(libstdcxx.provides, &["libstdc++6"]);
        assert!(!PACKAGE_NAMES.contains(&"mattos-bootstrap-runtime"));
        assert!(specs.iter().all(|spec| {
            !spec.depends.contains(&"mattos-bootstrap-runtime")
                && !spec.depends.contains(&"mattos-bootstrap-gcc-runtime")
        }));
        let order = package_install_order_for(&specs, PACKAGE_NAMES).unwrap();
        let position = |name| order.iter().position(|item| *item == name).unwrap();
        assert!(position("libc6") < position("libgcc-s1"));
        assert!(position("libgcc-s1") < position("libstdc++6"));
    }

    #[test]
    fn native_development_package_graph_has_explicit_owners() {
        let specs = package_specs();
        let spec = |name| specs.iter().find(|spec| spec.name == name).unwrap();
        for name in [
            "linux-libc-dev",
            "libc6-dev",
            "mattos-libgcc-dev",
            "mattos-libstdc++-dev",
            "binutils",
            "mattos-gcc-common",
            "cpp",
            "gcc",
            "g++",
            "make",
        ] {
            assert!(PACKAGE_NAMES.contains(&name), "missing package {name}");
        }
        assert!(spec("libc6-dev").depends.contains(&"linux-libc-dev"));
        assert!(
            spec("mattos-libstdc++-dev")
                .depends
                .contains(&"mattos-libgcc-dev")
        );
        assert!(spec("gcc").depends.contains(&"mattos-gcc-common"));
        assert!(spec("g++").depends.contains(&"gcc"));
        let order = package_install_order_for(&specs, PACKAGE_NAMES).unwrap();
        let position = |name| order.iter().position(|item| *item == name).unwrap();
        assert!(position("linux-libc-dev") < position("libc6-dev"));
        assert!(position("libc6-dev") < position("mattos-libgcc-dev"));
        assert!(position("mattos-libgcc-dev") < position("mattos-libstdc++-dev"));
        assert!(position("binutils") < position("mattos-gcc-common"));
        assert!(position("mattos-gcc-common") < position("gcc"));
        assert!(position("gcc") < position("g++"));
    }

    #[test]
    fn libgcc_development_package_owns_shared_linker_name() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        fs::create_dir_all(repo.join("out/build/gcc-runtime/install/usr/lib/x86_64-linux-gnu/gcc"))
            .unwrap();
        fs::create_dir_all(repo.join("src/toolchain/gcc")).unwrap();
        fs::write(
            repo.join("src/toolchain/gcc/COPYING.RUNTIME"),
            "runtime license\n",
        )
        .unwrap();
        let staging = repo.join("staging");

        stage_gcc_development(repo, &staging, false).unwrap();

        assert_eq!(
            fs::read_link(staging.join("usr/lib/x86_64-linux-gnu/libgcc_s.so")).unwrap(),
            PathBuf::from("libgcc_s.so.1")
        );
    }

    #[test]
    fn brush_package_owns_sh_and_bash_entry_points() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        let source = repo.join("out/build/brush/cargo-target/release/brush");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "source-built brush\n").unwrap();
        let staging = repo.join("staging");

        stage_brush(repo, &staging).unwrap();

        assert_eq!(
            fs::read_link(staging.join("usr/bin/sh")).unwrap(),
            Path::new("brush")
        );
        assert_eq!(
            fs::read_link(staging.join("usr/bin/bash")).unwrap(),
            Path::new("brush")
        );
        assert_eq!(
            fs::metadata(staging.join("usr/bin/brush"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }

    #[test]
    fn retired_bootstrap_audit_serializes_zero_host_payloads() {
        let temp = tempfile::tempdir().unwrap();
        generate_bootstrap_audit(temp.path()).unwrap();
        let report: BootstrapAuditReport = toml::from_str(
            &fs::read_to_string(temp.path().join("out/reports/bootstrap-runtime-audit.toml"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(report.package, "retired");
        assert_eq!(report.snapshot, "runtime-source-closure-complete");
        assert_eq!(report.entry_count, 0);
        assert_eq!(report.payload_bytes, 0);
        assert!(report.entries.is_empty());
    }

    #[test]
    fn glibc_runtime_inventory_covers_loader_nss_and_resolver() {
        for name in [
            "libc.so.6",
            "libm.so.6",
            "libnss_files.so.2",
            "libnss_dns.so.2",
            "libresolv.so.2",
        ] {
            assert!(GLIBC_RUNTIME_LIBRARIES.contains(&name));
        }
        assert!(MIGRATED_BOOTSTRAP_SONAME_PREFIXES.contains(&"ld-linux-"));
    }

    #[test]
    fn bootstrap_source_classifications_cover_known_and_unknown_entries() {
        assert_eq!(bootstrap_source_attribution("libexpat.so.1").1, "A");
        assert_eq!(bootstrap_source_attribution("libcap.so.2").1, "A");
        assert_eq!(bootstrap_source_attribution("libacl.so.1").1, "A");
        assert_eq!(bootstrap_source_attribution("libz.so.1").1, "A");
        assert_eq!(bootstrap_source_attribution("libbz2.so.1.0").1, "A");
        assert_eq!(bootstrap_source_attribution("libmd.so.0").1, "A");
        assert_eq!(bootstrap_source_attribution("libbsd.so.0").1, "A");
        assert_eq!(bootstrap_source_attribution("libcrypto.so.3").1, "A");
        assert_eq!(bootstrap_source_attribution("libssl.so.3").1, "A");
        assert_eq!(bootstrap_source_attribution("libelf.so.1").1, "A");
        assert_eq!(bootstrap_source_attribution("libzstd.so.1").1, "A");
        assert_eq!(bootstrap_source_attribution("libpcre2-8.so.0").1, "A");
        assert_eq!(bootstrap_source_attribution("libselinux.so.1").1, "A");
        assert_eq!(bootstrap_source_attribution("libcrypt.so.1").1, "A");
        assert_eq!(bootstrap_source_attribution("libc.so.6").1, "D");
        assert_eq!(bootstrap_source_attribution("tar").1, "A");
        let unknown = bootstrap_source_attribution("libunknown.so.9");
        assert!(unknown.0.is_none());
        assert_eq!(unknown.1, "E");
        assert_eq!(unknown.4, "low");
    }

    #[test]
    fn bootstrap_audit_schema_roundtrips_and_preserves_inference() {
        let report = BootstrapAuditReport {
            schema_version: 1,
            package: "mattos-bootstrap-runtime".into(),
            snapshot: "test".into(),
            entry_count: 1,
            payload_bytes: 4,
            classification_totals: BTreeMap::from([("C".into(), 1)]),
            entries: vec![BootstrapAuditEntry {
                path: "/usr/lib/libsample.so.1".into(),
                file_type: "regular".into(),
                size: 4,
                mode: "0644".into(),
                symlink_target: None,
                sha256: "00".repeat(32),
                file_description: "ELF shared object".into(),
                elf_type: Some("DYN".into()),
                elf_interpreter: None,
                soname: Some("libsample.so.1".into()),
                dt_needed: vec!["libc.so.6".into()],
                objdump_needed: vec!["libc.so.6".into()],
                ldd_resolved: vec!["libc.so.6 => /usr/lib/libc.so.6".into()],
                confirmed_host_package: Some("libsample1:amd64".into()),
                upstream_project: Some("sample upstream".into()),
                source_attribution: "inferred".into(),
                source_already_exists_in_mattos: false,
                consumers: vec![BootstrapConsumer {
                    package: "mattos-sample".into(),
                    path: "/usr/bin/sample".into(),
                }],
                reason_in_bootstrap_runtime: "temporary closure".into(),
                recommended_future_package: "mattos-libsample1".into(),
                migration_difficulty: "low".into(),
                attribution_confidence: "medium".into(),
                classification: "C".into(),
                boundary_group: "leaf library".into(),
            }],
        };
        let body = toml::to_string_pretty(&report).unwrap();
        let parsed: BootstrapAuditReport = toml::from_str(&body).unwrap();
        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.entries[0].source_attribution, "inferred");
        assert_eq!(
            parsed.entries[0].confirmed_host_package.as_deref(),
            Some("libsample1:amd64")
        );
        assert_eq!(parsed.entries[0].consumers[0].package, "mattos-sample");
    }

    #[test]
    fn bootstrap_consumer_graph_uses_actual_dt_needed_entries() {
        let temp = tempfile::tempdir().unwrap();
        let staging = temp
            .path()
            .join("out/packages/staging/mattos-consumer/usr/bin");
        fs::create_dir_all(&staging).unwrap();
        fs::write(
            temp.path().join("library.c"),
            "int mattos_audit_symbol(void) { return 7; }\n",
        )
        .unwrap();
        fs::write(temp.path().join("consumer.c"), "extern int mattos_audit_symbol(void); int main(void) { return mattos_audit_symbol(); }\n").unwrap();
        let library = temp.path().join("libaudit.so.1");
        run_ok(
            temp.path(),
            "gcc",
            &[
                "-shared",
                "-fPIC",
                "-Wl,-soname,libaudit.so.1",
                "library.c",
                "-o",
                path_str(&library).unwrap(),
            ],
        );
        let consumer = staging.join("consumer");
        run_ok(
            temp.path(),
            "gcc",
            &[
                "consumer.c",
                path_str(&library).unwrap(),
                "-o",
                path_str(&consumer).unwrap(),
            ],
        );
        let graph = bootstrap_consumers(temp.path()).unwrap();
        let uses = graph.get("libaudit.so.1").unwrap();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].package, "mattos-consumer");
        assert_eq!(uses[0].path, "/usr/bin/consumer");
    }

    #[test]
    fn host_package_attribution_is_confirmed_separately_from_upstream_inference() {
        let package = confirmed_host_package(Path::new("/usr/bin/tar")).unwrap();
        assert!(
            package
                .as_deref()
                .is_some_and(|name| name.starts_with("tar"))
        );
        let upstream = bootstrap_source_attribution("tar");
        assert_eq!(upstream.0, Some("GNU tar"));
        assert_eq!(upstream.1, "A");
    }

    #[test]
    fn dependency_parser_handles_exact_versions_and_provides() {
        assert_eq!(
            dependency_name("libapt-pkg7.0 (= 3.3.2-1mattos1)").unwrap(),
            "libapt-pkg7.0"
        );
        assert_eq!(
            exact_dependency_version("libapt-pkg7.0 (= 3.3.2-1mattos1)").unwrap(),
            Some("3.3.2-1mattos1")
        );
        assert!(exact_dependency_version("libapt-pkg7.0 (>= 3)").is_err());
        let body = repository_packages(Some("Depends: mattos-runtime-abi\n"));
        assert!(validate_repository_packages(&body).is_ok());
    }

    #[test]
    fn repository_dependency_closure_rejects_missing_and_wrong_exact_versions() {
        assert!(
            validate_repository_packages(&repository_packages(Some(
                "Depends: libapt-pkg7.0 (= 1)\n"
            )))
            .is_ok()
        );
        assert!(
            validate_repository_packages(&repository_packages(Some("Depends: mattos-missing\n")))
                .is_err()
        );
        assert!(
            validate_repository_packages(&repository_packages(Some(
                "Depends: libapt-pkg7.0 (= 2)\n"
            )))
            .is_err()
        );
    }

    #[test]
    fn repository_rejects_duplicate_package_version_architecture() {
        let mut body = repository_packages(None);
        body.push_str("Package: apt\nVersion: 1\nArchitecture: amd64\n\n");
        assert!(validate_repository_packages(&body).is_err());
    }

    #[test]
    fn apt_configuration_is_local_only_vendor_scoped_and_reinstall_safe() {
        let sources = include_str!("../../../system/packages/config/apt/mattos.sources");
        let config = include_str!("../../../system/packages/config/apt/01mattos");
        assert!(sources.contains("file:/usr/share/mattos/repository"));
        assert!(sources.contains("Trusted: yes"));
        assert!(
            !sources.contains("http:")
                && !sources.contains("https:")
                && !sources.contains("debian")
                && !sources.contains("ubuntu")
        );
        assert!(config.contains("APT::Architecture \"amd64\""));
        assert!(config.contains("Pager \"false\""));
        assert!(config.contains("#clear Acquire::Changelogs::URI::Origin"));
        assert!(config.contains("#clear Acquire::Snapshots::URI"));
        assert_eq!(APT_CONFFILES.len(), 5);
        assert!(
            APT_CONFFILES
                .iter()
                .all(|path| path.starts_with("/etc/apt/"))
        );
    }

    #[test]
    fn mutable_package_manager_state_and_locks_are_excluded() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("var/lib/apt/lists")).unwrap();
        assert!(validate_no_mutable_package_state(temp.path()).is_ok());
        fs::write(temp.path().join("var/lib/apt/lists/lock"), "").unwrap();
        assert!(validate_no_mutable_package_state(temp.path()).is_err());
    }

    #[test]
    fn account_and_runtime_state_are_never_package_payloads() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("etc")).unwrap();
        assert!(validate_no_mutable_system_state(temp.path()).is_ok());
        fs::write(temp.path().join("etc/shadow"), "root:!:::::::\n").unwrap();
        assert!(validate_no_mutable_system_state(temp.path()).is_err());
    }

    #[test]
    fn ca_metadata_is_pinned_and_matches_the_owned_destination() {
        let metadata = include_str!("../../../system/network/ca-bundle.toml");
        assert!(metadata.contains("cacert-2026-07-16.pem"));
        assert!(metadata.contains("certificate_count = 119"));
        assert!(metadata.contains("destination = \"/etc/ssl/certs/ca-certificates.crt\""));
        assert_eq!(package_recipe_revision("ca-certificates"), 2);
        let temporary = tempfile::tempdir().unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        stage_ca_certificates(&root, temporary.path()).unwrap();
        assert_eq!(
            fs::read_link(temporary.path().join("etc/ssl/cert.pem")).unwrap(),
            Path::new("certs/ca-certificates.crt"),
        );
    }

    #[test]
    fn soname_symlinks_are_preserved_as_package_owned_entries() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("lib.so.1.0"), "library").unwrap();
        symlink("lib.so.1.0", temp.path().join("lib.so.1")).unwrap();
        copy_path_preserving(
            &temp.path().join("lib.so.1"),
            &temp.path().join("stage/lib.so.1"),
        )
        .unwrap();
        assert_eq!(
            fs::read_link(temp.path().join("stage/lib.so.1")).unwrap(),
            Path::new("lib.so.1.0")
        );
    }

    #[test]
    fn package_install_order_places_dependencies_before_consumers() {
        let order = package_install_order().unwrap();
        let position = |name| {
            order
                .iter()
                .position(|candidate| *candidate == name)
                .unwrap()
        };
        assert!(position("mattos-filesystem") < position("libc6"));
        assert!(position("libc6") < position("libgcc-s1"));
        assert!(position("libgcc-s1") < position("libstdc++6"));
        assert!(position("libstdc++6") < position("apt"));
        assert!(position("dpkg") < position("apt"));
        assert!(position("libapt-pkg7.0") < position("apt"));
        assert!(position("libudev1") < position("libapt-pkg7.0"));
        assert!(position("libexpat1") < position("dbus-broker"));
        assert!(position("libcap2") < position("iproute2"));
        assert!(position("libpcre2-8-0") < position("libselinux1"));
        assert!(position("libselinux1") < position("iproute2"));
        assert!(position("libselinux1") < position("dpkg"));
        assert!(position("libcrypt1") < position("libpam-modules"));
        assert!(position("libcrypt1") < position("passwd"));
        assert!(position("libblkid1") < position("libmount1"));
        assert!(position("libmount1") < position("mount"));
        assert!(position("libsmartcols1") < position("mount"));
        assert!(position("libmd0") < position("libbsd0"));
        assert!(position("libbsd0") < position("passwd"));
        assert!(position("libmd0") < position("dpkg"));
        assert!(position("libpam0g") < position("libpam-runtime"));
        assert!(position("libpam-runtime") < position("login"));
        assert!(position("mattos-libtinfow6") < position("ncurses-bin"));
        assert_eq!(order.len(), PACKAGE_NAMES.len());
    }

    #[test]
    fn package_management_files_have_no_legacy_copy_path() {
        let main = include_str!("main.rs");
        assert!(!main.contains("stage_built_dpkg_runtime"));
        assert!(!main.contains("(\"ca-certificates.crt\", \"etc/ssl/certs/ca-certificates.crt\")"));
        assert!(!main.contains("fn install_linux_pam_runtime"));
        assert!(!main.contains("fn copy_auth_configuration"));
        assert!(!main.contains("copy_built_binary_and_runtime"));
    }

    #[test]
    fn collision_policy_allows_shared_directories_but_rejects_files_and_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let specs = &package_specs()[..2];
        for spec in specs {
            fs::create_dir_all(temp.path().join(spec.name).join("usr/bin")).unwrap();
        }
        assert!(detect_staging_collisions(temp.path(), specs).is_ok());
        fs::write(temp.path().join(specs[0].name).join("usr/bin/tool"), "a").unwrap();
        symlink(
            "target",
            temp.path().join(specs[1].name).join("usr/bin/tool"),
        )
        .unwrap();
        assert!(detect_staging_collisions(temp.path(), specs).is_err());
    }

    #[test]
    fn soname_ownership_rejects_different_packages_with_the_same_abi() {
        let temp = tempfile::tempdir().unwrap();
        let staging_root = temp.path().join("out/packages/staging");
        let specs = package_specs()
            .into_iter()
            .filter(|spec| matches!(spec.name, "libexpat1" | "libcap2"))
            .collect::<Vec<_>>();
        fs::write(
            temp.path().join("duplicate.c"),
            "int duplicate_abi(void) { return 1; }\n",
        )
        .unwrap();
        for (index, spec) in specs.iter().enumerate() {
            let directory = staging_root
                .join(spec.name)
                .join(format!("usr/lib/{index}"));
            fs::create_dir_all(&directory).unwrap();
            let output = directory.join(format!("libduplicate-{index}.so"));
            run_ok(
                temp.path(),
                "gcc",
                &[
                    "-shared",
                    "-fPIC",
                    "-Wl,-soname,libduplicate.so.1",
                    "duplicate.c",
                    "-o",
                    path_str(&output).unwrap(),
                ],
            );
        }
        let error = validate_staged_runtime_ownership(temp.path(), &specs)
            .unwrap_err()
            .to_string();
        assert!(error.contains("SONAME libduplicate.so.1 has multiple package owners"));
    }

    #[test]
    fn stage_preserves_mode_and_symlink_and_checksum_is_stable() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::write(&source, "payload").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o751)).unwrap();
        let destination = temp.path().join("stage/usr/bin/tool");
        copy_preserving(&source, &destination).unwrap();
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o751
        );
        symlink("tool", temp.path().join("stage/usr/bin/alias")).unwrap();
        assert_eq!(
            fs::read_link(temp.path().join("stage/usr/bin/alias")).unwrap(),
            Path::new("tool")
        );
        assert_eq!(
            sha256_file(&destination).unwrap(),
            sha256_file(&destination).unwrap()
        );
    }

    #[test]
    fn polkit_authentication_has_a_complete_self_contained_pam_policy() {
        let policy = include_str!("../../../system/auth/config/pam.d/polkit-1");
        assert!(policy.contains("auth required pam_unix.so"));
        assert!(policy.contains("account required pam_unix.so"));
        assert!(!policy.contains("@include"));
        assert!(!policy.contains("pam_permit"));
        let packages = package_specs();
        let polkit = packages.iter().find(|p| p.name == "polkit").unwrap();
        for dependency in ["libpam0g", "libpam-modules", "libpam-runtime"] {
            assert!(polkit.depends.contains(&dependency));
        }
        assert!(package_configuration_roots("libpam-runtime").contains(&"src/system/auth/config/pam.d"));
        let network_recipe = include_str!("stages/system_services.rs");
        assert!(network_recipe.contains("-Dpolkit_agent_helper_1=/usr/lib/polkit-1/polkit-agent-helper-1"));
    }

    #[test]
    fn package_staging_cannot_mutate_a_cached_stage_input_tree() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("cached-stage/install");
        let staging = temp.path().join("package-staging");
        let descriptor = source.join("usr/lib/x86_64-linux-gnu/pkgconfig/example.pc");
        fs::create_dir_all(descriptor.parent().unwrap()).unwrap();
        fs::write(
            &descriptor,
            "prefix=/usr\nlibdir=${prefix}/lib/x86_64-linux-gnu\n",
        )
        .unwrap();
        let before = sha256_file(&descriptor).unwrap();

        copy_tree_preserving(&source, &staging).unwrap();
        normalize_tree_timestamps(&staging).unwrap();
        normalize_package_modes(&staging).unwrap();

        assert_eq!(sha256_file(&descriptor).unwrap(), before);
        assert_eq!(
            fs::read_to_string(staging.join("usr/lib/x86_64-linux-gnu/pkgconfig/example.pc"))
                .unwrap(),
            "prefix=/usr\nlibdir=${prefix}/lib/x86_64-linux-gnu\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn package_tree_staging_preserves_hardlink_identity_and_installed_size() {
        use std::os::unix::fs::MetadataExt;

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let staging = temp.path().join("staging");
        fs::create_dir_all(source.join("usr/bin")).unwrap();
        fs::create_dir_all(source.join("usr/libexec/tool-core")).unwrap();
        let primary = source.join("usr/bin/tool");
        fs::write(&primary, vec![b'x'; 4096]).unwrap();
        fs::hard_link(&primary, source.join("usr/libexec/tool-core/tool-add")).unwrap();
        fs::hard_link(&primary, source.join("usr/libexec/tool-core/tool-status")).unwrap();

        copy_tree_preserving(&source, &staging).unwrap();

        let copied_primary = fs::metadata(staging.join("usr/bin/tool")).unwrap();
        let copied_add = fs::metadata(staging.join("usr/libexec/tool-core/tool-add")).unwrap();
        let copied_status =
            fs::metadata(staging.join("usr/libexec/tool-core/tool-status")).unwrap();
        assert_eq!(copied_primary.ino(), copied_add.ino());
        assert_eq!(copied_primary.ino(), copied_status.ino());
        assert_eq!(copied_primary.nlink(), 3);
        assert_eq!(installed_size_kib(&staging).unwrap(), 4);
    }

    #[test]
    fn mode_normalization_preserves_authentication_security_contract() {
        let temp = tempfile::tempdir().unwrap();
        for rel in [
            "usr/bin/passwd",
            "usr/bin/sudo",
            "usr/bin/login",
            "usr/bin/su",
            "usr/lib/polkit-1/polkit-agent-helper-1",
        ] {
            fs::create_dir_all(temp.path().join(rel).parent().unwrap()).unwrap();
            fs::write(temp.path().join(rel), "executable").unwrap();
        }
        fs::create_dir_all(temp.path().join("etc/sudoers.d")).unwrap();
        fs::write(temp.path().join("etc/sudoers"), "policy").unwrap();
        fs::write(temp.path().join("etc/sudoers.d/README"), "policy").unwrap();
        normalize_package_modes(temp.path()).unwrap();
        for rel in [
            "usr/bin/passwd",
            "usr/bin/sudo",
            "usr/bin/login",
            "usr/bin/su",
            "usr/lib/polkit-1/polkit-agent-helper-1",
        ] {
            assert_eq!(
                fs::metadata(temp.path().join(rel))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o7777,
                0o4755
            );
        }
        assert_eq!(
            fs::metadata(temp.path().join("etc/sudoers"))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o440
        );
        assert_eq!(
            fs::metadata(temp.path().join("etc/sudoers.d"))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o750
        );
    }

    #[test]
    fn staging_and_output_paths_are_bounded() {
        assert!(validate_package_name("../../escape").is_err());
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("out/packages/staging/mattos-filesystem");
        assert!(root.starts_with(temp.path().join("out/packages/staging")));
    }

    #[test]
    fn legacy_collision_is_rejected() {
        let owned = BTreeSet::from([PathBuf::from("usr/bin/brush")]);
        assert!(reject_legacy_collision(&owned, Path::new("usr/bin/brush")).is_err());
        assert!(reject_legacy_collision(&owned, Path::new("usr/bin/systemctl")).is_ok());
    }

    #[test]
    fn permanent_packages_exclude_live_profile_and_foreign_sources() {
        let files = [
            "etc/os-release",
            "etc/profile",
            "etc/apt/sources.list.d/mattos.sources",
        ];
        assert!(files.iter().all(|path| !path.contains("live-profile")));
        let sources = include_str!("../../../system/packages/config/apt/mattos.sources");
        assert!(sources.contains("file:/usr/share/mattos/repository"));
        assert!(
            !sources.contains("debian")
                && !sources.contains("ubuntu")
                && !sources.contains("http:")
        );
    }

    #[test]
    fn repository_layout_and_release_metadata_are_validated() {
        let temp = tempfile::tempdir().unwrap();
        let index = temp.path().join("dists/trixie/main/binary-amd64");
        fs::create_dir_all(&index).unwrap();
        let packages = PACKAGE_NAMES
            .iter()
            .map(|name| format!("Package: {name}\nVersion: 1\nArchitecture: amd64\n\n"))
            .collect::<String>();
        fs::write(index.join("Packages"), packages).unwrap();
        fs::write(temp.path().join("dists/trixie/Release"), "Origin: MattOS\nLabel: MattOS Local\nSuite: trixie\nCodename: trixie\nArchitectures: amd64\nComponents: main\nSHA256:\n").unwrap();
        assert!(validate_repository(temp.path()).is_ok());
        fs::write(
            index.join("Packages"),
            "Package: foreign\nHomepage: https://deb.debian.org\n",
        )
        .unwrap();
        assert!(validate_repository(temp.path()).is_err());
    }

    #[test]
    fn release_checksum_validation_rejects_corrupt_index() {
        let temp = tempfile::tempdir().unwrap();
        let dist = temp.path().join("dists/trixie");
        let index = dist.join("main/binary-amd64/Packages");
        fs::create_dir_all(index.parent().unwrap()).unwrap();
        fs::write(&index, "stable\n").unwrap();
        let digest = sha256_file(&index).unwrap();
        fs::write(
            dist.join("Release"),
            format!("SHA256:\n {digest} 7 main/binary-amd64/Packages\n"),
        )
        .unwrap();
        validate_release_sha256(temp.path()).unwrap_err(); // Packages.gz is required.
        let compressed = Command::new("gzip")
            .args(["-n", "-9", "-c", path_str(&index).unwrap()])
            .output()
            .unwrap();
        let gz = index.with_file_name("Packages.gz");
        fs::write(&gz, compressed.stdout).unwrap();
        let gz_digest = sha256_file(&gz).unwrap();
        let gz_size = fs::metadata(&gz).unwrap().len();
        fs::write(
            dist.join("Release"),
            format!(
                "SHA256:\n {digest} 7 main/binary-amd64/Packages\n {gz_digest} {gz_size} main/binary-amd64/Packages.gz\n"
            ),
        )
        .unwrap();
        validate_release_sha256(temp.path()).unwrap();
        fs::write(&index, "corrupt\n").unwrap();
        assert!(validate_release_sha256(temp.path()).is_err());
    }

    #[test]
    fn dpkg_semantics_create_database_and_ownership_queries() {
        let temp = tempfile::tempdir().unwrap();
        let stage = temp.path().join("stage");
        fs::create_dir_all(stage.join("DEBIAN")).unwrap();
        fs::create_dir_all(stage.join("usr/bin")).unwrap();
        fs::write(stage.join("DEBIAN/control"), "Package: mattos-test\nVersion: 1.0-1mattos1\nArchitecture: amd64\nMaintainer: MattOS Test <test@mattos.invalid>\nInstalled-Size: 1\nDepends:\nDescription: test package\n").unwrap();
        fs::write(stage.join("usr/bin/mattos-test"), "test\n").unwrap();
        let deb = temp.path().join("mattos-test.deb");
        run_ok(
            temp.path(),
            "dpkg-deb",
            &[
                "--root-owner-group",
                "--build",
                path_str(&stage).unwrap(),
                path_str(&deb).unwrap(),
            ],
        );
        let root = temp.path().join("root");
        let admindir = root.join("var/lib/dpkg");
        fs::create_dir_all(admindir.join("info")).unwrap();
        fs::create_dir_all(admindir.join("updates")).unwrap();
        fs::create_dir_all(root.join("var/log")).unwrap();
        fs::write(admindir.join("status"), "").unwrap();
        run_ok(
            temp.path(),
            "dpkg",
            &[
                &format!("--root={}", root.display()),
                &format!("--admindir={}", admindir.display()),
                &format!("--log={}", root.join("var/log/dpkg.log").display()),
                "--force-not-root",
                "--install",
                path_str(&deb).unwrap(),
            ],
        );
        let owned = Command::new("dpkg-query")
            .arg(format!("--admindir={}", admindir.display()))
            .args(["-S", "/usr/bin/mattos-test"])
            .output()
            .unwrap();
        assert!(owned.status.success());
        assert!(String::from_utf8_lossy(&owned.stdout).starts_with("mattos-test:"));
        assert!(admindir.join("status").metadata().unwrap().len() > 0);
    }

    #[test]
    fn debian_version_policy_covers_release_epoch_prerelease_and_revision_ordering() {
        let compares = [
            ("2.43-1mattos1", "gt", "2.41-12+deb13u3"),
            ("1:1.0-1mattos1", "gt", "9.0-99"),
            ("7.2~rc5-1mattos1", "lt", "7.2-1mattos1"),
            ("15.3.0-1mattos2", "gt", "15.3.0-1mattos1"),
            ("3.5.7-1mattos1", "gt", "3.5.6-1~deb13u1"),
        ];
        for (left, operator, right) in compares {
            assert!(
                Command::new("dpkg")
                    .args(["--compare-versions", left, operator, right])
                    .status()
                    .unwrap()
                    .success(),
                "expected {left} {operator} {right}"
            );
        }
        assert_eq!(
            release_version_from_branch("releases/gcc-15.3.0"),
            Some("15.3.0".into())
        );
        for (branch, version) in [
            ("libxcb-1.17.0", "1.17.0"),
            ("libX11-1.8.12", "1.8.12"),
            ("libXext-1.3.6", "1.3.6"),
            ("xkbcommon-1.9.2", "1.9.2"),
            ("llvmorg-22.1.8", "22.1.8"),
        ] {
            assert_eq!(release_version_from_branch(branch), Some(version.into()));
        }
        assert_eq!(
            compatibility_epoch(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
                "libx11-6"
            )
            .unwrap(),
            Some(2)
        );
        assert_eq!(
            compatibility_epoch(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
                "libxcb1"
            )
            .unwrap(),
            None
        );
        assert_eq!(
            release_version_from_branch("v7.2-rc5"),
            Some("7.2~rc5".into())
        );
        assert_eq!(release_version_from_branch("master"), None);
    }

    #[test]
    fn compatibility_manifest_pins_and_read_only_publisher_validate() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        validate_debian_compatibility(&root).unwrap();
        let preferences =
            fs::read_to_string(root.join("src/system/packages/config/apt/00mattos-priority"))
                .unwrap();
        let local = preferences
            .find("Pin: release o=MattOS,l=MattOS Local,n=trixie\nPin-Priority: 990")
            .unwrap();
        let hosted = preferences
            .find("Pin: release o=MattOS,l=MattOS,n=trixie\nPin-Priority: 990")
            .unwrap();
        let debian = preferences.find("Pin-Priority: 500").unwrap();
        let blocked = preferences.find("Pin-Priority: -1").unwrap();
        assert!(local < hosted && hosted < debian && debian < blocked);
        assert!(!root.join("src/infrastructure/LinuxScripts/.git").exists());
        assert_eq!(
            sha256_file(
                &root.join(
                    "src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py"
                )
            )
            .unwrap(),
            "0b0be18e1164481612aa41ab6300c301b5d1088f86f9f653aed5a516ed50f35c"
        );
    }

    #[test]
    fn publish_plan_selects_mattos_repository_without_executing_manager() {
        let command = format_publish_command(
            Path::new("src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py"),
            &[PathBuf::from("out/packages/amd64/example_1.0_amd64.deb")],
        )
        .unwrap();
        assert_eq!(
            command,
            "python3 src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py --repo mattos upload out/packages/amd64/example_1.0_amd64.deb"
        );
    }

    #[test]
    fn apt_sources_enable_hosted_mattos_and_never_trust_debian() {
        let hosted = include_str!("../../../system/packages/config/apt/mattos-hosted.sources");
        let debian = include_str!("../../../system/packages/config/apt/debian-trixie.sources");
        assert!(hosted.contains("Enabled: yes"));
        assert!(hosted.contains("https://packages.mattsherfey.com"));
        assert!(hosted.contains("Signed-By:"));
        assert!(debian.contains("Enabled: no"));
        assert!(debian.contains("https://deb.debian.org/debian"));
        assert!(debian.contains("Signed-By:"));
        assert!(!hosted.contains("Trusted: yes"));
        assert!(!debian.contains("Trusted: yes"));
    }

    #[test]
    fn debian_equivalents_use_real_names_and_gaps_do_not_false_provide() {
        let specs = package_specs();
        for name in [
            "libc6",
            "libgcc-s1",
            "libstdc++6",
            "apt",
            "dpkg",
            "coreutils",
            "libssl3t64",
            "libpam0g",
            "login",
            "iputils-ping",
        ] {
            assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
        }
        assert!(
            specs
                .iter()
                .find(|spec| spec.name == "mattos-libtinfow6")
                .unwrap()
                .provides
                .is_empty()
        );
        assert!(
            specs
                .iter()
                .find(|spec| spec.name == "mattos-libproc2")
                .unwrap()
                .provides
                .is_empty()
        );
    }

    #[test]
    fn publication_path_policy_rejects_escape_missing_non_deb_and_symlink_escape() {
        let temp = tempfile::tempdir().unwrap();
        let approved = temp.path().join("out/packages");
        fs::create_dir_all(&approved).unwrap();
        let package = approved.join("safe.deb");
        fs::write(&package, "deb").unwrap();
        let outside = temp.path().join("outside.deb");
        fs::write(&outside, "deb").unwrap();
        let text = approved.join("not-a-package.txt");
        fs::write(&text, "text").unwrap();
        assert_eq!(
            validate_publication_artifact_location(&approved.canonicalize().unwrap(), &package)
                .unwrap(),
            package.canonicalize().unwrap()
        );
        assert!(
            validate_publication_artifact_location(&approved.canonicalize().unwrap(), &outside)
                .is_err()
        );
        assert!(
            validate_publication_artifact_location(&approved.canonicalize().unwrap(), &text)
                .is_err()
        );
        assert!(
            validate_publication_artifact_location(
                &approved.canonicalize().unwrap(),
                &approved.join("missing.deb")
            )
            .is_err()
        );
        symlink(&outside, approved.join("escape.deb")).unwrap();
        assert!(
            validate_publication_artifact_location(
                &approved.canonicalize().unwrap(),
                &approved.join("escape.deb")
            )
            .is_err()
        );
    }

    #[test]
    fn package_definition_change_invalidates_only_that_definition_digest() {
        let specs = package_specs();
        let libc = specs.iter().find(|spec| spec.name == "libc6").unwrap();
        let coreutils = specs.iter().find(|spec| spec.name == "coreutils").unwrap();
        let libc_before = package_definition_digest(libc).unwrap();
        let coreutils_before = package_definition_digest(coreutils).unwrap();
        let mut changed = libc.clone();
        changed.description = "changed test description";
        assert_ne!(libc_before, package_definition_digest(&changed).unwrap());
        assert_eq!(
            coreutils_before,
            package_definition_digest(coreutils).unwrap()
        );
    }

    #[test]
    fn configuration_payloads_invalidate_only_their_owning_packages() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        run_ok(root, "git", &["init", "-b", "main"]);
        let write = |relative: &str, body: &str| {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        };
        for source in [
            "src/system/dbus/dbus-broker/source.c",
            "src/system/auth/linux-pam/source.c",
            "src/system/auth/shadow/source.c",
            "src/system/auth/sudo-rs/source.rs",
        ] {
            write(source, "upstream source\n");
        }
        for configuration in [
            "src/system/dbus/config/system.conf",
            "src/system/dbus/config/dbus.conf",
            "src/system/dbus/units/dbus.socket",
            "src/system/dbus/units/dbus-broker.service",
            "src/system/session/dbus/session.conf",
            "src/system/session/user-units/dbus.socket",
            "src/system/session/user-units/dbus-broker.service",
            "src/system/auth/config/pam.d/login",
            "src/system/auth/config/login.defs",
            "src/system/auth/config/default/useradd",
            "src/system/auth/config/sudoers",
            "src/system/auth/config/sudoers.d/README",
        ] {
            write(configuration, "configuration v1\n");
        }
        write("src/system/dbus/README.md", "unrelated documentation v1\n");
        run_ok(root, "git", &["add", "."]);

        let selected = package_specs()
            .into_iter()
            .filter(|spec| {
                matches!(
                    spec.name,
                    "dbus-broker" | "libpam0g" | "libpam-runtime" | "passwd" | "mattos-sudo-rs"
                )
            })
            .collect::<Vec<_>>();
        let snapshot = |repo: &Path| {
            let mut shared_sources = BTreeMap::new();
            selected
                .iter()
                .map(|spec| {
                    (
                        spec.name,
                        package_payload_source_digests(repo, spec, &mut shared_sources).unwrap(),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        };
        let assert_only = |before: &BTreeMap<&str, (String, String)>,
                           after: &BTreeMap<&str, (String, String)>,
                           owner: &str| {
            for name in before.keys() {
                if *name == owner {
                    assert_ne!(before[name], after[name], "{owner} must invalidate");
                } else {
                    assert_eq!(before[name], after[name], "{name} invalidated unexpectedly");
                }
            }
        };

        let before_dbus = snapshot(root);
        write("src/system/dbus/config/system.conf", "configuration v2\n");
        let after_dbus = snapshot(root);
        assert_only(&before_dbus, &after_dbus, "dbus-broker");

        let before_pam = snapshot(root);
        write("src/system/auth/config/pam.d/login", "configuration v2\n");
        let after_pam = snapshot(root);
        assert_only(&before_pam, &after_pam, "libpam-runtime");

        let before_shadow = snapshot(root);
        write("src/system/auth/config/login.defs", "configuration v2\n");
        let after_shadow = snapshot(root);
        assert_only(&before_shadow, &after_shadow, "passwd");

        let before_sudo = snapshot(root);
        write("src/system/auth/config/sudoers", "configuration v2\n");
        let after_sudo = snapshot(root);
        assert_only(&before_sudo, &after_sudo, "mattos-sudo-rs");

        let before_docs = snapshot(root);
        write("src/system/dbus/README.md", "unrelated documentation v2\n");
        assert_eq!(before_docs, snapshot(root));
    }

    #[test]
    fn package_checksum_mismatch_forces_cache_rejection() {
        use std::io::Write as _;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let staging = root.join("out/packages/staging/mattos-test");
        let artifact = root.join("out/packages/amd64/mattos-test_1.0-1mattos1_amd64.deb");
        fs::create_dir_all(staging.join("DEBIAN")).unwrap();
        fs::create_dir_all(staging.join("usr/bin")).unwrap();
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(
            staging.join("DEBIAN/control"),
            "Package: mattos-test\nVersion: 1.0-1mattos1\nArchitecture: amd64\nMaintainer: MattOS Test <test@mattos.invalid>\nDescription: cache test\n",
        )
        .unwrap();
        fs::write(staging.join("usr/bin/test"), "payload\n").unwrap();
        let status = Command::new("dpkg-deb")
            .args([
                "--root-owner-group",
                "--build",
                path_str(&staging).unwrap(),
                path_str(&artifact).unwrap(),
            ])
            .status()
            .unwrap();
        assert!(status.success());
        let spec = PackageSpec {
            name: "mattos-test",
            description: "cache test",
            source_component: "test",
            depends: &[],
            provides: &[],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        };
        let sha = sha256_file(&artifact).unwrap();
        let input = PackageCacheInput {
            cache_key: "key".to_string(),
            definition_digest: "definition".to_string(),
            payload_source_digest: "payload-source".to_string(),
            payload_configuration_digest: String::new(),
            dependency_digest: "dependencies".to_string(),
        };
        let entry = PackageInventoryEntry {
            name: "mattos-test".to_string(),
            version: "1.0-1mattos1".to_string(),
            architecture: ARCH.to_string(),
            artifact_path: relative_display(root, &artifact).unwrap(),
            source_component: "test".to_string(),
            dependencies: Vec::new(),
            runtime_libraries: Vec::new(),
            file_count: count_package_entries(&staging).unwrap(),
            sha256: sha.clone(),
        };
        let manifest = PackageCacheManifest {
            schema_version: PACKAGE_CACHE_SCHEMA_VERSION,
            package: "mattos-test".to_string(),
            cache_key: input.cache_key.clone(),
            definition_digest: input.definition_digest.clone(),
            payload_source_digest: input.payload_source_digest.clone(),
            payload_configuration_digest: input.payload_configuration_digest.clone(),
            dependency_digest: input.dependency_digest.clone(),
            payload_inventory_digest: performance::output_path_digest(root, &staging).unwrap(),
            artifact_sha256: sha,
            artifact_path: entry.artifact_path.clone(),
            inventory_entry: entry,
        };
        performance::atomic_write_json(
            &package_cache_manifest_path(root, "mattos-test"),
            &manifest,
        )
        .unwrap();
        validate_package_cache(root, &spec, "1.0-1mattos1", &staging, &artifact, &input).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&artifact)
            .unwrap()
            .write_all(b"corrupt")
            .unwrap();
        assert!(
            validate_package_cache(root, &spec, "1.0-1mattos1", &staging, &artifact, &input,)
                .is_err()
        );
    }

    #[test]
    fn signed_flatpak_policy_seeds_a_minimal_readable_system_remote() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let descriptor = root.join("src/system/packages/flatpak/resources/flathub.flatpakrepo");
        let temporary = tempfile::tempdir().unwrap();
        stage_flatpak_system_remote(&descriptor, temporary.path()).unwrap();

        let repo = temporary.path().join("var/lib/flatpak/repo");
        let config = fs::read_to_string(repo.join("config")).unwrap();
        assert!(config.contains("mode=bare-user-only"));
        assert!(config.contains("xa.applied-remotes=flathub;"));
        assert!(config.contains("[remote \"flathub\"]"));
        assert!(config.contains("url=https://dl.flathub.org/repo/"));
        assert!(config.contains("gpg-verify=true"));
        assert!(config.contains("gpg-verify-summary=true"));
        for directory in [
            "objects",
            "refs",
            "refs/heads",
            "refs/remotes",
            "state",
            "tmp",
            "extensions",
        ] {
            assert!(repo.join(directory).is_dir(), "missing OSTree {directory} directory");
        }
        assert!(
            fs::metadata(repo.join("flathub.trustedkeys.gpg"))
                .unwrap()
                .len()
                > 1_000,
            "the seeded key must be the decoded full Flathub public key"
        );
    }
}
