use super::*;
use clap::Subcommand;
use filetime::{FileTime, set_file_times, set_symlink_file_times};
use sha2::{Digest, Sha256};
use std::io::Read;

const ARCH: &str = "amd64";
const REVISION: &str = "1mattos1";
const SOURCE_DATE_EPOCH: i64 = 1_767_225_600; // 2026-01-01T00:00:00Z
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
    "usr/lib/apt/methods/store",
];
const APT_CONFFILES: &[&str] = &[
    "/etc/apt/apt.conf.d/01mattos",
    "/etc/apt/sources.list.d/mattos.sources",
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
const IPROUTE2_RUNTIME_PATHS: &[&str] = &[
    "usr/sbin/ip",
    "usr/sbin/ss",
    "usr/sbin/bridge",
    "usr/sbin/tc",
];
const IPUTILS_RUNTIME_PATHS: &[&str] = &["usr/bin/ping", "usr/bin/tracepath"];
#[cfg(test)]
const MIGRATED_BOOTSTRAP_SONAME_PREFIXES: &[&str] = &[
    "libc.so",
    "libm.so",
    "ld-linux-",
    "libexpat.so",
    "libcap.so",
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
    "mattos-libc6",
    "mattos-libgcc-s1",
    "mattos-libstdc++6",
    "mattos-linux-libc-dev",
    "mattos-libc6-dev",
    "mattos-libgcc-dev",
    "mattos-libstdc++-dev",
    "mattos-binutils",
    "mattos-gcc-common",
    "mattos-cpp",
    "mattos-gcc",
    "mattos-g++",
    "mattos-make",
    "mattos-libc-bin",
    "mattos-base-files",
    "mattos-ca-certificates",
    "mattos-brush",
    "mattos-coreutils",
    "mattos-curl",
    "mattos-libmd0",
    "mattos-libbsd0",
    "mattos-libzstd1",
    "mattos-libcrypto3",
    "mattos-libssl3",
    "mattos-libelf1",
    "mattos-libpcre2-8-0",
    "mattos-libselinux1",
    "mattos-libcrypt1",
    "mattos-libblkid1",
    "mattos-libmount1",
    "mattos-libsmartcols1",
    "mattos-mount",
    "mattos-dpkg",
    "mattos-libapt-pkg",
    "mattos-apt",
    "mattos-libtinfow6",
    "mattos-libncursesw6",
    "mattos-terminfo",
    "mattos-ncurses-bin",
    "mattos-libkmod2",
    "mattos-kmod",
    "mattos-libproc2",
    "mattos-procps",
    "mattos-libsystemd0",
    "mattos-libudev1",
    "mattos-libexpat1",
    "mattos-libcap2",
    "mattos-libacl1",
    "mattos-zlib1g",
    "mattos-libbz2-1.0",
    "mattos-liblz4-1",
    "mattos-liblzma5",
    "mattos-libxxhash0",
    "mattos-tar",
    "mattos-dbus-broker",
    "mattos-libpam0",
    "mattos-libpam-misc0",
    "mattos-pam-modules",
    "mattos-pam-runtime",
    "mattos-shadow",
    "mattos-sudo-rs",
    "mattos-util-linux-auth",
    "mattos-iproute2",
    "mattos-iputils",
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
}

#[derive(Clone, Debug)]
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
            name: "mattos-libc6",
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
            name: "mattos-libgcc-s1",
            description: "GCC shared unwinding runtime built for MattOS",
            source_component: "gcc",
            depends: &["mattos-filesystem", "mattos-libc6"],
            provides: &["libgcc-s1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-libstdc++6",
            description: "GNU C++ runtime library built for MattOS",
            source_component: "gcc",
            depends: &["mattos-filesystem", "mattos-libc6", "mattos-libgcc-s1"],
            provides: &["libstdc++6"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-linux-libc-dev",
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
            name: "mattos-libc6-dev",
            description: "GNU C Library headers and link-time files for MattOS",
            source_component: "glibc",
            depends: &["mattos-libc6", "mattos-linux-libc-dev"],
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
            depends: &["mattos-libc6-dev", "mattos-libgcc-s1"],
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
            depends: &["mattos-libc6-dev", "mattos-libgcc-dev", "mattos-libstdc++6"],
            provides: &["libstdc++-dev"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "mattos-binutils",
            description: "GNU binary utilities built natively for MattOS",
            source_component: "binutils",
            depends: &["mattos-libc6"],
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
            depends: &[
                "mattos-binutils",
                "mattos-libgcc-dev",
                "mattos-libstdc++6",
                "mattos-zlib1g",
            ],
            provides: &["gcc-common"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "mattos-cpp",
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
            name: "mattos-gcc",
            description: "GNU C compiler built natively for MattOS",
            source_component: "gcc",
            depends: &["mattos-cpp", "mattos-gcc-common", "mattos-libc6-dev"],
            provides: &["c-compiler", "gcc"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "mattos-g++",
            description: "GNU C++ compiler built natively for MattOS",
            source_component: "gcc",
            depends: &["mattos-gcc", "mattos-libstdc++-dev"],
            provides: &["c++-compiler", "g++"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "mattos-make",
            description: "GNU Make built natively for MattOS",
            source_component: "make",
            depends: &["mattos-libc6"],
            provides: &["make"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "mattos-libc-bin",
            description: "GNU C Library runtime utilities built for MattOS",
            source_component: "glibc",
            depends: &["mattos-filesystem", "mattos-libc6"],
            provides: &["libc-bin"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
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
            name: "mattos-ca-certificates",
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
            depends: &["mattos-filesystem", "mattos-libgcc-s1"],
            provides: &["mattos-shell", "sh", "bash"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-coreutils",
            description: "uutils core utilities built for MattOS",
            source_component: "coreutils",
            depends: &["mattos-filesystem", "mattos-libgcc-s1"],
            provides: &["coreutils"],
            conflicts: &["coreutils"],
            replaces: &["coreutils"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-curl",
            description: "curl command-line transfer client built for MattOS",
            source_component: "curl",
            depends: &[
                "mattos-filesystem",
                "mattos-ca-certificates",
                "mattos-zlib1g",
                "mattos-libzstd1",
                "mattos-libcrypto3",
                "mattos-libssl3",
            ],
            provides: &["curl"],
            conflicts: &["curl"],
            replaces: &["curl"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "mattos-libmd0",
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
            name: "mattos-libbsd0",
            description: "libbsd portability runtime library built for MattOS",
            source_component: "libbsd",
            depends: &["mattos-libmd0"],
            provides: &["libbsd0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libzstd1",
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
            depends: &["mattos-zlib1g", "mattos-libzstd1"],
            provides: &["libcrypto3"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libssl3",
            description: "OpenSSL TLS runtime library built for MattOS",
            source_component: "openssl",
            depends: &["mattos-libcrypto3", "mattos-zlib1g", "mattos-libzstd1"],
            provides: &["libssl3t64"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libelf1",
            description: "elfutils libelf runtime library built for MattOS",
            source_component: "elfutils",
            depends: &["mattos-zlib1g", "mattos-libzstd1"],
            provides: &["libelf1t64"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libpcre2-8-0",
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
            name: "mattos-libselinux1",
            description: "SELinux userspace runtime library built for MattOS",
            source_component: "selinux",
            depends: &["mattos-libpcre2-8-0"],
            provides: &["libselinux1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libcrypt1",
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
            name: "mattos-libblkid1",
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
            name: "mattos-libmount1",
            description: "util-linux mount runtime library built for MattOS",
            source_component: "util-linux",
            depends: &["mattos-libblkid1"],
            provides: &["libmount1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libsmartcols1",
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
            name: "mattos-mount",
            description: "util-linux mount and unmount tools built for MattOS",
            source_component: "util-linux",
            depends: &[
                "mattos-libblkid1",
                "mattos-libmount1",
                "mattos-libsmartcols1",
                "mattos-libselinux1",
            ],
            provides: &["mount"],
            conflicts: &["mount"],
            replaces: &["mount"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-dpkg",
            description: "dpkg binary package management runtime built for MattOS",
            source_component: "dpkg",
            depends: &[
                "mattos-filesystem",
                "mattos-tar",
                "mattos-zlib1g",
                "mattos-libbz2-1.0",
                "mattos-liblzma5",
                "mattos-libzstd1",
                "mattos-libmd0",
                "mattos-libpcre2-8-0",
                "mattos-libselinux1",
            ],
            provides: &["dpkg"],
            conflicts: &["dpkg"],
            replaces: &["dpkg"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-libapt-pkg",
            description: "APT public runtime library built for MattOS",
            source_component: "apt",
            depends: &[
                "mattos-libgcc-s1",
                "mattos-libstdc++6",
                "mattos-libudev1",
                "mattos-libsystemd0",
                "mattos-zlib1g",
                "mattos-libbz2-1.0",
                "mattos-liblz4-1",
                "mattos-liblzma5",
                "mattos-libxxhash0",
                "mattos-libzstd1",
                "mattos-libcrypto3",
            ],
            provides: &["libapt-pkg7.0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-apt",
            description: "APT command-line package manager and local repository methods for MattOS",
            source_component: "apt",
            depends: &[
                "mattos-libgcc-s1",
                "mattos-libstdc++6",
                "mattos-ca-certificates",
                "mattos-dpkg",
                "mattos-libapt-pkg",
                "mattos-libudev1",
                "mattos-libsystemd0",
                "mattos-zlib1g",
                "mattos-libbz2-1.0",
                "mattos-liblz4-1",
                "mattos-liblzma5",
                "mattos-libxxhash0",
                "mattos-libzstd1",
                "mattos-libcrypto3",
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
            provides: &["libtinfo6"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libncursesw6",
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
            name: "mattos-terminfo",
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
            name: "mattos-ncurses-bin",
            description: "ncurses terminal utilities built for MattOS",
            source_component: "ncurses",
            depends: &["mattos-libtinfow6", "mattos-terminfo"],
            provides: &["ncurses-bin"],
            conflicts: &["ncurses-bin"],
            replaces: &["ncurses-bin"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libkmod2",
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
            name: "mattos-kmod",
            description: "Linux kernel module management tools built for MattOS",
            source_component: "kmod",
            depends: &["mattos-libkmod2"],
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
            provides: &["libproc2-0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-procps",
            description: "procps process inspection utilities built for MattOS",
            source_component: "procps-ng",
            depends: &[
                "mattos-libproc2",
                "mattos-libncursesw6",
                "mattos-libtinfow6",
            ],
            provides: &["procps"],
            conflicts: &["procps"],
            replaces: &["procps"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libsystemd0",
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
            name: "mattos-libudev1",
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
            name: "mattos-libexpat1",
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
            name: "mattos-libcap2",
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
            name: "mattos-libacl1",
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
            name: "mattos-zlib1g",
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
            name: "mattos-libbz2-1.0",
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
            name: "mattos-liblz4-1",
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
            name: "mattos-liblzma5",
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
            name: "mattos-libxxhash0",
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
            name: "mattos-tar",
            description: "GNU tar archive utility built for MattOS",
            source_component: "tar",
            depends: &["mattos-libacl1"],
            provides: &["tar"],
            conflicts: &["tar"],
            replaces: &["tar"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-dbus-broker",
            description: "D-Bus message broker and MattOS bus policy",
            source_component: "dbus-broker",
            depends: &["mattos-libexpat1", "mattos-libsystemd0"],
            provides: &["dbus-system-bus"],
            conflicts: &["dbus-daemon"],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-libpam0",
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
            depends: &["mattos-libpam0"],
            provides: &["libpam-misc0"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-pam-modules",
            description: "Linux PAM authentication modules built for MattOS",
            source_component: "linux-pam",
            depends: &["mattos-libpam0", "mattos-libcrypt1"],
            provides: &["libpam-modules"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-pam-runtime",
            description: "MattOS PAM policy and authentication helper runtime",
            source_component: "linux-pam",
            depends: &["mattos-libpam0", "mattos-pam-modules", "mattos-libcrypt1"],
            provides: &["libpam-runtime"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-shadow",
            description: "Shadow account administration tools built for MattOS",
            source_component: "shadow",
            depends: &[
                "mattos-libpam0",
                "mattos-libpam-misc0",
                "mattos-pam-runtime",
                "mattos-libbsd0",
                "mattos-libmd0",
                "mattos-libcrypt1",
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
            depends: &["mattos-libgcc-s1", "mattos-libpam0", "mattos-pam-runtime"],
            provides: &["sudo"],
            conflicts: &["sudo"],
            replaces: &["sudo"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-util-linux-auth",
            description: "util-linux login, su, and agetty tools built for MattOS",
            source_component: "util-linux",
            depends: &[
                "mattos-libpam0",
                "mattos-libpam-misc0",
                "mattos-pam-runtime",
            ],
            provides: &["login"],
            conflicts: &["login"],
            replaces: &["login"],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-iproute2",
            description: "Linux routing and network configuration tools built for MattOS",
            source_component: "iproute2",
            depends: &[
                "mattos-libcap2",
                "mattos-zlib1g",
                "mattos-libzstd1",
                "mattos-libelf1",
                "mattos-libpcre2-8-0",
                "mattos-libselinux1",
            ],
            provides: &["iproute2"],
            conflicts: &["iproute2"],
            replaces: &["iproute2"],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-iputils",
            description: "Linux ping and tracepath network diagnostics built for MattOS",
            source_component: "iputils",
            depends: &[],
            provides: &["iputils-ping"],
            conflicts: &["iputils-ping"],
            replaces: &["iputils-ping"],
            essential: false,
            priority: "important",
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
        for dependency in spec
            .depends
            .iter()
            .copied()
            .filter(|name| name.starts_with("mattos-"))
        {
            if !names.contains(dependency) {
                bail!(
                    "package {} depends on unknown MattOS package {dependency}",
                    spec.name
                )
            }
            dependencies.insert(dependency);
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
    }
}

pub(crate) fn build_all_packages(repo_root: &Path) -> Result<()> {
    remove_path_if_exists(&repo_root.join("out/packages/staging/mattos-bootstrap-runtime"))?;
    let artifact_root = repo_root.join("out/packages/amd64");
    if artifact_root.is_dir() {
        for entry in fs::read_dir(&artifact_root)? {
            let path = entry?.path();
            if path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with("mattos-bootstrap-runtime_"))
            {
                fs::remove_file(path)?;
            }
        }
    }
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
    for spec in &selected {
        stage_package(repo_root, spec)?;
    }
    // Check the complete prototype set whenever it is fully staged, otherwise the
    // selected subset. Shared directories are intentionally permitted.
    let collision_specs: Vec<PackageSpec> = if PACKAGE_NAMES
        .iter()
        .all(|name| staging_root.join(name).is_dir())
    {
        specs.clone()
    } else {
        selected.clone()
    };
    detect_staging_collisions(&staging_root, &collision_specs)?;
    if collision_specs.len() == PACKAGE_NAMES.len() {
        validate_staged_runtime_ownership(repo_root, &collision_specs)?;
    }

    let mut inventory = read_inventory(repo_root).unwrap_or(PackageInventory {
        package: Vec::new(),
    });
    for spec in selected {
        let version = package_version(repo_root, &spec)?;
        let staging = staging_root.join(spec.name);
        normalize_tree_timestamps(&staging)?;
        let artifact = artifact_root.join(format!("{}_{}_{}.deb", spec.name, version, ARCH));
        let staging_arg = path_str(&staging)?;
        let artifact_arg = path_str(&artifact)?;
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
            bail!("dpkg-deb failed for {} with {status}", spec.name)
        }
        verify_deb(&artifact, spec.name, &version)?;
        let runtime_libraries = runtime_libraries_for_spec(repo_root, &spec)?;
        let entry = PackageInventoryEntry {
            name: spec.name.to_string(),
            version,
            architecture: ARCH.to_string(),
            artifact_path: relative_display(repo_root, &artifact)?,
            source_component: spec.source_component.to_string(),
            dependencies: package_dependencies(repo_root, &spec)?,
            runtime_libraries,
            file_count: count_package_entries(&staging)?,
            sha256: sha256_file(&artifact)?,
        };
        inventory.package.retain(|old| old.name != entry.name);
        inventory.package.push(entry);
    }
    inventory.package.sort_by(|a, b| a.name.cmp(&b.name));
    write_inventory(repo_root, &inventory)?;
    print_inventory(repo_root)
}

fn stage_package(repo_root: &Path, spec: &PackageSpec) -> Result<()> {
    let staging = repo_root.join("out/packages/staging").join(spec.name);
    remove_path_if_exists(&staging)?;
    fs::create_dir_all(staging.join("DEBIAN"))?;
    match spec.name {
        "mattos-filesystem" => stage_filesystem(&staging)?,
        "mattos-libc6" => stage_glibc_runtime(repo_root, &staging)?,
        "mattos-libgcc-s1" => {
            stage_gcc_runtime_library(repo_root, &staging, "libgcc_s.so.1", "mattos-libgcc-s1")?
        }
        "mattos-libstdc++6" => {
            stage_gcc_runtime_library(repo_root, &staging, "libstdc++.so.6", "mattos-libstdc++6")?
        }
        "mattos-linux-libc-dev" => stage_linux_libc_dev(repo_root, &staging)?,
        "mattos-libc6-dev" => stage_glibc_development(repo_root, &staging)?,
        "mattos-libgcc-dev" => stage_gcc_development(repo_root, &staging, false)?,
        "mattos-libstdc++-dev" => stage_gcc_development(repo_root, &staging, true)?,
        "mattos-binutils" => stage_native_binutils(repo_root, &staging)?,
        "mattos-gcc-common" => stage_native_gcc_common(repo_root, &staging)?,
        "mattos-cpp" => stage_native_compiler_driver(repo_root, &staging, "cpp")?,
        "mattos-gcc" => stage_native_compiler_driver(repo_root, &staging, "gcc")?,
        "mattos-g++" => stage_native_compiler_driver(repo_root, &staging, "g++")?,
        "mattos-make" => stage_native_make(repo_root, &staging)?,
        "mattos-libc-bin" => stage_glibc_utilities(repo_root, &staging)?,
        "mattos-base-files" => stage_base_files(repo_root, &staging)?,
        "mattos-ca-certificates" => stage_ca_certificates(repo_root, &staging)?,
        "mattos-brush" => stage_brush(repo_root, &staging)?,
        "mattos-coreutils" => stage_coreutils(repo_root, &staging)?,
        "mattos-curl" => {
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
        "mattos-dpkg" => stage_dpkg(repo_root, &staging)?,
        "mattos-libapt-pkg" => stage_libapt_pkg(repo_root, &staging)?,
        "mattos-apt" => stage_apt(repo_root, &staging)?,
        "mattos-libtinfow6" => stage_library_family(
            repo_root,
            &staging,
            "ncurses",
            &["libtinfow.so.6.6", "libtinfow.so.6"],
        )?,
        "mattos-libncursesw6" => stage_library_family(
            repo_root,
            &staging,
            "ncurses",
            &["libncursesw.so.6.6", "libncursesw.so.6"],
        )?,
        "mattos-terminfo" => stage_terminfo(repo_root, &staging)?,
        "mattos-ncurses-bin" => {
            stage_runtime_paths(repo_root, &staging, "ncurses", NCURSES_RUNTIME_PATHS)?
        }
        "mattos-libkmod2" => stage_library_family(
            repo_root,
            &staging,
            "kmod",
            &["libkmod.so.2.5.1", "libkmod.so.2"],
        )?,
        "mattos-kmod" => stage_runtime_paths(repo_root, &staging, "kmod", KMOD_RUNTIME_PATHS)?,
        "mattos-libproc2" => stage_library_family(
            repo_root,
            &staging,
            "procps-ng",
            &["libproc2.so.1.0.1", "libproc2.so.1"],
        )?,
        "mattos-procps" => stage_procps(repo_root, &staging)?,
        "mattos-libsystemd0" => stage_library_family(
            repo_root,
            &staging,
            "systemd",
            &["libsystemd.so.0.44.0", "libsystemd.so.0"],
        )?,
        "mattos-libudev1" => stage_library_family(
            repo_root,
            &staging,
            "systemd",
            &["libudev.so.1.7.14", "libudev.so.1"],
        )?,
        "mattos-libexpat1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "expat",
            "libexpat.so.1",
            "src/system/libraries/expat/expat/COPYING",
            "mattos-libexpat1",
        )?,
        "mattos-libcap2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libcap",
            "libcap.so.2",
            "src/system/libraries/libcap/License",
            "mattos-libcap2",
        )?,
        "mattos-libacl1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "acl",
            "libacl.so.1",
            "src/system/libraries/acl/doc/COPYING.LGPL",
            "mattos-libacl1",
        )?,
        "mattos-zlib1g" => stage_imported_soname_library(
            repo_root,
            &staging,
            "zlib",
            "libz.so.1",
            "src/system/libraries/zlib/LICENSE",
            "mattos-zlib1g",
        )?,
        "mattos-libbz2-1.0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "bzip2",
            "libbz2.so.1.0",
            "src/system/libraries/bzip2/LICENSE",
            "mattos-libbz2-1.0",
        )?,
        "mattos-liblz4-1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "lz4",
            "liblz4.so.1",
            "src/system/libraries/lz4/LICENSE",
            "mattos-liblz4-1",
        )?,
        "mattos-liblzma5" => stage_imported_soname_library(
            repo_root,
            &staging,
            "xz",
            "liblzma.so.5",
            "src/system/libraries/xz/COPYING",
            "mattos-liblzma5",
        )?,
        "mattos-libxxhash0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "xxhash",
            "libxxhash.so.0",
            "src/system/libraries/xxhash/LICENSE",
            "mattos-libxxhash0",
        )?,
        "mattos-libmd0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libmd",
            "libmd.so.0",
            "src/system/libraries/libmd/COPYING",
            "mattos-libmd0",
        )?,
        "mattos-libbsd0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libbsd",
            "libbsd.so.0",
            "src/system/libraries/libbsd/COPYING",
            "mattos-libbsd0",
        )?,
        "mattos-libzstd1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "zstd",
            "libzstd.so.1",
            "src/system/libraries/zstd/LICENSE",
            "mattos-libzstd1",
        )?,
        "mattos-libcrypto3" => stage_imported_soname_library(
            repo_root,
            &staging,
            "openssl",
            "libcrypto.so.3",
            "src/system/libraries/openssl/LICENSE.txt",
            "mattos-libcrypto3",
        )?,
        "mattos-libssl3" => stage_imported_soname_library(
            repo_root,
            &staging,
            "openssl",
            "libssl.so.3",
            "src/system/libraries/openssl/LICENSE.txt",
            "mattos-libssl3",
        )?,
        "mattos-libelf1" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "elfutils",
                "libelf.so.1",
                "src/system/libraries/elfutils/COPYING-LGPLV3",
                "mattos-libelf1",
            )?;
            copy_preserving(
                &repo_root.join("src/system/libraries/elfutils/COPYING-GPLV2"),
                &staging.join("usr/share/doc/mattos-libelf1/copyright.GPL-2"),
            )?;
        }
        "mattos-libpcre2-8-0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "pcre2",
            "libpcre2-8.so.0",
            "src/system/libraries/pcre2/LICENCE.md",
            "mattos-libpcre2-8-0",
        )?,
        "mattos-libselinux1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "selinux",
            "libselinux.so.1",
            "src/system/security/selinux/libselinux/LICENSE",
            "mattos-libselinux1",
        )?,
        "mattos-libcrypt1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libxcrypt",
            "libcrypt.so.1",
            "src/system/libraries/libxcrypt/COPYING.LIB",
            "mattos-libcrypt1",
        )?,
        "mattos-libblkid1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libblkid.so.1",
            "src/userland/util-linux/COPYING",
            "mattos-libblkid1",
        )?,
        "mattos-libmount1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libmount.so.1",
            "src/userland/util-linux/COPYING",
            "mattos-libmount1",
        )?,
        "mattos-libsmartcols1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libsmartcols.so.1",
            "src/userland/util-linux/COPYING",
            "mattos-libsmartcols1",
        )?,
        "mattos-mount" => {
            stage_runtime_paths(
                repo_root,
                &staging,
                "util-linux",
                &["usr/bin/mount", "usr/bin/umount"],
            )?;
            copy_preserving(
                &repo_root.join("src/userland/util-linux/COPYING"),
                &staging.join("usr/share/doc/mattos-mount/copyright"),
            )?;
            for rel in ["usr/bin/mount", "usr/bin/umount"] {
                set_mode(staging.join(rel), 0o4755)?;
            }
        }
        "mattos-tar" => {
            stage_executable(
                &repo_root.join("out/build/tar/install/usr/bin/tar"),
                &staging.join("usr/bin/tar"),
                0o755,
            )?;
            copy_preserving(
                &repo_root.join("src/userland/tar/COPYING"),
                &staging.join("usr/share/doc/mattos-tar/copyright"),
            )?;
        }
        "mattos-dbus-broker" => stage_dbus_broker(repo_root, &staging)?,
        "mattos-libpam0" => stage_library_family(
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
        "mattos-pam-modules" => stage_pam_modules(repo_root, &staging)?,
        "mattos-pam-runtime" => stage_pam_runtime(repo_root, &staging)?,
        "mattos-shadow" => stage_shadow(repo_root, &staging)?,
        "mattos-sudo-rs" => stage_sudo_rs(repo_root, &staging)?,
        "mattos-util-linux-auth" => stage_util_linux_auth(repo_root, &staging)?,
        "mattos-iproute2" => stage_iproute2(repo_root, &staging)?,
        "mattos-iputils" => {
            stage_runtime_paths(repo_root, &staging, "iputils", IPUTILS_RUNTIME_PATHS)?
        }
        _ => bail!("no staging implementation for {}", spec.name),
    }

    if !matches!(
        spec.name,
        "mattos-libc6" | "mattos-libgcc-s1" | "mattos-libstdc++6"
    ) {
        strip_staged_debug(repo_root, &staging)?;
    }

    let version = package_version(repo_root, spec)?;
    validate_debian_version(&version)?;
    let runtime_libraries = runtime_libraries_for_spec(repo_root, spec)?;
    write_provenance(repo_root, &staging, spec, &version, &runtime_libraries)?;
    if matches!(
        spec.name,
        "mattos-linux-libc-dev"
            | "mattos-libc6-dev"
            | "mattos-libgcc-dev"
            | "mattos-libstdc++-dev"
            | "mattos-binutils"
            | "mattos-gcc-common"
            | "mattos-cpp"
            | "mattos-gcc"
            | "mattos-g++"
            | "mattos-make"
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
        &staging.join("usr/share/doc/mattos-libc6/copyright"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/LICENSES"),
        &staging.join("usr/share/doc/mattos-libc6/LICENSES"),
    )?;
    fs::write(
        staging.join("usr/share/doc/mattos-libc6/runtime-files.tsv"),
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
        &staging.join("usr/share/doc/mattos-libc-bin/copyright"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/LICENSES"),
        &staging.join("usr/share/doc/mattos-libc-bin/LICENSES"),
    )?;
    Ok(())
}

fn stage_brush(repo_root: &Path, staging: &Path) -> Result<()> {
    let bin_dir = staging.join("usr/bin");
    stage_executable(
        &repo_root.join("src/userland/brush/target/release/brush"),
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
    let config = repo_root.join("src/system/packages/config/base-files");
    copy_preserving(&config.join("issue"), &staging.join("etc/issue"))?;
    let conffiles = ["/etc/hostname", "/etc/profile", "/etc/shells", "/etc/issue"];
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
    copy_preserving(
        &metadata,
        &staging.join("usr/share/doc/mattos-ca-certificates/ca-bundle.toml"),
    )?;
    fs::write(
        staging.join("usr/share/doc/mattos-ca-certificates/UPDATE.md"),
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

fn component_install(repo_root: &Path, component: &str) -> PathBuf {
    repo_root.join("out/build").join(component).join("install")
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

fn stage_library_family(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    names: &[&str],
) -> Result<()> {
    let source = component_install(repo_root, component).join("usr/lib/x86_64-linux-gnu");
    let destination = staging.join("usr/lib/x86_64-linux-gnu");
    for name in names {
        copy_path_preserving(&source.join(name), &destination.join(name))?;
    }
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
        &staging.join("usr/share/doc/mattos-linux-libc-dev/copyright"),
    )?;
    copy_preserving(
        &repo_root.join("out/build/glibc/linux-headers-inventory.txt"),
        &staging.join("usr/share/doc/mattos-linux-libc-dev/generated-files.txt"),
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
        &staging.join("usr/share/doc/mattos-libc6-dev/copyright"),
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
        &staging.join("usr/share/doc/mattos-binutils/copyright"),
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
        &staging
            .join("usr/share/doc")
            .join(format!("mattos-{driver}"))
            .join("copyright"),
    )
}

fn stage_native_make(repo_root: &Path, staging: &Path) -> Result<()> {
    copy_preserving(
        &repo_root.join("out/build/make/install/usr/bin/make"),
        &staging.join("usr/bin/make"),
    )?;
    copy_preserving(
        &repo_root.join("src/build-tools/make/COPYING"),
        &staging.join("usr/share/doc/mattos-make/copyright"),
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
    walk_tree(staging, &mut |path, metadata| {
        if metadata.is_file() && !path.starts_with(staging.join("DEBIAN")) {
            let header = Command::new("readelf").args(["-h"]).arg(path).output()?;
            if header.status.success() {
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
        if fs::symlink_metadata(&from)?.is_dir() {
            copy_tree_preserving(&from, &to)?;
        } else {
            copy_path_preserving(&from, &to)?;
        }
    }
    Ok(())
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
        "tar" => (Some("GNU tar"), "A", "mattos-tar", "medium", "high"),
        "libacl.so.1" => (
            Some("Linux ACL utilities"),
            "A",
            "mattos-libacl1",
            "low",
            "high",
        ),
        "libbsd.so.0" => (Some("libbsd"), "A", "mattos-libbsd0", "low", "high"),
        "libbz2.so.1.0" => (Some("bzip2"), "A", "mattos-libbz2-1.0", "low", "high"),
        "libc.so.6" | "libm.so.6" | "ld-linux-x86-64.so.2" => (
            Some("glibc"),
            "D",
            "future MattOS libc runtime",
            "very-high",
            "high",
        ),
        "libcap.so.2" => (Some("libcap"), "A", "mattos-libcap2", "low", "high"),
        "libcrypt.so.1" => (Some("libxcrypt"), "A", "mattos-libcrypt1", "medium", "high"),
        "libcrypto.so.3" => (Some("OpenSSL"), "A", "mattos-libcrypto3", "high", "high"),
        "libssl.so.3" => (Some("OpenSSL"), "A", "mattos-libssl3", "high", "high"),
        "libelf.so.1" => (Some("elfutils"), "A", "mattos-libelf1", "medium", "high"),
        "libexpat.so.1" => (Some("Expat"), "A", "mattos-libexpat1", "low", "high"),
        "libgcc_s.so.1" => (
            Some("GCC runtime"),
            "D",
            "future MattOS compiler runtime",
            "very-high",
            "high",
        ),
        "liblz4.so.1" => (Some("LZ4"), "C", "mattos-liblz4-1", "low", "high"),
        "liblzma.so.5" => (Some("XZ Utils"), "C", "mattos-liblzma5", "low", "high"),
        "libmd.so.0" => (Some("libmd"), "A", "mattos-libmd0", "low", "high"),
        "libpcre2-8.so.0" => (Some("PCRE2"), "A", "mattos-libpcre2-8-0", "medium", "high"),
        "libselinux.so.1" => (
            Some("SELinux userspace"),
            "A",
            "mattos-libselinux1",
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
        "libxxhash.so.0" => (Some("xxHash"), "C", "mattos-libxxhash0", "low", "high"),
        "libz.so.1" => (Some("zlib"), "A", "mattos-zlib1g", "low", "high"),
        "libzstd.so.1" => (Some("Zstandard"), "A", "mattos-libzstd1", "low", "high"),
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
    effective_dependencies(spec)
        .into_iter()
        .map(|dependency| {
            if dependency.starts_with("mattos-") {
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
    if spec.name != "mattos-filesystem"
        && spec.name != "mattos-libc6"
        && !dependencies.contains(&"mattos-libc6")
    {
        dependencies.insert(0, "mattos-libc6");
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
        "mattos-libc6" | "mattos-libc6-dev" | "mattos-libc-bin" => {
            component_snapshot_version(repo_root, "glibc")?
        }
        "mattos-linux-libc-dev" => component_snapshot_version(repo_root, "linux")?,
        "mattos-libgcc-s1"
        | "mattos-libstdc++6"
        | "mattos-libgcc-dev"
        | "mattos-libstdc++-dev"
        | "mattos-gcc-common"
        | "mattos-cpp"
        | "mattos-gcc"
        | "mattos-g++" => component_snapshot_version(repo_root, "gcc")?,
        "mattos-binutils" => component_snapshot_version(repo_root, "binutils")?,
        "mattos-make" => component_snapshot_version(repo_root, "make")?,
        "mattos-ca-certificates" => "2026.07.16".to_string(),
        "mattos-brush" => {
            cargo_package_version(&repo_root.join("src/userland/brush/brush/Cargo.toml"))?
        }
        "mattos-coreutils" => {
            cargo_workspace_version(&repo_root.join("src/userland/coreutils/Cargo.toml"))?
        }
        "mattos-curl" => curl_version(&repo_root.join("src/userland/curl/include/curl/curlver.h"))?,
        "mattos-dpkg" => fs::read_to_string(repo_root.join("out/build/dpkg/source/.dist-version"))?
            .trim()
            .to_string(),
        "mattos-libapt-pkg" | "mattos-apt" => apt_version(repo_root)?,
        "mattos-libtinfow6" | "mattos-libncursesw6" | "mattos-terminfo" | "mattos-ncurses-bin" => {
            component_snapshot_version(repo_root, "ncurses")?
        }
        "mattos-libkmod2" | "mattos-kmod" => component_snapshot_version(repo_root, "kmod")?,
        "mattos-libproc2" | "mattos-procps" => component_snapshot_version(repo_root, "procps-ng")?,
        "mattos-libsystemd0" | "mattos-libudev1" => {
            component_snapshot_version(repo_root, "systemd")?
        }
        "mattos-libexpat1" => component_snapshot_version(repo_root, "expat")?,
        "mattos-libcap2" => component_snapshot_version(repo_root, "libcap")?,
        "mattos-libacl1" => component_snapshot_version(repo_root, "acl")?,
        "mattos-zlib1g" => component_snapshot_version(repo_root, "zlib")?,
        "mattos-libbz2-1.0" => component_snapshot_version(repo_root, "bzip2")?,
        "mattos-liblz4-1" => component_snapshot_version(repo_root, "lz4")?,
        "mattos-liblzma5" => component_snapshot_version(repo_root, "xz")?,
        "mattos-libxxhash0" => component_snapshot_version(repo_root, "xxhash")?,
        "mattos-libmd0" => component_snapshot_version(repo_root, "libmd")?,
        "mattos-libbsd0" => component_snapshot_version(repo_root, "libbsd")?,
        "mattos-libzstd1" => component_snapshot_version(repo_root, "zstd")?,
        "mattos-libcrypto3" | "mattos-libssl3" => component_snapshot_version(repo_root, "openssl")?,
        "mattos-libelf1" => component_snapshot_version(repo_root, "elfutils")?,
        "mattos-libpcre2-8-0" => component_snapshot_version(repo_root, "pcre2")?,
        "mattos-libselinux1" => component_snapshot_version(repo_root, "selinux")?,
        "mattos-libcrypt1" => component_snapshot_version(repo_root, "libxcrypt")?,
        "mattos-tar" => component_snapshot_version(repo_root, "tar")?,
        "mattos-dbus-broker" => component_snapshot_version(repo_root, "dbus-broker")?,
        "mattos-libpam0" | "mattos-libpam-misc0" | "mattos-pam-modules" | "mattos-pam-runtime" => {
            component_snapshot_version(repo_root, "linux-pam")?
        }
        "mattos-shadow" => component_snapshot_version(repo_root, "shadow")?,
        "mattos-sudo-rs" => {
            cargo_package_version(&repo_root.join("src/system/auth/sudo-rs/Cargo.toml"))?
        }
        "mattos-libblkid1"
        | "mattos-libmount1"
        | "mattos-libsmartcols1"
        | "mattos-mount"
        | "mattos-util-linux-auth" => component_snapshot_version(repo_root, "util-linux")?,
        "mattos-iproute2" => component_snapshot_version(repo_root, "iproute2")?,
        "mattos-iputils" => component_snapshot_version(repo_root, "iputils")?,
        _ => bail!("unknown package {}", spec.name),
    };
    Ok(format!("{upstream}-{REVISION}"))
}

fn component_snapshot_version(repo_root: &Path, component: &str) -> Result<String> {
    let state = read_sync_state(repo_root, component)?
        .ok_or_else(|| anyhow!("upstream state missing for {component}"))?;
    let short = state
        .imported_commit
        .get(..12)
        .unwrap_or(&state.imported_commit);
    Ok(format!("0+git.{short}"))
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
    if !spec.provides.is_empty() {
        fields.push(format!("Provides: {}", spec.provides.join(", ")));
    }
    if !spec.conflicts.is_empty() {
        fields.push(format!("Conflicts: {}", spec.conflicts.join(", ")));
    }
    if !spec.replaces.is_empty() {
        fields.push(format!("Replaces: {}", spec.replaces.join(", ")));
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
            let invocation = if matches!(
                spec.name,
                "mattos-gcc-common" | "mattos-cpp" | "mattos-gcc" | "mattos-g++"
            ) {
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
        component @ ("glibc" | "ncurses" | "kmod" | "procps-ng" | "systemd" | "dbus-broker"
        | "linux-pam" | "shadow" | "sudo-rs" | "util-linux" | "iproute2"
        | "iputils" | "expat" | "libcap" | "acl" | "zlib" | "bzip2" | "lz4" | "xz"
        | "xxhash" | "zstd" | "openssl" | "elfutils" | "pcre2" | "selinux"
        | "libxcrypt" | "libmd" | "libbsd" | "tar") => {
            let state = read_sync_state(repo_root, component)?
                .ok_or_else(|| anyhow!("upstream state missing for {component}"))?;
            (
                state.destination_path,
                state.repo,
                state.imported_commit,
                format!("MattOS source build output in out/build/{component}/install"),
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
            &repo_root.join("src/userland/brush/target/release/brush"),
            None,
        ),
        "mattos-coreutils" => ldd_sonames(&resolve_coreutils_multicall(repo_root)?, None),
        "mattos-curl" => {
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
            "mattos-libc6"
                | "mattos-libc-bin"
                | "mattos-libgcc-s1"
                | "mattos-libstdc++6"
                | "mattos-binutils"
                | "mattos-gcc-common"
                | "mattos-cpp"
                | "mattos-gcc"
                | "mattos-g++"
                | "mattos-make"
        ) =>
        {
            runtime_libraries_in_staging(repo_root, name)
        }
        "mattos-dpkg" => {
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
        "mattos-libapt-pkg" => {
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
        "mattos-apt" => {
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
        name if matches!(
            name,
            "mattos-libtinfow6"
                | "mattos-libncursesw6"
                | "mattos-ncurses-bin"
                | "mattos-libkmod2"
                | "mattos-kmod"
                | "mattos-libproc2"
                | "mattos-procps"
                | "mattos-libsystemd0"
                | "mattos-libudev1"
                | "mattos-libexpat1"
                | "mattos-libcap2"
                | "mattos-libacl1"
                | "mattos-zlib1g"
                | "mattos-libbz2-1.0"
                | "mattos-liblz4-1"
                | "mattos-liblzma5"
                | "mattos-libxxhash0"
                | "mattos-libmd0"
                | "mattos-libbsd0"
                | "mattos-libzstd1"
                | "mattos-libcrypto3"
                | "mattos-libssl3"
                | "mattos-libelf1"
                | "mattos-libpcre2-8-0"
                | "mattos-libselinux1"
                | "mattos-libcrypt1"
                | "mattos-libblkid1"
                | "mattos-libmount1"
                | "mattos-libsmartcols1"
                | "mattos-mount"
                | "mattos-tar"
                | "mattos-dbus-broker"
                | "mattos-libpam0"
                | "mattos-libpam-misc0"
                | "mattos-pam-modules"
                | "mattos-pam-runtime"
                | "mattos-shadow"
                | "mattos-sudo-rs"
                | "mattos-util-linux-auth"
                | "mattos-iproute2"
                | "mattos-iputils"
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
    walk_tree(root, &mut |path, meta| {
        if meta.is_file() && !path.starts_with(root.join("DEBIAN")) {
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
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn verify_deb(path: &Path, expected_name: &str, expected_version: &str) -> Result<()> {
    for (field, expected) in [
        ("Package", expected_name),
        ("Version", expected_version),
        ("Architecture", ARCH),
    ] {
        let info = Command::new("dpkg-deb")
            .args(["--field", path_str(path)?, field])
            .output()
            .context("failed to inspect package metadata")?;
        if !info.status.success() {
            bail!("dpkg-deb --field failed for {}", path.display());
        }
        if String::from_utf8(info.stdout)?.trim() != expected {
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
    fs::write(path, toml::to_string_pretty(inventory)?)?;
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
        repo_root.join("out/repository/dists/mattos/main/binary-amd64/Packages");
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
    let repository = repo_root.join("out/repository");
    remove_path_if_exists(&repository)?;
    let pool = repository.join("pool/main");
    let index_dir = repository.join("dists/mattos/main/binary-amd64");
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
            "APT::FTPArchive::Release::Label=MattOS",
            "-o",
            "APT::FTPArchive::Release::Suite=mattos",
            "-o",
            "APT::FTPArchive::Release::Codename=mattos",
            "-o",
            "APT::FTPArchive::Release::Architectures=amd64",
            "-o",
            "APT::FTPArchive::Release::Components=main",
            "-o",
            "APT::FTPArchive::Release::Description=Local MattOS bootstrap repository",
            "release",
            "dists/mattos",
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
    fs::write(repository.join("dists/mattos/Release"), release_body)?;
    validate_repository(&repository)?;
    println!(
        "generated local MattOS repository at {}",
        repository.display()
    );
    Ok(())
}

fn validate_repository(repository: &Path) -> Result<()> {
    let packages = fs::read_to_string(repository.join("dists/mattos/main/binary-amd64/Packages"))?;
    if packages.contains("deb.debian.org") || packages.contains("archive.ubuntu.com") {
        bail!("foreign repository URL found in Packages");
    }
    validate_repository_packages(&packages)?;
    let release = fs::read_to_string(repository.join("dists/mattos/Release"))?;
    for field in [
        "Origin: MattOS",
        "Suite: mattos",
        "Codename: mattos",
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
    build_all_packages(repo_root)?;
    generate_repository(repo_root)?;
    let inventory = read_inventory(repo_root)?;
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
        .args(["--force-bad-path", "--install"]);
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
        ("/usr/bin/curl", "mattos-curl"),
        ("/usr/bin/ls", "mattos-coreutils"),
        ("/usr/bin/tar", "mattos-tar"),
        ("/usr/bin/dpkg", "mattos-dpkg"),
        ("/usr/bin/apt", "mattos-apt"),
        ("/usr/bin/apt-get", "mattos-apt"),
        ("/usr/bin/ldd", "mattos-libc-bin"),
        ("/usr/lib/apt/methods/file", "mattos-apt"),
        (
            "/usr/lib/x86_64-linux-gnu/libapt-pkg.so.7.0",
            "mattos-libapt-pkg",
        ),
        (
            "/usr/lib/x86_64-linux-gnu/libgcc_s.so.1",
            "mattos-libgcc-s1",
        ),
        ("/usr/lib/x86_64-linux-gnu/libgcc_s.so", "mattos-libgcc-dev"),
        (
            "/usr/lib/x86_64-linux-gnu/libstdc++.so.6",
            "mattos-libstdc++6",
        ),
        (
            "/etc/ssl/certs/ca-certificates.crt",
            "mattos-ca-certificates",
        ),
        ("/usr/lib/x86_64-linux-gnu/libpam.so.0", "mattos-libpam0"),
        (
            "/usr/lib/x86_64-linux-gnu/libncursesw.so.6",
            "mattos-libncursesw6",
        ),
        ("/usr/lib/x86_64-linux-gnu/libkmod.so.2", "mattos-libkmod2"),
        ("/usr/lib/x86_64-linux-gnu/libproc2.so.1", "mattos-libproc2"),
        (
            "/usr/lib/x86_64-linux-gnu/libexpat.so.1",
            "mattos-libexpat1",
        ),
        ("/usr/lib/x86_64-linux-gnu/libcap.so.2", "mattos-libcap2"),
        (
            "/usr/lib/x86_64-linux-gnu/libpcre2-8.so.0",
            "mattos-libpcre2-8-0",
        ),
        (
            "/usr/lib/x86_64-linux-gnu/libselinux.so.1",
            "mattos-libselinux1",
        ),
        (
            "/usr/lib/x86_64-linux-gnu/libcrypt.so.1",
            "mattos-libcrypt1",
        ),
        ("/usr/lib/x86_64-linux-gnu/libacl.so.1", "mattos-libacl1"),
        ("/usr/lib/x86_64-linux-gnu/libz.so.1", "mattos-zlib1g"),
        (
            "/usr/lib/x86_64-linux-gnu/libbz2.so.1.0",
            "mattos-libbz2-1.0",
        ),
        ("/usr/lib/x86_64-linux-gnu/liblz4.so.1", "mattos-liblz4-1"),
        ("/usr/lib/x86_64-linux-gnu/liblzma.so.5", "mattos-liblzma5"),
        (
            "/usr/lib/x86_64-linux-gnu/libxxhash.so.0",
            "mattos-libxxhash0",
        ),
        ("/usr/lib/x86_64-linux-gnu/libmd.so.0", "mattos-libmd0"),
        ("/usr/lib/x86_64-linux-gnu/libbsd.so.0", "mattos-libbsd0"),
        ("/usr/bin/dbus-broker", "mattos-dbus-broker"),
        ("/usr/bin/sudo", "mattos-sudo-rs"),
        ("/usr/bin/passwd", "mattos-shadow"),
        ("/usr/bin/login", "mattos-util-linux-auth"),
        ("/usr/sbin/ip", "mattos-iproute2"),
        ("/usr/bin/ping", "mattos-iputils"),
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
    if !source.join("dists/mattos/Release").is_file() {
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
    sync_build_source(&source, &source_copy)?;
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
            "-DCMAKE_INSTALL_PREFIX=/usr",
            "-DCMAKE_INSTALL_SYSCONFDIR=/etc",
            "-DCURRENT_VENDOR=mattos",
            "-DCOMMON_ARCH=amd64",
            "-DDPKG_DATADIR=/usr/share/dpkg",
            "-DWITH_DOC=OFF",
            "-DWITH_TESTS=OFF",
            "-DWITH_FTPARCHIVE=OFF",
            "-DUSE_NLS=OFF",
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
                let apt_extra = if *name == "mattos-apt" {
                    extra_apt_field.unwrap_or("")
                } else {
                    ""
                };
                let provides = if *name == "mattos-libc6" {
                    "Provides: mattos-runtime-abi\n"
                } else {
                    ""
                };
                format!("Package: {name}\nVersion: 1\nArchitecture: amd64\n{provides}{apt_extra}\n")
            })
            .collect()
    }

    #[test]
    fn validates_package_names_versions_and_architecture() {
        assert!(validate_package_name("mattos-coreutils").is_ok());
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
            .find(|s| s.name == "mattos-curl")
            .unwrap();
        let control = render_control(
            &spec,
            "8.22.0-1mattos1",
            42,
            &["mattos-libc6 (= 2.43-1mattos1)".into()],
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
            "Provides:",
            "Conflicts:",
            "Replaces:",
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
            "mattos-dpkg",
            "mattos-libapt-pkg",
            "mattos-apt",
            "mattos-ca-certificates",
            "mattos-libgcc-s1",
            "mattos-libstdc++6",
        ] {
            assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
        }
        let filesystem = specs
            .iter()
            .find(|spec| spec.name == "mattos-filesystem")
            .unwrap();
        let dpkg = specs
            .iter()
            .find(|spec| spec.name == "mattos-dpkg")
            .unwrap();
        assert!(filesystem.essential);
        assert_eq!(filesystem.priority, "required");
        assert!(!dpkg.essential);
        assert_eq!(dpkg.priority, "required");
        assert!(DPKG_RUNTIME_PATHS.contains(&"usr/bin/update-alternatives"));
        assert!(DPKG_RUNTIME_PATHS.contains(&"usr/sbin/start-stop-daemon"));
        assert!(APT_RUNTIME_PATHS.contains(&"usr/lib/apt/methods/file"));
        assert!(
            !specs
                .iter()
                .any(|spec| spec.name == "mattos-bootstrap-runtime")
        );
    }

    #[test]
    fn third_milestone_package_families_are_complete() {
        let specs = package_specs();
        for name in [
            "mattos-libtinfow6",
            "mattos-libncursesw6",
            "mattos-terminfo",
            "mattos-ncurses-bin",
            "mattos-libkmod2",
            "mattos-kmod",
            "mattos-libproc2",
            "mattos-procps",
            "mattos-libsystemd0",
            "mattos-libudev1",
            "mattos-dbus-broker",
            "mattos-libpam0",
            "mattos-libpam-misc0",
            "mattos-pam-modules",
            "mattos-pam-runtime",
            "mattos-shadow",
            "mattos-sudo-rs",
            "mattos-util-linux-auth",
            "mattos-libblkid1",
            "mattos-libmount1",
            "mattos-libsmartcols1",
            "mattos-mount",
            "mattos-iproute2",
            "mattos-iputils",
        ] {
            assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
        }
        assert_eq!(PACKAGE_NAMES.len(), 65);
    }

    #[test]
    fn small_library_migration_definitions_are_complete() {
        let specs = package_specs();
        let expat = specs
            .iter()
            .find(|spec| spec.name == "mattos-libexpat1")
            .unwrap();
        let libcap = specs
            .iter()
            .find(|spec| spec.name == "mattos-libcap2")
            .unwrap();
        let broker = specs
            .iter()
            .find(|spec| spec.name == "mattos-dbus-broker")
            .unwrap();
        let iproute2 = specs
            .iter()
            .find(|spec| spec.name == "mattos-iproute2")
            .unwrap();
        let acl = specs
            .iter()
            .find(|spec| spec.name == "mattos-libacl1")
            .unwrap();
        let zlib = specs
            .iter()
            .find(|spec| spec.name == "mattos-zlib1g")
            .unwrap();
        let bzip2 = specs
            .iter()
            .find(|spec| spec.name == "mattos-libbz2-1.0")
            .unwrap();
        let lz4 = specs
            .iter()
            .find(|spec| spec.name == "mattos-liblz4-1")
            .unwrap();
        let xz = specs
            .iter()
            .find(|spec| spec.name == "mattos-liblzma5")
            .unwrap();
        let xxhash = specs
            .iter()
            .find(|spec| spec.name == "mattos-libxxhash0")
            .unwrap();
        let libmd = specs
            .iter()
            .find(|spec| spec.name == "mattos-libmd0")
            .unwrap();
        let libbsd = specs
            .iter()
            .find(|spec| spec.name == "mattos-libbsd0")
            .unwrap();
        let zstd = specs
            .iter()
            .find(|spec| spec.name == "mattos-libzstd1")
            .unwrap();
        let crypto = specs
            .iter()
            .find(|spec| spec.name == "mattos-libcrypto3")
            .unwrap();
        let ssl = specs
            .iter()
            .find(|spec| spec.name == "mattos-libssl3")
            .unwrap();
        let elf = specs
            .iter()
            .find(|spec| spec.name == "mattos-libelf1")
            .unwrap();
        let shadow = specs
            .iter()
            .find(|spec| spec.name == "mattos-shadow")
            .unwrap();
        let tar = specs.iter().find(|spec| spec.name == "mattos-tar").unwrap();
        let dpkg = specs
            .iter()
            .find(|spec| spec.name == "mattos-dpkg")
            .unwrap();
        let apt = specs
            .iter()
            .find(|spec| spec.name == "mattos-libapt-pkg")
            .unwrap();
        assert_eq!(expat.source_component, "expat");
        assert_eq!(libcap.source_component, "libcap");
        assert!(broker.depends.contains(&"mattos-libexpat1"));
        assert!(iproute2.depends.contains(&"mattos-libcap2"));
        assert!(iproute2.depends.contains(&"mattos-zlib1g"));
        assert_eq!(acl.source_component, "acl");
        assert_eq!(zlib.source_component, "zlib");
        assert_eq!(bzip2.source_component, "bzip2");
        assert_eq!(lz4.source_component, "lz4");
        assert_eq!(xz.source_component, "xz");
        assert_eq!(xxhash.source_component, "xxhash");
        assert_eq!(libmd.source_component, "libmd");
        assert_eq!(libbsd.source_component, "libbsd");
        assert!(libbsd.depends.contains(&"mattos-libmd0"));
        assert_eq!(zstd.source_component, "zstd");
        assert_eq!(crypto.source_component, "openssl");
        assert!(crypto.depends.contains(&"mattos-libzstd1"));
        assert_eq!(ssl.source_component, "openssl");
        assert!(ssl.depends.contains(&"mattos-libcrypto3"));
        assert_eq!(elf.source_component, "elfutils");
        assert!(elf.depends.contains(&"mattos-libzstd1"));
        assert!(shadow.depends.contains(&"mattos-libbsd0"));
        assert!(shadow.depends.contains(&"mattos-libmd0"));
        assert_eq!(tar.source_component, "tar");
        assert!(tar.depends.contains(&"mattos-libacl1"));
        assert_eq!(tar.provides, &["tar"]);
        assert_eq!(tar.conflicts, &["tar"]);
        assert_eq!(tar.replaces, &["tar"]);
        assert!(dpkg.depends.contains(&"mattos-tar"));
        assert!(dpkg.depends.contains(&"mattos-zlib1g"));
        assert!(dpkg.depends.contains(&"mattos-libbz2-1.0"));
        assert!(dpkg.depends.contains(&"mattos-liblzma5"));
        assert!(dpkg.depends.contains(&"mattos-libzstd1"));
        assert!(dpkg.depends.contains(&"mattos-libmd0"));
        assert!(apt.depends.contains(&"mattos-zlib1g"));
        assert!(apt.depends.contains(&"mattos-libbz2-1.0"));
        assert!(apt.depends.contains(&"mattos-liblz4-1"));
        assert!(apt.depends.contains(&"mattos-liblzma5"));
        assert!(apt.depends.contains(&"mattos-libxxhash0"));
        assert!(apt.depends.contains(&"mattos-libzstd1"));
        assert!(apt.depends.contains(&"mattos-libcrypto3"));
        let apt_cli = specs.iter().find(|spec| spec.name == "mattos-apt").unwrap();
        assert!(apt_cli.depends.contains(&"mattos-zlib1g"));
        assert!(apt_cli.depends.contains(&"mattos-libbz2-1.0"));
        assert!(apt_cli.depends.contains(&"mattos-liblz4-1"));
        assert!(apt_cli.depends.contains(&"mattos-liblzma5"));
        assert!(apt_cli.depends.contains(&"mattos-libxxhash0"));
        assert!(apt_cli.depends.contains(&"mattos-libzstd1"));
        assert!(apt_cli.depends.contains(&"mattos-libcrypto3"));
        let curl = specs
            .iter()
            .find(|spec| spec.name == "mattos-curl")
            .unwrap();
        assert!(curl.depends.contains(&"mattos-zlib1g"));
        assert!(curl.depends.contains(&"mattos-libzstd1"));
        assert!(curl.depends.contains(&"mattos-libcrypto3"));
        assert!(curl.depends.contains(&"mattos-libssl3"));
        assert_eq!(
            MIGRATED_BOOTSTRAP_SONAME_PREFIXES,
            &[
                "libc.so",
                "libm.so",
                "ld-linux-",
                "libexpat.so",
                "libcap.so",
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
        assert!(position("mattos-libc6") < position("mattos-libzstd1"));
        assert!(position("mattos-libzstd1") < position("mattos-libcrypto3"));
        assert!(position("mattos-libzstd1") < position("mattos-libelf1"));
        assert!(position("mattos-libcrypto3") < position("mattos-libssl3"));
        assert!(position("mattos-libssl3") < position("mattos-curl"));
    }

    #[test]
    fn pcre2_selinux_libxcrypt_graph_is_active_and_acyclic() {
        let specs = package_specs();
        let spec = |name| specs.iter().find(|spec| spec.name == name).unwrap();
        assert!(
            spec("mattos-libselinux1")
                .depends
                .contains(&"mattos-libpcre2-8-0")
        );
        assert!(spec("mattos-dpkg").depends.contains(&"mattos-libselinux1"));
        assert!(
            spec("mattos-iproute2")
                .depends
                .contains(&"mattos-libselinux1")
        );
        assert!(
            spec("mattos-pam-modules")
                .depends
                .contains(&"mattos-libcrypt1")
        );
        assert!(
            spec("mattos-pam-runtime")
                .depends
                .contains(&"mattos-libcrypt1")
        );
        assert!(spec("mattos-shadow").depends.contains(&"mattos-libcrypt1"));
        assert!(
            spec("mattos-libmount1")
                .depends
                .contains(&"mattos-libblkid1")
        );
        assert!(spec("mattos-mount").depends.contains(&"mattos-libmount1"));
        assert!(
            spec("mattos-mount")
                .depends
                .contains(&"mattos-libsmartcols1")
        );
        assert!(spec("mattos-mount").depends.contains(&"mattos-libselinux1"));
        for prefix in ["libpcre2-8.so", "libselinux.so", "libcrypt.so"] {
            assert!(MIGRATED_BOOTSTRAP_SONAME_PREFIXES.contains(&prefix));
        }
        assert_eq!(
            package_install_order_for(&specs, PACKAGE_NAMES)
                .unwrap()
                .len(),
            65
        );
    }

    #[test]
    fn zstd_cycle_design_is_rejected() {
        let specs = [
            PackageSpec {
                name: "mattos-bootstrap-runtime",
                description: "test bootstrap",
                source_component: "test",
                depends: &["mattos-libzstd1"],
                provides: &[],
                conflicts: &[],
                replaces: &[],
                essential: false,
                priority: "required",
            },
            PackageSpec {
                name: "mattos-libzstd1",
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
        let error =
            package_install_order_for(&specs, &["mattos-bootstrap-runtime", "mattos-libzstd1"])
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
        let libc = specs
            .iter()
            .find(|spec| spec.name == "mattos-libc6")
            .unwrap();
        let libc_bin = specs
            .iter()
            .find(|spec| spec.name == "mattos-libc-bin")
            .unwrap();
        let libgcc = specs
            .iter()
            .find(|spec| spec.name == "mattos-libgcc-s1")
            .unwrap();
        let libstdcxx = specs
            .iter()
            .find(|spec| spec.name == "mattos-libstdc++6")
            .unwrap();
        assert_eq!(libc.depends, &["mattos-filesystem"]);
        assert!(libc_bin.depends.contains(&"mattos-libc6"));
        assert!(libgcc.depends.contains(&"mattos-libc6"));
        assert!(libstdcxx.depends.contains(&"mattos-libgcc-s1"));
        assert!(!libc.depends.contains(&"mattos-bootstrap-runtime"));
        let order = package_install_order_for(&specs, PACKAGE_NAMES).unwrap();
        let position = |name: &str| order.iter().position(|entry| *entry == name).unwrap();
        assert!(position("mattos-filesystem") < position("mattos-libc6"));
        assert!(position("mattos-libc6") < position("mattos-libgcc-s1"));
        assert!(position("mattos-libgcc-s1") < position("mattos-libstdc++6"));
        assert_eq!(order.len(), 65);
    }

    #[test]
    fn gcc_runtime_packages_are_minimal_acyclic_and_replace_bootstrap() {
        let specs = package_specs();
        let spec = |name| specs.iter().find(|spec| spec.name == name).unwrap();
        let libgcc = spec("mattos-libgcc-s1");
        let libstdcxx = spec("mattos-libstdc++6");
        assert_eq!(libgcc.source_component, "gcc");
        assert_eq!(libgcc.depends, &["mattos-filesystem", "mattos-libc6"]);
        assert_eq!(libgcc.provides, &["libgcc-s1"]);
        assert_eq!(libstdcxx.source_component, "gcc");
        assert_eq!(
            libstdcxx.depends,
            &["mattos-filesystem", "mattos-libc6", "mattos-libgcc-s1"]
        );
        assert_eq!(libstdcxx.provides, &["libstdc++6"]);
        assert!(!PACKAGE_NAMES.contains(&"mattos-bootstrap-runtime"));
        assert!(specs.iter().all(|spec| {
            !spec.depends.contains(&"mattos-bootstrap-runtime")
                && !spec.depends.contains(&"mattos-bootstrap-gcc-runtime")
        }));
        let order = package_install_order_for(&specs, PACKAGE_NAMES).unwrap();
        let position = |name| order.iter().position(|item| *item == name).unwrap();
        assert!(position("mattos-libc6") < position("mattos-libgcc-s1"));
        assert!(position("mattos-libgcc-s1") < position("mattos-libstdc++6"));
    }

    #[test]
    fn native_development_package_graph_has_explicit_owners() {
        let specs = package_specs();
        let spec = |name| specs.iter().find(|spec| spec.name == name).unwrap();
        for name in [
            "mattos-linux-libc-dev",
            "mattos-libc6-dev",
            "mattos-libgcc-dev",
            "mattos-libstdc++-dev",
            "mattos-binutils",
            "mattos-gcc-common",
            "mattos-cpp",
            "mattos-gcc",
            "mattos-g++",
            "mattos-make",
        ] {
            assert!(PACKAGE_NAMES.contains(&name), "missing package {name}");
        }
        assert!(
            spec("mattos-libc6-dev")
                .depends
                .contains(&"mattos-linux-libc-dev")
        );
        assert!(
            spec("mattos-libstdc++-dev")
                .depends
                .contains(&"mattos-libgcc-dev")
        );
        assert!(spec("mattos-gcc").depends.contains(&"mattos-gcc-common"));
        assert!(spec("mattos-g++").depends.contains(&"mattos-gcc"));
        let order = package_install_order_for(&specs, PACKAGE_NAMES).unwrap();
        let position = |name| order.iter().position(|item| *item == name).unwrap();
        assert!(position("mattos-linux-libc-dev") < position("mattos-libc6-dev"));
        assert!(position("mattos-libc6-dev") < position("mattos-libgcc-dev"));
        assert!(position("mattos-libgcc-dev") < position("mattos-libstdc++-dev"));
        assert!(position("mattos-binutils") < position("mattos-gcc-common"));
        assert!(position("mattos-gcc-common") < position("mattos-gcc"));
        assert!(position("mattos-gcc") < position("mattos-g++"));
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
        let source = repo.join("src/userland/brush/target/release/brush");
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
            dependency_name("mattos-libapt-pkg (= 3.3.2-1mattos1)").unwrap(),
            "mattos-libapt-pkg"
        );
        assert_eq!(
            exact_dependency_version("mattos-libapt-pkg (= 3.3.2-1mattos1)").unwrap(),
            Some("3.3.2-1mattos1")
        );
        assert!(exact_dependency_version("mattos-libapt-pkg (>= 3)").is_err());
        let body = repository_packages(Some("Depends: mattos-runtime-abi\n"));
        assert!(validate_repository_packages(&body).is_ok());
    }

    #[test]
    fn repository_dependency_closure_rejects_missing_and_wrong_exact_versions() {
        assert!(
            validate_repository_packages(&repository_packages(Some(
                "Depends: mattos-libapt-pkg (= 1)\n"
            )))
            .is_ok()
        );
        assert!(
            validate_repository_packages(&repository_packages(Some("Depends: mattos-missing\n")))
                .is_err()
        );
        assert!(
            validate_repository_packages(&repository_packages(Some(
                "Depends: mattos-libapt-pkg (= 2)\n"
            )))
            .is_err()
        );
    }

    #[test]
    fn repository_rejects_duplicate_package_version_architecture() {
        let mut body = repository_packages(None);
        body.push_str("Package: mattos-apt\nVersion: 1\nArchitecture: amd64\n\n");
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
        assert_eq!(APT_CONFFILES.len(), 2);
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
        assert!(position("mattos-filesystem") < position("mattos-libc6"));
        assert!(position("mattos-libc6") < position("mattos-libgcc-s1"));
        assert!(position("mattos-libgcc-s1") < position("mattos-libstdc++6"));
        assert!(position("mattos-libstdc++6") < position("mattos-apt"));
        assert!(position("mattos-dpkg") < position("mattos-apt"));
        assert!(position("mattos-libapt-pkg") < position("mattos-apt"));
        assert!(position("mattos-libudev1") < position("mattos-libapt-pkg"));
        assert!(position("mattos-libexpat1") < position("mattos-dbus-broker"));
        assert!(position("mattos-libcap2") < position("mattos-iproute2"));
        assert!(position("mattos-libpcre2-8-0") < position("mattos-libselinux1"));
        assert!(position("mattos-libselinux1") < position("mattos-iproute2"));
        assert!(position("mattos-libselinux1") < position("mattos-dpkg"));
        assert!(position("mattos-libcrypt1") < position("mattos-pam-modules"));
        assert!(position("mattos-libcrypt1") < position("mattos-shadow"));
        assert!(position("mattos-libblkid1") < position("mattos-libmount1"));
        assert!(position("mattos-libmount1") < position("mattos-mount"));
        assert!(position("mattos-libsmartcols1") < position("mattos-mount"));
        assert!(position("mattos-libmd0") < position("mattos-libbsd0"));
        assert!(position("mattos-libbsd0") < position("mattos-shadow"));
        assert!(position("mattos-libmd0") < position("mattos-dpkg"));
        assert!(position("mattos-libpam0") < position("mattos-pam-runtime"));
        assert!(position("mattos-pam-runtime") < position("mattos-util-linux-auth"));
        assert!(position("mattos-libtinfow6") < position("mattos-ncurses-bin"));
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
            .filter(|spec| matches!(spec.name, "mattos-libexpat1" | "mattos-libcap2"))
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
        let index = temp.path().join("dists/mattos/main/binary-amd64");
        fs::create_dir_all(&index).unwrap();
        let packages = PACKAGE_NAMES
            .iter()
            .map(|name| format!("Package: {name}\nVersion: 1\nArchitecture: amd64\n\n"))
            .collect::<String>();
        fs::write(index.join("Packages"), packages).unwrap();
        fs::write(temp.path().join("dists/mattos/Release"), "Origin: MattOS\nSuite: mattos\nCodename: mattos\nArchitectures: amd64\nComponents: main\nSHA256:\n").unwrap();
        assert!(validate_repository(temp.path()).is_ok());
        fs::write(
            index.join("Packages"),
            "Package: foreign\nHomepage: https://deb.debian.org\n",
        )
        .unwrap();
        assert!(validate_repository(temp.path()).is_err());
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
}
