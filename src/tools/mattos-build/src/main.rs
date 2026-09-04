use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use sha2::{Digest as ShaDigest, Sha256 as Sha256Hasher};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[cfg(test)]
mod build_system_tests;
mod cache_manifest;
mod elf_cache;
mod integrity_index;
mod packaging;
mod performance;
mod resources;
mod scheduler;
mod source_identity;
mod stage_cache;
mod stage_graph;
mod stage_inputs;
mod timing;
mod tool_identity;

use stage_graph::BuildStage;

thread_local! {
    static EXPERIMENTAL_CHILD_JOBS: Cell<Option<usize>> = const { Cell::new(None) };
}

const AUTHORITATIVE_GRUB_CFG: &str = stage_inputs::AUTHORITATIVE_GRUB_CFG;
const OBSOLETE_GRUB_CFG_PATHS: &[&str] = &["boot/grub/grub.cfg"];
const EXECUTABLE_PROBE_ID: &str = "mattos-build-probe-20260809T180000Z-info-normalization";
const GRUB_SYSTEMD_ENTRY: &str = "menuentry \"Start MattOS Live\"";
const GRUB_RESCUE_ENTRY: &str = "menuentry \"MattOS Rescue\"";
const INITRAMFS_ARCHIVE_PATH: &str = "out/build/early-initramfs.cpio.xz";
const LIVE_ROOT_IMAGE_PATH: &str = "out/build/live-root.squashfs";
const LIVE_ROOT_SQUASHFS_COMPRESSION: &str = "zstd";
const LIVE_ROOT_SQUASHFS_LEVEL: &str = "12";
const INSTALLED_INITRAMFS_PATH: &str = "out/build/installed-initramfs.cpio.xz";
const FINAL_ISO_PATH: &str = "out/images/mattos-x86_64.iso";
const ARTIFACT_REPORT_PATH: &str = "out/reports/artifacts.tsv";
const OBSOLETE_FULL_ROOT_INITRAMFS_PATHS: &[&str] = &[
    "out/build/initramfs.cpio.xz",
    "out/build/initramfs.cpio.gz",
    "out/build/initramfs.cpio.zst",
    "out/build/initramfs-compression-probe.cpio.zst",
    "out/build/initramfs-compression-probe-level10.cpio.zst",
];
const EARLY_INITRAMFS_SIZE_LIMIT: u64 = 32 * 1024 * 1024;
const GRUB_EARLY_RDINIT: &str = "rdinit=/init";
const GRUB_RESCUE_MARKER: &str = "mattos.rescue=1";
const SAFE_IMPORT_PLACEHOLDER_FILES: &[&str] = &[".gitkeep", "README.md"];
// These legacy skeleton files are installed after package payloads.  Keep the
// list explicit: package-owned files are protected from accidental overwrite.
const LEGACY_SKELETON_FILES: &[&str] = &[
    "README.md",
    "etc/group",
    "etc/inittab",
    "etc/passwd",
    "usr/libexec/mattos/brush-login",
    "usr/libexec/mattos/validate-shell-env",
];
const IMPORTED_TREE_DIGEST_ALGORITHM: &str = "sha256-git-ls-tree-no-gitlinks-v1";
const SELECTED_IMPORTED_TREE_DIGEST_ALGORITHM: &str = "sha256-selected-git-ls-tree-no-gitlinks-v1";
const USERLAND_INVENTORY_PATH: &str = "usr/share/mattos/userland-commands.txt";
const INITRAMFS_ARCHIVE_OWNER: &str = "0:0";
const MATTOS_BUILD_TMP_RELATIVE: &str = "out/tmp";
const MIN_MATTOS_TMP_FREE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
static MATTOS_TMP_PROBE_SEQUENCE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[derive(Debug, Deserialize)]
struct NvidiaDriverManifest {
    schema_version: u32,
    version: String,
    release_branch: String,
    architecture: String,
    runfile: String,
    url: String,
    sha256: String,
    license_sha256: String,
    kernel_source_commit: String,
    binary_policy: String,
    include_in_iso: bool,
}

const COREUTILS_PROVIDER: &str = "uutils/coreutils";
const GREP_PROVIDER: &str = "uutils/grep";
const SED_PROVIDER: &str = "uutils/sed";
const FINDUTILS_PROVIDER: &str = "uutils/findutils";
const DIFFUTILS_PROVIDER: &str = "uutils/diffutils";
const UTIL_LINUX_PROVIDER: &str = "util-linux";
const LINUX_PAM_PROVIDER: &str = "linux-pam";
const SHADOW_PROVIDER: &str = "shadow";
const SHADOW_UPSTREAM_COMMIT: &str = "855d15a04625818fa28a94e693dd4dc7acfb5af3";
const SHADOW_UPSTREAM_REPOSITORY: &str = "https://github.com/shadow-maint/shadow.git";
const SHADOW_MAN_PO_MAKEFILE_SHA256: &str =
    "344cedf9e4556d00918a70b37d109b572186bbd8ba85271122cf150976572037";
// The imported Attr checkout is the peeled v2.6.0 tag, which deliberately
// omits Autotools-generated distribution inputs.  Savannah's signed release
// archive is the authoritative source for those inputs at this exact commit.
// Keep both the retrieval and extraction strictly inside the output tree.
const ATTR_UPSTREAM_COMMIT: &str = "c440855d6b33446edf4b5eb1a2d892281f15a99b";
const ATTR_RELEASE_DIRECTORY: &str = "attr-2.6.0";
const ATTR_RELEASE_ARCHIVE_URL: &str =
    "https://download.savannah.gnu.org/releases/attr/attr-2.6.0.tar.xz";
const ATTR_RELEASE_ARCHIVE_SHA256: &str =
    "6c8a2148a7b85043b68492bce43316b0e2e214fc4e628c7ede078e76e216330b";
const ACL_RELEASE_DIRECTORY: &str = "acl-2.3.2";
const ACL_RELEASE_ARCHIVE_URL: &str =
    "https://download.savannah.gnu.org/releases/acl/acl-2.3.2.tar.xz";
const ACL_RELEASE_ARCHIVE_SHA256: &str =
    "97203a72cae99ab89a067fe2210c1cbf052bc492b479eca7d226d9830883b0bd";
const GZIP_RELEASE_ARCHIVE_URL: &str = "https://ftp.gnu.org/gnu/gzip/gzip-1.14.tar.xz";
const GZIP_RELEASE_ARCHIVE_SHA256: &str =
    "01a7b881bd220bfdf615f97b8718f80bdfd3f6add385b993dcf6efd14e8c0ac6";
const PATCH_RELEASE_ARCHIVE_URL: &str = "https://ftp.gnu.org/gnu/patch/patch-2.8.tar.xz";
const PATCH_RELEASE_ARCHIVE_SHA256: &str =
    "f87cee69eec2b4fcbf60a396b030ad6aa3415f192aa5f7ee84cad5e11f7f5ae3";
const RUST_RELEASE_ARCHIVE_URL: &str = "https://static.rust-lang.org/dist/rustc-1.97.1-src.tar.xz";
const RUST_RELEASE_ARCHIVE_SHA256: &str =
    "0ed06fdaffd4722a7702e0b4eebfafc897ab8f513e8e1b247cdd7e5c6df6ded2";
const MATTOS_GCC_INSTALL_DIR: &str = "/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0";
const LESS_RELEASE_ARCHIVE_URL: &str = "https://www.greenwoodsoftware.com/less/less-704.tar.gz";
const LESS_RELEASE_ARCHIVE_SHA256: &str =
    "20a0b0a2bb2525fa53c7eee9beb854b4c9cf172eabb209af7020743547bfe9fb";
const SUDO_RS_PROVIDER: &str = "sudo-rs";
const KMOD_PROVIDER: &str = "kmod";
const PROCPS_PROVIDER: &str = "procps-ng";
const NCURSES_PROVIDER: &str = "ncurses";
const IPROUTE2_PROVIDER: &str = "iproute2";
const IPUTILS_PROVIDER: &str = "iputils";
const CURL_PROVIDER: &str = "curl";
const GZIP_PROVIDER: &str = "gzip";
const BZIP2_PROVIDER: &str = "bzip2";
const XZ_PROVIDER: &str = "xz";
const ZSTD_PROVIDER: &str = "zstd";
const PATCH_PROVIDER: &str = "patch";
const FILE_PROVIDER: &str = "file";
const LESS_PROVIDER: &str = "less";
const GIT_PROVIDER: &str = "git";
const OPENSSH_PROVIDER: &str = "openssh";
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

const UTIL_LINUX_BASE_BINARIES: &[ComponentBinarySpec] = &[
    component_binary("usr/bin/lsblk", "lsblk"),
    component_binary("usr/bin/dmesg", "dmesg"),
    component_binary("usr/sbin/fdisk", "fdisk"),
    component_binary("usr/sbin/cfdisk", "cfdisk"),
    component_binary("usr/sbin/sfdisk", "sfdisk"),
    component_binary("usr/sbin/wipefs", "wipefs"),
    component_binary("usr/sbin/blkid", "blkid"),
    component_binary("usr/bin/findmnt", "findmnt"),
    component_binary("usr/sbin/losetup", "losetup"),
    component_binary("usr/bin/mountpoint", "mountpoint"),
    component_binary("usr/sbin/blockdev", "blockdev"),
    component_binary("usr/bin/flock", "flock"),
    component_binary("usr/bin/lscpu", "lscpu"),
    component_binary("usr/bin/lslocks", "lslocks"),
    component_binary("usr/bin/lsns", "lsns"),
    component_binary("usr/bin/nsenter", "nsenter"),
    component_binary("usr/bin/unshare", "unshare"),
    component_binary("usr/bin/taskset", "taskset"),
    component_binary("usr/bin/chrt", "chrt"),
    component_binary("usr/bin/ionice", "ionice"),
    component_binary("usr/bin/prlimit", "prlimit"),
    component_binary("usr/bin/uuidgen", "uuidgen"),
];

const GZIP_BINARIES: &[ComponentBinarySpec] = &[
    component_binary("usr/bin/gzip", "gzip"),
    component_binary("usr/bin/gunzip", "gunzip"),
    component_binary("usr/bin/zcat", "zcat"),
];
const BZIP2_BINARIES: &[ComponentBinarySpec] = &[
    component_binary("usr/bin/bzip2", "bzip2"),
    component_binary("usr/bin/bunzip2", "bunzip2"),
    component_binary("usr/bin/bzcat", "bzcat"),
    component_binary("usr/bin/bzip2recover", "bzip2recover"),
];
const XZ_BINARIES: &[ComponentBinarySpec] = &[
    component_binary("usr/bin/xz", "xz"),
    component_binary("usr/bin/unxz", "unxz"),
    component_binary("usr/bin/xzcat", "xzcat"),
    component_binary("usr/bin/lzma", "lzma"),
    component_binary("usr/bin/unlzma", "unlzma"),
    component_binary("usr/bin/lzcat", "lzcat"),
];
const ZSTD_BINARIES: &[ComponentBinarySpec] = &[
    component_binary("usr/bin/zstd", "zstd"),
    component_binary("usr/bin/unzstd", "unzstd"),
    component_binary("usr/bin/zstdcat", "zstdcat"),
];
const PATCH_BINARIES: &[ComponentBinarySpec] = &[component_binary("usr/bin/patch", "patch")];
const FILE_BINARIES: &[ComponentBinarySpec] = &[component_binary("usr/bin/file", "file")];
const LESS_BINARIES: &[ComponentBinarySpec] = &[
    component_binary("usr/bin/less", "less"),
    component_binary("usr/bin/lesskey", "lesskey"),
    component_binary_at("usr/libexec/lessecho", "usr/libexec/lessecho", "lessecho"),
];
const GIT_BINARIES: &[ComponentBinarySpec] = &[
    component_binary("usr/bin/git", "git"),
    component_binary("usr/bin/scalar", "scalar"),
];
const OPENSSH_BINARIES: &[ComponentBinarySpec] = &[
    component_binary("usr/bin/ssh", "ssh"),
    component_binary("usr/bin/scp", "scp"),
    component_binary("usr/bin/sftp", "sftp"),
    component_binary("usr/bin/ssh-add", "ssh-add"),
    component_binary("usr/bin/ssh-agent", "ssh-agent"),
    component_binary("usr/bin/ssh-keygen", "ssh-keygen"),
    component_binary("usr/bin/ssh-keyscan", "ssh-keyscan"),
    component_binary_at("usr/sbin/sshd", "usr/sbin/sshd", "sshd"),
];

const fn component_binary(path: &'static str, command_name: &'static str) -> ComponentBinarySpec {
    ComponentBinarySpec {
        source_rel: path,
        destination_rel: path,
        command_name,
    }
}

const fn component_binary_at(
    source_rel: &'static str,
    destination_rel: &'static str,
    command_name: &'static str,
) -> ComponentBinarySpec {
    ComponentBinarySpec {
        source_rel,
        destination_rel,
        command_name,
    }
}

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
        provider: UTIL_LINUX_PROVIDER,
        install_root_rel: "out/build/util-linux/install",
        binaries: UTIL_LINUX_BASE_BINARIES,
    },
    ComponentInstallManifest {
        provider: GZIP_PROVIDER,
        install_root_rel: "out/build/gzip/install",
        binaries: GZIP_BINARIES,
    },
    ComponentInstallManifest {
        provider: BZIP2_PROVIDER,
        install_root_rel: "out/build/bzip2/install",
        binaries: BZIP2_BINARIES,
    },
    ComponentInstallManifest {
        provider: XZ_PROVIDER,
        install_root_rel: "out/build/xz/install",
        binaries: XZ_BINARIES,
    },
    ComponentInstallManifest {
        provider: ZSTD_PROVIDER,
        install_root_rel: "out/build/zstd/install",
        binaries: ZSTD_BINARIES,
    },
    ComponentInstallManifest {
        provider: PATCH_PROVIDER,
        install_root_rel: "out/build/patch/install",
        binaries: PATCH_BINARIES,
    },
    ComponentInstallManifest {
        provider: FILE_PROVIDER,
        install_root_rel: "out/build/file/install",
        binaries: FILE_BINARIES,
    },
    ComponentInstallManifest {
        provider: LESS_PROVIDER,
        install_root_rel: "out/build/less/install",
        binaries: LESS_BINARIES,
    },
    ComponentInstallManifest {
        provider: GIT_PROVIDER,
        install_root_rel: "out/build/git/install",
        binaries: GIT_BINARIES,
    },
    ComponentInstallManifest {
        provider: OPENSSH_PROVIDER,
        install_root_rel: "out/build/openssh/install",
        binaries: OPENSSH_BINARIES,
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
        source_rel: "out/build/grep/cargo-target/release/grep",
        install_name: "grep",
        command_name: "grep",
    },
    BinaryInstallSpec {
        provider: SED_PROVIDER,
        source_rel: "out/build/sed/cargo-target/release/sed",
        install_name: "sed",
        command_name: "sed",
    },
    BinaryInstallSpec {
        provider: FINDUTILS_PROVIDER,
        source_rel: "out/build/findutils/cargo-target/release/find",
        install_name: "find",
        command_name: "find",
    },
    BinaryInstallSpec {
        provider: FINDUTILS_PROVIDER,
        source_rel: "out/build/findutils/cargo-target/release/xargs",
        install_name: "xargs",
        command_name: "xargs",
    },
    BinaryInstallSpec {
        provider: FINDUTILS_PROVIDER,
        source_rel: "out/build/findutils/cargo-target/release/locate",
        install_name: "locate",
        command_name: "locate",
    },
    BinaryInstallSpec {
        provider: FINDUTILS_PROVIDER,
        source_rel: "out/build/findutils/cargo-target/release/updatedb",
        install_name: "updatedb",
        command_name: "updatedb",
    },
    BinaryInstallSpec {
        provider: DIFFUTILS_PROVIDER,
        source_rel: "out/build/diffutils/cargo-target/release/diffutils",
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
    Timings,
    /// Validate and report the authoritative boot/image artifacts.
    Artifacts,
    Cache {
        #[command(subcommand)]
        command: CacheCommands,
    },
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
        #[arg(long, value_name = "JOBS", hide = true)]
        experimental_child_jobs: Option<usize>,
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
    #[command(hide = true)]
    ProbeExecutable {
        #[arg(long)]
        log_root: PathBuf,
    },
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

#[derive(Subcommand, Debug)]
enum CacheCommands {
    Status,
    Explain {
        #[arg(long)]
        details: bool,
        stage: String,
    },
    /// Explain the predicted cache blast radius without executing a build.
    Impact {
        /// Emit one JSON record per selected stage instead of readable lines.
        #[arg(long)]
        json: bool,
        #[arg(default_value = "all")]
        stage: String,
    },
    Invalidate {
        #[arg(long)]
        dependents: bool,
        stage: String,
    },
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

fn none_policy() -> String {
    "none".to_string()
}

#[derive(Debug, Deserialize, Clone)]
struct SourceSelectionPolicy {
    schema_version: u32,
    component: String,
    upstream_commit: String,
    scope: String,
    retain_arch_root_files: bool,
    retained_architectures: BTreeSet<String>,
    #[serde(default)]
    retained_arch_paths: BTreeSet<String>,
    #[serde(default)]
    x86_excluded_paths: BTreeSet<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct LfsHydrationPolicy {
    schema_version: u32,
    component: String,
    upstream_commit: String,
    source: String,
    object: Vec<LfsHydrationObject>,
}

#[derive(Debug, Deserialize, Clone)]
struct LfsHydrationObject {
    path: String,
    sha256: String,
    size: u64,
}

impl SourceSelectionPolicy {
    fn retains(&self, path: &str) -> bool {
        let mut parts = path.split('/');
        if parts.next() != Some("arch") {
            return true;
        }
        let Some(architecture) = parts.next() else {
            return true;
        };
        if parts.next().is_none() {
            return self.retain_arch_root_files;
        }
        let arch_relative = path.strip_prefix("arch/").unwrap_or(path);
        if self.retained_arch_paths.contains(arch_relative) {
            return true;
        }
        if !self.retained_architectures.contains(architecture) {
            return false;
        }
        if architecture != "x86" {
            return true;
        }
        let x86_relative = path.strip_prefix("arch/x86/").unwrap_or(path);
        !self.x86_excluded_paths.contains(x86_relative)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SyncState {
    schema_version: u32,
    component: String,
    repo: String,
    branch: String,
    imported_commit: String,
    imported_at_utc: String,
    sync_method: String,
    destination_path: String,
    upstream_tree: String,
    imported_tree_digest_algorithm: String,
    imported_tree_digest: String,
    #[serde(default = "none_policy")]
    source_selection_policy: String,
    #[serde(default = "none_policy")]
    source_selection_policy_sha256: String,
    intentional_omission_policy: String,
    gitlink_policy: String,
    patch_manifest: String,
    patch_manifest_sha256: String,
    #[serde(default = "none_policy")]
    lfs_policy: String,
    #[serde(default = "none_policy")]
    lfs_policy_sha256: String,
}

#[derive(Debug, Deserialize)]
struct ComponentPatchManifest {
    component: String,
    application: String,
    patch: Vec<ComponentPatchRecord>,
}

#[derive(Debug, Deserialize)]
struct ComponentPatchRecord {
    path: String,
    sha256: String,
}

#[derive(Debug)]
struct WslStatus {
    wsl_installed: bool,
    distros: Vec<String>,
}

fn ensure_private_cache_root(repo_root: &Path) -> Result<()> {
    let cache = repo_root.join("out/cache");
    if let Ok(metadata) = fs::symlink_metadata(&cache) {
        if metadata.file_type().is_symlink() {
            let actual = cache
                .canonicalize()
                .with_context(|| format!("unable to resolve cache symlink {}", cache.display()))?;
            let explicitly_shared = std::env::var_os("MATTOS_SHARED_CACHE_ROOT")
                .map(PathBuf::from)
                .map(|path| path.canonicalize().ok() == Some(actual.clone()))
                .unwrap_or(false);
            if !explicitly_shared {
                bail!(
                    "refusing external out/cache symlink {}; remove it for checkout-local cache use or set MATTOS_SHARED_CACHE_ROOT to the exact resolved target",
                    cache.display()
                );
            }
        }
    } else if !cache.exists() {
        fs::create_dir_all(&cache)
            .with_context(|| format!("failed to create private cache root {}", cache.display()))?;
    }
    Ok(())
}

fn prepare_source_ownership_tool_environment(repo_root: &Path) -> Result<()> {
    let dispatcher = repo_root.join("out/source-ownership/bin/cargo");
    let source = repo_root.join("DevUtils/cargo_source_owned.py");
    if !source.is_file() {
        return Ok(());
    }
    if !dispatcher.is_file() || fs::read(&dispatcher)? != fs::read(&source)? {
        if let Some(parent) = dispatcher.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source, &dispatcher)?;
        let mut permissions = fs::metadata(&dispatcher)?.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o755);
        }
        fs::set_permissions(&dispatcher, permissions)?;
    }
    let current_path = std::env::var_os("PATH").unwrap_or_default();
    let dispatcher_dir = dispatcher
        .parent()
        .expect("Cargo dispatcher has a parent")
        .to_path_buf();
    let real_cargo = std::env::split_paths(&current_path)
        .map(|directory| directory.join("cargo"))
        .find(|candidate| candidate.is_file() && candidate != &dispatcher);
    let Some(real_cargo) = real_cargo else {
        return Ok(());
    };
    let mut paths = vec![dispatcher_dir];
    paths.extend(std::env::split_paths(&current_path));
    // This runs during single-threaded process startup, before the scheduler
    // or any build worker exists. Rust 2024 marks process-environment mutation
    // unsafe because concurrent readers could observe a torn environment.
    unsafe {
        if std::env::var_os("MATTOS_REAL_CARGO").is_none() {
            std::env::set_var("MATTOS_REAL_CARGO", &real_cargo);
        }
        // The dispatcher is deliberately copied under out/source-ownership/
        // rather than executed from DevUtils.  Its file location therefore
        // cannot identify the checkout.  Give every Cargo child the actual
        // build root explicitly so the copied dispatcher enforces ownership
        // and reconciles its output lock instead of silently falling through
        // to the host Cargo binary.
        std::env::set_var("MATTOS_REPO_ROOT", repo_root);
        std::env::set_var("PATH", std::env::join_paths(paths)?);
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = std::env::current_dir().context("unable to determine current directory")?;
    ensure_private_cache_root(&repo_root)?;
    prepare_source_ownership_tool_environment(&repo_root)?;
    ensure_mattos_build_tmp(&repo_root)?;
    if let Commands::Build {
        stage,
        experimental_child_jobs,
    } = &cli.command
    {
        validate_experimental_child_jobs(
            stage.unwrap_or(BuildStage::All),
            *experimental_child_jobs,
        )?;
    }
    let timing_command = match &cli.command {
        Commands::Build {
            stage,
            experimental_child_jobs,
        } => Some(match experimental_child_jobs {
            Some(jobs) => format!(
                "build {} --experimental-child-jobs {jobs}",
                stage.map(build_stage_id).unwrap_or("all")
            ),
            None => format!("build {}", stage.map(build_stage_id).unwrap_or("all")),
        }),
        Commands::Image => Some("image".to_string()),
        Commands::Package { command } => Some(format!("package {command:?}")),
        _ => None,
    };
    if let Some(command) = timing_command.as_deref() {
        performance::start_timing_run(&repo_root, command)?;
    }

    let result = match cli.command {
        Commands::ProbeExecutable { log_root } => {
            performance::with_stage_log(&log_root, "executable-probe", || {
                performance::append_active_stage_log(&format!(
                    "executable-probe id={EXECUTABLE_PROBE_ID}"
                ))?;
                Ok(())
            })?;
            println!("MATTOS_BUILD_PROBE_ID={EXECUTABLE_PROBE_ID}");
            Ok(())
        }
        Commands::Doctor => doctor(),
        Commands::Timings => performance::show_latest_timings(&repo_root),
        Commands::Artifacts => report_artifacts(&repo_root),
        Commands::Cache { command } => cache_command(&repo_root, command),
        Commands::Upstream { command } => upstream_command(&repo_root, command),
        Commands::Package { command } => packaging::run_package_command(&repo_root, command),
        Commands::Build {
            stage,
            experimental_child_jobs,
        } => build(
            &repo_root,
            stage.unwrap_or(BuildStage::All),
            experimental_child_jobs,
        ),
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
    };
    if timing_command.is_some()
        && let Err(timing_error) = performance::finish_timing_run(&result)
    {
        if result.is_ok() {
            return Err(timing_error);
        }
        eprintln!("warning: failed to finish timing report: {timing_error:#}");
    }
    result
}

include!("commands/doctor.rs");
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

include!("commands/report.rs");
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
            for component in [
                "brush",
                "coreutils",
                "grep",
                "sed",
                "findutils",
                "diffutils",
                "sudo-rs",
            ] {
                remove_path_if_exists(
                    &repo_root
                        .join("out/build")
                        .join(component)
                        .join("cargo-target"),
                )?;
            }
        }
        CleanTarget::All => {
            remove_path_if_exists(&repo_root.join("out"))?;
            remove_path_if_exists(&repo_root.join("target"))?;
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

include!("commands/wsl.rs");
include!("source/import.rs");
fn build(
    repo_root: &Path,
    stage: BuildStage,
    experimental_child_jobs: Option<usize>,
) -> Result<()> {
    if stage == BuildStage::All {
        return build_all_scheduled(repo_root);
    }
    build_one_stage(repo_root, stage, None, experimental_child_jobs)
}

fn validate_experimental_child_jobs(stage: BuildStage, jobs: Option<usize>) -> Result<()> {
    validate_experimental_child_jobs_with_budget(stage, jobs, resources::discover().budget())
}

fn validate_experimental_child_jobs_with_budget(
    stage: BuildStage,
    jobs: Option<usize>,
    budget: resources::ResourceBudget,
) -> Result<()> {
    let Some(jobs) = jobs else {
        return Ok(());
    };
    if !matches!(
        stage,
        BuildStage::Glibc
            | BuildStage::GccRuntime
            | BuildStage::Binutils
            | BuildStage::GccToolchain
            | BuildStage::Make
            | BuildStage::Apt
    ) {
        bail!(
            "--experimental-child-jobs is restricted to isolated glibc, gcc-runtime, binutils, gcc-toolchain, make, or apt builds"
        );
    }
    let normal_jobs = stage_resource_profile(stage).minimum_cpu_grant;
    if jobs <= normal_jobs || jobs > budget.cpu_tokens {
        bail!(
            "experimental child jobs for {} must be above its safe baseline {} and at most the effective CPU budget {}",
            build_stage_id(stage),
            normal_jobs,
            budget.cpu_tokens,
        );
    }
    Ok(())
}

fn build_all_scheduled(repo_root: &Path) -> Result<()> {
    prune_derived_source_mirror_artifacts(repo_root)?;
    let stages = build_plan(BuildStage::All);
    let nodes = scheduled_build_nodes(&stages);
    let snapshot = resources::discover();
    let budget = snapshot.budget();
    scheduler::execute(nodes, budget, |id, context| {
        let stage = stages
            .iter()
            .copied()
            .find(|stage| build_stage_id(*stage) == id)
            .ok_or_else(|| anyhow!("scheduler selected unknown build stage {id}"))?;
        build_one_stage(repo_root, stage, Some(context), None)
    })
}

fn scheduled_build_nodes(stages: &[BuildStage]) -> Vec<scheduler::SchedulerNode> {
    let package_producers = stages
        .iter()
        .copied()
        .filter(|stage| {
            !matches!(
                stage,
                BuildStage::Kernel
                    | BuildStage::Rootfs
                    | BuildStage::LiveRoot
                    | BuildStage::Initramfs
                    | BuildStage::Iso
            )
        })
        .map(build_stage_id)
        .map(str::to_string)
        .collect::<Vec<_>>();
    stages
        .iter()
        .copied()
        .map(|stage| {
            let spec = build_stage_spec(stage);
            let dependencies = if stage == BuildStage::Rootfs {
                package_producers.clone()
            } else {
                spec.dependencies
                    .iter()
                    .map(|dependency| match dependency.as_str() {
                        "linux-headers" => "glibc",
                        "formal-sysroot" => "make",
                        other => other,
                    })
                    .map(str::to_string)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect()
            };
            scheduler::SchedulerNode {
                id: build_stage_id(stage).to_string(),
                dependencies,
                outputs: spec.outputs,
                profile: stage_resource_profile(stage),
            }
        })
        .collect()
}

fn build_one_stage(
    repo_root: &Path,
    stage: BuildStage,
    context: Option<&scheduler::JobContext>,
    experimental_child_jobs: Option<usize>,
) -> Result<()> {
    let profile = stage_resource_profile(stage);
    let standalone_envelope = resources::sample_now();
    let standalone_grant = scheduler::standalone_grant(profile, &standalone_envelope);
    scheduler::configure_child_jobs(
        experimental_child_jobs.unwrap_or(standalone_grant),
        profile.child_jobs,
    );
    EXPERIMENTAL_CHILD_JOBS.with(|current| current.set(experimental_child_jobs));
    if matches!(
        stage,
        BuildStage::Rootfs | BuildStage::LiveRoot | BuildStage::Initramfs | BuildStage::Iso
    ) {
        if let Some(context) = context {
            context.acquire_build_resources()?;
        }
        return build_stage(repo_root, stage);
    }
    let spec = build_stage_spec(stage);
    if is_cacheable_stage(stage) {
        performance::execute_cached_stage_with_resources(
            repo_root,
            &spec,
            || validate_cached_build_stage(repo_root, stage),
            || context.map_or(Ok(()), scheduler::JobContext::acquire_build_resources),
            || build_stage(repo_root, stage),
        )?;
    } else {
        if let Some(context) = context {
            context.acquire_build_resources()?;
        }
        let inputs = performance::compute_stage_inputs(repo_root, &spec)?;
        performance::timed(
            build_stage_id(stage),
            "n/a",
            "stage is intentionally non-cacheable in this milestone",
            &inputs.full_digest,
            || build_stage(repo_root, stage),
        )?;
    }
    if stage == BuildStage::Glibc {
        performance::record_virtual_stage(repo_root, &linux_headers_stage_spec())?;
    }
    if stage == BuildStage::Make {
        performance::record_virtual_stage(repo_root, &formal_sysroot_stage_spec())?;
    }
    Ok(())
}

include!("commands/cache.rs");
include!("stages/registry.rs");
fn kernel_config_state(config: &str, symbol: &str) -> Option<KernelConfigState> {
    if config.lines().any(|line| line == format!("{symbol}=y")) {
        Some(KernelConfigState::Builtin)
    } else if config.lines().any(|line| line == format!("{symbol}=m")) {
        Some(KernelConfigState::Module)
    } else if config
        .lines()
        .any(|line| line == format!("# {symbol} is not set"))
    {
        Some(KernelConfigState::Unsupported)
    } else {
        None
    }
}

include!("stages/toolchain.rs");
include!("stages/base_userland.rs");
include!("stages/helpers/native.rs");
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
    let stamp = format!(
        "{state}\n{}\nattr-bootstrap={ATTR_UPSTREAM_COMMIT} {} {}\n",
        options.join("\n"),
        ATTR_RELEASE_ARCHIVE_URL,
        ATTR_RELEASE_ARCHIVE_SHA256,
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    let archive = ensure_attr_release_archive(&out_root)?;
    stage_attr_bootstrap_inputs(&source, &source_copy, &archive)?;
    // The imported Git files and generated release files have unrelated
    // timestamps.  Normalize the output mirror after staging so Automake does
    // not attempt a host-versioned regeneration merely because a macro was
    // copied a few milliseconds after aclocal.m4.
    run_cmd(
        &source_copy,
        "find",
        // Imported Git files and generated files copied from the verified
        // release archive otherwise retain unrelated timestamps.  Give every
        // source input one deterministic timestamp so Make cannot decide the
        // generated Makefile.in is stale and invoke host automake-1.16.
        &[".", "-type", "f", "-exec", "touch", "-c", "-d", "@0", "{}", "+"],
    )?;
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").is_file() {
        let configure = source_copy.join("configure");
        run_cmd(&build_dir, path_str(&configure)?, &options)?;
    }
    // The official distribution archive already supplies the generated
    // Autotools files.  Do not let timestamp differences from the imported
    // Git checkout trigger a host-versioned aclocal rebuild.
    run_cmd(&build_dir, "make", &["-j", "4", "MAKE_MAINTAINER_MODE="])?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        &build_dir,
        "make",
        &[
            "MAKE_MAINTAINER_MODE=",
            "install",
            &format!("DESTDIR={}", install_dir.display()),
        ],
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

/// Obtains the official Attr v2.6.0 distribution archive in the Attr output
/// directory.  The archive is accepted only when its published SHA-256
/// matches, so an interrupted or substituted download cannot supply build
/// inputs.  `out/cache` is intentionally not used because this workspace
/// points it at the preserved reproduction baseline.
fn ensure_attr_release_archive(out_root: &Path) -> Result<PathBuf> {
    let bootstrap = out_root.join("bootstrap");
    let archive = bootstrap.join(format!("{ATTR_RELEASE_DIRECTORY}.tar.xz"));
    if archive.is_file() {
        verify_attr_release_archive(&archive)?;
        return Ok(archive);
    }

    fs::create_dir_all(&bootstrap)
        .with_context(|| format!("failed to create {}", bootstrap.display()))?;
    let temporary = bootstrap.join("attr-2.6.0.tar.xz.tmp");
    let temporary_arg = path_str(&temporary)?;
    run_cmd(
        out_root,
        "curl",
        &[
            "-fL",
            "--retry",
            "3",
            "--output",
            temporary_arg,
            ATTR_RELEASE_ARCHIVE_URL,
        ],
    )
    .context("failed to download the pinned official Attr v2.6.0 release archive")?;
    verify_attr_release_archive(&temporary)?;
    fs::rename(&temporary, &archive)
        .with_context(|| format!("failed to publish {}", archive.display()))?;
    Ok(archive)
}

fn verify_attr_release_archive(archive: &Path) -> Result<()> {
    let actual = performance::sha256_file(archive)?;
    if actual != ATTR_RELEASE_ARCHIVE_SHA256 {
        bail!(
            "Attr release archive checksum mismatch: expected {}, got {} at {}",
            ATTR_RELEASE_ARCHIVE_SHA256,
            actual,
            archive.display()
        );
    }
    Ok(())
}

/// Adds every distribution-only input from the verified release archive to an
/// output-owned Attr mirror.  Files present in the authoritative imported
/// checkout always win, including any intentional local source edits.  This
/// gives configure the complete generated release closure without modifying
/// the imported checkout or relying on host Autoconf macro packages.
fn stage_attr_bootstrap_inputs(
    authoritative_source: &Path,
    source_copy: &Path,
    archive: &Path,
) -> Result<()> {
    let release = archive
        .parent()
        .ok_or_else(|| anyhow!("Attr release archive has no parent directory"))?
        .join("release");
    remove_path_if_exists(&release)?;
    fs::create_dir_all(&release)
        .with_context(|| format!("failed to create {}", release.display()))?;
    let archive_arg = path_str(archive)?;
    let release_arg = path_str(&release)?;
    run_cmd(
        source_copy,
        "tar",
        &[
            "-xJf",
            archive_arg,
            "--strip-components=1",
            "-C",
            release_arg,
        ],
    )
    .context("failed to stage pinned Attr release bootstrap inputs")?;
    copy_attr_release_only_entries(&release, authoritative_source, source_copy)?;

    let visibility = source_copy.join("m4/visibility_hidden.m4");
    let contents = fs::read_to_string(&visibility)
        .with_context(|| format!("pinned Attr release omitted {}", visibility.display()))?;
    if !contents.contains("AC_DEFUN([AC_FUNC_GCC_VISIBILITY]") {
        bail!(
            "pinned Attr release bootstrap input {} does not define AC_FUNC_GCC_VISIBILITY",
            visibility.display()
        );
    }
    for required in [
        "configure",
        "aclocal.m4",
        "Makefile.in",
        "build-aux/config.rpath",
    ] {
        if !source_copy.join(required).is_file() {
            bail!("pinned Attr release bootstrap input is missing {required}");
        }
    }
    Ok(())
}

fn copy_attr_release_only_entries(
    release: &Path,
    authoritative: &Path,
    destination: &Path,
) -> Result<()> {
    let mut entries = fs::read_dir(release)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let source = entry.path();
        let original = authoritative.join(entry.file_name());
        let target = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source)?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            copy_attr_release_only_entries(&source, &original, &target)?;
            continue;
        }
        if fs::symlink_metadata(&original).is_ok_and(|_| true) {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        remove_path_if_exists(&target)?;
        if metadata.file_type().is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(&source)?, &target)?;
            #[cfg(not(unix))]
            fs::copy(&source, &target)?;
        } else {
            fs::copy(&source, &target).with_context(|| {
                format!(
                    "failed to stage {} to {}",
                    source.display(),
                    target.display()
                )
            })?;
            preserve_permissions(&metadata, &target)?;
        }
    }
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
    let archive = ensure_acl_release_archive(&out_root)?;
    stage_acl_bootstrap_inputs(&source, &source_copy, &archive)?;
    run_cmd(
        &source_copy,
        "find",
        &[".", "-type", "f", "-exec", "touch", "-c", "-d", "@0", "{}", "+"],
    )?;
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").is_file() {
        let configure = source_copy.join("configure");
        let attr = repo_root.join("out/build/attr/install/usr");
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&configure)?,
            &options,
            &[
                ("CPPFLAGS", format!("-I{}", attr.join("include").display())),
                (
                    "LDFLAGS",
                    format!("-L{}", attr.join("lib/x86_64-linux-gnu").display()),
                ),
            ],
        )?;
    }
    // The pinned distribution archive already supplies Autotools-generated
    // files.  Keep maintainer regeneration disabled so timestamp differences
    // in this disposable mirror cannot require a host-versioned aclocal.
    run_cmd(&build_dir, "make", &["-j", "4", "MAKE_MAINTAINER_MODE="])?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        &build_dir,
        "make",
        &[
            "MAKE_MAINTAINER_MODE=",
            "install",
            &format!("DESTDIR={}", install_dir.display()),
        ],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libacl.so.1");
    if !soname.exists() {
        bail!("ACL install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn ensure_acl_release_archive(out_root: &Path) -> Result<PathBuf> {
    let bootstrap = out_root.join("bootstrap");
    let archive = bootstrap.join(format!("{ACL_RELEASE_DIRECTORY}.tar.xz"));
    fs::create_dir_all(&bootstrap)?;
    if !archive.is_file() {
        let temp = bootstrap.join("acl.tar.xz.tmp");
        run_cmd(
            out_root,
            "curl",
            &[
                "-fL",
                "--retry",
                "3",
                "--output",
                path_str(&temp)?,
                ACL_RELEASE_ARCHIVE_URL,
            ],
        )?;
        let actual = performance::sha256_file(&temp)?;
        if actual != ACL_RELEASE_ARCHIVE_SHA256 {
            bail!(
                "ACL release archive checksum mismatch: expected {ACL_RELEASE_ARCHIVE_SHA256}, got {actual}"
            );
        }
        fs::rename(temp, &archive)?;
    }
    let actual = performance::sha256_file(&archive)?;
    if actual != ACL_RELEASE_ARCHIVE_SHA256 {
        bail!(
            "ACL release archive checksum mismatch: expected {ACL_RELEASE_ARCHIVE_SHA256}, got {actual}"
        );
    }
    Ok(archive)
}

fn stage_acl_bootstrap_inputs(
    authoritative: &Path,
    destination: &Path,
    archive: &Path,
) -> Result<()> {
    let release = archive.parent().unwrap().join("release");
    remove_path_if_exists(&release)?;
    fs::create_dir_all(&release)?;
    run_cmd(
        destination,
        "tar",
        &[
            "-xJf",
            path_str(archive)?,
            "--strip-components=1",
            "-C",
            path_str(&release)?,
        ],
    )?;
    copy_attr_release_only_entries(&release, authoritative, destination)?;
    for required in [
        "configure",
        "aclocal.m4",
        "m4/visibility_hidden.m4",
        "m4/package_attrdev.m4",
    ] {
        if !destination.join(required).is_file() {
            bail!("pinned ACL release bootstrap input is missing {required}");
        }
    }
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

fn build_gzip(repo_root: &Path) -> Result<()> {
    build_release_autotools_program(
        repo_root,
        "gzip",
        "gzip-1.14.tar.xz",
        GZIP_RELEASE_ARCHIVE_URL,
        GZIP_RELEASE_ARCHIVE_SHA256,
        &[],
        &["--prefix=/usr", "--disable-nls"],
        &["usr/bin/gzip"],
    )
}

fn build_patch(repo_root: &Path) -> Result<()> {
    build_release_autotools_program(
        repo_root,
        "patch",
        "patch-2.8.tar.xz",
        PATCH_RELEASE_ARCHIVE_URL,
        PATCH_RELEASE_ARCHIVE_SHA256,
        &[],
        &["--prefix=/usr", "--disable-nls"],
        &["usr/bin/patch"],
    )
}

include!("stages/helpers/pkgconfig.rs");
include!("stages/helpers/autotools.rs");
include!("stages/helpers/meson.rs");
include!("stages/graphics.rs");
include!("stages/desktop.rs");

fn build_polkit(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "polkit",
        "src/system/security/polkit",
        &[
            "glib",
            "zlib",
            "systemd",
            "dbus",
            "duktape",
            "linux-pam",
            "libffi",
        ],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dman=false",
            "-Dgtk_doc=false",
            "-Dexamples=false",
            "-Dintrospection=false",
            "-Dgettext=false",
            "-Dsession_tracking=logind",
            "-Dauthfw=pam",
            "-Dos_type=debian",
            "-Dpolkitd_uid=197",
        ],
        "usr/lib/x86_64-linux-gnu/libpolkit-agent-1.so.0",
        &[],
    )
}

