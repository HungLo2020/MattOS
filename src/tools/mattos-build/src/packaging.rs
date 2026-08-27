use super::*;
use clap::Subcommand;
use filetime::{FileTime, set_file_times, set_symlink_file_times};

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
const PACKAGE_NAMES: &[&str] = &[
    "mattos-filesystem",
    "mattos-compat",
    "libc6",
    "libgcc-s1",
    "libstdc++6",
    "linux-libc-dev",
    "linux-modules-7.2.0-rc5-mattos",
    "libc6-dev",
    "mattos-libgcc-dev",
    "mattos-libstdc++-dev",
    "binutils",
    "mattos-gcc-common",
    "cpp",
    "gcc",
    "g++",
    "make",
    "libc-bin",
    "locales",
    "iso-codes",
    "tzdata",
    "linux-firmware",
    "wireless-regdb",
    "mattos-base-files",
    "ca-certificates",
    "mattos-brush",
    "coreutils",
    "curl",
    "libmd0",
    "libbsd0",
    "libzstd1",
    "mattos-libcrypto3",
    "libssl3t64",
    "libelf1t64",
    "libpcre2-8-0",
    "libselinux1",
    "libcrypt1",
    "libblkid1",
    "libmount1",
    "libsmartcols1",
    "libuuid1",
    "libfdisk1",
    "mount",
    "util-linux",
    "dpkg",
    "libgpg-error0",
    "libgcrypt20",
    "libassuan9",
    "libksba8",
    "libnpth0",
    "gpgv",
    "libapt-pkg7.0",
    "apt",
    "mattos-libtinfow6",
    "libncursesw6",
    "libreadline8",
    "libndp0",
    "ncurses-base",
    "ncurses-bin",
    "libkmod2",
    "kmod",
    "mattos-libproc2",
    "procps",
    "libsystemd0",
    "libudev1",
    "udev",
    "libexpat1",
    "libcap2",
    "libattr1",
    "libacl1",
    "zlib1g",
    "libbz2-1.0",
    "gzip",
    "bzip2",
    "liblz4-1",
    "liblzma5",
    "xz-utils",
    "libxxhash0",
    "tar",
    "zstd",
    "patch",
    "libmagic1",
    "file",
    "less",
    "git",
    "openssh-client",
    "openssh-server",
    "libffi8",
    "libffi-dev",
    "libwayland-client0",
    "libwayland-server0",
    "libwayland-egl1",
    "libxkbcommon0",
    "xkb-data",
    "libseat1",
    "libdisplay-info3",
    "libevdev2",
    "libinput10",
    "libpixman-1-0",
    "libdrm2",
    "libdrm-amdgpu1",
    "libdrm-nouveau2",
    "libxau6",
    "libxdmcp6",
    "libxcb1",
    "libx11-6",
    "libxext6",
    "libglvnd0",
    "libopengl0",
    "libgbm1",
    "libegl1",
    "libgles1",
    "libgles2",
    "libegl-mesa0",
    "libgl1-mesa-dri",
    "libvulkan1",
    "libvulkan-dev",
    "mesa-vulkan-drivers",
    "vulkan-tools",
    "linux-modules-nvidia-595-open-7.2.0-rc5-mattos",
    "nvidia-firmware-595",
    "libnvidia-gl-595",
    "libnvidia-compute-595",
    "libnvidia-encode-595",
    "libnvidia-decode-595",
    "nvidia-utils-595",
    "nvidia-driver-595-open",
    "cosmic-comp",
    "cosmic-edit",
    "cosmic-initial-setup",
    "cosmic-desktop",
    "libduktape207",
    "polkit",
    "network-manager",
    "mattos-cozy",
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
    "libdbus-1-3",
    "libdav1d7",
    "libglib2.0-0t64",
    "pipewire",
    "dbus-broker",
    "libpam0g",
    "mattos-libpam-misc0",
    "libpam-modules",
    "libpam-runtime",
    "passwd",
    "mattos-sudo-rs",
    "login",
    "iproute2",
    "iputils-ping",
    "btrfs-progs",
    "dosfstools",
    "e2fsprogs",
    "mattos-installer",
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

#[derive(Clone, Debug, Serialize)]
struct PackageSpec {
    name: &'static str,
    description: &'static str,
    source_component: &'static str,
    depends: &'static [&'static str],
    provides: &'static [&'static str],
    conflicts: &'static [&'static str],
    replaces: &'static [&'static str],
    essential: bool,
    priority: &'static str,
}

#[derive(Debug, Serialize, Deserialize)]
struct PackageInventory {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PackageCacheManifest {
    schema_version: u32,
    package: String,
    cache_key: String,
    definition_digest: String,
    payload_source_digest: String,
    #[serde(default)]
    payload_configuration_digest: String,
    dependency_digest: String,
    payload_inventory_digest: String,
    artifact_sha256: String,
    artifact_path: String,
    inventory_entry: PackageInventoryEntry,
}

#[derive(Clone, Debug)]
struct PackageCacheInput {
    cache_key: String,
    definition_digest: String,
    payload_source_digest: String,
    payload_configuration_digest: String,
    dependency_digest: String,
}

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

#[derive(Debug, Serialize, Deserialize)]
struct PackageFacts {
    schema_version: u32,
    artifact_sha256: String,
    package: String,
    version: String,
    architecture: String,
    control: BTreeMap<String, String>,
    conffiles: Vec<String>,
    payload: Vec<PackagePayloadFact>,
    elf_members: Vec<PackageElfMember>,
    dependencies: Vec<String>,
    installed_size_kib: u64,
    provenance: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PackagePayloadFact {
    path: String,
    kind: String,
    mode: u32,
    symlink_target: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PackageElfMember {
    path: String,
    content_sha256: String,
    soname: Option<String>,
    needed: Vec<String>,
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
struct BootstrapConsumer {
    package: String,
    path: String,
}

fn package_specs() -> Vec<PackageSpec> {
    vec![
        PackageSpec {
            name: "mattos-filesystem",
            description: "MattOS base filesystem hierarchy",
            source_component: "MattOS",
            depends: &[],
            provides: &["mattos-filesystem-hierarchy"],
            conflicts: &[],
            replaces: &[],
            essential: true,
            priority: "required",
        },
        PackageSpec {
            name: "libc6",
            description: "GNU C Library runtime built for MattOS",
            source_component: "glibc",
            depends: &["mattos-filesystem"],
            provides: &["libc6", "mattos-runtime-abi"],
            conflicts: &[],
            replaces: &[],
            essential: true,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-compat",
            description: "Isolated Debian, Fedora, and Pop!_OS application compatibility manager",
            source_component: "mattos-compat",
            depends: &[
                "mattos-filesystem",
                "libsystemd0",
                "libcap2",
                "libmount1",
                "libblkid1",
                "libselinux1",
                "liblzma5",
                "libzstd1",
            ],
            provides: &["mattos-compat"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libgcc-s1",
            description: "GCC shared unwinding runtime built for MattOS",
            source_component: "gcc",
            depends: &["mattos-filesystem", "libc6"],
            provides: &["libgcc-s1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "libstdc++6",
            description: "GNU C++ runtime library built for MattOS",
            source_component: "gcc",
            depends: &["mattos-filesystem", "libc6", "libgcc-s1"],
            provides: &["libstdc++6"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "linux-libc-dev",
            description: "Linux userspace API headers for MattOS native development",
            source_component: "linux",
            depends: &["mattos-filesystem"],
            provides: &["linux-libc-dev"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "linux-modules-7.2.0-rc5-mattos",
            description: "MattOS generic x86_64 kernel modules and depmod metadata",
            source_component: "kernel-modules",
            depends: &["kmod"],
            provides: &["linux-modules-amd64", "linux-modules-generic"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libc6-dev",
            description: "GNU C Library headers and link-time files for MattOS",
            source_component: "glibc",
            depends: &["libc6", "linux-libc-dev"],
            provides: &["libc6-dev"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "mattos-libgcc-dev",
            description: "GCC support headers and static link libraries for MattOS",
            source_component: "gcc",
            depends: &["libc6-dev", "libgcc-s1"],
            provides: &["libgcc-dev"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "mattos-libstdc++-dev",
            description: "GNU C++ standard library headers and link-time files for MattOS",
            source_component: "gcc",
            depends: &["libc6-dev", "mattos-libgcc-dev", "libstdc++6"],
            provides: &["libstdc++-dev"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "binutils",
            description: "GNU binary utilities built natively for MattOS",
            source_component: "binutils",
            depends: &["libc6"],
            provides: &["binutils"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "mattos-gcc-common",
            description: "Shared compiler support and internal GCC helpers for MattOS",
            source_component: "gcc",
            depends: &["binutils", "mattos-libgcc-dev", "libstdc++6", "zlib1g"],
            provides: &["gcc-common"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "cpp",
            description: "GNU C preprocessor built natively for MattOS",
            source_component: "gcc",
            depends: &["mattos-gcc-common"],
            provides: &["cpp"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "gcc",
            description: "GNU C compiler built natively for MattOS",
            source_component: "gcc",
            depends: &["cpp", "mattos-gcc-common", "libc6-dev"],
            provides: &["c-compiler", "gcc"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "g++",
            description: "GNU C++ compiler built natively for MattOS",
            source_component: "gcc",
            depends: &["gcc", "mattos-libstdc++-dev"],
            provides: &["c++-compiler", "g++"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "make",
            description: "GNU Make built natively for MattOS",
            source_component: "make",
            depends: &["libc6"],
            provides: &["make"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libc-bin",
            description: "GNU C Library runtime utilities built for MattOS",
            source_component: "glibc",
            depends: &["mattos-filesystem", "libc6"],
            provides: &["libc-bin"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "locales",
            description: "glibc locale source data and localedef utility for offline MattOS locale generation",
            source_component: "glibc",
            depends: &["libc6", "libc-bin"],
            provides: &["locales"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "iso-codes",
            description: "Pinned ISO language and territory metadata for offline COSMIC locale selection",
            source_component: "iso-codes",
            depends: &["libc6"],
            provides: &["iso-codes"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "tzdata",
            description: "IANA timezone database built from pinned tzdata source",
            source_component: "tzdata",
            depends: &["libc6"],
            provides: &["tzdata"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "linux-firmware",
            description: "Broad upstream Linux firmware collection for supported modern hardware",
            source_component: "linux-firmware",
            depends: &["mattos-filesystem"],
            provides: &["linux-firmware", "firmware-linux"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "wireless-regdb",
            description: "Signed wireless regulatory database built from pinned upstream data",
            source_component: "wireless-regdb",
            depends: &["mattos-filesystem"],
            provides: &["wireless-regdb"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-base-files",
            description: "MattOS identity and baseline configuration",
            source_component: "MattOS",
            depends: &["mattos-filesystem"],
            provides: &["mattos-release"],
            conflicts: &["base-files"],
            replaces: &["base-files"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "ca-certificates",
            description: "Pinned Mozilla certificate authority bundle for MattOS",
            source_component: "ca-certificates",
            depends: &["mattos-filesystem"],
            provides: &["ca-certificates"],
            conflicts: &["ca-certificates"],
            replaces: &["ca-certificates"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-brush",
            description: "Brush shell with sh and bash entry points built for MattOS",
            source_component: "brush",
            depends: &["mattos-filesystem", "libgcc-s1"],
            provides: &["mattos-shell", "sh", "bash"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "coreutils",
            description: "uutils core utilities built for MattOS",
            source_component: "coreutils",
            depends: &["mattos-filesystem", "libgcc-s1"],
            provides: &["coreutils"],
            conflicts: &["coreutils"],
            replaces: &["coreutils"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "curl",
            description: "curl command-line transfer client built for MattOS",
            source_component: "curl",
            depends: &[
                "mattos-filesystem",
                "ca-certificates",
                "zlib1g",
                "libzstd1",
                "mattos-libcrypto3",
                "libssl3t64",
            ],
            provides: &["curl"],
            conflicts: &["curl"],
            replaces: &["curl"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libmd0",
            description: "libmd message-digest runtime library built for MattOS",
            source_component: "libmd",
            depends: &[],
            provides: &["libmd0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libbsd0",
            description: "libbsd portability runtime library built for MattOS",
            source_component: "libbsd",
            depends: &["libmd0"],
            provides: &["libbsd0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libzstd1",
            description: "Zstandard compression runtime library built for MattOS",
            source_component: "zstd",
            depends: &[],
            provides: &["libzstd1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libcrypto3",
            description: "OpenSSL cryptography runtime library built for MattOS",
            source_component: "openssl",
            depends: &["zlib1g", "libzstd1"],
            provides: &["libcrypto3"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libssl3t64",
            description: "OpenSSL TLS runtime library built for MattOS",
            source_component: "openssl",
            depends: &["mattos-libcrypto3", "zlib1g", "libzstd1"],
            provides: &["libssl3t64"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libelf1t64",
            description: "elfutils libelf runtime library built for MattOS",
            source_component: "elfutils",
            depends: &["zlib1g", "libzstd1"],
            provides: &["libelf1t64"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libpcre2-8-0",
            description: "PCRE2 8-bit regular expression runtime library built for MattOS",
            source_component: "pcre2",
            depends: &[],
            provides: &["libpcre2-8-0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libselinux1",
            description: "SELinux userspace runtime library built for MattOS",
            source_component: "selinux",
            depends: &["libpcre2-8-0"],
            provides: &["libselinux1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libcrypt1",
            description: "libxcrypt password hashing runtime library built for MattOS",
            source_component: "libxcrypt",
            depends: &[],
            provides: &["libcrypt1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "libblkid1",
            description: "util-linux block device identification runtime library built for MattOS",
            source_component: "util-linux",
            depends: &[],
            provides: &["libblkid1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libmount1",
            description: "util-linux mount runtime library built for MattOS",
            source_component: "util-linux",
            depends: &["libblkid1"],
            provides: &["libmount1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libsmartcols1",
            description: "util-linux structured table runtime library built for MattOS",
            source_component: "util-linux",
            depends: &[],
            provides: &["libsmartcols1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mount",
            description: "util-linux mount and unmount tools built for MattOS",
            source_component: "util-linux",
            depends: &["libblkid1", "libmount1", "libsmartcols1", "libselinux1"],
            provides: &["mount"],
            conflicts: &["mount"],
            replaces: &["mount"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "dpkg",
            description: "dpkg binary package management runtime built for MattOS",
            source_component: "dpkg",
            depends: &[
                "mattos-filesystem",
                "tar",
                "zlib1g",
                "libbz2-1.0",
                "liblzma5",
                "libzstd1",
                "libmd0",
                "libpcre2-8-0",
                "libselinux1",
            ],
            provides: &["dpkg"],
            conflicts: &["dpkg"],
            replaces: &["dpkg"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "libgpg-error0",
            description: "GnuPG error and runtime support library built for MattOS",
            source_component: "libgpg-error",
            depends: &[],
            provides: &["libgpg-error0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libgcrypt20",
            description: "GnuPG cryptographic runtime library built for MattOS",
            source_component: "libgcrypt",
            depends: &["libgpg-error0"],
            provides: &["libgcrypt20"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libassuan9",
            description: "GnuPG IPC runtime library built for MattOS",
            source_component: "libassuan",
            depends: &["libgpg-error0"],
            provides: &["libassuan9"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libksba8",
            description: "GnuPG X.509 and CMS runtime library built for MattOS",
            source_component: "libksba",
            depends: &["libgpg-error0"],
            provides: &["libksba8"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libnpth0",
            description: "GnuPG non-preemptive threading runtime library built for MattOS",
            source_component: "npth",
            depends: &[],
            provides: &["libnpth0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "gpgv",
            description: "Source-built OpenPGP signature verifier used by APT",
            source_component: "gnupg",
            depends: &[
                "libc6",
                "zlib1g",
                "libgpg-error0",
                "libgcrypt20",
                "libassuan9",
                "libksba8",
                "libnpth0",
            ],
            provides: &["gpgv"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "libapt-pkg7.0",
            description: "APT public runtime library built for MattOS",
            source_component: "apt",
            depends: &[
                "libgcc-s1",
                "libstdc++6",
                "libudev1",
                "libsystemd0",
                "zlib1g",
                "libbz2-1.0",
                "liblz4-1",
                "liblzma5",
                "libxxhash0",
                "libzstd1",
                "mattos-libcrypto3",
                "libssl3t64",
                "gpgv",
            ],
            provides: &["libapt-pkg7.0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "apt",
            description: "APT command-line package manager and local repository methods for MattOS",
            source_component: "apt",
            depends: &[
                "libgcc-s1",
                "libstdc++6",
                "ca-certificates",
                "dpkg",
                "libapt-pkg7.0",
                "libudev1",
                "libsystemd0",
                "zlib1g",
                "libbz2-1.0",
                "liblz4-1",
                "liblzma5",
                "libxxhash0",
                "libzstd1",
                "mattos-libcrypto3",
                "libssl3t64",
                "gpgv",
            ],
            provides: &["apt"],
            conflicts: &["apt"],
            replaces: &["apt"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libtinfow6",
            description: "ncurses wide-character terminfo runtime library built for MattOS",
            source_component: "ncurses",
            depends: &[],
            provides: &[],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libncursesw6",
            description: "ncurses wide-character runtime library built for MattOS",
            source_component: "ncurses",
            depends: &["mattos-libtinfow6"],
            provides: &["libncursesw6"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "ncurses-base",
            description: "MattOS essential terminal capability database",
            source_component: "ncurses",
            depends: &["mattos-filesystem"],
            provides: &["terminfo"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "ncurses-bin",
            description: "ncurses terminal utilities built for MattOS",
            source_component: "ncurses",
            depends: &["mattos-libtinfow6", "ncurses-base"],
            provides: &["ncurses-bin"],
            conflicts: &["ncurses-bin"],
            replaces: &["ncurses-bin"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libreadline8",
            description: "GNU Readline runtime library built for MattOS",
            source_component: "readline",
            depends: &["libc6"],
            provides: &["libreadline8"],
            conflicts: &[], replaces: &[], essential: false, priority: "important",
        },
        PackageSpec {
            name: "libndp0",
            description: "IPv6 Neighbor Discovery Protocol runtime library built for MattOS",
            source_component: "libndp",
            depends: &["libc6"],
            provides: &["libndp0"],
            conflicts: &[], replaces: &[], essential: false, priority: "important",
        },
        PackageSpec {
            name: "libkmod2",
            description: "kmod runtime library built for MattOS",
            source_component: "kmod",
            depends: &[],
            provides: &["libkmod2"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "kmod",
            description: "Linux kernel module management tools built for MattOS",
            source_component: "kmod",
            depends: &["libkmod2"],
            provides: &["kmod"],
            conflicts: &["kmod"],
            replaces: &["kmod"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libproc2",
            description: "procps process information runtime library built for MattOS",
            source_component: "procps-ng",
            depends: &[],
            provides: &[],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "procps",
            description: "procps process inspection utilities built for MattOS",
            source_component: "procps-ng",
            depends: &["mattos-libproc2", "libncursesw6", "mattos-libtinfow6"],
            provides: &["procps"],
            conflicts: &["procps"],
            replaces: &["procps"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libsystemd0",
            description: "systemd public runtime library built for MattOS",
            source_component: "systemd",
            depends: &[],
            provides: &["libsystemd0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libudev1",
            description: "systemd device enumeration runtime library built for MattOS",
            source_component: "systemd",
            depends: &[],
            provides: &["libudev1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "udev",
            description: "udev hardware database sources and prebuilt database for MattOS",
            source_component: "systemd",
            depends: &["libudev1", "libblkid1"],
            provides: &["udev"],
            conflicts: &["udev"],
            replaces: &["udev"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libexpat1",
            description: "Expat XML parser runtime library built for MattOS",
            source_component: "expat",
            depends: &[],
            provides: &["libexpat1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libcap2",
            description: "Linux capabilities runtime library built for MattOS",
            source_component: "libcap",
            depends: &[],
            provides: &["libcap2"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libattr1",
            description: "Extended attribute runtime library built for MattOS",
            source_component: "attr",
            depends: &[],
            provides: &["libattr1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libacl1",
            description: "POSIX access control list runtime library built for MattOS",
            source_component: "acl",
            depends: &[],
            provides: &["libacl1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "zlib1g",
            description: "zlib compression runtime library built for MattOS",
            source_component: "zlib",
            depends: &[],
            provides: &["zlib1g"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libbz2-1.0",
            description: "bzip2 compression runtime library built for MattOS",
            source_component: "bzip2",
            depends: &[],
            provides: &["libbz2-1.0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "liblz4-1",
            description: "LZ4 compression runtime library built for MattOS",
            source_component: "lz4",
            depends: &[],
            provides: &["liblz4-1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "liblzma5",
            description: "XZ Utils liblzma compression runtime library built for MattOS",
            source_component: "xz",
            depends: &[],
            provides: &["liblzma5"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libxxhash0",
            description: "xxHash runtime library built for MattOS",
            source_component: "xxhash",
            depends: &[],
            provides: &["libxxhash0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "tar",
            description: "GNU tar archive utility built for MattOS",
            source_component: "tar",
            depends: &["libacl1"],
            provides: &["tar"],
            conflicts: &["tar"],
            replaces: &["tar"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "libdbus-1-3",
            description: "Reference D-Bus client library and private-session tools built for MattOS",
            source_component: "dbus",
            depends: &["libexpat1"],
            provides: &["libdbus-1-3"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libdav1d7",
            description: "AV1 decoder runtime used by COSMIC image handling",
            source_component: "dav1d",
            depends: &["libc6"],
            provides: &["libdav1d7"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libglib2.0-0t64",
            description: "GLib, GObject, and GIO runtime for the COSMIC desktop",
            source_component: "glib",
            depends: &["libc6", "libffi8", "libpcre2-8-0", "zlib1g"],
            provides: &["libglib2.0-0", "libglib2.0-0t64"],
            conflicts: &[],
            replaces: &["libglib2.0-0"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "pipewire",
            description: "PipeWire audio service, PulseAudio compatibility, and SPA runtime",
            source_component: "pipewire",
            depends: &["libc6", "libdbus-1-3", "libsystemd0"],
            provides: &[
                "pipewire",
                "pipewire-pulse",
                "libpipewire-0.3-0",
                "libspa-0.2-modules",
            ],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "dbus-broker",
            description: "D-Bus message broker and MattOS bus policy",
            source_component: "dbus-broker",
            depends: &["libexpat1", "libsystemd0"],
            provides: &["dbus-system-bus"],
            // MattOS keeps the reference daemon only for dbus-run-session and
            // private buses.  The broker owns the system/user systemd units;
            // the two package payloads do not overlap.
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libpam0g",
            description: "Linux PAM core runtime library built for MattOS",
            source_component: "linux-pam",
            depends: &[],
            provides: &["libpam0g"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-libpam-misc0",
            description: "Linux PAM miscellaneous runtime library built for MattOS",
            source_component: "linux-pam",
            depends: &["libpam0g"],
            provides: &["libpam-misc0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "libpam-modules",
            description: "Linux PAM authentication modules built for MattOS",
            source_component: "linux-pam",
            depends: &["libpam0g", "libcrypt1"],
            provides: &["libpam-modules"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "libpam-runtime",
            description: "MattOS PAM policy and authentication helper runtime",
            source_component: "linux-pam",
            depends: &["libpam0g", "libpam-modules", "libcrypt1"],
            provides: &["libpam-runtime"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "passwd",
            description: "Shadow account administration tools built for MattOS",
            source_component: "shadow",
            depends: &[
                "libpam0g",
                "mattos-libpam-misc0",
                "libpam-runtime",
                "libbsd0",
                "libmd0",
                "libcrypt1",
            ],
            provides: &["passwd"],
            conflicts: &["passwd"],
            replaces: &["passwd"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-sudo-rs",
            description: "Privilege delegation tools built for MattOS",
            source_component: "sudo-rs",
            depends: &["libgcc-s1", "libpam0g", "libpam-runtime"],
            provides: &["sudo"],
            conflicts: &["sudo"],
            replaces: &["sudo"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "login",
            description: "util-linux login, su, and agetty tools built for MattOS",
            source_component: "util-linux",
            depends: &["libpam0g", "mattos-libpam-misc0", "libpam-runtime"],
            provides: &["login"],
            conflicts: &["login"],
            replaces: &["login"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "iproute2",
            description: "Linux routing and network configuration tools built for MattOS",
            source_component: "iproute2",
            depends: &[
                "libcap2",
                "zlib1g",
                "libzstd1",
                "libelf1t64",
                "libpcre2-8-0",
                "libselinux1",
            ],
            provides: &["iproute2"],
            conflicts: &["iproute2"],
            replaces: &["iproute2"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "iputils-ping",
            description: "Linux ping and tracepath network diagnostics built for MattOS",
            source_component: "iputils",
            depends: &[],
            provides: &["iputils-ping"],
            conflicts: &["iputils-ping"],
            replaces: &["iputils-ping"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "btrfs-progs",
            description: "Btrfs filesystem administration tools built for MattOS",
            source_component: "btrfs-progs",
            depends: &["libc6", "libblkid1", "libuuid1", "zlib1g", "libzstd1"],
            provides: &["btrfs-progs"],
            conflicts: &["btrfs-progs"],
            replaces: &["btrfs-progs"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "dosfstools",
            description: "FAT filesystem administration tools built for MattOS",
            source_component: "dosfstools",
            depends: &["libc6"],
            provides: &["dosfstools"],
            conflicts: &["dosfstools"],
            replaces: &["dosfstools"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "e2fsprogs",
            description: "ext2/ext3/ext4 filesystem utilities built for MattOS",
            source_component: "e2fsprogs",
            depends: &["libc6", "libblkid1", "libuuid1"],
            provides: &["e2fsprogs"],
            conflicts: &["e2fsprogs"],
            replaces: &["e2fsprogs"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-installer",
            description: "MattOS shared installation backend with CLI and native COSMIC frontends",
            source_component: "installer",
            depends: &[
                "libc6",
                "libgcc-s1",
                "util-linux",
                "libblkid1",
                "libuuid1",
                "zlib1g",
                "libzstd1",
                "passwd",
                "coreutils",
                "xz-utils",
                "btrfs-progs",
                "dosfstools",
                "e2fsprogs",
                "libcrypt1",
                "libwayland-client0",
                "libxkbcommon0",
                "cosmic-comp",
                "linux-modules-7.2.0-rc5-mattos",
                "linux-firmware",
                "wireless-regdb",
            ],
            provides: &[
                "mattos-installer",
                "mattos-installer-cli",
                "mattos-installer-cosmic",
            ],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libuuid1",
            description: "util-linux UUID runtime library built for MattOS",
            source_component: "util-linux",
            depends: &[],
            provides: &["libuuid1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libfdisk1",
            description: "util-linux partitioning runtime library built for MattOS",
            source_component: "util-linux",
            depends: &["libblkid1", "libuuid1", "libsmartcols1"],
            provides: &["libfdisk1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "util-linux",
            description: "Essential Linux system administration utilities built for MattOS",
            source_component: "util-linux",
            depends: &[
                "libblkid1",
                "libmount1",
                "libsmartcols1",
                "libuuid1",
                "libfdisk1",
                "libselinux1",
                "libncursesw6",
                "mattos-libtinfow6",
            ],
            provides: &["util-linux"],
            conflicts: &["util-linux"],
            replaces: &["util-linux"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "gzip",
            description: "GNU gzip compression tools built for MattOS",
            source_component: "gzip",
            depends: &[],
            provides: &["gzip"],
            conflicts: &["gzip"],
            replaces: &["gzip"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "bzip2",
            description: "bzip2 compression tools built for MattOS",
            source_component: "bzip2",
            depends: &["libbz2-1.0"],
            provides: &["bzip2"],
            conflicts: &["bzip2"],
            replaces: &["bzip2"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "xz-utils",
            description: "XZ compression tools built for MattOS",
            source_component: "xz",
            depends: &["liblzma5"],
            provides: &["xz-utils"],
            conflicts: &["xz-utils"],
            replaces: &["xz-utils"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "zstd",
            description: "Zstandard compression tools built for MattOS",
            source_component: "zstd",
            depends: &["libzstd1"],
            provides: &["zstd"],
            conflicts: &["zstd"],
            replaces: &["zstd"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "patch",
            description: "GNU patch utility built for MattOS",
            source_component: "patch",
            depends: &["libattr1"],
            provides: &["patch"],
            conflicts: &["patch"],
            replaces: &["patch"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libmagic1",
            description: "libmagic runtime and compiled magic database built for MattOS",
            source_component: "file",
            depends: &["zlib1g"],
            provides: &["libmagic1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "file",
            description: "File type identification utility built for MattOS",
            source_component: "file",
            depends: &["libmagic1", "zlib1g"],
            provides: &["file"],
            conflicts: &["file"],
            replaces: &["file"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "less",
            description: "Interactive terminal pager built for MattOS",
            source_component: "less",
            depends: &["mattos-libtinfow6", "libpcre2-8-0"],
            provides: &["less"],
            conflicts: &["less"],
            replaces: &["less"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "git",
            description: "Git distributed version control with HTTPS support built for MattOS",
            source_component: "git",
            depends: &[
                "curl",
                "ca-certificates",
                "zlib1g",
                "libzstd1",
                "libexpat1",
                "libpcre2-8-0",
                "mattos-libcrypto3",
                "libssl3t64",
            ],
            provides: &["git"],
            conflicts: &["git"],
            replaces: &["git"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "openssh-client",
            description: "OpenSSH client tools built for MattOS",
            source_component: "openssh",
            depends: &["zlib1g", "libzstd1", "mattos-libcrypto3", "libssl3t64"],
            provides: &["ssh-client"],
            conflicts: &["openssh-client"],
            replaces: &["openssh-client"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "openssh-server",
            description: "OpenSSH secure shell server and MattOS service configuration",
            source_component: "openssh",
            depends: &[
                "openssh-client",
                "libpam0g",
                "libpam-runtime",
                "libcrypt1",
                "zlib1g",
                "libzstd1",
                "mattos-libcrypto3",
                "libssl3t64",
            ],
            provides: &["ssh-server"],
            conflicts: &["openssh-server"],
            replaces: &["openssh-server"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libffi8",
            description: "Foreign-function interface runtime library built for MattOS",
            source_component: "libffi",
            depends: &["libc6"],
            provides: &["libffi8"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libffi-dev",
            description: "Foreign-function interface development files built for MattOS",
            source_component: "libffi",
            depends: &["libffi8", "libc6-dev"],
            provides: &["libffi-dev"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libwayland-client0",
            description: "Wayland client runtime library built for MattOS",
            source_component: "wayland",
            depends: &["libc6", "libffi8"],
            provides: &["libwayland-client0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libwayland-egl1",
            description: "Wayland EGL window runtime library built for MattOS",
            source_component: "wayland",
            depends: &["libc6", "libwayland-client0"],
            provides: &["libwayland-egl1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libwayland-server0",
            description: "Wayland compositor protocol runtime library built for MattOS",
            source_component: "wayland",
            depends: &["libc6", "libffi8"],
            provides: &["libwayland-server0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libxkbcommon0",
            description: "XKB keyboard description runtime library built for MattOS",
            source_component: "xkbcommon",
            depends: &["libc6", "xkb-data"],
            provides: &["libxkbcommon0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "xkb-data",
            description: "X Keyboard Extension data files built from pinned xkeyboard-config source",
            source_component: "xkeyboard-config",
            depends: &[],
            provides: &["xkb-data"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libseat1",
            description: "Seat management runtime library built for MattOS",
            source_component: "seatd",
            depends: &["libc6", "libsystemd0"],
            provides: &["libseat1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libdisplay-info3",
            description: "Display information parsing runtime library built for MattOS",
            source_component: "libdisplay-info",
            depends: &["libc6"],
            provides: &["libdisplay-info3"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libevdev2",
            description: "Linux input event runtime library built for MattOS",
            source_component: "libevdev",
            depends: &["libc6"],
            provides: &["libevdev2"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libinput10",
            description: "Input device handling runtime library built for MattOS",
            source_component: "libinput",
            depends: &["libc6", "libevdev2", "libudev1"],
            provides: &["libinput10"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libpixman-1-0",
            description: "Pixel manipulation runtime library built for MattOS",
            source_component: "pixman",
            depends: &["libc6"],
            provides: &["libpixman-1-0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libdrm2",
            description: "Direct Rendering Manager userspace runtime library built for MattOS",
            source_component: "libdrm",
            depends: &["libc6"],
            provides: &["libdrm2"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libdrm-amdgpu1",
            description: "AMDGPU DRM userspace runtime library built for MattOS",
            source_component: "libdrm",
            depends: &["libc6", "libdrm2"],
            provides: &["libdrm-amdgpu1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libdrm-nouveau2",
            description: "Nouveau DRM userspace runtime library built for MattOS",
            source_component: "libdrm",
            depends: &["libc6", "libdrm2"],
            provides: &["libdrm-nouveau2"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libxau6",
            description: "Minimal X authority ABI required by the NVIDIA Vulkan vendor library",
            source_component: "x11-compat",
            depends: &["libc6"],
            provides: &["libxau6"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libxdmcp6",
            description: "Minimal XDMCP ABI required by the NVIDIA Vulkan vendor library",
            source_component: "x11-compat",
            depends: &["libc6"],
            provides: &["libxdmcp6"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libxcb1",
            description: "X protocol transport ABI required by the NVIDIA Vulkan vendor library",
            source_component: "x11-compat",
            depends: &["libc6", "libxau6", "libxdmcp6"],
            provides: &["libxcb1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libx11-6",
            description: "X11 client ABI retained privately for NVIDIA Vulkan loader compatibility",
            source_component: "x11-compat",
            depends: &["libc6", "libxau6", "libxdmcp6", "libxcb1"],
            provides: &["libx11-6"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libxext6",
            description: "X11 extension ABI retained privately for NVIDIA Vulkan loader compatibility",
            source_component: "x11-compat",
            depends: &["libc6", "libxau6", "libxdmcp6", "libxcb1", "libx11-6"],
            provides: &["libxext6"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libglvnd0",
            description: "GLVND neutral OpenGL dispatch runtime built for MattOS",
            source_component: "libglvnd",
            depends: &["libc6"],
            provides: &["libglvnd0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libopengl0",
            description: "GLVND neutral OpenGL API runtime built for MattOS",
            source_component: "libglvnd",
            depends: &["libc6", "libglvnd0"],
            provides: &["libopengl0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libgbm1",
            description: "Mesa GBM runtime library built for MattOS",
            source_component: "mesa",
            depends: &["libc6", "libdrm2"],
            provides: &["libgbm1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libegl1",
            description: "GLVND neutral EGL dispatch runtime built for MattOS",
            source_component: "libglvnd",
            depends: &["libc6", "libglvnd0"],
            provides: &["libegl1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libgles1",
            description: "GLVND neutral OpenGL ES 1 dispatch runtime built for MattOS",
            source_component: "libglvnd",
            depends: &["libc6", "libglvnd0"],
            provides: &["libgles1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libgles2",
            description: "GLVND neutral OpenGL ES 2 dispatch runtime built for MattOS",
            source_component: "libglvnd",
            depends: &["libc6", "libglvnd0"],
            provides: &["libgles2"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libegl-mesa0",
            description: "Mesa EGL vendor implementation and GLVND registration",
            source_component: "mesa",
            depends: &[
                "libc6",
                "libegl1",
                "libgbm1",
                "libdrm2",
                "libdrm-amdgpu1",
                "libdrm-nouveau2",
                "libelf1t64",
                "libexpat1",
                "libffi8",
                "libgcc-s1",
                "libgl1-mesa-dri",
                "libllvm22",
                "libstdc++6",
                "libwayland-client0",
                "libwayland-egl1",
                "libzstd1",
                "zlib1g",
            ],
            provides: &["libegl-mesa0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libgl1-mesa-dri",
            description: "Mesa DRI drivers for modern hardware, virtual GPUs, and software fallback",
            source_component: "mesa",
            depends: &[
                "libc6",
                "libgcc-s1",
                "libstdc++6",
                "libllvm22",
                "libdrm2",
                "libdrm-amdgpu1",
                "libdrm-nouveau2",
                "libgbm1",
                "libexpat1",
                "libelf1t64",
                "zlib1g",
                "libzstd1",
            ],
            provides: &[
                "libgl1-mesa-dri",
                "mattos-mesa-llvmpipe",
                "mattos-mesa-virgl",
            ],
            conflicts: &[],
            replaces: &["mattos-mesa-llvmpipe"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libvulkan1",
            description: "Khronos Vulkan loader runtime built for MattOS",
            source_component: "vulkan-loader",
            depends: &["libc6"],
            provides: &["libvulkan1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "libvulkan-dev",
            description: "Khronos Vulkan headers and loader development metadata for MattOS",
            source_component: "vulkan-loader",
            depends: &["libc6-dev", "libvulkan1"],
            provides: &["libvulkan-dev"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "mesa-vulkan-drivers",
            description: "Mesa Vulkan ICDs for AMD, Intel, Nouveau, VirtIO, and software rendering",
            source_component: "mesa",
            depends: &[
                "libc6",
                "libvulkan1",
                "libgcc-s1",
                "libstdc++6",
                "libllvm22",
                "libdrm2",
                "libdrm-amdgpu1",
                "libdrm-nouveau2",
                "libgbm1",
                "libexpat1",
                "libelf1t64",
                "libwayland-client0",
                "libdisplay-info3",
                "libudev1",
                "zlib1g",
                "libzstd1",
            ],
            provides: &["mesa-vulkan-drivers"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "vulkan-tools",
            description: "Wayland and direct-display Vulkan diagnostics built for MattOS",
            source_component: "vulkan-tools",
            depends: &[
                "libc6",
                "libffi8",
                "libgcc-s1",
                "libstdc++6",
                "libvulkan1",
                "libwayland-client0",
            ],
            provides: &["vulkan-tools"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "linux-modules-nvidia-595-open-7.2.0-rc5-mattos",
            description: "NVIDIA 595.84 open GPU kernel modules for the exact MattOS kernel",
            source_component: "nvidia-driver",
            depends: &[
                "linux-modules-7.2.0-rc5-mattos",
                "nvidia-firmware-595",
                "kmod",
            ],
            provides: &["nvidia-open-kernel-modules", "nvidia-kernel-support-any"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "nvidia-firmware-595",
            description: "Matching NVIDIA 595.84 GSP firmware for Turing and newer GPUs",
            source_component: "nvidia-driver",
            depends: &["mattos-filesystem"],
            provides: &["firmware-nvidia-gsp", "nvidia-kernel-common-595"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libnvidia-gl-595",
            description: "NVIDIA 595.84 EGL, OpenGL ES, Vulkan, GBM, and Wayland vendor stack",
            source_component: "nvidia-driver",
            depends: &[
                "libc6",
                "libexpat1",
                "libffi8",
                "libgcc-s1",
                "libglvnd0",
                "libegl1",
                "libgles1",
                "libgles2",
                "libopengl0",
                "libdrm2",
                "libgbm1",
                "libwayland-client0",
                "libwayland-server0",
                "libxau6",
                "libxdmcp6",
                "libxcb1",
                "libx11-6",
                "libxext6",
                "libnvidia-compute-595",
            ],
            provides: &["nvidia-vulkan-icd", "nvidia-egl-icd", "nvidia-driver-libs"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libnvidia-compute-595",
            description: "NVIDIA 595.84 CUDA driver, NVML, and shader compiler runtime",
            source_component: "nvidia-driver",
            depends: &["libc6"],
            provides: &["libcuda1", "libnvidia-ml1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libnvidia-encode-595",
            description: "NVIDIA 595.84 NVENC video encoding runtime",
            source_component: "nvidia-driver",
            depends: &["libc6", "libnvidia-compute-595", "libnvidia-decode-595"],
            provides: &["libnvidia-encode1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libnvidia-decode-595",
            description: "NVIDIA 595.84 NVDEC video decoding runtime",
            source_component: "nvidia-driver",
            depends: &["libc6", "libnvidia-compute-595"],
            provides: &["libnvcuvid1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "nvidia-utils-595",
            description: "NVIDIA 595.84 nvidia-smi, module helper, and persistence utilities",
            source_component: "nvidia-driver",
            depends: &[
                "libc6",
                "libnvidia-compute-595",
                "linux-modules-nvidia-595-open-7.2.0-rc5-mattos",
            ],
            provides: &["nvidia-smi"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "nvidia-driver-595-open",
            description: "Complete official NVIDIA 595.84 open-kernel Wayland graphics stack",
            source_component: "nvidia-driver",
            depends: &[
                "linux-modules-nvidia-595-open-7.2.0-rc5-mattos",
                "libnvidia-gl-595",
                "libnvidia-compute-595",
                "libnvidia-encode-595",
                "libnvidia-decode-595",
                "nvidia-utils-595",
            ],
            provides: &["nvidia-driver", "nvidia-driver-any"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "cosmic-comp",
            description: "COSMIC Wayland compositor for MattOS graphical sessions",
            source_component: "cosmic-comp",
            depends: &[
                "libc6",
                "libgcc-s1",
                "libstdc++6",
                "libseat1",
                "libdisplay-info3",
                "libinput10",
                "libpixman-1-0",
                "libgbm1",
                "libegl1",
                "libgles1",
                "libgles2",
                "libgl1-mesa-dri",
                "libwayland-client0",
                "libxkbcommon0",
                "libudev1",
            ],
            provides: &["cosmic-comp"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "cosmic-desktop",
            description: "Complete source-built COSMIC desktop and graphical login for MattOS",
            source_component: "cosmic-desktop",
            depends: &[
                "cosmic-comp",
                "dbus-broker",
                "libdbus-1-3",
                "libdav1d7",
                "libglib2.0-0t64",
                "pipewire",
                "libpam0g",
                "libpam-modules",
                "libpam-runtime",
                "libsystemd0",
                "udev",
                "libwayland-client0",
                "libxkbcommon0",
                "xkb-data",
                "libgbm1",
                "libegl1",
                "libgl1-mesa-dri",
                "mesa-vulkan-drivers",
            ],
            provides: &[
                "cosmic-session",
                "cosmic-greeter",
                "cosmic-panel",
                "cosmic-launcher",
                "cosmic-settings",
                "cosmic-settings-daemon",
                "cosmic-notifications",
                "cosmic-osd",
                "cosmic-bg",
                "cosmic-workspaces",
                "cosmic-files",
                "cosmic-term",
                "greetd",
            ],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "cosmic-edit",
            description: "COSMIC Text Editor built from the pinned upstream source",
            source_component: "cosmic-edit",
            depends: &[
                "libc6", "libgcc-s1", "libstdc++6", "cosmic-desktop",
            ],
            provides: &[],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "cosmic-initial-setup",
            description: "COSMIC first-login setup wizard built from the pinned upstream source",
            source_component: "cosmic-initial-setup",
            depends: &["libc6", "libgcc-s1", "libstdc++6", "cosmic-desktop", "network-manager", "iso-codes"],
            provides: &["cosmic-initial-setup"], conflicts: &[], replaces: &[], essential: false, priority: "optional",
        },
        PackageSpec {
            name: "libduktape207",
            description: "Duktape JavaScript engine runtime built from the pinned upstream source",
            source_component: "duktape",
            depends: &["libc6"],
            provides: &["libduktape.so.207"], conflicts: &[], replaces: &[], essential: false, priority: "optional",
        },
        PackageSpec {
            name: "polkit",
            description: "Source-built PolicyKit authorization service and agent helper",
            source_component: "polkit",
            depends: &["libc6", "libglib2.0-0t64", "libpam0g", "libdbus-1-3", "libsystemd0", "libduktape207"],
            provides: &["polkit-1"], conflicts: &[], replaces: &[], essential: false, priority: "important",
        },
        PackageSpec {
            name: "network-manager",
            description: "Source-built NetworkManager daemon, D-Bus API, and nmcli",
            source_component: "networkmanager",
            depends: &["libc6", "libglib2.0-0t64", "libsystemd0", "libdbus-1-3", "polkit", "iproute2", "libndp0", "libreadline8", "libncursesw6", "mattos-libtinfow6"],
            provides: &["network-manager", "networkmanager"], conflicts: &[], replaces: &[], essential: false, priority: "important",
        },
        PackageSpec {
            name: "mattos-cozy",
            description: "Cozy terminal text editor built from the pinned upstream source",
            source_component: "cozy",
            depends: &["libc6", "libgcc-s1"],
            provides: &[],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libpython3.14",
            description: "CPython 3.14 shared runtime library built for MattOS",
            source_component: "cpython",
            depends: &["libc6"],
            provides: &["libpython3.14"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "python3",
            description: "CPython interpreter and standard library built for MattOS",
            source_component: "cpython",
            depends: &[
                "libpython3.14",
                "libffi8",
                "mattos-libcrypto3",
                "libssl3t64",
                "zlib1g",
                "libbz2-1.0",
                "liblzma5",
                "libexpat1",
                "libzstd1",
                "libncursesw6",
                "mattos-libtinfow6",
                "libuuid1",
            ],
            provides: &["python3"],
            conflicts: &["python3"],
            replaces: &["python3"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "python3-venv",
            description: "CPython virtual-environment and ensurepip support built for MattOS",
            source_component: "cpython",
            depends: &["python3"],
            provides: &["python3-venv"],
            conflicts: &["python3-venv"],
            replaces: &["python3-venv"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "python3-dev",
            description: "CPython headers and native extension development files built for MattOS",
            source_component: "cpython",
            depends: &["python3", "libpython3.14", "libffi-dev", "libc6-dev"],
            provides: &["python3-dev"],
            conflicts: &["python3-dev"],
            replaces: &["python3-dev"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "libllvm22",
            description: "LLVM 22 shared runtime library built for MattOS",
            source_component: "llvm",
            depends: &["libc6", "libgcc-s1", "libstdc++6", "zlib1g", "libzstd1"],
            provides: &["libllvm22"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "llvm",
            description: "LLVM development command-line tools built for MattOS",
            source_component: "llvm",
            depends: &[
                "libllvm22",
                "libc6",
                "libgcc-s1",
                "libstdc++6",
                "zlib1g",
                "libzstd1",
            ],
            provides: &["llvm"],
            conflicts: &["llvm"],
            replaces: &["llvm"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "llvm-dev",
            description: "LLVM headers and CMake development metadata built for MattOS",
            source_component: "llvm",
            depends: &["llvm", "libllvm22", "libc6-dev", "mattos-libstdc++-dev"],
            provides: &["llvm-dev"],
            conflicts: &["llvm-dev"],
            replaces: &["llvm-dev"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "clang",
            description: "Clang C and C++ compiler built for MattOS",
            source_component: "llvm",
            depends: &[
                "libllvm22",
                "libc6",
                "libgcc-s1",
                "libstdc++6",
                "zlib1g",
                "libzstd1",
                "libc6-dev",
                "mattos-libgcc-dev",
                "mattos-libstdc++-dev",
                "binutils",
            ],
            provides: &["clang"],
            conflicts: &["clang"],
            replaces: &["clang"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "lld",
            description: "LLVM linker built for MattOS",
            source_component: "llvm",
            depends: &[
                "libllvm22",
                "libc6",
                "libgcc-s1",
                "libstdc++6",
                "zlib1g",
                "libzstd1",
            ],
            provides: &["lld"],
            conflicts: &["lld"],
            replaces: &["lld"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "rustc",
            description: "Rust compiler, standard library, and rustdoc built for MattOS",
            source_component: "rust",
            depends: &[
                "libc6",
                "libgcc-s1",
                "libstdc++6",
                "libllvm22",
                "zlib1g",
                "libzstd1",
                "gcc",
                "binutils",
                "libc6-dev",
            ],
            provides: &["rustc", "rustdoc"],
            conflicts: &["rustc"],
            replaces: &["rustc"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "cargo",
            description: "Cargo Rust package manager built for MattOS",
            source_component: "rust",
            depends: &[
                "rustc",
                "libgcc-s1",
                "git",
                "ca-certificates",
                "mattos-libcrypto3",
                "libssl3t64",
                "zlib1g",
                "libzstd1",
            ],
            provides: &["cargo"],
            conflicts: &["cargo"],
            replaces: &["cargo"],
            essential: false,
            priority: "optional",
        },
    ]
}

fn package_install_order() -> Result<Vec<&'static str>> {
    let specs = package_specs();
    package_install_order_for(&specs, PACKAGE_NAMES)
}

fn package_install_order_for(
    specs: &[PackageSpec],
    preference: &[&'static str],
) -> Result<Vec<&'static str>> {
    let names: BTreeSet<&str> = specs.iter().map(|spec| spec.name).collect();
    let mut remaining: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for spec in specs {
        let mut dependencies = BTreeSet::new();
        for dependency in spec.depends.iter().copied() {
            if names.contains(dependency) {
                dependencies.insert(dependency);
            } else {
                bail!(
                    "package {} depends on unknown package {dependency}",
                    spec.name
                )
            }
        }
        remaining.insert(spec.name, dependencies);
    }
    let mut installed = BTreeSet::new();
    let mut order = Vec::new();
    while !remaining.is_empty() {
        let next = preference.iter().copied().find(|name| {
            remaining
                .get(name)
                .is_some_and(|dependencies| dependencies.is_subset(&installed))
        });
        let Some(next) = next else {
            let cycle = remaining.keys().copied().collect::<Vec<_>>().join(", ");
            bail!("circular or unresolvable MattOS package dependencies: {cycle}")
        };
        remaining.remove(next);
        installed.insert(next);
        order.push(next);
    }
    Ok(order)
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
            bail!("compatibility entry {} has an invalid zero Debian epoch", package.mattos_name)
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
        "Pin: release o=MattOS,l=MattOS Local,n=trixie\nPin-Priority: 1001",
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
        || !hosted.contains("Enabled: no")
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
        || !installed_debian.contains("Enabled: yes")
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
    let command = std::iter::once("python3".to_string())
        .chain(std::iter::once(shell_escape(path_str(&publisher)?)))
        .chain(std::iter::once("upload".to_string()))
        .chain(
            approved
                .iter()
                .map(|path| shell_escape(path_str(path).unwrap())),
        )
        .collect::<Vec<_>>()
        .join(" ");
    println!("validated non-publishing command (not executed):\n{command}");
    Ok(())
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
            .find(|spec| spec.name == name)
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
    print_inventory(repo_root)
}

fn ensure_package_facts(repo_root: &Path, inventory: &PackageInventory) -> Result<()> {
    for entry in &inventory.package {
        let path = repo_root
            .join("out/state/package-facts")
            .join(format!("{}.json", entry.sha256));
        let reusable = fs::read(&path)
            .ok()
            .and_then(|body| serde_json::from_slice::<PackageFacts>(&body).ok())
            .is_some_and(|facts| {
                facts.schema_version == 1
                    && facts.artifact_sha256 == entry.sha256
                    && facts.package == entry.name
            });
        if reusable {
            continue;
        }
        let staging = repo_root.join("out/packages/staging").join(&entry.name);
        let control_body = fs::read_to_string(staging.join("DEBIAN/control"))?;
        let control = parse_control_paragraphs(&control_body)?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("empty control metadata for {}", entry.name))?;
        let conffiles_path = staging.join("DEBIAN/conffiles");
        let conffiles = if conffiles_path.is_file() {
            fs::read_to_string(conffiles_path)?
                .lines()
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };
        let mut payload = Vec::new();
        let mut elf_members = Vec::new();
        walk_tree(&staging, &mut |member, metadata| {
            if member.starts_with(staging.join("DEBIAN")) {
                return Ok(());
            }
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o7777
            };
            #[cfg(not(unix))]
            let mode = 0;
            let relative = format!("/{}", member.strip_prefix(&staging)?.display());
            let kind = if metadata.file_type().is_symlink() {
                "symlink"
            } else if metadata.is_dir() {
                "directory"
            } else {
                "file"
            };
            payload.push(PackagePayloadFact {
                path: relative.clone(),
                kind: kind.into(),
                mode,
                symlink_target: if metadata.file_type().is_symlink() {
                    Some(fs::read_link(member)?.display().to_string())
                } else {
                    None
                },
            });
            if metadata.is_file() {
                if let Some(facts) = elf_cache::inspect(repo_root, member)? {
                    elf_members.push(PackageElfMember {
                        path: relative,
                        content_sha256: facts.content_sha256,
                        soname: facts.soname,
                        needed: facts.needed,
                    });
                }
            }
            Ok(())
        })?;
        payload.sort_by(|a, b| a.path.cmp(&b.path));
        elf_members.sort_by(|a, b| a.path.cmp(&b.path));
        let facts = PackageFacts {
            schema_version: 1,
            artifact_sha256: entry.sha256.clone(),
            package: entry.name.clone(),
            version: entry.version.clone(),
            architecture: entry.architecture.clone(),
            control,
            conffiles,
            payload,
            elf_members,
            dependencies: entry.dependencies.clone(),
            installed_size_kib: installed_size_kib(&staging)?,
            provenance: entry.source_component.clone(),
        };
        performance::atomic_write_json(&path, &facts)?;
    }
    Ok(())
}

pub(crate) fn package_facts_status(repo_root: &Path) -> Result<String> {
    let root = repo_root.join("out/state/package-facts");
    let count = if root.is_dir() {
        fs::read_dir(root)?.count()
    } else {
        0
    };
    Ok(format!(
        "package-audit: {count} content-addressed package fact record(s)"
    ))
}

pub(crate) fn invalidate_package_facts(repo_root: &Path) -> Result<usize> {
    let mut count = 0;
    for root in [
        repo_root.join("out/state/package-facts"),
        repo_root.join("out/state/audits"),
    ] {
        if root.is_dir() {
            count += fs::read_dir(&root)?.count();
            fs::remove_dir_all(root)?;
        }
    }
    Ok(count)
}

fn package_cache_manifest_path(repo_root: &Path, package: &str) -> PathBuf {
    repo_root
        .join("out/state/packages")
        .join(format!("{package}.json"))
}

fn package_definition_digest(spec: &PackageSpec) -> Result<String> {
    let revision = package_recipe_revision(spec.name);
    if revision == 1 {
        // Preserve the established revision-1 key exactly. Adding a recipe
        // discriminator for one package must not create a one-time rebuild of
        // every unrelated package.
        performance::digest_value(&(
            PACKAGE_CACHE_SCHEMA_VERSION,
            spec,
            ARCH,
            REVISION,
            SOURCE_DATE_EPOCH,
            "dpkg-deb --root-owner-group -Zzstd -z19",
        ))
    } else {
        performance::digest_value(&(
            PACKAGE_CACHE_SCHEMA_VERSION,
            revision,
            spec,
            ARCH,
            REVISION,
            SOURCE_DATE_EPOCH,
            "dpkg-deb --root-owner-group -Zzstd -z19",
        ))
    }
}

fn package_recipe_revision(package: &str) -> u32 {
    match package {
        // Revision 2 adds cfdisk to the deliberately selected base payload.
        // Keep this per-package so an unrelated staging-recipe edit does not
        // invalidate every package.
        "util-linux" => 2,
        // Revision 2 preserves Git's upstream hardlink topology through
        // package staging. Without this targeted invalidation, a cached
        // revision-1 package expands the built-in aliases into hundreds of
        // independent executable copies and can exhaust the live rootfs.
        "git" => 2,
        // Revision 2 owns the split sshd-session/sshd-auth executables that
        // OpenSSH 10.4 requires after the monitor process starts.
        "openssh-server" => 2,
        // Revision 2 owns libpanelw, which CPython's source-built
        // _curses_panel extension requires at runtime.
        "libncursesw6" => 2,
        // Revision 2 exposes the pinned bundle at OpenSSL's compiled default
        // CA file as well as Debian's canonical ca-certificates path.
        "ca-certificates" => 2,
        // Revision 2 ships the source-built quirk database required by
        // libinput at runtime.  A cache hit from the library-only recipe
        // would otherwise leave the live compositor without /usr/share/libinput.
        "libinput10" => 2,
        // Revision 2 stages Meson-generated xkeyboard-config rules from an
        // output-owned mirror.  Revision 1 copied only upstream fragments,
        // leaving the required rules/evdev runtime database absent.
        "xkb-data" => 2,
        // Revision 2 retains the complete upstream LICENSES directory and
        // top-level license notice alongside WHENCE in the binary package.
        "linux-firmware" => 2,
        // Revision 2 includes Linux-PAM's source-built vendor pam_env.conf;
        // the revision-1 cache key tracked only MattOS /etc/pam.d policy.
        "libpam-runtime" => 2,
        // Revision 4 requires COSMIC Tweaks in the aggregate desktop payload.
        // Revision 3 keeps the greeter daemon display-manager-scoped instead
        // of enabling it in every multi-user/CLI boot. Revision 2 supplied the
        // freedesktop hicolor fallback index.
        "cosmic-desktop" => 4,
        "mattos-compat" => 3,
        "cosmic-edit" | "mattos-cozy" => 1,
        "libgpg-error0" | "libgcrypt20" | "libassuan9" | "libksba8" | "libnpth0" | "gpgv" => 2,
        _ => 1,
    }
}

fn package_cache_input(
    repo_root: &Path,
    spec: &PackageSpec,
    version: &str,
    source_digests: &mut BTreeMap<String, String>,
) -> Result<PackageCacheInput> {
    let definition_digest = package_definition_digest(spec)?;
    let (payload_source_digest, payload_configuration_digest) =
        package_payload_source_digests(repo_root, spec, source_digests)?;
    let dependency_digest = package_stage_dependency_digest(repo_root, spec.source_component)?;
    let resolved_dependencies = package_dependencies(repo_root, spec)?;
    let cache_key = performance::digest_value(&(
        PACKAGE_CACHE_SCHEMA_VERSION,
        spec.name,
        version,
        ARCH,
        &definition_digest,
        &payload_source_digest,
        &dependency_digest,
        &resolved_dependencies,
        SOURCE_DATE_EPOCH,
        "deb-format=2.0;compression=zstd;level=19;root-owner-group=true",
    ))?;
    Ok(PackageCacheInput {
        cache_key,
        definition_digest,
        payload_source_digest,
        payload_configuration_digest,
        dependency_digest,
    })
}

fn package_stage_dependency_digest(repo_root: &Path, source_component: &str) -> Result<String> {
    let stage_dependencies = package_stage_dependencies(source_component);
    let mut dependency_values = BTreeMap::new();
    for dependency in stage_dependencies {
        let value = match performance::read_stage_manifest(repo_root, dependency) {
            Ok(manifest) => manifest.output_content_digest,
            Err(_) => "<missing>".to_string(),
        };
        dependency_values.insert(dependency.to_string(), value);
    }
    performance::digest_value(&dependency_values)
}

fn package_payload_source_digests(
    repo_root: &Path,
    spec: &PackageSpec,
    source_digests: &mut BTreeMap<String, String>,
) -> Result<(String, String)> {
    let source_key = spec.source_component.to_string();
    let upstream_source_digest = if let Some(digest) = source_digests.get(&source_key) {
        digest.clone()
    } else {
        let roots = package_source_roots(spec.source_component)
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let digest = performance::tracked_source_digest(repo_root, &roots, false)?;
        source_digests.insert(source_key, digest.clone());
        digest
    };
    let configuration_roots = package_configuration_roots(spec.name)
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let payload_configuration_digest = if configuration_roots.is_empty() {
        String::new()
    } else {
        performance::tracked_source_digest(repo_root, &configuration_roots, false)?
    };
    let payload_source_digest = if payload_configuration_digest.is_empty() {
        upstream_source_digest
    } else {
        performance::digest_value(&(&upstream_source_digest, &payload_configuration_digest))?
    };
    Ok((payload_source_digest, payload_configuration_digest))
}

fn validate_package_cache(
    repo_root: &Path,
    spec: &PackageSpec,
    version: &str,
    staging: &Path,
    artifact: &Path,
    input: &PackageCacheInput,
) -> Result<PackageCacheManifest> {
    let path = package_cache_manifest_path(repo_root, spec.name);
    let manifest: PackageCacheManifest = serde_json::from_slice(
        &fs::read(&path)
            .with_context(|| format!("package cache manifest missing: {}", path.display()))?,
    )
    .with_context(|| format!("package cache manifest is invalid: {}", path.display()))?;
    if manifest.schema_version != PACKAGE_CACHE_SCHEMA_VERSION {
        bail!("package cache schema changed")
    }
    if manifest.package != spec.name || manifest.cache_key != input.cache_key {
        bail!("package cache input key changed")
    }
    if manifest.definition_digest != input.definition_digest
        || manifest.payload_source_digest != input.payload_source_digest
        || manifest.payload_configuration_digest != input.payload_configuration_digest
        || manifest.dependency_digest != input.dependency_digest
    {
        bail!("package cache component digest changed")
    }
    if !staging.is_dir() || !artifact.is_file() {
        bail!("cached package staging tree or artifact is missing")
    }
    let payload_digest = performance::measure_package_validation_step("payload_inventory", || {
        performance::output_path_digest(repo_root, staging)
    })?;
    if payload_digest != manifest.payload_inventory_digest {
        bail!("cached package payload inventory/content/modes changed")
    }
    let artifact_sha =
        performance::measure_package_validation_step("artifact_sha256", || sha256_file(artifact))?;
    if artifact_sha != manifest.artifact_sha256 {
        bail!("cached package artifact checksum changed")
    }
    performance::measure_package_validation_step("dpkg_deb", || {
        verify_deb(artifact, spec.name, version)
    })?;
    if manifest.inventory_entry.name != spec.name
        || manifest.inventory_entry.version != version
        || manifest.inventory_entry.architecture != ARCH
        || manifest.inventory_entry.sha256 != artifact_sha
        || repo_root.join(&manifest.artifact_path) != artifact
    {
        bail!("cached package inventory/control metadata changed")
    }
    Ok(manifest)
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
            "cosmic-desktop" => &["cosmic-desktop"],
            "cosmic-edit" => &["cosmic-edit"],
            "cosmic-initial-setup" => &["cosmic-initial-setup"],
            "polkit" => &["polkit"],
            "networkmanager" => &["networkmanager"],
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
        "MattOS" => &["src/rootfs/skeleton", "src/system/packages/config"],
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
        "duktape" => &["src/system/security/duktape", "src/tools/mattos-build/src/main.rs", "src/tools/mattos-build/src/packaging.rs"],
        "polkit" => &[
                "src/system/security/polkit",
                "src/tools/mattos-build/src/main.rs",
                "src/tools/mattos-build/src/packaging.rs",
            ],
            "networkmanager" => &["src/system/network/NetworkManager"],
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

pub(crate) fn print_package_cache_status(repo_root: &Path) -> Result<()> {
    let mut hits = 0usize;
    for name in PACKAGE_NAMES {
        let path = package_cache_manifest_path(repo_root, name);
        if path.is_file() {
            hits += 1;
            println!("package:{name}: manifest present");
        } else {
            println!("package:{name}: rebuild: manifest absent");
        }
    }
    println!("package cache manifests: {hits}/{}", PACKAGE_NAMES.len());
    Ok(())
}

pub(crate) fn explain_package_cache(repo_root: &Path, name: &str) -> Result<()> {
    validate_package_name(name)?;
    let spec = package_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| anyhow!("unknown MattOS package {name}"))?;
    let version = package_version(repo_root, &spec)?;
    let mut source_digests = BTreeMap::new();
    let input = package_cache_input(repo_root, &spec, &version, &mut source_digests)?;
    let staging = repo_root.join("out/packages/staging").join(name);
    let artifact = repo_root
        .join("out/packages/amd64")
        .join(format!("{name}_{version}_{ARCH}.deb"));
    match validate_package_cache(repo_root, &spec, &version, &staging, &artifact, &input) {
        Ok(_) => println!("package:{name}: reusable; key={}", input.cache_key),
        Err(error) => println!("package:{name}: rebuild: {error:#}"),
    }
    Ok(())
}

pub(crate) fn invalidate_package_cache(repo_root: &Path, name: &str) -> Result<()> {
    validate_package_name(name)?;
    if !PACKAGE_NAMES.contains(&name) {
        bail!("unknown MattOS package {name}")
    }
    let path = package_cache_manifest_path(repo_root, name);
    if path.exists() {
        fs::remove_file(&path)?;
        println!("invalidated package cache manifest: {name}");
    } else {
        println!("package cache manifest was already absent: {name}");
    }
    println!(
        "staging and .deb outputs were preserved; the next package build will validate/rebuild them"
    );
    Ok(())
}

fn stage_package(repo_root: &Path, spec: &PackageSpec) -> Result<()> {
    let staging = repo_root.join("out/packages/staging").join(spec.name);
    remove_path_if_exists(&staging)?;
    fs::create_dir_all(staging.join("DEBIAN"))?;
    match spec.name {
        "mattos-filesystem" => stage_filesystem(&staging)?,
        "mattos-compat" => stage_mattos_compat(repo_root, &staging)?,
        "libc6" => stage_glibc_runtime(repo_root, &staging)?,
        "libgcc-s1" => {
            stage_gcc_runtime_library(repo_root, &staging, "libgcc_s.so.1", "libgcc-s1")?
        }
        "libstdc++6" => {
            stage_gcc_runtime_library(repo_root, &staging, "libstdc++.so.6", "libstdc++6")?
        }
        "linux-libc-dev" => stage_linux_libc_dev(repo_root, &staging)?,
        "linux-modules-7.2.0-rc5-mattos" => stage_linux_modules(repo_root, &staging)?,
        "libc6-dev" => stage_glibc_development(repo_root, &staging)?,
        "mattos-libgcc-dev" => stage_gcc_development(repo_root, &staging, false)?,
        "mattos-libstdc++-dev" => stage_gcc_development(repo_root, &staging, true)?,
        "binutils" => stage_native_binutils(repo_root, &staging)?,
        "mattos-gcc-common" => stage_native_gcc_common(repo_root, &staging)?,
        "cpp" => stage_native_compiler_driver(repo_root, &staging, "cpp")?,
        "gcc" => stage_native_compiler_driver(repo_root, &staging, "gcc")?,
        "g++" => stage_native_compiler_driver(repo_root, &staging, "g++")?,
        "make" => stage_native_make(repo_root, &staging)?,
        "libc-bin" => stage_glibc_utilities(repo_root, &staging)?,
        "locales" => stage_glibc_locales(repo_root, &staging)?,
        "iso-codes" => stage_iso_codes(repo_root, &staging)?,
        "tzdata" => stage_tzdata(repo_root, &staging)?,
        "linux-firmware" => stage_linux_firmware(repo_root, &staging)?,
        "wireless-regdb" => stage_wireless_regdb(repo_root, &staging)?,
        "mattos-base-files" => stage_base_files(repo_root, &staging)?,
        "ca-certificates" => stage_ca_certificates(repo_root, &staging)?,
        "mattos-brush" => stage_brush(repo_root, &staging)?,
        "coreutils" => stage_coreutils(repo_root, &staging)?,
        "curl" => {
            let source = repo_root.join("out/build/curl/install/usr/bin/curl");
            stage_executable(&source, &staging.join("usr/bin/curl"), 0o755)?;
            let source_libdir = repo_root.join("out/build/curl/install/usr/lib/x86_64-linux-gnu");
            let destination_libdir = staging.join("usr/lib/x86_64-linux-gnu");
            fs::create_dir_all(&destination_libdir)?;
            stage_executable(
                &source_libdir.join("libcurl.so.4.8.0"),
                &destination_libdir.join("libcurl.so.4.8.0"),
                0o644,
            )?;
            std::os::unix::fs::symlink(
                "libcurl.so.4.8.0",
                destination_libdir.join("libcurl.so.4"),
            )?;
        }
        "dpkg" => stage_dpkg(repo_root, &staging)?,
        "libgpg-error0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libgpg-error",
            "libgpg-error.so.0",
            "src/system/security/libgpg-error/COPYING.LIB",
            "libgpg-error0",
        )?,
        "libgcrypt20" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libgcrypt",
            "libgcrypt.so.20",
            "src/system/security/libgcrypt/COPYING.LIB",
            "libgcrypt20",
        )?,
        "libassuan9" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libassuan",
            "libassuan.so.9",
            "src/system/security/libassuan/COPYING.LIB",
            "libassuan9",
        )?,
        "libksba8" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libksba",
            "libksba.so.8",
            "src/system/security/libksba/COPYING.LGPLv3",
            "libksba8",
        )?,
        "libnpth0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "npth",
            "libnpth.so.0",
            "src/system/security/npth/COPYING.LIB",
            "libnpth0",
        )?,
        "gpgv" => {
            stage_runtime_paths(repo_root, &staging, "gpgv", &["usr/bin/gpgv"])?;
            copy_preserving(
                &repo_root.join("src/system/security/gnupg/COPYING"),
                &staging.join("usr/share/doc/gpgv/copyright"),
            )?;
        }
        "libapt-pkg7.0" => stage_libapt_pkg(repo_root, &staging)?,
        "apt" => stage_apt(repo_root, &staging)?,
        "mattos-libtinfow6" => stage_library_family(
            repo_root,
            &staging,
            "ncurses",
            &["libtinfow.so.6.6", "libtinfow.so.6"],
        )?,
        "libncursesw6" => stage_library_family(
            repo_root,
            &staging,
            "ncurses",
            &[
                "libncursesw.so.6.6",
                "libncursesw.so.6",
                "libpanelw.so.6.6",
                "libpanelw.so.6",
            ],
        )?,
        "libreadline8" => stage_library_family(
            repo_root,
            &staging,
            "readline",
            &["libreadline.so.8.2", "libreadline.so.8"],
        )?,
        "libndp0" => stage_library_family(
            repo_root,
            &staging,
            "libndp",
            &["libndp.so.0", "libndp.so.0.3.0"],
        )?,
        "ncurses-base" => stage_terminfo(repo_root, &staging)?,
        "ncurses-bin" => {
            stage_runtime_paths(repo_root, &staging, "ncurses", NCURSES_RUNTIME_PATHS)?
        }
        "libkmod2" => stage_library_family(
            repo_root,
            &staging,
            "kmod",
            &["libkmod.so.2.5.1", "libkmod.so.2"],
        )?,
        "kmod" => stage_runtime_paths(repo_root, &staging, "kmod", KMOD_RUNTIME_PATHS)?,
        "mattos-libproc2" => stage_library_family(
            repo_root,
            &staging,
            "procps-ng",
            &["libproc2.so.1.0.1", "libproc2.so.1"],
        )?,
        "procps" => stage_procps(repo_root, &staging)?,
        "libsystemd0" => stage_library_family(
            repo_root,
            &staging,
            "systemd",
            &["libsystemd.so.0.44.0", "libsystemd.so.0"],
        )?,
        "libudev1" => stage_library_family(
            repo_root,
            &staging,
            "systemd",
            &["libudev.so.1.7.14", "libudev.so.1"],
        )?,
        "udev" => stage_udev_hwdb(repo_root, &staging)?,
        "libexpat1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "expat",
            "libexpat.so.1",
            "src/system/libraries/expat/expat/COPYING",
            "libexpat1",
        )?,
        "libcap2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libcap",
            "libcap.so.2",
            "src/system/libraries/libcap/License",
            "libcap2",
        )?,
        "libattr1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "attr",
            "libattr.so.1",
            "src/system/libraries/attr/doc/COPYING.LGPL",
            "libattr1",
        )?,
        "libacl1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "acl",
            "libacl.so.1",
            "src/system/libraries/acl/doc/COPYING.LGPL",
            "libacl1",
        )?,
        "zlib1g" => stage_imported_soname_library(
            repo_root,
            &staging,
            "zlib",
            "libz.so.1",
            "src/system/libraries/zlib/LICENSE",
            "zlib1g",
        )?,
        "libbz2-1.0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "bzip2",
            "libbz2.so.1.0",
            "src/system/libraries/bzip2/LICENSE",
            "libbz2-1.0",
        )?,
        "liblz4-1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "lz4",
            "liblz4.so.1",
            "src/system/libraries/lz4/LICENSE",
            "liblz4-1",
        )?,
        "liblzma5" => stage_imported_soname_library(
            repo_root,
            &staging,
            "xz",
            "liblzma.so.5",
            "src/system/libraries/xz/COPYING",
            "liblzma5",
        )?,
        "libxxhash0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "xxhash",
            "libxxhash.so.0",
            "src/system/libraries/xxhash/LICENSE",
            "libxxhash0",
        )?,
        "libmd0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libmd",
            "libmd.so.0",
            "src/system/libraries/libmd/COPYING",
            "libmd0",
        )?,
        "libbsd0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libbsd",
            "libbsd.so.0",
            "src/system/libraries/libbsd/COPYING",
            "libbsd0",
        )?,
        "libzstd1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "zstd",
            "libzstd.so.1",
            "src/system/libraries/zstd/LICENSE",
            "libzstd1",
        )?,
        "mattos-libcrypto3" => stage_imported_soname_library(
            repo_root,
            &staging,
            "openssl",
            "libcrypto.so.3",
            "src/system/libraries/openssl/LICENSE.txt",
            "mattos-libcrypto3",
        )?,
        "libssl3t64" => stage_imported_soname_library(
            repo_root,
            &staging,
            "openssl",
            "libssl.so.3",
            "src/system/libraries/openssl/LICENSE.txt",
            "libssl3t64",
        )?,
        "libelf1t64" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "elfutils",
                "libelf.so.1",
                "src/system/libraries/elfutils/COPYING-LGPLV3",
                "libelf1t64",
            )?;
            copy_preserving(
                &repo_root.join("src/system/libraries/elfutils/COPYING-GPLV2"),
                &staging.join("usr/share/doc/libelf1t64/copyright.GPL-2"),
            )?;
        }
        "libpcre2-8-0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "pcre2",
            "libpcre2-8.so.0",
            "src/system/libraries/pcre2/LICENCE.md",
            "libpcre2-8-0",
        )?,
        "libselinux1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "selinux",
            "libselinux.so.1",
            "src/system/security/selinux/libselinux/LICENSE",
            "libselinux1",
        )?,
        "libcrypt1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libxcrypt",
            "libcrypt.so.1",
            "src/system/libraries/libxcrypt/COPYING.LIB",
            "libcrypt1",
        )?,
        "libblkid1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libblkid.so.1",
            "src/userland/util-linux/COPYING",
            "libblkid1",
        )?,
        "libmount1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libmount.so.1",
            "src/userland/util-linux/COPYING",
            "libmount1",
        )?,
        "libsmartcols1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libsmartcols.so.1",
            "src/userland/util-linux/COPYING",
            "libsmartcols1",
        )?,
        "mount" => {
            stage_runtime_paths(
                repo_root,
                &staging,
                "util-linux",
                &["usr/bin/mount", "usr/bin/umount"],
            )?;
            copy_preserving(
                &repo_root.join("src/userland/util-linux/COPYING"),
                &staging.join("usr/share/doc/mount/copyright"),
            )?;
            for rel in ["usr/bin/mount", "usr/bin/umount"] {
                set_mode(staging.join(rel), 0o4755)?;
            }
        }
        "libuuid1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libuuid.so.1",
            "src/userland/util-linux/COPYING",
            "libuuid1",
        )?,
        "libfdisk1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libfdisk.so.1",
            "src/userland/util-linux/COPYING",
            "libfdisk1",
        )?,
        "util-linux" => {
            stage_runtime_paths(repo_root, &staging, "util-linux", UTIL_LINUX_BASE_PATHS)?;
            copy_preserving(
                &repo_root.join("src/userland/util-linux/COPYING"),
                &staging.join("usr/share/doc/util-linux/copyright"),
            )?;
        }
        "gzip" => stage_runtime_paths(
            repo_root,
            &staging,
            "gzip",
            &["usr/bin/gzip", "usr/bin/gunzip", "usr/bin/zcat"],
        )?,
        "bzip2" => stage_runtime_paths(
            repo_root,
            &staging,
            "bzip2",
            &[
                "usr/bin/bzip2",
                "usr/bin/bunzip2",
                "usr/bin/bzcat",
                "usr/bin/bzip2recover",
            ],
        )?,
        "xz-utils" => stage_runtime_paths(
            repo_root,
            &staging,
            "xz",
            &[
                "usr/bin/xz",
                "usr/bin/unxz",
                "usr/bin/xzcat",
                "usr/bin/lzma",
                "usr/bin/unlzma",
                "usr/bin/lzcat",
            ],
        )?,
        "zstd" => stage_runtime_paths(
            repo_root,
            &staging,
            "zstd",
            &["usr/bin/zstd", "usr/bin/unzstd", "usr/bin/zstdcat"],
        )?,
        "patch" => stage_runtime_paths(repo_root, &staging, "patch", &["usr/bin/patch"])?,
        "libmagic1" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "file",
                "libmagic.so.1",
                "src/userland/file/COPYING",
                "libmagic1",
            )?;
            copy_preserving(
                &repo_root.join("out/build/file/install/usr/share/misc/magic.mgc"),
                &staging.join("usr/share/misc/magic.mgc"),
            )?;
        }
        "file" => stage_runtime_paths(repo_root, &staging, "file", &["usr/bin/file"])?,
        "less" => stage_runtime_paths(
            repo_root,
            &staging,
            "less",
            &["usr/bin/less", "usr/bin/lesskey", "usr/libexec/lessecho"],
        )?,
        "git" => copy_tree_preserving(
            &repo_root.join("out/build/git/install/usr"),
            &staging.join("usr"),
        )?,
        "libffi8" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libffi",
            "libffi.so.8",
            "src/system/libraries/libffi/libffi/LICENSE",
            "libffi8",
        )?,
        "libffi-dev" => stage_libffi_dev(repo_root, &staging)?,
        "libxkbcommon0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "xkbcommon",
            "libxkbcommon.so.0",
            "src/system/libraries/xkbcommon/LICENSE",
            "libxkbcommon0",
        )?,
        "libwayland-client0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "wayland",
            "libwayland-client.so.0",
            "src/system/libraries/wayland/COPYING",
            "libwayland-client0",
        )?,
        "libwayland-server0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "wayland",
            "libwayland-server.so.0",
            "src/system/libraries/wayland/COPYING",
            "libwayland-server0",
        )?,
        "libwayland-egl1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "wayland",
            "libwayland-egl.so.1",
            "src/system/libraries/wayland/COPYING",
            "libwayland-egl1",
        )?,
        "xkb-data" => stage_xkeyboard_config_data(repo_root, &staging)?,
        "libseat1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "seatd",
            "libseat.so.1",
            "src/system/libraries/seatd/LICENSE",
            "libseat1",
        )?,
        "libdisplay-info3" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libdisplay-info",
            "libdisplay-info.so.3",
            "src/system/libraries/libdisplay-info/LICENSE",
            "libdisplay-info3",
        )?,
        "libevdev2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libevdev",
            "libevdev.so.2",
            "src/system/libraries/libevdev/COPYING",
            "libevdev2",
        )?,
        "libinput10" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "libinput",
                "libinput.so.10",
                "src/system/libraries/libinput/COPYING",
                "libinput10",
            )?;
            copy_tree_preserving(
                &repo_root.join("out/build/libinput/install/usr/share/libinput"),
                &staging.join("usr/share/libinput"),
            )?;
        }
        "libpixman-1-0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "pixman",
            "libpixman-1.so.0",
            "src/system/libraries/pixman/COPYING",
            "libpixman-1-0",
        )?,
        "libdrm2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libdrm",
            "libdrm.so.2",
            "src/system/libraries/libdrm/README.rst",
            "libdrm2",
        )?,
        "libdrm-amdgpu1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libdrm",
            "libdrm_amdgpu.so.1",
            "src/system/libraries/libdrm/README.rst",
            "libdrm-amdgpu1",
        )?,
        "libdrm-nouveau2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libdrm",
            "libdrm_nouveau.so.2",
            "src/system/libraries/libdrm/README.rst",
            "libdrm-nouveau2",
        )?,
        "libxau6" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXau.so.6",
            "src/system/graphics/libxau/COPYING",
            "libxau6",
        )?,
        "libxdmcp6" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXdmcp.so.6",
            "src/system/graphics/libxdmcp/COPYING",
            "libxdmcp6",
        )?,
        "libxcb1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libxcb.so.1",
            "src/system/graphics/libxcb/COPYING",
            "libxcb1",
        )?,
        "libx11-6" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "x11-compat",
                "libX11.so.6",
                "src/system/graphics/libx11/COPYING",
                "libx11-6",
            )?;
            copy_tree_preserving(
                &component_install(repo_root, "x11-compat").join("usr/share/X11/locale"),
                &staging.join("usr/share/X11/locale"),
            )?;
        }
        "libxext6" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXext.so.6",
            "src/system/graphics/libxext/COPYING",
            "libxext6",
        )?,
        "libglvnd0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libGLdispatch.so.0",
            "src/system/graphics/libglvnd/README.md",
            "libglvnd0",
        )?,
        "libopengl0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libOpenGL.so.0",
            "src/system/graphics/libglvnd/README.md",
            "libopengl0",
        )?,
        "libgbm1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "mesa",
            "libgbm.so.1",
            "src/system/graphics/mesa/docs/license.rst",
            "libgbm1",
        )?,
        "libegl1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libEGL.so.1",
            "src/system/graphics/libglvnd/README.md",
            "libegl1",
        )?,
        "libgles1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libGLESv1_CM.so.1",
            "src/system/graphics/libglvnd/README.md",
            "libgles1",
        )?,
        "libgles2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libGLESv2.so.2",
            "src/system/graphics/libglvnd/README.md",
            "libgles2",
        )?,
        "libegl-mesa0" => stage_mesa_egl_vendor(repo_root, &staging)?,
        "libgl1-mesa-dri" => stage_mesa_dri_runtime(repo_root, &staging)?,
        "libvulkan1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "vulkan-loader",
            "libvulkan.so.1",
            "src/system/graphics/vulkan-loader/LICENSE.txt",
            "libvulkan1",
        )?,
        "libvulkan-dev" => stage_vulkan_development(repo_root, &staging)?,
        "mesa-vulkan-drivers" => stage_mesa_vulkan_runtime(repo_root, &staging)?,
        "vulkan-tools" => stage_vulkan_tools(repo_root, &staging)?,
        "linux-modules-nvidia-595-open-7.2.0-rc5-mattos"
        | "nvidia-firmware-595"
        | "libnvidia-gl-595"
        | "libnvidia-compute-595"
        | "libnvidia-encode-595"
        | "libnvidia-decode-595"
        | "nvidia-utils-595"
        | "nvidia-driver-595-open" => stage_nvidia_package(repo_root, &staging, spec.name)?,
        "cosmic-comp" => {
            stage_runtime_paths(repo_root, &staging, "cosmic-comp", &["usr/bin/cosmic-comp"])?;
            for (source, destination) in [
                (
                    "src/desktop/cosmic/cosmic-comp/data/keybindings.ron",
                    "usr/share/cosmic/com.system76.CosmicSettings.Shortcuts/v1/defaults",
                ),
                (
                    "src/desktop/cosmic/cosmic-comp/data/tiling-exceptions.ron",
                    "usr/share/cosmic/com.system76.CosmicSettings.WindowRules/v1/tiling_exception_defaults",
                ),
            ] {
                copy_preserving(&repo_root.join(source), &staging.join(destination))?;
            }
        }
        "cosmic-desktop" => stage_cosmic_desktop(repo_root, &staging)?,
        "cosmic-edit" => stage_cosmic_edit(repo_root, &staging)?,
        "cosmic-initial-setup" => stage_cosmic_initial_setup(repo_root, &staging)?,
        "libduktape207" => stage_runtime_paths(repo_root, &staging, "duktape", &[
            "usr/lib/x86_64-linux-gnu/libduktape.so.207.2.7.0",
            "usr/lib/x86_64-linux-gnu/libduktape.so.207",
            "usr/lib/x86_64-linux-gnu/libduktape.so",
        ])?,
        "polkit" => stage_runtime_paths(repo_root, &staging, "polkit", &[
            "usr/bin/pkcheck",
            "usr/lib/polkit-1/polkitd",
            "usr/lib/polkit-1/polkit-agent-helper-1",
            "usr/lib/x86_64-linux-gnu/libpolkit-agent-1.so",
            "usr/lib/x86_64-linux-gnu/libpolkit-agent-1.so.0",
            "usr/lib/x86_64-linux-gnu/libpolkit-agent-1.so.0.0.0",
            "usr/lib/x86_64-linux-gnu/libpolkit-gobject-1.so",
            "usr/lib/x86_64-linux-gnu/libpolkit-gobject-1.so.0",
            "usr/lib/x86_64-linux-gnu/libpolkit-gobject-1.so.0.0.0",
        ])?,
        "network-manager" => stage_network_manager(repo_root, &staging)?,
        "mattos-cozy" => stage_cozy(repo_root, &staging)?,
        "libdbus-1-3" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "dbus",
                "libdbus-1.so.3",
                "src/system/dbus/dbus/COPYING",
                "libdbus-1-3",
            )?;
            // COSMIC's upstream session launcher uses a private reference bus
            // when a login manager has not supplied a user bus (including the
            // live installer compositor). dbus-broker remains the sole
            // systemd-managed system/user daemon.
            stage_runtime_paths(
                repo_root,
                &staging,
                "dbus",
                &[
                    "usr/bin/dbus-daemon",
                    "usr/bin/dbus-run-session",
                    "usr/bin/dbus-update-activation-environment",
                ],
            )?;
            let reference_config =
                component_install(repo_root, "dbus").join("usr/share/dbus-1/session.conf");
            let private_config = fs::read_to_string(&reference_config)?
                .lines()
                .map(|line| {
                    if line.contains("<listen>") {
                        "  <listen>unix:tmpdir=/tmp</listen>"
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            if !private_config.contains("<listen>unix:tmpdir=/tmp</listen>") {
                bail!("private D-Bus session config is missing its runtime listen address");
            }
            let private_config_path = staging.join("usr/share/dbus-1/mattos-private-session.conf");
            fs::create_dir_all(
                private_config_path
                    .parent()
                    .expect("private D-Bus config parent"),
            )?;
            fs::write(private_config_path, private_config)?;
        }
        "libdav1d7" => stage_imported_soname_library(
            repo_root,
            &staging,
            "dav1d",
            "libdav1d.so.7",
            "src/system/multimedia/dav1d/COPYING",
            "libdav1d7",
        )?,
        "libglib2.0-0t64" => {
            stage_library_family(
                repo_root,
                &staging,
                "glib",
                &[
                    "libglib-2.0.so.0",
                    "libgobject-2.0.so.0",
                    "libgio-2.0.so.0",
                    "libgmodule-2.0.so.0",
                    "libgthread-2.0.so.0",
                ],
            )?;
            stage_runtime_paths(
                repo_root,
                &staging,
                "glib",
                &["usr/bin/glib-compile-schemas", "usr/bin/gio-querymodules"],
            )?;
            copy_preserving(
                &repo_root.join("src/system/libraries/glib/COPYING"),
                &staging.join("usr/share/doc/libglib2.0-0t64/copyright"),
            )?;
        }
        "pipewire" => stage_pipewire(repo_root, &staging)?,
        "libpython3.14" => {
            stage_library_family(repo_root, &staging, "cpython", &["libpython3.14.so.1.0"])?;
            copy_preserving(
                &repo_root.join("src/development/python/cpython/LICENSE"),
                &staging.join("usr/share/doc/libpython3.14/copyright"),
            )?;
        }
        "python3" => stage_cpython_runtime(repo_root, &staging)?,
        "python3-venv" => stage_cpython_venv(repo_root, &staging)?,
        "python3-dev" => stage_cpython_dev(repo_root, &staging)?,
        "libllvm22" => stage_llvm_runtime(repo_root, &staging)?,
        "llvm" => stage_llvm_tools(repo_root, &staging)?,
        "llvm-dev" => stage_llvm_development(repo_root, &staging)?,
        "clang" => stage_clang(repo_root, &staging)?,
        "lld" => stage_lld(repo_root, &staging)?,
        "rustc" => stage_rustc(repo_root, &staging)?,
        "cargo" => stage_cargo(repo_root, &staging)?,
        "openssh-client" => {
            stage_runtime_paths(
                repo_root,
                &staging,
                "openssh",
                &[
                    "usr/bin/ssh",
                    "usr/bin/scp",
                    "usr/bin/sftp",
                    "usr/bin/ssh-add",
                    "usr/bin/ssh-agent",
                    "usr/bin/ssh-keygen",
                    "usr/bin/ssh-keyscan",
                ],
            )?;
            copy_preserving(
                &repo_root.join("src/system/network/openssh/ssh_config"),
                &staging.join("etc/ssh/ssh_config"),
            )?;
            fs::write(staging.join("DEBIAN/conffiles"), "/etc/ssh/ssh_config\n")?;
        }
        "openssh-server" => stage_openssh_server(repo_root, &staging)?,
        "tar" => {
            stage_executable(
                &repo_root.join("out/build/tar/install/usr/bin/tar"),
                &staging.join("usr/bin/tar"),
                0o755,
            )?;
            copy_preserving(
                &repo_root.join("src/userland/tar/COPYING"),
                &staging.join("usr/share/doc/tar/copyright"),
            )?;
        }
        "dbus-broker" => stage_dbus_broker(repo_root, &staging)?,
        "libpam0g" => stage_library_family(
            repo_root,
            &staging,
            "linux-pam",
            &["libpam.so.0.85.1", "libpam.so.0"],
        )?,
        "mattos-libpam-misc0" => stage_library_family(
            repo_root,
            &staging,
            "linux-pam",
            &["libpam_misc.so.0.82.1", "libpam_misc.so.0"],
        )?,
        "libpam-modules" => stage_pam_modules(repo_root, &staging)?,
        "libpam-runtime" => stage_pam_runtime(repo_root, &staging)?,
        "passwd" => stage_shadow(repo_root, &staging)?,
        "mattos-sudo-rs" => stage_sudo_rs(repo_root, &staging)?,
        "login" => stage_util_linux_auth(repo_root, &staging)?,
        "iproute2" => stage_iproute2(repo_root, &staging)?,
        "iputils-ping" => {
            stage_runtime_paths(repo_root, &staging, "iputils", IPUTILS_RUNTIME_PATHS)?
        }
        "mattos-installer" => stage_mattos_installer(repo_root, &staging)?,
        "btrfs-progs" => copy_tree_preserving(
            &repo_root.join("out/build/btrfs-progs/install/usr"),
            &staging.join("usr"),
        )?,
        "dosfstools" => copy_tree_preserving(
            &repo_root.join("out/build/dosfstools/install/usr"),
            &staging.join("usr"),
        )?,
        "e2fsprogs" => {
            let install = repo_root.join("out/build/e2fsprogs/install");
            for relative in ["usr/bin", "usr/sbin", "usr/libexec", "usr/share/man", "etc"] {
                copy_tree_preserving(&install.join(relative), &staging.join(relative))?;
            }
        }
        _ => bail!("no staging implementation for {}", spec.name),
    }

    // NVIDIA's redistribution grant requires its userspace binaries to remain
    // unmodified. Open modules are already compressed; preserve every file in
    // this separately versioned stack byte-for-byte after extraction.
    if !matches!(
        spec.name,
        "libc6"
            | "libgcc-s1"
            | "libstdc++6"
            | "linux-modules-nvidia-595-open-7.2.0-rc5-mattos"
            | "nvidia-firmware-595"
            | "libnvidia-gl-595"
            | "libnvidia-compute-595"
            | "libnvidia-encode-595"
            | "libnvidia-decode-595"
            | "nvidia-utils-595"
            | "nvidia-driver-595-open"
    ) {
        strip_staged_debug(repo_root, &staging)?;
    }

    let version = package_version(repo_root, spec)?;
    validate_debian_version(&version)?;
    let runtime_libraries = runtime_libraries_for_spec(repo_root, spec)?;
    write_provenance(repo_root, &staging, spec, &version, &runtime_libraries)?;
    if matches!(
        spec.name,
        "linux-libc-dev"
            | "libc6-dev"
            | "mattos-libgcc-dev"
            | "mattos-libstdc++-dev"
            | "binutils"
            | "mattos-gcc-common"
            | "cpp"
            | "gcc"
            | "g++"
            | "make"
    ) {
        validate_no_embedded_build_root(repo_root, &staging)?;
    }
    let installed_size = installed_size_kib(&staging)?;
    let dependencies = package_dependencies(repo_root, spec)?;
    let control = render_control(
        spec,
        &version,
        installed_size,
        &dependencies,
        &runtime_libraries,
    )?;
    fs::write(staging.join("DEBIAN/control"), control)?;
    normalize_package_modes(&staging)?;
    Ok(())
}

fn stage_mattos_compat(repo_root: &Path, staging: &Path) -> Result<()> {
    let status = Command::new("cargo")
        .current_dir(repo_root)
        .args(["build", "--release", "-p", "mattos-compat"])
        .status()
        .context("build mattos-compat")?;
    if !status.success() {
        bail!("cargo failed while building mattos-compat: {status}");
    }
    let binary = repo_root.join("target/release/mattos-compat");
    if !binary.is_file() {
        bail!("mattos-compat build did not produce {}", binary.display());
    }
    copy_preserving(&binary, &staging.join("usr/bin/mattos-compat"))?;
    let nspawn = repo_root.join("out/build/systemd/install/usr/bin/systemd-nspawn");
    if !nspawn.is_file() {
        bail!(
            "systemd-nspawn is missing from the systemd stage at {}; enable the nspawn component before building mattos-compat",
            nspawn.display()
        );
    }
    copy_preserving(&nspawn, &staging.join("usr/bin/systemd-nspawn"))?;
    Ok(())
}

fn stage_libffi_dev(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "libffi").join("usr");
    copy_tree_preserving(&install.join("include"), &staging.join("usr/include"))?;
    for relative in [
        "lib/x86_64-linux-gnu/libffi.so",
        "lib/x86_64-linux-gnu/pkgconfig/libffi.pc",
    ] {
        copy_path_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    copy_tree_preserving(
        &install.join("share/man/man3"),
        &staging.join("usr/share/man/man3"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/libraries/libffi/libffi/LICENSE"),
        &staging.join("usr/share/doc/libffi-dev/copyright"),
    )
}

fn stage_mattos_installer(repo_root: &Path, staging: &Path) -> Result<()> {
    let installer = repo_root.join("out/build/installer");
    stage_executable(
        &installer.join("cargo-target/release/mattos-install"),
        &staging.join("usr/bin/mattos-install"),
        0o755,
    )?;
    stage_executable(
        &installer.join("cosmic-target/release/mattos-install-cosmic"),
        &staging.join("usr/bin/mattos-install-cosmic"),
        0o755,
    )?;
    let assets = staging.join("usr/lib/mattos/installer");
    fs::create_dir_all(&assets)?;
    for (source, name) in [
        (
            repo_root.join("out/build/linux/build/arch/x86/boot/bzImage"),
            "vmlinuz",
        ),
        (
            repo_root.join("out/build/installed-initramfs.cpio.xz"),
            "installed-initramfs.cpio.xz",
        ),
        (installer.join("BOOTX64.EFI"), "BOOTX64.EFI"),
    ] {
        copy_preserving(&source, &assets.join(name))?;
    }
    copy_preserving(
        &repo_root.join("src/system/installer/policy/example-plan.toml"),
        &staging.join("usr/share/doc/mattos-installer/example-plan.toml"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/installer/PROVENANCE.md"),
        &staging.join("usr/share/doc/mattos-installer/PROVENANCE.md"),
    )?;
    for name in [
        "mattos-install-cli.service",
        "mattos-install-cli.target",
        "mattos-install-graphical.service",
        "mattos-install-graphical.target",
        "mattos-cosmic-installer-session.service",
    ] {
        copy_preserving(
            &repo_root.join("src/system/units").join(name),
            &staging.join("usr/lib/systemd/system").join(name),
        )?;
    }
    Ok(())
}

fn stage_cpython_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cpython").join("usr");
    for relative in [
        "bin/python3",
        "bin/python3.14",
        "bin/pydoc3",
        "bin/pydoc3.14",
    ] {
        copy_path_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    let stdlib = install.join("lib/python3.14");
    copy_tree_filtered(
        &stdlib,
        &staging.join("usr/lib/python3.14"),
        &|relative, _| {
            let first = relative.components().next().map(|part| part.as_os_str());
            !matches!(
                first.and_then(|part| part.to_str()),
                Some("ensurepip" | "venv" | "site-packages")
            ) && !relative
                .components()
                .next()
                .and_then(|part| part.as_os_str().to_str())
                .is_some_and(|name| name.starts_with("config-"))
        },
    )?;
    copy_preserving(
        &repo_root.join("src/development/python/cpython/LICENSE"),
        &staging.join("usr/share/doc/python3/copyright"),
    )
}

fn stage_cpython_venv(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cpython").join("usr");
    for relative in ["bin/pip3", "bin/pip3.14"] {
        copy_path_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    for relative in [
        "lib/python3.14/ensurepip",
        "lib/python3.14/venv",
        "lib/python3.14/site-packages",
    ] {
        copy_tree_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    copy_preserving(
        &repo_root.join("src/development/python/cpython/LICENSE"),
        &staging.join("usr/share/doc/python3-venv/copyright"),
    )
}

fn stage_cpython_dev(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cpython").join("usr");
    for relative in ["bin/python3-config", "bin/python3.14-config"] {
        copy_path_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    copy_tree_preserving(&install.join("include"), &staging.join("usr/include"))?;
    for relative in [
        "lib/x86_64-linux-gnu/libpython3.14.so",
        "lib/x86_64-linux-gnu/libpython3.so",
    ] {
        copy_path_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    copy_tree_preserving(
        &install.join("lib/x86_64-linux-gnu/pkgconfig"),
        &staging.join("usr/lib/x86_64-linux-gnu/pkgconfig"),
    )?;
    let stdlib = install.join("lib/python3.14");
    copy_tree_filtered(
        &stdlib,
        &staging.join("usr/lib/python3.14"),
        &|relative, _| {
            relative
                .components()
                .next()
                .and_then(|part| part.as_os_str().to_str())
                .is_some_and(|name| name.starts_with("config-"))
        },
    )?;
    copy_preserving(
        &repo_root.join("src/development/python/cpython/LICENSE"),
        &staging.join("usr/share/doc/python3-dev/copyright"),
    )
}

fn llvm_install(repo_root: &Path) -> PathBuf {
    component_install(repo_root, "llvm").join("usr")
}

fn stage_llvm_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = llvm_install(repo_root);
    for name in [
        "libLLVM.so.22.1",
        "libLLVM-22.so",
        "libclang-cpp.so.22.1",
        "libclang.so.22.1.8",
        "libclang.so.22.1",
        "libLTO.so.22.1",
        "libRemarks.so.22.1",
    ] {
        let relative = Path::new("lib/x86_64-linux-gnu").join(name);
        copy_path_preserving(
            &install.join(&relative),
            &staging.join("usr").join(relative),
        )?;
    }
    copy_preserving(
        &repo_root.join("src/toolchain/llvm-project/llvm/LICENSE.TXT"),
        &staging.join("usr/share/doc/libllvm22/copyright"),
    )
}

fn stage_llvm_tools(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = llvm_install(repo_root);
    copy_tree_filtered(
        &install.join("bin"),
        &staging.join("usr/bin"),
        &|relative, metadata| {
            if metadata.is_dir() {
                return true;
            }
            let name = relative
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default();
            name.starts_with("llvm-")
                || matches!(name, "FileCheck" | "llc" | "lli" | "opt" | "bugpoint")
        },
    )?;
    copy_tree_preserving(
        &install.join("share/opt-viewer"),
        &staging.join("usr/share/opt-viewer"),
    )?;
    copy_preserving(
        &repo_root.join("src/toolchain/llvm-project/llvm/LICENSE.TXT"),
        &staging.join("usr/share/doc/llvm/copyright"),
    )
}

fn stage_llvm_development(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = llvm_install(repo_root);
    copy_tree_preserving(&install.join("include"), &staging.join("usr/include"))?;
    copy_tree_preserving(
        &install.join("lib/x86_64-linux-gnu/cmake"),
        &staging.join("usr/lib/x86_64-linux-gnu/cmake"),
    )?;
    copy_tree_filtered(
        &install.join("lib/x86_64-linux-gnu"),
        &staging.join("usr/lib/x86_64-linux-gnu"),
        &|relative, metadata| {
            if metadata.is_dir() {
                return true;
            }
            relative
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| {
                    name.ends_with(".a") || (name.ends_with(".so") && name != "libLLVM-22.so")
                })
                && !relative.starts_with("cmake")
        },
    )?;
    copy_preserving(
        &repo_root.join("src/toolchain/llvm-project/llvm/LICENSE.TXT"),
        &staging.join("usr/share/doc/llvm-dev/copyright"),
    )
}

fn stage_clang(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = llvm_install(repo_root);
    copy_tree_filtered(
        &install.join("bin"),
        &staging.join("usr/bin"),
        &|relative, metadata| {
            if metadata.is_dir() {
                return true;
            }
            let name = relative
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default();
            name.starts_with("clang")
                || matches!(
                    name,
                    "analyze-build"
                        | "diagtool"
                        | "git-clang-format"
                        | "hmaptool"
                        | "intercept-build"
                        | "reduce-chunk-list"
                        | "sancov"
                        | "sanstats"
                        | "scan-build"
                        | "scan-build-py"
                        | "scan-view"
                        | "verify-uselistorder"
                )
        },
    )?;
    copy_tree_preserving(
        &install.join("lib/x86_64-linux-gnu/clang"),
        &staging.join("usr/lib/x86_64-linux-gnu/clang"),
    )?;
    for relative in ["share/clang", "share/scan-build", "share/scan-view"] {
        copy_tree_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    copy_tree_preserving(
        &component_install(repo_root, "llvm").join("etc/clang"),
        &staging.join("etc/clang"),
    )?;
    copy_preserving(
        &repo_root.join("src/toolchain/llvm-project/clang/LICENSE.TXT"),
        &staging.join("usr/share/doc/clang/copyright"),
    )
}

fn stage_lld(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = llvm_install(repo_root);
    for name in ["lld", "ld.lld", "ld64.lld", "lld-link", "wasm-ld"] {
        copy_path_preserving(
            &install.join("bin").join(name),
            &staging.join("usr/bin").join(name),
        )?;
    }
    copy_preserving(
        &repo_root.join("src/toolchain/llvm-project/lld/LICENSE.TXT"),
        &staging.join("usr/share/doc/lld/copyright"),
    )
}

fn rust_install(repo_root: &Path) -> PathBuf {
    component_install(repo_root, "rust").join("usr")
}

fn stage_rustc(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = rust_install(repo_root);
    copy_tree_filtered(
        &install.join("bin"),
        &staging.join("usr/bin"),
        &|relative, metadata| {
            metadata.is_dir()
                || relative
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name != "cargo")
        },
    )?;
    copy_tree_preserving(&install.join("lib"), &staging.join("usr/lib"))?;
    copy_tree_preserving(
        &install.join("share/doc/rustc"),
        &staging.join("usr/share/doc/rustc"),
    )?;
    for name in ["rustc.1", "rustdoc.1"] {
        copy_preserving(
            &install.join("share/man/man1").join(name),
            &staging.join("usr/share/man/man1").join(name),
        )?;
    }
    Ok(())
}

fn stage_cargo(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = rust_install(repo_root);
    copy_preserving(&install.join("bin/cargo"), &staging.join("usr/bin/cargo"))?;
    copy_tree_preserving(
        &install.join("share/doc/cargo"),
        &staging.join("usr/share/doc/cargo"),
    )?;
    copy_tree_filtered(
        &install.join("share/man/man1"),
        &staging.join("usr/share/man/man1"),
        &|relative, metadata| {
            metadata.is_dir()
                || relative
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name == "cargo.1" || name.starts_with("cargo-"))
        },
    )?;
    copy_preserving(
        &install.join("share/zsh/site-functions/_cargo"),
        &staging.join("usr/share/zsh/site-functions/_cargo"),
    )?;
    Ok(())
}

fn stage_filesystem(staging: &Path) -> Result<()> {
    for rel in [
        "usr/bin",
        "usr/sbin",
        "usr/lib",
        "usr/lib64",
        "usr/share",
        "usr/share/doc",
        "etc",
        "var",
        "var/lib",
        "home",
        "root",
        "run",
        "tmp",
    ] {
        fs::create_dir_all(staging.join(rel))?;
    }
    set_mode(staging.join("root"), 0o700)?;
    set_mode(staging.join("tmp"), 0o1777)?;
    #[cfg(unix)]
    for (link, target) in [
        ("bin", "usr/bin"),
        ("sbin", "usr/sbin"),
        ("lib", "usr/lib"),
        ("lib64", "usr/lib64"),
    ] {
        std::os::unix::fs::symlink(target, staging.join(link))?;
    }
    Ok(())
}

const GLIBC_RUNTIME_LIBRARIES: &[&str] = &[
    "libBrokenLocale.so.1",
    "libanl.so.1",
    "libc.so.6",
    "libdl.so.2",
    "libm.so.6",
    "libmvec.so.1",
    "libnsl.so.1",
    "libnss_compat.so.2",
    "libnss_db.so.2",
    "libnss_dns.so.2",
    "libnss_files.so.2",
    "libnss_hesiod.so.2",
    "libpthread.so.0",
    "libresolv.so.2",
    "librt.so.1",
    "libthread_db.so.1",
    "libutil.so.1",
];

fn stage_glibc_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/glibc/install");
    let source_libdir = install.join("usr/lib/x86_64-linux-gnu");
    let destination_libdir = staging.join("usr/lib/x86_64-linux-gnu");
    fs::create_dir_all(&destination_libdir)?;
    let mut manifest = Vec::new();
    for name in GLIBC_RUNTIME_LIBRARIES {
        let source = source_libdir.join(name);
        let destination = destination_libdir.join(name);
        copy_path_preserving(&source, &destination)?;
        manifest.push(format!(
            "/usr/lib/x86_64-linux-gnu/{name}\t{}",
            sha256_file(&destination)?
        ));
    }
    let loader = staging.join("usr/lib64/ld-linux-x86-64.so.2");
    copy_path_preserving(&install.join("lib64/ld-linux-x86-64.so.2"), &loader)?;
    manifest.push(format!(
        "/usr/lib64/ld-linux-x86-64.so.2\t{}",
        sha256_file(&loader)?
    ));
    manifest.sort();
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/COPYING.LIB"),
        &staging.join("usr/share/doc/libc6/copyright"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/LICENSES"),
        &staging.join("usr/share/doc/libc6/LICENSES"),
    )?;
    fs::write(
        staging.join("usr/share/doc/libc6/runtime-files.tsv"),
        format!("path\tsha256\n{}\n", manifest.join("\n")),
    )?;
    Ok(())
}

fn stage_glibc_utilities(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/glibc/install");
    for name in ["getent", "locale"] {
        stage_executable(
            &install.join("usr/bin").join(name),
            &staging.join("usr/bin").join(name),
            0o755,
        )?;
    }
    copy_path_preserving(&install.join("usr/bin/ldd"), &staging.join("usr/bin/ldd"))?;
    stage_executable(
        &install.join("sbin/ldconfig"),
        &staging.join("usr/sbin/ldconfig"),
        0o755,
    )?;
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/COPYING.LIB"),
        &staging.join("usr/share/doc/libc-bin/copyright"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/LICENSES"),
        &staging.join("usr/share/doc/libc-bin/LICENSES"),
    )?;
    Ok(())
}

/// Ship glibc's own locale definitions and compiler.  The installer uses
/// these source-owned inputs to generate exactly the selected locale in the
/// target instead of claiming host-generated locales are available.
fn stage_glibc_locales(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/glibc/install/usr");
    stage_executable(
        &install.join("bin/localedef"),
        &staging.join("usr/bin/localedef"),
        0o755,
    )?;
    copy_tree_preserving(&install.join("share/i18n"), &staging.join("usr/share/i18n"))?;
    if !staging.join("usr/share/i18n/locales/en_US").is_file()
        || !staging.join("usr/share/i18n/charmaps/UTF-8.gz").is_file()
    {
        bail!("glibc locale package is missing en_US or UTF-8 source data")
    }
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/COPYING.LIB"),
        &staging.join("usr/share/doc/locales/copyright"),
    )?;
    Ok(())
}

/// Ship the pinned ISO-codes JSON contract required by locales-rs.  This is
/// source data, not a host locale database and is intentionally limited to
/// the three registries consumed by COSMIC Initial Setup.
fn stage_iso_codes(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/iso-codes");
    let destination = staging.join("usr/share/iso-codes/json");
    fs::create_dir_all(&destination)?;
    for name in ["iso_3166-1.json", "iso_639-2.json", "iso_639-3.json"] {
        let path = source.join(name);
        if !path.is_file() {
            bail!("ISO-codes source is missing {name}");
        }
        copy_preserving(&path, &destination.join(name))?;
    }
    copy_preserving(
        &source.join("PROVENANCE.md"),
        &staging.join("usr/share/doc/iso-codes/PROVENANCE.md"),
    )?;
    Ok(())
}

/// Compile the pinned IANA database in an output-owned mirror and package
/// only the runtime zoneinfo tree; no host timezone files are consulted.
fn stage_tzdata(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/tzdata");
    let output = repo_root.join("out/build/tzdata");
    let build_source = output.join("source");
    let zoneinfo = output.join("zoneinfo");
    remove_path_if_exists(&output)?;
    sync_build_source(&source, &build_source)?;
    fs::create_dir_all(&zoneinfo)?;
    run_cmd(&build_source, "make", &["zic"])?;
    let zic = build_source.join("zic");
    let destination = format!("-d{}", zoneinfo.display());
    run_cmd(
        &build_source,
        path_str(&zic)?,
        &[
            destination.as_str(),
            "africa",
            "antarctica",
            "asia",
            "australasia",
            "backward",
            "etcetera",
            "europe",
            "northamerica",
            "southamerica",
        ],
    )?;
    for file in ["zone.tab", "zone1970.tab", "iso3166.tab"] {
        copy_preserving(&source.join(file), &zoneinfo.join(file))?;
    }
    if !zoneinfo.join("Etc/UTC").is_file() || !zoneinfo.join("America/Los_Angeles").is_file() {
        bail!("pinned tzdata build did not produce canonical zoneinfo files")
    }
    copy_tree_preserving(&zoneinfo, &staging.join("usr/share/zoneinfo"))?;
    copy_preserving(
        &source.join("LICENSE"),
        &staging.join("usr/share/doc/tzdata/copyright"),
    )?;
    Ok(())
}

/// Stage the complete WHENCE-described firmware closure. Firmware is the
/// documented source-closure exception: the authoritative, pinned upstream
/// tree and its redistribution metadata are retained even though most payload
/// files are device bytecode rather than preferred-form source.
fn stage_linux_firmware(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/linux-firmware");
    let firmware = staging.join("usr/lib/firmware");
    if !source.join("WHENCE").is_file() || !source.join("copy-firmware.sh").is_file() {
        bail!("pinned linux-firmware source is missing WHENCE or its installer")
    }
    fs::create_dir_all(&firmware)?;
    run_cmd(
        &source,
        "sh",
        &["./copy-firmware.sh", "--zstd", path_str(&firmware)?],
    )?;
    if !firmware.join("intel").is_dir()
        || !firmware.join("amdgpu").is_dir()
        || !firmware
            .join("intel/iwlwifi/iwlwifi-so-a0-gf-a0-83.ucode.zst")
            .is_file()
    {
        bail!("linux-firmware staging lacks broad Intel/AMD firmware coverage")
    }
    let documentation = staging.join("usr/share/doc/linux-firmware");
    for name in ["WHENCE", "README.md", "LICENSE"] {
        copy_preserving(&source.join(name), &documentation.join(name))?;
    }
    copy_tree_preserving(&source.join("LICENSES"), &documentation.join("LICENSES"))?;
    for entry in fs::read_dir(&source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.path().is_file() && (name.starts_with("LICENCE.") || name.starts_with("LICENSE."))
        {
            copy_preserving(&entry.path(), &documentation.join(name.as_ref()))?;
        }
    }
    Ok(())
}

/// Regenerate the canonical database in an output-owned directory and require
/// it to match the signed upstream artifact before pairing it with that
/// artifact's detached signature and public redistribution metadata.
fn stage_wireless_regdb(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/wireless-regdb");
    let output = repo_root.join("out/build/wireless-regdb");
    remove_path_if_exists(&output)?;
    fs::create_dir_all(&output)?;
    run_cmd(
        &source,
        "python3",
        &[
            "db2fw.py",
            path_str(&output.join("regulatory.db"))?,
            "db.txt",
        ],
    )?;
    let generated = fs::read(output.join("regulatory.db"))?;
    let signed_payload = fs::read(source.join("regulatory.db"))?;
    if generated != signed_payload {
        bail!("generated wireless regulatory database does not match the pinned signed artifact")
    }
    let firmware = staging.join("usr/lib/firmware");
    copy_preserving(
        &output.join("regulatory.db"),
        &firmware.join("regulatory.db"),
    )?;
    copy_preserving(
        &source.join("regulatory.db.p7s"),
        &firmware.join("regulatory.db.p7s"),
    )?;
    let documentation = staging.join("usr/share/doc/wireless-regdb");
    copy_preserving(&source.join("LICENSE"), &documentation.join("copyright"))?;
    copy_preserving(&source.join("db.txt"), &documentation.join("db.txt"))?;
    copy_preserving(
        &source.join("wens.key.pub.pem"),
        &documentation.join("wens.key.pub.pem"),
    )?;
    Ok(())
}

fn stage_brush(repo_root: &Path, staging: &Path) -> Result<()> {
    let bin_dir = staging.join("usr/bin");
    stage_executable(
        &repo_root.join("out/build/brush/cargo-target/release/brush"),
        &bin_dir.join("brush"),
        0o755,
    )?;
    #[cfg(unix)]
    for alias in ["sh", "bash"] {
        std::os::unix::fs::symlink("brush", bin_dir.join(alias))?;
    }
    Ok(())
}

fn stage_gcc_runtime_library(
    repo_root: &Path,
    staging: &Path,
    soname: &str,
    package: &str,
) -> Result<()> {
    let source_libdir = repo_root.join("out/build/gcc-runtime/runtime/usr/lib/x86_64-linux-gnu");
    let destination_libdir = staging.join("usr/lib/x86_64-linux-gnu");
    fs::create_dir_all(&destination_libdir)?;
    let soname_source = source_libdir.join(soname);
    if !path_entry_exists(&soname_source) {
        bail!(
            "GCC runtime build is missing {soname} at {}; build the gcc-runtime stage first",
            soname_source.display()
        )
    }
    let mut installed = Vec::new();
    if fs::symlink_metadata(&soname_source)?
        .file_type()
        .is_symlink()
    {
        let target = fs::read_link(&soname_source)?;
        let target_name = target
            .file_name()
            .ok_or_else(|| anyhow!("invalid GCC runtime symlink {}", soname_source.display()))?;
        copy_path_preserving(
            &source_libdir.join(target_name),
            &destination_libdir.join(target_name),
        )?;
        installed.push(target_name.to_string_lossy().to_string());
    }
    copy_path_preserving(&soname_source, &destination_libdir.join(soname))?;
    installed.push(soname.to_string());
    installed.sort();

    for license in ["COPYING3", "COPYING.RUNTIME"] {
        copy_preserving(
            &repo_root.join("src/toolchain/gcc").join(license),
            &staging.join("usr/share/doc").join(package).join(license),
        )?;
    }
    copy_preserving(
        &repo_root.join("out/build/gcc-runtime/runtime-abi.tsv"),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("runtime-abi.tsv"),
    )?;
    fs::write(
        staging
            .join("usr/share/doc")
            .join(package)
            .join("runtime-files.txt"),
        format!("{}\n", installed.join("\n")),
    )?;
    Ok(())
}

fn stage_base_files(repo_root: &Path, staging: &Path) -> Result<()> {
    let skeleton = repo_root.join("src/rootfs/skeleton/etc");
    for name in ["os-release", "hostname", "profile", "shells"] {
        copy_preserving(&skeleton.join(name), &staging.join("etc").join(name))?;
    }
    copy_preserving(
        &repo_root.join("src/system/packages/config/base-files/environment"),
        &staging.join("etc/environment"),
    )?;
    let config = repo_root.join("src/system/packages/config/base-files");
    copy_preserving(&config.join("issue"), &staging.join("etc/issue"))?;
    let conffiles = [
        "/etc/hostname", "/etc/profile", "/etc/shells", "/etc/issue", "/etc/environment",
    ];
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", conffiles.join("\n")),
    )?;
    Ok(())
}

fn stage_ca_certificates(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/network");
    let bundle = source.join("ca-certificates.crt");
    let metadata = source.join("ca-bundle.toml");
    let parsed: toml::Value = toml::from_str(&fs::read_to_string(&metadata)?)?;
    let expected_sha = parsed
        .get("sha256")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| anyhow!("CA bundle metadata lacks sha256"))?;
    if sha256_file(&bundle)? != expected_sha {
        bail!("pinned CA bundle checksum does not match ca-bundle.toml")
    }
    let expected_count = parsed
        .get("certificate_count")
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| anyhow!("CA bundle metadata lacks certificate_count"))?;
    let actual_count = fs::read_to_string(&bundle)?
        .matches("-----BEGIN CERTIFICATE-----")
        .count() as i64;
    if actual_count != expected_count {
        bail!("pinned CA bundle contains {actual_count} certificates; expected {expected_count}")
    }
    copy_preserving(&bundle, &staging.join("etc/ssl/certs/ca-certificates.crt"))?;
    let openssl_default = staging.join("etc/ssl/cert.pem");
    if let Some(parent) = openssl_default.parent() {
        fs::create_dir_all(parent)?;
    }
    std::os::unix::fs::symlink("certs/ca-certificates.crt", &openssl_default)?;
    copy_preserving(
        &metadata,
        &staging.join("usr/share/doc/ca-certificates/ca-bundle.toml"),
    )?;
    fs::write(
        staging.join("usr/share/doc/ca-certificates/UPDATE.md"),
        "Update only by replacing the pinned curl CA Extract input, then update the source URL, Mozilla data timestamp, SHA-256, certificate count, and MPL-2.0 license metadata in ca-bundle.toml. Ordinary builds never download a mutable CA bundle.\n",
    )?;
    Ok(())
}

fn stage_dpkg(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/dpkg/install");
    for rel in DPKG_RUNTIME_PATHS {
        stage_executable(&install.join(rel), &staging.join(rel), 0o755)?;
    }
    copy_tree_preserving(
        &install.join("usr/share/dpkg"),
        &staging.join("usr/share/dpkg"),
    )?;
    fs::create_dir_all(staging.join("etc/dpkg/dpkg.cfg.d"))?;
    fs::create_dir_all(staging.join("etc/alternatives"))?;
    fs::create_dir_all(staging.join("var/lib/dpkg/alternatives"))?;
    copy_preserving(
        &repo_root.join("src/system/packages/config/dpkg/dpkg.cfg"),
        &staging.join("etc/dpkg/dpkg.cfg"),
    )?;
    fs::write(staging.join("DEBIAN/conffiles"), "/etc/dpkg/dpkg.cfg\n")?;
    validate_no_mutable_package_state(staging)
}

fn stage_libapt_pkg(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("out/build/apt/install/usr/lib/x86_64-linux-gnu");
    let destination = staging.join("usr/lib/x86_64-linux-gnu");
    for name in ["libapt-pkg.so.7.0.0", "libapt-pkg.so.7.0", "libapt-pkg.so"] {
        copy_path_preserving(&source.join(name), &destination.join(name))?;
    }
    Ok(())
}

fn stage_apt(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/apt/install");
    for rel in APT_RUNTIME_PATHS {
        stage_executable(&install.join(rel), &staging.join(rel), 0o755)?;
    }
    for rel in ["usr/lib/apt/planners", "usr/lib/apt/solvers"] {
        copy_tree_preserving(&install.join(rel), &staging.join(rel))?;
    }
    let libdir = "usr/lib/x86_64-linux-gnu";
    for name in ["libapt-private.so.0.0.0", "libapt-private.so.0.0"] {
        copy_path_preserving(
            &install.join(libdir).join(name),
            &staging.join(libdir).join(name),
        )?;
    }
    let config = repo_root.join("src/system/packages/config/apt");
    copy_preserving(
        &config.join("mattos.sources"),
        &staging.join("etc/apt/sources.list.d/mattos.sources"),
    )?;
    copy_preserving(
        &config.join("01mattos"),
        &staging.join("etc/apt/apt.conf.d/01mattos"),
    )?;
    for name in ["mattos-hosted.sources", "debian-trixie.sources"] {
        copy_preserving(
            &config.join(name),
            &staging.join("etc/apt/sources.list.d").join(name),
        )?;
    }
    copy_preserving(
        &config.join("00mattos-priority"),
        &staging.join("etc/apt/preferences.d/00mattos-priority"),
    )?;
    let installed_config = config.join("installed");
    for name in [
        "01mattos",
        "00mattos-priority",
        "mattos.sources",
        "mattos-hosted.sources",
        "debian-trixie.sources",
    ] {
        copy_preserving(
            &installed_config.join(name),
            &staging.join("usr/share/mattos/apt/installed").join(name),
        )?;
    }
    for name in ["mattos-archive-keyring.asc", "debian-archive-keyring.asc"] {
        copy_preserving(
            &config.join("keys").join(name),
            &staging.join("usr/share/keyrings").join(name),
        )?;
    }
    for rel in [
        "etc/apt/auth.conf.d",
        "etc/apt/preferences.d",
        "etc/apt/trusted.gpg.d",
        "var/lib/apt/lists/partial",
        "var/cache/apt/archives/partial",
        "var/log/apt",
    ] {
        fs::create_dir_all(staging.join(rel))?;
    }
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", APT_CONFFILES.join("\n")),
    )?;
    validate_no_mutable_package_state(staging)
}

/// Apply the deliberately offline/live APT policy after package installation.
/// The apt package itself carries the installed policy templates; this overlay
/// makes the ISO root deterministic without relying on network availability.
pub(crate) fn apply_live_apt_policy(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let config = repo_root.join("src/system/packages/config/apt");
    copy_preserving(
        &config.join("01mattos"),
        &rootfs.join("etc/apt/apt.conf.d/01mattos"),
    )?;
    copy_preserving(
        &config.join("mattos.sources"),
        &rootfs.join("etc/apt/sources.list.d/mattos.sources"),
    )?;
    for name in ["mattos-hosted.sources", "debian-trixie.sources"] {
        copy_preserving(
            &config.join(name),
            &rootfs.join("etc/apt/sources.list.d").join(name),
        )?;
    }
    copy_preserving(
        &config.join("00mattos-priority"),
        &rootfs.join("etc/apt/preferences.d/00mattos-priority"),
    )?;
    validate_live_apt_policy(rootfs)
}

pub(crate) fn validate_live_apt_policy(rootfs: &Path) -> Result<()> {
    let local = fs::read_to_string(rootfs.join("etc/apt/sources.list.d/mattos.sources"))?;
    let hosted = fs::read_to_string(rootfs.join("etc/apt/sources.list.d/mattos-hosted.sources"))?;
    let debian = fs::read_to_string(rootfs.join("etc/apt/sources.list.d/debian-trixie.sources"))?;
    let preferences = fs::read_to_string(rootfs.join("etc/apt/preferences.d/00mattos-priority"))?;
    let keyrings = rootfs.join("usr/share/keyrings");
    if !local.contains("URIs: file:/usr/share/mattos/repository")
        || local.contains("Enabled: no")
        || !local.contains("Trusted: yes")
        || !hosted.contains("Enabled: no")
        || !debian.contains("Enabled: no")
        || !hosted.contains("Signed-By: /usr/share/keyrings/mattos-archive-keyring.asc")
        || !debian.contains("Signed-By: /usr/share/keyrings/debian-archive-keyring.asc")
        || !keyrings.join("mattos-archive-keyring.asc").is_file()
        || !keyrings.join("debian-archive-keyring.asc").is_file()
        || !rootfs.join("usr/bin/gpgv").is_file()
        || !preferences.contains("Pin-Priority: 1001")
    {
        bail!("live APT policy is not embedded-repository-only")
    }
    Ok(())
}

fn component_install(repo_root: &Path, component: &str) -> PathBuf {
    repo_root.join("out/build").join(component).join("install")
}

fn stage_udev_hwdb(repo_root: &Path, staging: &Path) -> Result<()> {
    let systemd_install = component_install(repo_root, "systemd");

    // Meson converts the authoritative imported systemd hwdb inputs into the
    // exact vendor-source closure selected by this pinned systemd revision.
    // Stage that closure into the package-owned output tree, then compile the
    // binary database there. The imported systemd tree is never writable.
    copy_tree_preserving(
        &systemd_install.join(UDEV_HWDB_SOURCE_REL),
        &staging.join(UDEV_HWDB_SOURCE_REL),
    )?;
    for rel in [UDEV_HWDB_UNIT_REL, UDEV_HWDB_WANTS_REL] {
        copy_path_preserving(&systemd_install.join(rel), &staging.join(rel))?;
    }
    generate_udev_hwdb(repo_root, staging)?;
    copy_preserving(
        &repo_root.join("src/system/systemd/LICENSE.LGPL2.1"),
        &staging.join("usr/share/doc/udev/copyright"),
    )?;
    validate_udev_hwdb_payload(repo_root, staging)
}

fn generate_udev_hwdb(repo_root: &Path, root: &Path) -> Result<()> {
    let glibc_install = component_install(repo_root, "glibc");
    let systemd_install = component_install(repo_root, "systemd");
    let loader = glibc_install.join("lib64/ld-linux-x86-64.so.2");
    let generator = systemd_install.join("usr/bin/systemd-hwdb");
    let library_path = std::env::join_paths([
        glibc_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu/systemd"),
    ])?;
    let root_arg = format!("--root={}", root.display());
    let loader_text = path_str(&loader)?;
    let generator_text = path_str(&generator)?;
    let library_path_text = library_path
        .to_str()
        .ok_or_else(|| anyhow!("systemd-hwdb library path is not valid UTF-8"))?;
    run_cmd(
        repo_root,
        loader_text,
        &[
            "--library-path",
            library_path_text,
            generator_text,
            &root_arg,
            "--usr",
            "--strict",
            "update",
        ],
    )
}

pub(crate) fn validate_udev_hwdb_payload(repo_root: &Path, root: &Path) -> Result<()> {
    let database = root.join(UDEV_HWDB_BINARY_REL);
    let bytes = fs::read(&database)
        .with_context(|| format!("prebuilt udev hwdb is missing at {}", database.display()))?;
    if bytes.len() < 1_000_000 || !bytes.starts_with(b"KSLPHHRH") {
        bail!("prebuilt udev hwdb has an invalid header or implausible size")
    }
    if path_entry_exists(&root.join("etc/udev/hwdb.bin")) {
        bail!("mutable /etc/udev/hwdb.bin must not be baked into the udev package")
    }

    let systemd_install = component_install(repo_root, "systemd");
    let glibc_install = component_install(repo_root, "glibc");
    let loader = glibc_install.join("lib64/ld-linux-x86-64.so.2");
    let generator = systemd_install.join("usr/bin/systemd-hwdb");
    let library_path = std::env::join_paths([
        glibc_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu/systemd"),
    ])?;
    let output = Command::new(loader)
        .args(["--library-path"])
        .arg(library_path)
        .arg(generator)
        .arg(format!("--root={}", root.display()))
        .args(["query", UDEV_HWDB_TEST_MODALIAS])
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("SOURCE_DATE_EPOCH", SOURCE_DATE_EPOCH.to_string())
        .output()
        .context("failed to query the prebuilt udev hwdb")?;
    if !output.status.success() {
        bail!("source-built systemd-hwdb rejected the prebuilt database")
    }
    let stdout = String::from_utf8(output.stdout)?;
    for required in ["Intel Corporation", "82540EM Gigabit Ethernet Controller"] {
        if !stdout.contains(required) {
            bail!("prebuilt udev hwdb query is missing {required}")
        }
    }
    Ok(())
}

fn stage_runtime_paths(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    paths: &[&str],
) -> Result<()> {
    let install = component_install(repo_root, component);
    for rel in paths {
        copy_path_preserving(&install.join(rel), &staging.join(rel))?;
    }
    Ok(())
}

fn stage_cosmic_desktop(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cosmic-desktop");
    copy_tree_preserving(&install.join("usr"), &staging.join("usr"))?;
    // The pinned settings schema carries the complete v1 component styling,
    // while the matching libcosmic v2 model still requires list_button. Keep
    // this MattOS integration in package staging so an integration-only change
    // does not invalidate and rebuild every upstream COSMIC workspace.
    for theme in ["Dark", "Light"] {
        let theme_root = staging.join(format!("usr/share/cosmic/com.system76.CosmicTheme.{theme}"));
        let source = theme_root.join("v1/list_button");
        let destination = theme_root.join("v2/list_button");
        if !source.is_file() || !destination.parent().is_some_and(Path::is_dir) {
            bail!("COSMIC {theme} theme lacks the expected v1/v2 schemas");
        }
        fs::copy(source, destination)?;
    }
    fs::write(
        staging.join("usr/share/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom"),
        "{}\n",
    )?;
    let start_cosmic = staging.join("usr/bin/start-cosmic");
    let wayland_session = fs::read_to_string(&start_cosmic)?
        .replace("GDK_BACKEND=wayland,x11", "GDK_BACKEND=wayland")
        .replace("QT_QPA_PLATFORM=\"wayland;xcb\"", "QT_QPA_PLATFORM=wayland")
        .replace(
            "exec /usr/bin/dbus-run-session -- /usr/bin/cosmic-session",
            "exec /usr/bin/dbus-run-session --config-file=/usr/share/dbus-1/mattos-private-session.conf -- /usr/bin/cosmic-session",
        );
    fs::write(&start_cosmic, wayland_session)?;

    let integration = repo_root.join("src/system/session/cosmic");
    copy_preserving(
        &integration.join("cosmic-greeter.toml"),
        &staging.join("etc/greetd/cosmic-greeter.toml"),
    )?;
    copy_preserving(
        &integration.join("cosmic-greeter.pam"),
        &staging.join("etc/pam.d/cosmic-greeter"),
    )?;
    copy_preserving(
        &integration.join("cosmic-greeter-start"),
        &staging.join("usr/bin/cosmic-greeter-start"),
    )?;
    set_mode(staging.join("usr/bin/cosmic-greeter-start"), 0o755)?;
    for unit in ["cosmic-greeter.service", "cosmic-greeter-daemon.service"] {
        copy_preserving(
            &integration.join(unit),
            &staging.join("usr/lib/systemd/system").join(unit),
        )?;
    }
    copy_preserving(
        &integration.join("cosmic-desktop.conf"),
        &staging.join("usr/lib/environment.d/90-cosmic-desktop.conf"),
    )?;
    copy_preserving(
        &integration.join("README.md"),
        &staging.join("usr/share/doc/cosmic-desktop/README.md"),
    )?;
    copy_preserving(
        &integration.join("hicolor-index.theme"),
        &staging.join("usr/share/icons/hicolor/index.theme"),
    )?;

    let display_manager = staging.join("etc/systemd/system/display-manager.service");
    fs::create_dir_all(display_manager.parent().expect("display-manager parent"))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        "/usr/lib/systemd/system/cosmic-greeter.service",
        &display_manager,
    )?;
    for required in [
        "usr/bin/cosmic-session",
        "usr/bin/cosmic-panel",
        "usr/bin/cosmic-launcher",
        "usr/bin/cosmic-term",
        "usr/bin/cosmic-ext-tweaks",
        "usr/bin/greetd",
        "usr/bin/cosmic-greeter-start",
        "usr/share/wayland-sessions/cosmic.desktop",
        "usr/share/icons/Pop/cursors/default",
        "usr/share/icons/hicolor/index.theme",
        "usr/share/fonts/truetype/open-sans/OpenSans-Regular.ttf",
        "usr/share/fonts/truetype/noto/NotoSansMono[wdth,wght].ttf",
        "etc/pam.d/cosmic-greeter",
        "etc/systemd/system/display-manager.service",
    ] {
        if fs::symlink_metadata(staging.join(required)).is_err() {
            bail!("cosmic-desktop package is missing /{required}");
        }
    }
    let launcher = fs::read_to_string(staging.join("usr/bin/cosmic-greeter-start"))?;
    for contract in [
        "LIBSEAT_BACKEND=logind",
        "XDG_SESSION_TYPE=wayland",
        "cosmic-comp --no-xwayland /usr/bin/cosmic-greeter",
    ] {
        if !launcher.contains(contract) {
            bail!("COSMIC greeter launcher is missing runtime contract: {contract}");
        }
    }
    let greeter_unit =
        fs::read_to_string(staging.join("usr/lib/systemd/system/cosmic-greeter.service"))?;
    if !greeter_unit.contains("Restart=always")
        || !greeter_unit.contains("After=systemd-user-sessions.service systemd-logind.service")
        || !greeter_unit.contains("TimeoutStopSec=10s")
    {
        bail!(
            "COSMIC display manager lacks logind ordering, bounded shutdown, or restart recovery"
        );
    }
    Ok(())
}

fn stage_cosmic_edit(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cosmic-edit");
    copy_tree_preserving(&install.join("usr"), &staging.join("usr"))?;
    copy_preserving(
        &repo_root.join("src/desktop/cosmic/cosmic-edit/LICENSE"),
        &staging.join("usr/share/doc/cosmic-edit/copyright"),
    )?;
    for required in [
        "usr/bin/cosmic-edit",
        "usr/share/applications/com.system76.CosmicEdit.desktop",
        "usr/share/metainfo/com.system76.CosmicEdit.metainfo.xml",
    ] {
        if !staging.join(required).is_file() {
            bail!("cosmic-edit package is missing /{required}");
        }
    }
    let desktop = fs::read_to_string(staging.join(
        "usr/share/applications/com.system76.CosmicEdit.desktop",
    ))?;
    if !desktop.contains("Exec=cosmic-edit %F") || !desktop.contains("MimeType=text/plain;") {
        bail!("cosmic-edit desktop entry does not advertise the expected editor contract");
    }
    Ok(())
}

fn stage_cosmic_initial_setup(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cosmic-initial-setup");
    copy_tree_preserving(&install.join("usr"), &staging.join("usr"))?;
    copy_tree_preserving(&install.join("etc"), &staging.join("etc"))?;
    let launcher = staging.join("usr/libexec/mattos/cosmic-initial-setup-autostart");
    if let Some(parent) = launcher.parent() { fs::create_dir_all(parent)?; }
    fs::write(&launcher, "#!/bin/sh\n# Live media starts COSMIC without running the installed-user wizard.\n[ ! -e /run/mattos-live ] || exit 0\nexec /usr/bin/cosmic-initial-setup\n")?;
    set_mode(launcher, 0o755)?;
    let desktop = staging.join("etc/xdg/autostart/com.system76.CosmicInitialSetup.Autostart.desktop");
    let body = fs::read_to_string(&desktop)?.replace("Exec=cosmic-initial-setup", "Exec=/usr/libexec/mattos/cosmic-initial-setup-autostart");
    fs::write(desktop, body)?;
    copy_preserving(&repo_root.join("src/desktop/cosmic/cosmic-initial-setup/LICENSE"), &staging.join("usr/share/doc/cosmic-initial-setup/copyright"))?;
    for rel in [
        "usr/bin/cosmic-initial-setup",
        "usr/share/applications/com.system76.CosmicInitialSetup.desktop",
        "etc/xdg/autostart/com.system76.CosmicInitialSetup.Autostart.desktop",
        "usr/share/icons/hicolor/scalable/apps/com.system76.CosmicInitialSetup.svg",
        "usr/share/polkit-1/rules.d/20-cosmic-initial-setup.rules",
        "usr/share/cosmic-layouts/top-panel-and-bottom-dock/layout.kdl",
        "usr/share/cosmic-layouts/top-panel-and-bottom-dock/icon.png",
        "usr/share/cosmic-themes/nebula-dark.ron",
    ] {
        if !staging.join(rel).is_file() { bail!("cosmic-initial-setup package is missing /{rel}"); }
    }
    Ok(())
}

fn stage_network_manager(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "networkmanager");
    copy_tree_preserving(&install.join("usr"), &staging.join("usr"))?;
    copy_tree_preserving(&install.join("etc"), &staging.join("etc"))?;
    for rel in ["usr/sbin/NetworkManager", "usr/bin/nmcli", "usr/lib/systemd/system/NetworkManager.service", "usr/lib/systemd/system/NetworkManager-wait-online.service"] {
        if !staging.join(rel).exists() { bail!("network-manager package is missing /{rel}"); }
    }
    Ok(())
}

fn stage_cozy(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_executable(
        &component_install(repo_root, "cozy").join("usr/bin/cozy"),
        &staging.join("usr/bin/cozy"),
        0o755,
    )?;
    for license in ["LICENSE-MIT", "LICENSE-APACHE"] {
        copy_preserving(
            &repo_root.join("src/userland/cozy").join(license),
            &staging.join("usr/share/doc/mattos-cozy").join(license),
        )?;
    }
    Ok(())
}

fn stage_pipewire(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "pipewire");
    for relative in [
        "usr/bin",
        "usr/lib/x86_64-linux-gnu",
        "usr/lib/systemd/user",
        "usr/share/pipewire",
    ] {
        copy_tree_preserving(&install.join(relative), &staging.join(relative))?;
    }
    copy_preserving(
        &repo_root.join("src/system/multimedia/pipewire/COPYING"),
        &staging.join("usr/share/doc/pipewire/copyright"),
    )?;
    let socket_wants = staging.join("usr/lib/systemd/user/sockets.target.wants");
    fs::create_dir_all(&socket_wants)?;
    #[cfg(unix)]
    for unit in ["pipewire.socket", "pipewire-pulse.socket"] {
        std::os::unix::fs::symlink(format!("../{unit}"), socket_wants.join(unit))?;
    }
    for required in [
        "usr/bin/pipewire",
        "usr/bin/pipewire-pulse",
        "usr/lib/x86_64-linux-gnu/libpipewire-0.3.so.0",
        "usr/lib/systemd/user/pipewire.service",
        "usr/lib/systemd/user/pipewire.socket",
        "usr/lib/systemd/user/sockets.target.wants/pipewire.socket",
    ] {
        if fs::symlink_metadata(staging.join(required)).is_err() {
            bail!("pipewire package is missing /{required}");
        }
    }
    Ok(())
}

fn stage_mesa_dri_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "mesa");
    let library_dir = install.join("usr/lib/x86_64-linux-gnu");
    let gallium = fs::read_dir(&library_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .find(|name| {
            let name = name.to_string_lossy();
            name.starts_with("libgallium-") && name.ends_with(".so")
        })
        .ok_or_else(|| anyhow!("Mesa did not install its versioned Gallium DRI runtime"))?;
    let gallium_rel = format!("usr/lib/x86_64-linux-gnu/{}", gallium.to_string_lossy());
    stage_runtime_paths(
        repo_root,
        staging,
        "mesa",
        &[&gallium_rel, "usr/lib/x86_64-linux-gnu/gbm/dri_gbm.so"],
    )?;
    copy_tree_preserving(
        &install.join("usr/share/drirc.d"),
        &staging.join("usr/share/drirc.d"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/mesa/docs/license.rst"),
        &staging.join("usr/share/doc/libgl1-mesa-dri/copyright"),
    )?;
    Ok(())
}

fn stage_mesa_egl_vendor(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "mesa",
        &[
            "usr/lib/x86_64-linux-gnu/libEGL_mesa.so.0.0.0",
            "usr/lib/x86_64-linux-gnu/libEGL_mesa.so.0",
            "usr/share/glvnd/egl_vendor.d/50_mesa.json",
        ],
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/mesa/docs/license.rst"),
        &staging.join("usr/share/doc/libegl-mesa0/copyright"),
    )
}

fn stage_nvidia_package(repo_root: &Path, staging: &Path, package: &str) -> Result<()> {
    let install = component_install(repo_root, "nvidia-driver");
    let lib = "usr/lib/x86_64-linux-gnu";
    let copy_libraries = |names: &[&str]| -> Result<()> {
        for name in names {
            copy_path_preserving(&install.join(lib).join(name), &staging.join(lib).join(name))?;
        }
        Ok(())
    };
    match package {
        "linux-modules-nvidia-595-open-7.2.0-rc5-mattos" => {
            copy_tree_preserving(
                &install.join("usr/lib/modules/7.2.0-rc5-mattos/updates/nvidia"),
                &staging.join("usr/lib/modules/7.2.0-rc5-mattos/updates/nvidia"),
            )?;
            copy_preserving(
                &repo_root.join("src/system/graphics/nvidia-driver/nvidia-modprobe.conf"),
                &staging.join("etc/modprobe.d/nvidia.conf"),
            )?;
            copy_preserving(
                &install.join("usr/lib/modprobe.d/nvidia-supported-gpus.conf"),
                &staging.join("usr/lib/modprobe.d/nvidia-supported-gpus.conf"),
            )?;
            copy_preserving(
                &install.join("usr/libexec/mattos-nvidia-select"),
                &staging.join("usr/libexec/mattos-nvidia-select"),
            )?;
            fs::write(
                staging.join("DEBIAN/conffiles"),
                "/etc/modprobe.d/nvidia.conf\n",
            )?;
            fs::write(
                staging.join("DEBIAN/postinst"),
                "#!/bin/sh\nset -e\n# Offline image assembly runs depmod after all module packages are unpacked.\n[ -n \"${DPKG_ROOT:-}\" ] && exit 0\nif command -v depmod >/dev/null 2>&1; then depmod 7.2.0-rc5-mattos; fi\n",
            )?;
            set_mode(staging.join("DEBIAN/postinst"), 0o755)?;
            fs::write(
                staging.join("DEBIAN/postrm"),
                "#!/bin/sh\nset -e\n# Do not modify the build host while assembling an offline root.\n[ -n \"${DPKG_ROOT:-}\" ] && exit 0\nif command -v depmod >/dev/null 2>&1; then depmod 7.2.0-rc5-mattos; fi\n",
            )?;
            set_mode(staging.join("DEBIAN/postrm"), 0o755)?;
        }
        "nvidia-firmware-595" => copy_tree_preserving(
            &install.join("usr/lib/firmware/nvidia/595.84"),
            &staging.join("usr/lib/firmware/nvidia/595.84"),
        )?,
        "libnvidia-gl-595" => {
            copy_libraries(&[
                "libEGL_nvidia.so.595.84",
                "libEGL_nvidia.so.0",
                "libGLESv1_CM_nvidia.so.595.84",
                "libGLESv1_CM_nvidia.so.1",
                "libGLESv2_nvidia.so.595.84",
                "libGLESv2_nvidia.so.2",
                "libGLX_nvidia.so.595.84",
                "libGLX_nvidia.so.0",
                "libnvidia-allocator.so.595.84",
                "libnvidia-allocator.so.1",
                "libnvidia-egl-gbm.so.1.1.3",
                "libnvidia-egl-gbm.so.1",
                "libnvidia-egl-wayland.so.1.1.20",
                "libnvidia-egl-wayland.so.1",
                "libnvidia-egl-wayland2.so.1.0.1",
                "libnvidia-egl-wayland2.so.1",
                "libnvidia-eglcore.so.595.84",
                "libnvidia-glcore.so.595.84",
                "libnvidia-glsi.so.595.84",
                "libnvidia-glvkspirv.so.595.84",
                "libnvidia-gpucomp.so.595.84",
                "libnvidia-present.so.595.84",
                "libnvidia-tls.so.595.84",
            ])?;
            for relative in [
                "usr/share/glvnd/egl_vendor.d/10_nvidia.json",
                "usr/share/vulkan/icd.d/nvidia_icd.json",
                "usr/share/vulkan/implicit_layer.d/nvidia_layers.json",
                "usr/share/egl/egl_external_platform.d/09_nvidia_wayland2.json",
                "usr/share/egl/egl_external_platform.d/10_nvidia_wayland.json",
                "usr/share/egl/egl_external_platform.d/15_nvidia_gbm.json",
            ] {
                copy_path_preserving(&install.join(relative), &staging.join(relative))?;
            }
            let backend = staging.join("usr/lib/x86_64-linux-gnu/gbm/nvidia-drm_gbm.so");
            fs::create_dir_all(backend.parent().expect("NVIDIA GBM backend parent"))?;
            std::os::unix::fs::symlink("../libnvidia-allocator.so.1", backend)?;
            validate_nvidia_graphics_metadata(staging)?;
        }
        "libnvidia-compute-595" => copy_libraries(&[
            "libcuda.so.595.84",
            "libcuda.so.1",
            "libnvidia-ml.so.595.84",
            "libnvidia-ml.so.1",
            "libnvidia-ptxjitcompiler.so.595.84",
            "libnvidia-ptxjitcompiler.so.1",
        ])?,
        "libnvidia-encode-595" => {
            copy_libraries(&["libnvidia-encode.so.595.84", "libnvidia-encode.so.1"])?
        }
        "libnvidia-decode-595" => copy_libraries(&["libnvcuvid.so.595.84", "libnvcuvid.so.1"])?,
        "nvidia-utils-595" => {
            for name in ["nvidia-smi", "nvidia-modprobe", "nvidia-persistenced"] {
                copy_path_preserving(
                    &install.join("usr/bin").join(name),
                    &staging.join("usr/bin").join(name),
                )?;
            }
        }
        "nvidia-driver-595-open" => {}
        _ => bail!("unknown NVIDIA package {package}"),
    }
    copy_preserving(
        &install.join("usr/share/doc/nvidia-driver-595/LICENSE"),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("copyright"),
    )?;
    copy_preserving(
        &install.join("usr/share/doc/nvidia-driver-595/manifest.toml"),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("manifest.toml"),
    )?;
    for name in [
        "README.md",
        "runfile.sha256",
        "supported-gpus.json",
        "supported-gpus.LICENSE",
    ] {
        copy_preserving(
            &install.join("usr/share/doc/nvidia-driver-595").join(name),
            &staging.join("usr/share/doc").join(package).join(name),
        )?;
    }
    Ok(())
}

fn validate_nvidia_graphics_metadata(root: &Path) -> Result<()> {
    let icd_path = root.join("usr/share/vulkan/icd.d/nvidia_icd.json");
    let icd: serde_json::Value = serde_json::from_slice(&fs::read(&icd_path)?)?;
    let library = icd
        .pointer("/ICD/library_path")
        .and_then(|value| value.as_str());
    if library != Some("libGLX_nvidia.so.0")
        || !root
            .join("usr/lib/x86_64-linux-gnu/libGLX_nvidia.so.0")
            .is_symlink()
    {
        bail!("NVIDIA Vulkan ICD does not resolve through its canonical system SONAME");
    }
    for relative in [
        "usr/share/glvnd/egl_vendor.d/10_nvidia.json",
        "usr/share/egl/egl_external_platform.d/09_nvidia_wayland2.json",
        "usr/share/egl/egl_external_platform.d/10_nvidia_wayland.json",
        "usr/share/egl/egl_external_platform.d/15_nvidia_gbm.json",
    ] {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(root.join(relative))?)?;
        if value
            .get("file_format_version")
            .and_then(|version| version.as_str())
            .is_none()
        {
            bail!("NVIDIA metadata /{relative} is not a valid vendor manifest");
        }
    }
    Ok(())
}

fn stage_mesa_vulkan_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "mesa",
        &[
            "usr/lib/x86_64-linux-gnu/libVkLayer_MESA_device_select.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_radeon.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_intel.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_nouveau.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_virtio.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_lvp.so",
            "usr/share/vulkan/icd.d/radeon_icd.x86_64.json",
            "usr/share/vulkan/icd.d/intel_icd.x86_64.json",
            "usr/share/vulkan/icd.d/nouveau_icd.x86_64.json",
            "usr/share/vulkan/icd.d/virtio_icd.x86_64.json",
            "usr/share/vulkan/icd.d/lvp_icd.x86_64.json",
            "usr/share/vulkan/implicit_layer.d/VkLayer_MESA_device_select.json",
        ],
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/mesa/docs/license.rst"),
        &staging.join("usr/share/doc/mesa-vulkan-drivers/copyright"),
    )?;
    validate_vulkan_icd_manifests(staging)?;
    Ok(())
}

fn validate_vulkan_icd_manifests(root: &Path) -> Result<()> {
    let manifests = [
        ("radeon_icd.x86_64.json", "libvulkan_radeon.so"),
        ("intel_icd.x86_64.json", "libvulkan_intel.so"),
        ("nouveau_icd.x86_64.json", "libvulkan_nouveau.so"),
        ("virtio_icd.x86_64.json", "libvulkan_virtio.so"),
        ("lvp_icd.x86_64.json", "libvulkan_lvp.so"),
    ];
    for (manifest, library) in manifests {
        let path = root.join("usr/share/vulkan/icd.d").join(manifest);
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("invalid Vulkan ICD manifest {}", path.display()))?;
        let expected = format!("/usr/lib/x86_64-linux-gnu/{library}");
        if value.pointer("/ICD/library_path").and_then(|v| v.as_str()) != Some(&expected)
            || value
                .pointer("/ICD/api_version")
                .and_then(|v| v.as_str())
                .is_none()
            || value
                .get("file_format_version")
                .and_then(|v| v.as_str())
                .is_none()
        {
            bail!("Vulkan ICD manifest {} is not canonical", path.display())
        }
        if !root.join(expected.trim_start_matches('/')).is_file() {
            bail!("Vulkan ICD {manifest} references missing {expected}")
        }
    }
    Ok(())
}

fn stage_vulkan_development(repo_root: &Path, staging: &Path) -> Result<()> {
    let headers = component_install(repo_root, "vulkan-headers");
    for relative in [
        "usr/include/vulkan",
        "usr/include/vk_video",
        "usr/share/vulkan/registry",
        "usr/share/cmake/VulkanHeaders",
    ] {
        copy_tree_preserving(&headers.join(relative), &staging.join(relative))?;
    }
    let loader = component_install(repo_root, "vulkan-loader");
    for relative in [
        "usr/lib/x86_64-linux-gnu/libvulkan.so",
        "usr/lib/x86_64-linux-gnu/pkgconfig/vulkan.pc",
    ] {
        copy_path_preserving(&loader.join(relative), &staging.join(relative))?;
    }
    let cmake = "usr/lib/x86_64-linux-gnu/cmake/VulkanLoader";
    copy_tree_preserving(&loader.join(cmake), &staging.join(cmake))?;
    copy_preserving(
        &repo_root.join("src/system/graphics/vulkan-loader/LICENSE.txt"),
        &staging.join("usr/share/doc/libvulkan-dev/copyright"),
    )?;
    Ok(())
}

fn stage_vulkan_tools(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "vulkan-tools",
        &["usr/bin/vulkaninfo", "usr/bin/vkcube"],
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/vulkan-tools/LICENSE.txt"),
        &staging.join("usr/share/doc/vulkan-tools/copyright"),
    )?;
    Ok(())
}

fn stage_library_family(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    names: &[&str],
) -> Result<()> {
    let source = component_install(repo_root, component).join("usr/lib/x86_64-linux-gnu");
    let destination = staging.join("usr/lib/x86_64-linux-gnu");
    for name in names {
        let source_soname = source.join(name);
        let metadata = fs::symlink_metadata(&source_soname)
            .with_context(|| format!("{component} did not install {name}"))?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&source_soname)?;
            if target.is_absolute() || target.components().count() != 1 {
                bail!(
                    "{component} installed unsafe SONAME target {} -> {}",
                    source_soname.display(),
                    target.display()
                );
            }
            copy_preserving(&source.join(&target), &destination.join(&target))?;
            copy_path_preserving(&source_soname, &destination.join(name))?;
        } else {
            copy_preserving(&source_soname, &destination.join(name))?;
        }
    }
    Ok(())
}

/// Install generated XKB rules from the output-owned xkeyboard-config mirror.
/// The Git import contains rules fragments; Meson produces `rules/evdev`.
fn stage_xkeyboard_config_data(repo_root: &Path, staging: &Path) -> Result<()> {
    build_xkeyboard_config(repo_root)?;
    let source = repo_root.join("out/build/xkeyboard-config/install/usr/share");
    copy_tree_preserving(
        &source.join("xkeyboard-config-2"),
        &staging.join("usr/share/xkeyboard-config-2"),
    )?;
    // Meson's installed legacy link is absolute (`/usr/share/...`).  Preserve
    // its in-image meaning rather than copying an absolute host-root link
    // into package staging.
    let legacy_root = staging.join("usr/share/X11/xkb");
    if let Some(parent) = legacy_root.parent() {
        fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("../xkeyboard-config-2", &legacy_root)?;
    #[cfg(not(unix))]
    bail!("xkeyboard-config package staging requires Unix symlinks");
    let rules = staging.join("usr/share/xkeyboard-config-2/rules/evdev");
    if !rules.is_file() {
        bail!(
            "xkeyboard-config staging did not contain generated {}",
            rules.display()
        );
    }
    copy_preserving(
        &repo_root.join("src/system/data/xkeyboard-config/COPYING"),
        &staging.join("usr/share/doc/xkb-data/copyright"),
    )?;
    Ok(())
}

fn stage_imported_soname_library(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    soname: &str,
    license_rel: &str,
    package: &str,
) -> Result<()> {
    let source_dir = component_install(repo_root, component).join("usr/lib/x86_64-linux-gnu");
    let source_soname = source_dir.join(soname);
    let destination_dir = staging.join("usr/lib/x86_64-linux-gnu");
    fs::create_dir_all(&destination_dir)?;
    let metadata = fs::symlink_metadata(&source_soname)
        .with_context(|| format!("{component} did not install {soname}"))?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(&source_soname)?;
        if target.is_absolute() || target.components().count() != 1 {
            bail!(
                "{component} installed unsafe SONAME target {} -> {}",
                source_soname.display(),
                target.display()
            );
        }
        copy_preserving(&source_dir.join(&target), &destination_dir.join(&target))?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, destination_dir.join(soname))?;
        #[cfg(not(unix))]
        copy_preserving(&source_dir.join(&target), &destination_dir.join(soname))?;
    } else {
        copy_preserving(&source_soname, &destination_dir.join(soname))?;
    }
    copy_preserving(
        &repo_root.join(license_rel),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("copyright"),
    )?;
    Ok(())
}

fn stage_terminfo(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = component_install(repo_root, "ncurses").join("usr/share/terminfo");
    for terminal in TERMINFO_ENTRIES {
        let first = terminal.as_bytes()[0];
        let candidates = [
            source.join(char::from(first).to_string()).join(terminal),
            source.join(format!("{first:x}")).join(terminal),
        ];
        let entry = candidates
            .iter()
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| {
                anyhow!(
                    "terminfo entry {terminal} missing from {}",
                    source.display()
                )
            })?;
        let relative = entry.strip_prefix(&source)?;
        copy_preserving(entry, &staging.join("usr/share/terminfo").join(relative))?;
    }
    Ok(())
}

fn stage_procps(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "procps-ng", PROCPS_RUNTIME_PATHS)?;
    copy_preserving(
        &repo_root.join("src/userland/procps-ng/sysctl.conf"),
        &staging.join("etc/sysctl.conf"),
    )?;
    fs::write(staging.join("DEBIAN/conffiles"), "/etc/sysctl.conf\n")?;
    Ok(())
}

fn stage_dbus_broker(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "dbus-broker");
    for rel in ["usr/bin/dbus-broker", "usr/bin/dbus-broker-launch"] {
        copy_path_preserving(&install.join(rel), &staging.join(rel))?;
    }
    let dbus = repo_root.join("src/system/dbus");
    copy_preserving(
        &dbus.join("config/system.conf"),
        &staging.join("etc/dbus-1/system.conf"),
    )?;
    copy_preserving(
        &dbus.join("config/dbus.conf"),
        &staging.join("usr/lib/sysusers.d/dbus.conf"),
    )?;
    copy_tree_preserving(&dbus.join("units"), &staging.join("usr/lib/systemd/system"))?;
    let session = repo_root.join("src/system/session");
    copy_preserving(
        &session.join("dbus/session.conf"),
        &staging.join("usr/share/dbus-1/session.conf"),
    )?;
    copy_tree_preserving(
        &session.join("user-units"),
        &staging.join("usr/lib/systemd/user"),
    )?;
    for rel in [
        "etc/dbus-1/system.d",
        "etc/dbus-1/session.d",
        "usr/share/dbus-1/system-services",
        "usr/share/dbus-1/system.d",
        "usr/share/dbus-1/session.d",
        "usr/share/dbus-1/services",
    ] {
        fs::create_dir_all(staging.join(rel))?;
    }
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/dbus-1/system.conf\n",
    )?;
    Ok(())
}

fn stage_pam_modules(repo_root: &Path, staging: &Path) -> Result<()> {
    let source =
        component_install(repo_root, "linux-pam").join("usr/lib/x86_64-linux-gnu/security");
    let destination = staging.join("usr/lib/x86_64-linux-gnu/security");
    for module in PAM_MODULES {
        copy_preserving(&source.join(module), &destination.join(module))?;
    }
    Ok(())
}

fn stage_pam_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    copy_path_preserving(
        &component_install(repo_root, "linux-pam").join("usr/sbin/unix_chkpwd"),
        &staging.join("usr/sbin/unix_chkpwd"),
    )?;
    let pam_policy = repo_root.join("src/system/auth/config/pam.d");
    copy_tree_preserving(&pam_policy, &staging.join("etc/pam.d"))?;
    // Linux-PAM is built with its upstream vendor configuration directory at
    // /usr/share/pam.  Ship the source-built pam_env defaults there so PAM
    // does not warn on every login while still allowing /etc/security to
    // override them locally.
    copy_preserving(
        &component_install(repo_root, "linux-pam").join("usr/share/pam/security/pam_env.conf"),
        &staging.join("usr/share/pam/security/pam_env.conf"),
    )?;
    let mut conffiles = fs::read_dir(&pam_policy)?
        .map(|entry| {
            Ok(format!(
                "/etc/pam.d/{}",
                entry?.file_name().to_string_lossy()
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    conffiles.sort();
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", conffiles.join("\n")),
    )?;
    Ok(())
}

fn stage_shadow(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "shadow", SHADOW_RUNTIME_PATHS)?;
    let config = repo_root.join("src/system/auth/config");
    copy_preserving(&config.join("login.defs"), &staging.join("etc/login.defs"))?;
    copy_preserving(
        &config.join("default/useradd"),
        &staging.join("etc/default/useradd"),
    )?;
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/login.defs\n/etc/default/useradd\n",
    )?;
    for rel in ["usr/bin/passwd", "usr/bin/newgrp"] {
        set_mode(staging.join(rel), 0o4755)?;
    }
    validate_no_mutable_system_state(staging)
}

fn stage_sudo_rs(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "sudo-rs",
        &["usr/bin/sudo", "usr/bin/visudo"],
    )?;
    let config = repo_root.join("src/system/auth/config");
    copy_preserving(&config.join("sudoers"), &staging.join("etc/sudoers"))?;
    copy_preserving(
        &config.join("sudoers.d/README"),
        &staging.join("etc/sudoers.d/README"),
    )?;
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/sudoers\n/etc/sudoers.d/README\n",
    )?;
    set_mode(staging.join("usr/bin/sudo"), 0o4755)?;
    set_mode(staging.join("etc/sudoers"), 0o440)?;
    set_mode(staging.join("etc/sudoers.d"), 0o750)?;
    set_mode(staging.join("etc/sudoers.d/README"), 0o440)?;
    Ok(())
}

fn stage_util_linux_auth(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "util-linux", UTIL_LINUX_AUTH_PATHS)?;
    for rel in ["usr/bin/login", "usr/bin/su"] {
        set_mode(staging.join(rel), 0o4755)?;
    }
    Ok(())
}

fn stage_openssh_server(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "openssh", OPENSSH_SERVER_RUNTIME_PATHS)?;
    let config = repo_root.join("src/system/network/openssh");
    copy_preserving(
        &config.join("sshd_config"),
        &staging.join("etc/ssh/sshd_config"),
    )?;
    copy_preserving(&config.join("ssh-pam"), &staging.join("etc/pam.d/sshd"))?;
    copy_preserving(
        &config.join("ssh.service"),
        &staging.join("usr/lib/systemd/system/ssh.service"),
    )?;
    copy_preserving(
        &config.join("openssh-sysusers.conf"),
        &staging.join("usr/lib/sysusers.d/openssh.conf"),
    )?;
    fs::create_dir_all(staging.join("etc/ssh/sshd_config.d"))?;
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/ssh/sshd_config\n/etc/pam.d/sshd\n",
    )?;
    Ok(())
}

fn stage_iproute2(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "iproute2", IPROUTE2_RUNTIME_PATHS)?;
    copy_tree_preserving(
        &component_install(repo_root, "iproute2").join("usr/share/iproute2"),
        &staging.join("usr/share/iproute2"),
    )
}

fn stage_linux_libc_dev(repo_root: &Path, staging: &Path) -> Result<()> {
    let glibc_headers = repo_root.join("out/build/glibc/install/usr/include");
    copy_tree_filtered(
        &repo_root.join("out/build/glibc/linux-headers/usr/include"),
        &staging.join("usr/include"),
        &|relative, _| !path_entry_exists(&glibc_headers.join(relative)),
    )?;
    copy_preserving(
        &repo_root.join("src/kernel/linux/COPYING"),
        &staging.join("usr/share/doc/linux-libc-dev/copyright"),
    )?;
    copy_preserving(
        &repo_root.join("out/build/glibc/linux-headers-inventory.txt"),
        &staging.join("usr/share/doc/linux-libc-dev/generated-files.txt"),
    )
}

fn stage_linux_modules(repo_root: &Path, staging: &Path) -> Result<()> {
    let release = fs::read_to_string(repo_root.join("out/build/linux/kernel-release"))?;
    let release = release.trim();
    if release != "7.2.0-rc5-mattos" {
        bail!("kernel module package name does not match built release {release}");
    }
    let source = repo_root
        .join("out/build/linux/modules/usr/lib/modules")
        .join(release);
    copy_tree_preserving(&source, &staging.join("usr/lib/modules").join(release))?;
    copy_preserving(
        &repo_root.join("src/kernel/linux/COPYING"),
        &staging.join(format!("usr/share/doc/linux-modules-{release}/copyright")),
    )
}

fn stage_glibc_development(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/glibc/install/usr");
    copy_tree_preserving(&install.join("include"), &staging.join("usr/include"))?;
    copy_tree_filtered(
        &install.join("lib/x86_64-linux-gnu"),
        &staging.join("usr/lib/x86_64-linux-gnu"),
        &|relative, _| {
            relative
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| {
                    name.ends_with(".a") || name.ends_with(".o") || name.ends_with(".so")
                })
        },
    )?;
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/COPYING.LIB"),
        &staging.join("usr/share/doc/libc6-dev/copyright"),
    )
}

fn stage_gcc_development(repo_root: &Path, staging: &Path, cxx: bool) -> Result<()> {
    let install = repo_root.join("out/build/gcc-runtime/install/usr");
    if cxx {
        copy_tree_preserving(
            &install.join("include/c++"),
            &staging.join("usr/include/c++"),
        )?;
        let source = install.join("lib/lib64");
        let destination = staging.join("usr/lib/x86_64-linux-gnu");
        for name in ["libstdc++.a", "libsupc++.a"] {
            copy_preserving(&source.join(name), &destination.join(name))?;
        }
        fs::create_dir_all(&destination)?;
        std::os::unix::fs::symlink("libstdc++.so.6", destination.join("libstdc++.so"))?;
        copy_preserving(
            &repo_root.join("src/toolchain/gcc/COPYING3"),
            &staging.join("usr/share/doc/mattos-libstdc++-dev/copyright"),
        )?;
        copy_preserving(
            &repo_root.join("src/toolchain/gcc/COPYING.RUNTIME"),
            &staging.join("usr/share/doc/mattos-libstdc++-dev/copyright.RUNTIME"),
        )?;
    } else {
        copy_tree_preserving(
            &install.join("lib/x86_64-linux-gnu/gcc"),
            &staging.join("usr/lib/x86_64-linux-gnu/gcc"),
        )?;
        let destination = staging.join("usr/lib/x86_64-linux-gnu");
        fs::create_dir_all(&destination)?;
        std::os::unix::fs::symlink("libgcc_s.so.1", destination.join("libgcc_s.so"))?;
        copy_preserving(
            &repo_root.join("src/toolchain/gcc/COPYING.RUNTIME"),
            &staging.join("usr/share/doc/mattos-libgcc-dev/copyright"),
        )?;
    }
    Ok(())
}

fn stage_native_binutils(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("out/build/binutils/install/usr/bin");
    for name in [
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
    ] {
        copy_preserving(&source.join(name), &staging.join("usr/bin").join(name))?;
    }
    copy_preserving(
        &repo_root.join("src/toolchain/binutils/COPYING3"),
        &staging.join("usr/share/doc/binutils/copyright"),
    )
}

fn stage_native_gcc_common(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/gcc-toolchain/install/usr");
    copy_tree_preserving(
        &install.join("libexec/gcc"),
        &staging.join("usr/libexec/gcc"),
    )?;
    if install.join("lib/gcc").is_dir() {
        copy_tree_preserving(&install.join("lib/gcc"), &staging.join("usr/lib/gcc"))?;
    }
    let multiarch_gcc = install.join("lib/x86_64-linux-gnu/gcc");
    if multiarch_gcc.is_dir() {
        let runtime_development =
            repo_root.join("out/build/gcc-runtime/install/usr/lib/x86_64-linux-gnu/gcc");
        copy_tree_filtered(
            &multiarch_gcc,
            &staging.join("usr/lib/x86_64-linux-gnu/gcc"),
            &|relative, _| !path_entry_exists(&runtime_development.join(relative)),
        )?;
    }
    copy_preserving(
        &repo_root.join("src/toolchain/gcc/COPYING3"),
        &staging.join("usr/share/doc/mattos-gcc-common/copyright"),
    )
}

fn stage_native_compiler_driver(repo_root: &Path, staging: &Path, driver: &str) -> Result<()> {
    let source = repo_root.join("out/build/gcc-toolchain/install/usr/bin");
    copy_preserving(&source.join(driver), &staging.join("usr/bin").join(driver))?;
    match driver {
        "gcc" => std::os::unix::fs::symlink("gcc", staging.join("usr/bin/cc"))?,
        "g++" => std::os::unix::fs::symlink("g++", staging.join("usr/bin/c++"))?,
        _ => {}
    }
    copy_preserving(
        &repo_root.join("src/toolchain/gcc/COPYING3"),
        &staging.join("usr/share/doc").join(driver).join("copyright"),
    )
}

fn stage_native_make(repo_root: &Path, staging: &Path) -> Result<()> {
    copy_preserving(
        &repo_root.join("out/build/make/install/usr/bin/make"),
        &staging.join("usr/bin/make"),
    )?;
    copy_preserving(
        &repo_root.join("src/build-tools/make/COPYING"),
        &staging.join("usr/share/doc/make/copyright"),
    )
}

fn copy_tree_filtered(
    source: &Path,
    destination: &Path,
    include: &dyn Fn(&Path, &fs::Metadata) -> bool,
) -> Result<()> {
    fn recurse(
        root: &Path,
        current: &Path,
        destination: &Path,
        include: &dyn Fn(&Path, &fs::Metadata) -> bool,
    ) -> Result<()> {
        let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let source = entry.path();
            let relative = source.strip_prefix(root)?;
            let metadata = fs::symlink_metadata(&source)?;
            if metadata.is_dir() {
                recurse(root, &source, destination, include)?;
            } else if include(relative, &metadata) {
                copy_path_preserving(&source, &destination.join(relative))?;
            }
        }
        Ok(())
    }
    if !source.is_dir() {
        bail!(
            "required package input directory missing at {}",
            source.display()
        )
    }
    recurse(source, source, destination, include)
}

fn validate_no_mutable_system_state(staging: &Path) -> Result<()> {
    for forbidden in [
        "etc/passwd",
        "etc/group",
        "etc/shadow",
        "etc/gshadow",
        "etc/machine-id",
        "run",
        "var/log",
        "var/lib/systemd/random-seed",
        "var/lib/dhcp",
    ] {
        if path_entry_exists(&staging.join(forbidden)) {
            bail!("mutable system state must not be packaged: /{forbidden}")
        }
    }
    Ok(())
}

fn validate_no_embedded_build_root(repo_root: &Path, staging: &Path) -> Result<()> {
    let needle = repo_root.to_string_lossy();
    walk_tree(staging, &mut |path, metadata| {
        if metadata.is_file() && !path.starts_with(staging.join("DEBIAN")) {
            let bytes = fs::read(path)?;
            if bytes
                .windows(needle.len())
                .any(|window| window == needle.as_bytes())
            {
                bail!(
                    "package payload /{} embeds the host build root",
                    path.strip_prefix(staging)?.display()
                )
            }
        }
        Ok(())
    })
}

fn strip_staged_debug(repo_root: &Path, staging: &Path) -> Result<()> {
    let strip = repo_root.join("out/build/binutils/cross-install/usr/bin/strip");
    if !strip.is_file() {
        bail!(
            "source-built Binutils strip is required before package staging at {}",
            strip.display()
        )
    }
    let mut objects = Vec::new();
    #[cfg(unix)]
    let mut object_inodes = BTreeSet::new();
    walk_tree(staging, &mut |path, metadata| {
        if metadata.is_file() && !path.starts_with(staging.join("DEBIAN")) {
            let header = Command::new("readelf").args(["-h"]).arg(path).output()?;
            if header.status.success() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if !object_inodes.insert((metadata.dev(), metadata.ino())) {
                        return Ok(());
                    }
                }
                objects.push(path.to_path_buf());
            }
        }
        Ok(())
    })?;
    for object in objects {
        let status = Command::new(&strip)
            .arg("--strip-debug")
            .arg(&object)
            .status()?;
        if !status.success() {
            bail!("source-built strip failed for {}", object.display())
        }
    }
    Ok(())
}

fn copy_tree_preserving(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(unix)]
    let mut hardlinks = BTreeMap::new();
    copy_tree_preserving_inner(source, destination, &mut hardlinks)
}

#[cfg(unix)]
type HardlinkMap = BTreeMap<(u64, u64), PathBuf>;

#[cfg(not(unix))]
type HardlinkMap = ();

fn copy_tree_preserving_inner(
    source: &Path,
    destination: &Path,
    hardlinks: &mut HardlinkMap,
) -> Result<()> {
    if !source.is_dir() {
        bail!(
            "required package input directory missing at {}",
            source.display()
        )
    }
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&from)?;
        if metadata.is_dir() {
            copy_tree_preserving_inner(&from, &to, hardlinks)?;
        } else {
            copy_path_preserving_with_hardlinks(&from, &to, &metadata, hardlinks)?;
        }
    }
    Ok(())
}

fn copy_path_preserving_with_hardlinks(
    source: &Path,
    destination: &Path,
    metadata: &fs::Metadata,
    hardlinks: &mut HardlinkMap,
) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.is_file() && metadata.nlink() > 1 {
            let identity = (metadata.dev(), metadata.ino());
            if let Some(first_destination) = hardlinks.get(&identity) {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::hard_link(first_destination, destination).with_context(|| {
                    format!(
                        "failed to preserve hardlink {} -> {}",
                        destination.display(),
                        first_destination.display()
                    )
                })?;
                return Ok(());
            }
            copy_path_preserving(source, destination)?;
            hardlinks.insert(identity, destination.to_path_buf());
            return Ok(());
        }
    }
    copy_path_preserving(source, destination)
}

fn copy_path_preserving(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("required package input missing at {}", source.display()))?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    if metadata.file_type().is_symlink() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(fs::read_link(source)?, destination)?;
        #[cfg(not(unix))]
        bail!("package symlink staging requires Unix")
    } else {
        copy_preserving(source, destination)?;
    }
    Ok(())
}

fn validate_no_mutable_package_state(staging: &Path) -> Result<()> {
    for forbidden in [
        "var/lib/dpkg/status",
        "var/lib/dpkg/available",
        "var/lib/dpkg/lock",
        "var/lib/dpkg/lock-frontend",
        "var/lib/apt/lists/lock",
        "var/cache/apt/archives/lock",
    ] {
        if path_entry_exists(&staging.join(forbidden)) {
            bail!("mutable package-manager state must not be packaged: /{forbidden}")
        }
    }
    Ok(())
}

#[cfg(test)]
fn validate_migrated_bootstrap_absent(manifest: &[String]) -> Result<()> {
    for row in manifest {
        let destination = row.split('\t').next();
        if destination == Some("/usr/bin/tar") {
            bail!("migrated GNU tar remains in mattos-bootstrap-runtime")
        }
        if let Some(name) = destination
            .and_then(|path| Path::new(path).file_name())
            .and_then(OsStr::to_str)
        {
            if MIGRATED_BOOTSTRAP_SONAME_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                bail!("migrated runtime library {name} remains in mattos-bootstrap-runtime")
            }
        }
    }
    Ok(())
}

fn command_text(program: &str, args: &[&str], path: &Path) -> Result<Option<String>> {
    let output = Command::new(program)
        .args(args)
        .arg(path)
        .output()
        .with_context(|| format!("failed to run {program} on {}", path.display()))?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8(output.stdout)?))
}

fn dynamic_value(text: &str, label: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let marker = format!("{label}: [");
        let (_, value) = line.split_once(&marker)?;
        Some(value.trim_end_matches(']').to_string())
    })
}

#[cfg(test)]
fn dynamic_values(text: &str, label: &str) -> Vec<String> {
    let marker = format!("{label}: [");
    text.lines()
        .filter_map(|line| {
            line.split_once(&marker)
                .map(|(_, value)| value.trim_end_matches(']').to_string())
        })
        .collect()
}

#[cfg(test)]
fn bootstrap_source_attribution(
    name: &str,
) -> (
    Option<&'static str>,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match name {
        "tar" => (Some("GNU tar"), "A", "tar", "medium", "high"),
        "libattr.so.1" => (
            Some("Linux extended attributes"),
            "A",
            "libattr1",
            "low",
            "high",
        ),
        "libacl.so.1" => (Some("Linux ACL utilities"), "A", "libacl1", "low", "high"),
        "libbsd.so.0" => (Some("libbsd"), "A", "libbsd0", "low", "high"),
        "libbz2.so.1.0" => (Some("bzip2"), "A", "libbz2-1.0", "low", "high"),
        "libc.so.6" | "libm.so.6" | "ld-linux-x86-64.so.2" => (
            Some("glibc"),
            "D",
            "future MattOS libc runtime",
            "very-high",
            "high",
        ),
        "libcap.so.2" => (Some("libcap"), "A", "libcap2", "low", "high"),
        "libcrypt.so.1" => (Some("libxcrypt"), "A", "libcrypt1", "medium", "high"),
        "libcrypto.so.3" => (Some("OpenSSL"), "A", "mattos-libcrypto3", "high", "high"),
        "libssl.so.3" => (Some("OpenSSL"), "A", "libssl3t64", "high", "high"),
        "libelf.so.1" => (Some("elfutils"), "A", "libelf1t64", "medium", "high"),
        "libexpat.so.1" => (Some("Expat"), "A", "libexpat1", "low", "high"),
        "libgcc_s.so.1" => (
            Some("GCC runtime"),
            "D",
            "future MattOS compiler runtime",
            "very-high",
            "high",
        ),
        "liblz4.so.1" => (Some("LZ4"), "C", "liblz4-1", "low", "high"),
        "liblzma.so.5" => (Some("XZ Utils"), "C", "liblzma5", "low", "high"),
        "libmd.so.0" => (Some("libmd"), "A", "libmd0", "low", "high"),
        "libpcre2-8.so.0" => (Some("PCRE2"), "A", "libpcre2-8-0", "medium", "high"),
        "libselinux.so.1" => (
            Some("SELinux userspace"),
            "A",
            "libselinux1",
            "high",
            "high",
        ),
        "libstdc++.so.6" => (
            Some("GCC libstdc++ runtime"),
            "D",
            "future MattOS C++ runtime",
            "very-high",
            "high",
        ),
        "libxxhash.so.0" => (Some("xxHash"), "C", "libxxhash0", "low", "high"),
        "libz.so.1" => (Some("zlib"), "A", "zlib1g", "low", "high"),
        "libzstd.so.1" => (Some("Zstandard"), "A", "libzstd1", "low", "high"),
        _ => (None, "E", "unresolved", "unknown", "low"),
    }
}

#[cfg(test)]
fn bootstrap_consumers(repo_root: &Path) -> Result<BTreeMap<String, Vec<BootstrapConsumer>>> {
    let staging_root = repo_root.join("out/packages/staging");
    let mut graph = BTreeMap::<String, Vec<BootstrapConsumer>>::new();
    if !staging_root.is_dir() {
        bail!(
            "package staging tree missing at {}; build packages first",
            staging_root.display()
        );
    }
    let mut package_dirs = fs::read_dir(&staging_root)?.collect::<std::io::Result<Vec<_>>>()?;
    package_dirs.sort_by_key(|entry| entry.file_name());
    for package_entry in package_dirs {
        let package_root = package_entry.path();
        if !package_root.is_dir() {
            continue;
        }
        let package = package_entry.file_name().to_string_lossy().to_string();
        walk_tree(&package_root, &mut |path, metadata| {
            if !metadata.is_file() || path.starts_with(package_root.join("DEBIAN")) {
                return Ok(());
            }
            let Some(dynamic) = command_text("readelf", &["-d"], path)? else {
                return Ok(());
            };
            let consumer_path = format!("/{}", path.strip_prefix(&package_root)?.display());
            for needed in dynamic_values(&dynamic, "Shared library") {
                graph.entry(needed).or_default().push(BootstrapConsumer {
                    package: package.clone(),
                    path: consumer_path.clone(),
                });
            }
            Ok(())
        })?;
    }
    for consumers in graph.values_mut() {
        consumers.sort_by(|a, b| (&a.package, &a.path).cmp(&(&b.package, &b.path)));
        consumers.dedup_by(|a, b| a.package == b.package && a.path == b.path);
    }
    Ok(graph)
}

#[cfg(test)]
fn confirmed_host_package(source: &Path) -> Result<Option<String>> {
    let canonical = fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let output = Command::new("dpkg-query")
        .args(["-S"])
        .arg(&canonical)
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(String::from_utf8(output.stdout)?
        .lines()
        .next()
        .and_then(|line| {
            line.split_once(": ")
                .map(|(package, _)| package.to_string())
        }))
}

fn generate_bootstrap_audit(repo_root: &Path) -> Result<()> {
    let report = BootstrapAuditReport {
        schema_version: 1,
        package: "retired".to_string(),
        snapshot: "runtime-source-closure-complete".to_string(),
        entry_count: 0,
        payload_bytes: 0,
        classification_totals: BTreeMap::new(),
        entries: Vec::new(),
    };
    let reports = repo_root.join("out/reports");
    fs::create_dir_all(&reports)?;
    let destination = reports.join("bootstrap-runtime-audit.toml");
    fs::write(&destination, toml::to_string_pretty(&report)?)?;
    println!(
        "generated zero-entry retired bootstrap audit at {}",
        destination.display()
    );
    Ok(())
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

fn stage_coreutils(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = resolve_coreutils_multicall(repo_root)?;
    stage_executable(&source, &staging.join("usr/bin/coreutils"), 0o755)?;
    let applets = package_coreutils_applets(&source)?;
    #[cfg(unix)]
    for applet in applets {
        let path = staging.join("usr/bin").join(&applet);
        if path_entry_exists(&path) {
            bail!("duplicate coreutils command alias {applet}")
        }
        std::os::unix::fs::symlink("coreutils", path)?;
    }
    Ok(())
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

fn stage_executable(source: &Path, destination: &Path, mode: u32) -> Result<()> {
    if !source.is_file() {
        bail!("required package input missing at {}", source.display())
    }
    copy_preserving(source, destination)?;
    set_mode(destination.to_path_buf(), mode)
}

fn copy_preserving(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(source)?.permissions().mode();
        fs::set_permissions(destination, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
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
        "gpgv" => component_snapshot_version(repo_root, "gnupg")?,
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
        "libglvnd0" | "libopengl0" | "libegl1" | "libgles1" | "libgles2" => {
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
        "cosmic-edit" => cargo_package_version(
            &repo_root.join("src/desktop/cosmic/cosmic-edit/Cargo.toml"),
        )?,
        "cosmic-initial-setup" => cargo_package_version(
            &repo_root.join("src/desktop/cosmic/cosmic-initial-setup/Cargo.toml"),
        )?,
        "libduktape207" => component_snapshot_version(repo_root, "duktape")?,
        "polkit" => component_snapshot_version(repo_root, "polkit")?,
        "network-manager" => component_snapshot_version(repo_root, "networkmanager")?,
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
        | "libgpg-error" | "libgcrypt" | "libassuan" | "libksba" | "npth" | "gnupg"
        | "less" | "git" | "openssh" | "libffi" | "wayland" | "xkbcommon"
        | "libglvnd" | "xkeyboard-config" | "cpython" | "llvm" | "rust") => {
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

fn runtime_libraries_for_spec(repo_root: &Path, spec: &PackageSpec) -> Result<Vec<String>> {
    match spec.name {
        "mattos-brush" => ldd_sonames(
            &repo_root.join("out/build/brush/cargo-target/release/brush"),
            None,
        ),
        "coreutils" => ldd_sonames(&resolve_coreutils_multicall(repo_root)?, None),
        "curl" => {
            let install = repo_root.join("out/build/curl/install");
            let openssl = repo_root.join("out/build/openssl/install/usr/lib/x86_64-linux-gnu");
            let zlib = repo_root.join("out/build/zlib/install/usr/lib/x86_64-linux-gnu");
            let zstd = repo_root.join("out/build/zstd/install/usr/lib/x86_64-linux-gnu");
            ldd_sonames_many(
                &[
                    install.join("usr/bin/curl"),
                    install.join("usr/lib/x86_64-linux-gnu/libcurl.so.4.8.0"),
                ],
                &[
                    install.join("usr/lib/x86_64-linux-gnu"),
                    openssl,
                    zlib,
                    zstd,
                ],
            )
        }
        name if matches!(
            name,
            "libc6"
                | "libc-bin"
                | "libgcc-s1"
                | "libstdc++6"
                | "binutils"
                | "mattos-gcc-common"
                | "cpp"
                | "gcc"
                | "g++"
                | "make"
        ) =>
        {
            runtime_libraries_in_staging(repo_root, name)
        }
        "dpkg" => {
            let install = repo_root.join("out/build/dpkg/install");
            let zlib = repo_root.join("out/build/zlib/install/usr/lib/x86_64-linux-gnu");
            let bzip2 = repo_root.join("out/build/bzip2/install/usr/lib/x86_64-linux-gnu");
            let xz = repo_root.join("out/build/xz/install/usr/lib/x86_64-linux-gnu");
            let zstd = repo_root.join("out/build/zstd/install/usr/lib/x86_64-linux-gnu");
            let libmd = repo_root.join("out/build/libmd/install/usr/lib/x86_64-linux-gnu");
            let selinux = repo_root.join("out/build/selinux/install/usr/lib/x86_64-linux-gnu");
            let pcre2 = repo_root.join("out/build/pcre2/install/usr/lib/x86_64-linux-gnu");
            ldd_sonames_many(
                &[
                    install.join("usr/bin/dpkg"),
                    install.join("usr/bin/dpkg-deb"),
                    install.join("usr/bin/dpkg-query"),
                    install.join("usr/bin/dpkg-divert"),
                    install.join("usr/bin/dpkg-realpath"),
                    install.join("usr/bin/dpkg-split"),
                    install.join("usr/bin/dpkg-statoverride"),
                    install.join("usr/bin/dpkg-trigger"),
                    install.join("usr/bin/update-alternatives"),
                    install.join("usr/sbin/start-stop-daemon"),
                ],
                &[zlib, bzip2, xz, zstd, libmd, selinux, pcre2],
            )
        }
        "libapt-pkg7.0" => {
            let install = repo_root.join("out/build/apt/install");
            let systemd = repo_root.join("out/build/systemd/install/usr/lib/x86_64-linux-gnu");
            let zlib = repo_root.join("out/build/zlib/install/usr/lib/x86_64-linux-gnu");
            let bzip2 = repo_root.join("out/build/bzip2/install/usr/lib/x86_64-linux-gnu");
            let lz4 = repo_root.join("out/build/lz4/install/usr/lib/x86_64-linux-gnu");
            let xz = repo_root.join("out/build/xz/install/usr/lib/x86_64-linux-gnu");
            let xxhash = repo_root.join("out/build/xxhash/install/usr/lib/x86_64-linux-gnu");
            let zstd = repo_root.join("out/build/zstd/install/usr/lib/x86_64-linux-gnu");
            let openssl = repo_root.join("out/build/openssl/install/usr/lib/x86_64-linux-gnu");
            ldd_sonames_many(
                &[install.join("usr/lib/x86_64-linux-gnu/libapt-pkg.so.7.0.0")],
                &[
                    install.join("usr/lib/x86_64-linux-gnu"),
                    systemd,
                    zlib,
                    bzip2,
                    lz4,
                    xz,
                    xxhash,
                    zstd,
                    openssl,
                ],
            )
        }
        "apt" => {
            let install = repo_root.join("out/build/apt/install");
            let systemd = repo_root.join("out/build/systemd/install/usr/lib/x86_64-linux-gnu");
            let zlib = repo_root.join("out/build/zlib/install/usr/lib/x86_64-linux-gnu");
            let bzip2 = repo_root.join("out/build/bzip2/install/usr/lib/x86_64-linux-gnu");
            let lz4 = repo_root.join("out/build/lz4/install/usr/lib/x86_64-linux-gnu");
            let xz = repo_root.join("out/build/xz/install/usr/lib/x86_64-linux-gnu");
            let xxhash = repo_root.join("out/build/xxhash/install/usr/lib/x86_64-linux-gnu");
            let zstd = repo_root.join("out/build/zstd/install/usr/lib/x86_64-linux-gnu");
            let openssl = repo_root.join("out/build/openssl/install/usr/lib/x86_64-linux-gnu");
            ldd_sonames_many(
                &[
                    install.join("usr/bin/apt"),
                    install.join("usr/bin/apt-cache"),
                    install.join("usr/bin/apt-config"),
                    install.join("usr/bin/apt-get"),
                    install.join("usr/bin/apt-mark"),
                    install.join("usr/lib/apt/apt-helper"),
                    install.join("usr/lib/apt/methods/copy"),
                    install.join("usr/lib/apt/methods/file"),
                    install.join("usr/lib/apt/methods/http"),
                    install.join("usr/lib/apt/methods/https"),
                    install.join("usr/lib/apt/methods/store"),
                    install.join("usr/lib/x86_64-linux-gnu/libapt-private.so.0.0.0"),
                ],
                &[
                    install.join("usr/lib/x86_64-linux-gnu"),
                    systemd,
                    zlib,
                    bzip2,
                    lz4,
                    xz,
                    xxhash,
                    zstd,
                    openssl,
                ],
            )
        }
        "libgpg-error0" | "libgcrypt20" | "libassuan9" | "libksba8" | "libnpth0" => {
            let component = match spec.name {
                "libgpg-error0" => "libgpg-error",
                "libgcrypt20" => "libgcrypt",
                "libassuan9" => "libassuan",
                "libksba8" => "libksba",
                "libnpth0" => "npth",
                _ => unreachable!(),
            };
            let install = repo_root
                .join("out/build")
                .join(component)
                .join("install");
            let libdir = install.join("usr/lib/x86_64-linux-gnu");
            let mut search = vec![libdir.clone()];
            if component != "libgpg-error" && component != "npth" {
                search.push(
                    repo_root
                        .join("out/build/libgpg-error/install/usr/lib/x86_64-linux-gnu"),
                );
            }
            ldd_sonames_many(&[libdir.join(match spec.name {
                "libgpg-error0" => "libgpg-error.so.0",
                "libgcrypt20" => "libgcrypt.so.20",
                "libassuan9" => "libassuan.so.9",
                "libksba8" => "libksba.so.8",
                "libnpth0" => "libnpth.so.0",
                _ => unreachable!(),
            })], &search)
        }
        "gpgv" => {
            let install = repo_root.join("out/build/gpgv/install");
            let mut search = vec![install.join("usr/lib/x86_64-linux-gnu")];
            for component in [
                "libgpg-error",
                "libgcrypt",
                "libassuan",
                "libksba",
                "npth",
                "zlib",
            ] {
                search.push(
                    repo_root
                        .join("out/build")
                        .join(component)
                        .join("install/usr/lib/x86_64-linux-gnu"),
                );
            }
            ldd_sonames_many(&[install.join("usr/bin/gpgv")], &search)
        }
        name if matches!(
            name,
            "mattos-libtinfow6"
                | "libncursesw6"
                | "ncurses-bin"
                | "libkmod2"
                | "kmod"
                | "mattos-libproc2"
                | "procps"
                | "libsystemd0"
                | "libudev1"
                | "libexpat1"
                | "libcap2"
                | "libattr1"
                | "libacl1"
                | "zlib1g"
                | "libbz2-1.0"
                | "liblz4-1"
                | "liblzma5"
                | "libxxhash0"
                | "libmd0"
                | "libbsd0"
                | "libzstd1"
                | "mattos-libcrypto3"
                | "libssl3t64"
                | "libelf1t64"
                | "libpcre2-8-0"
                | "libselinux1"
                | "libcrypt1"
                | "libblkid1"
                | "libmount1"
                | "libsmartcols1"
                | "libuuid1"
                | "libfdisk1"
                | "mount"
                | "util-linux"
                | "gzip"
                | "bzip2"
                | "xz-utils"
                | "zstd"
                | "patch"
                | "libmagic1"
                | "file"
                | "less"
                | "git"
                | "openssh-client"
                | "openssh-server"
                | "libffi8"
                | "libffi-dev"
                | "libwayland-client0"
                | "libwayland-server0"
                | "libwayland-egl1"
                | "libxkbcommon0"
                | "libvulkan1"
                | "libvulkan-dev"
                | "vulkan-tools"
                | "libxau6"
                | "libxdmcp6"
                | "libxcb1"
                | "libx11-6"
                | "libxext6"
                | "libglvnd0"
                | "libopengl0"
                | "libegl1"
                | "libgles1"
                | "libgles2"
                | "libegl-mesa0"
                | "libnvidia-gl-595"
                | "libnvidia-compute-595"
                | "libnvidia-encode-595"
                | "libnvidia-decode-595"
                | "nvidia-utils-595"
                | "libpython3.14"
                | "python3"
                | "python3-venv"
                | "python3-dev"
                | "libllvm22"
                | "llvm"
                | "llvm-dev"
                | "clang"
                | "lld"
                | "rustc"
                | "cargo"
                | "tar"
                | "dbus-broker"
                | "libpam0g"
                | "mattos-libpam-misc0"
                | "libpam-modules"
                | "libpam-runtime"
                | "passwd"
                | "mattos-sudo-rs"
                | "login"
                | "iproute2"
                | "iputils-ping"
                | "btrfs-progs"
                | "dosfstools"
                | "e2fsprogs"
                | "mattos-installer"
        ) =>
        {
            runtime_libraries_in_staging(repo_root, name)
        }
        _ => Ok(Vec::new()),
    }
}

fn runtime_libraries_in_staging(repo_root: &Path, package: &str) -> Result<Vec<String>> {
    let staging = repo_root.join("out/packages/staging").join(package);
    let mut binaries = Vec::new();
    walk_tree(&staging, &mut |path, metadata| {
        if metadata.is_file() && !path.starts_with(staging.join("DEBIAN")) {
            let output = Command::new("readelf").args(["-h"]).arg(path).output()?;
            let header = String::from_utf8_lossy(&output.stdout);
            if output.status.success()
                && header.lines().any(|line| {
                    let mut fields = line.split_whitespace();
                    fields.next() == Some("Type:") && matches!(fields.next(), Some("DYN" | "EXEC"))
                })
            {
                binaries.push(path.to_path_buf());
            }
        }
        Ok(())
    })?;
    let library_dirs = [
        staging.join("usr/lib/x86_64-linux-gnu"),
        repo_root.join("out/sysroot/usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "apt").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "curl").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "libffi").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "wayland").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "mesa").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "vulkan-loader").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "xkbcommon").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "cpython").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "llvm").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "ncurses").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "kmod").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "procps-ng").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "linux-pam").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "systemd").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "lz4").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "xz").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "xxhash").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "zstd").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "openssl").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "elfutils").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "libmd").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "libbsd").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "pcre2").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "selinux").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "libxcrypt").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "util-linux").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "zlib").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "bzip2").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "file").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "git").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "openssh").join("usr/lib/x86_64-linux-gnu"),
    ];
    ldd_sonames_many(&binaries, &library_dirs)
}

fn ldd_sonames_many(binaries: &[PathBuf], library_dirs: &[PathBuf]) -> Result<Vec<String>> {
    let library_path = if library_dirs.is_empty() {
        None
    } else {
        Some(std::env::join_paths(library_dirs)?)
    };
    let mut libraries = BTreeSet::new();
    for binary in binaries {
        let mut command = Command::new("ldd");
        command.arg(binary);
        if let Some(path) = &library_path {
            command.env("LD_LIBRARY_PATH", path);
        }
        let output = command
            .output()
            .with_context(|| format!("failed to inspect {} with ldd", binary.display()))?;
        let text = String::from_utf8(output.stdout)?;
        if !output.status.success() || text.contains("not found") {
            bail!(
                "unresolved ELF dependency for {}:\n{text}",
                binary.display()
            )
        }
        for line in text.lines() {
            let token = line.trim().split_whitespace().next().unwrap_or_default();
            if token.contains(".so") {
                libraries.insert(token.to_string());
            }
        }
    }
    Ok(libraries.into_iter().collect())
}

fn ldd_sonames(binary: &Path, library_path: Option<&Path>) -> Result<Vec<String>> {
    let mut command = Command::new("ldd");
    command.arg(binary);
    if let Some(library_path) = library_path {
        command.env("LD_LIBRARY_PATH", library_path);
    }
    let output = command.output().with_context(|| {
        format!(
            "failed to inspect runtime libraries for {}",
            binary.display()
        )
    })?;
    if !output.status.success() {
        bail!("ldd failed for {}", binary.display());
    }
    let mut libraries = BTreeSet::new();
    for line in String::from_utf8(output.stdout)?.lines() {
        let token = line.trim().split_whitespace().next().unwrap_or_default();
        if token.contains(".so") {
            libraries.insert(token.to_string());
        }
    }
    Ok(libraries.into_iter().collect())
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

fn detect_staging_collisions(staging_root: &Path, specs: &[PackageSpec]) -> Result<()> {
    let mut owners: BTreeMap<PathBuf, (&str, bool)> = BTreeMap::new();
    for spec in specs {
        let root = staging_root.join(spec.name);
        walk_tree(&root, &mut |path, meta| {
            if path.starts_with(root.join("DEBIAN")) {
                return Ok(());
            }
            let rel = path.strip_prefix(&root)?.to_path_buf();
            let is_dir = meta.is_dir();
            if let Some((owner, owner_is_dir)) = owners.get(&rel) {
                if !is_dir || !owner_is_dir {
                    bail!(
                        "package ownership collision at /{}: {} and {}",
                        rel.display(),
                        owner,
                        spec.name
                    )
                }
            } else {
                owners.insert(rel, (spec.name, is_dir));
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn validate_staged_runtime_ownership(repo_root: &Path, specs: &[PackageSpec]) -> Result<()> {
    let staging_root = repo_root.join("out/packages/staging");
    let mut owners = BTreeMap::<String, &str>::new();
    let mut soname_owners = BTreeMap::<String, &str>::new();
    for spec in specs {
        let root = staging_root.join(spec.name);
        walk_tree(&root, &mut |path, metadata| {
            if !metadata.is_dir() && !path.starts_with(root.join("DEBIAN")) {
                if let Some(name) = path.file_name().and_then(OsStr::to_str) {
                    owners.entry(name.to_string()).or_insert(spec.name);
                }
                if let Some(dynamic) = command_text("readelf", &["-d"], path)? {
                    if let Some(soname) = dynamic_value(&dynamic, "Library soname") {
                        if let Some(owner) = soname_owners.get(&soname) {
                            if *owner != spec.name {
                                bail!(
                                    "SONAME {soname} has multiple package owners: {owner} and {}",
                                    spec.name
                                );
                            }
                        } else {
                            soname_owners.insert(soname, spec.name);
                        }
                    }
                }
            }
            Ok(())
        })?;
    }
    for spec in specs {
        for soname in runtime_libraries_for_spec(repo_root, spec)? {
            let name = Path::new(&soname)
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or(&soname);
            if name.starts_with("linux-vdso.so") {
                continue;
            }
            let owner = soname_owners
                .get(name)
                .or_else(|| owners.get(name))
                .ok_or_else(|| anyhow!("{} has unowned runtime dependency {name}", spec.name))?;
            if *owner != spec.name && !effective_dependencies(spec).contains(owner) {
                bail!(
                    "{} uses {name} from {owner} without declaring that dependency",
                    spec.name
                )
            }
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
    let paragraphs = parse_control_paragraphs(&fields)?;
    let paragraph = paragraphs
        .first()
        .ok_or_else(|| anyhow!("package {} has no control fields", path.display()))?;
    for (field, expected) in [
        ("Package", expected_name),
        ("Version", expected_version),
        ("Architecture", ARCH),
    ] {
        if control_field(paragraph, field)? != expected {
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
        validate_repository_packages(&fs::read_to_string(repository_packages)?)?;
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

pub(crate) fn generate_repository(repo_root: &Path) -> Result<()> {
    let spec = repository_stage_spec(repo_root)?;
    performance::execute_cached_stage(
        repo_root,
        &spec,
        || {
            validate_repository_against_inventory(
                &repo_root.join("out/repository"),
                &read_inventory(repo_root)?,
            )
        },
        || generate_repository_atomic(repo_root),
    )
}

pub(crate) fn repository_stage_spec(repo_root: &Path) -> Result<performance::StageSpec> {
    // inventory.toml is the ordered package name/version/architecture/SHA set.
    // Do not hash every .deb again merely to compute the key; package manifests
    // already validate those artifacts and a miss validates every copied pool
    // object against the recorded SHA before publication.
    let _ = read_inventory(repo_root)?;
    let inputs = vec![PathBuf::from("out/packages/inventory.toml")];
    Ok(performance::StageSpec {
        id: "repository".into(),
        source_inputs: Vec::new(),
        configuration_inputs: inputs,
        tools: vec![
            "dpkg-scanpackages".into(),
            "apt-ftparchive".into(),
            "gzip".into(),
        ],
        dependencies: Vec::new(),
        outputs: vec!["out/repository".into()],
        recipe: format!(
            "repository-v2:suite=trixie:codename=trixie:component=main:arch={ARCH}:origin=MattOS:label=MattOS Local:gzip=-n,-9:epoch={SOURCE_DATE_EPOCH}:manifest-schema={}",
            performance::STAGE_MANIFEST_SCHEMA_VERSION
        ),
    })
}

fn generate_repository_atomic(repo_root: &Path) -> Result<()> {
    let repository = repo_root.join("out/repository");
    let temp = performance::temporary_sibling(&repository, "building")?;
    let result = generate_repository_inner(repo_root, &temp);
    if let Err(error) = result {
        let _ = remove_path_if_exists(&temp);
        return Err(error);
    }
    performance::atomic_replace_path(&temp, &repository)
}

fn generate_repository_inner(repo_root: &Path, repository: &Path) -> Result<()> {
    let inventory = read_inventory(repo_root)?;
    for name in PACKAGE_NAMES {
        if !inventory.package.iter().any(|entry| entry.name == *name) {
            bail!("package {name} has not been built");
        }
    }
    let mut inventory_keys = BTreeSet::new();
    for entry in &inventory.package {
        if !PACKAGE_NAMES.contains(&entry.name.as_str()) {
            bail!("inventory contains unexpected package {}", entry.name)
        }
        let key = (&entry.name, &entry.version, &entry.architecture);
        if !inventory_keys.insert(key) {
            bail!(
                "duplicate package/version/architecture in inventory: {} {} {}",
                entry.name,
                entry.version,
                entry.architecture
            )
        }
    }
    let pool = repository.join("pool/main");
    let index_dir = repository.join("dists/trixie/main/binary-amd64");
    fs::create_dir_all(&pool)?;
    fs::create_dir_all(&index_dir)?;
    for entry in &inventory.package {
        let source = repo_root.join(&entry.artifact_path);
        let file_name = source
            .file_name()
            .ok_or_else(|| anyhow!("invalid artifact path"))?;
        fs::copy(&source, pool.join(file_name))?;
    }
    let scan = Command::new("dpkg-scanpackages")
        .args(["pool/main", "/dev/null"])
        .current_dir(&repository)
        .output()
        .context("failed to run dpkg-scanpackages")?;
    if !scan.status.success() {
        bail!(
            "dpkg-scanpackages failed: {}",
            String::from_utf8_lossy(&scan.stderr)
        );
    }
    let packages = index_dir.join("Packages");
    fs::write(&packages, scan.stdout)?;
    let gzip = Command::new("gzip")
        .args(["-n", "-9", "-c", path_str(&packages)?])
        .output()?;
    if !gzip.status.success() {
        bail!("gzip failed for Packages index");
    }
    fs::write(index_dir.join("Packages.gz"), gzip.stdout)?;

    let release = Command::new("apt-ftparchive")
        .args([
            "-o",
            "APT::FTPArchive::Release::Origin=MattOS",
            "-o",
            "APT::FTPArchive::Release::Label=MattOS Local",
            "-o",
            "APT::FTPArchive::Release::Suite=trixie",
            "-o",
            "APT::FTPArchive::Release::Codename=trixie",
            "-o",
            "APT::FTPArchive::Release::Architectures=amd64",
            "-o",
            "APT::FTPArchive::Release::Components=main",
            "-o",
            "APT::FTPArchive::Release::Description=Local MattOS bootstrap repository",
            "release",
            "dists/trixie",
        ])
        .env("SOURCE_DATE_EPOCH", SOURCE_DATE_EPOCH.to_string())
        .current_dir(&repository)
        .output()
        .context("failed to run apt-ftparchive")?;
    if !release.status.success() {
        bail!(
            "apt-ftparchive failed: {}",
            String::from_utf8_lossy(&release.stderr)
        );
    }
    let release_body = String::from_utf8(release.stdout)?;
    let release_body = release_body
        .lines()
        .map(|line| {
            if line.starts_with("Date: ") {
                "Date: Thu, 01 Jan 2026 00:00:00 +0000"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(repository.join("dists/trixie/Release"), release_body)?;
    validate_repository_against_inventory(repository, &inventory)?;
    println!(
        "generated local MattOS repository at {}",
        repository.display()
    );
    Ok(())
}

fn validate_repository(repository: &Path) -> Result<()> {
    let packages = fs::read_to_string(repository.join("dists/trixie/main/binary-amd64/Packages"))?;
    if packages.contains("deb.debian.org") || packages.contains("archive.ubuntu.com") {
        bail!("foreign repository URL found in Packages");
    }
    validate_repository_packages(&packages)?;
    let release = fs::read_to_string(repository.join("dists/trixie/Release"))?;
    for field in [
        "Origin: MattOS",
        "Label: MattOS Local",
        "Suite: trixie",
        "Codename: trixie",
        "Architectures: amd64",
        "Components: main",
        "SHA256:",
    ] {
        if !release.contains(field) {
            bail!("Release missing {field}");
        }
    }
    Ok(())
}

fn validate_repository_against_inventory(
    repository: &Path,
    inventory: &PackageInventory,
) -> Result<()> {
    validate_repository(repository)?;
    let packages_path = repository.join("dists/trixie/main/binary-amd64/Packages");
    let packages_body = fs::read_to_string(&packages_path)?;
    let paragraphs = parse_control_paragraphs(&packages_body)?;
    if paragraphs.len() != inventory.package.len() {
        bail!("Packages entry count differs from package inventory");
    }
    let mut expected_files = BTreeSet::new();
    for entry in &inventory.package {
        let artifact_name = Path::new(&entry.artifact_path)
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(|| anyhow!("invalid package artifact path {}", entry.artifact_path))?;
        let relative = format!("pool/main/{artifact_name}");
        expected_files.insert(relative.clone());
        let pool_artifact = repository.join(&relative);
        if sha256_file(&pool_artifact)? != entry.sha256 {
            bail!("repository artifact digest differs for {}", entry.name);
        }
        let paragraph = paragraphs
            .iter()
            .find(|paragraph| paragraph.get("Package") == Some(&entry.name))
            .ok_or_else(|| anyhow!("Packages index missing {}", entry.name))?;
        if control_field(paragraph, "Version")? != entry.version
            || control_field(paragraph, "Architecture")? != entry.architecture
            || control_field(paragraph, "Filename")? != relative
            || control_field(paragraph, "SHA256")? != entry.sha256
        {
            bail!(
                "Packages metadata differs from inventory for {}",
                entry.name
            );
        }
    }
    let pool = repository.join("pool/main");
    let actual_files = fs::read_dir(&pool)?
        .map(|entry| {
            entry.map(|entry| format!("pool/main/{}", entry.file_name().to_string_lossy()))
        })
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual_files != expected_files {
        bail!("repository pool path set differs from package inventory");
    }

    let compressed = Command::new("gzip")
        .args([
            "-dc",
            path_str(&packages_path.with_file_name("Packages.gz"))?,
        ])
        .output()?;
    if !compressed.status.success() || compressed.stdout != packages_body.as_bytes() {
        bail!("Packages.gz is corrupt or differs from Packages");
    }
    validate_release_sha256(repository)?;
    Ok(())
}

fn validate_release_sha256(repository: &Path) -> Result<()> {
    let release_path = repository.join("dists/trixie/Release");
    let body = fs::read_to_string(&release_path)?;
    let mut in_sha256 = false;
    let mut checked = BTreeSet::new();
    for line in body.lines() {
        if line == "SHA256:" {
            in_sha256 = true;
            continue;
        }
        if in_sha256 && !line.starts_with(' ') {
            break;
        }
        if !in_sha256 || line.trim().is_empty() {
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 {
            bail!("invalid SHA256 entry in Release: {line}");
        }
        let relative = fields[2];
        let path = release_path.parent().unwrap().join(relative);
        let metadata = fs::metadata(&path)?;
        if metadata.len().to_string() != fields[1] || sha256_file(&path)? != fields[0] {
            bail!("Release SHA256 mismatch for {relative}");
        }
        checked.insert(relative.to_string());
    }
    for required in [
        "main/binary-amd64/Packages",
        "main/binary-amd64/Packages.gz",
    ] {
        if !checked.contains(required) {
            bail!("Release SHA256 inventory missing {required}");
        }
    }
    Ok(())
}

fn validate_repository_packages(body: &str) -> Result<()> {
    package_install_order()?;
    let paragraphs = parse_control_paragraphs(body)?;
    let mut by_name: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut provided = BTreeSet::new();
    let mut keys = BTreeSet::new();
    for paragraph in &paragraphs {
        let name = control_field(paragraph, "Package")?.to_string();
        let version = control_field(paragraph, "Version")?.to_string();
        let architecture = control_field(paragraph, "Architecture")?.to_string();
        validate_package_name(&name)?;
        validate_debian_version(&version)?;
        if architecture != ARCH {
            bail!("repository package {name} has architecture {architecture}, expected {ARCH}")
        }
        if !keys.insert((name.clone(), version.clone(), architecture.clone())) {
            bail!(
                "duplicate repository package/version/architecture: {name} {version} {architecture}"
            )
        }
        by_name
            .entry(name)
            .or_default()
            .push((version, architecture));
        if let Some(provides) = paragraph.get("Provides") {
            for item in provides.split(',') {
                let virtual_name = dependency_name(item)?;
                provided.insert(virtual_name.to_string());
            }
        }
    }
    for name in PACKAGE_NAMES {
        if !by_name.contains_key(*name) {
            bail!("Packages index missing {name}")
        }
    }
    for paragraph in &paragraphs {
        let package = control_field(paragraph, "Package")?;
        let Some(depends) = paragraph.get("Depends") else {
            continue;
        };
        for group in depends
            .split(',')
            .map(str::trim)
            .filter(|group| !group.is_empty())
        {
            let mut satisfied = false;
            for alternative in group.split('|').map(str::trim) {
                let name = dependency_name(alternative)?;
                if let Some(candidates) = by_name.get(name) {
                    if let Some(expected) = exact_dependency_version(alternative)? {
                        satisfied = candidates.iter().any(|(version, _)| version == expected);
                    } else {
                        satisfied = true;
                    }
                } else if provided.contains(name)
                    && exact_dependency_version(alternative)?.is_none()
                {
                    satisfied = true;
                }
                if satisfied {
                    break;
                }
            }
            if !satisfied {
                bail!("repository dependency for {package} is unsatisfied: {group}")
            }
        }
    }
    Ok(())
}

fn parse_control_paragraphs(body: &str) -> Result<Vec<BTreeMap<String, String>>> {
    let mut paragraphs = Vec::new();
    for raw in body
        .split("\n\n")
        .filter(|paragraph| !paragraph.trim().is_empty())
    {
        let mut paragraph: BTreeMap<String, String> = BTreeMap::new();
        let mut last_key: Option<String> = None;
        for line in raw.lines() {
            if line.starts_with([' ', '\t']) {
                if let Some(key) = &last_key {
                    paragraph.get_mut(key).expect("field exists").push_str(line);
                }
                continue;
            }
            let (key, value) = line
                .split_once(':')
                .ok_or_else(|| anyhow!("invalid control line {line:?}"))?;
            paragraph.insert(key.to_string(), value.trim().to_string());
            last_key = Some(key.to_string());
        }
        paragraphs.push(paragraph);
    }
    Ok(paragraphs)
}

fn control_field<'a>(paragraph: &'a BTreeMap<String, String>, field: &str) -> Result<&'a str> {
    paragraph
        .get(field)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("repository package paragraph lacks {field}"))
}

fn dependency_name(relation: &str) -> Result<&str> {
    let name = relation
        .trim()
        .split([' ', '('])
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    validate_package_name(name)?;
    Ok(name)
}

fn exact_dependency_version(relation: &str) -> Result<Option<&str>> {
    let Some((_, constraint)) = relation.split_once('(') else {
        return Ok(None);
    };
    let constraint = constraint.trim_end_matches(')').trim();
    let mut fields = constraint.split_whitespace();
    let operator = fields.next().unwrap_or_default();
    let version = fields.next().unwrap_or_default();
    if operator != "=" || version.is_empty() {
        bail!("MattOS bootstrap repository only accepts exact dependency constraints: {relation}")
    }
    Ok(Some(version))
}

pub(crate) fn install_prototype_packages(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let inventory = read_inventory(repo_root)?;
    validate_repository_against_inventory(&repo_root.join("out/repository"), &inventory)?;
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
        assert!(resources
            .join("defaults/com.system76.CosmicPanel/v1/entries")
            .is_file());
        assert!(resources
            .join("layouts/top-panel-and-bottom-dock/layout.kdl")
            .is_file());
        assert!(resources.join("themes/nebula-dark.ron").is_file());

        let source = include_str!("main.rs");
        for path in [
            "resources/COSMIC/defaults",
            "resources/COSMIC/layouts",
            "resources/COSMIC/themes",
            "/usr/share/cosmic",
            "/usr/share/cosmic-layouts",
            "/usr/share/cosmic-themes",
        ] {
            assert!(source.contains(path), "COSMIC resource contract omits {path}");
        }

        let libcosmic = fs::read_to_string(root.join("src/desktop/cosmic/libcosmic/cosmic-config/src/lib.rs")).unwrap();
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
        ] {
            assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
        }
        assert_eq!(PACKAGE_NAMES.len(), 163);
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
            let path = staging
                .path()
                .join("usr/share/iso-codes/json")
                .join(name);
            assert!(path.is_file(), "missing {name}");
            assert!(!fs::read_to_string(path).unwrap().is_empty());
        }
        assert!(staging
            .path()
            .join("usr/share/doc/iso-codes/PROVENANCE.md")
            .is_file());
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
        assert_eq!(PACKAGE_NAMES.len(), 163);
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
        assert_eq!(PACKAGE_NAMES.len(), 163);
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
        let source = include_str!("packaging.rs");
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
        let local = preferences.find("Pin-Priority: 1001").unwrap();
        let hosted = preferences.find("Pin-Priority: 990").unwrap();
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
            "ff56c6cb56951543dfb8eb0298f424d34517a1d87175a44060ef6f97d6a51cd4"
        );
    }

    #[test]
    fn apt_sources_are_signed_disabled_scaffolds_and_never_trust_debian() {
        let hosted = include_str!("../../../system/packages/config/apt/mattos-hosted.sources");
        let debian = include_str!("../../../system/packages/config/apt/debian-trixie.sources");
        assert!(hosted.contains("Enabled: no"));
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
}