fn build_libfyaml(repo_root: &Path) -> Result<()> {
    build_vulkan_cmake(
        repo_root,
        "libfyaml",
        "src/system/libraries/libfyaml",
        &[],
        &["-DFYAML_BUILD_TESTS=OFF".to_string()],
        &["usr/lib/x86_64-linux-gnu/libfyaml.so.0"],
        None,
    )
}

fn build_libxmlb(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libxmlb",
        "src/system/libraries/libxmlb",
        &["glib", "libffi", "xz", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dgtkdoc=false",
            "-Dintrospection=false",
            "-Dcli=false",
        ],
        "usr/lib/x86_64-linux-gnu/libxmlb.so.2",
        &[],
    )
}

fn build_json_glib(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "json-glib",
        "src/system/libraries/json-glib",
        &["glib", "libffi", "pcre2", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dintrospection=disabled",
            "--wrap-mode=nofallback",
        ],
        "usr/lib/x86_64-linux-gnu/libjson-glib-1.0.so.0",
        &[],
    )
}

fn build_appstream(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "appstream",
        "src/system/libraries/appstream",
        &[
            "glib", "libffi", "libxml2", "zlib", "curl", "openssl", "libfyaml", "libxmlb", "xz",
            "zstd", "systemd", "wayland",
        ],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dapidocs=false",
            "-Dstemming=false",
            "-Dbash-completion=false",
            "-Dinstall-docs=false",
            "-Dman=false",
            "-Dvapi=false",
            "-Dgir=false",
            "--wrap-mode=nofallback",
        ],
        "usr/lib/x86_64-linux-gnu/libappstream.so.5",
        &[],
    )
}

fn build_gdk_pixbuf(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "gdk-pixbuf",
        "src/system/libraries/gdk-pixbuf",
        // gdk-pixbuf links helper binaries against libglib; GLib's target
        // ABI requires PCRE2 at that link step, not merely at final runtime.
        &["glib", "libffi", "zlib", "libpng", "pcre2"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dinstalled_tests=false",
            "-Dintrospection=disabled",
            "-Dman=false",
            "-Dgio_sniffing=false",
            "-Djpeg=disabled",
            "-Dtiff=disabled",
            "-Dothers=disabled",
            "--wrap-mode=nofallback",
        ],
        "usr/lib/x86_64-linux-gnu/libgdk_pixbuf-2.0.so.0",
        &[],
    )
}

fn build_gpgme(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "gpgme",
        "src/system/security/gpgme",
        &["libassuan", "libgcrypt", "libgpg-error", "libksba", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-gpgsm",
            "--disable-gpgconf",
            "--disable-gpg-test",
        ],
        &["usr/lib/x86_64-linux-gnu/libgpgme.so.45"],
    )?;
    // Libtool consumers otherwise resolve this build-tree .la file and embed
    // its absolute staging directory as a RUNPATH.  The target .so and
    // pkg-config metadata are the published interface; the .la archive is a
    // build-private libtool convenience file and is not part of it.
    remove_path_if_exists(
        &repo_root.join("out/build/gpgme/install/usr/lib/x86_64-linux-gnu/libgpgme.la"),
    )?;
    Ok(())
}

fn build_flatpak(repo_root: &Path) -> Result<()> {
    // Flatpak is a native target package-manager runtime.  Keep its build
    // isolated from the COSMIC aggregate so its pkg-config and ELF closure
    // can be audited independently.
    build_meson_runtime(
        repo_root,
        "flatpak",
        "src/system/packages/flatpak",
        &[
            "glib",
            "libffi",
            "zlib",
            "xz",
            "curl",
            "openssl",
            "libcap",
            "libarchive",
            "libxml2",
            "fuse3",
            "ostree",
            "systemd",
            "dbus",
            "gpgv",
            "zstd",
            "wayland",
            "xkbcommon",
            "libpng",
            "libbsd",
            "libassuan",
            "libgcrypt",
            "libgpg-error",
            "libksba",
            "json-glib",
            "appstream",
            "gdk-pixbuf",
            "gpgme",
            "polkit",
            "bubblewrap",
            "xdg-dbus-proxy",
        ],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dinstalled_tests=false",
            "-Dman=disabled",
            "-Ddocbook_docs=disabled",
            "-Dgtkdoc=disabled",
            "-Dgir=disabled",
            "-Ddconf=disabled",
            "-Dmalcontent=disabled",
            "-Dselinux_module=disabled",
            "-Dxauth=disabled",
            "-Dwayland_security_context=disabled",
            "-Dsystem_helper=enabled",
            "-Dsystemd=enabled",
            // MattOS grants ordinary administrative users membership in
            // `sudo`, not Debian/Fedora's `wheel`.  Flatpak's generated
            // system-helper polkit rule must follow that distro policy so
            // COSMIC Store can authorize system installs without making the
            // installation tree writable or running Store as root.
            "-Dprivileged_group=sudo",
            "-Dseccomp=disabled",
            // Never let Meson record the staged build-tree path returned by
            // find_program("fusermount3") in the shipped binary.  Flatpak
            // executes fusermount from the target package closure at this
            // stable runtime location.
            "-Dsystem_fusermount=/usr/bin/fusermount3",
            "-Dsystem_bubblewrap=/usr/bin/bwrap",
            "-Dsystem_dbus_proxy=/usr/bin/xdg-dbus-proxy",
            "--wrap-mode=nofallback",
        ],
        "usr/bin/flatpak",
        &[],
    )?;
    build_flatpak_target_install_helper(repo_root)?;
    Ok(())
}

/// Build the MattOS-owned installer helper against the target-built
/// libflatpak shipped in the Flatpak package. It opens an explicit target
/// installation while running in the booted live system, never in a chroot.
fn build_flatpak_target_install_helper(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/installer/flatpak-target-install.c");
    if !source.is_file() {
        bail!("missing MattOS Flatpak target-install helper {}", source.display());
    }
    let install = repo_root.join("out/build/flatpak/install");
    let output = install.join("usr/libexec/mattos-flatpak-target-install");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let compiler = repo_root.join("out/build/gcc-toolchain/install/usr/bin/gcc");
    let sysroot = repo_root.join("out/sysroot");
    let libc_search = format!("-B{}/usr/lib/x86_64-linux-gnu/", sysroot.display());
    let gcc_search = format!(
        "-B{}/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0/",
        sysroot.display()
    );
    let flatpak_usr = install.join("usr");
    let flatpak_lib = flatpak_usr.join("lib/x86_64-linux-gnu");
    let glib_usr = repo_root.join("out/build/glib/install/usr");
    let glib_lib = glib_usr.join("lib/x86_64-linux-gnu");
    let ostree_usr = repo_root.join("out/build/ostree/install/usr");
    let gcc_runtime_lib = repo_root.join("out/build/gcc-runtime/install/usr/lib/lib64");
    let args = vec![
        format!("--sysroot={}", sysroot.display()),
        libc_search,
        gcc_search,
        "-std=c11".to_string(),
        "-O2".to_string(),
        "-fno-ident".to_string(),
        format!("-ffile-prefix-map={}=/usr/src/mattos", repo_root.display()),
        format!("-fdebug-prefix-map={}=/usr/src/mattos", repo_root.display()),
        format!("-fmacro-prefix-map={}=/usr/src/mattos", repo_root.display()),
        format!("-I{}", flatpak_usr.join("include").display()),
        format!("-I{}", flatpak_usr.join("include/flatpak").display()),
        format!("-I{}", glib_usr.join("include/glib-2.0").display()),
        format!("-I{}", glib_lib.join("glib-2.0/include").display()),
        format!("-I{}", ostree_usr.join("include/ostree-1").display()),
        format!("-L{}", flatpak_lib.display()),
        format!("-L{}", glib_lib.display()),
        format!("-L{}", gcc_runtime_lib.display()),
        format!("-Wl,-rpath-link,{}", flatpak_lib.display()),
        format!("-Wl,-rpath-link,{}", glib_lib.display()),
        format!("-Wl,-rpath-link,{}", gcc_runtime_lib.display()),
        format!("-Wl,-rpath-link,{}", ostree_usr.join("lib/x86_64-linux-gnu").display()),
        path_str(&source)?.to_owned(),
        "-Wl,--no-as-needed".to_string(),
        "-lflatpak".to_string(),
        "-lgio-2.0".to_string(),
        "-lgobject-2.0".to_string(),
        "-lglib-2.0".to_string(),
        "-Wl,--as-needed".to_string(),
        "-o".to_string(),
        path_str(&output)?.to_owned(),
    ];
    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    let environment = staged_library_environment(
        repo_root,
        &[
            "flatpak", "glib", "libffi", "zlib", "xz", "curl", "openssl", "libcap",
            "libarchive", "libxml2", "fuse3", "ostree", "systemd", "dbus", "gpgv",
            "zstd", "wayland", "xkbcommon", "libpng", "libbsd", "libassuan",
            "libgcrypt", "libgpg-error", "libksba", "json-glib", "appstream",
            "gdk-pixbuf", "gpgme", "polkit", "bubblewrap", "xdg-dbus-proxy",
        ],
    )?;
    run_cmd_with_env_overrides(repo_root, path_str(&compiler)?, &args_ref, &environment)?;
    set_mode(output.clone(), 0o755)?;
    if !output.is_file() {
        bail!("Flatpak target-install helper was not produced at {}", output.display());
    }
    Ok(())
}

fn build_libarchive(repo_root: &Path) -> Result<()> {
    build_vulkan_cmake(
        repo_root,
        "libarchive",
        "src/system/libraries/libarchive",
        &["zlib", "zstd", "bzip2", "xz", "lz4", "libcap"],
        &[
            "-DENABLE_TEST=OFF".to_string(),
            "-DENABLE_TAR=OFF".to_string(),
            "-DENABLE_CPIO=OFF".to_string(),
            "-DENABLE_CAT=OFF".to_string(),
            "-DENABLE_OPENSSL=OFF".to_string(),
            "-DENABLE_ACL=OFF".to_string(),
            "-DENABLE_XATTR=OFF".to_string(),
            "-DENABLE_ICONV=OFF".to_string(),
            "-DENABLE_EXPAT=OFF".to_string(),
        ],
        &["usr/lib/x86_64-linux-gnu/libarchive.so.13"],
        None,
    )
}

fn build_libxml2(repo_root: &Path) -> Result<()> {
    build_vulkan_cmake(
        repo_root,
        "libxml2",
        "src/system/libraries/libxml2",
        &["zlib", "expat"],
        &[
            "-DLIBXML2_WITH_TESTS=OFF".to_string(),
            "-DLIBXML2_WITH_PYTHON=OFF".to_string(),
            "-DLIBXML2_WITH_LZMA=OFF".to_string(),
            "-DLIBXML2_WITH_ZSTD=OFF".to_string(),
            "-DLIBXML2_WITH_ICU=OFF".to_string(),
        ],
        &["usr/lib/x86_64-linux-gnu/libxml2.so.16"],
        None,
    )
}

fn build_libpng(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "libpng",
        "src/system/libraries/libpng",
        &["zlib"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-tests",
        ],
        &["usr/lib/x86_64-linux-gnu/libpng16.so.16"],
    )
}

fn build_fuse3(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "fuse3",
        "src/system/libraries/fuse3",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dexamples=false",
            "-Duseroot=false",
            "-Denable-io-uring=false",
            "-Dudevrulesdir=/usr/lib/udev/rules.d",
            "-Dinitscriptdir=",
        ],
        "usr/lib/x86_64-linux-gnu/libfuse3.so.4",
        &[],
    )
}

fn build_ostree(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "ostree",
        "src/system/packages/ostree",
        &[
            "glib",
            "libffi",
            "zlib",
            "bzip2",
            "xz",
            "zstd",
            "curl",
            "openssl",
            "libarchive",
            "libxml2",
            "fuse3",
            "gpgme",
            "libassuan",
            "libgpg-error",
            "gpgv",
            "libbsd",
            "installer",
        ],
        &[
            "--host=x86_64-linux-gnu",
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-tests",
            "--disable-man",
            "--disable-gtk-doc",
            "--disable-introspection",
            "--with-gpgme",
            // Flatpak pulls OSTree commits from HTTPS remotes such as
            // Flathub.  The target-built curl stage is the selected fetcher
            // backend; disabling both Soup backends remains intentional.
            "--with-curl",
            "--disable-selinux",
            "--disable-composefs",
            "--disable-systemd",
            "--disable-rofiles-fuse",
            "--with-soup3=no",
            "--with-soup=no",
            "LIBS=-lbsd",
        ],
        &["usr/lib/x86_64-linux-gnu/libostree-1.so.1"],
    )
}

fn build_duktape(repo_root: &Path) -> Result<()> {
    let out_root = repo_root.join("out/build/duktape");
    let source = out_root.join("source");
    let install = out_root.join("install/usr");
    sync_build_source(&repo_root.join("src/system/security/duktape"), &source)?;
    remove_path_if_exists(&install)?;
    let configure = source.join("tools/configure.py");
    let configure_body = fs::read_to_string(&configure)?
        .replace("open(apiheader_filename, 'rb')", "open(apiheader_filename, 'r')")
        .replace("open(src, 'rb')", "open(src, 'r', encoding='utf-8')")
        .replace("open(dst, 'wb')", "open(dst, 'w', encoding='utf-8')")
        .replace("open(value, 'rb')", "open(value, 'r', encoding='utf-8')")
        .replace("open(license_file, 'rb')", "open(license_file, 'r', encoding='utf-8')")
        .replace("open(authors_file, 'rb')", "open(authors_file, 'r', encoding='utf-8')")
        .replace("open(tmpfn, 'wb')", "open(tmpfn, 'w', encoding='utf-8')")
        .replace("open(tmpfn, 'rb')", "open(tmpfn, 'r', encoding='utf-8')")
        .replace("open(os.path.join(tempdir, suffix + '.txt'), 'wb')", "open(os.path.join(tempdir, suffix + '.txt'), 'w', encoding='utf-8')")
        .replace("open(os.path.join(tempdir, 'caseconv.txt'), 'wb')", "open(os.path.join(tempdir, 'caseconv.txt'), 'w', encoding='utf-8')")
        .replace("open(os.path.join(tempdir, 'caseconv_re_canon_lookup.txt'), 'wb')", "open(os.path.join(tempdir, 'caseconv_re_canon_lookup.txt'), 'w', encoding='utf-8')")
        .replace("open(os.path.join(tempdir, 'caseconv_re_canon_bitmap.txt'), 'wb')", "open(os.path.join(tempdir, 'caseconv_re_canon_bitmap.txt'), 'w', encoding='utf-8')")
        .replace("open(os.path.join(tempdir, 'duk_used_stridx_bidx_defs.json.tmp'), 'wb')", "open(os.path.join(tempdir, 'duk_used_stridx_bidx_defs.json.tmp'), 'w', encoding='utf-8')")
        .replace("'rb')", "'r', encoding='utf-8')")
        .replace("'wb')", "'w', encoding='utf-8')")
        .replace("line = line.decode('utf-8')", "line = line")
        .replace("f.write(i)", "f.write(i.decode('utf-8') if isinstance(i, bytes) else i)")
        .replace("ret = proc.communicate(input=input)", "ret = proc.communicate(input=input)\n        ret = (ret[0].decode('utf-8'), ret[1].decode('utf-8'))")
        .replace("f.write(res.decode('utf-8'))", "f.write(res)")
        .replace("f.write(i)", "f.write(i)")
        .replace("f_out.write(f_in.read())", "f_out.write(f_in.read())")
        .replace("f_out.write(c.encode('ascii'))", "f_out.write(c)")
        .replace("f.write(json.dumps(doc, indent=4))", "f.write(json.dumps(doc, indent=4))")
        .replace("duk_version / 10000", "duk_version // 10000")
        .replace("duk_version % 10000 / 100", "duk_version % 10000 // 100");
    fs::write(&configure, configure_body)?;
    let scanner = source.join("tools/scan_used_stridx_bidx.py");
    let scanner_body =
        fs::read_to_string(&scanner)?.replace("open(fn, 'rb')", "open(fn, 'r', encoding='utf-8')");
    fs::write(scanner, scanner_body)?;
    let genconfig = source.join("tools/genconfig.py");
    let mut genconfig_body = fs::read_to_string(&genconfig)?
        .replace(
            "import logging",
            "unicode = str\nlong = int\nxrange = range\nimport logging",
        )
        .replace("'rb')", "'r', encoding='utf-8')")
        .replace("'wb')", "'w', encoding='utf-8')")
        .replace("yaml.load(", "yaml.safe_load(")
        .replace(
            "import logging",
            "from functools import cmp_to_key\nimport logging",
        )
        .replace(
            "strs.sort(cmp=sortCmp)",
            "strs.sort(key=cmp_to_key(sortCmp))",
        );
    for (old, new) in [
        ("self.provides.has_key(m)", "m in self.provides"),
        ("assumed_provides.has_key(k)", "k in assumed_provides"),
        ("sn2.provides.has_key(k)", "k in sn2.provides"),
        ("not graph.has_key(sn)", "sn not in graph"),
        ("handled.has_key(sn)", "sn in handled"),
        ("not handled.has_key(sn)", "sn not in handled"),
        ("handled.has_key(dep)", "dep in handled"),
        (
            "not emitted_provides.has_key(k)",
            "k not in emitted_provides",
        ),
        ("handled.has_key(dname)", "dname in handled"),
        ("not handled.has_key(dname)", "dname not in handled"),
        ("use_defs.has_key(k)", "k in use_defs"),
        ("defval.has_key('verbatim')", "'verbatim' in defval"),
        ("defval.has_key('string')", "'string' in defval"),
        (
            "not forced_opts.has_key(doc['define'])",
            "doc['define'] not in forced_opts",
        ),
        (
            "forced_opts.has_key('DUK_USE_CPP_EXCEPTIONS')",
            "'DUK_USE_CPP_EXCEPTIONS' in forced_opts",
        ),
        (
            "not forced_opts.has_key(defname)",
            "defname not in forced_opts",
        ),
        ("not doc.has_key('default')", "'default' not in doc"),
        ("tmp.provides.has_key(defname)", "defname in tmp.provides"),
        ("need.has_key(k)", "k in need"),
        (
            "not defs_used.has_key(meta['define'])",
            "meta['define'] not in defs_used",
        ),
        ("not meta.has_key('removed')", "'removed' not in meta"),
        ("keys = use_defs.keys()", "keys = list(use_defs.keys())"),
        ("keys = opt_defs.keys()", "keys = list(opt_defs.keys())"),
        (
            "use_tags_list = use_tags.keys()",
            "use_tags_list = list(use_tags.keys())",
        ),
    ] {
        genconfig_body = genconfig_body.replace(old, new);
    }
    genconfig_body = rewrite_python2_has_key(genconfig_body);
    fs::write(genconfig, genconfig_body)?;
    let genbuiltins = source.join("tools/genbuiltins.py");
    let mut genbuiltins_body = fs::read_to_string(&genbuiltins)?
        .replace("#!/usr/bin/env python2", "#!/usr/bin/env python3")
        .replace("import logging", "import base64\nunicode = str\nunichr = chr\nlong = int\nxrange = range\ncmp = lambda a, b: (a > b) - (a < b)\nimport logging")
        .replace("except Exception, e:", "except Exception as e:")
        .replace("'rb')", "'r', encoding='utf-8')")
        .replace("'wb')", "'w', encoding='utf-8')")
        .replace("yaml.load(", "yaml.safe_load(")
        .replace("strs.sort(cmp=sortCmp)", "strs.sort(key=cmp_to_key(sortCmp))")
        .replace("val['bytes'].decode('hex')", "bytes.fromhex(val['bytes'])")
        .replace("val.decode('hex')", "bytes.fromhex(val)")
        .replace("data = ''.join([ val[indexlist[idx]] for idx in xrange(8) ])", "data = bytes([val[indexlist[idx]] for idx in xrange(8)])")
        .replace("val.encode('hex')", "val.hex()")
        .replace("data.encode('hex')", "data.hex()")
        .replace("struct.pack('>d', float(v)).encode('hex')", "struct.pack('>d', float(v)).hex()")
        .replace("ord(c2)", "(c2 if isinstance(c2, int) else ord(c2))")
        .replace("ord(c)", "(c if isinstance(c, int) else ord(c))")
        .replace("ord(val[i])", "(val[i] if isinstance(val[i], int) else ord(val[i]))")
        .replace("ord(v[0])", "(v[0] if isinstance(v[0], int) else ord(v[0]))")
        .replace("for idx, c in enumerate(s):", "for idx, c in enumerate(s):\n        c = chr(c) if isinstance(c, int) else c")
        .replace("c2 = s[idx+1]", "c2 = s[idx+1]\n            c2 = chr(c2) if isinstance(c2, int) else c2")
        .replace("unicode_to_bytes(s['str']).encode('base64').strip()", "base64.b64encode(unicode_to_bytes(s['str']).encode('utf-8')).decode('ascii').strip()")
        .replace("import logging", "from functools import cmp_to_key\nimport logging");
    for (old, new) in [
        (
            "user_meta.has_key('add_objects')",
            "'add_objects' in user_meta",
        ),
        (
            "user_meta.has_key('replace_objects')",
            "'replace_objects' in user_meta",
        ),
        (
            "user_meta.has_key('modify_objects')",
            "'modify_objects' in user_meta",
        ),
        ("if o.has_key('nargs')", "if 'nargs' in o"),
        ("assert(o.has_key('nargs'))", "assert('nargs' in o)"),
        ("not pval.has_key('length')", "'length' not in pval"),
        ("not pval.has_key('nargs')", "'nargs' not in pval"),
        ("not val.has_key('getter')", "'getter' not in val"),
        ("not val.has_key('setter')", "'setter' not in val"),
        ("prop.has_key(k)", "k in prop"),
        ("val['value'].has_key('getter')", "'getter' in val['value']"),
        ("val['value'].has_key('setter')", "'setter' in val['value']"),
        ("if o.has_key('native')", "if 'native' in o"),
        ("and not o.has_key('bidx')", "and 'bidx' not in o"),
        ("prop.has_key('value')", "'value' in prop"),
        ("targ.has_key('magic')", "'magic' in targ"),
        ("not reachable.has_key(o['id'])", "o['id'] not in reachable"),
        ("special_defs.has_key(v)", "v in special_defs"),
        ("s.has_key('define')", "'define' in s"),
        (
            "defs_needed.has_key(s['define'])",
            "s['define'] in defs_needed",
        ),
        ("not defs_found.has_key(k)", "k not in defs_found"),
        ("prev.has_key(k)", "k in prev"),
        ("kw_index.has_key(s['str'])", "s['str'] in kw_index"),
        (
            "meta.has_key('objects_ram_toplevel')",
            "'objects_ram_toplevel' in meta",
        ),
        ("elem.has_key('type')", "'type' in elem"),
        ("bi.has_key('nargs')", "'nargs' in bi"),
        ("bi.has_key('callable')", "'callable' in bi"),
        (
            "bi.has_key('internal_prototype')",
            "'internal_prototype' in bi",
        ),
        ("not emitted.has_key(fname)", "fname not in emitted"),
        ("v.has_key('getter_id')", "'getter_id' in v"),
        ("v.has_key('length')", "'length' in v"),
        ("v.has_key('magic')", "'magic' in v"),
        (
            "not chain_lens.has_key(chainlen)",
            "chainlen not in chain_lens",
        ),
        ("reserved_words.has_key(v)", "v in reserved_words"),
        (
            "strict_reserved_words.has_key(v)",
            "v in strict_reserved_words",
        ),
        ("romstr_next.has_key(v)", "v in romstr_next"),
        (
            "if obj.has_key('internal_prototype')",
            "if 'internal_prototype' in obj",
        ),
        ("elif obj.has_key('nargs')", "elif 'nargs' in obj"),
        ("not emitted.has_key(fname)", "fname not in emitted"),
        ("assert(v.has_key('native'))", "assert('native' in v)"),
        ("target.has_key('native')", "'native' in target"),
        ("not reachable.has_key(o['id'])", "o['id'] not in reachable"),
        ("string_to_stridx.has_key(val)", "val in string_to_stridx"),
        ("val.has_key('getter_id')", "'getter_id' in val"),
        ("val.has_key('setter_id')", "'setter_id' in val"),
        ("funobj.has_key('nargs')", "'nargs' in funobj"),
        ("not defs_found.has_key(k)", "k not in defs_found"),
        (
            "metadata_lookup_object(meta, prop['value']['id']).has_key('native')",
            "'native' in metadata_lookup_object(meta, prop['value']['id'])",
        ),
        (
            "not metadata_lookup_object(meta, prop['value']['id']).has_key('bidx')",
            "'bidx' not in metadata_lookup_object(meta, prop['value']['id'])",
        ),
    ] {
        genbuiltins_body = genbuiltins_body.replace(old, new);
    }
    fs::write(genbuiltins, genbuiltins_body)?;
    let dukutil = source.join("tools/dukutil.py");
    let dukutil_body = fs::read_to_string(&dukutil)?
        .replace("xrange", "range")
        .replace("unicode", "str")
        // Duktape's generators use Python 2 ``str`` as a byte string.  The
        // Python 3 compatibility alias above makes that value a Unicode
        // string, so emitArray() must restore the original one-byte mapping
        // instead of UTF-8 expanding values above 0xff.  Those expansions
        // silently truncate generated tables in C and make every Duktape
        // evaluation fatal at runtime.
        .replace("data = data.encode('utf-8')", "data = data.encode('latin-1')")
        .replace("return nbits / 8", "return nbits // 8")
        .replace("(skip * (res % 256)) / 256", "(skip * (res % 256)) // 256")
        .replace(
            "ord(x[i])",
            "(x[i] if isinstance(x[i], int) else ord(x[i]))",
        );
    fs::write(dukutil, dukutil_body)?;
    let unicode_prepare = source.join("tools/prepare_unicode_data.py");
    let unicode_prepare_body = fs::read_to_string(&unicode_prepare)?
        .replace("#!/usr/bin/env python2", "#!/usr/bin/env python3")
        .replace("import os", "from functools import cmp_to_key\nlong = int\nxrange = range\ncmp = lambda a, b: (a > b) - (a < b)\nimport os")
        .replace("open(opts.unicode_data, 'rb')", "open(opts.unicode_data, 'r', encoding='utf-8')")
        .replace("open(opts.output, 'wb')", "open(opts.output, 'w', encoding='utf-8')");
    fs::write(unicode_prepare, unicode_prepare_body)?;
    let extract_chars = source.join("tools/extract_chars.py");
    let mut extract_chars_body = fs::read_to_string(&extract_chars)?
        .replace("#!/usr/bin/env python2", "#!/usr/bin/env python3")
        .replace("import os", "from functools import cmp_to_key\nlong = int\nxrange = range\ncmp = lambda a, b: (a > b) - (a < b)\nimport os")
        .replace("open(unidata, 'rb')", "open(unidata, 'r', encoding='utf-8')")
        .replace("open(opts.out_source, 'wb')", "open(opts.out_source, 'w', encoding='utf-8')")
        .replace("open(opts.out_header, 'wb')", "open(opts.out_header, 'w', encoding='utf-8')");
    for (old, new) in [
        (
            "exclude_cat_exact.has_key(category)",
            "category in exclude_cat_exact",
        ),
        (
            "include_cat_exact.has_key(category)",
            "category in include_cat_exact",
        ),
        ("m.has_key(long(cp))", "long(cp) in m"),
        (
            "print 'CATSEXC: %s' % repr(catsexc)",
            "print('CATSEXC: %s' % repr(catsexc))",
        ),
        (
            "print 'CATSINC: %s' % repr(catsinc)",
            "print('CATSINC: %s' % repr(catsinc))",
        ),
        (
            "print 'match table length: %d bytes' % len(matchtable3)",
            "print('match table length: %d bytes' % len(matchtable3))",
        ),
        ("print 'encoding freq:'", "print('encoding freq:')"),
        (
            "print '  %6d: %d' % (i, freq[i])",
            "print('  %6d: %d' % (i, freq[i]))",
        ),
    ] {
        extract_chars_body = extract_chars_body.replace(old, new);
    }
    extract_chars_body =
        extract_chars_body.replace("res.sort(cmp=mycmp)", "res.sort(key=cmp_to_key(mycmp))");
    fs::write(extract_chars, extract_chars_body)?;
    let extract_caseconv = source.join("tools/extract_caseconv.py");
    let mut extract_caseconv_body = fs::read_to_string(&extract_caseconv)?
        .replace("#!/usr/bin/env python2", "#!/usr/bin/env python3")
        .replace("import os", "from functools import cmp_to_key\nlong = int\nxrange = range\nunichr = chr\ncmp = lambda a, b: (a > b) - (a < b)\nimport os")
        .replace("open(filename, 'rb')", "open(filename, 'r', encoding='utf-8')")
        .replace("open(opts.out_source, 'wb')", "open(opts.out_source, 'w', encoding='utf-8')")
        .replace("open(opts.out_header, 'wb')", "open(opts.out_header, 'w', encoding='utf-8')")
        .replace("res.sort(cmp=mycmp)", "res.sort(key=cmp_to_key(mycmp))");
    for (old, new) in [
        ("convmap.has_key(i)", "i in convmap"),
        ("not convmap.has_key(conv_i)", "conv_i not in convmap"),
        ("not convmap.has_key(new_i)", "new_i not in convmap"),
        ("convmap.has_key(cp)", "cp in convmap"),
    ] {
        extract_caseconv_body = extract_caseconv_body.replace(old, new);
    }
    for (old, new) in [
        (
            "print '- singles: ' + repr(t)",
            "print('- singles: ' + repr(t))",
        ),
        (
            "print '- multis: ' + repr(t)",
            "print('- multis: ' + repr(t))",
        ),
        (
            "print '- range mappings: %d' % len(ranges)",
            "print('- range mappings: %d' % len(ranges))",
        ),
        (
            "print '- single character mappings: %d' % len(singles)",
            "print('- single character mappings: %d' % len(singles))",
        ),
        (
            "print '- complex mappings (1:n): %d' % len(multis)",
            "print('- complex mappings (1:n): %d' % len(multis))",
        ),
        (
            "print '- remaining (should be zero): %d' % len(convmap.keys())",
            "print('- remaining (should be zero): %d' % len(convmap.keys()))",
        ),
        (
            "print '- %d %d' % (t[0] - prev[0], t[1] - prev[1])",
            "print('- %d %d' % (t[0] - prev[0], t[1] - prev[1]))",
        ),
        (
            "print '- start: %d %d' % (t[0], t[1])",
            "print('- start: %d %d' % (t[0], t[1]))",
        ),
    ] {
        extract_caseconv_body = extract_caseconv_body.replace(old, new);
    }
    extract_caseconv_body =
        extract_caseconv_body.replace("k = convmap.keys()", "k = list(convmap.keys())");
    extract_caseconv_body = extract_caseconv_body
        .replace(
            "(conv_i - start_i) / skip + 1",
            "(conv_i - start_i) // skip + 1",
        )
        .replace("65536 / block_size", "65536 // block_size");
    fs::write(extract_caseconv, extract_caseconv_body)?;
    let combine_src = source.join("tools/combine_src.py");
    let mut combine_src_body = fs::read_to_string(&combine_src)?
        .replace("#!/usr/bin/env python2", "#!/usr/bin/env python3")
        .replace("import logging", "unicode = str\nimport logging")
        .replace(
            "open(filename, 'rb')",
            "open(filename, 'r', encoding='utf-8')",
        )
        .replace(
            "open(prologue_filename, 'rb')",
            "open(prologue_filename, 'r', encoding='utf-8')",
        )
        .replace(
            "open(opts.output_source, 'wb')",
            "open(opts.output_source, 'w', encoding='utf-8')",
        )
        .replace(
            "open(opts.output_metadata, 'wb')",
            "open(opts.output_metadata, 'w', encoding='utf-8')",
        )
        .replace(
            "apply(os.path.join, [ path ] + inccomp)",
            "os.path.join(path, *inccomp)",
        );
    for (old, new) in [
        ("defined.has_key(m.group(1))", "m.group(1) in defined"),
        ("included.has_key(incpath)", "incpath in included"),
    ] {
        combine_src_body = combine_src_body.replace(old, new);
    }
    fs::write(combine_src, combine_src_body)?;
    let prep = source.join("prep/nondebug");
    fs::create_dir_all(source.join("prep"))?;
    run_cmd(
        &source,
        "python3",
        &[
            "tools/configure.py",
            "--output-directory",
            "prep/nondebug",
            "--source-directory",
            "src-input",
            "--config-metadata",
            "config",
            "--option-file",
            "util/makeduk_base.yaml",
            "--line-directives",
        ],
    )?;
    fs::create_dir_all(install.join("lib/x86_64-linux-gnu/pkgconfig"))?;
    let lib = install.join("lib/x86_64-linux-gnu/libduktape.so.207.2.7.0");
    run_cmd_with_env_overrides(
        &source,
        "cc",
        &[
            "-shared",
            "-fPIC",
            "-O2",
            "-Iprep/nondebug",
            "-Wl,-soname,libduktape.so.207",
            "-o",
            path_str(&lib)?,
            "prep/nondebug/duktape.c",
            "-lm",
        ],
        &[],
    )?;
    std::os::unix::fs::symlink(
        "libduktape.so.207.2.7.0",
        install.join("lib/x86_64-linux-gnu/libduktape.so.207"),
    )?;
    std::os::unix::fs::symlink(
        "libduktape.so.207",
        install.join("lib/x86_64-linux-gnu/libduktape.so"),
    )?;
    fs::create_dir_all(install.join("include"))?;
    for name in ["duktape.h", "duk_config.h"] {
        fs::copy(prep.join(name), install.join("include").join(name))?;
    }
    fs::write(
        install.join("lib/x86_64-linux-gnu/pkgconfig/duktape.pc"),
        "prefix=/usr\nlibdir=${prefix}/lib/x86_64-linux-gnu\nincludedir=${prefix}/include\nName: duktape\nDescription: Duktape JavaScript engine\nVersion: 2.7.0\nLibs: -L${libdir} -lduktape\nCflags: -I${includedir}\n",
    )?;
    Ok(())
}

fn rewrite_python2_has_key(mut body: String) -> String {
    while let Some(marker) = body.find(".has_key(") {
        let lhs_end = marker;
        let mut lhs_start = lhs_end;
        while lhs_start > 0 {
            let byte = body.as_bytes()[lhs_start - 1];
            if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' {
                lhs_start -= 1;
            } else {
                break;
            }
        }
        let arg_start = marker + ".has_key(".len();
        let Some(arg_end_rel) = body[arg_start..].find(')') else {
            break;
        };
        let arg_end = arg_start + arg_end_rel;
        let lhs = body[lhs_start..lhs_end].to_string();
        let arg = body[arg_start..arg_end].trim().to_string();
        let before = &body[..lhs_start];
        let negated = before.trim_end().ends_with("not");
        let replacement = if negated {
            format!("{} not in {}", arg, lhs)
        } else {
            format!("{} in {}", arg, lhs)
        };
        let replace_start = if negated {
            before.trim_end().len() - 3
        } else {
            lhs_start
        };
        body.replace_range(replace_start..=arg_end, &replacement);
    }
    body
}

fn build_networkmanager(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "networkmanager",
        "src/system/network/NetworkManager",
        &[
            "glib",
            "systemd",
            "dbus",
            "polkit",
            "iproute2",
            "util-linux",
            "libndp",
            "zlib",
            "readline",
            "ncurses",
            "libffi",
        ],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=no",
            "-Ddocs=false",
            "-Dman=false",
            "-Dpolkit=true",
            "-Dnmcli=true",
            "-Dnmtui=false",
            "-Dwifi=true",
            "-Dmodem_manager=false",
            "-Dovs=false",
            "-Dclat=false",
            "-Dconcheck=false",
            "-Dppp=false",
            "-Dlibpsl=false",
            "-Dcrypto=null",
            "-Dsession_tracking=systemd",
            "-Dconfig_dns_rc_manager_default=symlink",
            "-Dconfig_auth_polkit_default=true",
            "-Dintrospection=false",
            "-Dselinux=false",
            "-Dlibaudit=no",
            "-Dnm_cloud_setup=false",
            "-Dnbft=false",
            "-Dsystemdsystemunitdir=/usr/lib/systemd/system",
            "-Ddbus_conf_dir=/usr/share/dbus-1/system.d",
        ],
        "usr/sbin/NetworkManager",
        &[],
    )
}

fn build_cozy(repo_root: &Path) -> Result<()> {
    let out_root = repo_root.join("out/build/cozy");
    let install = out_root.join("install");
    let mirror = out_root.join("source");
    remove_path_if_exists(&install)?;
    sync_build_source(&repo_root.join("src/userland/cozy"), &mirror)?;
    isolate_cargo_build_mirror(&mirror)?;
    let target = out_root.join("cargo-target");
    run_cmd_with_env_overrides(
        &mirror,
        "cargo",
        &["build", "--locked", "--release", "--bin", "cozy"],
        &[
            ("CARGO_TARGET_DIR", target.display().to_string()),
            ("CARGO_BUILD_JOBS", "4".to_string()),
            ("CARGO_INCREMENTAL", "0".to_string()),
            (
                "RUSTFLAGS",
                format!(
                    "--remap-path-prefix={}=/usr/src/mattos",
                    repo_root.display()
                ),
            ),
        ],
    )?;
    stage_output_file(
        &target.join("release/cozy"),
        &install.join("usr/bin/cozy"),
        0o755,
    )
}

fn stage_output_file(source: &Path, destination: &Path, mode: u32) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)
        .with_context(|| format!("failed to stage {}", source.display()))?;
    set_mode(destination.to_path_buf(), mode)
}

fn sanitize_embedded_output_path(binary: &Path, mirror: &Path) -> Result<()> {
    let old = mirror.to_string_lossy().into_owned();
    let replacement = "/usr/src/mattos/cosmic-sources/cosmic-workspaces";
    if replacement.len() > old.len() {
        bail!("sanitized output path is longer than the embedded host path");
    }
    let mut bytes = fs::read(binary)?;
    let old_bytes = old.as_bytes();
    let mut replacements = 0;
    let mut offset = 0;
    while let Some(relative) = bytes[offset..]
        .windows(old_bytes.len())
        .position(|window| window == old_bytes)
    {
        let start = offset + relative;
        bytes[start..start + old_bytes.len()].fill(0);
        bytes[start..start + replacement.len()].copy_from_slice(replacement.as_bytes());
        replacements += 1;
        offset = start + replacement.len();
    }
    if replacements == 0 {
        bail!(
            "{} did not contain the expected embedded mirror path",
            binary.display()
        );
    }
    fs::write(binary, bytes)?;
    Ok(())
}

fn copy_file_preserving(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)?;
    let permissions = fs::metadata(source)?.permissions();
    fs::set_permissions(destination, permissions)?;
    Ok(())
}

#[allow(dead_code)]
fn build_cosmic_desktop_legacy(repo_root: &Path) -> Result<()> {
    const JUST_COMPONENTS: &[&str] = &[
        "cosmic-session",
        "cosmic-greeter",
        "cosmic-panel",
        "cosmic-applets",
        "cosmic-applibrary",
        "cosmic-launcher",
        "cosmic-settings",
        "cosmic-notifications",
        "cosmic-osd",
        "cosmic-bg",
        "cosmic-files",
        "cosmic-term",
        "cosmic-randr",
        "cosmic-screenshot",
        "pop-launcher",
    ];
    const MAKE_COMPONENTS: &[&str] = &["cosmic-settings-daemon", "cosmic-workspaces"];

    let out_root = repo_root.join("out/build/cosmic-desktop");
    let sources = out_root.join("sources");
    let install = out_root.join("install");
    remove_path_if_exists(&install)?;
    fs::create_dir_all(&sources)?;
    fs::create_dir_all(&install)?;

    // Upstream COSMIC uses `just` as its install/build recipe runner.  Keep
    // this build-only Rust dependency output-owned instead of requiring an
    // untracked host executable.
    let just_root = repo_root.join("out/tools/cosmic-just");
    let just = just_root.join("bin/just");
    if !just.is_file() {
        fs::create_dir_all(&just_root)?;
        let root_arg = format!("--root={}", just_root.display());
        run_cmd_with_env_overrides(
            repo_root,
            "cargo",
            &[
                "install",
                "just",
                "--version",
                "1.40.0",
                "--locked",
                root_arg.as_str(),
            ],
            &[("CARGO_BUILD_JOBS", "4".to_string())],
        )?;
    }
    let just_program = just
        .to_str()
        .ok_or_else(|| anyhow!("non-UTF-8 output-owned just path"))?;

    let native_components = [
        "glibc",
        "gcc-runtime",
        "openssl",
        "zlib",
        "zstd",
        "wayland",
        "xkbcommon",
        "mesa",
        "libdrm",
        "libinput",
        "systemd",
        "dbus",
        "dbus-broker",
        "dav1d",
        "glib",
        "pipewire",
    ];
    let native = staged_library_environment(repo_root, &native_components)?;
    let mut common_env = native;
    let inherited_path = common_env
        .iter()
        .find_map(|(key, value)| (*key == "PATH").then_some(value.as_str()))
        .unwrap_or_default();
    let tool_path = std::env::join_paths(
        std::iter::once(just_root.join("bin")).chain(std::env::split_paths(inherited_path)),
    )?
    .to_string_lossy()
    .to_string();
    if let Some((_, value)) = common_env.iter_mut().find(|(key, _)| *key == "PATH") {
        *value = tool_path;
    }
    common_env.push(("CARGO_BUILD_JOBS", "4".to_string()));
    common_env.push(("CARGO_INCREMENTAL", "0".to_string()));
    // All COSMIC applications use the same pinned libcosmic stack. Sharing
    // Cargo's output-owned target cache avoids rebuilding that dependency
    // graph independently for every upstream workspace; builds remain
    // sequential here, so Cargo never has concurrent writers to the cache.
    common_env.push((
        "CARGO_TARGET_DIR",
        out_root.join("cargo-target").display().to_string(),
    ));
    common_env.push(("RUSTFLAGS", cosmic_source_remap_flags(repo_root)));
    // Distribution binaries do not need every COSMIC application to perform
    // a separate whole-program ThinLTO pass. This materially reduces peak
    // link memory and wall time without changing enabled functionality.
    common_env.push(("CARGO_PROFILE_RELEASE_LTO", "false".to_string()));
    common_env.push(("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "4".to_string()));
    common_env.push(("DESTDIR", install.display().to_string()));

    for component in JUST_COMPONENTS {
        let source = repo_root.join("src/desktop/cosmic").join(component);
        let mirror = sources.join(component);
        sync_build_source(&source, &mirror)?;
        isolate_cargo_build_mirror(&mirror)?;
        // cosmic-launcher and cosmic-notifications derive their profile name
        // from env!("OUT_DIR").  Besides being unnecessary for a fixed
        // distribution release build, that embeds Cargo's absolute output
        // directory in the ELF payload.  Patch only the output-owned mirror;
        // the authoritative pinned upstream sources remain untouched.
        if matches!(*component, "cosmic-launcher" | "cosmic-notifications") {
            let config = mirror.join("src/config.rs");
            let original = fs::read_to_string(&config)?;
            let profile_helper = r#"pub fn profile() -> &'static str {
    std::env!("OUT_DIR")
        .split(std::path::MAIN_SEPARATOR)
        .nth_back(3)
        .unwrap_or("unknown")
}"#;
            if !original.contains(profile_helper) {
                bail!(
                    "{} no longer contains the expected OUT_DIR profile helper",
                    config.display()
                );
            }
            fs::write(
                &config,
                original.replace(
                    profile_helper,
                    "pub fn profile() -> &'static str {\n    \"release\"\n}",
                ),
            )?;
        }
        run_cmd_with_env_overrides(
            &mirror,
            just_program,
            &["build-release", "--locked"],
            &common_env,
        )?;
        let rootdir = format!("rootdir={}", install.display());
        let pop_launcher_target_dir = common_env
            .iter()
            .find(|(key, _)| *key == "CARGO_TARGET_DIR")
            .map(|(_, value)| format!("target-dir={}/release", value));
        let install_args = if *component == "pop-launcher" {
            let mut args = vec![rootdir.as_str(), "install"];
            if let Some(target_dir) = pop_launcher_target_dir.as_deref() {
                args.insert(0, target_dir);
            }
            args
        } else {
            vec![rootdir.as_str(), "prefix=/usr", "install"]
        };
        run_cmd_with_env_overrides(&mirror, just_program, &install_args, &common_env)?;
    }

    for component in MAKE_COMPONENTS {
        let source = repo_root.join("src/desktop/cosmic").join(component);
        let mirror = sources.join(component);
        sync_build_source(&source, &mirror)?;
        isolate_cargo_build_mirror(&mirror)?;
        let target = out_root.join("cargo-target");
        let mut env = common_env.clone();
        env.push(("CARGO_TARGET_DIR", target.display().to_string()));
        run_cmd_with_env_overrides(&mirror, "make", &["-j4"], &env)?;
        let destdir = format!("DESTDIR={}", install.display());
        run_cmd_with_env_overrides(
            &mirror,
            "make",
            &[destdir.as_str(), "prefix=/usr", "install"],
            &env,
        )?;
    }

    // The portal uses just for installation but names its build recipe
    // `build`, not the `build-release` convention used by the applications.
    // Invoke Cargo explicitly so the checked-in lockfile remains mandatory.
    let portal_component = "xdg-desktop-portal-cosmic";
    let portal = sources.join(portal_component);
    sync_build_source(
        &repo_root.join("src/desktop/cosmic").join(portal_component),
        &portal,
    )?;
    isolate_cargo_build_mirror(&portal)?;
    let portal_target = out_root.join("cargo-target");
    let mut portal_env = common_env.clone();
    portal_env.push(("CARGO_TARGET_DIR", portal_target.display().to_string()));
    run_cmd_with_env_overrides(
        &portal,
        "cargo",
        &[
            "build",
            "--release",
            "--locked",
            "--bin",
            "xdg-desktop-portal-cosmic",
        ],
        &portal_env,
    )?;
    let portal_rootdir = format!("rootdir={}", install.display());
    run_cmd_with_env_overrides(
        &portal,
        just_program,
        &[portal_rootdir.as_str(), "prefix=/usr", "install"],
        &portal_env,
    )?;

    let icons = sources.join("cosmic-icons");
    sync_build_source(&repo_root.join("src/desktop/cosmic/cosmic-icons"), &icons)?;
    let icons_rootdir = format!("rootdir={}", install.display());
    run_cmd_with_env_overrides(
        &icons,
        just_program,
        &[icons_rootdir.as_str(), "prefix=/usr", "install"],
        &common_env,
    )?;
    // COSMIC does not ship a cursor set of its own.  Use the source-owned Pop
    // cursor theme that upstream COSMIC distributions pair with the desktop.
    copy_tree_contents(
        &repo_root.join("src/desktop/themes/pop-icon-theme/Pop/cursors"),
        &install.join("usr/share/icons/Pop/cursors"),
    )?;
    for metadata in ["index.theme", "cursor.theme"] {
        let source = repo_root
            .join("src/desktop/themes/pop-icon-theme/Pop")
            .join(metadata);
        if source.is_file() {
            let destination = install.join("usr/share/icons/Pop").join(metadata);
            fs::create_dir_all(destination.parent().expect("Pop theme parent"))?;
            fs::copy(&source, destination)?;
        }
    }
    // Match libcosmic's source defaults instead of relying on host font
    // discovery. Open Sans is the interface family and Noto Sans Mono is
    // required by cosmic-term; Pop's Fira families provide a broad fallback.
    copy_tree_contents(
        &repo_root.join("src/desktop/fonts/open-sans/fonts/ttf"),
        &install.join("usr/share/fonts/truetype/open-sans"),
    )?;
    copy_tree_contents(
        &repo_root.join("src/desktop/fonts/noto-sans-mono"),
        &install.join("usr/share/fonts/truetype/noto"),
    )?;
    copy_tree_contents(
        &repo_root.join("src/desktop/fonts/pop-fonts/fira"),
        &install.join("usr/share/fonts/opentype/fira"),
    )?;

    let greetd = sources.join("greetd");
    sync_build_source(&repo_root.join("src/system/session/greetd"), &greetd)?;
    isolate_cargo_build_mirror(&greetd)?;
    let greetd_target = greetd.join("target");
    let mut greetd_env = common_env.clone();
    greetd_env.push(("CARGO_TARGET_DIR", greetd_target.display().to_string()));
    run_cmd_with_env_overrides(
        &greetd,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "-p",
            "greetd",
            "-p",
            "agreety",
        ],
        &greetd_env,
    )?;
    for binary in ["greetd", "agreety"] {
        let destination = install.join("usr/bin").join(binary);
        fs::create_dir_all(destination.parent().expect("greetd bin parent"))?;
        fs::copy(greetd_target.join("release").join(binary), &destination)?;
        set_mode(destination, 0o755)?;
    }

    for required in [
        "usr/bin/cosmic-session",
        "usr/bin/cosmic-panel",
        "usr/bin/cosmic-launcher",
        "usr/bin/cosmic-settings-daemon",
        "usr/bin/cosmic-notifications",
        "usr/bin/cosmic-osd",
        "usr/bin/cosmic-bg",
        "usr/bin/cosmic-workspaces",
        "usr/bin/cosmic-files",
        "usr/bin/cosmic-term",
        "usr/bin/cosmic-ext-tweaks",
        "usr/bin/greetd",
        "usr/share/wayland-sessions/cosmic.desktop",
        "usr/share/fonts/truetype/open-sans/OpenSans-Regular.ttf",
        "usr/share/fonts/truetype/noto/NotoSansMono[wdth,wght].ttf",
    ] {
        if !install.join(required).is_file() {
            bail!("COSMIC desktop build did not install /{required}");
        }
    }
    Ok(())
}

include!("stages/helpers/cargo.rs");
fn build_cpython(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/development/python/cpython");
    let out_root = repo_root.join("out/build/cpython");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/cpython.toml"))?;
    let openssl = repo_root.join("out/build/openssl/install/usr");
    let options = vec![
        "--prefix=/usr".to_string(),
        "--libdir=/usr/lib/x86_64-linux-gnu".to_string(),
        "--enable-shared".to_string(),
        "--without-static-libpython".to_string(),
        "--with-ensurepip=install".to_string(),
        "--with-system-expat".to_string(),
        "--disable-test-modules".to_string(),
        format!("--with-openssl={}", openssl.display()),
    ];
    let stamp = format!(
        "{state}\n{}\nlib-dynload=/usr/lib/python3.14/lib-dynload\noptional-modules=no-gdbm,no-readline,no-sqlite3,no-tk,no-uuid\n",
        options.join("\n")
    );
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    fs::create_dir_all(&build_dir)?;
    let mut env = staged_library_environment(
        repo_root,
        &[
            "openssl", "zlib", "bzip2", "xz", "expat", "ncurses", "libffi",
        ],
    )?;
    env.push(("PYTHON_FOR_BUILD", "python3".to_string()));
    if !build_dir.join("Makefile").is_file() {
        let option_refs = options.iter().map(String::as_str).collect::<Vec<_>>();
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &option_refs,
            &env,
        )?;
    }
    restore_cpython_getpath_vpath(&build_dir)?;
    // A prior interrupted run may have already produced a normalized getpath
    // object. Force the bootstrap interpreter back to the real output-mirror
    // source path before any remaining frozen-module generation.
    remove_path_if_exists(&build_dir.join("Modules/getpath.o"))?;
    let child_jobs = scheduler::child_job_limit().to_string();
    run_cmd_with_env_overrides(&build_dir, "make", &["_bootstrap_python"], &env)?;
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", &child_jobs], &env)?;
    // The bootstrap interpreter needs the real source VPATH while producing
    // frozen modules. Once generation is complete, rebuild only the owning
    // getpath object and its consumers with the deterministic installed-tree
    // fallback before publishing libpython.
    normalize_cpython_getpath_vpath(&build_dir)?;
    remove_path_if_exists(&build_dir.join("Modules/getpath.o"))?;
    // Frozen headers were completed by the real-VPATH bootstrap pass above.
    // Do not rebuild the bootstrap interpreter (and thereby make those headers
    // stale) while relinking only the installed shared library.
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["FREEZE_MODULE_DEPS=", "libpython3.14.so"],
        &env,
    )?;
    let normalized_libpython = out_root.join("libpython3.14.so.1.0.normalized");
    fs::copy(
        build_dir.join("libpython3.14.so.1.0"),
        &normalized_libpython,
    )?;
    // CPython's install recipes also execute the bootstrap interpreter. Put
    // that private build tool back on its real output-mirror path; the
    // installed library is restored from the valid normalized link above.
    restore_cpython_getpath_vpath(&build_dir)?;
    remove_path_if_exists(&build_dir.join("Modules/getpath.o"))?;
    run_cmd_with_env_overrides(&build_dir, "make", &["_bootstrap_python"], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env,
    )?;
    fs::copy(
        &normalized_libpython,
        install_dir.join("usr/lib/x86_64-linux-gnu/libpython3.14.so.1.0"),
    )?;
    // CPython applies --libdir to both libpython and extension modules, but its
    // installed path configuration searches for extension modules below the
    // platform-independent standard-library root. Keep libpython in Debian's
    // multiarch directory while publishing lib-dynload where python3 searches.
    let multiarch_dynload = install_dir.join("usr/lib/x86_64-linux-gnu/python3.14/lib-dynload");
    let runtime_dynload = install_dir.join("usr/lib/python3.14/lib-dynload");
    if multiarch_dynload.is_dir() {
        remove_path_if_exists(&runtime_dynload)?;
        if let Some(parent) = runtime_dynload.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&multiarch_dynload, &runtime_dynload)?;
    }
    for required in [
        "usr/bin/python3",
        "usr/lib/x86_64-linux-gnu/libpython3.14.so.1.0",
        "usr/lib/python3.14/os.py",
        "usr/lib/python3.14/lib-dynload/_ctypes.cpython-314-x86_64-linux-gnu.so",
        "usr/include/python3.14/Python.h",
    ] {
        if !install_dir.join(required).exists() {
            bail!("CPython install did not produce {required}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

/// Keep Make's real VPATH for source discovery while preventing CPython's
/// generated getpath object from compiling that checkout path into libpython.
/// Installed Python resolves its standard library from the executable prefix;
/// this macro is only a development-tree fallback.
fn normalize_cpython_getpath_vpath(build_dir: &Path) -> Result<()> {
    let makefile = build_dir.join("Makefile");
    let mut contents = fs::read_to_string(&makefile)
        .with_context(|| format!("read generated {}", makefile.display()))?;
    let original = "-DVPATH='\"$(VPATH)\"'";
    let normalized = "-DVPATH='\"/usr/src/mattos/cpython\"'";
    if contents.contains(original) {
        contents = contents.replacen(original, normalized, 1);
    } else if !contents.contains(normalized) {
        bail!(
            "generated {} lacks expected CPython getpath VPATH definition",
            makefile.display()
        );
    }
    fs::write(&makefile, contents)
        .with_context(|| format!("normalize generated {}", makefile.display()))?;
    Ok(())
}

fn restore_cpython_getpath_vpath(build_dir: &Path) -> Result<()> {
    let makefile = build_dir.join("Makefile");
    let mut contents = fs::read_to_string(&makefile)
        .with_context(|| format!("read generated {}", makefile.display()))?;
    let original = "-DVPATH='\"$(VPATH)\"'";
    let normalized = "-DVPATH='\"/usr/src/mattos/cpython\"'";
    if contents.contains(normalized) {
        contents = contents.replacen(normalized, original, 1);
        fs::write(&makefile, contents)
            .with_context(|| format!("restore generated {}", makefile.display()))?;
    } else if !contents.contains(original) {
        bail!(
            "generated {} lacks expected CPython getpath VPATH definition",
            makefile.display()
        );
    }
    Ok(())
}

fn build_llvm(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/toolchain/llvm-project/llvm");
    let out_root = repo_root.join("out/build/llvm");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/llvm.toml"))?;
    let options = vec![
        "-G".to_string(),
        "Ninja".to_string(),
        format!("-S{}", source.display()),
        format!("-B{}", build_dir.display()),
        "-DCMAKE_BUILD_TYPE=Release".to_string(),
        "-DCMAKE_INSTALL_PREFIX=/usr".to_string(),
        // MattOS deliberately normalizes llvm-config's generated development-tree
        // roots after configuration.  Suppress Ninja's implicit CMake rerun so
        // that it cannot silently regenerate BuildVariables.inc afterward.
        "-DCMAKE_SUPPRESS_REGENERATION=ON".to_string(),
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu".to_string(),
        "-DLLVM_LIBDIR_SUFFIX=/x86_64-linux-gnu".to_string(),
        "-DLLVM_INSTALL_PACKAGE_DIR=lib/x86_64-linux-gnu/cmake/llvm".to_string(),
        "-DCLANG_INSTALL_PACKAGE_DIR=lib/x86_64-linux-gnu/cmake/clang".to_string(),
        "-DCLANG_CONFIG_FILE_SYSTEM_DIR=/etc/clang".to_string(),
        "-DLLD_INSTALL_PACKAGE_DIR=lib/x86_64-linux-gnu/cmake/lld".to_string(),
        "-DLLVM_FORCE_VC_REPOSITORY=https://github.com/llvm/llvm-project.git".to_string(),
        "-DLLVM_FORCE_VC_REVISION=ca7933e47d3a3451d81e72ac174dcb5aa28b59d1".to_string(),
        "-DLLVM_ENABLE_PROJECTS=clang;lld".to_string(),
        // AMDGPU is a userspace compiler backend required by radeonsi/RADV;
        // it does not add a MattOS CPU architecture target.
        "-DLLVM_TARGETS_TO_BUILD=X86;AArch64;RISCV;AMDGPU".to_string(),
        "-DLLVM_ENABLE_ASSERTIONS=OFF".to_string(),
        "-DLLVM_INCLUDE_TESTS=OFF".to_string(),
        "-DLLVM_INCLUDE_EXAMPLES=OFF".to_string(),
        "-DLLVM_INCLUDE_BENCHMARKS=OFF".to_string(),
        "-DLLVM_ENABLE_BINDINGS=OFF".to_string(),
        "-DLLVM_ENABLE_TERMINFO=OFF".to_string(),
        "-DLLVM_ENABLE_LIBXML2=OFF".to_string(),
        "-DLLVM_ENABLE_LIBEDIT=OFF".to_string(),
        "-DLLVM_ENABLE_ZLIB=FORCE_ON".to_string(),
        "-DLLVM_ENABLE_ZSTD=FORCE_ON".to_string(),
        "-DLLVM_BUILD_LLVM_DYLIB=ON".to_string(),
        "-DLLVM_LINK_LLVM_DYLIB=ON".to_string(),
        "-DCLANG_LINK_CLANG_DYLIB=ON".to_string(),
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    let stamp_path = out_root.join("build-stamp.txt");
    let configuration_changed =
        fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str());
    fs::create_dir_all(&out_root)?;
    let env = staged_library_environment(repo_root, &["zlib", "zstd"])?;
    if configuration_changed || !build_dir.join("build.ninja").is_file() {
        let option_refs = options.iter().map(String::as_str).collect::<Vec<_>>();
        run_cmd_with_env_overrides(repo_root, "cmake", &option_refs, &env)?;
    }
    normalize_llvm_config_build_roots(repo_root, &build_dir)?;
    let child_jobs = scheduler::child_job_limit().to_string();
    run_cmd_with_env_overrides(&build_dir, "ninja", &["-j", &child_jobs], &env)?;
    remove_path_if_exists(&install_dir)?;
    let destdir_env = [("DESTDIR", install_dir.display().to_string())];
    run_cmd_with_env_overrides(&build_dir, "ninja", &["install"], &destdir_env)?;
    fs::copy(
        build_dir.join("bin/FileCheck"),
        install_dir.join("usr/bin/FileCheck"),
    )?;
    let clang_config_dir = install_dir.join("etc/clang");
    fs::create_dir_all(&clang_config_dir)?;
    fs::write(
        clang_config_dir.join("clang.cfg"),
        format!("--gcc-install-dir={MATTOS_GCC_INSTALL_DIR}\n"),
    )?;
    fs::write(
        clang_config_dir.join("clang++.cfg"),
        format!(
            "--gcc-install-dir={MATTOS_GCC_INSTALL_DIR}\n-isystem/usr/include/c++/15.3.0\n-isystem/usr/include/c++/15.3.0/x86_64-pc-linux-gnu\n"
        ),
    )?;
    for required in [
        "usr/bin/clang",
        "usr/bin/clang++",
        "usr/bin/ld.lld",
        "usr/bin/llvm-config",
        "usr/bin/FileCheck",
        "etc/clang/clang.cfg",
        "etc/clang/clang++.cfg",
    ] {
        if !install_dir.join(required).is_file() {
            bail!("LLVM install did not produce {required}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

/// Replace llvm-config's output-generated development-tree identities with
/// deterministic, relocatable identities before compiling the tool.
///
/// LLVM generates these two macros from the absolute CMake source and object
/// directories. They are useful only when running llvm-config from that exact
/// build tree; an installed llvm-config derives its prefix from argv[0]. Keeping
/// checkout-specific literals in the installed ELF leaks the builder path and
/// makes otherwise identical builds differ by checkout location. The imported
/// LLVM source is never changed: only CMake's output-owned generated header is
/// normalized, and the exact expected input is checked fail-closed.
fn normalize_llvm_config_build_roots(repo_root: &Path, build_dir: &Path) -> Result<()> {
    let generated = build_dir.join("tools/llvm-config/BuildVariables.inc");
    let mut contents = fs::read_to_string(&generated)
        .with_context(|| format!("read generated {}", generated.display()))?;
    let source_line = format!(
        "#define LLVM_SRC_ROOT \"{}\"",
        repo_root.join("src/toolchain/llvm-project/llvm").display()
    );
    let object_line = format!("#define LLVM_OBJ_ROOT \"{}\"", build_dir.display());
    for (actual, normalized) in [
        (
            &source_line,
            "#define LLVM_SRC_ROOT \"/usr/src/mattos/llvm\"",
        ),
        (
            &object_line,
            "#define LLVM_OBJ_ROOT \"/usr/lib/llvm-22/build\"",
        ),
    ] {
        if contents.contains(actual) {
            contents = contents.replacen(actual, normalized, 1);
        } else if !contents.contains(normalized) {
            bail!(
                "generated {} lacks expected LLVM build-root definition: {}",
                generated.display(),
                actual
            );
        }
    }
    fs::write(&generated, contents)
        .with_context(|| format!("normalize generated {}", generated.display()))?;
    Ok(())
}

fn build_rust(repo_root: &Path) -> Result<()> {
    let out_root = repo_root.join("out/build/rust");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    let archive = ensure_verified_release_archive(
        &out_root,
        "rustc-1.97.1-src.tar.xz",
        RUST_RELEASE_ARCHIVE_URL,
        RUST_RELEASE_ARCHIVE_SHA256,
    )?;
    if !source_copy.join("x.py").is_file() {
        stage_release_source(&archive, &source_copy)?;
    }
    isolate_standalone_cargo_manifest(&source_copy.join("src/bootstrap/Cargo.toml"))?;
    isolate_standalone_cargo_manifest(
        &source_copy.join("compiler/rustc_codegen_cranelift/Cargo.toml"),
    )?;
    isolate_standalone_cargo_manifest(&source_copy.join("compiler/rustc_codegen_gcc/Cargo.toml"))?;
    let llvm_config = repo_root.join("out/build/llvm/install/usr/bin/llvm-config");
    let llvm_filecheck = repo_root.join("out/build/llvm/install/usr/bin/FileCheck");
    let gcc = repo_root.join("out/build/gcc-toolchain/install/usr/bin/gcc");
    let gxx = repo_root.join("out/build/gcc-toolchain/install/usr/bin/g++");
    let ar = repo_root.join("out/build/binutils/install/usr/bin/ar");
    let ranlib = repo_root.join("out/build/binutils/install/usr/bin/ranlib");
    let sysroot = repo_root.join("out/sysroot");
    for required in [&llvm_config, &llvm_filecheck, &gcc, &gxx, &ar, &ranlib] {
        if !required.is_file() {
            bail!("Rust bootstrap dependency missing: {}", required.display());
        }
    }
    let wrappers = out_root.join("tool-wrappers");
    fs::create_dir_all(&wrappers)?;
    let gcc_wrapper = wrappers.join("gcc");
    let gxx_wrapper = wrappers.join("g++");
    let gcc_internal = sysroot.join(MATTOS_GCC_INSTALL_DIR.trim_start_matches('/'));
    let multiarch_lib = sysroot.join("usr/lib/x86_64-linux-gnu");
    let gcc_link_lib = repo_root.join("out/build/gcc-runtime/install/usr/lib/lib64");
    let cxx_include = sysroot.join("usr/include/c++/15.3.0");
    let cxx_target_include = cxx_include.join("x86_64-pc-linux-gnu");
    for (wrapper, compiler, language_flags) in [
        (&gcc_wrapper, &gcc, String::new()),
        (
            &gxx_wrapper,
            &gxx,
            format!(
                " -isystem{} -isystem{}",
                shell_escape(path_str(&cxx_include)?),
                shell_escape(path_str(&cxx_target_include)?),
            ),
        ),
    ] {
        fs::write(
            wrapper,
            format!(
                "#!/bin/sh\nexec {} --sysroot={} -B{} -B{} -L{}{} \"$@\"\n",
                shell_escape(path_str(compiler)?),
                shell_escape(path_str(&sysroot)?),
                shell_escape(path_str(&multiarch_lib)?),
                shell_escape(path_str(&gcc_internal)?),
                shell_escape(path_str(&gcc_link_lib)?),
                language_flags,
            ),
        )?;
        set_mode(wrapper.to_path_buf(), 0o755)?;
    }
    let child_jobs = scheduler::child_job_limit();
    let config = format!(
        "profile = \"compiler\"\nchange-id = 999999\n\n[llvm]\ndownload-ci-llvm = false\n\n[build]\nbuild = \"x86_64-unknown-linux-gnu\"\nhost = [\"x86_64-unknown-linux-gnu\"]\ntarget = [\"x86_64-unknown-linux-gnu\"]\njobs = {}\ndocs = false\nsubmodules = false\nvendor = true\nlocked-deps = true\nextended = true\ntools = [\"cargo\", \"rustdoc\"]\npython = \"python3\"\n\n[install]\nprefix = \"/usr\"\nsysconfdir = \"/etc\"\n\n[rust]\nchannel = \"stable\"\ndebug = false\ndebuginfo-level = 0\nstrip = true\n\n[target.x86_64-unknown-linux-gnu]\nllvm-config = \"{}\"\nllvm-filecheck = \"{}\"\nllvm-has-rust-patches = false\ncc = \"{}\"\ncxx = \"{}\"\nar = \"{}\"\nranlib = \"{}\"\nlinker = \"{}\"\nrustflags = [\"-C\", \"link-arg=--sysroot={}\", \"--remap-path-prefix={}=/usr/src/mattos/rust\"]\n",
        child_jobs,
        llvm_config.display(),
        llvm_filecheck.display(),
        gcc_wrapper.display(),
        gxx_wrapper.display(),
        ar.display(),
        ranlib.display(),
        gcc_wrapper.display(),
        sysroot.display(),
        repo_root.display(),
    );
    fs::write(source_copy.join("bootstrap.toml"), config)?;
    run_cmd(&source_copy, "python3", &["x.py", "build", "--stage", "2"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &source_copy,
        "python3",
        &["x.py", "install", "--stage", "2"],
        &[("DESTDIR", install_dir.display().to_string())],
    )?;
    for required in ["usr/bin/rustc", "usr/bin/cargo", "usr/bin/rustdoc"] {
        if !install_dir.join(required).is_file() {
            bail!("Rust install did not produce {required}");
        }
    }
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
    let cflags = format!(
        "-O2 -g0 -fPIC -ffile-prefix-map={}=/usr/src/mattos/bzip2 -fdebug-prefix-map={}=/usr/src/mattos/bzip2 -fmacro-prefix-map={}=/usr/src/mattos/bzip2",
        repo_root.display(),
        repo_root.display(),
        repo_root.display()
    );
    // Makefile-libbz2_so assigns CFLAGS with `=`, so an environment variable
    // alone is deliberately insufficient.  A make command-line assignment has
    // precedence and keeps the imported Makefile untouched.
    let cflags_override = format!("CFLAGS={cflags}");
    run_cmd_with_env_overrides(
        &source_copy,
        "make",
        &[
            "-B",
            "-f",
            "Makefile-libbz2_so",
            "-j",
            "4",
            &cflags_override,
        ],
        &[("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string())],
    )?;
    run_cmd_with_env_overrides(
        &source_copy,
        "make",
        &["-B", "-j", "4", &cflags_override],
        &[("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string())],
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
    let bindir = install_dir.join("usr/bin");
    fs::create_dir_all(&bindir)?;
    for binary in ["bzip2", "bzip2recover"] {
        fs::copy(source_copy.join(binary), bindir.join(binary))?;
        set_mode(bindir.join(binary), 0o755)?;
    }
    std::os::unix::fs::symlink("bzip2", bindir.join("bunzip2"))?;
    std::os::unix::fs::symlink("bzip2", bindir.join("bzcat"))?;
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
    for binary in ["xz", "unxz", "xzcat"] {
        if !install_dir.join("usr/bin").join(binary).exists() {
            bail!("XZ Utils install did not produce usr/bin/{binary}");
        }
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
        "-DZSTD_BUILD_PROGRAMS=ON",
        "-DZSTD_BUILD_TESTS=OFF",
        // Upstream's CLI links its static library by design; the MattOS
        // runtime package still publishes only the shared SONAME.
        "-DZSTD_BUILD_STATIC=ON",
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
    if !install_dir.join("usr/bin/zstd").is_file() {
        bail!("Zstandard install did not produce usr/bin/zstd");
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_gpg_autotools_library(
    repo_root: &Path,
    component: &str,
    dependency_components: &[&str],
    expected_soname: &str,
) -> Result<()> {
    let source = repo_root.join("src/system/security").join(component);
    let out_root = repo_root.join("out/build").join(component);
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(
        repo_root
            .join("upstream/state")
            .join(format!("{component}.toml")),
    )
    .with_context(|| format!("failed to read {component} upstream state"))?;

    let mut include_dirs = Vec::new();
    let mut library_dirs = Vec::new();
    let mut pkgconfig_dirs = Vec::new();
    for dependency in dependency_components {
        let usr = repo_root
            .join("out/build")
            .join(dependency)
            .join("install/usr");
        include_dirs.push(usr.join("include"));
        library_dirs.push(usr.join("lib/x86_64-linux-gnu"));
        pkgconfig_dirs.push(usr.join("lib/x86_64-linux-gnu/pkgconfig"));
    }
    let cppflags = include_dirs
        .iter()
        .map(|path| format!("-I{}", path.display()))
        .collect::<Vec<_>>()
        .join(" ");
    let ldflags = library_dirs
        .iter()
        .map(|path| format!("-L{}", path.display()))
        .collect::<Vec<_>>()
        .join(" ");
    let library_path = std::env::join_paths(&library_dirs)?
        .to_string_lossy()
        .to_string();
    let pkgconfig_path = std::env::join_paths(&pkgconfig_dirs)?
        .to_string_lossy()
        .to_string();
    let mut tool_path = dependency_components
        .iter()
        .map(|dependency| {
            repo_root
                .join("out/build")
                .join(dependency)
                .join("install/usr/bin")
        })
        .collect::<Vec<_>>();
    tool_path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let tool_path = std::env::join_paths(tool_path)?
        .to_string_lossy()
        .to_string();
    let env_overrides = [
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("CPPFLAGS", cppflags),
        ("LDFLAGS", ldflags),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        ("PKG_CONFIG_PATH", pkgconfig_path),
        ("PATH", tool_path),
    ];
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--disable-doc",
        "--disable-tests",
        "--disable-nls",
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
        remove_path_if_exists(&install_dir)?;
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
    let soname = install_dir
        .join("usr/lib/x86_64-linux-gnu")
        .join(expected_soname);
    if !soname.exists() {
        bail!("{component} install did not produce {}", soname.display());
    }
    remove_path_if_exists(&install_dir.join(format!("usr/lib/x86_64-linux-gnu/{component}.la")))?;
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_gpgv(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/security/gnupg");
    let out_root = repo_root.join("out/build/gpgv");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let dependencies = [
        "libgpg-error",
        "libgcrypt",
        "libassuan",
        "libksba",
        "npth",
        "zlib",
    ];
    let mut include_dirs = Vec::new();
    let mut library_dirs = Vec::new();
    let mut pkgconfig_dirs = Vec::new();
    for dependency in dependencies {
        let usr = repo_root
            .join("out/build")
            .join(dependency)
            .join("install/usr");
        include_dirs.push(usr.join("include"));
        library_dirs.push(usr.join("lib/x86_64-linux-gnu"));
        pkgconfig_dirs.push(usr.join("lib/x86_64-linux-gnu/pkgconfig"));
    }
    let library_path = std::env::join_paths(&library_dirs)?
        .to_string_lossy()
        .to_string();
    let mut tool_path = dependencies
        .iter()
        .map(|dependency| {
            repo_root
                .join("out/build")
                .join(dependency)
                .join("install/usr/bin")
        })
        .collect::<Vec<_>>();
    tool_path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let tool_path = std::env::join_paths(tool_path)?
        .to_string_lossy()
        .to_string();
    let env_overrides = [
        (
            "CPPFLAGS",
            include_dirs
                .iter()
                .map(|path| format!("-I{}", path.display()))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            "LDFLAGS",
            library_dirs
                .iter()
                .map(|path| format!("-L{}", path.display()))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths(&pkgconfig_dirs)?
                .to_string_lossy()
                .to_string(),
        ),
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("PATH", tool_path),
    ];
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-doc",
        "--disable-tests",
        "--disable-nls",
        "--disable-ldap",
        "--disable-card-support",
        "--disable-ntbtls",
        "--disable-gnutls",
        "--disable-sqlite",
        "--disable-bzip2",
    ];
    let state = fs::read_to_string(repo_root.join("upstream/state/gnupg.toml"))
        .context("failed to read GnuPG upstream state")?;
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
        remove_path_if_exists(&install_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "autoreconf", &["-fi"])?;
    }
    let common_dir = source_copy.join("common");
    run_cmd(
        &common_dir,
        "sh",
        &[
            "-c",
            "awk -f exaudit.awk audit.h | awk -f mkstrtable.awk -v textidx=3 -v nogettext=1 -v pkg_namespace=eventstr_ > audit-events.h && awk -f exstatus.awk status.h | awk -f mkstrtable.awk -v textidx=3 -v nogettext=1 -v pkg_namespace=statusstr_ > status-codes.h",
        ],
    )?;
    run_cmd(
        &source_copy.join("regexp"),
        "sh",
        &[
            "-c",
            "awk -f parse-unidata.awk UnicodeData.txt > _unicode_mapping.c",
        ],
    )?;
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
            &env_overrides,
        )?;
    }
    let build_common_dir = build_dir.join("common");
    fs::create_dir_all(&build_common_dir)?;
    for generated in ["audit-events.h", "status-codes.h"] {
        fs::copy(
            source_copy.join("common").join(generated),
            build_common_dir.join(generated),
        )?;
    }
    fs::create_dir_all(build_dir.join("regexp"))?;
    fs::copy(
        source_copy.join("regexp/_unicode_mapping.c"),
        build_dir.join("regexp/_unicode_mapping.c"),
    )?;
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env_overrides)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env_overrides,
    )?;
    if !install_dir.join("usr/bin/gpgv").is_file() {
        bail!("GnuPG install did not produce usr/bin/gpgv");
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
    copy_imported_working_tree(
        repo_root,
        Path::new("src/system/libraries/pcre2"),
        &source_copy,
    )?;
    copy_imported_working_tree(
        repo_root,
        Path::new("src/build-support/sljit"),
        &source_copy.join("deps/sljit"),
    )?;
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
    apply_component_patches(repo_root, "libxcrypt", &source_copy)?;
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

fn build_libndp(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/network/libndp");
    let out_root = repo_root.join("out/build/libndp");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/libndp.toml"))?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--disable-nls",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
        remove_path_if_exists(&install_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "autoreconf", &["-fi"])?;
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
    if !install_dir
        .join("usr/lib/x86_64-linux-gnu/libndp.so.0")
        .exists()
        || !install_dir
            .join("usr/lib/x86_64-linux-gnu/pkgconfig/libndp.pc")
            .exists()
    {
        bail!("libndp install did not produce its runtime library and pkg-config metadata");
    }
    remove_path_if_exists(&install_dir.join("usr/lib/x86_64-linux-gnu/libndp.la"))?;
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_readline(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "readline",
        "src/system/userland/readline",
        &["ncurses"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
            "--with-curses",
        ],
        &[
            "usr/lib/x86_64-linux-gnu/libreadline.so.8",
            "usr/lib/x86_64-linux-gnu/pkgconfig/readline.pc",
        ],
    )?;
    let pc =
        repo_root.join("out/build/readline/install/usr/lib/x86_64-linux-gnu/pkgconfig/readline.pc");
    let body = fs::read_to_string(&pc)?
        .lines()
        .map(|line| {
            if line.starts_with("Libs:") && !line.contains("-lncursesw") {
                format!("{line} -lncursesw")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(pc, body)?;
    Ok(())
}

fn build_tar(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/tar");
    let paxutils = repo_root.join("src/build-support/paxutils");
    let gnulib = repo_root.join("src/build-support/gnulib");
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
    if !gnulib.join("gnulib-tool").is_file() {
        bail!(
            "pinned Gnulib build support not found in {}; run upstream import gnulib first",
            gnulib.display()
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
    let gnulib_state = fs::read_to_string(repo_root.join("upstream/state/gnulib.toml"))
        .context("failed to read Gnulib upstream state")?;
    let acl_state = fs::read_to_string(repo_root.join("upstream/state/acl.toml"))
        .context("failed to read ACL upstream state")?;
    let options = [
        "--prefix=/usr",
        "--disable-nls",
        "--without-selinux",
        "--with-posix-acls",
    ];
    let stamp = format!(
        "{state}\n{paxutils_state}\n{gnulib_state}\n{acl_state}\n{}\n",
        options.join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    copy_imported_working_tree(repo_root, Path::new("src/userland/tar"), &source_copy)?;
    copy_imported_working_tree(
        repo_root,
        Path::new("src/build-support/paxutils"),
        &source_copy.join("paxutils"),
    )?;
    copy_imported_working_tree(
        repo_root,
        Path::new("src/build-support/gnulib"),
        &source_copy.join("gnulib"),
    )?;
    apply_component_patches(repo_root, "tar", &source_copy)?;
    if !source_copy.join("configure").is_file() {
        let gnulib_arg = format!("--gnulib-srcdir={}", source_copy.join("gnulib").display());
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
                &gnulib_arg,
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
    } else {
        // Meson persists version-sensitive state in build.dat.  The normal
        // MattOS cache intentionally permits reuse of completed artifacts
        // across a host Meson update, but a dependency-output miss may still
        // need to enter this disposable build directory.  Reconfigure first
        // so a newer Meson can safely compile and install instead of failing
        // late in `meson install` while reading an older build.dat.
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
    // This is a build-private libtool convenience archive. Leaving it in
    // the staged install lets downstream libtool consumers embed this
    // checkout's absolute staging path as an ELF RUNPATH. The libcurl .so
    // and pkg-config metadata are the target-facing interface.
    remove_path_if_exists(&install_dir.join("usr/lib/x86_64-linux-gnu/libcurl.la"))?;
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
        repo_root.join("out/build/dbus/install/usr"),
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
    rewrite_staged_pkgconfig_files(&install_dir)?;

    patch_systemd_osc_profile_for_posix_login_shell(&install_dir)?;

    let pid1 = install_dir.join("usr/lib/systemd/systemd");
    if !pid1.exists() {
        bail!("systemd install did not produce {}", pid1.display());
    }

    Ok(())
}

fn patch_systemd_osc_profile_for_posix_login_shell(install_dir: &Path) -> Result<()> {
    let path = install_dir.join("etc/profile.d/80-systemd-osc-context.sh");
    let body =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let bash_guard = "# Not bash?\n[ -n \"${BASH_VERSION:-}\" ] || return 0";
    let guarded = "# MattOS can inherit BASH_VERSION into a POSIX login shell. Verify the\n# required Bash builtin itself before parsing the interactive prompt setup.\ncommand -v shopt >/dev/null 2>&1 || return 0";
    let upstream = "    [ -n \"$(declare -p PROMPT_COMMAND 2>/dev/null)\" ] || PROMPT_COMMAND+=('')\n\n    # Whenever a new prompt is shown, close the previous command, and prepare new command\n    PROMPT_COMMAND+=(__systemd_osc_context_precmdline)";
    let replacement = "    # MattOS login commands are launched by a POSIX shell. Array assignment\n    # syntax is rejected while parsing even when this Bash-only branch is not\n    # executed, so preserve the hook with a scalar PROMPT_COMMAND instead.\n    if [ -n \"${PROMPT_COMMAND:-}\" ]; then\n        PROMPT_COMMAND=\"__systemd_osc_context_precmdline;${PROMPT_COMMAND}\"\n    else\n        PROMPT_COMMAND=__systemd_osc_context_precmdline\n    fi";
    if !body.contains(bash_guard) || !body.contains(upstream) {
        bail!("systemd OSC profile no longer matches the reviewed POSIX-shell compatibility patch");
    }
    let body = body.replacen(bash_guard, guarded, 1);
    fs::write(&path, body.replacen(upstream, replacement, 1))
        .with_context(|| format!("failed to patch {}", path.display()))
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
        // mattos-compat uses the target-owned nspawn binary to run isolated
        // distro userlands; keep it in the systemd stage and package it from
        // that output rather than relying on the host implementation.
        "-Dnspawn=enabled".to_string(),
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
        // COSMIC Initial Setup uses the standard org.freedesktop.locale1 API
        // to read and apply the selected system locale.
        "-Dlocaled=true".to_string(),
        "-Dtimedated=true".to_string(),
        "-Dnsresourced=false".to_string(),
        "-Ddefault-network=false".to_string(),
        "-Ddbus=enabled".to_string(),
        // The target dbus-1.pc is queried under PKG_CONFIG_SYSROOT_DIR while
        // configuring systemd.  Do not let its absolute host/sysroot paths
        // become Meson install destinations; these are target filesystem
        // paths in the finished systemd package.
        "-Ddbussessionservicedir=/usr/share/dbus-1/services".to_string(),
        "-Ddbussystemservicedir=/usr/share/dbus-1/system-services".to_string(),
        "-Ddbus-interfaces-dir=/usr/share/dbus-1/interfaces".to_string(),
        "-Ddbuspolicydir=/usr/share/dbus-1/system.d".to_string(),
        "-Dglib=disabled".to_string(),
        "-Dseccomp=disabled".to_string(),
        "-Dselinux=enabled".to_string(),
        "-Dacl=disabled".to_string(),
        "-Daudit=disabled".to_string(),
        // udev must probe filesystem and GPT metadata so the stable
        // /dev/disk/by-{uuid,partuuid} names used by installed fstab entries
        // exist during coldplug.
        "-Dblkid=enabled".to_string(),
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

fn build_dbus(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "dbus",
        "src/system/dbus/dbus",
        &["expat"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Dmessage_bus=true",
            "-Dtools=true",
            "-Dinstalled_tests=false",
            "-Dintrusive_tests=false",
            "-Dmodular_tests=disabled",
            "-Ddoxygen_docs=disabled",
            "-Dducktype_docs=disabled",
            "-Dqt_help=disabled",
            "-Dapparmor=disabled",
            "-Dselinux=disabled",
            "-Dlibaudit=disabled",
            "-Dsystemd=disabled",
        ],
        "usr/bin/dbus-run-session",
        &[],
    )?;
    let dbus_usr = repo_root.join("out/build/dbus/install/usr");
    rewrite_pkgconfig_prefixes(&dbus_usr.join("lib/x86_64-linux-gnu/pkgconfig"), &dbus_usr)?;
    for required in [
        "usr/lib/x86_64-linux-gnu/libdbus-1.so.3",
        "usr/bin/dbus-daemon",
        "usr/bin/dbus-run-session",
    ] {
        if !repo_root
            .join("out/build/dbus/install")
            .join(required)
            .is_file()
        {
            bail!("D-Bus build did not install /{required}");
        }
    }
    Ok(())
}

fn build_dav1d(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "dav1d",
        "src/system/multimedia/dav1d",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Denable_asm=false",
            "-Denable_tools=false",
            "-Denable_examples=false",
            "-Denable_tests=false",
            "-Denable_docs=false",
        ],
        "usr/lib/x86_64-linux-gnu/libdav1d.so.7",
        &[],
    )
}

fn build_glib(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "glib",
        "src/system/libraries/glib",
        &["libffi", "pcre2", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Dtests=false",
            "-Dinstalled_tests=false",
            "-Dnls=disabled",
            "-Dselinux=disabled",
            "-Dlibmount=disabled",
            "-Dlibelf=disabled",
            "-Dintrospection=disabled",
            "-Dman-pages=disabled",
            "-Ddtrace=disabled",
            "-Dsystemtap=disabled",
            "-Dsysprof=disabled",
            "-Dglib_debug=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libglib-2.0.so.0",
        &[],
    )?;
    let glib_usr = repo_root.join("out/build/glib/install/usr");
    let glib_pc = glib_usr.join("lib/x86_64-linux-gnu/pkgconfig");
    rewrite_pkgconfig_prefixes(&glib_pc, &glib_usr)?;
    // GLib's public .pc files expose these private requirements even for a
    // dynamic consumer. Keep their development metadata in the same
    // output-owned SDK directory so pkg-config cannot fall back to the host.
    for (component, names) in [
        ("pcre2", &["libpcre2-8.pc"][..]),
        ("libffi", &["libffi.pc"][..]),
    ] {
        let dependency_usr = repo_root
            .join("out/build")
            .join(component)
            .join("install/usr");
        let dependency_pc = dependency_usr.join("lib/x86_64-linux-gnu/pkgconfig");
        for name in names {
            fs::copy(dependency_pc.join(name), glib_pc.join(name))?;
        }
        rewrite_selected_pkgconfig_prefixes(&glib_pc, names, &dependency_usr)?;
    }
    for required in [
        "usr/lib/x86_64-linux-gnu/libgobject-2.0.so.0",
        "usr/lib/x86_64-linux-gnu/libgio-2.0.so.0",
        "usr/bin/glib-compile-schemas",
    ] {
        if !repo_root
            .join("out/build/glib/install")
            .join(required)
            .is_file()
        {
            bail!("GLib build did not install /{required}");
        }
    }
    Ok(())
}

fn rewrite_pkgconfig_prefixes(directory: &Path, physical_usr: &Path) -> Result<()> {
    let names = fs::read_dir(directory)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension() == Some(OsStr::new("pc")))
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let name_refs = names.iter().map(String::as_str).collect::<Vec<_>>();
    rewrite_selected_pkgconfig_prefixes(directory, &name_refs, physical_usr)
}

fn rewrite_selected_pkgconfig_prefixes(
    directory: &Path,
    names: &[&str],
    physical_usr: &Path,
) -> Result<()> {
    for name in names {
        let path = directory.join(name);
        let body = fs::read_to_string(&path)?;
        let expected_prefix = format!("prefix={}", physical_usr.display());
        let rewritten = if body.lines().any(|line| line == expected_prefix) {
            // build_meson_runtime has already made this descriptor point at
            // its output-owned /usr tree.  Reusing that output is valid and
            // must not be mistaken for a missing relocatable prefix.
            body
        } else if body.lines().any(|line| line == "prefix=/usr") {
            body.replacen("prefix=/usr", &expected_prefix, 1)
        } else {
            bail!(
                "pkg-config metadata {} has no relocatable /usr prefix",
                path.display()
            )
        };
        fs::write(path, rewritten)?;
    }
    Ok(())
}

fn build_pipewire(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "pipewire",
        "src/system/multimedia/pipewire",
        &["systemd", "dbus"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Ddocs=disabled",
            "-Dman=disabled",
            "-Dexamples=disabled",
            "-Dtests=disabled",
            "-Dinstalled_tests=disabled",
            "-Dgstreamer=disabled",
            "-Dsystemd=enabled",
            "-Dlogind=enabled",
            "-Dsystemd-system-service=disabled",
            "-Dsystemd-user-service=enabled",
            "-Dselinux=disabled",
            "-Dpipewire-alsa=disabled",
            "-Dpipewire-jack=disabled",
            "-Dpipewire-v4l2=disabled",
            "-Dalsa=disabled",
            "-Dbluez5=disabled",
            "-Dffmpeg=disabled",
            "-Djack=disabled",
            "-Dv4l2=disabled",
            "-Dlibcamera=disabled",
            "-Dvulkan=disabled",
            "-Dsdl2=disabled",
            "-Dsndfile=disabled",
            "-Dlibmysofa=disabled",
            "-Dlibpulse=disabled",
            "-Davahi=disabled",
            "-Dlibusb=disabled",
            "-Dsession-managers=[]",
            "-Dx11=disabled",
            "-Dx11-xfixes=disabled",
            "-Dlibcanberra=disabled",
            "-Dlegacy-rtkit=false",
            "-Dflatpak=disabled",
            "-Dreadline=disabled",
            "-Dgsettings=disabled",
            "-Dgsettings-pulse-schema=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libpipewire-0.3.so.0",
        &[],
    )?;
    let pipewire_usr = repo_root.join("out/build/pipewire/install/usr");
    rewrite_pkgconfig_prefixes(
        &pipewire_usr.join("lib/x86_64-linux-gnu/pkgconfig"),
        &pipewire_usr,
    )?;
    for required in [
        "usr/bin/pipewire",
        "usr/bin/pipewire-pulse",
        "usr/lib/systemd/user/pipewire.service",
        "usr/lib/systemd/user/pipewire.socket",
    ] {
        if !repo_root
            .join("out/build/pipewire/install")
            .join(required)
            .exists()
        {
            bail!("PipeWire build did not install /{required}");
        }
    }
    Ok(())
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
    let dependency_outputs = ["expat", "systemd"]
        .iter()
        .map(|dependency| {
            let manifest = stage_cache::read_stage_manifest(repo_root, dependency)
                .with_context(|| format!("failed to read {dependency} dependency manifest"))?;
            Ok::<_, anyhow::Error>(format!("{dependency}={}", manifest.output_content_digest))
        })
        .collect::<Result<Vec<_>>>()?;
    let stamp = format!(
        "{state}\n{expat_state}\n{}\n{}\ndependency-outputs={}\n",
        options.join("\n"),
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n"),
        dependency_outputs.join(",")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }

    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    copy_imported_working_tree(
        repo_root,
        Path::new("src/system/dbus/dbus-broker"),
        &source_copy,
    )?;
    apply_component_patches(repo_root, "dbus-broker", &source_copy)?;
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

include!("stages/image.rs");
#[cfg(test)]
mod tests {
    use super::*;

    fn write_pkgconfig_overlay_fixture_manifest(root: &Path, stage: &str, digest: &str) {
        let manifest = cache_manifest::StageManifest {
            schema_version: cache_manifest::STAGE_MANIFEST_SCHEMA_VERSION,
            stage: stage.to_string(),
            inputs: cache_manifest::StageInputs {
                source_digest: "fixture".to_string(),
                configuration_digest: "fixture".to_string(),
                tool_digest: "fixture".to_string(),
                build_provenance_digest: "fixture".to_string(),
                environment_digest: "fixture".to_string(),
                dependency_digests: BTreeMap::new(),
                full_digest: format!("fixture-{stage}"),
            },
            input_details: cache_manifest::StageInputDetails::default(),
            expected_outputs: Vec::new(),
            output_content_digest: digest.to_string(),
        };
        stage_cache::write_stage_manifest(root, &manifest).unwrap();
    }

    #[test]
    fn pkgconfig_consumer_overlay_is_repeatable_and_never_rewrites_published_producers() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let autotools = root.join("out/build/autotools-fixture/install/usr/lib/x86_64-linux-gnu/pkgconfig");
        let meson = root.join("out/build/meson-fixture/install/usr/lib/x86_64-linux-gnu/pkgconfig");
        fs::create_dir_all(&autotools).unwrap();
        fs::create_dir_all(&meson).unwrap();
        let autotools_pc = autotools.join("autotools-fixture.pc");
        let meson_pc = meson.join("meson-fixture.pc");
        fs::write(&autotools_pc, "prefix=/usr\nlibdir=/usr/lib/x86_64-linux-gnu\n").unwrap();
        fs::write(&meson_pc, "prefix=/usr\nincludedir=/usr/include\n").unwrap();
        write_pkgconfig_overlay_fixture_manifest(root, "autotools-fixture", "autotools-output");
        write_pkgconfig_overlay_fixture_manifest(root, "meson-fixture", "meson-output");

        let sources = vec![
            ("autotools-fixture".to_string(), "lib".to_string(), autotools.clone()),
            ("meson-fixture".to_string(), "lib".to_string(), meson.clone()),
        ];
        let before_autotools = fs::read(&autotools_pc).unwrap();
        let before_meson = fs::read(&meson_pc).unwrap();
        let first = staged_pkgconfig_overlay(root, &sources).unwrap();
        let second = staged_pkgconfig_overlay(root, &sources).unwrap();

        assert_eq!(first, second, "identical consumers reuse one immutable overlay");
        assert_eq!(fs::read(&autotools_pc).unwrap(), before_autotools);
        assert_eq!(fs::read(&meson_pc).unwrap(), before_meson);
        assert_eq!(
            fs::read_to_string(first[0].join("autotools-fixture.pc")).unwrap(),
            format!(
                "prefix={}\nlibdir=${{prefix}}/lib/x86_64-linux-gnu\n",
                root.join("out/build/autotools-fixture/install/usr").display()
            )
        );
        assert_eq!(
            fs::read_to_string(first[1].join("meson-fixture.pc")).unwrap(),
            format!(
                "prefix={}\nincludedir=${{prefix}}/include\n",
                root.join("out/build/meson-fixture/install/usr").display()
            )
        );
    }

    #[test]
    fn staged_pkgconfig_rewrite_is_idempotent() {
        let prefix = Path::new("/tmp/mattos-stage/usr");
        let first = rewrite_pkgconfig_for_staged_consumer(
            "prefix=/usr\nlibdir=/usr/lib/x86_64-linux-gnu\nincludedir=/usr/include\n",
            prefix,
        );
        assert_eq!(rewrite_pkgconfig_for_staged_consumer(&first, prefix), first);
    }

    #[test]
    fn source_mirror_sync_excludes_and_deletes_derived_cargo_outputs() {
        assert!(SOURCE_MIRROR_RSYNC_FLAGS.contains(&"--delete"));
        assert!(SOURCE_MIRROR_RSYNC_FLAGS.contains(&"--delete-excluded"));
        assert!(SOURCE_MIRROR_RSYNC_FLAGS.contains(&"--exclude=target/"));
        assert!(SOURCE_MIRROR_RSYNC_FLAGS.contains(&"--exclude=__pycache__/"));
        assert!(SOURCE_MIRROR_RSYNC_FLAGS.contains(&"--exclude=*.pyc"));
    }

    #[test]
    fn nvidia_selector_routes_turing_to_official_and_pascal_to_nouveau() {
        let ids = BTreeSet::from([0x1e04, 0x2684]);
        let (config, selector) = render_nvidia_driver_selection(&ids);
        assert!(config.contains("install nvidia "));
        assert!(config.contains("install nouveau "));
        assert!(!config.contains("blacklist"));
        assert!(selector.contains("0x1e04|0x2684"));
        assert!(!selector.contains("0x1b80"));
        assert!(selector.contains("nouveau) [ \"$supported\" -eq 0 ]"));
        assert!(selector.contains("nvidia*) [ \"$supported\" -eq 1 ]"));
    }

    #[test]
    fn child_parallelism_is_capped_to_scheduler_grant() {
        scheduler::set_child_jobs_for_test(4, scheduler::ChildJobPolicy::SchedulerGrant);
        assert_eq!(
            scheduler_command_args(&["-C", "src", "-j", "4", "all"]),
            ["-C", "src", "-j", "4", "all"]
        );
        assert_eq!(scheduler_command_args(&["-j", "2"]), ["-j", "2"]);
        assert_eq!(
            scheduler_command_args(&["--build", "build", "--parallel", "8"]),
            ["--build", "build", "--parallel", "4"]
        );
        assert_eq!(scheduler_command_args(&["--jobs=8"]), ["--jobs=4"]);

        scheduler::set_child_jobs_for_test(4, scheduler::ChildJobPolicy::Capped(2));
        assert_eq!(scheduler::child_job_limit(), 2);
        assert_eq!(
            scheduler_command_args(&["--build", "build", "--parallel", "4"]),
            ["--build", "build", "--parallel", "2"]
        );
        assert_eq!(scheduler_command_args(&["-j4"]), ["-j2"]);

        scheduler::set_child_jobs_for_test(4, scheduler::ChildJobPolicy::SchedulerGrant);
    }

    #[test]
    fn effective_command_telemetry_reports_normalized_argv_and_child_limit() {
        scheduler::set_child_jobs_for_test(4, scheduler::ChildJobPolicy::Capped(2));
        let args = scheduler_command_args(&["-C", "src", "-j4", "all"]);
        assert_eq!(
            effective_command_display("make", &args),
            "make -C src -j2 all\n[mattos-command] child_jobs=2 argv=[\"make\", \"-C\", \"src\", \"-j2\", \"all\"]"
        );
        scheduler::set_child_jobs_for_test(4, scheduler::ChildJobPolicy::SchedulerGrant);
    }

    #[test]
    fn experimental_child_jobs_are_restricted_to_direct_candidate_builds() {
        let budget = resources::ResourceBudget {
            cpu_tokens: 12,
            build_memory_bytes: 12 * 1024 * 1024 * 1024,
            reserved_memory_bytes: 2 * 1024 * 1024 * 1024,
            available_memory_bytes: 66 * 1024 * 1024 * 1024,
        };
        assert!(
            validate_experimental_child_jobs_with_budget(BuildStage::All, Some(8), budget).is_err()
        );
        assert!(
            validate_experimental_child_jobs_with_budget(BuildStage::Libcap, Some(8), budget)
                .is_err()
        );
        assert!(
            validate_experimental_child_jobs_with_budget(BuildStage::Apt, Some(1), budget).is_err()
        );
        assert!(
            validate_experimental_child_jobs_with_budget(BuildStage::Glibc, Some(13), budget)
                .is_err()
        );
        assert!(
            validate_experimental_child_jobs_with_budget(BuildStage::Glibc, Some(8), budget)
                .is_ok()
        );
    }

    #[test]
    fn experimental_child_jobs_raise_explicit_recipe_limits_only_when_enabled() {
        scheduler::set_child_jobs_for_test(8, scheduler::ChildJobPolicy::SchedulerGrant);
        EXPERIMENTAL_CHILD_JOBS.with(|current| current.set(Some(8)));
        assert_eq!(scheduler_command_args(&["-j", "4"]), ["-j", "8"]);
        assert_eq!(scheduler_command_args(&["-j4"]), ["-j8"]);
        assert_eq!(scheduler_command_args(&["--parallel=4"]), ["--parallel=8"]);

        EXPERIMENTAL_CHILD_JOBS.with(|current| current.set(None));
        scheduler::set_child_jobs_for_test(4, scheduler::ChildJobPolicy::SchedulerGrant);
        assert_eq!(scheduler_command_args(&["-j", "2"]), ["-j", "2"]);
    }

    #[test]
    fn serial_child_policy_prevents_missing_dependency_race() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("Makefile"),
            "all: generated consumer\n\ngenerated:\n\t@sleep 0.2\n\t@printf '#define READY 1\\n' > generated.h\n\nconsumer:\n\t@test -f generated.h\n",
        )
        .unwrap();
        let parallel = Command::new("make")
            .args(["-j", "2"])
            .env_remove("MAKEFLAGS")
            .current_dir(temp.path())
            .status()
            .unwrap();
        assert!(
            !parallel.success(),
            "fixture must reproduce the dependency race"
        );

        scheduler::set_child_jobs_for_test(4, scheduler::ChildJobPolicy::Serial);
        run_cmd(temp.path(), "make", &["all"]).unwrap();
        assert!(temp.path().join("generated.h").is_file());
        scheduler::set_child_jobs_for_test(4, scheduler::ChildJobPolicy::SchedulerGrant);
    }

    #[test]
    fn child_job_policy_controls_all_build_system_environments() {
        scheduler::set_child_jobs_for_test(4, scheduler::ChildJobPolicy::Capped(2));
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "printf '%s\n' \"$MAKEFLAGS\" \"$CARGO_BUILD_JOBS\" \"$CMAKE_BUILD_PARALLEL_LEVEL\" \"$MESON_NUM_PROCESSES\" \"$NINJAFLAGS\"",
        ]);
        apply_scheduler_parallelism(&mut command);
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "-j2\n2\n2\n2\n-j2\n"
        );
        scheduler::set_child_jobs_for_test(4, scheduler::ChildJobPolicy::SchedulerGrant);
    }

    #[test]
    fn gcc_make_uses_the_authoritative_scheduler_parallelism() {
        for jobs in [4, 6] {
            scheduler::set_child_jobs_for_test(jobs, scheduler::ChildJobPolicy::SchedulerGrant);
            let mut command = Command::new("sh");
            command.args(["-c", "printf '%s' \"$MAKEFLAGS\""]);
            apply_scheduler_parallelism(&mut command);
            let output = command.output().unwrap();
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                format!("-j{jobs}")
            );
            assert_eq!(scheduler_command_args(&["all-gcc"]), ["all-gcc"]);
        assert_eq!(
            scheduler_command_args(&["all-target-libgcc"]),
            ["all-target-libgcc"]
        );
    }
        let source = include_str!("stages/toolchain.rs");
        let prerequisite = source
            .split_once("fn build_static_prerequisite(")
            .unwrap()
            .1
            .split_once("fn log_gcc_info_index_boundary(")
            .unwrap()
            .0;
        assert!(
            !prerequisite.contains("\"-j\""),
            "GCC prerequisite builds must not reintroduce recipe-local job flags"
        );
        scheduler::set_child_jobs_for_test(4, scheduler::ChildJobPolicy::SchedulerGrant);
    }

    #[test]
    fn flatpak_build_forces_owned_sandbox_helpers_without_wrap_fallbacks() {
        let source = include_str!("main.rs");
        let flatpak = source
            .split_once("fn build_flatpak(repo_root: &Path) -> Result<()>")
            .expect("build_flatpak implementation")
            .1
            .split_once("fn build_libarchive")
            .expect("build_flatpak boundary")
            .0;
        for required in [
            "\"bubblewrap\"",
            "\"xdg-dbus-proxy\"",
            "-Dsystem_bubblewrap=/usr/bin/bwrap",
            "-Dsystem_dbus_proxy=/usr/bin/xdg-dbus-proxy",
            "--wrap-mode=nofallback",
        ] {
            assert!(flatpak.contains(required), "Flatpak build omits {required}");
        }
    }

    #[test]
    fn flatpak_stage_has_no_built_in_firefox_payload() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let source = include_str!("main.rs");
        let flatpak = source
            .split_once("fn build_flatpak(repo_root: &Path) -> Result<()>")
            .expect("build_flatpak implementation")
            .1
            .split_once("fn build_libarchive")
            .expect("build_flatpak boundary")
            .0;
        for forbidden in [
            "org.mozilla.firefox",
            "provision_firefox_flatpak",
            "firefox-provenance.toml",
            "resolved_app_commit",
            "resolved_runtime_commit",
            "create-usb",
        ] {
            assert!(!flatpak.contains(forbidden), "Flatpak still contains Firefox integration: {forbidden}");
        }
        for unit in [
            "mattos-flatpak-system-update.service",
            "mattos-flatpak-system-update.timer",
            "mattos-flatpak-user-update.service",
            "mattos-flatpak-user-update.timer",
        ] {
            let body = fs::read_to_string(
                root.join("src/system/packages/flatpak/resources").join(unit),
            )
            .unwrap();
            if unit.ends_with(".timer") {
                assert!(body.contains("Persistent=true"));
                assert!(body.contains("RandomizedDelaySec="));
            } else {
                assert!(body.contains("flatpak"), "update unit {unit} is not Flatpak-backed");
                assert!(body.contains("--noninteractive"));
                assert!(body.contains("--assumeyes"));
            }
        }
    }

    #[test]
    fn flatpak_stage_publishes_the_target_rooted_installer_helper() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let helper = std::fs::read_to_string(
            root.join("src/system/installer/flatpak-target-install.c"),
        )
        .unwrap();
        for required in [
            "flatpak_installation_new_for_path",
            "flatpak_transaction_new_for_installation",
            "flatpak_transaction_add_install",
            "flatpak_transaction_run",
            "var", "lib", "flatpak",
        ] {
            assert!(helper.contains(required), "target-install helper omits {required}");
        }
        for forbidden in ["/usr/bin/chroot", "resolv.conf", "FLATPAK_SYSTEM_DIR"] {
            assert!(
                !helper.contains(forbidden),
                "target-install helper must not depend on {forbidden}"
            );
        }
        assert!(
            build_stage_spec(BuildStage::Flatpak)
                .outputs
                .iter()
                .any(|output| output.ends_with("usr/libexec/mattos-flatpak-target-install")),
            "Flatpak stage must publish the helper in its verified output contract"
        );
    }

    #[test]
    fn installed_apt_metadata_refresh_is_timer_driven_and_never_upgrades() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let resources = root.join("src/system/packages/apt/resources");
        let service = std::fs::read_to_string(resources.join("mattos-apt-daily.service")).unwrap();
        let timer = std::fs::read_to_string(resources.join("mattos-apt-daily.timer")).unwrap();
        assert!(service.contains("ExecStart=/usr/bin/apt-get update"));
        for forbidden in ["upgrade", "dist-upgrade", " install"] {
            assert!(!service.contains(forbidden), "APT metadata service contains {forbidden}");
        }
        assert!(service.contains("Wants=network-online.target"));
        assert!(service.contains("After=network-online.target"));
        for required in [
            "OnBootSec=5min",
            "OnUnitActiveSec=1d",
            "Persistent=true",
            "RandomizedDelaySec=30min",
            "WantedBy=timers.target",
        ] {
            assert!(timer.contains(required), "APT metadata timer omits {required}");
        }
    }

    #[test]
    fn memory_intensive_toolchain_and_graphics_stages_are_capped() {
        for stage in build_plan(BuildStage::All) {
            let expected = if stage == BuildStage::Libcap {
                scheduler::ChildJobPolicy::Serial
            } else if matches!(
                stage,
                BuildStage::Llvm
                    | BuildStage::Mesa
                    | BuildStage::CosmicComp
                    | BuildStage::CosmicSession
                    | BuildStage::CosmicGreeter
                    | BuildStage::CosmicPanel
                    | BuildStage::CosmicApplets
                    | BuildStage::CosmicAppLibrary
                    | BuildStage::CosmicLauncher
                    | BuildStage::CosmicSettings
                    | BuildStage::CosmicSettingsDaemon
                    | BuildStage::CosmicNotifications
                    | BuildStage::CosmicOsd
                    | BuildStage::CosmicBg
                    | BuildStage::CosmicWorkspaces
                    | BuildStage::CosmicFiles
                    | BuildStage::CosmicTerm
                    | BuildStage::CosmicTweaks
                    | BuildStage::CosmicUtilities
                    | BuildStage::CosmicRandr
                    | BuildStage::CosmicScreenshot
                    | BuildStage::PopLauncher
                    | BuildStage::CosmicCalculator
                    | BuildStage::CosmicStorage
                    | BuildStage::CosmicMonitor
                    | BuildStage::CosmicStore
                    | BuildStage::Flatpak
                    | BuildStage::CosmicPortal
                    | BuildStage::CosmicInitialSetup
                    | BuildStage::CosmicEdit
                    | BuildStage::Greetd
            ) {
                scheduler::ChildJobPolicy::Capped(4)
            } else {
                scheduler::ChildJobPolicy::SchedulerGrant
            };
            assert_eq!(scheduler_child_job_policy(stage), expected);
        }
    }

    #[test]
    fn production_scheduler_plan_is_valid_and_simulates_successful_cold_run() {
        let stages = build_plan(BuildStage::All);
        let nodes = scheduled_build_nodes(&stages);
        let budget = resources::ResourceBudget {
            cpu_tokens: 12,
            build_memory_bytes: 64 * 1024 * 1024 * 1024,
            reserved_memory_bytes: 2 * 1024 * 1024 * 1024,
            available_memory_bytes: 14 * 1024 * 1024 * 1024,
        };
        scheduler::validate(&nodes, budget).unwrap();
        let by_id = nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        assert!(by_id["linux"].dependencies.is_empty());
        assert!(by_id["glibc"].dependencies.is_empty());
        assert_eq!(by_id["gcc-runtime"].dependencies, ["glibc"]);
        assert!(by_id["brush"].dependencies.contains(&"make".to_string()));
        assert!(by_id["rootfs"].dependencies.contains(&"apt".to_string()));
        assert!(by_id["rootfs"].dependencies.contains(&"init".to_string()));
        assert!(!by_id["rootfs"].dependencies.contains(&"linux".to_string()));
        assert_eq!(by_id["live-root"].dependencies, ["rootfs"]);
        // The production scheduler represents the already-materialized
        // formal-sysroot barrier by its final producer, Make.
        assert_eq!(by_id["initramfs"].dependencies, ["linux", "make"]);
        assert_eq!(
            by_id["iso"].dependencies,
            ["initramfs", "linux", "live-root"]
        );

        let durations = BTreeMap::from([
            ("acl", 16.862),
            ("apt", 168.123),
            ("attr", 12.009),
            ("binutils", 144.898),
            ("brush", 262.577),
            ("bzip2", 3.025),
            ("coreutils", 283.164),
            ("cosmic-comp", 120.000),
            ("cosmic-initial-setup", 120.000),
            ("cosmic-edit", 90.000),
            ("cozy", 30.000),
            ("cosmic-session", 45.000),
            ("cosmic-greeter", 75.000),
            ("cosmic-panel", 60.000),
            ("cosmic-applets", 180.000),
            ("cosmic-applibrary", 90.000),
            ("cosmic-launcher", 90.000),
            ("cosmic-settings", 180.000),
            ("cosmic-settings-daemon", 90.000),
            ("cosmic-notifications", 60.000),
            ("cosmic-osd", 45.000),
            ("cosmic-bg", 45.000),
            ("cosmic-workspaces", 60.000),
            ("cosmic-files", 120.000),
            ("cosmic-term", 90.000),
            ("cosmic-tweaks", 90.000),
            ("cosmic-randr", 45.000),
            ("cosmic-screenshot", 60.000),
            ("pop-launcher", 60.000),
            ("cosmic-calculator", 45.000),
            ("cosmic-storage", 60.000),
            ("cosmic-monitor", 60.000),
            ("cosmic-utilities", 120.000),
            ("cosmic-portal", 60.000),
            ("cosmic-assets", 5.000),
            ("greetd", 30.000),
            ("cosmic-desktop", 2.000),
            ("curl", 100.519),
            ("dav1d", 15.000),
            ("dbus", 80.000),
            ("dbus-broker", 24.564),
            ("diffutils", 22.260),
            ("dpkg", 85.190),
            ("duktape", 30.000),
            ("elfutils", 39.547),
            ("expat", 6.265),
            ("file", 8.000),
            ("flatpak", 180.000),
            ("bubblewrap", 10.000),
            ("xdg-dbus-proxy", 5.000),
            ("cosmic-store", 120.000),
            ("fuse3", 20.000),
            ("findutils", 57.703),
            ("gcc-compiler", 647.434),
            ("gcc-runtime", 773.452),
            ("glibc", 453.080),
            ("grep", 24.148),
            ("git", 90.000),
            ("glib", 180.000),
            ("appstream", 60.000),
            ("gdk-pixbuf", 30.000),
            ("json-glib", 20.000),
            ("libfyaml", 10.000),
            ("libpng", 20.000),
            ("libxmlb", 30.000),
            ("gpgme", 30.000),
            ("gpgv", 20.000),
            ("gstreamer", 120.000),
            ("gstreamer-base", 60.000),
            ("gzip", 8.000),
            ("init", 2.104),
            ("initramfs", 57.528),
            ("installer", 28.769),
            ("live-root", 651.930),
            ("iproute2", 44.138),
            ("iputils", 2.816),
            ("iso", 1.912),
            ("kmod", 4.024),
            ("libbsd", 24.058),
            ("libcap", 1.061),
            ("libdisplay-info", 20.000),
            ("libdrm", 20.000),
            ("libevdev", 20.000),
            ("libffi", 20.000),
            ("libarchive", 48.000),
            ("libepoxy", 20.000),
            ("freetype", 20.000),
            ("libfontenc", 10.000),
            ("libxfont", 10.000),
            ("libxcvt", 10.000),
            ("libxshmfence", 5.000),
            ("libxkbfile", 10.000),
            ("xkbcomp", 10.000),
            ("libxml2", 20.000),
            ("libinput", 20.000),
            ("libmd", 15.339),
            ("libgpg-error", 10.000),
            ("libgcrypt", 20.000),
            ("libassuan", 10.000),
            ("libksba", 10.000),
            ("npth", 5.000),
            ("libndp", 10.000),
            ("libxcrypt", 182.310),
            ("linux", 442.489),
            ("linux-pam", 11.197),
            ("less", 8.000),
            ("llvm", 900.000),
            ("lz4", 19.220),
            ("make", 17.947),
            ("mesa", 300.000),
            ("vulkan-headers", 7.000),
            ("vulkan-loader", 14.000),
            ("vulkan-tools", 43.000),
            ("x11-compat", 30.000),
            ("libglvnd", 20.000),
            ("xwayland", 33.772),
            ("xdg-desktop-portal", 16.284),
            ("nvidia-driver", 180.000),
            ("ncurses", 39.520),
            ("openssl", 197.919),
            ("ostree", 180.000),
            ("openssh", 35.000),
            ("pcre2", 27.509),
            ("patch", 8.000),
            ("pixman", 20.000),
            ("pipewire", 180.000),
            ("polkit", 45.000),
            ("networkmanager", 90.000),
            ("readline", 20.000),
            ("procps-ng", 29.727),
            ("cpython", 180.000),
            ("rootfs", 107.053),
            ("rust", 1_800.000),
            ("seatd", 20.000),
            ("sed", 53.006),
            ("selinux", 10.330),
            ("shadow", 57.264),
            ("sudo-rs", 18.338),
            ("systemd", 51.721),
            ("tar", 238.505),
            ("util-linux", 15.213),
            ("wayland", 20.000),
            ("xkbcommon", 20.000),
            ("xxhash", 0.539),
            ("xz", 37.482),
            ("zlib", 3.408),
            ("zstd", 45.216),
        ])
        .into_iter()
        .map(|(stage, duration)| (stage.to_string(), duration))
        .collect();
        let report = scheduler::simulate(&nodes, &durations, budget).unwrap();
        println!(
            "successful cold simulation: serial={:.3}s scheduled={:.3}s critical={:.3}s",
            report.serial_seconds, report.scheduled_seconds, report.critical_path_seconds
        );
        assert!(report.scheduled_seconds < report.serial_seconds);
        assert!(report.scheduled_seconds >= report.critical_path_seconds);
    }

    #[test]
    fn stage_keys_exclude_logging_and_documentation_implementation() {
        for stage in [
            BuildStage::Kernel,
            BuildStage::Glibc,
            BuildStage::GccRuntime,
            BuildStage::Binutils,
            BuildStage::GccToolchain,
            BuildStage::Make,
        ] {
            let spec = build_stage_spec(stage);
            assert!(
                !spec
                    .configuration_inputs
                    .iter()
                    .any(|path| path == Path::new("src/tools/mattos-build/src/performance.rs"))
            );
            assert!(
                !spec
                    .configuration_inputs
                    .iter()
                    .any(|path| path.starts_with("docs"))
            );
        }
    }

    #[test]
    fn dependency_outputs_are_not_duplicated_as_configuration_inputs() {
        let rootfs = build_stage_spec(BuildStage::Rootfs);
        assert!(
            !rootfs
                .configuration_inputs
                .iter()
                .any(|path| path == Path::new("out/repository"))
        );
        assert!(
            rootfs
                .dependencies
                .iter()
                .any(|dependency| dependency == "repository")
        );

        let initramfs = build_stage_spec(BuildStage::Initramfs);
        assert!(initramfs.configuration_inputs.is_empty());
        assert_eq!(initramfs.dependencies, ["formal-sysroot", "linux"]);

        let live_root = build_stage_spec(BuildStage::LiveRoot);
        assert_eq!(live_root.dependencies, ["rootfs"]);
        let live_root_resources = stage_resource_profile(BuildStage::LiveRoot);
        assert_eq!(live_root_resources.preferred_child_jobs, 4);
        assert!(live_root_resources.memory_heavy);

        let iso = build_stage_spec(BuildStage::Iso);
        assert!(iso.configuration_inputs.is_empty());
        assert!(
            iso.dependencies
                .iter()
                .any(|dependency| dependency == "initramfs")
        );
    }

    #[test]
    fn gstreamer_stage_output_contract_covers_the_complete_packaged_install() {
        for (stage, install) in [
            (BuildStage::Gstreamer, "out/build/gstreamer/install"),
            (BuildStage::GstreamerBase, "out/build/gstreamer-base/install"),
            (
                BuildStage::XdgDesktopPortal,
                "out/build/xdg-desktop-portal/install",
            ),
        ] {
            let spec = build_stage_spec(stage);
            assert_eq!(spec.outputs, [PathBuf::from(install)]);
        }
    }

    #[test]
    fn cosmic_components_share_one_persistent_serialized_cargo_target() {
        let root = Path::new("/workspace");
        assert_eq!(
            cosmic_shared_target(root),
            PathBuf::from("/workspace/out/build/cosmic-desktop/target")
        );
        assert_eq!(
            cosmic_shared_target_lock(root),
            PathBuf::from("/workspace/out/cache/cosmic-cargo-target.lock")
        );

        for stage in [
            BuildStage::CosmicSession,
            BuildStage::CosmicGreeter,
            BuildStage::CosmicPanel,
            BuildStage::CosmicApplets,
            BuildStage::CosmicLauncher,
            BuildStage::CosmicSettings,
        ] {
            let output = &build_stage_spec(stage).outputs[0];
            assert!(output.starts_with(Path::new("out/build")));
            assert!(!output.starts_with(Path::new("out/build/cosmic-desktop/install")));
        }
    }

    #[test]
    fn cosmic_cargo_remaps_canonical_and_consumer_sources_to_one_identity() {
        let flags = cosmic_source_remap_flags(Path::new("/workspace"));
        assert!(flags.contains(
            "--remap-path-prefix=/workspace/out/build/cosmic-desktop/sources=/usr/src/mattos/cosmic-sources"
        ));
        assert!(flags.contains(
            "--remap-path-prefix=/workspace/out/source-ownership/sources=/usr/src/mattos/cosmic-sources"
        ));
        assert!(flags.contains("--remap-path-prefix=/workspace=/usr/src/mattos"));
        assert_eq!(flags.matches("/usr/src/mattos/cosmic-sources").count(), 2);
    }

    #[test]
    fn linux_projection_metadata_does_not_invalidate_unrelated_builds() {
        for stage in [
            BuildStage::Glibc,
            BuildStage::GccRuntime,
            BuildStage::Binutils,
            BuildStage::GccToolchain,
            BuildStage::Make,
            BuildStage::Coreutils,
        ] {
            let spec = build_stage_spec(stage);
            assert!(!spec.source_inputs.iter().any(|path| {
                path == Path::new("upstream/policies/linux-source-selection.toml")
                    || path == Path::new("upstream/state/linux.toml")
            }));
            assert!(!spec.configuration_inputs.iter().any(|path| {
                path == Path::new("upstream/policies/linux-source-selection.toml")
                    || path == Path::new("upstream/state/linux.toml")
            }));
        }
    }

    #[test]
    fn provenance_policies_are_not_component_build_inputs() {
        for stage in [BuildStage::Tar, BuildStage::Openssl, BuildStage::Pcre2] {
            let spec = build_stage_spec(stage);
            assert!(
                !spec
                    .source_inputs
                    .contains(&PathBuf::from("upstream/policies/gitlinks.toml"))
            );
            assert!(
                !spec
                    .configuration_inputs
                    .contains(&PathBuf::from("upstream/policies/gitlinks.toml"))
            );
        }
    }

    #[test]
    fn native_and_rust_stages_use_only_relevant_workspace_metadata() {
        for stage in [
            BuildStage::Kernel,
            BuildStage::Glibc,
            BuildStage::GccRuntime,
            BuildStage::Binutils,
            BuildStage::GccToolchain,
            BuildStage::Make,
        ] {
            let spec = build_stage_spec(stage);
            assert!(
                !spec
                    .configuration_inputs
                    .contains(&PathBuf::from("Cargo.toml"))
            );
            assert!(
                !spec
                    .configuration_inputs
                    .contains(&PathBuf::from("Cargo.lock"))
            );
        }
        for stage in [BuildStage::Brush, BuildStage::Coreutils, BuildStage::Grep] {
            let spec = build_stage_spec(stage);
            assert!(
                !spec
                    .configuration_inputs
                    .contains(&PathBuf::from("Cargo.toml"))
            );
            assert!(
                !spec
                    .configuration_inputs
                    .contains(&PathBuf::from("Cargo.lock"))
            );
            let component = stage_graph::stage_id(stage);
            assert!(spec.configuration_inputs.contains(&PathBuf::from(format!(
                "out/source-ownership/cargo/contracts/{component}.json"
            ))));
        }
    }

    #[test]
    fn linux_consumers_track_uapi_inputs_without_kernel_image_dependency() {
        let glibc = build_stage_spec(BuildStage::Glibc);
        assert!(!glibc.dependencies.contains(&"linux".to_string()));
        for input in linux_x86_uapi_inputs() {
            assert!(glibc.source_inputs.contains(&PathBuf::from(input)));
        }

        let headers = linux_headers_stage_spec();
        assert_eq!(headers.dependencies, vec!["glibc"]);
        assert_eq!(
            headers.source_inputs,
            linux_x86_uapi_inputs()
                .into_iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        );
    }

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

    fn git_index_snapshot(path: &Path) -> Vec<u8> {
        let output = Command::new("git")
            .args(["ls-files", "--stage", "-z"])
            .current_dir(path)
            .output()
            .expect("read Git index");
        assert!(output.status.success(), "git ls-files failed");
        output.stdout
    }

    fn git_untracked_paths(path: &Path) -> Vec<String> {
        let output = Command::new("git")
            .args(["ls-files", "--others", "--exclude-standard", "-z"])
            .current_dir(path)
            .output()
            .expect("read untracked paths");
        assert!(output.status.success(), "git ls-files --others failed");
        output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| String::from_utf8(path.to_vec()).expect("UTF-8 test path"))
            .collect()
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
            schema_version: 2,
            component: "linux".to_string(),
            repo: "https://github.com/torvalds/linux.git".to_string(),
            branch: "master".to_string(),
            imported_commit: "abc123".to_string(),
            imported_at_utc: "2026-01-01T00:00:00Z".to_string(),
            sync_method: "copy".to_string(),
            destination_path: "src/kernel/linux".to_string(),
            upstream_tree: "0123456789012345678901234567890123456789".to_string(),
            imported_tree_digest_algorithm: IMPORTED_TREE_DIGEST_ALGORITHM.to_string(),
            imported_tree_digest: "0".repeat(64),
            source_selection_policy: "none".to_string(),
            source_selection_policy_sha256: "none".to_string(),
            intentional_omission_policy: "none".to_string(),
            gitlink_policy: "none".to_string(),
            patch_manifest: "none".to_string(),
            patch_manifest_sha256: "none".to_string(),
            lfs_policy: "none".to_string(),
            lfs_policy_sha256: "none".to_string(),
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

    #[cfg(unix)]
    #[test]
    fn initial_import_preserves_filesystem_identity_without_staging_any_path() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let upstream = tempfile::tempdir().expect("upstream tempdir");
        let upstream_root = upstream.path();
        init_git_repo(upstream_root);
        write(
            &upstream_root.join(".gitattributes"),
            "*.bat text eol=crlf\n",
        );
        write(&upstream_root.join("line-endings.bat"), "one\ntwo\n");
        write(&upstream_root.join("normal file.txt"), "space-safe\n");
        write(&upstream_root.join("tool.sh"), "#!/bin/sh\nexit 0\n");
        fs::set_permissions(
            upstream_root.join("tool.sh"),
            fs::Permissions::from_mode(0o755),
        )
        .expect("make executable");
        symlink("normal file.txt", upstream_root.join("normal-link"))
            .expect("create upstream symlink");
        run_ok(upstream_root, "git", &["add", "."]);
        run_ok(upstream_root, "git", &["commit", "-m", "fixture"]);

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        init_git_repo(root);
        write(&root.join("README.md"), "workspace\n");
        run_ok(root, "git", &["add", "README.md"]);
        run_ok(root, "git", &["commit", "-m", "workspace"]);
        let before = git_index_snapshot(root);

        let comp = ComponentDef {
            name: "fixture".to_string(),
            repo: upstream_root.to_string_lossy().to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/userland/fixture tree".to_string(),
            sync: "copy".to_string(),
        };
        import_component(root, &comp, false).expect("initial import");

        assert_eq!(git_index_snapshot(root), before, "import mutated Git index");
        let imported = root.join("src/userland/fixture tree");
        assert_eq!(
            fs::metadata(imported.join("tool.sh"))
                .expect("executable metadata")
                .permissions()
                .mode()
                & 0o111,
            0o111
        );
        assert_eq!(
            fs::read_link(imported.join("normal-link")).expect("imported symlink"),
            Path::new("normal file.txt")
        );
        assert_eq!(
            fs::read(imported.join("line-endings.bat")).expect("exact blob bytes"),
            b"one\ntwo\n"
        );
        assert!(root.join("upstream/state/fixture.toml").is_file());
        let untracked = git_untracked_paths(root);
        assert!(untracked.contains(&"src/userland/fixture tree/normal file.txt".to_string()));
        assert!(untracked.contains(&"upstream/state/fixture.toml".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn sync_updates_worktree_and_state_without_staging_modifications() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let upstream = make_upstream_component_repo("fixture", "tool.sh", "#!/bin/sh\nexit 0\n");
        fs::set_permissions(
            upstream.path().join("tool.sh"),
            fs::Permissions::from_mode(0o755),
        )
        .expect("make executable");
        run_ok(upstream.path(), "git", &["add", "tool.sh"]);
        run_ok(upstream.path(), "git", &["commit", "--amend", "--no-edit"]);

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        init_git_repo(root);
        write(&root.join("README.md"), "workspace\n");
        run_ok(root, "git", &["add", "README.md"]);
        run_ok(root, "git", &["commit", "-m", "workspace"]);
        let comp = ComponentDef {
            name: "fixture".to_string(),
            repo: upstream.path().to_string_lossy().to_string(),
            branch: "main".to_string(),
            revision: None,
            path: "src/userland/fixture".to_string(),
            sync: "copy".to_string(),
        };
        import_component(root, &comp, false).expect("initial import");
        run_ok(
            root,
            "git",
            &["add", "src/userland/fixture", "upstream/state/fixture.toml"],
        );
        run_ok(root, "git", &["commit", "-m", "record fixture"]);

        write(
            &upstream.path().join("tool.sh"),
            "#!/bin/sh\nprintf updated\\n\n",
        );
        write(&upstream.path().join("new file.txt"), "new\n");
        symlink("new file.txt", upstream.path().join("new-link")).expect("new upstream symlink");
        run_ok(upstream.path(), "git", &["add", "."]);
        run_ok(upstream.path(), "git", &["commit", "-m", "update fixture"]);
        let before = git_index_snapshot(root);

        import_component(root, &comp, true).expect("sync update");

        assert_eq!(git_index_snapshot(root), before, "sync mutated Git index");
        assert_eq!(
            fs::metadata(root.join("src/userland/fixture/tool.sh"))
                .expect("updated executable metadata")
                .permissions()
                .mode()
                & 0o111,
            0o111
        );
        assert_eq!(
            fs::read_link(root.join("src/userland/fixture/new-link")).expect("updated symlink"),
            Path::new("new file.txt")
        );
        let status = run_cmd_capture(root, "git", &["status", "--porcelain", "-uall"])
            .expect("read worktree status");
        assert!(status.contains(" M src/userland/fixture/tool.sh"));
        assert!(status.contains(" M upstream/state/fixture.toml"));
        let untracked = git_untracked_paths(root);
        assert!(untracked.contains(&"src/userland/fixture/new file.txt".to_string()));
        assert!(untracked.contains(&"src/userland/fixture/new-link".to_string()));
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

    #[cfg(unix)]
    #[test]
    fn linux_source_selection_removes_stale_architectures_and_preserves_retained_files() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("linux");
        write(&root.join("drivers/example.c"), "outside arch\n");
        write(&root.join("arch/Kconfig"), "shared architecture config\n");
        write(&root.join("arch/x86/kernel/shared.c"), "shared x86\n");
        write(&root.join("arch/x86/kernel/entry_32.S"), "32-bit only\n");
        write(&root.join("arch/arm64/kernel/head.S"), "arm64\n");
        write(&root.join("arch/riscv/kernel/head.S"), "riscv\n");
        write(&root.join("arch/um/kernel/main.c"), "um\n");
        write(
            &root.join("arch/arm/crypto/Kconfig"),
            "shared crypto config\n",
        );
        write(
            &root.join("arch/arm/kernel/head.S"),
            "excluded architecture\n",
        );
        fs::set_permissions(
            root.join("arch/x86/kernel/shared.c"),
            fs::Permissions::from_mode(0o755),
        )
        .expect("set executable mode");
        symlink("shared.c", root.join("arch/x86/kernel/shared-link"))
            .expect("create retained symlink");

        let policy = SourceSelectionPolicy {
            schema_version: 1,
            component: "linux".to_string(),
            upstream_commit: "0".repeat(40),
            scope: "arch".to_string(),
            retain_arch_root_files: true,
            retained_architectures: ["x86", "arm64", "riscv", "um"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            retained_arch_paths: ["arm/crypto/Kconfig"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            x86_excluded_paths: ["kernel/entry_32.S"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        };

        apply_source_selection(&root, Some(&policy)).expect("apply projection");

        assert!(root.join("drivers/example.c").is_file());
        assert!(root.join("arch/Kconfig").is_file());
        assert!(root.join("arch/x86/kernel/shared.c").is_file());
        assert!(root.join("arch/arm64/kernel/head.S").is_file());
        assert!(root.join("arch/riscv/kernel/head.S").is_file());
        assert!(root.join("arch/um/kernel/main.c").is_file());
        assert!(root.join("arch/arm/crypto/Kconfig").is_file());
        assert!(!root.join("arch/arm/kernel").exists());
        assert!(!root.join("arch/x86/kernel/entry_32.S").exists());
        assert_eq!(
            fs::metadata(root.join("arch/x86/kernel/shared.c"))
                .expect("retained metadata")
                .permissions()
                .mode()
                & 0o111,
            0o111
        );
        assert_eq!(
            fs::read_link(root.join("arch/x86/kernel/shared-link")).expect("retained symlink"),
            Path::new("shared.c")
        );
    }

    #[test]
    fn importer_reapplies_linux_source_selection_at_unchanged_pin() {
        let upstream = tempfile::tempdir().expect("upstream tempdir");
        let upstream_root = upstream.path();
        init_git_repo(upstream_root);
        write(&upstream_root.join("drivers/example.c"), "outside arch\n");
        write(&upstream_root.join("arch/x86/Kconfig"), "retained x86\n");
        write(&upstream_root.join("arch/arm/Kconfig"), "excluded arm\n");
        run_ok(upstream_root, "git", &["add", "."]);
        run_ok(upstream_root, "git", &["commit", "-m", "pinned linux"]);
        let revision = run_cmd_capture(upstream_root, "git", &["rev-parse", "HEAD"])
            .expect("upstream revision")
            .trim()
            .to_string();

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        init_git_repo(root);
        write(&root.join("README.md"), "workspace\n");
        run_ok(root, "git", &["add", "README.md"]);
        run_ok(root, "git", &["commit", "-m", "workspace"]);
        let policy = format!(
            "schema_version = 1\ncomponent = \"linux\"\nupstream_commit = \"{revision}\"\nscope = \"arch\"\nretain_arch_root_files = true\nretained_architectures = [\"x86\", \"arm64\", \"riscv\", \"um\"]\nx86_excluded_paths = [\"kernel/entry_32.S\"]\n"
        );
        let policy_path = root.join("upstream/policies/linux-source-selection.toml");
        write(&policy_path, &policy);
        let policy_sha256 = format!("{:x}", Sha256Hasher::digest(policy.as_bytes()));
        write(
            &root.join("upstream/sources.toml"),
            &format!(
                "[[component]]\nname = \"linux\"\nrepo = \"{}\"\nbranch = \"main\"\nrevision = \"{revision}\"\npath = \"src/kernel/linux\"\nsync = \"copy\"\nsource_selection_policy = \"upstream/policies/linux-source-selection.toml\"\nsource_selection_policy_sha256 = \"{policy_sha256}\"\n",
                upstream_root.display()
            ),
        );
        let comp = ComponentDef {
            name: "linux".to_string(),
            repo: upstream_root.to_string_lossy().to_string(),
            branch: "main".to_string(),
            revision: Some(revision),
            path: "src/kernel/linux".to_string(),
            sync: "copy".to_string(),
        };

        import_component(root, &comp, false).expect("initial projected import");
        let imported = root.join("src/kernel/linux");
        assert!(imported.join("arch/x86/Kconfig").is_file());
        assert!(imported.join("drivers/example.c").is_file());
        assert!(!imported.join("arch/arm").exists());

        write(&imported.join("arch/arm/stale.c"), "stale excluded path\n");
        import_component(root, &comp, true).expect("unchanged-pin projected sync");
        assert!(!imported.join("arch/arm").exists());
        assert!(imported.join("arch/x86/Kconfig").is_file());
        let state = read_sync_state(root, "linux")
            .expect("read state")
            .expect("state exists");
        assert_eq!(
            state.imported_tree_digest_algorithm,
            SELECTED_IMPORTED_TREE_DIGEST_ALGORITHM
        );
        assert_eq!(state.source_selection_policy_sha256, policy_sha256);
    }

    #[cfg(unix)]
    #[test]
    fn importer_preserves_ignored_tracked_files_modes_and_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let upstream = tempfile::tempdir().expect("upstream tempdir");
        let upstream_root = upstream.path();
        init_git_repo(upstream_root);
        write(&upstream_root.join(".gitignore"), "release-input\n");
        write(
            &upstream_root.join(".gitattributes"),
            "*.bat text eol=crlf\n",
        );
        write(
            &upstream_root.join("release-input"),
            "tracked despite upstream ignore\n",
        );
        write(&upstream_root.join("windows.bat"), "first\nsecond\n");
        write(&upstream_root.join("tool.sh"), "#!/bin/sh\nexit 0\n");
        fs::set_permissions(
            upstream_root.join("tool.sh"),
            fs::Permissions::from_mode(0o755),
        )
        .expect("set executable mode");
        symlink("tool.sh", upstream_root.join("tool-link")).expect("create upstream symlink");
        run_ok(
            upstream_root,
            "git",
            &[
                "add",
                ".gitignore",
                ".gitattributes",
                "windows.bat",
                "tool.sh",
                "tool-link",
            ],
        );
        run_ok(upstream_root, "git", &["add", "-f", "release-input"]);
        run_ok(upstream_root, "git", &["commit", "-m", "pinned source"]);
        let revision = run_cmd_capture(upstream_root, "git", &["rev-parse", "HEAD"])
            .expect("upstream revision")
            .trim()
            .to_string();

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let root = workspace.path();
        init_git_repo(root);
        write(&root.join("README.md"), "workspace\n");
        run_ok(root, "git", &["add", "README.md"]);
        run_ok(root, "git", &["commit", "-m", "workspace"]);
        let comp = ComponentDef {
            name: "example".to_string(),
            repo: upstream_root.to_string_lossy().to_string(),
            branch: "main".to_string(),
            revision: Some(revision),
            path: "src/imported/example".to_string(),
            sync: "copy".to_string(),
        };

        let index_before =
            run_cmd_capture(root, "git", &["write-tree"]).expect("snapshot index before import");
        import_component(root, &comp, false).expect("import pinned source");
        let index_after =
            run_cmd_capture(root, "git", &["write-tree"]).expect("snapshot index after import");
        assert_eq!(
            index_after, index_before,
            "import must not mutate the index"
        );
        let imported = root.join("src/imported/example");
        assert_eq!(
            fs::metadata(imported.join("tool.sh"))
                .expect("tool metadata")
                .permissions()
                .mode()
                & 0o111,
            0o111
        );
        assert_eq!(
            fs::read_link(imported.join("tool-link")).expect("imported symlink"),
            Path::new("tool.sh")
        );
        assert_eq!(
            fs::read(imported.join("windows.bat")).expect("attributed source"),
            b"first\nsecond\n"
        );
        let state = read_sync_state(root, "example")
            .expect("read state")
            .expect("state exists");
        assert_eq!(state.schema_version, 2);
        assert_eq!(
            state.imported_tree_digest_algorithm,
            IMPORTED_TREE_DIGEST_ALGORITHM
        );
        assert_eq!(state.upstream_tree.len(), 40);
        assert_eq!(state.imported_tree_digest.len(), 64);
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
    fn mattos_build_temp_directory_is_output_owned_and_writable() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path();
        fs::create_dir_all(repository.join("src/tools/mattos-build")).unwrap();
        fs::write(
            repository.join("src/tools/mattos-build/Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();
        let selected = ensure_mattos_build_tmp(repository).unwrap();
        assert_eq!(selected, repository.join("out/tmp"));
        assert!(selected.is_dir());
        assert!(fs::read_dir(&selected).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".write-probe-")
        }));
    }

    #[test]
    fn mattos_build_temp_probe_is_concurrency_safe() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = std::sync::Arc::new(temporary.path().to_path_buf());
        fs::create_dir_all(repository.join("src/tools/mattos-build")).unwrap();
        fs::write(
            repository.join("src/tools/mattos-build/Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));
        let workers = (0..16)
            .map(|_| {
                let repository = repository.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    ensure_mattos_build_tmp(repository.as_ref())
                })
            })
            .collect::<Vec<_>>();

        for worker in workers {
            assert_eq!(
                worker.join().expect("temp probe worker panicked").unwrap(),
                repository.join("out/tmp")
            );
        }

        let leftovers = fs::read_dir(repository.join("out/tmp"))
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".write-probe-"))
            .collect::<Vec<_>>();
        assert!(leftovers.is_empty(), "temp probes leaked: {leftovers:?}");
    }

    #[test]
    fn mattos_temp_environment_overrides_host_tmpdir_for_repo_commands() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path();
        fs::create_dir_all(repository.join("src/tools/mattos-build")).unwrap();
        fs::write(
            repository.join("src/tools/mattos-build/Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();
        let mut command = Command::new("sh");
        command.env("TMPDIR", "/host/tmp");
        apply_mattos_tmp_environment(&mut command, repository).unwrap();
        let debug = format!("{command:?}");
        assert!(debug.contains("out/tmp"));
        assert!(!debug.contains("/host/tmp"));

        let observed = run_cmd_capture(repository, "sh", &["-c", "printf '%s' \"$TMPDIR\""])
            .expect("child should inherit MattOS TMPDIR");
        assert_eq!(observed, repository.join("out/tmp").display().to_string());
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
            "menuentry \"Start MattOS Live\" {}\nmenuentry \"MattOS Rescue\" {}\n",
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
            "menuentry \"Start MattOS Live\" {}\nmenuentry \"MattOS Rescue\" {}\n",
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
            "set default=0\nmenuentry \"Start MattOS Live\" { linux /boot/vmlinuz rdinit=/init }\nmenuentry \"Start MattOS Live (CLI)\" { linux /boot/vmlinuz rdinit=/init }\nmenuentry \"Install MattOS\" { linux /boot/vmlinuz rdinit=/init }\nmenuentry \"Install MattOS (CLI)\" { linux /boot/vmlinuz rdinit=/init }\n",
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
            "menuentry \"Start MattOS Live\" { linux /boot/vmlinuz rdinit=/init initrd /boot/early-initramfs.cpio.xz }\nmenuentry \"Start MattOS Live (CLI)\" { linux /boot/vmlinuz rdinit=/init initrd /boot/early-initramfs.cpio.xz }\nmenuentry \"Install MattOS\" { linux /boot/vmlinuz rdinit=/init initrd /boot/early-initramfs.cpio.xz }\nmenuentry \"Install MattOS (CLI)\" { linux /boot/vmlinuz rdinit=/init initrd /boot/early-initramfs.cpio.xz }\nmenuentry \"MattOS Rescue\" { linux /boot/vmlinuz rdinit=/init mattos.rescue=1 initrd /boot/early-initramfs.cpio.xz }\n",
        );

        validate_staged_grub_config(&path).expect("valid staged config should pass");
    }

    #[test]
    fn authoritative_grub_uses_one_live_payload_for_all_boot_modes() {
        let grub = include_str!("../../../boot/grub/grub.cfg");
        let linux_lines = grub
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("linux "))
            .collect::<Vec<_>>();
        assert_eq!(linux_lines.len(), 5);
        assert!(
            linux_lines
                .iter()
                .all(|line| line.contains(GRUB_EARLY_RDINIT))
        );
        assert_eq!(
            grub.matches("initrd /boot/early-initramfs.cpio.xz").count(),
            5
        );
        for required in ["insmod all_video", "set gfxpayload=keep"] {
            assert!(
                grub.lines().any(|line| line == required),
                "missing {required}"
            );
        }
        assert!(
            grub.lines()
                .any(|line| line.starts_with("set gfxmode=") && line.contains("auto")),
            "missing an automatic fallback in gfxmode"
        );
        assert!(!grub.contains("initramfs_options=size="));
    }

    #[test]
    fn live_media_boot_modes_are_explicit_and_early_dispatcher_owned() {
        let grub = include_str!("../../../boot/grub/grub.cfg");
        for mode in [
            "mattos.mode=live",
            "mattos.mode=live-cli",
            "mattos.mode=install-gui",
            "mattos.mode=install-cli",
        ] {
            assert!(grub.contains(mode), "GRUB omits explicit {mode} contract");
        }
        assert!(!grub.contains("systemd.unit="));

        let dispatcher = include_str!("../../../boot/live-init.c");
        for target in [
            "mattos-live-graphical.target",
            "mattos.target",
            "mattos-install-graphical.target",
            "mattos-install-cli.target",
        ] {
            assert!(
                dispatcher.contains(target),
                "early dispatcher omits {target}"
            );
        }
        assert!(dispatcher.contains("/proc/cmdline"));
        assert!(dispatcher.contains("--unit=%s"));
        assert!(dispatcher.contains("command_line_has_token(\"mattos.mode=live\")"));
        assert!(dispatcher.contains("systemd_target = LIVE_GUI_TARGET"));
        assert!(dispatcher.contains("systemd_target = LIVE_CLI_TARGET"));

        let graphical = include_str!("../../../system/units/mattos-live-graphical.target");
        assert!(graphical.contains("Requires=graphical.target"));
        assert!(graphical.contains("After=graphical.target"));
        let cli = include_str!("../../../system/units/mattos.target");
        assert!(cli.contains("Requires=multi-user.target"));
        assert!(!cli.contains("graphical.target"));

        let live_greetd = include_str!("../../../system/profiles/live/etc/greetd/cosmic-live.toml");
        assert!(live_greetd.contains("[initial_session]"));
        assert!(live_greetd.contains("command = \"/usr/bin/start-cosmic\""));
        assert!(live_greetd.contains("user = \"mattos\""));
        let live_override = include_str!(
            "../../../system/profiles/live/etc/systemd/system/cosmic-greeter.service.d/live.conf"
        );
        assert!(
            live_override
                .contains("ExecStart=/usr/bin/greetd --config /etc/greetd/cosmic-live.toml")
        );
    }

    #[test]
    fn flatpak_defaults_leave_socket_policy_to_unmodified_application_manifests() {
        // MattOS deliberately ships no Flatpak overrides. Socket policy belongs
        // to each upstream application manifest: Wayland-only apps use COSMIC's
        // Wayland socket, X11-only apps use Xwayland, and fallback-X11 apps may
        // choose native Wayland when it is available.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let skeleton = root.join("src/rootfs/skeleton/etc/skel/.local/share/flatpak/overrides");
        assert!(
            !skeleton.exists(),
            "MattOS must not ship global or application-specific Flatpak overrides"
        );
        assert!(
            !LEGACY_SKELETON_FILES.contains(&"etc/skel/.local/share/flatpak/overrides/global"),
            "rootfs assembly must not retain a copy rule for the removed X11-only override"
        );
    }

    #[test]
    fn flatpak_package_owns_signed_flathub_policy_without_application_overrides() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let descriptor = root.join("src/system/packages/flatpak/resources/flathub.flatpakrepo");
        let policy = std::fs::read_to_string(&descriptor)
            .expect("MattOS must retain the packaged Flathub descriptor");
        assert!(policy.contains("[Flatpak Repo]"));
        assert!(policy.contains("Url=https://dl.flathub.org/repo/"));
        let key = policy
            .split_once("GPGKey=")
            .expect("Flathub policy must embed its verification key")
            .1
            .trim();
        assert!(key.len() > 3_000, "Flathub policy must retain the full pinned public key");
        let package_source = include_str!("packaging.rs");
        assert!(package_source.contains("usr/share/flatpak/remotes.d/flathub.flatpakrepo"));
        assert!(package_source.contains("stage_flatpak_system_remote"));
        assert!(package_source.contains("var/lib/flatpak/repo"));
        assert!(!root.join("src/rootfs/skeleton/etc/skel/.local/share/flatpak/overrides").exists());
    }

    #[test]
    fn graphical_installer_waits_for_the_actual_wayland_socket() {
        let unit = include_str!("../../../system/units/mattos-install-graphical.service");
        assert!(unit.contains("TimeoutStartSec=120"));
        assert!(unit.contains("ExecStartPre=/usr/bin/sh -ec"));
        assert!(unit.contains("test -S \"$${XDG_RUNTIME_DIR}/$${WAYLAND_DISPLAY}\""));
        assert!(unit.contains("compositor did not publish"));
        assert!(unit.contains("Environment=WGPU_BACKEND=gl"));
        assert!(!unit.contains("LIBGL_ALWAYS_SOFTWARE"));
        assert!(!unit.contains("MESA_LOADER_DRIVER_OVERRIDE"));
        assert!(unit.contains("ExecStart=/usr/bin/mattos-install-cosmic"));
    }

    #[test]
    fn graphical_installer_session_leaves_virtio_kms_renderer_selection_to_mesa() {
        let unit = include_str!("../../../system/units/mattos-cosmic-installer-session.service");
        assert!(unit.contains("VirGL Gallium path"));
        assert!(unit.contains("Type=notify"));
        assert!(unit.contains("NotifyAccess=all"));
        assert!(unit.contains("Environment=LANG=en_US.UTF-8"));
        assert!(unit.contains("Environment=XCURSOR_THEME=Pop"));
        assert!(unit.contains(
            "dbus-run-session --config-file=/usr/share/dbus-1/mattos-private-session.conf"
        ));
        assert!(!unit.contains("LIBGL_ALWAYS_SOFTWARE"));
        assert!(!unit.contains("MESA_LOADER_DRIVER_OVERRIDE"));
        assert!(!unit.contains("GALLIUM_DRIVER"));
    }

    #[test]
    fn mesa_stage_covers_generic_hardware_virtual_and_software_renderers() {
        let source = include_str!("stages/graphics.rs");
        let start = source.find("fn build_mesa").unwrap();
        let recipe = &source[start..];
        assert!(recipe.contains("-Dgallium-drivers=radeonsi,iris,nouveau,virgl,llvmpipe,svga"));
        assert!(recipe.contains("-Dvulkan-drivers=amd,intel,nouveau,swrast,virtio"));
        for option in [
            "-Dplatforms=wayland",
            "-Degl=enabled",
            "-Dgbm=enabled",
            "-Dopengl=true",
            "-Dgles1=enabled",
            "-Dgles2=enabled",
            "-Dvulkan-layers=device-select",
        ] {
            assert!(recipe.contains(option), "Mesa recipe omits {option}");
        }
        assert!(!recipe.contains("LIBGL_ALWAYS_SOFTWARE"));
        assert!(!recipe.contains("MESA_LOADER_DRIVER_OVERRIDE"));
    }

    #[test]
    fn obsolete_full_root_initramfs_names_are_rejected() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        assert!(reject_obsolete_full_root_initramfs(root).is_ok());
        let obsolete = root.join(OBSOLETE_FULL_ROOT_INITRAMFS_PATHS[0]);
        fs::create_dir_all(obsolete.parent().unwrap()).unwrap();
        fs::write(&obsolete, b"obsolete full root").unwrap();
        let error = reject_obsolete_full_root_initramfs(root)
            .unwrap_err()
            .to_string();
        assert!(error.contains("obsolete full-root initramfs"));
        assert!(error.contains(INITRAMFS_ARCHIVE_PATH));
        assert!(error.contains(LIVE_ROOT_IMAGE_PATH));
    }

    #[test]
    fn artifact_report_has_unambiguous_live_and_installed_roles() {
        let source = include_str!("main.rs");
        for role in [
            "Kernel",
            "Live early initramfs",
            "Live early initramfs (uncompressed)",
            "Live root SquashFS",
            "Installed initramfs",
            "UEFI ISO boot image",
            "Final ISO",
        ] {
            assert!(source.contains(role), "artifact report is missing {role}");
        }
        assert_ne!(INITRAMFS_ARCHIVE_PATH, LIVE_ROOT_IMAGE_PATH);
        assert_ne!(INITRAMFS_ARCHIVE_PATH, INSTALLED_INITRAMFS_PATH);
    }

    #[test]
    fn cosmic_installer_lock_pins_normal_dependencies() {
        let lock_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../system/installer/gui/cosmic/Cargo.lock");
        validate_cosmic_installer_lock(&lock_path).unwrap();
        let lock = fs::read_to_string(lock_path).unwrap();
        for package in [
            "libcosmic",
            "cosmic-protocols",
            "cosmic-freedesktop-icons",
            "cosmic-settings-daemon",
            "winit",
            "accesskit_winit",
        ] {
            assert!(
                lock.contains(&format!("name = {package:?}")),
                "native COSMIC lock is missing {package}"
            );
        }
    }

    #[test]
    fn cosmic_installer_lock_rejects_unpinned_git_and_registry_sources() {
        let temporary = tempfile::tempdir().unwrap();
        let authoritative = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../system/installer/gui/cosmic/Cargo.lock"),
        )
        .unwrap();
        let unpinned =
            authoritative.replacen("#e429a025df36ab8145708acb309080ae3deec17a\"", "\"", 1);
        let unpinned_path = temporary.path().join("unpinned.lock");
        fs::write(&unpinned_path, unpinned).unwrap();
        assert!(validate_cosmic_installer_lock(&unpinned_path).is_err());

        let unchecked = authoritative.replacen("checksum = \"", "unchecked = \"", 1);
        let unchecked_path = temporary.path().join("unchecked.lock");
        fs::write(&unchecked_path, unchecked).unwrap();
        assert!(validate_cosmic_installer_lock(&unchecked_path).is_err());
    }

    #[test]
    fn cosmic_installer_recreates_its_output_owned_source_mirror() {
        let source = include_str!("stages/image.rs");
        let function = source
            .split_once("fn build_cosmic_installer_frontend")
            .unwrap()
            .1
            .split_once("fn validate_cosmic_installer_lock")
            .unwrap()
            .0;
        let cleanup = function
            .find("remove_path_if_exists(&source_root)")
            .unwrap();
        let first_sync = function.find("sync_build_source").unwrap();
        assert!(cleanup < first_sync);
        for demoted in [
            "dbus-settings-bindings",
            "freedesktop-icons",
            "winit",
            "window-clipboard",
            "softbuffer",
            "smithay-clipboard",
            "accesskit",
            "cryoglyph",
            "rust-atomicwrites",
        ] {
            assert!(!function.contains(&format!("source_root.join({demoted:?})")));
        }
    }

    #[test]
    fn write_sync_state_creates_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let state = SyncState {
            schema_version: 2,
            component: "brush".to_string(),
            repo: "https://example.invalid/brush.git".to_string(),
            branch: "main".to_string(),
            imported_commit: "def456".to_string(),
            imported_at_utc: "2026-01-01T00:00:00Z".to_string(),
            sync_method: "copy".to_string(),
            destination_path: "src/userland/brush".to_string(),
            upstream_tree: "0123456789012345678901234567890123456789".to_string(),
            imported_tree_digest_algorithm: IMPORTED_TREE_DIGEST_ALGORITHM.to_string(),
            imported_tree_digest: "0".repeat(64),
            source_selection_policy: "none".to_string(),
            source_selection_policy_sha256: "none".to_string(),
            intentional_omission_policy: "none".to_string(),
            gitlink_policy: "none".to_string(),
            patch_manifest: "none".to_string(),
            patch_manifest_sha256: "none".to_string(),
            lfs_policy: "none".to_string(),
            lfs_policy_sha256: "none".to_string(),
        };
        write_sync_state(root, "brush", &state).expect("write state");
        assert!(root.join("upstream/state/brush.toml").exists());
    }

    #[test]
    fn check_name_rejects_empty() {
        assert!(validate_component_name("").is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn host_tool_probe_does_not_depend_on_external_which() {
        use std::os::unix::fs::PermissionsExt;

        let temporary = tempfile::tempdir().unwrap();
        let executable = temporary.path().join("mattos-tool");
        fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::join_paths([temporary.path()]).unwrap();
        assert!(command_exists_in_path("mattos-tool", &path));
        assert!(!command_exists_in_path("which", &path));
        assert!(!command_exists_in_path("missing-tool", &path));
        assert!(include_str!("commands/doctor.rs")
            .contains("if !missing_required.contains(&\"pkg-config\")"));
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
    fn cosmic_just_mirror_uses_external_cargo_target_for_install() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let justfile = tmp.path().join("justfile");
        write(&justfile, "bin-src := 'target' / 'release' / name\n");

        patch_cosmic_just_target_path(tmp.path()).expect("patch justfile");

        let body = fs::read_to_string(justfile).expect("read patched justfile");
        assert!(body.contains("bin-src := env('CARGO_TARGET_DIR', 'target') / 'release' / name"));
    }

    #[test]
    fn cosmic_just_mirror_preserves_target_override() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let justfile = tmp.path().join("justfile");
        let original = "bin-src := env('CARGO_TARGET_DIR', 'target') / 'release' / name\n";
        write(&justfile, original);

        patch_cosmic_just_target_path(tmp.path()).expect("inspect justfile");

        assert_eq!(
            fs::read_to_string(justfile).expect("read justfile"),
            original
        );
    }

    #[test]
    fn cosmic_just_mirror_adapts_release_recipe_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let justfile = tmp.path().join("justfile");
        write(
            &justfile,
            "bin-src := 'target' / 'release' / name\ndesktop-src := 'resources' / appid + '.desktop'\nappdata-src := 'resources' / appid + '.metainfo.xml'\nrelease *args:\n    cargo build --release {{args}}\n",
        );

        patch_cosmic_just_target_path(tmp.path()).expect("patch justfile");

        let body = fs::read_to_string(justfile).expect("read patched justfile");
        assert!(body.contains("build-release *args: (release args)"));
        assert!(!body.contains("--locked {{args}}"));
        assert!(body.contains("desktop-src := 'resources' / 'app.desktop'"));
        assert!(body.contains("appdata-src := 'resources' / 'app.metainfo.xml'"));
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
    fn kernel_build_metadata_and_builtin_initramfs_time_are_pinned() {
        let source = include_str!("main.rs");
        let start = source.find("fn build_kernel").unwrap();
        let build = &source[start..];
        for required in [
            "KBUILD_BUILD_TIMESTAMP=2026-01-01 00:00:00 UTC",
            "KBUILD_BUILD_USER=mattos",
            "KBUILD_BUILD_HOST=mattos-build",
            "KBUILD_BUILD_VERSION=1",
            "KCONFIG_NOTIMESTAMP=1",
        ] {
            assert!(
                build.contains(required),
                "missing kernel reproducibility setting {required}"
            );
        }
        assert!(build.contains("olddefconfig_args.extend(kernel_reproducible_args)"));
        assert!(build.contains("build_args.extend(kernel_reproducible_args)"));
    }

    #[test]
    fn generic_kernel_policy_classifies_builtin_module_and_unsupported_symbols() {
        let config = include_str!("../../../kernel/config/x86_64_mattos.config");
        let policy: KernelConfigPolicy = toml::from_str(include_str!(
            "../../../kernel/config/x86_64_mattos.policy.toml"
        ))
        .unwrap();
        validate_kernel_config_policy(config, &policy).unwrap();
        assert_eq!(
            kernel_config_state(config, "CONFIG_ISO9660_FS"),
            Some(KernelConfigState::Builtin)
        );
        assert_eq!(
            kernel_config_state(config, "CONFIG_SCSI_VIRTIO"),
            Some(KernelConfigState::Module)
        );
        assert_eq!(
            kernel_config_state(config, "CONFIG_PCCARD"),
            Some(KernelConfigState::Unsupported)
        );
    }

    #[test]
    fn initramfs_module_closure_orders_dependencies_before_boot_drivers() {
        let dependencies = BTreeMap::from([
            ("kernel/drivers/virtio/virtio.ko.zst".into(), vec![]),
            (
                "kernel/drivers/scsi/virtio_scsi.ko.zst".into(),
                vec!["kernel/drivers/virtio/virtio.ko.zst".into()],
            ),
        ]);
        let mut visiting = BTreeSet::new();
        let mut ordered = Vec::new();
        add_module_with_dependencies(
            "kernel/drivers/scsi/virtio_scsi.ko.zst",
            &dependencies,
            &mut visiting,
            &mut ordered,
        )
        .unwrap();
        assert_eq!(
            ordered,
            [
                "kernel/drivers/virtio/virtio.ko.zst",
                "kernel/drivers/scsi/virtio_scsi.ko.zst"
            ]
        );
        assert_eq!(
            module_basename("kernel/fs/btrfs/btrfs.ko.zst").as_deref(),
            Some("btrfs")
        );
    }

    #[test]
    fn early_init_is_static_role_driven_and_switches_to_the_live_root() {
        let init = include_str!("../../../boot/live-init.c");
        for required in [
            "LOOP_CTL_GET_FREE",
            "LOOP_SET_FD",
            "int loop_descriptor = attach_live_root_loop",
            "close(loop_descriptor);",
            "rootfs.squashfs",
            "\"squashfs\"",
            "lowerdir=/run/mattos/lower",
            "\"overlay\"",
            "make_directory(\"/newroot/dev\"",
            "make_directory(\"/newroot/proc\"",
            "make_directory(\"/newroot/sys\"",
            "make_directory(\"/newroot/run\"",
            "MS_MOVE",
            "chroot",
            "SYSTEMD_PATH",
        ] {
            assert!(
                init.contains(required),
                "missing early-init policy {required}"
            );
        }
        assert!(
            init.find("mount_required(loop_path").unwrap()
                < init.find("close(loop_descriptor);").unwrap(),
            "the autoclear loop descriptor must remain open until SquashFS is mounted"
        );
        let spec = build_stage_spec(BuildStage::Initramfs);
        assert_eq!(
            spec.source_inputs,
            [
                PathBuf::from("src/boot/live-init.c"),
                PathBuf::from("src/boot/module-loader.h"),
                PathBuf::from("src/system/data/linux-firmware"),
                PathBuf::from("src/tools/mattos-build/src/stages/image.rs")
            ]
        );
        assert!(
            !spec
                .dependencies
                .iter()
                .any(|dependency| dependency == "rootfs")
        );
        assert_eq!(spec.dependencies, ["formal-sysroot", "linux"]);
    }

    #[test]
    fn early_initramfs_root_mode_is_explicit_and_not_umask_dependent() {
        let source = include_str!("stages/image.rs");
        let start = source.find("fn build_initramfs_atomic").unwrap();
        let end = source[start..]
            .find("fn validate_initramfs_archive_owner")
            .unwrap()
            + start;
        assert!(source[start..end].contains("set_mode(tree.clone(), 0o755)?"));
    }

    #[test]
    fn rust_bootstrap_uses_the_mattos_llvm_install_not_a_second_llvm() {
        let source = include_str!("main.rs");
        let start = source.find("fn build_rust").unwrap();
        let end = source[start..].find("fn build_bzip2").unwrap() + start;
        let rust = &source[start..end];
        assert!(rust.contains("out/build/llvm/install/usr/bin/llvm-config"));
        assert!(rust.contains("download-ci-llvm = false"));
        assert!(rust.contains("submodules = false"));
        assert!(rust.contains("llvm-has-rust-patches = false"));
    }

    #[test]
    fn imported_build_outputs_are_all_under_out() {
        for stage in [
            BuildStage::Kernel,
            BuildStage::Brush,
            BuildStage::Coreutils,
            BuildStage::Grep,
            BuildStage::Sed,
            BuildStage::Findutils,
            BuildStage::Diffutils,
            BuildStage::Procps,
            BuildStage::Shadow,
            BuildStage::SudoRs,
        ] {
            let spec = build_stage_spec(stage);
            assert!(
                spec.outputs.iter().all(|path| path.starts_with("out/")),
                "{} has a source-tree output: {:?}",
                build_stage_id(stage),
                spec.outputs
            );
        }
        for binary in USERLAND_BINARY_INSTALLS {
            assert!(Path::new(binary.source_rel).starts_with("out/build"));
        }
    }

    #[test]
    fn imported_source_mirror_excludes_ignored_residue_and_preserves_source() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        init_git_repo(root);
        let source = root.join("src/imported/example");
        write(&source.join(".gitignore"), "target/\nignored-source.txt\n");
        write(&source.join("tracked.txt"), "tracked\n");
        write(
            &source.join("ignored-source.txt"),
            "upstream tracks this release input\n",
        );
        write(&source.join("untracked.txt"), "untracked\n");
        write(&source.join("target/generated.o"), "old generated output\n");
        run_ok(root, "git", &["add", "src/imported/example/.gitignore"]);
        run_ok(root, "git", &["add", "src/imported/example/tracked.txt"]);
        run_ok(
            root,
            "git",
            &["add", "-f", "src/imported/example/ignored-source.txt"],
        );
        let before = performance::output_path_digest(root, &source).expect("source snapshot");

        let mirror = root.join("out/build/example/source");
        copy_imported_working_tree(root, Path::new("src/imported/example"), &mirror)
            .expect("create source mirror");
        assert_eq!(
            fs::read_to_string(mirror.join("tracked.txt")).unwrap(),
            "tracked\n"
        );
        assert_eq!(
            fs::read_to_string(mirror.join("untracked.txt")).unwrap(),
            "untracked\n"
        );
        assert_eq!(
            fs::read_to_string(mirror.join("ignored-source.txt")).unwrap(),
            "upstream tracks this release input\n"
        );
        assert!(!mirror.join("target/generated.o").exists());
        write(&mirror.join("generated/config.h"), "generated in output\n");

        let after = performance::output_path_digest(root, &source).expect("source snapshot");
        assert_eq!(before, after);
    }

    #[test]
    fn rootfs_configuration_digest_is_exact_and_documentation_stable() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        for path in stage_inputs::rootfs_configuration_inputs() {
            let absolute = root.join(path);
            if absolute.extension().is_some()
                || absolute.file_name() == Some(OsStr::new("hosts"))
                || absolute.file_name() == Some(OsStr::new("networks"))
            {
                write(&absolute, "configuration\n");
            } else {
                write(&absolute.join("payload"), "configuration\n");
            }
        }
        write(
            &root.join("src/tools/mattos-build/src/stages/image.rs"),
            "builder\n",
        );
        write(
            &root.join("src/tools/mattos-build/Cargo.toml"),
            "workspace\n",
        );
        write(&root.join("Cargo.lock"), "lock\n");
        write(&root.join("out/packages/inventory.toml"), "packages\n");
        write(&root.join("out/repository/Packages"), "repository\n");

        let spec = build_stage_spec(BuildStage::Rootfs);
        let first = performance::compute_stage_inputs(root, &spec).expect("first rootfs key");
        write(
            &root.join("src/system/units/payload"),
            "changed configuration\n",
        );
        let changed = performance::compute_stage_inputs(root, &spec).expect("changed rootfs key");
        assert_ne!(first.configuration_digest, changed.configuration_digest);

        write(
            &root.join("src/system/network/README.md"),
            "unrelated documentation\n",
        );
        let documented =
            performance::compute_stage_inputs(root, &spec).expect("documentation rootfs key");
        assert_eq!(
            changed.configuration_digest,
            documented.configuration_digest
        );

        for direct_dependency in ["grep", "sed", "findutils", "diffutils"] {
            assert!(
                spec.dependencies
                    .iter()
                    .any(|value| value == direct_dependency)
            );
        }
        assert_eq!(build_stage_dependencies(BuildStage::LiveRoot), &["rootfs"]);
        assert_eq!(
            build_stage_dependencies(BuildStage::Initramfs),
            &["formal-sysroot", "linux"]
        );
        assert!(
            build_stage_dependencies(BuildStage::Iso)
                .iter()
                .any(|dependency| *dependency == "initramfs")
        );
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
        assert!(plan.contains(&BuildStage::Libffi));
        assert!(plan.contains(&BuildStage::Python));
        assert!(plan.contains(&BuildStage::Llvm));
        assert!(plan.contains(&BuildStage::Rust));
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
    fn binutils_git_build_pins_missing_distribution_input() {
        assert_eq!(BINUTILS_UPSTREAM_COMMIT.len(), 40);
        assert_eq!(BINUTILS_SYSROFF_SHA256.len(), 64);
        assert!(BINUTILS_UPSTREAM_MIRROR.starts_with("https://git.sr.ht/~sourceware/"));
    }

    #[test]
    fn shadow_git_build_pins_ignored_upstream_input() {
        assert_eq!(SHADOW_UPSTREAM_COMMIT.len(), 40);
        assert_eq!(SHADOW_MAN_PO_MAKEFILE_SHA256.len(), 64);
        assert_eq!(
            SHADOW_UPSTREAM_REPOSITORY,
            "https://github.com/shadow-maint/shadow.git"
        );
        let source = include_str!("stages/base_userland.rs");
        let start = source.find("fn build_shadow").unwrap();
        let end = source[start..]
            .find("fn ensure_shadow_man_po_makefile")
            .unwrap()
            + start;
        let build = &source[start..end];
        assert!(build.contains("copy_imported_working_tree"));
        assert!(build.contains("source.join(\"man/po/Makefile.in\")"));
    }

    #[test]
    fn native_compiler_configuration_is_guest_default_and_minimal() {
        let source = include_str!("stages/toolchain.rs");
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
            "cc_name.clone()",
            "cxx_name.clone()",
        ] {
            assert!(
                build.contains(required),
                "missing native GCC setting {required}"
            );
        }
        assert!(build.contains("wrapper directory is already first in PATH"));
        assert!(build.contains("checksum-options"));
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
    fn cargo_sysroot_link_argument_is_checkout_independent() {
        let source = include_str!("stages/image.rs");
        let start = source.find("fn apply_mattos_sysroot_environment").unwrap();
        let end = source[start..].find("fn run_cmd_output").unwrap() + start;
        let body = &source[start..end];
        assert!(body.contains("relative_sysroot.push(\"..\")"));
        assert!(body.contains("relative_sysroot.push(\"out/sysroot\")"));
        assert!(body.contains("--remap-path-prefix={}=/usr/src/mattos"));
        assert!(!body.contains("format!(\"-C link-arg={sysroot_flag}\")"));
    }

    #[test]
    fn gcc_runtime_configuration_is_target_only_and_sysrooted() {
        let source = include_str!("stages/toolchain.rs");
        let start = source.find("fn build_gcc_runtime").unwrap();
        let end = source[start..].find("fn build_binutils").unwrap() + start;
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
        assert_eq!(attr.revision.as_deref(), Some(ATTR_UPSTREAM_COMMIT));
        assert_eq!(ATTR_RELEASE_DIRECTORY, "attr-2.6.0");
        assert!(ATTR_RELEASE_ARCHIVE_URL.ends_with("/attr-2.6.0.tar.xz"));
        assert_eq!(ATTR_RELEASE_ARCHIVE_SHA256.len(), 64);
    }

    #[test]
    fn base_userland_release_archives_are_exact_and_output_owned() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let policy = fs::read_to_string(root.join("upstream/policies/release-archives.toml"))
            .expect("release archive policy");
        for (component, version, commit, url, sha256) in [
            (
                "gzip",
                "1.14",
                "fbc4883eb9c304a04623ac506dd5cf5450d055f1",
                GZIP_RELEASE_ARCHIVE_URL,
                GZIP_RELEASE_ARCHIVE_SHA256,
            ),
            (
                "patch",
                "2.8",
                "48ceda8200aaf30c3ce42c31cd70ff6087db2425",
                PATCH_RELEASE_ARCHIVE_URL,
                PATCH_RELEASE_ARCHIVE_SHA256,
            ),
            (
                "less",
                "704",
                "7ea9586a9a1273eb9658d76af8986fdcf6738096",
                LESS_RELEASE_ARCHIVE_URL,
                LESS_RELEASE_ARCHIVE_SHA256,
            ),
        ] {
            let state = read_sync_state(&root, component).unwrap().unwrap();
            assert_eq!(state.imported_commit, commit);
            assert!(policy.contains(&format!("component = \"{component}\"")));
            assert!(policy.contains(&format!("version = \"{version}\"")));
            assert!(policy.contains(&format!("source_commit = \"{commit}\"")));
            assert!(policy.contains(&format!("url = \"{url}\"")));
            assert!(policy.contains(&format!("sha256 = \"{sha256}\"")));
        }
        assert_eq!(
            policy
                .matches("staging_policy = \"output-mirror-only\"")
                .count(),
            6
        );
        let source = include_str!("stages/helpers/native.rs");
        let start = source.find("fn build_release_autotools_program").unwrap();
        let helper = &source[start..];
        assert!(helper.contains("out/build"));
        assert!(helper.contains("ensure_verified_release_archive"));
        assert!(helper.contains("stage_release_source"));
        assert!(!helper.contains("src/userland"));
    }

    #[test]
    fn self_hosting_toolchain_inputs_and_clang_policy_are_pinned() {
        assert_eq!(
            RUST_RELEASE_ARCHIVE_URL,
            "https://static.rust-lang.org/dist/rustc-1.97.1-src.tar.xz"
        );
        assert_eq!(
            RUST_RELEASE_ARCHIVE_SHA256,
            "0ed06fdaffd4722a7702e0b4eebfafc897ab8f513e8e1b247cdd7e5c6df6ded2"
        );
        assert_eq!(
            MATTOS_GCC_INSTALL_DIR,
            "/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0"
        );

        let source = include_str!("main.rs");
        let llvm_start = source.find("fn build_llvm").unwrap();
        let rust_start = source.find("fn build_rust").unwrap();
        let llvm = &source[llvm_start..rust_start];
        for required in [
            "-DCLANG_CONFIG_FILE_SYSTEM_DIR=/etc/clang",
            "etc/clang/clang.cfg",
            "etc/clang/clang++.cfg",
            "-isystem/usr/include/c++/15.3.0",
        ] {
            assert!(llvm.contains(required), "missing Clang policy {required}");
        }

        let rust_end = source[rust_start..].find("fn build_bzip2").unwrap() + rust_start;
        let rust = &source[rust_start..rust_end];
        for required in [
            "ensure_verified_release_archive",
            "stage_release_source",
            "download-ci-llvm = false",
            "submodules = false",
            "vendor = true",
            "locked-deps = true",
            "llvm-config",
            "llvm-filecheck",
            "jobs = {}",
            "tool-wrappers",
            "MATTOS_GCC_INSTALL_DIR",
        ] {
            assert!(
                rust.contains(required),
                "missing Rust bootstrap policy {required}"
            );
        }
        assert!(rust.contains("out/build/rust"));
        assert!(!rust.contains("src/toolchain/rust/x.py"));
    }

    #[test]
    fn llvm_config_build_roots_are_normalized_only_in_generated_output() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path().join("checkout-a");
        let build = repo.join("out/build/llvm/build");
        let generated = build.join("tools/llvm-config/BuildVariables.inc");
        fs::create_dir_all(generated.parent().unwrap()).unwrap();
        fs::write(
            &generated,
            format!(
                "#define LLVM_SRC_ROOT \"{}\"\n#define LLVM_OBJ_ROOT \"{}\"\n#define LLVM_BUILDMODE \"Release\"\n",
                repo.join("src/toolchain/llvm-project/llvm").display(),
                build.display(),
            ),
        )
        .unwrap();

        normalize_llvm_config_build_roots(&repo, &build).unwrap();
        normalize_llvm_config_build_roots(&repo, &build).unwrap();
        let normalized = fs::read_to_string(&generated).unwrap();
        assert!(normalized.contains("#define LLVM_SRC_ROOT \"/usr/src/mattos/llvm\""));
        assert!(normalized.contains("#define LLVM_OBJ_ROOT \"/usr/lib/llvm-22/build\""));
        assert!(!normalized.contains(temporary.path().to_str().unwrap()));
        assert!(normalized.contains("#define LLVM_BUILDMODE \"Release\""));

        let source = include_str!("main.rs");
        let llvm_start = source.find("fn build_llvm").unwrap();
        let rust_start = source.find("fn build_rust").unwrap();
        let llvm = &source[llvm_start..rust_start];
        assert!(llvm.contains("-DCMAKE_SUPPRESS_REGENERATION=ON"));
        assert!(llvm.contains("normalize_llvm_config_build_roots(repo_root, &build_dir)?"));
    }

    #[test]
    fn cpython_getpath_vpath_is_normalized_without_changing_make_source_search() {
        let temporary = tempfile::tempdir().unwrap();
        let build = temporary.path().join("build");
        fs::create_dir_all(&build).unwrap();
        fs::write(
            build.join("Makefile"),
            "VPATH=\t/tmp/checkout/cpython\n\t-DVPATH='\"$(VPATH)\"' \\\n+\t-o $@ $(srcdir)/Modules/getpath.c\n",
        )
        .unwrap();
        normalize_cpython_getpath_vpath(&build).unwrap();
        normalize_cpython_getpath_vpath(&build).unwrap();
        let normalized = fs::read_to_string(build.join("Makefile")).unwrap();
        assert!(normalized.contains("VPATH=\t/tmp/checkout/cpython"));
        assert!(normalized.contains("-DVPATH='\"/usr/src/mattos/cpython\"'"));
        assert!(!normalized.contains("-DVPATH='\"$(VPATH)\"'"));
        restore_cpython_getpath_vpath(&build).unwrap();
        let restored = fs::read_to_string(build.join("Makefile")).unwrap();
        assert!(restored.contains("-DVPATH='\"$(VPATH)\"'"));

        let source = include_str!("main.rs");
        let python_start = source.find("fn build_cpython").unwrap();
        let llvm_start = source.find("fn build_llvm").unwrap();
        let python = &source[python_start..llvm_start];
        let first_make = python
            .find("run_cmd_with_env_overrides(&build_dir, \"make\"")
            .unwrap();
        let normalize = python.find("normalize_cpython_getpath_vpath").unwrap();
        let remove = python[normalize..].find("Modules/getpath.o").unwrap() + normalize;
        let second_make = python[remove..]
            .find("run_cmd_with_env_overrides(&build_dir, \"make\"")
            .unwrap()
            + remove;
        assert!(first_make < normalize && normalize < remove && remove < second_make);
    }

    #[test]
    fn cpython_getpath_vpath_normalization_fails_closed() {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(temporary.path().join("Makefile"), "VPATH=/unexpected\n").unwrap();
        let error = normalize_cpython_getpath_vpath(temporary.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("lacks expected CPython getpath VPATH definition"));
    }

    #[test]
    fn llvm_config_build_root_normalization_fails_closed() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path().join("checkout-b");
        let build = repo.join("out/build/llvm/build");
        let generated = build.join("tools/llvm-config/BuildVariables.inc");
        fs::create_dir_all(generated.parent().unwrap()).unwrap();
        fs::write(
            &generated,
            "#define LLVM_SRC_ROOT \"/unexpected/source\"\n#define LLVM_OBJ_ROOT \"/unexpected/build\"\n",
        )
        .unwrap();
        let error = normalize_llvm_config_build_roots(&repo, &build)
            .unwrap_err()
            .to_string();
        assert!(error.contains("lacks expected LLVM build-root definition"));
    }

    #[test]
    fn rust_bootstrap_output_mirror_has_an_explicit_workspace_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = temp.path().join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"bootstrap\"\nversion = \"0.0.0\"\n",
        )
        .unwrap();
        isolate_standalone_cargo_manifest(&manifest).unwrap();
        isolate_standalone_cargo_manifest(&manifest).unwrap();
        let contents = fs::read_to_string(manifest).unwrap();
        assert_eq!(contents.matches("[workspace]").count(), 1);
        assert!(contents.contains("MattOS output-mirror workspace boundary"));
    }

    #[test]
    fn base_userland_stage_names_dispatch() {
        for (name, expected) in [
            ("gzip", BuildStage::Gzip),
            ("patch", BuildStage::Patch),
            ("file", BuildStage::File),
            ("less", BuildStage::Less),
            ("git", BuildStage::Git),
            ("openssh", BuildStage::Openssh),
        ] {
            assert_eq!(BuildStage::from_str(name, true).unwrap(), expected);
        }
    }

    #[test]
    fn brush_compatibility_modes_are_formal_output_only_policy() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let manifest = fs::read_to_string(root.join("upstream/patches/brush/manifest.toml"))
            .expect("Brush patch manifest");
        let patch_path =
            root.join("upstream/patches/brush/0002-select-sh-mode-from-invocation-name.patch");
        assert!(manifest.contains("application = \"output-mirror-only\""));
        assert!(manifest.contains(
            "sha256 = \"a5049e836e578d76d424b075246aafb18a3d4f2f2f08f3447d7ccb8484811a59\"",
        ));
        assert_eq!(
            performance::sha256_file(&patch_path).unwrap(),
            "a5049e836e578d76d424b075246aafb18a3d4f2f2f08f3447d7ccb8484811a59"
        );
        let patch = fs::read_to_string(patch_path).unwrap();
        assert!(patch.contains("invoked_as_sh"));
        assert!(patch.contains("name == \"sh\""));
        assert!(patch.contains("args.insert(1, \"--sh\".to_string())"));
    }

    #[test]
    fn attr_release_checksum_rejects_unverified_inputs() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = temporary.path().join("attr-2.6.0.tar.xz");
        fs::write(&archive, b"not the official Attr release archive").unwrap();
        let error = verify_attr_release_archive(&archive)
            .unwrap_err()
            .to_string();
        assert!(error.contains("checksum mismatch"));
    }

    #[test]
    fn staged_attr_bootstrap_inputs_supply_configure_and_visibility_macro() {
        let temporary = tempfile::tempdir().unwrap();
        let release = temporary.path().join(ATTR_RELEASE_DIRECTORY);
        fs::create_dir_all(release.join("m4")).unwrap();
        fs::create_dir_all(release.join("build-aux")).unwrap();
        fs::write(release.join("configure"), "#!/bin/sh\n").unwrap();
        fs::write(release.join("aclocal.m4"), "dnl generated\n").unwrap();
        fs::write(release.join("Makefile.in"), "all:\n\t@true\n").unwrap();
        fs::write(
            release.join("m4/visibility_hidden.m4"),
            "AC_DEFUN([AC_FUNC_GCC_VISIBILITY], [:])\n",
        )
        .unwrap();
        fs::write(release.join("build-aux/config.rpath"), "# generated\n").unwrap();
        let archive = temporary.path().join("attr-2.6.0.tar.xz");
        let parent = temporary.path();
        run_cmd(
            parent,
            "tar",
            &[
                "-cJf",
                path_str(&archive).unwrap(),
                "-C",
                path_str(parent).unwrap(),
                ATTR_RELEASE_DIRECTORY,
            ],
        )
        .unwrap();
        let mirror = temporary.path().join("mirror");
        fs::create_dir_all(&mirror).unwrap();
        stage_attr_bootstrap_inputs(&temporary.path().join("authoritative"), &mirror, &archive)
            .unwrap();
        assert!(mirror.join("configure").is_file());
        assert!(mirror.join("aclocal.m4").is_file());
        assert!(mirror.join("Makefile.in").is_file());
        assert!(
            fs::read_to_string(mirror.join("m4/visibility_hidden.m4"))
                .unwrap()
                .contains("AC_FUNC_GCC_VISIBILITY")
        );
    }

    #[test]
    fn attr_uses_release_generated_files_without_host_versioned_aclocal() {
        let builder = include_str!("main.rs");
        let start = builder.find("fn build_attr").unwrap();
        let end = builder[start..]
            .find("fn ensure_attr_release_archive")
            .unwrap()
            + start;
        let attr_build = &builder[start..end];
        assert!(attr_build.contains("stage_attr_bootstrap_inputs"));
        assert!(attr_build.contains("MAKE_MAINTAINER_MODE="));
        assert!(attr_build.contains("touch",));
        assert!(!attr_build.contains("./autogen.sh"));
    }

    #[test]
    fn acl_release_bootstrap_is_pinned_and_output_owned() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let sources = read_sources(&root).unwrap();
        let acl = sources
            .component
            .iter()
            .find(|item| item.name == "acl")
            .unwrap();
        assert_eq!(acl.branch, "v2.3.2");
        let state = read_sync_state(&root, "acl").unwrap().unwrap();
        assert_eq!(state.schema_version, 2);
        assert_eq!(
            state.imported_commit,
            "214c7d146945c31a9dc04cb7094b85053f52a21e"
        );
        assert_eq!(
            state.upstream_tree,
            "0fc760b8b9935266e0e496b17effa771e9c57b42"
        );
        assert_eq!(state.imported_tree_digest.len(), 64);
        assert_eq!(state.patch_manifest, "none");
        assert!(ACL_RELEASE_ARCHIVE_URL.ends_with("/acl-2.3.2.tar.xz"));
        assert_eq!(ACL_RELEASE_ARCHIVE_SHA256.len(), 64);
        let builder = include_str!("main.rs");
        let start = builder.find("fn build_acl").unwrap();
        let end = builder[start..]
            .find("fn ensure_acl_release_archive")
            .unwrap()
            + start;
        let acl_build = &builder[start..end];
        assert!(acl_build.contains("stage_acl_bootstrap_inputs"));
        assert!(acl_build.contains("MAKE_MAINTAINER_MODE="));
        assert!(acl_build.contains("\"-d\", \"@0\""));
        assert!(!acl_build.contains("./autogen.sh"));
    }

    #[test]
    fn bzip2_shared_library_build_is_path_independent_and_debug_free() {
        let builder = include_str!("main.rs");
        let start = builder.find("fn build_bzip2").unwrap();
        let end = builder[start..].find("fn build_lz4").unwrap() + start;
        let bzip2_build = &builder[start..end];

        // bzip2's Makefile otherwise inherits the host CFLAGS, including a
        // possible -g.  Rebuilding forces stale objects out while the maps
        // make every output-owned source mirror look identical to the linker.
        assert!(
            bzip2_build
                .contains("\"-B\",\n            \"-f\",\n            \"Makefile-libbz2_so\"")
        );
        assert!(bzip2_build.contains("let cflags_override = format!(\"CFLAGS={cflags}\")"));
        assert!(bzip2_build.contains("-O2 -g0 -fPIC"));
        assert!(bzip2_build.contains("-ffile-prefix-map={}=/usr/src/mattos/bzip2"));
        assert!(bzip2_build.contains("-fdebug-prefix-map={}=/usr/src/mattos/bzip2"));
        assert!(bzip2_build.contains("-fmacro-prefix-map={}=/usr/src/mattos/bzip2"));
        assert!(bzip2_build.contains("SOURCE_DATE_EPOCH"));
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
        assert!(options.iter().any(|option| option == "-Dblkid=enabled"));
        assert!(!options.iter().any(|option| option == "-Dpam=disabled"));
        assert!(!options.iter().any(|option| option == "-Dblkid=disabled"));
        assert_eq!(
            SYSTEMD_PAM_MODULE_REL,
            "usr/lib/x86_64-linux-gnu/security/pam_systemd.so"
        );
    }

    #[test]
    fn installed_udev_rules_require_blkid_backed_stable_disk_identities() {
        let rootfs = tempfile::tempdir().expect("rootfs");
        let rules = rootfs
            .path()
            .join("usr/lib/udev/rules.d/60-persistent-storage.rules");
        write(
            &rules,
            "IMPORT{builtin}=\"blkid\"\nSYMLINK+=\"disk/by-uuid/$env{ID_FS_UUID_ENC}\"\nSYMLINK+=\"disk/by-partuuid/$env{ID_PART_ENTRY_UUID}\"\n",
        );
        write(
            &rootfs
                .path()
                .join("etc/profile.d/80-systemd-osc-context.sh"),
            "command -v shopt >/dev/null 2>&1 || return 0\nPROMPT_COMMAND=__systemd_osc_context_precmdline\n",
        );
        validate_udev_storage_identity_support(rootfs.path()).expect("complete storage rules");

        write(
            &rules,
            "SYMLINK+=\"disk/by-partuuid/$env{ID_PART_ENTRY_UUID}\"\n",
        );
        let error = validate_udev_storage_identity_support(rootfs.path())
            .expect_err("rules without blkid probing must fail")
            .to_string();
        assert!(error.contains("IMPORT{builtin}=\"blkid\""));
    }

    #[test]
    fn systemd_osc_profile_patch_is_parseable_by_posix_login_shells() {
        let install = tempfile::tempdir().expect("install");
        let profile = install
            .path()
            .join("etc/profile.d/80-systemd-osc-context.sh");
        write(
            &profile,
            "# Not bash?\n[ -n \"${BASH_VERSION:-}\" ] || return 0\nif [ -n \"${BASH_VERSION:-}\" ]; then\n    [ -n \"$(declare -p PROMPT_COMMAND 2>/dev/null)\" ] || PROMPT_COMMAND+=('')\n\n    # Whenever a new prompt is shown, close the previous command, and prepare new command\n    PROMPT_COMMAND+=(__systemd_osc_context_precmdline)\nfi\n",
        );
        patch_systemd_osc_profile_for_posix_login_shell(install.path()).expect("patch profile");
        let body = fs::read_to_string(profile).expect("profile");
        assert!(!body.contains("PROMPT_COMMAND+=("));
        assert!(body.contains("command -v shopt >/dev/null 2>&1 || return 0"));
        assert!(
            body.contains("PROMPT_COMMAND=\"__systemd_osc_context_precmdline;${PROMPT_COMMAND}\"")
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
            (
                "sshd",
                "auth       required     pam_unix.so\nsession    required     pam_unix.so\nsession    optional     pam_systemd.so\n",
            ),
        ] {
            write(&rootfs.join("etc/pam.d").join(stack), body);
        }
        write(
            &rootfs.join("usr/share/pam/security/pam_env.conf"),
            "# MattOS source-built PAM environment defaults.\n",
        );
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
            "usr/bin/fusermount3",
            "usr/lib/polkit-1/polkit-agent-helper-1",
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
        let fusermount_mode = fs::metadata(root.join("usr/bin/fusermount3"))
            .expect("fusermount3 metadata")
            .permissions()
            .mode()
            & 0o7777;
        assert_eq!(fusermount_mode, 0o4755);

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
            "usr/bin/fusermount3",
            "usr/lib/polkit-1/polkit-agent-helper-1",
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
    fn xz_initramfs_validation_checks_magic_and_early_size_ceiling() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("archive.xz");
        let magic = [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00];
        fs::write(&archive, magic).unwrap();
        assert!(has_xz_header(&archive).unwrap());
        fs::write(&archive, &magic[..5]).unwrap();
        assert!(!has_xz_header(&archive).unwrap());
        fs::write(&archive, [0x00; 6]).unwrap();
        assert!(!has_xz_header(&archive).unwrap());
        fs::write(&archive, magic).unwrap();
        let oversized = fs::OpenOptions::new().write(true).open(&archive).unwrap();
        oversized.set_len(EARLY_INITRAMFS_SIZE_LIMIT + 1).unwrap();
        drop(oversized);
        assert!(validate_early_initramfs(&archive).is_err());
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
        fs::create_dir_all(root.join("out/build/grep/cargo-target/release")).expect("mkdir");

        let spec = BinaryInstallSpec {
            provider: GREP_PROVIDER,
            source_rel: "out/build/grep/cargo-target/release/grep",
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
        let plan = build_plan(BuildStage::All);
        let systemd = plan
            .iter()
            .position(|stage| *stage == BuildStage::Systemd)
            .unwrap();
        let broker = plan
            .iter()
            .position(|stage| *stage == BuildStage::DbusBroker)
            .unwrap();
        assert!(systemd < broker);
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
            "systemd-localed.service",
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
            "locale1",
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
        inspect_and_stage_executable(
            Path::new("/bin/true"),
            &rootfs.join("usr/lib/systemd/systemd-localed"),
            &rootfs,
            &roots,
            &libraries,
        )
        .expect("stage test localed ELF and dependency closure");
        write(&rootfs.join("usr/bin/busctl"), "present\n");
        write(&rootfs.join("usr/bin/localectl"), "present\n");
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
    fn dbus_validation_rejects_competing_service_owner_and_stale_socket() {
        let (_tmp, repo, rootfs) = make_dbus_test_trees();
        install_dbus_configuration(&repo, &rootfs).expect("install D-Bus integration");
        let broker_unit = rootfs.join("usr/lib/systemd/system/dbus-broker.service");
        let original_broker_unit = fs::read_to_string(&broker_unit).unwrap();
        write(
            &broker_unit,
            "[Service]\nExecStart=/usr/bin/dbus-daemon --system\n",
        );
        assert!(
            validate_dbus_configuration(&rootfs)
                .expect_err("competing owner must fail")
                .to_string()
                .contains("exactly one system-bus implementation")
        );
        write(&broker_unit, &original_broker_unit);
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
                        || (binary.command_name == "lessecho"
                            && binary.destination_rel == "usr/libexec/lessecho")
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
        fs::create_dir_all(rootfs.join("etc/systemd/system")).expect("systemd unit dir");
        symlink(
            "/dev/null",
            rootfs.join("etc/systemd/system/systemd-networkd.service"),
        )
        .expect("networkd mask");
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
            "usr/sbin/NetworkManager",
            "usr/bin/nmcli",
            "usr/lib/systemd/system/NetworkManager.service",
            "usr/lib/systemd/system/NetworkManager-wait-online.service",
            "usr/lib/systemd/systemd-resolved",
            "usr/lib/systemd/systemd-timesyncd",
            "usr/lib/x86_64-linux-gnu/libnss_resolve.so.2",
            "etc/systemd/system/multi-user.target.wants/NetworkManager.service",
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
