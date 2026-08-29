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

    if !missing_required.contains(&"pkg-config") {
        for (args, package) in [
            (&["--exists", "mount"][..], "libmount-dev"),
            (&["--exists", "openssl"][..], "libssl-dev"),
            (&["--atleast-version=2.2", "expat"][..], "libexpat1-dev"),
            (&["--exists", "zlib"][..], "zlib1g-dev"),
            (&["--exists", "liblzma"][..], "liblzma-dev"),
            (&["--exists", "libzstd"][..], "libzstd-dev"),
            (&["--exists", "liblz4"][..], "liblz4-dev"),
            (&["--exists", "libxxhash"][..], "libxxhash-dev"),
        ] {
            if let Some(message) = check_tool_runtime("pkg-config", args)? {
                println!("[broken]  {package} ({message})");
                broken_required.push(package);
            }
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
    build_live_root(repo_root)?;
    build_initramfs(repo_root)?;
    build_iso(repo_root)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArtifactRecord {
    role: &'static str,
    path: String,
    bytes: u64,
    sha256: String,
    detail: &'static str,
}

fn reject_obsolete_full_root_initramfs(repo_root: &Path) -> Result<()> {
    let stale = OBSOLETE_FULL_ROOT_INITRAMFS_PATHS
        .iter()
        .filter(|path| repo_root.join(path).exists())
        .copied()
        .collect::<Vec<_>>();
    if !stale.is_empty() {
        bail!(
            "obsolete full-root initramfs artifact(s) coexist with the live-root architecture: {}; remove these generated outputs and use {} plus {}",
            stale.join(", "),
            INITRAMFS_ARCHIVE_PATH,
            LIVE_ROOT_IMAGE_PATH,
        );
    }
    Ok(())
}

fn artifact_record(
    repo_root: &Path,
    role: &'static str,
    relative: &str,
    detail: &'static str,
) -> Result<ArtifactRecord> {
    let path = repo_root.join(relative);
    let metadata =
        fs::metadata(&path).with_context(|| format!("{role} is missing at {}", path.display()))?;
    if !metadata.is_file() {
        bail!("{role} is not a regular file: {}", path.display());
    }
    Ok(ArtifactRecord {
        role,
        path: relative.to_string(),
        bytes: metadata.len(),
        sha256: performance::sha256_file(&path)?,
        detail,
    })
}

fn extract_efi_image_record(repo_root: &Path) -> Result<ArtifactRecord> {
    let iso = repo_root.join(FINAL_ISO_PATH);
    let temporary = repo_root
        .join("out/tmp")
        .join(format!("artifact-report-efi-{}.img", std::process::id()));
    if let Some(parent) = temporary.parent() {
        fs::create_dir_all(parent)?;
    }
    remove_path_if_exists(&temporary)?;
    let result = run_cmd(
        repo_root,
        "xorriso",
        &[
            "-osirrox",
            "on",
            "-indev",
            path_str(&iso)?,
            "-extract",
            "/efi.img",
            path_str(&temporary)?,
        ],
    );
    if let Err(error) = result {
        let _ = remove_path_if_exists(&temporary);
        return Err(error).context("failed to extract the UEFI image from the final ISO");
    }
    let record = ArtifactRecord {
        role: "UEFI ISO boot image",
        path: format!("{FINAL_ISO_PATH}:/efi.img"),
        bytes: fs::metadata(&temporary)?.len(),
        sha256: performance::sha256_file(&temporary)?,
        detail: "FAT image inside ISO",
    };
    remove_path_if_exists(&temporary)?;
    Ok(record)
}

fn collect_artifact_records(repo_root: &Path) -> Result<Vec<ArtifactRecord>> {
    reject_obsolete_full_root_initramfs(repo_root)?;
    validate_early_initramfs(&repo_root.join(INITRAMFS_ARCHIVE_PATH))?;
    validate_squashfs_image(&repo_root.join(LIVE_ROOT_IMAGE_PATH))?;
    validate_staged_grub_config(&repo_root.join("out/build/iso/boot/grub/grub.cfg"))?;

    let expanded = Command::new("xz")
        .args(["-dc"])
        .arg(repo_root.join(INITRAMFS_ARCHIVE_PATH))
        .output()
        .context("failed to expand the live early initramfs for reporting")?;
    if !expanded.status.success() {
        bail!("xz rejected the live early initramfs");
    }

    let mut records = vec![
        artifact_record(
            repo_root,
            "Kernel",
            "out/build/linux/build/arch/x86/boot/bzImage",
            "Linux bzImage",
        )?,
        artifact_record(
            repo_root,
            "Live early initramfs",
            INITRAMFS_ARCHIVE_PATH,
            "XZ newc; minimal /init only",
        )?,
        ArtifactRecord {
            role: "Live early initramfs (uncompressed)",
            path: INITRAMFS_ARCHIVE_PATH.to_string(),
            bytes: expanded.stdout.len() as u64,
            sha256: format!("{:x}", Sha256Hasher::digest(&expanded.stdout)),
            detail: "uncompressed newc stream",
        },
        artifact_record(
            repo_root,
            "Live root SquashFS",
            LIVE_ROOT_IMAGE_PATH,
            "read-only live root",
        )?,
        artifact_record(
            repo_root,
            "Installed initramfs",
            INSTALLED_INITRAMFS_PATH,
            "XZ newc for installed Btrfs root",
        )?,
    ];
    records.push(extract_efi_image_record(repo_root)?);
    records.push(artifact_record(
        repo_root,
        "Final ISO",
        FINAL_ISO_PATH,
        "hybrid BIOS/UEFI ISO",
    )?);
    Ok(records)
}

fn report_artifacts(repo_root: &Path) -> Result<()> {
    let records = collect_artifact_records(repo_root)?;
    let mut report = String::from("role\tpath\tbytes\tsha256\tdetail\n");
    for record in &records {
        report.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            record.role, record.path, record.bytes, record.sha256, record.detail
        ));
        println!(
            "{:<38} {:>12} bytes  {}  {}",
            format!("{}:", record.role),
            record.bytes,
            record.sha256,
            record.path
        );
    }
    let destination = repo_root.join(ARTIFACT_REPORT_PATH);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&destination, report)?;
    println!("Artifact report: {ARTIFACT_REPORT_PATH}");
    Ok(())
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
            "set -euo pipefail; cd {0}; if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then echo 'qemu-system-x86_64 missing in WSL'; exit 22; fi; mkdir -p out/logs; rm -f out/logs/qemu-boot-test.log; (sleep 8; printf 'echo __MATTOS_START__\npwd\nls /\necho MARK_MATTOS\nuname -s\ncat /proc/version\nmkdir -p /tmp/test\ntouch /tmp/test/file\nls /tmp/test\necho __MATTOS_BOOT_OK__\n'; sleep 2) | timeout 180s qemu-system-x86_64 -m 1024 -drive file=out/images/mattos-x86_64.iso,if=none,id=mattos-cd,media=cdrom,readonly=on -device virtio-scsi-pci,id=mattos-scsi -device scsi-cd,drive=mattos-cd,bus=mattos-scsi.0,bootindex=1 -nographic -serial stdio -monitor none -no-reboot -no-shutdown >out/logs/qemu-boot-test.log 2>&1 || true; grep -q '^__MATTOS_START__$' out/logs/qemu-boot-test.log; grep -q '^MARK_MATTOS$' out/logs/qemu-boot-test.log; grep -q '^Linux$' out/logs/qemu-boot-test.log; grep -q '^file$' out/logs/qemu-boot-test.log; grep -q '^__MATTOS_BOOT_OK__$' out/logs/qemu-boot-test.log",
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
    #[cfg(windows)]
    {
        return Command::new("where")
            .arg(cmd)
            .status()
            .with_context(|| format!("failed to probe tool {cmd}"))
            .map(|status| status.success());
    }

    #[cfg(not(windows))]
    {
        let Some(path) = std::env::var_os("PATH") else {
            return Ok(false);
        };
        Ok(command_exists_in_path(cmd, &path))
    }
}

#[cfg(not(windows))]
fn command_exists_in_path(cmd: &str, path: &OsStr) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(path).any(|directory| {
        let candidate = directory.join(cmd);
        candidate
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    })
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
    let (source_selection, source_selection_policy, source_selection_policy_sha256) =
        load_source_selection_policy(repo_root, comp)?;
    let (upstream_tree, imported_tree_digest) =
        imported_tree_identity(&tmp, source_selection.as_ref())?;
    let (
        intentional_omission_policy,
        gitlink_policy,
        patch_manifest,
        patch_manifest_sha256,
        lfs_policy_name,
        lfs_policy_sha256,
    ) = component_provenance_policy(repo_root, &comp.name)?;
    let lfs_policy =
        load_lfs_hydration_policy(repo_root, comp, &lfs_policy_name, &lfs_policy_sha256)?;

    clear_directory_contents(destination)?;
    materialize_git_tree_exact(&tmp, "HEAD", destination, source_selection.as_ref())?;
    apply_source_selection(destination, source_selection.as_ref())?;
    hydrate_lfs_objects(repo_root, comp, destination, lfs_policy.as_ref())?;

    let state = SyncState {
        schema_version: 2,
        component: comp.name.clone(),
        repo: comp.repo.clone(),
        branch: comp.branch.clone(),
        imported_commit: commit.trim().to_owned(),
        imported_at_utc: Utc::now().to_rfc3339(),
        sync_method: comp.sync.clone(),
        destination_path: comp.path.clone(),
        upstream_tree,
        imported_tree_digest_algorithm: if source_selection.is_some() {
            SELECTED_IMPORTED_TREE_DIGEST_ALGORITHM
        } else {
            IMPORTED_TREE_DIGEST_ALGORITHM
        }
        .to_string(),
        imported_tree_digest,
        source_selection_policy,
        source_selection_policy_sha256,
        intentional_omission_policy,
        gitlink_policy,
        patch_manifest,
        patch_manifest_sha256,
        lfs_policy: lfs_policy_name,
        lfs_policy_sha256,
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
    // The three-way sync below needs the prior imported commit as well as the
    // new branch head. Hydrate a genuinely shallow clone before constructing
    // the merge. Local fixture repositories ignore --depth during clone, so
    // asking Git to unshallow them is an error rather than a harmless no-op.
    let shallow = run_cmd_capture(
        &tmp_upstream,
        "git",
        &["rev-parse", "--is-shallow-repository"],
    )?;
    if shallow.trim() == "true" {
        run_cmd(&tmp_upstream, "git", &["fetch", "--unshallow", "origin"])?;
    }
    let new_commit = run_cmd_capture(&tmp_upstream, "git", &["rev-parse", "HEAD"])?;
    let (source_selection, source_selection_policy, source_selection_policy_sha256) =
        load_source_selection_policy(repo_root, comp)?;
    let (upstream_tree, imported_tree_digest) =
        imported_tree_identity(&tmp_upstream, source_selection.as_ref())?;
    let (
        intentional_omission_policy,
        gitlink_policy,
        patch_manifest,
        patch_manifest_sha256,
        lfs_policy_name,
        lfs_policy_sha256,
    ) = component_provenance_policy(repo_root, &comp.name)?;
    let lfs_policy =
        load_lfs_hydration_policy(repo_root, comp, &lfs_policy_name, &lfs_policy_sha256)?;

    let old_commit = prior_state.imported_commit.trim();
    if new_commit.trim() == old_commit {
        clear_directory_contents(destination)?;
        materialize_git_tree_exact(
            &tmp_upstream,
            "HEAD",
            destination,
            source_selection.as_ref(),
        )?;
        apply_source_selection(destination, source_selection.as_ref())?;
        hydrate_lfs_objects(repo_root, comp, destination, lfs_policy.as_ref())?;
        let state = SyncState {
            schema_version: 2,
            component: comp.name.clone(),
            repo: comp.repo.clone(),
            branch: comp.branch.clone(),
            imported_commit: new_commit.trim().to_owned(),
            imported_at_utc: Utc::now().to_rfc3339(),
            sync_method: comp.sync.clone(),
            destination_path: comp.path.clone(),
            upstream_tree,
            imported_tree_digest_algorithm: if source_selection.is_some() {
                SELECTED_IMPORTED_TREE_DIGEST_ALGORITHM
            } else {
                IMPORTED_TREE_DIGEST_ALGORITHM
            }
            .to_string(),
            imported_tree_digest,
            source_selection_policy,
            source_selection_policy_sha256,
            intentional_omission_policy,
            gitlink_policy,
            patch_manifest,
            patch_manifest_sha256,
            lfs_policy: lfs_policy_name,
            lfs_policy_sha256,
        };
        write_sync_state(repo_root, &comp.name, &state)?;
        fs::remove_dir_all(&tmp_upstream)
            .with_context(|| format!("failed to remove {}", tmp_upstream.display()))?;
        println!(
            "Synchronized {} at unchanged commit {}",
            comp.name, state.imported_commit
        );
        return Ok(());
    }
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
    restore_lfs_pointers_for_merge(&tmp_merge, lfs_policy.as_ref())?;
    run_cmd(&tmp_merge, "git", &["add", "-A"])?;
    let local_status = run_cmd_capture(&tmp_merge, "git", &["status", "--porcelain"])?;
    if !local_status.is_empty() {
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
    }

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
    if has_conflicts {
        copy_tree_excluding_dotgit(&tmp_merge, destination)?;
    } else {
        let merged_tree = run_cmd_capture(&tmp_merge, "git", &["write-tree"])?;
        materialize_git_tree_exact(
            &tmp_merge,
            merged_tree.trim(),
            destination,
            source_selection.as_ref(),
        )?;
    }
    apply_source_selection(destination, source_selection.as_ref())?;
    if !has_conflicts {
        hydrate_lfs_objects(repo_root, comp, destination, lfs_policy.as_ref())?;
    }

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
        schema_version: 2,
        component: comp.name.clone(),
        repo: comp.repo.clone(),
        branch: comp.branch.clone(),
        imported_commit: new_commit.trim().to_owned(),
        imported_at_utc: Utc::now().to_rfc3339(),
        sync_method: comp.sync.clone(),
        destination_path: comp.path.clone(),
        upstream_tree,
        imported_tree_digest_algorithm: if source_selection.is_some() {
            SELECTED_IMPORTED_TREE_DIGEST_ALGORITHM
        } else {
            IMPORTED_TREE_DIGEST_ALGORITHM
        }
        .to_string(),
        imported_tree_digest,
        source_selection_policy,
        source_selection_policy_sha256,
        intentional_omission_policy,
        gitlink_policy,
        patch_manifest,
        patch_manifest_sha256,
        lfs_policy: lfs_policy_name,
        lfs_policy_sha256,
    };
    write_sync_state(repo_root, &comp.name, &state)?;

    println!("Updated {} to commit {}", comp.name, state.imported_commit);
    Ok(())
}

/// Returns the immutable upstream Git tree object and a SHA-256 over the
/// canonical recursive `git ls-tree` records that have physical vendored-tree
/// representations. Gitlinks are excluded from the imported-tree digest and
/// are instead required to have an explicit replacement/exclusion policy.
fn imported_tree_identity(
    source_git: &Path,
    source_selection: Option<&SourceSelectionPolicy>,
) -> Result<(String, String)> {
    let upstream_tree = run_cmd_capture(source_git, "git", &["rev-parse", "HEAD^{tree}"])?
        .trim()
        .to_string();
    let output = run_cmd_output(source_git, "git", &["ls-tree", "-rz", "HEAD"])?;
    if !output.status.success() {
        bail!(
            "failed to enumerate imported upstream tree in {}",
            source_git.display()
        );
    }
    let mut digest = Sha256Hasher::new();
    for record in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        if record.starts_with(b"160000 ") {
            continue;
        }
        let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
            bail!("malformed git ls-tree record in {}", source_git.display());
        };
        let path = String::from_utf8_lossy(&record[tab + 1..]);
        if source_selection.is_some_and(|policy| !policy.retains(&path)) {
            continue;
        }
        digest.update(record);
        digest.update([0]);
    }
    Ok((upstream_tree, format!("{:x}", digest.finalize())))
}

fn load_source_selection_policy(
    repo_root: &Path,
    comp: &ComponentDef,
) -> Result<(Option<SourceSelectionPolicy>, String, String)> {
    let (policy_name, expected_sha256) =
        component_source_selection_metadata(repo_root, &comp.name)?;
    if policy_name == "none" {
        if expected_sha256 != "none" {
            bail!(
                "{} records a source-selection digest without a policy",
                comp.name
            );
        }
        return Ok((None, policy_name, expected_sha256));
    }
    let policy_path = resolve_component_destination(repo_root, &policy_name)?;
    let payload = fs::read(&policy_path).with_context(|| {
        format!(
            "failed to read source-selection policy: {}",
            policy_path.display()
        )
    })?;
    let actual_sha256 = format!("{:x}", Sha256Hasher::digest(&payload));
    if actual_sha256 != expected_sha256 {
        bail!(
            "{} source-selection policy checksum mismatch: expected {}, got {}",
            comp.name,
            expected_sha256,
            actual_sha256
        );
    }
    let policy: SourceSelectionPolicy = toml::from_str(
        std::str::from_utf8(&payload).context("source-selection policy is not UTF-8")?,
    )
    .with_context(|| format!("failed to parse {}", policy_path.display()))?;
    if policy.schema_version != 1
        || policy.component != comp.name
        || policy.scope != "arch"
        || comp.revision.as_deref() != Some(policy.upstream_commit.as_str())
    {
        bail!(
            "{} source-selection policy metadata does not match sources.toml",
            comp.name
        );
    }
    Ok((Some(policy), policy_name, expected_sha256))
}

fn component_source_selection_metadata(
    repo_root: &Path,
    component_name: &str,
) -> Result<(String, String)> {
    let source_path = repo_root.join("upstream/sources.toml");
    if !source_path.is_file() {
        return Ok(("none".to_string(), "none".to_string()));
    }
    let source_text = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let document: toml::Value = toml::from_str(&source_text)
        .with_context(|| format!("failed to parse {}", source_path.display()))?;
    let component = document
        .get("component")
        .and_then(toml::Value::as_array)
        .and_then(|components| {
            components.iter().find(|value| {
                value.get("name").and_then(toml::Value::as_str) == Some(component_name)
            })
        });
    let field = |name: &str| {
        component
            .and_then(|value| value.get(name))
            .and_then(toml::Value::as_str)
            .unwrap_or("none")
            .to_string()
    };
    Ok((
        field("source_selection_policy"),
        field("source_selection_policy_sha256"),
    ))
}

fn apply_source_selection(
    destination: &Path,
    source_selection: Option<&SourceSelectionPolicy>,
) -> Result<()> {
    let Some(policy) = source_selection else {
        return Ok(());
    };
    let arch_root = destination.join("arch");
    if !arch_root.is_dir() {
        bail!("source-selection policy requires {}", arch_root.display());
    }
    prune_source_selection_tree(destination, &arch_root, policy)?;
    Ok(())
}

fn prune_source_selection_tree(
    destination: &Path,
    directory: &Path,
    policy: &SourceSelectionPolicy,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            prune_source_selection_tree(destination, &path, policy)?;
            if fs::read_dir(&path)?.next().is_none() {
                fs::remove_dir(&path)?;
            }
            continue;
        }
        let relative = path
            .strip_prefix(destination)
            .expect("source-selection path is inside destination")
            .to_string_lossy();
        if !policy.retains(&relative) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn component_provenance_policy(
    repo_root: &Path,
    component_name: &str,
) -> Result<(String, String, String, String, String, String)> {
    let source_path = repo_root.join("upstream/sources.toml");
    if !source_path.is_file() {
        return Ok((
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
        ));
    }
    let source_text = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let document: toml::Value = toml::from_str(&source_text)
        .with_context(|| format!("failed to parse {}", source_path.display()))?;
    let components = document
        .get("component")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| anyhow!("sources metadata has no component array"))?;
    let Some(component) = components
        .iter()
        .find(|value| value.get("name").and_then(toml::Value::as_str) == Some(component_name))
    else {
        return Ok((
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
        ));
    };
    let field = |name: &str| {
        component
            .get(name)
            .and_then(toml::Value::as_str)
            .unwrap_or("none")
            .to_string()
    };
    Ok((
        field("intentional_omission_policy"),
        field("gitlink_policy"),
        field("patch_manifest"),
        field("patch_manifest_sha256"),
        field("lfs_policy"),
        field("lfs_policy_sha256"),
    ))
}

fn load_lfs_hydration_policy(
    repo_root: &Path,
    comp: &ComponentDef,
    policy_name: &str,
    expected_policy_sha256: &str,
) -> Result<Option<LfsHydrationPolicy>> {
    if policy_name == "none" {
        if expected_policy_sha256 != "none" {
            bail!("{} has an LFS policy checksum but no policy", comp.name);
        }
        return Ok(None);
    }
    if expected_policy_sha256 == "none" {
        bail!("{} LFS policy has no pinned SHA-256", comp.name);
    }
    let path = resolve_component_destination(repo_root, policy_name)?;
    let actual_policy_sha256 = performance::sha256_file(&path)?;
    if actual_policy_sha256 != expected_policy_sha256 {
        bail!(
            "{} LFS policy checksum mismatch: expected {}, got {}",
            comp.name,
            expected_policy_sha256,
            actual_policy_sha256
        );
    }
    let policy: LfsHydrationPolicy = toml::from_str(
        &fs::read_to_string(&path)
            .with_context(|| format!("failed to read LFS policy {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse LFS policy {}", path.display()))?;
    let revision = comp
        .revision
        .as_deref()
        .ok_or_else(|| anyhow!("{} LFS hydration requires an exact revision", comp.name))?;
    if policy.schema_version != 1
        || policy.component != comp.name
        || policy.upstream_commit != revision
        || !policy.source.contains("{path}")
    {
        bail!(
            "{} LFS policy identity is inconsistent with sources.toml",
            comp.name
        );
    }
    let mut paths = BTreeSet::new();
    for object in &policy.object {
        resolve_component_destination(Path::new("/"), &object.path)
            .with_context(|| format!("invalid LFS object path: {}", object.path))?;
        if !paths.insert(&object.path) {
            bail!("duplicate LFS object path: {}", object.path);
        }
        if object.sha256.len() != 64 || !object.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("invalid LFS SHA-256 for {}", object.path);
        }
    }
    Ok(Some(policy))
}

fn lfs_pointer(object: &LfsHydrationObject) -> String {
    format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize {}\n",
        object.sha256, object.size
    )
}

fn restore_lfs_pointers_for_merge(
    merge_tree: &Path,
    policy: Option<&LfsHydrationPolicy>,
) -> Result<()> {
    let Some(policy) = policy else {
        return Ok(());
    };
    for object in &policy.object {
        let object_name = format!("HEAD:{}", object.path);
        if run_cmd_status(merge_tree, "git", &["cat-file", "-e", &object_name])?.success() {
            run_cmd(merge_tree, "git", &["checkout", "HEAD", "--", &object.path])?;
        } else {
            remove_path_if_exists(&merge_tree.join(&object.path))?;
        }
    }
    Ok(())
}

fn hydrate_lfs_objects(
    repo_root: &Path,
    comp: &ComponentDef,
    destination: &Path,
    policy: Option<&LfsHydrationPolicy>,
) -> Result<()> {
    let Some(policy) = policy else {
        return Ok(());
    };
    let cache = repo_root
        .join("upstream/.tmp")
        .join(format!("{}-lfs", comp.name));
    fs::create_dir_all(&cache)?;
    for object in &policy.object {
        let target = destination.join(&object.path);
        let pointer = fs::read_to_string(&target)
            .with_context(|| format!("missing upstream LFS pointer {}", target.display()))?;
        if pointer != lfs_pointer(object) {
            bail!("upstream LFS pointer metadata mismatch for {}", object.path);
        }
        let payload = cache.join(&object.sha256);
        let valid_cached = payload.is_file()
            && payload.metadata()?.len() == object.size
            && performance::sha256_file(&payload)? == object.sha256;
        if !valid_cached {
            let temporary = cache.join(format!("{}.partial", object.sha256));
            remove_path_if_exists(&temporary)?;
            let url = policy.source.replace("{path}", &object.path);
            run_cmd(
                repo_root,
                "curl",
                &[
                    "--fail",
                    "--location",
                    "--silent",
                    "--show-error",
                    "--output",
                    temporary
                        .to_str()
                        .ok_or_else(|| anyhow!("invalid LFS cache path"))?,
                    &url,
                ],
            )?;
            if temporary.metadata()?.len() != object.size
                || performance::sha256_file(&temporary)? != object.sha256
            {
                bail!(
                    "downloaded LFS payload failed verification for {}",
                    object.path
                );
            }
            fs::rename(&temporary, &payload)?;
        }
        fs::copy(&payload, &target)?;
        set_mode(target, 0o644)?;
    }
    fs::remove_dir_all(&cache)?;
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
            "-c",
            "core.autocrlf=false",
            "--no-checkout",
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
    } else {
        let remote_branch = format!("origin/{}", comp.branch);
        run_cmd(&tmp, "git", &["checkout", "--detach", &remote_branch])?;
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

/// Materializes Git blob bytes and modes directly, bypassing checkout-time
/// attributes such as `eol=crlf`, host clean/smudge filters, and autocrlf.
/// Authoritative imported trees must represent the pinned Git tree itself,
/// not a host-specific working-tree projection of it.
fn materialize_git_tree_exact(
    source_git: &Path,
    treeish: &str,
    destination: &Path,
    source_selection: Option<&SourceSelectionPolicy>,
) -> Result<()> {
    fs::create_dir_all(destination)?;
    let tree = run_cmd_output(source_git, "git", &["ls-tree", "-rz", treeish])?;
    if !tree.status.success() {
        bail!(
            "failed to enumerate Git tree {treeish} in {}",
            source_git.display()
        );
    }

    let mut objects = Vec::new();
    for record in tree
        .stdout
        .split(|byte| *byte == 0)
        .filter(|r| !r.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| anyhow!("malformed git ls-tree record"))?;
        let header = std::str::from_utf8(&record[..tab]).context("non-UTF-8 tree header")?;
        let mut fields = header.split_whitespace();
        let mode = fields.next().ok_or_else(|| anyhow!("missing tree mode"))?;
        let kind = fields
            .next()
            .ok_or_else(|| anyhow!("missing object kind"))?;
        let object = fields.next().ok_or_else(|| anyhow!("missing object id"))?;
        let path =
            std::str::from_utf8(&record[tab + 1..]).context("imported source path is not UTF-8")?;
        if mode == "160000" || kind == "commit" {
            continue;
        }
        if source_selection.is_some_and(|policy| !policy.retains(path)) {
            continue;
        }
        if Path::new(path).is_absolute()
            || Path::new(path)
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            bail!("Git tree path escapes import destination: {path}");
        }
        objects.push((mode.to_string(), object.to_string(), path.to_string()));
    }

    let mut child = Command::new("git")
        .args(["cat-file", "--batch"])
        .current_dir(source_git)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to start git cat-file --batch")?;
    let mut input = child.stdin.take().expect("piped cat-file stdin");
    let mut output = BufReader::new(child.stdout.take().expect("piped cat-file stdout"));

    for (mode, object, relative) in objects {
        writeln!(input, "{object}")?;
        input.flush()?;
        let mut header = String::new();
        output.read_line(&mut header)?;
        let mut fields = header.split_whitespace();
        let actual_object = fields.next().unwrap_or_default();
        let kind = fields.next().unwrap_or_default();
        let size = fields
            .next()
            .ok_or_else(|| anyhow!("missing cat-file size for {relative}"))?
            .parse::<usize>()
            .with_context(|| format!("invalid cat-file size for {relative}"))?;
        if actual_object != object || kind != "blob" {
            bail!(
                "unexpected cat-file response for {relative}: {}",
                header.trim()
            );
        }
        let mut payload = vec![0; size];
        output.read_exact(&mut payload)?;
        let mut terminator = [0_u8; 1];
        output.read_exact(&mut terminator)?;
        if terminator[0] != b'\n' {
            bail!("malformed cat-file terminator for {relative}");
        }

        let target = destination.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        remove_path_if_exists(&target)?;
        if mode == "120000" {
            #[cfg(unix)]
            std::os::unix::fs::symlink(OsString::from_vec(payload), &target)?;
            #[cfg(not(unix))]
            bail!("exact symlink imports require Unix");
        } else {
            fs::write(&target, payload)?;
            set_mode(target, if mode == "100755" { 0o755 } else { 0o644 })?;
        }
    }
    drop(input);
    let status = child.wait()?;
    if !status.success() {
        bail!("git cat-file failed while materializing {treeish}");
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
            remove_path_if_exists(&to)?;
            copy_symlink(&from, &to)?;
        } else if metadata.is_dir() {
            if to.symlink_metadata().is_ok() && !to.is_dir() {
                remove_path_if_exists(&to)?;
            }
            copy_tree_excluding_dotgit(&from, &to)?;
        } else {
            if to.symlink_metadata().is_ok() && !to.is_file() {
                remove_path_if_exists(&to)?;
            }
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
            preserve_permissions(&metadata, &to)?;
        }
    }
    Ok(())
}

/// Copies the authoritative working-tree inputs for an imported component into
/// an output-owned source mirror. Tracked modifications and non-ignored
/// untracked inputs are preserved; ignored build residue is deliberately not.
fn copy_imported_working_tree(
    repo_root: &Path,
    source_relative: &Path,
    destination: &Path,
) -> Result<()> {
    if source_relative.is_absolute()
        || source_relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!(
            "imported source path must be repository-relative: {}",
            source_relative.display()
        );
    }
    let source = repo_root.join(source_relative);
    if !source.is_dir() {
        bail!("imported source directory missing: {}", source.display());
    }

    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
        ])
        .arg(source_relative)
        .current_dir(repo_root)
        .output()
        .context("failed to enumerate authoritative imported-source inputs")?;
    if !output.status.success() {
        bail!(
            "git could not enumerate imported source {}: {}",
            source_relative.display(),
            output.status
        );
    }

    remove_path_if_exists(destination)?;
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    for raw in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let repository_path = PathBuf::from(String::from_utf8_lossy(raw).into_owned());
        let relative = repository_path
            .strip_prefix(source_relative)
            .with_context(|| {
                format!(
                    "git returned {} outside imported source {}",
                    repository_path.display(),
                    source_relative.display()
                )
            })?;
        let from = repo_root.join(&repository_path);
        let Ok(metadata) = fs::symlink_metadata(&from) else {
            // A deleted tracked file is an authoritative working-tree deletion.
            continue;
        };
        let to = destination.join(relative);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        if metadata.file_type().is_symlink() {
            copy_symlink(&from, &to)?;
        } else if metadata.is_file() {
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
            preserve_permissions(&metadata, &to)?;
        }
    }
    Ok(())
}

/// Applies checksummed MattOS patches only after authoritative source has been
/// copied to an output-owned mirror. Vendored source trees remain byte-for-byte
/// equal to their pinned upstream trees.
fn apply_component_patches(
    repo_root: &Path,
    component_name: &str,
    source_mirror: &Path,
) -> Result<()> {
    let output_root = repo_root.join("out");
    if !source_mirror.starts_with(&output_root) {
        bail!(
            "refusing to patch non-output source tree {}",
            source_mirror.display()
        );
    }
    let _lock = ConsumerMirrorLock::acquire(repo_root, source_mirror)?;
    let mirror_relative = source_mirror.strip_prefix(repo_root).with_context(|| {
        format!(
            "output mirror is outside repository: {}",
            source_mirror.display()
        )
    })?;
    let directory_arg = format!("--directory={}", mirror_relative.display());
    let state = read_sync_state(repo_root, component_name)?
        .ok_or_else(|| anyhow!("missing provenance state for {component_name}"))?;
    if state.patch_manifest == "none" {
        return Ok(());
    }
    let manifest_relative = validated_repo_relative_path(&state.patch_manifest)?;
    let manifest_path = repo_root.join(manifest_relative);
    let manifest_sha256 = performance::sha256_file(&manifest_path)?;
    if manifest_sha256 != state.patch_manifest_sha256 {
        bail!(
            "patch manifest checksum mismatch for {}: expected {}, got {}",
            manifest_path.display(),
            state.patch_manifest_sha256,
            manifest_sha256
        );
    }
    let body = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read patch manifest {}", manifest_path.display()))?;
    let manifest: ComponentPatchManifest = toml::from_str(&body)
        .with_context(|| format!("failed to parse patch manifest {}", manifest_path.display()))?;
    if manifest.component != component_name {
        bail!("patch manifest component does not match {component_name}");
    }
    if manifest.application != "output-mirror-only" {
        bail!("patch manifest for {component_name} is not output-mirror-only");
    }
    for record in manifest.patch {
        let patch_relative = validated_repo_relative_path(&record.path)?;
        let patch_path = repo_root.join(patch_relative);
        let actual = performance::sha256_file(&patch_path)?;
        if actual != record.sha256 {
            bail!(
                "patch checksum mismatch for {}: expected {}, got {}",
                patch_path.display(),
                record.sha256,
                actual
            );
        }
        let patch_text = patch_path
            .to_str()
            .ok_or_else(|| anyhow!("patch path is not valid UTF-8: {}", patch_path.display()))?;
        run_cmd(
            repo_root,
            "git",
            &[
                "apply",
                "--check",
                "--whitespace=error-all",
                directory_arg.as_str(),
                patch_text,
            ],
        )?;
        run_cmd(
            repo_root,
            "git",
            &[
                "apply",
                "--whitespace=error-all",
                directory_arg.as_str(),
                patch_text,
            ],
        )?;
    }
    Ok(())
}

fn validated_repo_relative_path(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("provenance path is not a safe repository-relative path: {value}");
    }
    Ok(path)
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

fn stage_resource_profile(stage: BuildStage) -> scheduler::StageResourceProfile {
    if stage == BuildStage::Libcap {
        return scheduler::StageResourceProfile::serial();
    }
    if matches!(
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
            | BuildStage::Flatpak
            | BuildStage::CosmicPortal
            | BuildStage::CosmicEdit
            | BuildStage::CosmicInitialSetup
            | BuildStage::Greetd
    ) {
        return scheduler::StageResourceProfile::high_memory_parallel();
    }
    match stage {
        BuildStage::Kernel
        | BuildStage::Glibc
        | BuildStage::GccRuntime
        | BuildStage::Binutils
        | BuildStage::GccToolchain
        | BuildStage::Brush
        | BuildStage::Coreutils
        | BuildStage::Grep
        | BuildStage::Sed
        | BuildStage::Findutils
        | BuildStage::Diffutils
        | BuildStage::Git
        | BuildStage::Libffi
        | BuildStage::NvidiaDriver
        | BuildStage::Python
        | BuildStage::Rust
        | BuildStage::SudoRs
        | BuildStage::Init
        // XZ-backed squashfs compression scales cleanly to four workers but
        // needs the same bounded per-worker memory admission as compilers.
        | BuildStage::LiveRoot => scheduler::StageResourceProfile::memory_heavy(),
        _ => scheduler::StageResourceProfile::standard(),
    }
}

#[cfg(test)]
fn scheduler_child_job_policy(stage: BuildStage) -> scheduler::ChildJobPolicy {
    stage_resource_profile(stage).child_jobs
}

fn is_cacheable_stage(stage: BuildStage) -> bool {
    !matches!(
        stage,
        BuildStage::Rootfs
            | BuildStage::LiveRoot
            | BuildStage::Initramfs
            | BuildStage::Iso
            | BuildStage::All
    )
}

fn build_stage_id(stage: BuildStage) -> &'static str {
    stage_graph::stage_id(stage)
}

fn build_stage_spec(stage: BuildStage) -> performance::StageSpec {
    let id = build_stage_id(stage);
    let sources = stage_inputs::source_inputs(stage);
    let outputs: Vec<PathBuf> = match stage {
        BuildStage::Kernel => vec![
            "out/build/linux/build/arch/x86/boot/bzImage".into(),
            "out/build/linux/modules/usr/lib/modules".into(),
            "out/build/linux/kernel-release".into(),
        ],
        BuildStage::Glibc => vec![
            "out/build/glibc/install".into(),
            "out/build/glibc/linux-headers".into(),
            "out/build/glibc/linux-headers-inventory.txt".into(),
            "out/sysroot/usr/include/stdio.h".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libc.so.6".into(),
            "out/sysroot/lib64/ld-linux-x86-64.so.2".into(),
        ],
        BuildStage::GccRuntime => vec![
            "out/build/gcc-runtime/install".into(),
            "out/build/gcc-runtime/runtime".into(),
            "out/build/gcc-runtime/runtime-abi.tsv".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libgcc_s.so.1".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libstdc++.so.6.0.34".into(),
        ],
        BuildStage::Binutils => vec![
            "out/build/binutils/cross-install".into(),
            "out/build/binutils/install".into(),
            "out/build/binutils/configure-invocation.txt".into(),
        ],
        BuildStage::GccToolchain => vec![
            "out/build/gcc-toolchain/install".into(),
            "out/build/gcc-toolchain/configure-invocation.txt".into(),
        ],
        BuildStage::Make => vec!["out/build/make/install".into()],
        BuildStage::Brush => vec!["out/build/brush/cargo-target/release/brush".into()],
        BuildStage::Coreutils => {
            vec!["out/build/coreutils/cargo-target/release/coreutils".into()]
        }
        BuildStage::Grep => vec!["out/build/grep/cargo-target/release/grep".into()],
        BuildStage::Sed => vec!["out/build/sed/cargo-target/release/sed".into()],
        BuildStage::Findutils => vec!["out/build/findutils/cargo-target/release/find".into()],
        BuildStage::Diffutils => {
            vec!["out/build/diffutils/cargo-target/release/diffutils".into()]
        }
        BuildStage::Gzip => vec!["out/build/gzip/install".into()],
        BuildStage::Patch => vec!["out/build/patch/install".into()],
        BuildStage::File => vec!["out/build/file/install".into()],
        BuildStage::Less => vec!["out/build/less/install".into()],
        BuildStage::Git => vec!["out/build/git/install".into()],
        BuildStage::Openssh => vec!["out/build/openssh/install".into()],
        BuildStage::Libffi => vec!["out/build/libffi/install".into()],
        BuildStage::Wayland => vec!["out/build/wayland/install".into()],
        BuildStage::Xkbcommon => vec!["out/build/xkbcommon/install".into()],
        BuildStage::Libseat => vec!["out/build/seatd/install".into()],
        BuildStage::LibdisplayInfo => vec!["out/build/libdisplay-info/install".into()],
        BuildStage::Libevdev => vec!["out/build/libevdev/install".into()],
        BuildStage::Libinput => vec!["out/build/libinput/install".into()],
        BuildStage::Pixman => vec!["out/build/pixman/install".into()],
        BuildStage::Libdrm => vec!["out/build/libdrm/install".into()],
        BuildStage::VulkanHeaders => vec!["out/build/vulkan-headers/install".into()],
        BuildStage::VulkanLoader => vec!["out/build/vulkan-loader/install".into()],
        BuildStage::VulkanTools => vec!["out/build/vulkan-tools/install".into()],
        BuildStage::X11Compat => vec!["out/build/x11-compat/install".into()],
        BuildStage::Libglvnd => vec!["out/build/libglvnd/install".into()],
        BuildStage::Mesa => vec!["out/build/mesa/install".into()],
        BuildStage::NvidiaDriver => vec![
            "out/build/nvidia-driver/install".into(),
            "out/build/nvidia-driver/source/LICENSE".into(),
            "out/build/nvidia-driver/runfile.sha256".into(),
        ],
        BuildStage::CosmicComp => vec!["out/build/cosmic-comp/install/usr/bin/cosmic-comp".into()],
        BuildStage::CosmicSession => {
            vec!["out/build/cosmic-session/install/usr/bin/cosmic-session".into()]
        }
        BuildStage::CosmicGreeter => {
            vec!["out/build/cosmic-greeter/install/usr/bin/cosmic-greeter".into()]
        }
        BuildStage::CosmicPanel => {
            vec!["out/build/cosmic-panel/install/usr/bin/cosmic-panel".into()]
        }
        BuildStage::CosmicApplets => {
            vec!["out/build/cosmic-applets/install/usr/bin/cosmic-applets".into()]
        }
        BuildStage::CosmicAppLibrary => {
            vec!["out/build/cosmic-applibrary/install/usr/bin/cosmic-app-library".into()]
        }
        BuildStage::CosmicLauncher => {
            vec!["out/build/cosmic-launcher/install/usr/bin/cosmic-launcher".into()]
        }
        BuildStage::CosmicSettings => {
            vec!["out/build/cosmic-settings/install/usr/bin/cosmic-settings".into()]
        }
        BuildStage::CosmicSettingsDaemon => {
            vec!["out/build/cosmic-settings-daemon/install/usr/bin/cosmic-settings-daemon".into()]
        }
        BuildStage::CosmicNotifications => {
            vec!["out/build/cosmic-notifications/install/usr/bin/cosmic-notifications".into()]
        }
        BuildStage::CosmicOsd => {
            vec!["out/build/cosmic-osd/install/usr/bin/cosmic-osd".into()]
        }
        BuildStage::CosmicBg => {
            vec!["out/build/cosmic-bg/install/usr/bin/cosmic-bg".into()]
        }
        BuildStage::CosmicWorkspaces => {
            vec!["out/build/cosmic-workspaces/install/usr/bin/cosmic-workspaces".into()]
        }
        BuildStage::CosmicFiles => {
            vec!["out/build/cosmic-files/install/usr/bin/cosmic-files".into()]
        }
        BuildStage::CosmicEdit => vec![
            "out/build/cosmic-edit/install/usr/bin/cosmic-edit".into(),
            "out/build/cosmic-edit/install/usr/share/applications/com.system76.CosmicEdit.desktop".into(),
        ],
        BuildStage::CosmicInitialSetup => vec![
            "out/build/cosmic-initial-setup/install/usr/bin/cosmic-initial-setup".into(),
            "out/build/cosmic-initial-setup/install/usr/share/applications/com.system76.CosmicInitialSetup.desktop".into(),
            "out/build/cosmic-initial-setup/install/usr/share/cosmic-layouts/top-panel-and-bottom-dock/layout.kdl".into(),
            "out/build/cosmic-initial-setup/install/usr/share/cosmic-themes/nebula-dark.ron".into(),
        ],
        BuildStage::Duktape => vec!["out/build/duktape/install/usr/lib/x86_64-linux-gnu/libduktape.so.207".into()],
        BuildStage::CosmicTerm => {
            vec!["out/build/cosmic-term/install/usr/bin/cosmic-term".into()]
        }
        BuildStage::CosmicTweaks => {
            vec!["out/build/cosmic-tweaks/install/usr/bin/cosmic-ext-tweaks".into()]
        }
        BuildStage::CosmicUtilities => vec!["out/build/cosmic-utilities/install".into()],
        BuildStage::Flatpak => vec![
            "out/build/flatpak/install/usr/bin/flatpak".into(),
            "out/build/flatpak/install/usr/lib/x86_64-linux-gnu/libflatpak.so.0".into(),
        ],
        BuildStage::Libarchive => vec!["out/build/libarchive/install/usr/lib/x86_64-linux-gnu/libarchive.so.13".into()],
        BuildStage::Libxml2 => vec!["out/build/libxml2/install/usr/lib/x86_64-linux-gnu/libxml2.so.16".into()],
        BuildStage::Libpng => vec!["out/build/libpng/install/usr/lib/x86_64-linux-gnu/libpng16.so.16".into()],
        BuildStage::Fuse3 => vec!["out/build/fuse3/install/usr/lib/x86_64-linux-gnu/libfuse3.so.4".into()],
        BuildStage::Libfyaml => vec!["out/build/libfyaml/install/usr/lib/x86_64-linux-gnu/libfyaml.so.0".into()],
        BuildStage::Libxmlb => vec!["out/build/libxmlb/install/usr/lib/x86_64-linux-gnu/libxmlb.so.2".into()],
        BuildStage::JsonGlib => vec!["out/build/json-glib/install/usr/lib/x86_64-linux-gnu/libjson-glib-1.0.so.0".into()],
        BuildStage::Appstream => vec!["out/build/appstream/install/usr/lib/x86_64-linux-gnu/libappstream.so.5".into()],
        BuildStage::GdkPixbuf => vec!["out/build/gdk-pixbuf/install/usr/lib/x86_64-linux-gnu/libgdk_pixbuf-2.0.so.0".into()],
        BuildStage::Gpgme => vec!["out/build/gpgme/install/usr/lib/x86_64-linux-gnu/libgpgme.so.45".into()],
        BuildStage::Ostree => vec!["out/build/ostree/install/usr/lib/x86_64-linux-gnu/libostree-1.so.1".into()],
        BuildStage::CosmicPortal => {
            vec!["out/build/cosmic-portal/install/usr/libexec/xdg-desktop-portal-cosmic".into()]
        }
        BuildStage::CosmicAssets => {
            vec![
                "out/build/cosmic-assets/install/usr/share/icons/Cosmic/index.theme".into(),
                "out/build/cosmic-assets/install/usr/share/cosmic/com.system76.CosmicPanel/v1/entries".into(),
            ]
        }
        BuildStage::Greetd => vec!["out/build/greetd/install/usr/bin/greetd".into()],
        BuildStage::CosmicDesktop => vec![
            "out/build/cosmic-desktop/install/usr/bin/cosmic-session".into(),
            "out/build/cosmic-desktop/install/usr/bin/cosmic-panel".into(),
            "out/build/cosmic-desktop/install/usr/bin/cosmic-term".into(),
            "out/build/cosmic-desktop/install/usr/bin/greetd".into(),
        ],
        BuildStage::Cozy => vec!["out/build/cozy/install/usr/bin/cozy".into()],
        BuildStage::Python => vec!["out/build/cpython/install".into()],
        BuildStage::Llvm => vec!["out/build/llvm/install".into()],
        BuildStage::Rust => vec!["out/build/rust/install".into()],
        BuildStage::SudoRs => vec!["out/build/sudo-rs/cargo-target/release/sudo".into()],
        BuildStage::Init => vec!["target/release/mattos-init".into()],
        BuildStage::Installer => vec![
            "out/build/installer/cargo-target/release/mattos-install".into(),
            "out/build/installer/cosmic-target/release/mattos-install-cosmic".into(),
            "out/build/btrfs-progs/install/usr/bin/btrfs".into(),
            "out/build/btrfs-progs/install/usr/include/btrfsutil.h".into(),
            "out/build/btrfs-progs/install/usr/lib/x86_64-linux-gnu/libbtrfsutil.so".into(),
            "out/build/btrfs-progs/install/usr/lib/x86_64-linux-gnu/pkgconfig/libbtrfsutil.pc".into(),
            "out/build/dosfstools/install/usr/sbin/mkfs.fat".into(),
            "out/build/e2fsprogs/install/usr/sbin/mkfs.ext4".into(),
            "out/build/installed-initramfs.cpio.xz".into(),
            "out/build/installer/BOOTX64.EFI".into(),
        ],
        BuildStage::LiveRoot => vec![
            LIVE_ROOT_IMAGE_PATH.into(),
            "out/reports/live-root-inventory.tsv".into(),
        ],
        BuildStage::Initramfs => vec![INITRAMFS_ARCHIVE_PATH.into()],
        BuildStage::Iso => vec![
            "out/build/iso".into(),
            "out/images/mattos-x86_64.iso".into(),
            "out/reports/live-image-inventory.tsv".into(),
        ],
        BuildStage::Rootfs => vec!["out/build/rootfs".into()],
        _ => vec![format!("out/build/{}/install", stage_output_directory(stage)).into()],
    };
    performance::StageSpec {
        id: id.to_string(),
        source_inputs: sources,
        configuration_inputs: stage_inputs::configuration_inputs(stage),
        tools: stage_inputs::tool_names(stage),
        dependencies: build_stage_dependencies(stage)
            .iter()
            .map(|value| value.to_string())
            .collect(),
        outputs,
        recipe: format!(
            "mattos-build-stage:{id}:recipe={}:schema={}",
            stage_inputs::recipe_revision(stage),
            performance::STAGE_MANIFEST_SCHEMA_VERSION
        ),
    }
}

fn linux_x86_uapi_inputs() -> Vec<&'static str> {
    stage_inputs::linux_x86_uapi_inputs()
}

fn stage_output_directory(stage: BuildStage) -> &'static str {
    match stage {
        BuildStage::GccToolchain => "gcc-toolchain",
        BuildStage::Procps => "procps-ng",
        BuildStage::Iputils => "iputils",
        BuildStage::Pam => "linux-pam",
        _ => build_stage_id(stage),
    }
}

fn build_stage_dependencies(stage: BuildStage) -> &'static [&'static str] {
    stage_graph::direct_dependencies(stage)
}

fn linux_headers_stage_spec() -> performance::StageSpec {
    performance::StageSpec {
        id: "linux-headers".to_string(),
        source_inputs: linux_x86_uapi_inputs()
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        configuration_inputs: Vec::new(),
        tools: vec!["make".to_string(), "gcc".to_string()],
        dependencies: vec!["glibc".to_string()],
        outputs: vec![
            "out/build/glibc/linux-headers".into(),
            "out/build/glibc/linux-headers-inventory.txt".into(),
        ],
        recipe: "make ARCH=x86 headers_install".to_string(),
    }
}

fn formal_sysroot_stage_spec() -> performance::StageSpec {
    performance::StageSpec {
        id: "formal-sysroot".to_string(),
        source_inputs: Vec::new(),
        configuration_inputs: Vec::new(),
        tools: vec!["gcc".to_string(), "ld".to_string()],
        dependencies: vec![
            "linux-headers".to_string(),
            "glibc".to_string(),
            "gcc-runtime".to_string(),
        ],
        outputs: vec![
            "out/sysroot/usr/include/stdio.h".into(),
            "out/sysroot/usr/include/linux/version.h".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libc.so.6".into(),
            "out/sysroot/lib64/ld-linux-x86-64.so.2".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libgcc_s.so.1".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libstdc++.so.6.0.34".into(),
        ],
        recipe: "formal MattOS sysroot inventory".to_string(),
    }
}

fn validate_cached_build_stage(repo_root: &Path, stage: BuildStage) -> Result<()> {
    match stage {
        BuildStage::Kernel => {
            if !repo_root
                .join("out/build/linux/build/arch/x86/boot/bzImage")
                .is_file()
            {
                bail!("cached Linux image is missing")
            }
        }
        BuildStage::Glibc => {
            for path in [
                "out/sysroot/usr/include/stdio.h",
                "out/sysroot/usr/lib/x86_64-linux-gnu/libc.so.6",
                "out/sysroot/lib64/ld-linux-x86-64.so.2",
            ] {
                if !repo_root.join(path).exists() {
                    bail!("cached glibc/sysroot output is missing: {path}")
                }
            }
        }
        BuildStage::GccRuntime => {
            if !repo_root
                .join("out/sysroot/usr/lib/x86_64-linux-gnu/libgcc_s.so.1")
                .is_file()
            {
                bail!("cached GCC runtime is missing")
            }
        }
        BuildStage::Rust => validate_cached_rust_install(repo_root)?,
        BuildStage::Binutils => {
            for tool in ["as", "ld", "readelf", "strip"] {
                if !repo_root
                    .join("out/build/binutils/install/usr/bin")
                    .join(tool)
                    .is_file()
                {
                    bail!("cached native Binutils tool is missing: {tool}")
                }
            }
        }
        BuildStage::GccToolchain => {
            for tool in ["gcc", "g++"] {
                if !repo_root
                    .join("out/build/gcc-toolchain/install/usr/bin")
                    .join(tool)
                    .is_file()
                {
                    bail!("cached native compiler is missing: {tool}")
                }
            }
        }
        BuildStage::Make => {
            if !repo_root
                .join("out/build/make/install/usr/bin/make")
                .is_file()
            {
                bail!("cached native GNU Make is missing")
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_cached_rust_install(repo_root: &Path) -> Result<()> {
    let install = repo_root.join("out/build/rust/install/usr");
    let rustc = install.join("bin/rustc");
    let cargo = install.join("bin/cargo");
    if !rustc.is_file() || !cargo.is_file() {
        bail!("cached Rust installation is missing rustc or Cargo")
    }
    let rustc_path = path_str(&rustc)?;
    let sysroot = run_cmd_capture(&install, rustc_path, &["--print", "sysroot"])?;
    let reported_sysroot = PathBuf::from(sysroot.trim());
    let expected_sysroot = install.clone();
    let canonical_reported = reported_sysroot.canonicalize().with_context(|| {
        format!(
            "published rustc reported missing sysroot {}",
            reported_sysroot.display()
        )
    })?;
    let canonical_expected = expected_sysroot.canonicalize()?;
    if canonical_reported != canonical_expected {
        bail!(
            "published rustc/sysroot mismatch: rustc reports {}, expected {}",
            canonical_reported.display(),
            canonical_expected.display()
        )
    }
    let target_libdir = run_cmd_capture(&install, rustc_path, &["--print", "target-libdir"])?;
    let target_libdir = PathBuf::from(target_libdir.trim());
    if !target_libdir.is_dir() || !target_libdir.starts_with(&install) {
        bail!(
            "published rustc target library directory is outside its install: {}",
            target_libdir.display()
        )
    }
    if fs::read_dir(&target_libdir)?
        .filter_map(Result::ok)
        .all(|entry| {
            !entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "rlib" || extension == "rmeta")
        })
    {
        bail!("published Rust target library directory has no compiler sysroot artifacts")
    }
    Ok(())
}

fn build_plan(stage: BuildStage) -> Vec<BuildStage> {
    stage_graph::build_plan(stage)
}

fn cacheable_stage_specs(repo_root: &Path) -> Result<Vec<performance::StageSpec>> {
    let mut specs = build_plan(BuildStage::All)
        .into_iter()
        .filter(|stage| {
            is_cacheable_stage(*stage)
                || matches!(
                    stage,
                    BuildStage::Rootfs
                        | BuildStage::LiveRoot
                        | BuildStage::Initramfs
                        | BuildStage::Iso
                )
        })
        .map(build_stage_spec)
        .collect::<Vec<_>>();
    specs.push(linux_headers_stage_spec());
    specs.push(formal_sysroot_stage_spec());
    if let Ok(repository) = packaging::repository_stage_spec(repo_root) {
        specs.push(repository);
    }
    specs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(specs)
}

fn cache_impact(
    repo_root: &Path,
    specs: &[performance::StageSpec],
    requested: &str,
    json: bool,
) -> Result<()> {
    let selected = if requested == "all" {
        specs
            .iter()
            .map(|spec| spec.id.clone())
            .collect::<BTreeSet<_>>()
    } else {
        if !specs.iter().any(|spec| spec.id == requested) {
            bail!("unknown cache stage {requested}")
        }
        let mut selected = BTreeSet::from([requested.to_string()]);
        let mut changed = true;
        while changed {
            changed = false;
            for spec in specs {
                if spec
                    .dependencies
                    .iter()
                    .any(|dependency| selected.contains(dependency))
                    && selected.insert(spec.id.clone())
                {
                    changed = true;
                }
            }
        }
        selected
    };
    let historical_seconds = fs::read_to_string(repo_root.join("out/reports/build-timings.json"))
        .ok()
        .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
        .and_then(|value| value.get("stages").cloned())
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|record| {
            Some((
                record.get("stage")?.as_str()?.to_string(),
                record.get("wall_seconds")?.as_f64()?,
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let mut estimated = 0.0;
    let mut required = 0usize;
    let mut suspicious = 0usize;
    let mut migrations = 0usize;
    performance::begin_read_only_integrity_cache();
    let result = (|| -> Result<()> {
        let impacts = specs
            .iter()
            .filter(|spec| selected.contains(&spec.id))
            .map(|spec| performance::explain_stage_impact(repo_root, spec))
            .collect::<Result<Vec<_>>>()?;
        let impact_by_stage = impacts
            .iter()
            .map(|impact| (impact.stage.as_str(), impact))
            .collect::<BTreeMap<_, _>>();
        if !json {
            println!("cache impact: {requested} (read-only; no build actions will run)");
            println!(
                "tool validity mode: {} (exact tool identities remain recorded as build provenance)",
                if stage_cache::strict_tool_identity_mode() {
                    "strict reproducible"
                } else {
                    "development artifact reuse"
                }
            );
        }
        for (spec, impact) in specs
            .iter()
            .filter(|spec| selected.contains(&spec.id))
            .zip(impacts.iter())
        {
            let previous = historical_seconds.get(&spec.id).copied().unwrap_or(0.0);
            if impact.status == "MIGRATE" {
                migrations += 1;
            } else if impact.status == "MISS" {
                estimated += previous;
                if impact.classification == "unexplained/unrelated invalidation" {
                    suspicious += 1;
                } else {
                    required += 1;
                }
            }
            let chain = spec
                .dependencies
                .iter()
                .filter(|dependency| {
                    impact.changes.iter().any(|change| {
                        change.category == "dependency-output" && change.key == dependency.as_str()
                    })
                })
                .filter_map(|dependency| {
                    impact_by_stage
                        .get(dependency.as_str())
                        .map(|_| dependency.clone())
                })
                .collect::<Vec<_>>();
            if json {
                let mut record = serde_json::to_value(impact)?;
                if let Some(object) = record.as_object_mut() {
                    object.insert(
                        "historical_seconds".to_string(),
                        serde_json::json!(previous),
                    );
                    object.insert("causal_chain".to_string(), serde_json::json!(chain));
                    object.insert(
                        "work_class".to_string(),
                        serde_json::json!(if impact.status == "MISS"
                            && impact.classification == "unexplained/unrelated invalidation"
                        {
                            "suspicious"
                        } else if impact.status == "MISS" {
                            "required"
                        } else if impact.status == "MIGRATE" {
                            "migration"
                        } else {
                            "none"
                        }),
                    );
                }
                println!("{}", serde_json::to_string(&record)?);
            } else if impact.status != "HIT" {
                println!(
                    "{:<7} {:<24} class={:<32} historical={previous:.1}s reason={} changes={} chain={}",
                    impact.status,
                    spec.id,
                    impact.classification,
                    impact.reason,
                    serde_json::to_string(&impact.changes)?,
                    serde_json::to_string(&chain)?,
                );
            }
        }
        if !json {
            println!(
                "totals: selected={} misses={} required={} suspicious={} migrations={} historical_estimate={estimated:.1}s",
                selected.len(),
                required + suspicious,
                required,
                suspicious,
                migrations
            );
        } else {
            println!(
                "{}",
                serde_json::json!({
                    "type": "summary",
                    "selected": selected.len(),
                    "misses": required + suspicious,
                    "required": required,
                    "suspicious": suspicious,
                    "migrations": migrations,
                    "historical_seconds": estimated,
                })
            );
        }
        Ok(())
    })();
    performance::end_read_only_integrity_cache();
    result?;
    println!(
        "downstream propagation: {} stage(s); historical estimated work: {:.1}s",
        selected.len(),
        estimated
    );
    Ok(())
}

fn cache_command(repo_root: &Path, command: CacheCommands) -> Result<()> {
    let specs = cacheable_stage_specs(repo_root)?;
    match command {
        CacheCommands::Status => {
            for spec in &specs {
                println!("{}", performance::explain_stage(repo_root, spec)?);
            }
            packaging::print_package_cache_status(repo_root)?;
            println!("{}", packaging::package_facts_status(repo_root)?);
            println!("{}", elf_cache::status(repo_root)?);
            println!(
                "rootfs-base: not materialized separately; live assembly currently consumes package staging directly"
            );
            Ok(())
        }
        CacheCommands::Explain { stage, details } => {
            if let Some(package) = stage.strip_prefix("package:") {
                return packaging::explain_package_cache(repo_root, package);
            }
            if stage == "elf-facts" {
                println!("{}", elf_cache::status(repo_root)?);
                return Ok(());
            }
            if stage == "package-audit" {
                println!("{}", packaging::package_facts_status(repo_root)?);
                return Ok(());
            }
            if stage == "rootfs-base" || stage == "rootfs-live" {
                let spec = resolve_cache_stage(&specs, "rootfs")?;
                if details {
                    print!("{}", performance::explain_stage_details(repo_root, spec)?);
                } else {
                    println!("{}", performance::explain_stage(repo_root, spec)?);
                }
                return Ok(());
            }
            let spec = resolve_cache_stage(&specs, &stage)?;
            if details {
                print!("{}", performance::explain_stage_details(repo_root, spec)?);
            } else {
                println!("{}", performance::explain_stage(repo_root, spec)?);
            }
            Ok(())
        }
        CacheCommands::Impact { json, stage } => cache_impact(repo_root, &specs, &stage, json),
        CacheCommands::Invalidate { dependents, stage } => {
            if let Some(package) = stage.strip_prefix("package:") {
                return packaging::invalidate_package_cache(repo_root, package);
            }
            if stage == "elf-facts" {
                println!(
                    "invalidated {} ELF fact record(s)",
                    elf_cache::invalidate(repo_root)?
                );
                return Ok(());
            }
            if stage == "package-audit" {
                println!(
                    "invalidated {} package fact/audit record(s)",
                    packaging::invalidate_package_facts(repo_root)?
                );
                return Ok(());
            }
            let stage = if stage == "rootfs-base" || stage == "rootfs-live" {
                "rootfs".to_string()
            } else {
                stage
            };
            let root = resolve_cache_stage(&specs, &stage)?.id.clone();
            let mut selected = BTreeSet::from([root.clone()]);
            if dependents {
                loop {
                    let before = selected.len();
                    for spec in &specs {
                        if spec
                            .dependencies
                            .iter()
                            .any(|dependency| selected.contains(dependency))
                        {
                            selected.insert(spec.id.clone());
                        }
                    }
                    if selected.len() == before {
                        break;
                    }
                }
            }
            for stage in selected {
                if performance::invalidate_manifest(repo_root, &stage)? {
                    println!("invalidated cache manifest: {stage}");
                } else {
                    println!("cache manifest was already absent: {stage}");
                }
            }
            println!(
                "build outputs were preserved; the next dependency-correct build will refresh them"
            );
            Ok(())
        }
    }
}

fn resolve_cache_stage<'a>(
    specs: &'a [performance::StageSpec],
    supplied: &str,
) -> Result<&'a performance::StageSpec> {
    let normalized = if supplied == "gcc-toolchain" {
        "gcc-compiler"
    } else if supplied == "kernel" {
        "linux"
    } else {
        supplied
    };
    specs
        .iter()
        .find(|spec| spec.id == normalized)
        .ok_or_else(|| anyhow!("unknown cache stage {supplied}"))
}

fn build_stage(repo_root: &Path, stage: BuildStage) -> Result<()> {
    performance::trace_log_context("build_stage-entry");
    match stage {
        BuildStage::Kernel => build_kernel(repo_root),
        BuildStage::Glibc => build_glibc(repo_root),
        BuildStage::GccRuntime => build_gcc_runtime(repo_root),
        BuildStage::Binutils => build_binutils(repo_root),
        BuildStage::GccToolchain => {
            performance::trace_log_context("build_stage-before-gcc-toolchain-dispatch");
            build_gcc_toolchain(repo_root)
        }
        BuildStage::Make => build_make(repo_root),
        BuildStage::Brush => build_brush(repo_root),
        BuildStage::Coreutils => build_coreutils(repo_root),
        BuildStage::Grep => build_grep(repo_root),
        BuildStage::Sed => build_sed(repo_root),
        BuildStage::Findutils => build_findutils(repo_root),
        BuildStage::Diffutils => build_diffutils(repo_root),
        BuildStage::Gzip => build_gzip(repo_root),
        BuildStage::Patch => build_patch(repo_root),
        BuildStage::File => build_file(repo_root),
        BuildStage::Less => build_less(repo_root),
        BuildStage::Git => build_git(repo_root),
        BuildStage::Openssh => build_openssh(repo_root),
        BuildStage::Libffi => build_libffi(repo_root),
        BuildStage::Wayland => build_wayland(repo_root),
        BuildStage::Xkbcommon => build_xkbcommon(repo_root),
        BuildStage::Libseat => build_libseat(repo_root),
        BuildStage::LibdisplayInfo => build_libdisplay_info(repo_root),
        BuildStage::Libevdev => build_libevdev(repo_root),
        BuildStage::Libinput => build_libinput(repo_root),
        BuildStage::Pixman => build_pixman(repo_root),
        BuildStage::Libdrm => build_libdrm(repo_root),
        BuildStage::VulkanHeaders => build_vulkan_headers(repo_root),
        BuildStage::VulkanLoader => build_vulkan_loader(repo_root),
        BuildStage::VulkanTools => build_vulkan_tools(repo_root),
        BuildStage::X11Compat => build_x11_compat(repo_root),
        BuildStage::Libglvnd => build_libglvnd(repo_root),
        BuildStage::Mesa => build_mesa(repo_root),
        BuildStage::NvidiaDriver => build_nvidia_driver(repo_root),
        BuildStage::Flatpak => build_flatpak(repo_root),
        BuildStage::Libarchive => build_libarchive(repo_root),
        BuildStage::Libxml2 => build_libxml2(repo_root),
        BuildStage::Libpng => build_libpng(repo_root),
        BuildStage::Fuse3 => build_fuse3(repo_root),
        BuildStage::Libfyaml => build_libfyaml(repo_root),
        BuildStage::Libxmlb => build_libxmlb(repo_root),
        BuildStage::JsonGlib => build_json_glib(repo_root),
        BuildStage::Appstream => build_appstream(repo_root),
        BuildStage::GdkPixbuf => build_gdk_pixbuf(repo_root),
        BuildStage::Gpgme => build_gpgme(repo_root),
        BuildStage::Ostree => build_ostree(repo_root),
        BuildStage::CosmicComp => build_cosmic_comp(repo_root),
        BuildStage::CosmicSession
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
        | BuildStage::CosmicPortal
        | BuildStage::CosmicAssets
        | BuildStage::Greetd => build_cosmic_desktop_component(repo_root, stage),
        BuildStage::CosmicEdit => build_cosmic_edit(repo_root),
        BuildStage::CosmicInitialSetup => build_cosmic_initial_setup(repo_root),
        BuildStage::CosmicDesktop => build_cosmic_desktop(repo_root),
        BuildStage::Cozy => build_cozy(repo_root),
        BuildStage::Python => build_cpython(repo_root),
        BuildStage::Llvm => build_llvm(repo_root),
        BuildStage::Rust => build_rust(repo_root),
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
        BuildStage::Dav1d => build_dav1d(repo_root),
        BuildStage::Glib => build_glib(repo_root),
        BuildStage::Pipewire => build_pipewire(repo_root),
        BuildStage::Openssl => build_openssl(repo_root),
        BuildStage::Elfutils => build_elfutils(repo_root),
        BuildStage::Pcre2 => build_pcre2(repo_root),
        BuildStage::Selinux => build_selinux(repo_root),
        BuildStage::Libxcrypt => build_libxcrypt(repo_root),
        BuildStage::Libmd => build_libmd(repo_root),
        BuildStage::Libbsd => build_libbsd(repo_root),
        BuildStage::Libndp => build_libndp(repo_root),
        BuildStage::Readline => build_readline(repo_root),
        BuildStage::Pam => build_linux_pam(repo_root),
        BuildStage::Shadow => build_shadow(repo_root),
        BuildStage::SudoRs => build_sudo_rs(repo_root),
        BuildStage::UtilLinux => build_util_linux(repo_root),
        BuildStage::Systemd => build_systemd(repo_root),
        BuildStage::Dbus => build_dbus(repo_root),
        BuildStage::DbusBroker => build_dbus_broker(repo_root),
        BuildStage::Dpkg => packaging::build_dpkg(repo_root),
        BuildStage::LibgpgError => {
            build_gpg_autotools_library(repo_root, "libgpg-error", &[], "libgpg-error.so.0")
        }
        BuildStage::Libgcrypt => build_gpg_autotools_library(
            repo_root,
            "libgcrypt",
            &["libgpg-error"],
            "libgcrypt.so.20",
        ),
        BuildStage::Libassuan => {
            build_gpg_autotools_library(repo_root, "libassuan", &["libgpg-error"], "libassuan.so.9")
        }
        BuildStage::Libksba => {
            build_gpg_autotools_library(repo_root, "libksba", &["libgpg-error"], "libksba.so.8")
        }
        BuildStage::Npth => build_gpg_autotools_library(repo_root, "npth", &[], "libnpth.so.0"),
        BuildStage::Gpgv => build_gpgv(repo_root),
        BuildStage::Polkit => build_polkit(repo_root),
        BuildStage::Duktape => build_duktape(repo_root),
        BuildStage::NetworkManager => build_networkmanager(repo_root),
        BuildStage::Apt => packaging::build_apt(repo_root),
        BuildStage::Init => build_init(repo_root),
        BuildStage::Installer => build_installer(repo_root),
        BuildStage::Rootfs => build_rootfs(repo_root),
        BuildStage::LiveRoot => build_live_root(repo_root),
        BuildStage::Initramfs => build_initramfs(repo_root),
        BuildStage::Iso => build_iso(repo_root),
        BuildStage::All => {
            bail!("internal error: BuildStage::All should be expanded by build_plan")
        }
    }
}

#[derive(Debug, Deserialize)]
struct KernelConfigPolicy {
    minimum_module_symbols: usize,
    builtin: Vec<String>,
    module: Vec<String>,
    unsupported: Vec<String>,
    unsupported_prefixes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KernelConfigState {
    Builtin,
    Module,
    Unsupported,
}

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

fn validate_kernel_config_policy(config: &str, policy: &KernelConfigPolicy) -> Result<()> {
    for (symbols, expected) in [
        (&policy.builtin, KernelConfigState::Builtin),
        (&policy.module, KernelConfigState::Module),
    ] {
        for symbol in symbols {
            let actual = kernel_config_state(config, symbol)
                .with_context(|| format!("kernel policy symbol {symbol} is absent"))?;
            if actual != expected {
                bail!("kernel policy requires {symbol}={expected:?}, found {actual:?}");
            }
        }
    }
    for symbol in &policy.unsupported {
        if let Some(actual @ (KernelConfigState::Builtin | KernelConfigState::Module)) =
            kernel_config_state(config, symbol)
        {
            bail!("kernel policy requires {symbol}=Unsupported, found {actual:?}");
        }
    }
    for prefix in &policy.unsupported_prefixes {
        if let Some(line) = config.lines().find(|line| {
            line.starts_with(prefix)
                && !line.starts_with("CONFIG_PATA_TIMINGS=")
                && (line.ends_with("=y") || line.ends_with("=m"))
        }) {
            bail!("kernel legacy-family policy rejects {line}");
        }
    }
    let modules = config.lines().filter(|line| line.ends_with("=m")).count();
    if modules < policy.minimum_module_symbols {
        bail!(
            "kernel generic coverage regressed to {modules} module symbols; policy requires at least {}",
            policy.minimum_module_symbols
        );
    }
    Ok(())
}

fn read_kernel_config_policy(repo_root: &Path) -> Result<KernelConfigPolicy> {
    let path = repo_root.join("src/kernel/config/x86_64_mattos.policy.toml");
    toml::from_str(&fs::read_to_string(&path)?)
        .with_context(|| format!("parse kernel configuration policy {}", path.display()))
}

fn kernel_source_worktree_identity(repo_root: &Path) -> Result<String> {
    let relative = "src/kernel/linux";
    let diff = Command::new("git")
        .args(["diff", "--binary", "HEAD", "--", relative])
        .current_dir(repo_root)
        .output()?;
    if !diff.status.success() {
        bail!("git could not inspect the Linux working tree");
    }
    let untracked = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--others",
            "--exclude-standard",
            "--",
            relative,
        ])
        .current_dir(repo_root)
        .output()?;
    if !untracked.status.success() {
        bail!("git could not inspect untracked Linux inputs");
    }
    let mut hasher = Sha256Hasher::new();
    hasher.update(fs::read(repo_root.join("upstream/state/linux.toml"))?);
    hasher.update(&diff.stdout);
    for raw in untracked
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let path = PathBuf::from(String::from_utf8_lossy(raw).into_owned());
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(fs::read(repo_root.join(path))?);
    }
    Ok(format!("{:x}", hasher.finalize()))
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

    let out_root = repo_root.join("out/build/linux");
    let source = out_root.join("source");
    let build = out_root.join("build");
    let source_identity = kernel_source_worktree_identity(repo_root)?;
    let source_identity_path = out_root.join("source-identity");
    let source_changed =
        fs::read_to_string(&source_identity_path).ok().as_deref() != Some(source_identity.as_str());
    fs::create_dir_all(&out_root)?;
    if source_changed {
        remove_path_if_exists(&source)?;
        remove_path_if_exists(&build)?;
    }
    remove_path_if_exists(&out_root.join("modules"))?;
    if !source.is_dir() {
        copy_imported_working_tree(repo_root, Path::new("src/kernel/linux"), &source)?;
        fs::write(&source_identity_path, &source_identity)?;
    }
    fs::create_dir_all(&build).with_context(|| format!("failed to create {}", build.display()))?;

    let config_text = fs::read_to_string(&config)
        .with_context(|| format!("failed to read {}", config.display()))?;
    let policy = read_kernel_config_policy(repo_root)?;
    validate_kernel_config_policy(&config_text, &policy)?;
    fs::write(build.join(".config"), config_text)
        .with_context(|| format!("failed to stage kernel config from {}", config.display()))?;

    let env = local_tool_env(repo_root);
    if let Some(env) = &env {
        println!(
            "Using local rootless toolchain from {}",
            env.tool_root.display()
        );
    }
    let output_arg = format!("O={}", build.display());
    // The kernel does not consume SOURCE_DATE_EPOCH directly for all of its
    // generated metadata.  Pin the release banner and built-in initramfs cpio
    // mtimes explicitly; otherwise two healthy builds differ only by their
    // wall-clock build time and the GNU build ID derived from it.
    let kernel_reproducible_args = [
        "KBUILD_BUILD_TIMESTAMP=2026-01-01 00:00:00 UTC",
        "KBUILD_BUILD_USER=mattos",
        "KBUILD_BUILD_HOST=mattos-build",
        "KBUILD_BUILD_VERSION=1",
        "KCONFIG_NOTIMESTAMP=1",
    ];
    let mut olddefconfig_args = vec![output_arg.as_str(), "olddefconfig"];
    olddefconfig_args.extend(kernel_reproducible_args);
    run_cmd_with_env(&source, "make", &olddefconfig_args, env.as_ref())?;
    validate_kernel_config_policy(&fs::read_to_string(build.join(".config"))?, &policy)?;
    let mut build_args = vec![output_arg.as_str(), "-j", "4"];
    build_args.extend(kernel_reproducible_args);
    run_cmd_with_env(&source, "make", &build_args, env.as_ref()).context("kernel build failed")?;

    let bz = build.join("arch/x86/boot/bzImage");
    if !bz.exists() {
        bail!("kernel build finished without bzImage at {}", bz.display())
    }
    let modules = out_root.join("modules");
    fs::create_dir_all(&modules)?;
    let release = fs::read_to_string(build.join("include/config/kernel.release"))?
        .trim()
        .to_owned();
    let module_dir = modules.join("usr/lib/modules").join(&release);
    let modlib = format!("MODLIB={}", module_dir.display());
    let mut modules_install_args = vec![
        output_arg.as_str(),
        "modules_install",
        modlib.as_str(),
        "DEPMOD=true",
    ];
    modules_install_args.extend(kernel_reproducible_args);
    run_cmd_with_env(&source, "make", &modules_install_args, env.as_ref())?;
    for link in ["build", "source"] {
        remove_path_if_exists(&module_dir.join(link))?;
    }
    run_cmd(
        repo_root,
        "depmod",
        &[
            "-b",
            path_str(&modules)?,
            "-m",
            "/usr/lib/modules",
            &release,
        ],
    )?;
    for metadata in ["modules.dep", "modules.alias", "modules.builtin"] {
        if !module_dir.join(metadata).is_file() {
            bail!("kernel modules_install/depmod did not produce {metadata}");
        }
    }
    let mut module_files = Vec::new();
    collect_regular_files(&module_dir, &mut module_files)?;
    let module_count = module_files
        .iter()
        .filter(|path| path.to_string_lossy().ends_with(".ko.zst"))
        .count();
    if module_count < 500 {
        bail!("generic kernel produced only {module_count} compressed modules");
    }
    fs::write(out_root.join("kernel-release"), format!("{release}\n"))?;
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

    let linux_source = output.join("linux-source");
    let linux_build = output.join("linux-build");
    copy_imported_working_tree(repo_root, Path::new("src/kernel/linux"), &linux_source)?;
    fs::create_dir_all(&linux_build)?;
    let output_arg = format!("O={}", linux_build.display());
    let headers_arg = format!("INSTALL_HDR_PATH={}", headers_root.display());
    run_cmd(
        &linux_source,
        "make",
        &[
            output_arg.as_str(),
            "ARCH=x86",
            "headers_install",
            headers_arg.as_str(),
        ],
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
    let mut command = Command::new(program);
    let scheduler_args = scheduler_command_args(args);
    command.current_dir(cwd).args(&scheduler_args);
    apply_reproducible_process_environment(&mut command);
    for (key, value) in env {
        command.env(key, value);
    }
    apply_scheduler_parallelism(&mut command);
    let display = effective_command_display(&program.display().to_string(), &scheduler_args);
    let status = performance::run_logged_command(&mut command, &display)?;
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
            "SOURCE_DATE_EPOCH={} LC_ALL=C TZ=UTC CFLAGS_FOR_TARGET='{}' CXXFLAGS_FOR_TARGET='{}' LDFLAGS_FOR_TARGET='-Wl,-z,relro -Wl,-z,now' {} {}\nmake all-target-libgcc all-target-libstdc++-v3\nmake DESTDIR={} install-target-libgcc install-target-libstdc++-v3\n",
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
        &["all-target-libgcc", "all-target-libstdc++-v3"],
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
const BINUTILS_UPSTREAM_COMMIT: &str = "5e56594815854de5eca35c7c04b11705d0f19c02";
const BINUTILS_UPSTREAM_MIRROR: &str = "https://git.sr.ht/~sourceware/binutils-gdb";
const BINUTILS_SYSROFF_SHA256: &str =
    "cfb4453d4514513d18f1cc2f98fcb97fcce2273b39a31df9507c20dbc5abc3d8";

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
    let imported_source = repo_root.join("src/toolchain/binutils");
    let output = repo_root.join("out/build/binutils");
    let source = output.join("source");
    let cross_build = output.join("cross-build");
    let cross_install = output.join("cross-install");
    let native_build = output.join("native-build");
    let native_install = output.join("install");
    let wrapper_dir = output.join("bootstrap-bin");
    if !imported_source.join("configure").is_file() {
        bail!(
            "Binutils source is missing at {}",
            imported_source.display()
        )
    }
    if !repo_root.join("out/sysroot/usr/include/stdio.h").is_file() {
        bail!("Binutils requires the completed MattOS development sysroot")
    }
    let sysroff_info = ensure_binutils_sysroff_info(repo_root)?;
    remove_path_if_exists(&output)?;
    copy_imported_working_tree(repo_root, Path::new("src/toolchain/binutils"), &source)?;
    fs::copy(&sysroff_info, source.join("binutils/sysroff.info")).with_context(|| {
        format!(
            "failed to stage {} into output-owned Binutils source mirror",
            sysroff_info.display()
        )
    })?;
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

fn ensure_binutils_sysroff_info(repo_root: &Path) -> Result<PathBuf> {
    let cache = repo_root
        .join("out/cache/binutils")
        .join(BINUTILS_UPSTREAM_COMMIT);
    let file = cache.join("sysroff.info");
    if file.is_file() {
        let actual = performance::sha256_file(&file)?;
        if actual != BINUTILS_SYSROFF_SHA256 {
            bail!(
                "cached Binutils sysroff.info checksum mismatch: expected {}, got {} at {}",
                BINUTILS_SYSROFF_SHA256,
                actual,
                file.display()
            );
        }
        return Ok(file);
    }

    fs::create_dir_all(&cache).with_context(|| format!("failed to create {}", cache.display()))?;
    let git_dir = repo_root.join("out/cache/binutils/upstream.git");
    if !git_dir.is_dir() {
        run_cmd(repo_root, "git", &["init", "--bare", path_str(&git_dir)?])?;
    }
    let git_dir_arg = format!("--git-dir={}", git_dir.display());
    run_cmd(
        repo_root,
        "git",
        &[
            git_dir_arg.as_str(),
            "fetch",
            "--depth=1",
            BINUTILS_UPSTREAM_MIRROR,
            BINUTILS_UPSTREAM_COMMIT,
        ],
    )?;
    let object = format!("{BINUTILS_UPSTREAM_COMMIT}:binutils/sysroff.info");
    let output = Command::new("git")
        .args([git_dir_arg.as_str(), "show", object.as_str()])
        .output()
        .context("failed to read sysroff.info from pinned Binutils commit")?;
    if !output.status.success() {
        bail!(
            "pinned Binutils commit did not provide binutils/sysroff.info: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let temp = file.with_extension("info.tmp");
    fs::write(&temp, &output.stdout)
        .with_context(|| format!("failed to write {}", temp.display()))?;
    let actual = performance::sha256_file(&temp)?;
    if actual != BINUTILS_SYSROFF_SHA256 {
        let _ = fs::remove_file(&temp);
        bail!(
            "downloaded Binutils sysroff.info checksum mismatch: expected {}, got {}",
            BINUTILS_SYSROFF_SHA256,
            actual
        );
    }
    fs::rename(&temp, &file).with_context(|| format!("failed to publish {}", file.display()))?;
    Ok(file)
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
    // MAKEFLAGS is installed from the scheduler's launch-time child-job grant.
    // Do not retain a recipe-local cap here: these prerequisite builds are part
    // of the GCC compiler stage and must use the same authoritative grant.
    run_gcc_bootstrap_command(build, Path::new("make"), &[], env)?;
    run_gcc_bootstrap_command(build, Path::new("make"), &["install"], env)?;
    Ok(())
}

fn log_gcc_info_index_boundary(label: &str, install: &Path) -> Result<()> {
    let index = install.join("usr/share/info/dir");
    let state = match fs::symlink_metadata(&index) {
        Ok(metadata) => format!(
            "exists type={:?} size={}",
            metadata.file_type(),
            metadata.len()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "absent".to_string(),
        Err(error) => format!("metadata-error={error}"),
    };
    performance::append_active_stage_log(&format!(
        "gcc-info-normalization boundary={label} install={} index={} {state}",
        install.display(),
        index.display()
    ))
}

fn build_gcc_toolchain(repo_root: &Path) -> Result<()> {
    let output = repo_root.join("out/build/gcc-toolchain");
    let build = output.join("build");
    let install = output.join("install");
    let prereq_install = output.join("prerequisite-install");
    performance::trace_log_context("build_gcc_toolchain-entry");
    log_gcc_info_index_boundary("build_gcc_toolchain-entry", &install)?;
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
    // GCC feeds the selected linker command into `checksum-options`, which is
    // then hashed into cc1/cc1plus for PCH compatibility.  An absolute wrapper
    // path therefore makes otherwise identical compilers checkout-dependent.
    // The wrapper directory is already first in PATH, so use stable basenames
    // for the compiler proper while retaining absolute paths for prerequisite
    // builds that execute from several different working directories.
    let cc_name = cc
        .file_name()
        .and_then(OsStr::to_str)
        .context("GCC bootstrap C wrapper has no UTF-8 basename")?
        .to_string();
    let cxx_name = cxx
        .file_name()
        .and_then(OsStr::to_str)
        .context("GCC bootstrap C++ wrapper has no UTF-8 basename")?
        .to_string();
    gcc_env.extend([
        ("CC", cc_name.clone()),
        ("CXX", cxx_name.clone()),
        ("CFLAGS", "-O2 -g0".to_string()),
        ("CXXFLAGS", "-O2 -g0".to_string()),
        ("LDFLAGS", "-Wl,-z,relro -Wl,-z,now".to_string()),
    ]);
    run_gcc_bootstrap_command(&build, &configure, &configure_args, &gcc_env)
        .context("MattOS-native GCC configure failed")?;
    run_gcc_bootstrap_command(&build, Path::new("make"), &["all-gcc"], &gcc_env)
        .context("MattOS-native GCC compiler build failed")?;
    let destdir = format!("DESTDIR={}", install.display());
    log_gcc_info_index_boundary("before-install-gcc", &install)?;
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &[destdir.as_str(), "install-gcc"],
        &gcc_env,
    )?;
    log_gcc_info_index_boundary("after-install-gcc", &install)?;
    // `install-gcc` invokes install-info for several manuals.  That shared
    // index is updated by parallel install rules and can omit/reorder entries
    // between otherwise identical builds.  The individual .info manuals are
    // authoritative; Debian-compatible package installation regenerates the
    // directory index through install-info, so do not publish this transient
    // build-time index.
    let info_dir_index = install.join("usr/share/info/dir");
    log_gcc_info_index_boundary("before-normalization", &install)?;
    remove_path_if_exists(&info_dir_index)?;
    log_gcc_info_index_boundary("after-normalization", &install)?;
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
            "CC={} CXX={} CC_FOR_BUILD=/usr/bin/gcc CXX_FOR_BUILD=/usr/bin/g++ {} {}\nmake all-gcc\nmake DESTDIR={} install-gcc\n",
            cc_name,
            cxx_name,
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
    let source_relative = Path::new("src/userland/brush");
    let brush = repo_root.join(source_relative);
    if !brush.join("Cargo.toml").exists() {
        bail!(
            "brush source not found in {}; run import first",
            brush.display()
        );
    }
    let out_root = repo_root.join("out/build/brush");
    let source_copy = out_root.join("source");
    let target = out_root.join("cargo-target");
    copy_imported_working_tree(repo_root, source_relative, &source_copy)?;
    apply_component_patches(repo_root, "brush", &source_copy)?;
    run_cmd_with_env_overrides(
        &source_copy,
        "cargo",
        &["build", "--locked", "--release", "-p", "brush"],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
    )
}

fn build_coreutils(repo_root: &Path) -> Result<()> {
    let coreutils = repo_root.join("src/userland/coreutils");
    if !coreutils.join("Cargo.toml").exists() {
        bail!(
            "coreutils source not found in {}; run import first",
            coreutils.display()
        );
    }
    let target = repo_root.join("out/build/coreutils/cargo-target");
    run_cmd_with_env_overrides(
        &coreutils,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "-p",
            "coreutils",
            "--no-default-features",
            "--features",
            "unix",
        ],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
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
    let target = repo_root.join("out/build/grep/cargo-target");
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/userland/grep/Cargo.toml",
            "--bin",
            "grep",
        ],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
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
    let target = repo_root.join("out/build/sed/cargo-target");
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/userland/sed/Cargo.toml",
            "--bin",
            "sed",
        ],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
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
    let target = repo_root.join("out/build/findutils/cargo-target");
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/userland/findutils/Cargo.toml",
            "--bins",
        ],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
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
    let target = repo_root.join("out/build/diffutils/cargo-target");
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/userland/diffutils/Cargo.toml",
            "--bin",
            "diffutils",
        ],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
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
    } else {
        // Meson build.dat is not portable across Meson versions. Reconfigure
        // an existing tree on every invocation so a host Meson upgrade cannot
        // leave this stage with stale serialized build data.
        let mut setup_args = vec![
            "setup".to_string(),
            "--reconfigure".to_string(),
            build_dir.display().to_string(),
            pam_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        if needs_reconfigure {
            fs::write(&options_path, &options_text)
                .with_context(|| format!("failed to write {}", options_path.display()))?;
        }
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

    let out_root = repo_root.join("out/build/shadow");
    let source = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp = build_dir.join("config.stamp");
    let man_po_makefile = ensure_shadow_man_po_makefile(repo_root)?;
    remove_path_if_exists(&out_root)?;
    copy_imported_working_tree(repo_root, Path::new("src/system/auth/shadow"), &source)?;
    fs::copy(&man_po_makefile, source.join("man/po/Makefile.in")).with_context(|| {
        format!(
            "failed to stage {} into output-owned Shadow source mirror",
            man_po_makefile.display()
        )
    })?;
    if !source.join("configure").exists() {
        run_cmd(&source, "autoreconf", &["-v", "-f", "-i"])?;
    }
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
            source
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

fn ensure_shadow_man_po_makefile(repo_root: &Path) -> Result<PathBuf> {
    let cache = repo_root
        .join("out/cache/shadow")
        .join(SHADOW_UPSTREAM_COMMIT);
    let file = cache.join("man-po-Makefile.in");
    if file.is_file() {
        let actual = performance::sha256_file(&file)?;
        if actual != SHADOW_MAN_PO_MAKEFILE_SHA256 {
            bail!(
                "cached Shadow man/po/Makefile.in checksum mismatch: expected {}, got {} at {}",
                SHADOW_MAN_PO_MAKEFILE_SHA256,
                actual,
                file.display()
            );
        }
        return Ok(file);
    }

    fs::create_dir_all(&cache).with_context(|| format!("failed to create {}", cache.display()))?;
    let git_dir = repo_root.join("out/cache/shadow/upstream.git");
    if !git_dir.is_dir() {
        run_cmd(repo_root, "git", &["init", "--bare", path_str(&git_dir)?])?;
    }
    let git_dir_arg = format!("--git-dir={}", git_dir.display());
    run_cmd(
        repo_root,
        "git",
        &[
            git_dir_arg.as_str(),
            "fetch",
            "--depth=1",
            SHADOW_UPSTREAM_REPOSITORY,
            SHADOW_UPSTREAM_COMMIT,
        ],
    )?;
    let object = format!("{SHADOW_UPSTREAM_COMMIT}:man/po/Makefile.in");
    let output = Command::new("git")
        .args([git_dir_arg.as_str(), "show", object.as_str()])
        .output()
        .context("failed to read man/po/Makefile.in from pinned Shadow commit")?;
    if !output.status.success() {
        bail!(
            "pinned Shadow commit did not provide man/po/Makefile.in: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let temp = file.with_extension("tmp");
    fs::write(&temp, &output.stdout)
        .with_context(|| format!("failed to write {}", temp.display()))?;
    let actual = performance::sha256_file(&temp)?;
    if actual != SHADOW_MAN_PO_MAKEFILE_SHA256 {
        let _ = fs::remove_file(&temp);
        bail!(
            "downloaded Shadow man/po/Makefile.in checksum mismatch: expected {}, got {}",
            SHADOW_MAN_PO_MAKEFILE_SHA256,
            actual
        );
    }
    fs::rename(&temp, &file).with_context(|| format!("failed to publish {}", file.display()))?;
    Ok(file)
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
    let target = repo_root.join("out/build/sudo-rs/cargo-target");
    let env_overrides = vec![
        ("RUSTFLAGS", rustflags),
        ("LIBRARY_PATH", library_path),
        ("CARGO_TARGET_DIR", target.display().to_string()),
    ];

    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
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
        let src = target.join("release").join(bin);
        if !src.exists() {
            bail!("sudo-rs build did not produce {}", src.display());
        }
        let dst = install_dir.join("usr/bin").join(bin);
        fs::copy(&src, &dst).with_context(|| format!("failed to copy {}", src.display()))?;
    }

    Ok(())
}

fn build_util_linux(repo_root: &Path) -> Result<()> {
    let authoritative_source = repo_root.join("src/userland/util-linux");
    if !authoritative_source.join("meson.build").exists() {
        bail!(
            "util-linux source not found in {}; run upstream import util-linux first",
            authoritative_source.display()
        );
    }

    let out_root = repo_root.join("out/build/util-linux");
    let util_linux_src = out_root.join("source");
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
    let ncurses_install = repo_root.join("out/build/ncurses/install/usr");
    let ncurses_pkgconfig = ncurses_install.join("lib/x86_64-linux-gnu/pkgconfig");
    let ncurses_include = ncurses_install.join("include");
    let ncurses_lib = ncurses_install.join("lib/x86_64-linux-gnu");
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
    let staged_pkg_config = std::env::join_paths([
        &pam_pkgconfig,
        &selinux_pkgconfig,
        &pcre2_pkgconfig,
        &ncurses_pkgconfig,
    ])?
    .to_string_lossy()
    .to_string();
    let pkg_config_path = if current_pkg_config.is_empty() {
        staged_pkg_config
    } else {
        format!("{staged_pkg_config}:{current_pkg_config}")
    };
    let current_cflags = std::env::var("CFLAGS").unwrap_or_default();
    let staged_cflags = format!(
        "-I{} -I{} -I{} -I{}",
        pam_include.display(),
        selinux_include.display(),
        pcre2_include.display(),
        ncurses_include.display()
    );
    let cflags = if current_cflags.is_empty() {
        staged_cflags
    } else {
        format!("{staged_cflags} {current_cflags}")
    };
    let current_ldflags = std::env::var("LDFLAGS").unwrap_or_default();
    let staged_ldflags = format!(
        "-L{} -L{} -L{} -L{}",
        pam_lib.display(),
        selinux_lib.display(),
        pcre2_lib.display(),
        ncurses_lib.display()
    );
    let ldflags = if current_ldflags.is_empty() {
        staged_ldflags
    } else {
        format!("{staged_ldflags} {current_ldflags}")
    };
    let library_path = std::env::join_paths([&pam_lib, &selinux_lib, &pcre2_lib, &ncurses_lib])?
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
    sync_build_source(&authoritative_source, &util_linux_src)?;
    apply_component_patches(repo_root, "util-linux", &util_linux_src)?;

    let options = util_linux_meson_options();
    let options_text = format!(
        "policy=base-userland-output-mirror-v2\n{}\n",
        options.join("\n")
    );
    let existing_options = fs::read_to_string(&options_path).ok();
    let needs_reconfigure = existing_options.as_deref() != Some(options_text.as_str());
    let env_changed = existing_env.as_deref() != Some(env_text.as_str());
    let mut configured = build_dir.join("build.ninja").exists();

    if configured && (env_changed || needs_reconfigure) {
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
    rewrite_staged_pkgconfig_files(&install_dir)?;

    for path in [
        install_dir.join("usr/sbin/agetty"),
        install_dir.join("usr/bin/login"),
        install_dir.join("usr/bin/su"),
        install_dir.join("usr/bin/mount"),
        install_dir.join("usr/bin/umount"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libblkid.so.1"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libmount.so.1"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libsmartcols.so.1"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libuuid.so.1"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libfdisk.so.1"),
        install_dir.join("usr/bin/lsblk"),
        install_dir.join("usr/bin/dmesg"),
        install_dir.join("usr/sbin/fdisk"),
        install_dir.join("usr/sbin/sfdisk"),
        install_dir.join("usr/sbin/cfdisk"),
        install_dir.join("usr/sbin/wipefs"),
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
        "-Dbuild-libuuid=enabled".to_string(),
        "-Dbuild-libfdisk=enabled".to_string(),
        "-Dbuild-mount=enabled".to_string(),
        "-Dbuild-fdisks=enabled".to_string(),
        "-Dbuild-losetup=enabled".to_string(),
        "-Dbuild-lsns=enabled".to_string(),
        "-Dbuild-wipefs=enabled".to_string(),
        "-Dbuild-mountpoint=enabled".to_string(),
        "-Dbuild-unshare=enabled".to_string(),
        "-Dbuild-nsenter=enabled".to_string(),
        "-Dbuild-blockdev=enabled".to_string(),
        "-Dbuild-lsblk=enabled".to_string(),
        "-Dbuild-lslocks=enabled".to_string(),
        "-Dbuild-findmnt=enabled".to_string(),
        "-Dbuild-flock=enabled".to_string(),
        "-Dbuild-dmesg=enabled".to_string(),
        "-Dbuild-schedutils=enabled".to_string(),
        "-Dbuild-prlimit=enabled".to_string(),
        "-Dbuild-lscpu=enabled".to_string(),
        "-Dncursesw=enabled".to_string(),
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
    let imported_source = repo_root.join("src/userland/procps-ng");
    if !imported_source.join("configure.ac").exists() {
        bail!(
            "procps-ng source not found in {}; run upstream import procps-ng first",
            imported_source.display()
        );
    }
    let out_root = repo_root.join("out/build/procps-ng");
    let source = out_root.join("source");
    remove_path_if_exists(&out_root)?;
    copy_imported_working_tree(repo_root, Path::new("src/userland/procps-ng"), &source)?;
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

const SOURCE_MIRROR_RSYNC_FLAGS: &[&str] = &[
    "-a",
    "--delete",
    "--delete-excluded",
    "--exclude=.git/",
    "--exclude=target/",
    "--exclude=__pycache__/",
    "--exclude=*.pyc",
];

fn sync_build_source(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let _lock = ConsumerMirrorLock::acquire(&source_lock_repo_root(source)?, destination)?;
    let source_arg = format!("{}/", source.display());
    let destination_arg = format!("{}/", destination.display());
    let mut args = SOURCE_MIRROR_RSYNC_FLAGS.to_vec();
    args.extend([source_arg.as_str(), destination_arg.as_str()]);
    run_cmd(Path::new("/"), "rsync", &args)
}

fn source_lock_repo_root(source: &Path) -> Result<PathBuf> {
    source
        .ancestors()
        .find(|candidate| {
            candidate.join("Cargo.toml").is_file()
                && candidate
                    .join("src/tools/mattos-build/Cargo.toml")
                    .is_file()
        })
        .map(Path::to_path_buf)
        .or_else(|| {
            std::env::current_dir().ok().and_then(|cwd| {
                cwd.ancestors()
                    .find(|candidate| {
                        candidate.join("Cargo.toml").is_file()
                            && candidate
                                .join("src/tools/mattos-build/Cargo.toml")
                                .is_file()
                    })
                    .map(Path::to_path_buf)
            })
        })
        .ok_or_else(|| {
            anyhow!(
                "unable to locate MattOS root for source mirror {}",
                source.display()
            )
        })
}

fn prune_derived_source_mirror_artifacts(repo_root: &Path) -> Result<()> {
    let root = repo_root.join("out/build/cosmic-desktop/sources");
    if !root.is_dir() {
        return Ok(());
    }
    fn visit(path: &Path) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let child = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                if entry.file_name() == "target" || entry.file_name() == "__pycache__" {
                    fs::remove_dir_all(&child).with_context(|| {
                        format!(
                            "failed to prune derived source mirror directory {}",
                            child.display()
                        )
                    })?;
                } else {
                    visit(&child)?;
                }
            } else if file_type.is_file()
                && child
                    .extension()
                    .is_some_and(|extension| extension == "pyc")
            {
                fs::remove_file(&child).with_context(|| {
                    format!(
                        "failed to prune derived source mirror file {}",
                        child.display()
                    )
                })?;
            }
        }
        Ok(())
    }
    visit(&root)
}

struct ConsumerMirrorLock {
    #[cfg(unix)]
    file: fs::File,
}

impl ConsumerMirrorLock {
    fn acquire(repo_root: &Path, mirror: &Path) -> Result<Self> {
        let locks = repo_root.join("out/source-ownership/locks");
        fs::create_dir_all(&locks)?;
        let resolved = mirror
            .canonicalize()
            .with_context(|| format!("unable to resolve consumer mirror {}", mirror.display()))?;
        let digest = Sha256Hasher::digest(resolved.to_string_lossy().as_bytes());
        let lock_id = digest[..12]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = locks.join(format!("consumer-{lock_id}.lock"));
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if result != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(Self { file })
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(Self {})
        }
    }
}

impl Drop for ConsumerMirrorLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            unsafe {
                libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
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
        &[".", "-type", "f", "-exec", "touch", "-c", "{}", "+"],
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
        &[".", "-type", "f", "-exec", "touch", "-c", "{}", "+"],
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

fn ensure_verified_release_archive(
    out_root: &Path,
    filename: &str,
    url: &str,
    expected_sha256: &str,
) -> Result<PathBuf> {
    let bootstrap = out_root.join("bootstrap");
    fs::create_dir_all(&bootstrap)?;
    let archive = bootstrap.join(filename);
    if archive.is_file() && performance::sha256_file(&archive)? == expected_sha256 {
        return Ok(archive);
    }
    let temporary = bootstrap.join(format!("{filename}.tmp"));
    remove_path_if_exists(&temporary)?;
    run_cmd(
        out_root,
        "curl",
        &[
            "-fL",
            "--retry",
            "3",
            "--output",
            path_str(&temporary)?,
            url,
        ],
    )?;
    let actual = performance::sha256_file(&temporary)?;
    if actual != expected_sha256 {
        bail!(
            "release archive checksum mismatch for {url}: expected {expected_sha256}, got {actual}"
        );
    }
    fs::rename(&temporary, &archive)?;
    Ok(archive)
}

fn stage_release_source(archive: &Path, source_copy: &Path) -> Result<()> {
    remove_path_if_exists(source_copy)?;
    fs::create_dir_all(source_copy)?;
    let extract_flag = if archive.extension().and_then(OsStr::to_str) == Some("gz") {
        "-xzf"
    } else {
        "-xJf"
    };
    run_cmd(
        source_copy,
        "tar",
        &[extract_flag, path_str(archive)?, "--strip-components=1"],
    )
}

fn isolate_standalone_cargo_manifest(manifest: &Path) -> Result<()> {
    let mut contents = fs::read_to_string(manifest)
        .with_context(|| format!("failed to read {}", manifest.display()))?;
    if !contents.lines().any(|line| line.trim() == "[workspace]") {
        // Cargo otherwise keeps walking above an output-owned release mirror
        // and can incorrectly adopt MattOS's outer workspace. Rust's bootstrap
        // crate is intentionally standalone upstream; make that boundary
        // explicit without changing the authoritative imported source tree.
        contents.push_str("\n# MattOS output-mirror workspace boundary.\n[workspace]\n");
        fs::write(manifest, contents)
            .with_context(|| format!("failed to isolate {}", manifest.display()))?;
    }
    Ok(())
}

fn build_release_autotools_program(
    repo_root: &Path,
    component: &str,
    archive_filename: &str,
    archive_url: &str,
    archive_sha256: &str,
    dependencies: &[&str],
    options: &[&str],
    required_outputs: &[&str],
) -> Result<()> {
    let out_root = repo_root.join("out/build").join(component);
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(
        repo_root
            .join("upstream/state")
            .join(format!("{component}.toml")),
    )?;
    let stamp = format!(
        "{state}\n{archive_url}\n{archive_sha256}\n{}\n{}\n",
        dependencies.join("\n"),
        options.join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    let archive =
        ensure_verified_release_archive(&out_root, archive_filename, archive_url, archive_sha256)?;
    if !source_copy.join("configure").is_file() {
        stage_release_source(&archive, &source_copy)?;
    }
    fs::create_dir_all(&build_dir)?;
    let env = staged_library_environment(repo_root, dependencies)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            options,
            &env,
        )?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env,
    )?;
    for relative in required_outputs {
        if !install_dir.join(relative).is_file() {
            bail!("{component} install did not produce {relative}");
        }
    }
    fs::write(stamp_path, stamp)?;
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

fn staged_library_environment(
    repo_root: &Path,
    components: &[&str],
) -> Result<Vec<(&'static str, String)>> {
    let mut include_dirs = Vec::new();
    let mut library_dirs = Vec::new();
    let mut pkgconfig_dirs = Vec::new();
    let mut program_dirs = Vec::new();
    for component in components {
        let usr = repo_root
            .join("out/build")
            .join(component)
            .join("install/usr");
        let include = usr.join("include");
        let bin = usr.join("bin");
        let library = usr.join("lib/x86_64-linux-gnu");
        if include.is_dir() {
            include_dirs.push(include.clone());
        }
        if library.is_dir() {
            pkgconfig_dirs.push(library.join("pkgconfig"));
            library_dirs.push(library);
        }
        let shared_pkgconfig = usr.join("share/pkgconfig");
        if shared_pkgconfig.is_dir() {
            pkgconfig_dirs.push(shared_pkgconfig);
        }
        if bin.is_dir() {
            program_dirs.push(bin);
        }
    }
    let cppflags = include_dirs
        .iter()
        .map(|p| format!("-I{}", p.display()))
        .collect::<Vec<_>>()
        .join(" ");
    let ldflags = library_dirs
        .iter()
        .map(|p| format!("-L{} -Wl,-rpath-link,{}", p.display(), p.display()))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(vec![
        ("CPPFLAGS", cppflags),
        ("LDFLAGS", ldflags),
        (
            "LIBRARY_PATH",
            std::env::join_paths(&library_dirs)?
                .to_string_lossy()
                .to_string(),
        ),
        (
            "LD_LIBRARY_PATH",
            std::env::join_paths(&library_dirs)?
                .to_string_lossy()
                .to_string(),
        ),
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths(&pkgconfig_dirs)?
                .to_string_lossy()
                .to_string(),
        ),
        // Do not fall back to host .pc files. Native runtime stages are built
        // only against previously produced MattOS development metadata.
        (
            "PKG_CONFIG_LIBDIR",
            std::env::join_paths(&pkgconfig_dirs)?
                .to_string_lossy()
                .to_string(),
        ),
        (
            "PATH",
            std::env::join_paths(
                &program_dirs
                    .iter()
                    .cloned()
                    .chain(std::env::split_paths(
                        &std::env::var_os("PATH").unwrap_or_default(),
                    ))
                    .collect::<Vec<_>>(),
            )?
            .to_string_lossy()
            .to_string(),
        ),
    ])
}

fn build_autotools_import(
    repo_root: &Path,
    component: &str,
    source_relative: &str,
    dependencies: &[&str],
    options: &[&str],
    required_outputs: &[&str],
) -> Result<()> {
    let source = repo_root.join(source_relative);
    let out_root = repo_root.join("out/build").join(component);
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(
        repo_root
            .join("upstream/state")
            .join(format!("{component}.toml")),
    )?;
    let adaptation_stamp = match component {
        "networkmanager" => "output-policy-install-adaptation-v4",
        "readline" => "output-pkgconfig-adaptation-v1",
        "ostree" => "output-submodule-and-docs-staging-adaptation-v5",
        _ => "",
    };
    let stamp = format!(
        "{state}\n{}\ndependencies={}\n{adaptation_stamp}\n",
        options.join("\n"),
        dependencies.join(",")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if component == "ostree" {
        // The release repository keeps this generated include out of the
        // source tree.  Materialize it in the output mirror before
        // autoreconf; authoritative imported source remains unchanged.
        for (directory, template_name, variable) in [
            ("libglnx", "Makefile-libglnx.am", "$$(libglnx_srcpath)"),
            ("bsdiff", "Makefile-bsdiff.am", "$$(libbsdiff_srcpath)"),
        ] {
            let generated = source_copy
                .join(directory)
                .join(format!("{template_name}.inc"));
            if !generated.is_file() {
                let template = fs::read_to_string(source_copy.join(directory).join(template_name))?;
                fs::write(generated, template.replace(variable, directory))?;
            }
        }
        // gtk-doc is disabled for the target package, but automake still
        // parses the conditional apidoc makefile and requires this generated
        // include to exist during autoreconf.
        let gtk_doc_make = source_copy.join("gtk-doc.make");
        if !gtk_doc_make.is_file() {
            fs::write(gtk_doc_make, "# gtk-doc disabled in this MattOS build\n")?;
        }
        let makefile = source_copy.join("Makefile.am");
        let make_contents = fs::read_to_string(&makefile)?;
        let make_without_apidoc = make_contents.replace(
            "if ENABLE_GTK_DOC\nSUBDIRS += apidoc\nendif\n",
            "# gtk-doc disabled in this MattOS build\n",
        );
        if make_without_apidoc != make_contents {
            fs::write(makefile, make_without_apidoc)?;
        }
        let configure = source_copy.join("configure.ac");
        let configure_contents = fs::read_to_string(&configure)?;
        let configure_without_apidoc = configure_contents.replace("apidoc/Makefile\n", "");
        if configure_without_apidoc != configure_contents {
            fs::write(configure, configure_without_apidoc)?;
        }
        let syscall_header = source_copy.join("libglnx/glnx-missing-syscall.h");
        let syscall_contents = fs::read_to_string(&syscall_header)?;
        let syscall_fixed = syscall_contents.replace(
            "#if !HAVE_DECL_NAME_TO_HANDLE_AT && defined(__NR_name_to_handle_at)",
            "#if defined(HAVE_DECL_NAME_TO_HANDLE_AT) && !HAVE_DECL_NAME_TO_HANDLE_AT && defined(__NR_name_to_handle_at)",
        );
        if syscall_fixed != syscall_contents {
            fs::write(syscall_header, syscall_fixed)?;
        }
        let dump = source_copy.join("src/ostree/ot-dump.c");
        let dump_contents = fs::read_to_string(&dump)?;
        let dump_fixed = dump_contents
            .replace("#include <bsd/err.h>", "#include <err.h>")
            .replace(
                "errx (1, \"Failed to read commit: %s\",",
                "g_error (\"Failed to read commit: %s\",",
            );
        if dump_fixed != dump_contents {
            fs::write(dump, dump_fixed)?;
        }
        let err_compat = source_copy.join("mattos-err-compat.h");
        fs::write(
            &err_compat,
            "#ifndef MATTOS_OSTREE_ERR_COMPAT_H\n#define MATTOS_OSTREE_ERR_COMPAT_H\n#include <stdarg.h>\nvoid err(int, const char *, ...);\nvoid errx(int, const char *, ...);\n#endif\n",
        )?;
    }
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "autoreconf", &["-fiv"])?;
    }
    let mut env = staged_library_environment(repo_root, dependencies)?;
    if component == "ostree" {
        // libbsd's compatibility headers include the target libc headers by
        // their normal names.  Its nested `include/bsd` directory must not
        // be placed on the general include search path: doing so makes
        // <sys/cdefs.h> resolve to bsd/sys/cdefs.h and recurse into itself
        // under the MattOS sysroot.  Keep libbsd's public root available and
        // link it explicitly below, but remove only this accidental nested
        // include directory from the generated environment.
        let libbsd_nested = repo_root
            .join("out/build/libbsd/install/usr/include/bsd")
            .display()
            .to_string();
        for (key, value) in &mut env {
            if *key == "CPPFLAGS" {
                *value = value
                    .split_whitespace()
                    .filter(|flag| *flag != format!("-I{libbsd_nested}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                value.push_str(&format!(
                    " -include {}",
                    source_copy.join("mattos-err-compat.h").display()
                ));
            }
        }
        // e2p is part of the target-owned e2fsprogs development install,
        // which is produced as an installer sub-output rather than a
        // standalone BuildStage.
        let e2fs_usr = repo_root.join("out/build/e2fsprogs/install/usr");
        let e2fs_include = e2fs_usr.join("include");
        let e2fs_lib = e2fs_usr.join("lib/x86_64-linux-gnu");
        let e2fs_pc = e2fs_lib.join("pkgconfig");
        for (key, value) in &mut env {
            if *key == "CPPFLAGS" {
                value.push_str(&format!(" -I{}", e2fs_include.display()));
            } else if *key == "LDFLAGS" {
                value.push_str(&format!(
                    " -L{} -Wl,-rpath-link,{}",
                    e2fs_lib.display(),
                    e2fs_lib.display()
                ));
            } else if *key == "LIBRARY_PATH" || *key == "LD_LIBRARY_PATH" {
                *value = format!("{}:{}", e2fs_lib.display(), value);
            } else if *key == "PKG_CONFIG_PATH" || *key == "PKG_CONFIG_LIBDIR" {
                *value = format!("{}:{}", e2fs_pc.display(), value);
            }
        }
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            options,
            &env,
        )?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env,
    )?;
    for relative in required_outputs {
        if !install_dir.join(relative).is_file() {
            bail!("{component} install did not produce {relative}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_file(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "file",
        "src/userland/file",
        &["zlib"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
        ],
        &[
            "usr/bin/file",
            "usr/lib/x86_64-linux-gnu/libmagic.so.1",
            "usr/share/misc/magic.mgc",
        ],
    )
}

fn build_less(repo_root: &Path) -> Result<()> {
    build_release_autotools_program(
        repo_root,
        "less",
        "less-704.tar.gz",
        LESS_RELEASE_ARCHIVE_URL,
        LESS_RELEASE_ARCHIVE_SHA256,
        &["ncurses", "pcre2"],
        &["--prefix=/usr", "--sysconfdir=/etc", "--with-regex=pcre2"],
        &["usr/bin/less", "usr/bin/lesskey", "usr/libexec/lessecho"],
    )
}

fn build_git(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/git");
    let out_root = repo_root.join("out/build/git");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    let env = staged_library_environment(
        repo_root,
        &["curl", "expat", "openssl", "zlib", "zstd", "pcre2"],
    )?;
    let curl_config = repo_root.join("out/build/curl/install/usr/bin/curl-config");
    if !curl_config.is_file() {
        bail!(
            "Git requires MattOS curl-config at {}",
            curl_config.display()
        );
    }
    let common = vec![
        "prefix=/usr".to_string(),
        "NO_GETTEXT=YesPlease".to_string(),
        "NO_TCLTK=YesPlease".to_string(),
        "NO_PERL=YesPlease".to_string(),
        "NO_PYTHON=YesPlease".to_string(),
        "NO_RUST=YesPlease".to_string(),
        "USE_LIBPCRE2=YesPlease".to_string(),
        format!("CURL_CONFIG={}", curl_config.display()),
        "CURL_LDFLAGS=-lcurl".to_string(),
    ];
    let mut build_args = vec!["-j", "4"];
    build_args.extend(common.iter().map(String::as_str));
    run_cmd_with_env_overrides(&source_copy, "make", &build_args, &env)?;
    remove_path_if_exists(&install_dir)?;
    let destdir = format!("DESTDIR={}", install_dir.display());
    let mut install_args = vec!["install", destdir.as_str()];
    install_args.extend(common.iter().map(String::as_str));
    run_cmd_with_env_overrides(&source_copy, "make", &install_args, &env)?;
    for rel in ["usr/bin/git", "usr/libexec/git-core/git-remote-https"] {
        if !install_dir.join(rel).is_file() {
            bail!("Git install did not produce {rel}");
        }
    }
    Ok(())
}

fn build_openssh(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "openssh",
        "src/system/network/openssh-portable",
        &["openssl", "zlib", "zstd", "linux-pam", "libxcrypt"],
        &[
            "--prefix=/usr",
            "--sysconfdir=/etc/ssh",
            "--sbindir=/usr/sbin",
            "--libexecdir=/usr/lib/openssh",
            "--with-pam",
            "--with-privsep-path=/run/sshd",
            "--with-privsep-user=sshd",
            "--with-default-path=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        ],
        &["usr/bin/ssh", "usr/sbin/sshd", "usr/bin/ssh-keygen"],
    )
}

fn build_libffi(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "libffi",
        "src/system/libraries/libffi/libffi",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
            "--disable-docs",
            "--disable-multi-os-directory",
        ],
        &[
            "usr/lib/x86_64-linux-gnu/libffi.so.8",
            "usr/include/ffi.h",
            "usr/include/ffitarget.h",
        ],
    )
}

/// Build the Wayland client runtime needed by the native COSMIC installer.
/// Winit loads libwayland-client with dlopen, so it is not visible to the ELF
/// NEEDED audit and must be represented as an explicit source-built runtime
/// dependency rather than falling back to a host library.
fn build_wayland(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "wayland",
        "src/system/libraries/wayland",
        &["libffi"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dlibraries=true",
            // The source tree uses its own scanner to generate the protocol
            // glue for the libraries.  Build it in the output mirror; it is
            // deliberately not shipped by the runtime package.
            "-Dscanner=true",
            "-Dtests=false",
            "-Ddocumentation=false",
            "-Ddtd_validation=false",
        ],
        "usr/lib/x86_64-linux-gnu/libwayland-client.so.0",
        &[],
    )
}

/// Build only libxkbcommon itself.  The native COSMIC installer dynamically
/// needs `libxkbcommon.so.0`; X11 helpers, Wayland helper tools, registry,
/// documentation, and shell completion are deliberately not source-closure
/// requirements for this runtime library.
fn build_xkbcommon(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/xkbcommon");
    if !source.join("meson.build").is_file() {
        bail!(
            "xkbcommon source not found in {}; run upstream import xkbcommon first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/xkbcommon");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/xkbcommon.toml"))?;
    let options = [
        "--prefix=/usr",
        "--libdir=lib/x86_64-linux-gnu",
        "-Denable-tools=false",
        "-Denable-x11=false",
        "-Denable-wayland=false",
        "-Denable-xkbregistry=false",
        "-Denable-docs=false",
        "-Denable-bash-completion=false",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !build_dir.join("build.ninja").is_file() {
        let mut args = vec!["setup", path_str(&build_dir)?, path_str(&source_copy)?];
        args.extend(options);
        run_cmd(repo_root, "meson", &args)?;
    } else {
        // Meson serializes its internal build model.  A build directory made
        // by an older Meson can still have build.ninja while meson compile
        // rejects build.dat; reconfigure the derived directory before use.
        let mut args = vec![
            "setup",
            "--reconfigure",
            path_str(&build_dir)?,
            path_str(&source_copy)?,
        ];
        args.extend(options);
        run_cmd(repo_root, "meson", &args)?;
    }
    run_cmd(
        repo_root,
        "ninja",
        &["-C", path_str(&build_dir)?, "libxkbcommon.so.0.9.2"],
    )?;
    remove_path_if_exists(&install_dir)?;
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
            "--tags",
            "runtime,devel",
        ],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libxkbcommon.so.0");
    if !soname.is_file() {
        bail!("xkbcommon install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)?;
    println!(
        "xkbcommon origin: {}; features=x11,wayland,tools,registry,docs disabled",
        install_dir.display()
    );
    Ok(())
}

/// Build generated XKB rules in an output-owned mirror.  The pinned upstream
/// Git tree contains source fragments; `rules/evdev` is a Meson output and
/// must never be generated inside the authoritative import.
fn build_xkeyboard_config(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/xkeyboard-config");
    if !source.join("meson.build").is_file() {
        bail!(
            "xkeyboard-config source not found in {}; run upstream import xkeyboard-config first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/xkeyboard-config");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/xkeyboard-config.toml"))?;
    let options = ["--prefix=/usr", "--datadir=share", "-Dnls=false"];
    // Meson serializes its own version-sensitive state in build.dat.  Include
    // the active Meson identity in this output-owned stamp so a host Meson
    // upgrade cannot leave us reusing an incompatible build directory.
    let meson_version = run_cmd_capture(repo_root, "meson", &["--version"])?;
    let stamp = format!(
        "{state}\n{}\nmeson-version={meson_version}\n",
        options.join("\n")
    );
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !build_dir.join("build.ninja").is_file() {
        let mut args = vec!["setup", path_str(&build_dir)?, path_str(&source_copy)?];
        args.extend(options);
        run_cmd(repo_root, "meson", &args)?;
    }
    run_cmd(
        repo_root,
        "meson",
        &["compile", "-C", path_str(&build_dir)?],
    )?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            path_str(&build_dir)?,
            "--destdir",
            path_str(&install_dir)?,
        ],
    )?;
    let rules = install_dir.join("usr/share/xkeyboard-config-2/rules/evdev");
    let legacy_root = install_dir.join("usr/share/X11/xkb");
    if !rules.is_file() || !legacy_root.is_symlink() {
        bail!("xkeyboard-config install did not produce generated rules or the legacy XKB symlink");
    }
    fs::write(&stamp_path, stamp)?;
    println!(
        "xkeyboard-config origin: {}; generated XKB rules in output-owned mirror",
        install_dir.display()
    );
    Ok(())
}

/// Build a Meson-based native runtime in an output-owned mirror.  Native
/// installer libraries must never be configured against host headers or .pc
/// files: every dependency is an earlier MattOS stage and pkg-config's default
/// search path is deliberately disabled.
fn build_meson_runtime(
    repo_root: &Path,
    component: &str,
    source_relative: &str,
    dependencies: &[&str],
    options: &[&str],
    required_output: &str,
    extra_env: &[(&str, String)],
) -> Result<()> {
    let source = repo_root.join(source_relative);
    if !source.join("meson.build").is_file() {
        bail!(
            "{component} source not found in {}; run its upstream import first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build").join(component);
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let state = fs::read_to_string(
        repo_root
            .join("upstream/state")
            .join(format!("{component}.toml")),
    )?;
    let adaptation_stamp = match component {
        "networkmanager" => "output-policy-install-adaptation-v4",
        "polkit" => "output-duktape-link-adaptation-v2",
        "appstream" => "output-source-closure-adaptation-v2",
        _ => "",
    };
    // Meson stores compiler/build-tool state in build.dat.  A cache miss can
    // be caused by a changed dependency output while the component's own
    // recipe stamp remains unchanged; reusing that old Meson directory can
    // then fail (or, worse, consume stale dependency metadata).  Bind the
    // output-owned build directory to the actual producer output digests so
    // dependency changes force a fresh configure before compilation.
    let dependency_outputs = dependencies
        .iter()
        .map(|dependency| {
            let manifest = stage_cache::read_stage_manifest(repo_root, dependency)
                .with_context(|| format!("failed to read {dependency} dependency manifest"))?;
            Ok::<_, anyhow::Error>(format!("{dependency}={}", manifest.output_content_digest))
        })
        .collect::<Result<Vec<_>>>()?;
    let stamp = format!(
        "{state}\n{}\ndependencies={}\ndependency-outputs={}\n{adaptation_stamp}\n",
        options.join("\n"),
        dependencies.join(","),
        dependency_outputs.join(",")
    );
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    for dependency in dependencies {
        rewrite_staged_pkgconfig_files(
            &repo_root.join("out/build").join(dependency).join("install"),
        )?;
    }
    sync_build_source(&source, &source_copy)?;
    if component == "appstream" {
        // The host does not provide itstool.  AppStream's untranslated
        // release-note metadata is still a valid source-owned artifact, so
        // replace only the output mirror's optional localization join with a
        // deterministic install of that upstream XML.  The authoritative
        // imported source remains untouched.
        let data_meson = source_copy.join("data/meson.build");
        let body = fs::read_to_string(&data_meson)?;
        let start = body
            .find("metainfo_i18n = i18n.itstool_join(")
            .context("AppStream data layout changed: missing itstool join")?;
        let end = body[start..]
            .find("\n\n")
            .map(|offset| start + offset)
            .context("AppStream data layout changed: unterminated itstool join")?;
        let replacement = "metainfo_i18n = files('org.freedesktop.appstream.cli.metainfo.xml')\ninstall_data(metainfo_i18n, install_dir: metainfo_dir)";
        let adapted = format!("{}{}{}", &body[..start], replacement, &body[end..]);
        fs::write(data_meson, adapted)?;
    }
    if component == "networkmanager" {
        let data_meson = source_copy.join("data/meson.build");
        let body = fs::read_to_string(&data_meson)?.replace(
            r#"  i18n.merge_file(
    input: 'org.freedesktop.NetworkManager.policy.in',
    output: '@BASENAME@',
    po_dir: po_dir,
    install: true,
    install_dir: polkit_policydir,
  )"#,
            r#"  install_data(
    'org.freedesktop.NetworkManager.policy.in',
    rename: 'org.freedesktop.NetworkManager.policy',
    install_dir: polkit_policydir,
  )"#,
        );
        fs::write(data_meson, body)?;
        let root_meson = source_copy.join("meson.build");
        let body = fs::read_to_string(&root_meson)?.replace(
            "readline_dep = declare_dependency(link_args: '-lreadline')",
            "readline_dep = declare_dependency(link_args: ['-lreadline', '-lncursesw', '-ltinfow'])",
        );
        fs::write(root_meson, body)?;
    }
    if component == "polkit" {
        let meson = source_copy.join("meson.build");
        let body = fs::read_to_string(&meson)?;
        let old = "  js_dep = dependency('duktape', version: duktape_req_version, required: false)\n  if not js_dep.found()\n    message('Falling back to looking for library and header...')\n    js_dep = cc.find_library('duktape', has_headers: ['duktape.h'], required: true)\n  endif";
        let replacement = format!(
            "  js_dep = declare_dependency(compile_args: ['-I{}'], link_args: ['-lduktape'])",
            repo_root
                .join("out/build/duktape/install/usr/include")
                .display()
        );
        if !body.contains(old) {
            bail!("polkit Duktape dependency block changed unexpectedly");
        }
        let body = body.replace(old, &replacement);
        fs::write(meson, body)?;
    }
    let mut env = staged_library_environment(repo_root, dependencies)?;
    if component == "flatpak" {
        if let Some((_, flags)) = env.iter_mut().find(|(key, _)| *key == "LDFLAGS") {
            flags.push_str(&format!(
                " -Wl,--no-as-needed {} {} -Wl,--as-needed",
                repo_root
                    .join("out/build/libxmlb/install/usr/lib/x86_64-linux-gnu/libxmlb.so.2")
                    .display(),
                repo_root
                    .join("out/build/libfyaml/install/usr/lib/x86_64-linux-gnu/libfyaml.so.0")
                    .display()
            ));
        }
    }
    env.extend(extra_env.iter().map(|(key, value)| (*key, value.clone())));
    if !build_dir.join("build.ninja").is_file() {
        let mut args = vec!["setup", path_str(&build_dir)?, path_str(&source_copy)?];
        args.extend(options.iter().copied());
        run_cmd_with_env_overrides(repo_root, "meson", &args, &env)?;
    }
    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &["compile", "-C", path_str(&build_dir)?],
        &env,
    )?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            path_str(&build_dir)?,
            "--destdir",
            path_str(&install_dir)?,
        ],
        &env,
    )?;
    rewrite_staged_pkgconfig_files(&install_dir)?;
    let required = install_dir.join(required_output);
    if !required.is_file() {
        bail!("{component} install did not produce {}", required.display());
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_libseat(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "seatd",
        "src/system/libraries/seatd",
        &["systemd"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dserver=disabled",
            "-Dlibseat-seatd=disabled",
            "-Dlibseat-logind=systemd",
            "-Dlibseat-builtin=enabled",
            "-Dexamples=disabled",
            "-Dman-pages=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libseat.so.1",
        &[],
    )
}

fn rewrite_staged_pkgconfig_files(install_dir: &Path) -> Result<()> {
    fn visit(path: &Path, prefix: &Path) -> Result<()> {
        if !path.is_dir() {
            return Ok(());
        }
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                visit(&path, prefix)?;
            } else if metadata.is_file() && path.extension().and_then(OsStr::to_str) == Some("pc") {
                let contents = fs::read_to_string(&path)?;
                let rewritten = contents
                    .lines()
                    .map(|line| {
                        if let Some(value) = line.strip_prefix("prefix=/usr") {
                            format!("prefix={}{}", prefix.display(), value)
                        } else if let Some(value) = line.strip_prefix("libdir=/usr") {
                            format!("libdir=${{prefix}}{}", value)
                        } else if let Some(value) = line.strip_prefix("includedir=/usr") {
                            format!("includedir=${{prefix}}{}", value)
                        } else {
                            line.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n";
                fs::write(&path, rewritten)?;
            }
        }
        Ok(())
    }
    visit(install_dir, &install_dir.join("usr"))
}

fn remove_staged_libtool_archives(path: &Path) -> Result<()> {
    if !path.is_dir() {
        return Ok(());
    }
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            remove_staged_libtool_archives(&path)?;
        } else if metadata.is_file() && path.extension().and_then(OsStr::to_str) == Some("la") {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn build_xorg_autotools_component(
    repo_root: &Path,
    component: &str,
    dependencies: &[&str],
    options: &[&str],
    required_outputs: &[&str],
) -> Result<()> {
    let source = repo_root.join("src/system/graphics").join(component);
    let out_root = repo_root.join("out/build").join(component);
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(
        repo_root
            .join("upstream/state")
            .join(format!("{component}.toml")),
    )?;
    let stamp = format!(
        "{state}\n{}\ndependencies={}\nxorg-compat-recipe=2\n",
        options.join("\n"),
        dependencies.join(",")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
        remove_path_if_exists(&install_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    let mut env = staged_library_environment(repo_root, dependencies)?;
    let aclocal = repo_root.join("out/build/xorg-util-macros/install/usr/share/aclocal");
    if aclocal.is_dir() {
        env.push(("ACLOCAL_PATH", aclocal.display().to_string()));
    }
    if !source_copy.join("configure").is_file() {
        run_cmd_with_env_overrides(&source_copy, "autoreconf", &["-fiv"], &env)?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            options,
            &env,
        )?;
    }
    let jobs = scheduler::child_job_limit().max(1).to_string();
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", &jobs], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env,
    )?;
    rewrite_staged_pkgconfig_files(&install_dir)?;
    // Libtool archives encode build-time absolute paths and make later Xorg
    // components chase target /usr paths on the host. Shared-library and
    // pkg-config metadata are the canonical output of this runtime closure.
    remove_staged_libtool_archives(&install_dir)?;
    for relative in required_outputs {
        if !install_dir.join(relative).is_file() {
            bail!("{component} install did not produce {relative}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_x11_compat(repo_root: &Path) -> Result<()> {
    let common = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
    ];
    build_xorg_autotools_component(repo_root, "xorg-util-macros", &[], &["--prefix=/usr"], &[])?;
    build_meson_runtime(
        repo_root,
        "xorgproto",
        "src/system/graphics/xorgproto",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dlegacy=true",
        ],
        "usr/include/X11/X.h",
        &[],
    )?;
    rewrite_staged_pkgconfig_files(&repo_root.join("out/build/xorgproto/install"))?;
    build_xorg_autotools_component(
        repo_root,
        "xtrans",
        &["xorg-util-macros", "xorgproto"],
        &common,
        &[],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "libxau",
        &["xorg-util-macros", "xorgproto"],
        &common,
        &["usr/lib/x86_64-linux-gnu/libXau.so.6"],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "libxdmcp",
        &["xorg-util-macros", "xorgproto"],
        &common,
        &["usr/lib/x86_64-linux-gnu/libXdmcp.so.6"],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "xcb-proto",
        &["xorg-util-macros", "cpython"],
        &["--prefix=/usr"],
        &["usr/share/xcb/xproto.xml"],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "libxcb",
        &[
            "xorg-util-macros",
            "xorgproto",
            "libxau",
            "libxdmcp",
            "xcb-proto",
            "cpython",
            "expat",
        ],
        &common,
        &["usr/lib/x86_64-linux-gnu/libxcb.so.1"],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "libx11",
        &[
            "xorg-util-macros",
            "xorgproto",
            "xtrans",
            "libxau",
            "libxdmcp",
            "libxcb",
        ],
        &common,
        &["usr/lib/x86_64-linux-gnu/libX11.so.6"],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "libxext",
        &[
            "xorg-util-macros",
            "xorgproto",
            "libxau",
            "libxdmcp",
            "libxcb",
            "libx11",
        ],
        &common,
        &["usr/lib/x86_64-linux-gnu/libXext.so.6"],
    )?;

    let aggregate = repo_root.join("out/build/x11-compat/install");
    remove_path_if_exists(&aggregate)?;
    for component in ["libxau", "libxdmcp", "libxcb", "libx11", "libxext"] {
        copy_tree_contents(
            &repo_root.join("out/build").join(component).join("install"),
            &aggregate,
        )?;
    }
    for relative in [
        "usr/lib/x86_64-linux-gnu/libX11.so.6",
        "usr/lib/x86_64-linux-gnu/libXext.so.6",
        "usr/lib/x86_64-linux-gnu/libxcb.so.1",
    ] {
        if !aggregate.join(relative).exists() {
            bail!("X11 compatibility runtime did not produce {relative}");
        }
    }
    Ok(())
}

fn build_libglvnd(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libglvnd",
        "src/system/graphics/libglvnd",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dx11=disabled",
            "-Dglx=disabled",
            "-Degl=true",
            "-Dgles1=true",
            "-Dgles2=true",
            "-Dhgl=false",
        ],
        "usr/lib/x86_64-linux-gnu/libEGL.so.1",
        &[],
    )?;
    rewrite_staged_pkgconfig_files(&repo_root.join("out/build/libglvnd/install"))
}

fn nvidia_library_soname(path: &Path) -> Result<String> {
    let output = run_cmd_capture(
        path.parent().context("NVIDIA library has no parent")?,
        "readelf",
        &["-d", path_str(path)?],
    )?;
    output
        .lines()
        .find(|line| line.contains("(SONAME)"))
        .and_then(|line| line.split_once('['))
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(soname, _)| soname.to_owned())
        .with_context(|| format!("NVIDIA library {} has no ELF SONAME", path.display()))
}

fn stage_nvidia_library(source: &Path, destination: &Path) -> Result<()> {
    let filename = source
        .file_name()
        .context("NVIDIA library has no filename")?;
    fs::create_dir_all(destination)?;
    let target = destination.join(filename);
    fs::copy(source, &target)?;
    let soname = nvidia_library_soname(source)?;
    let soname_path = destination.join(&soname);
    if soname_path != target {
        remove_path_if_exists(&soname_path)?;
        std::os::unix::fs::symlink(filename, soname_path)?;
    }
    Ok(())
}

fn render_nvidia_driver_selection(open_device_ids: &BTreeSet<u16>) -> (String, String) {
    let config = "# Generated from NVIDIA 595.84 supported-gpus.json.\n\
# Route both competing drivers through the release-matched hardware gate.\n\
install nvidia /usr/libexec/mattos-nvidia-select nvidia $CMDLINE_OPTS\n\
install nvidia_drm /usr/libexec/mattos-nvidia-select nvidia_drm $CMDLINE_OPTS\n\
install nvidia_modeset /usr/libexec/mattos-nvidia-select nvidia_modeset $CMDLINE_OPTS\n\
install nvidia_uvm /usr/libexec/mattos-nvidia-select nvidia_uvm $CMDLINE_OPTS\n\
install nvidia_peermem /usr/libexec/mattos-nvidia-select nvidia_peermem $CMDLINE_OPTS\n\
install nouveau /usr/libexec/mattos-nvidia-select nouveau $CMDLINE_OPTS\n"
        .to_string();
    let patterns = open_device_ids
        .iter()
        .map(|device| format!("0x{device:04x}"))
        .collect::<Vec<_>>()
        .join("|");
    let selector = format!(
        "#!/bin/sh\n\
set -eu\n\
module=$1\n\
shift\n\
supported=0\n\
devices=${{MATTOS_NVIDIA_SYSFS_ROOT:-/sys/bus/pci/devices}}\n\
for path in \"$devices\"/*; do\n\
    [ -d \"$path\" ] || continue\n\
    [ \"$(cat \"$path/vendor\" 2>/dev/null || true)\" = 0x10de ] || continue\n\
    device=$(tr 'A-F' 'a-f' < \"$path/device\" 2>/dev/null || true)\n\
    case \"$device\" in\n\
        {patterns}) supported=1; break ;;\n\
    esac\n\
done\n\
case \"$module\" in\n\
    nouveau) [ \"$supported\" -eq 0 ] || exit 1 ;;\n\
    nvidia*) [ \"$supported\" -eq 1 ] || exit 1 ;;\n\
    *) exit 2 ;;\n\
esac\n\
exec \"${{MATTOS_MODPROBE:-/usr/sbin/modprobe}}\" --ignore-install \"$module\" \"$@\"\n"
    );
    (config, selector)
}

fn build_nvidia_driver(repo_root: &Path) -> Result<()> {
    let manifest_path = repo_root.join("src/system/graphics/nvidia-driver/manifest.toml");
    let manifest_body = fs::read_to_string(&manifest_path)?;
    let manifest: NvidiaDriverManifest = toml::from_str(&manifest_body)?;
    if manifest.schema_version != 1
        || manifest.version != "595.84"
        || manifest.release_branch != "production"
        || manifest.architecture != "x86_64"
        || manifest.kernel_source_commit != "722ae84526a09ed672fbe75448e2909834ba4cce"
        || manifest.binary_policy != "verbatim-extraction-no-strip-no-patch"
        || !manifest.include_in_iso
    {
        bail!("NVIDIA driver manifest does not match MattOS's pinned production policy");
    }
    let out_root = repo_root.join("out/build/nvidia-driver");
    fs::create_dir_all(&out_root)?;
    let runfile = ensure_verified_release_archive(
        &out_root,
        &manifest.runfile,
        &manifest.url,
        &manifest.sha256,
    )?;
    let extracted = out_root.join("source");
    let extraction_stamp = out_root.join("extraction.stamp");
    if fs::read_to_string(&extraction_stamp).ok().as_deref() != Some(manifest.sha256.as_str())
        || !extracted.join("LICENSE").is_file()
    {
        remove_path_if_exists(&extracted)?;
        run_cmd(
            &out_root,
            "sh",
            &[
                path_str(&runfile)?,
                "--extract-only",
                "--target",
                path_str(&extracted)?,
            ],
        )?;
        fs::write(&extraction_stamp, &manifest.sha256)?;
    }
    let license_hash = performance::sha256_file(&extracted.join("LICENSE"))?;
    if license_hash != manifest.license_sha256 {
        bail!(
            "NVIDIA license checksum mismatch: expected {}, got {license_hash}",
            manifest.license_sha256
        );
    }

    let release = fs::read_to_string(repo_root.join("out/build/linux/kernel-release"))?
        .trim()
        .to_owned();
    let kernel_source = repo_root.join("out/build/linux/source");
    let kernel_output = repo_root.join("out/build/linux/build");
    if !kernel_output
        .join("include/config/kernel.release")
        .is_file()
    {
        bail!("NVIDIA modules require the prepared MattOS kernel output");
    }
    let open_source = repo_root.join("out/build/nvidia-driver/kernel-source");
    let open_stamp_path = out_root.join("kernel-source.stamp");
    let open_state =
        fs::read_to_string(repo_root.join("upstream/state/nvidia-open-gpu-kernel-modules.toml"))?;
    let open_stamp = format!("{open_state}\nkernel-release={release}\nrecipe=2\n");
    if fs::read_to_string(&open_stamp_path).ok().as_deref() != Some(open_stamp.as_str()) {
        remove_path_if_exists(&open_source)?;
        sync_build_source(
            &repo_root.join("src/system/graphics/nvidia-open-gpu-kernel-modules"),
            &open_source,
        )?;
        apply_component_patches(repo_root, "nvidia-open-gpu-kernel-modules", &open_source)?;
        fs::write(&open_stamp_path, &open_stamp)?;
    }
    let jobs = scheduler::child_job_limit().max(1).to_string();
    let sys_source = format!("SYSSRC={}", kernel_source.display());
    let sys_output = format!("SYSOUT={}", kernel_output.display());
    run_cmd(
        &open_source,
        "make",
        &[
            "modules",
            "-j",
            &jobs,
            &sys_source,
            &sys_output,
            // Linux 7.2's delayed final-link objtool pass cannot rewrite the
            // immutable precompiled NVIDIA core. Per-object objtool checking
            // remains enabled for every source-built open-module object.
            "delay-objtool=",
        ],
    )?;
    let raw_install = out_root.join("modules-install");
    remove_path_if_exists(&raw_install)?;
    let install_mod_path = format!("INSTALL_MOD_PATH={}", raw_install.display());
    run_cmd(
        &open_source,
        "make",
        &[
            "modules_install",
            &sys_source,
            &sys_output,
            &install_mod_path,
            "INSTALL_MOD_DIR=updates/nvidia",
            "DEPMOD=true",
            "delay-objtool=",
        ],
    )?;

    let install = out_root.join("install");
    remove_path_if_exists(&install)?;
    let raw_module_root = raw_install.join("lib/modules").join(&release);
    let module_root = install.join("usr/lib/modules").join(&release);
    copy_tree_contents(&raw_module_root, &module_root)?;
    for link in ["build", "source"] {
        remove_path_if_exists(&module_root.join(link))?;
    }
    let mut module_files = Vec::new();
    collect_regular_files(&module_root, &mut module_files)?;
    let mut module_count = 0usize;
    for module in module_files.into_iter().filter(|path| {
        path.extension().and_then(OsStr::to_str) == Some("ko")
            || path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.ends_with(".ko.zst"))
    }) {
        let vermagic = run_cmd_capture(
            repo_root,
            "modinfo",
            &["-F", "vermagic", path_str(&module)?],
        )?;
        if !vermagic.starts_with(&release) {
            bail!(
                "{} has mismatched vermagic {}",
                module.display(),
                vermagic.trim()
            );
        }
        if module.extension().and_then(OsStr::to_str) == Some("ko") {
            let compressed = PathBuf::from(format!("{}.zst", module.display()));
            run_cmd(
                repo_root,
                "zstd",
                &[
                    "-q",
                    "-19",
                    "-T1",
                    "-f",
                    path_str(&module)?,
                    "-o",
                    path_str(&compressed)?,
                ],
            )?;
            remove_path_if_exists(&module)?;
        }
        module_count += 1;
    }
    if module_count != 5 {
        bail!("NVIDIA open module install produced {module_count} modules, expected 5");
    }
    run_cmd(
        repo_root,
        "depmod",
        &[
            "-b",
            path_str(&install)?,
            "-m",
            "/usr/lib/modules",
            &release,
        ],
    )?;

    let libdir = install.join("usr/lib/x86_64-linux-gnu");
    for filename in [
        "libEGL_nvidia.so.595.84",
        "libGLESv1_CM_nvidia.so.595.84",
        "libGLESv2_nvidia.so.595.84",
        "libGLX_nvidia.so.595.84",
        "libcuda.so.595.84",
        "libnvcuvid.so.595.84",
        "libnvidia-allocator.so.595.84",
        "libnvidia-egl-gbm.so.1.1.3",
        "libnvidia-egl-wayland.so.1.1.20",
        "libnvidia-egl-wayland2.so.1.0.1",
        "libnvidia-eglcore.so.595.84",
        "libnvidia-encode.so.595.84",
        "libnvidia-glcore.so.595.84",
        "libnvidia-glsi.so.595.84",
        "libnvidia-glvkspirv.so.595.84",
        "libnvidia-gpucomp.so.595.84",
        "libnvidia-ml.so.595.84",
        "libnvidia-present.so.595.84",
        "libnvidia-ptxjitcompiler.so.595.84",
        "libnvidia-tls.so.595.84",
    ] {
        stage_nvidia_library(&extracted.join(filename), &libdir)?;
    }
    for filename in ["nvidia-smi", "nvidia-modprobe", "nvidia-persistenced"] {
        let destination = install.join("usr/bin").join(filename);
        fs::create_dir_all(destination.parent().expect("NVIDIA binary parent"))?;
        fs::copy(extracted.join(filename), &destination)?;
        set_mode(
            destination,
            if filename == "nvidia-modprobe" {
                0o4755
            } else {
                0o755
            },
        )?;
    }
    for (source_name, destination_relative) in [
        (
            "10_nvidia.json",
            "usr/share/glvnd/egl_vendor.d/10_nvidia.json",
        ),
        ("nvidia_icd.json", "usr/share/vulkan/icd.d/nvidia_icd.json"),
        (
            "nvidia_layers.json",
            "usr/share/vulkan/implicit_layer.d/nvidia_layers.json",
        ),
        (
            "09_nvidia_wayland2.json",
            "usr/share/egl/egl_external_platform.d/09_nvidia_wayland2.json",
        ),
        (
            "10_nvidia_wayland.json",
            "usr/share/egl/egl_external_platform.d/10_nvidia_wayland.json",
        ),
        (
            "15_nvidia_gbm.json",
            "usr/share/egl/egl_external_platform.d/15_nvidia_gbm.json",
        ),
    ] {
        let destination = install.join(destination_relative);
        fs::create_dir_all(destination.parent().expect("NVIDIA metadata parent"))?;
        fs::copy(extracted.join(source_name), destination)?;
    }
    let firmware_dir = install.join("usr/lib/firmware/nvidia/595.84");
    fs::create_dir_all(&firmware_dir)?;
    for firmware in ["gsp_tu10x.bin", "gsp_ga10x.bin"] {
        fs::copy(
            extracted.join("firmware").join(firmware),
            firmware_dir.join(firmware),
        )?;
    }
    let supported_gpu_source = extracted.join("supported-gpus/supported-gpus.json");
    let supported_gpu_data: serde_json::Value =
        serde_json::from_slice(&fs::read(&supported_gpu_source)?)?;
    let mut open_device_ids = BTreeSet::new();
    for chip in supported_gpu_data["chips"]
        .as_array()
        .context("NVIDIA supported GPU manifest has no chips array")?
    {
        let is_open = chip.get("legacybranch").is_none()
            && chip["features"]
                .as_array()
                .is_some_and(|features| features.iter().any(|feature| feature == "kernelopen"));
        if !is_open {
            continue;
        }
        let raw = chip["devid"]
            .as_str()
            .context("NVIDIA supported GPU entry has no devid")?;
        let device = u16::from_str_radix(raw.trim_start_matches("0x"), 16)
            .with_context(|| format!("invalid NVIDIA device ID {raw}"))?;
        open_device_ids.insert(device);
    }
    if open_device_ids.len() < 100
        || !open_device_ids.contains(&0x1e04)
        || open_device_ids.contains(&0x1b80)
    {
        bail!("NVIDIA kernelopen GPU selection is missing Turing or includes Pascal");
    }
    let (selection_config, selector) = render_nvidia_driver_selection(&open_device_ids);
    let modprobe_dir = install.join("usr/lib/modprobe.d");
    fs::create_dir_all(&modprobe_dir)?;
    fs::write(
        modprobe_dir.join("nvidia-supported-gpus.conf"),
        selection_config,
    )?;
    let selector_path = install.join("usr/libexec/mattos-nvidia-select");
    fs::create_dir_all(selector_path.parent().expect("NVIDIA selector parent"))?;
    fs::write(&selector_path, selector)?;
    set_mode(selector_path, 0o755)?;
    let doc = install.join("usr/share/doc/nvidia-driver-595");
    fs::create_dir_all(&doc)?;
    fs::copy(extracted.join("LICENSE"), doc.join("LICENSE"))?;
    fs::copy(&manifest_path, doc.join("manifest.toml"))?;
    fs::copy(
        repo_root.join("src/system/graphics/nvidia-driver/README.md"),
        doc.join("README.md"),
    )?;
    fs::copy(&supported_gpu_source, doc.join("supported-gpus.json"))?;
    fs::copy(
        extracted.join("supported-gpus/LICENSE"),
        doc.join("supported-gpus.LICENSE"),
    )?;
    fs::write(
        out_root.join("runfile.sha256"),
        format!("{}  {}\n", manifest.sha256, manifest.runfile),
    )?;
    fs::write(
        doc.join("runfile.sha256"),
        fs::read(out_root.join("runfile.sha256"))?,
    )?;
    Ok(())
}

fn build_libdisplay_info(repo_root: &Path) -> Result<()> {
    // libdisplay-info otherwise reads /usr/share/hwdata/pnp.ids at configure
    // time. Supply a tiny output-owned pkg-config descriptor pointing at the
    // imported, pinned hwdata data instead of ever consulting the host.
    let hwdata_root = repo_root.join("out/build/libdisplay-info/hwdata");
    fs::create_dir_all(hwdata_root.join("pkgconfig"))?;
    fs::copy(
        repo_root.join("src/system/data/hwdata/pnp.ids"),
        hwdata_root.join("pnp.ids"),
    )?;
    fs::write(
        hwdata_root.join("pkgconfig/hwdata.pc"),
        format!(
            "prefix={}\npkgdatadir=${{prefix}}\nName: hwdata\nDescription: pinned MattOS hardware data\nVersion: 0.410\n",
            hwdata_root.display()
        ),
    )?;
    build_meson_runtime(
        repo_root,
        "libdisplay-info",
        "src/system/libraries/libdisplay-info",
        &[],
        &["--prefix=/usr", "--libdir=lib/x86_64-linux-gnu"],
        "usr/lib/x86_64-linux-gnu/libdisplay-info.so.3",
        &[
            (
                "PKG_CONFIG_PATH",
                hwdata_root.join("pkgconfig").display().to_string(),
            ),
            (
                "PKG_CONFIG_LIBDIR",
                hwdata_root.join("pkgconfig").display().to_string(),
            ),
        ],
    )
}

fn build_libevdev(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libevdev",
        "src/system/libraries/libevdev",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=disabled",
            "-Dtools=disabled",
            "-Ddocumentation=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libevdev.so.2",
        &[],
    )
}

fn build_libinput(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libinput",
        "src/system/libraries/libinput",
        &["libevdev", "systemd"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Ddocumentation=false",
            "-Ddebug-gui=false",
            "-Dlibwacom=false",
            "-Dmtdev=false",
            "-Dlua-plugins=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libinput.so.10",
        &[],
    )
}

fn build_pixman(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "pixman",
        "src/system/libraries/pixman",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=disabled",
            "-Ddemos=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libpixman-1.so.0",
        &[],
    )
}

fn build_libdrm(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libdrm",
        "src/system/libraries/libdrm",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dcairo-tests=disabled",
            "-Dman-pages=disabled",
            // Iris and ANV use the DRM uAPI directly; libdrm_intel is the
            // pre-GEM compatibility helper and would pull in libpciaccess.
            "-Dintel=disabled",
            "-Dradeon=disabled",
            "-Damdgpu=enabled",
            "-Dnouveau=enabled",
            "-Dvmwgfx=enabled",
            "-Dfreedreno=disabled",
            "-Dvc4=disabled",
            "-Detnaviv=disabled",
            "-Dudev=false",
        ],
        "usr/lib/x86_64-linux-gnu/libdrm.so.2",
        &[],
    )
}

fn ensure_pinned_transitive_checkout(root: &Path, repo: &str, commit: &str) -> Result<()> {
    if !root.join(".git").is_dir() {
        remove_path_if_exists(root)?;
        fs::create_dir_all(root.parent().expect("transitive checkout parent"))?;
        run_cmd(
            root.parent().expect("transitive checkout parent"),
            "git",
            &[
                "clone",
                "--filter=blob:none",
                "--no-checkout",
                repo,
                path_str(root)?,
            ],
        )?;
        run_cmd(root, "git", &["checkout", "--detach", commit])?;
    }
    let checked_out = run_cmd_capture(root, "git", &["rev-parse", "HEAD"])?;
    if checked_out.trim() != commit {
        bail!(
            "transitive build input {} is at {}, expected {commit}",
            root.display(),
            checked_out.trim()
        )
    }
    Ok(())
}

fn prepare_mesa_spirv_dependencies(repo_root: &Path) -> Result<PathBuf> {
    const TOOLS_COMMIT: &str = "0539c81f69a3daeb706fd3477dca61435b475156";
    const TOOLS_HEADERS_COMMIT: &str = "ad9184e76a66b1001c29db9b0a3e87f646c64de0";
    const TRANSLATOR_COMMIT: &str = "c88a2e4a1ec77f7adc8916940afd9754c3a30fab";
    const TRANSLATOR_HEADERS_COMMIT: &str = "948a3b0997e2dffea5484b3df7bd5590c5b844cc";

    let root = repo_root.join("out/build/mesa/spirv-deps");
    let tools = root.join("tools");
    let tools_headers = root.join("headers");
    let translator = root.join("translator");
    let translator_headers = root.join("translator-headers");
    ensure_pinned_transitive_checkout(
        &tools,
        "https://github.com/KhronosGroup/SPIRV-Tools.git",
        TOOLS_COMMIT,
    )?;
    ensure_pinned_transitive_checkout(
        &tools_headers,
        "https://github.com/KhronosGroup/SPIRV-Headers.git",
        TOOLS_HEADERS_COMMIT,
    )?;
    ensure_pinned_transitive_checkout(
        &translator,
        "https://github.com/KhronosGroup/SPIRV-LLVM-Translator.git",
        TRANSLATOR_COMMIT,
    )?;
    ensure_pinned_transitive_checkout(
        &translator_headers,
        "https://github.com/KhronosGroup/SPIRV-Headers.git",
        TRANSLATOR_HEADERS_COMMIT,
    )?;

    let install = root.join("install");
    let libdir = install.join("usr/lib/x86_64-linux-gnu");
    let pkgconfig = libdir.join("pkgconfig");
    let tools_build = root.join("tools-build");
    if !pkgconfig.join("SPIRV-Tools.pc").is_file() {
        run_cmd(
            repo_root,
            "cmake",
            &[
                "-S",
                path_str(&tools)?,
                "-B",
                path_str(&tools_build)?,
                "-G",
                "Ninja",
                "-DCMAKE_BUILD_TYPE=Release",
                "-DCMAKE_INSTALL_PREFIX=/usr",
                "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
                &format!("-DSPIRV-Headers_SOURCE_DIR={}", tools_headers.display()),
                "-DSPIRV_SKIP_TESTS=ON",
                "-DSPIRV_SKIP_EXECUTABLES=ON",
                "-DSPIRV_WERROR=OFF",
            ],
        )?;
        run_cmd(
            repo_root,
            "cmake",
            &["--build", path_str(&tools_build)?, "--parallel"],
        )?;
        run_cmd_with_env_overrides(
            repo_root,
            "cmake",
            &["--install", path_str(&tools_build)?],
            &[("DESTDIR", install.display().to_string())],
        )?;
    }

    let translator_build = root.join("translator-build");
    if !pkgconfig.join("LLVMSPIRVLib.pc").is_file() {
        let pkg_path = pkgconfig.display().to_string();
        run_cmd_with_env_overrides(
            repo_root,
            "cmake",
            &[
                "-S",
                path_str(&translator)?,
                "-B",
                path_str(&translator_build)?,
                "-G",
                "Ninja",
                "-DCMAKE_BUILD_TYPE=Release",
                "-DCMAKE_INSTALL_PREFIX=/usr",
                "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
                &format!(
                    "-DLLVM_DIR={}",
                    repo_root
                        .join("out/build/llvm/install/usr/lib/x86_64-linux-gnu/cmake/llvm")
                        .display()
                ),
                &format!(
                    "-DLLVM_EXTERNAL_SPIRV_HEADERS_SOURCE_DIR={}",
                    translator_headers.display()
                ),
                "-DLLVM_SPIRV_BUILD_EXTERNAL=YES",
                "-DLLVM_SPIRV_INCLUDE_TESTS=OFF",
                "-DLLVM_SPIRV_ENABLE_LIBSPIRV_DIS=OFF",
                "-DBUILD_SHARED_LIBS=OFF",
            ],
            &[("PKG_CONFIG_PATH", pkg_path.clone())],
        )?;
        run_cmd_with_env_overrides(
            repo_root,
            "cmake",
            &["--build", path_str(&translator_build)?, "--parallel"],
            &[("PKG_CONFIG_PATH", pkg_path.clone())],
        )?;
        run_cmd_with_env_overrides(
            repo_root,
            "cmake",
            &["--install", path_str(&translator_build)?],
            &[
                ("DESTDIR", install.display().to_string()),
                ("PKG_CONFIG_PATH", pkg_path),
            ],
        )?;
    }
    // These packages are staged beneath DESTDIR but advertise /usr in their
    // generated .pc files. Point build-only consumers at the output-owned
    // prefix so pkg-config can never resolve matching host headers/libraries.
    for name in ["SPIRV-Tools.pc", "SPIRV-Tools-shared.pc", "LLVMSPIRVLib.pc"] {
        let descriptor = pkgconfig.join(name);
        if descriptor.is_file() {
            let contents = fs::read_to_string(&descriptor)?;
            let output_prefix = format!("prefix={}", install.join("usr").display());
            let normalized = contents.replacen("prefix=/usr", &output_prefix, 1);
            fs::write(&descriptor, normalized)?;
        }
    }
    Ok(pkgconfig)
}

fn rewrite_pkgconfig_prefix(source: &Path, destination: &Path, prefix: &Path) -> Result<()> {
    let contents = fs::read_to_string(source)
        .with_context(|| format!("failed to read {}", source.display()))?;
    let rewritten = contents.replacen("prefix=/usr", &format!("prefix={}", prefix.display()), 1);
    fs::write(destination, rewritten)
        .with_context(|| format!("failed to write {}", destination.display()))
}

/// Vulkan-Tools needs both Wayland's scanner XML and wayland-protocols at
/// configure/build time. Their installed pkg-config files deliberately use
/// the final `/usr` prefix, so make output-owned build descriptors that point
/// at the staged MattOS trees rather than accidentally consulting the host.
fn vulkan_wayland_pkgconfig(repo_root: &Path) -> Result<PathBuf> {
    let output = repo_root.join("out/build/vulkan-tools/build-pkgconfig");
    remove_path_if_exists(&output)?;
    fs::create_dir_all(&output)?;
    let wayland_usr = repo_root.join("out/build/wayland/install/usr");
    let wayland_pc = wayland_usr.join("lib/x86_64-linux-gnu/pkgconfig");
    for name in ["wayland-client.pc", "wayland-scanner.pc"] {
        rewrite_pkgconfig_prefix(&wayland_pc.join(name), &output.join(name), &wayland_usr)?;
    }
    let protocols_usr = repo_root.join("out/build/mesa/install/usr");
    rewrite_pkgconfig_prefix(
        &protocols_usr.join("share/pkgconfig/wayland-protocols.pc"),
        &output.join("wayland-protocols.pc"),
        &protocols_usr,
    )?;
    Ok(output)
}

fn build_vulkan_cmake(
    repo_root: &Path,
    component: &str,
    source_relative: &str,
    dependencies: &[&str],
    options: &[String],
    required_outputs: &[&str],
    pkgconfig_override: Option<&Path>,
) -> Result<()> {
    let source = repo_root.join(source_relative);
    let out_root = repo_root.join("out/build").join(component);
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("recipe.stamp");
    let state = fs::read_to_string(
        repo_root
            .join("upstream/state")
            .join(format!("{component}.toml")),
    )?;
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
        remove_path_if_exists(&install_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    if !source_copy.join("CMakeLists.txt").is_file() {
        sync_build_source(&source, &source_copy)?;
    }
    fs::create_dir_all(&build_dir)?;
    let mut env = staged_library_environment(repo_root, dependencies)?;
    if let Some(override_dir) = pkgconfig_override {
        let existing = env
            .iter()
            .find(|(key, _)| *key == "PKG_CONFIG_LIBDIR")
            .map(|(_, value)| value.as_str())
            .unwrap_or_default();
        let value = if existing.is_empty() {
            override_dir.display().to_string()
        } else {
            format!("{}:{existing}", override_dir.display())
        };
        for (key, current) in &mut env {
            if *key == "PKG_CONFIG_PATH" || *key == "PKG_CONFIG_LIBDIR" {
                *current = value.clone();
            }
        }
    }
    if !build_dir.join("build.ninja").is_file() {
        let mut args = vec![
            "-S".to_string(),
            source_copy.display().to_string(),
            "-B".to_string(),
            build_dir.display().to_string(),
            "-G".to_string(),
            "Ninja".to_string(),
            "-DCMAKE_BUILD_TYPE=Release".to_string(),
            "-DCMAKE_INSTALL_PREFIX=/usr".to_string(),
            "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu".to_string(),
            "-DCMAKE_FIND_PACKAGE_NO_PACKAGE_REGISTRY=ON".to_string(),
        ];
        args.extend(options.iter().cloned());
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        run_cmd_with_env_overrides(repo_root, "cmake", &refs, &env)?;
    }
    let jobs = scheduler::child_job_limit().max(1).to_string();
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--build", path_str(&build_dir)?, "--parallel", &jobs],
        &env,
    )?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build_dir)?, "--prefix", "/usr"],
        &[
            env.as_slice(),
            &[("DESTDIR", install_dir.display().to_string())],
        ]
        .concat(),
    )?;
    for relative in required_outputs {
        if !install_dir.join(relative).is_file() {
            bail!("{component} install did not produce {relative}")
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_vulkan_headers(repo_root: &Path) -> Result<()> {
    build_vulkan_cmake(
        repo_root,
        "vulkan-headers",
        "src/system/graphics/vulkan-headers",
        &[],
        &[
            "-DVULKAN_HEADERS_ENABLE_TESTS=OFF".to_string(),
            "-DVULKAN_HEADERS_ENABLE_MODULE=OFF".to_string(),
        ],
        &[
            "usr/include/vulkan/vulkan.h",
            "usr/share/vulkan/registry/vk.xml",
        ],
        None,
    )
}

fn build_vulkan_loader(repo_root: &Path) -> Result<()> {
    let headers = repo_root.join("out/build/vulkan-headers/install/usr/share/cmake/VulkanHeaders");
    build_vulkan_cmake(
        repo_root,
        "vulkan-loader",
        "src/system/graphics/vulkan-loader",
        &["vulkan-headers", "wayland", "cpython"],
        &[
            format!("-DVulkanHeaders_DIR={}", headers.display()),
            "-DBUILD_TESTS=OFF".to_string(),
            "-DBUILD_WERROR=OFF".to_string(),
            "-DLOADER_CODEGEN=ON".to_string(),
            "-DBUILD_WSI_XCB_SUPPORT=OFF".to_string(),
            "-DBUILD_WSI_XLIB_SUPPORT=OFF".to_string(),
            "-DBUILD_WSI_XLIB_XRANDR_SUPPORT=OFF".to_string(),
            "-DBUILD_WSI_WAYLAND_SUPPORT=ON".to_string(),
        ],
        &["usr/lib/x86_64-linux-gnu/libvulkan.so.1"],
        None,
    )
}

fn build_vulkan_tools(repo_root: &Path) -> Result<()> {
    let pkgconfig = vulkan_wayland_pkgconfig(repo_root)?;
    let headers = repo_root.join("out/build/vulkan-headers/install/usr/share/cmake/VulkanHeaders");
    build_vulkan_cmake(
        repo_root,
        "vulkan-tools",
        "src/system/graphics/vulkan-tools",
        &[
            "vulkan-headers",
            "vulkan-loader",
            "wayland",
            "libffi",
            "mesa",
            "cpython",
        ],
        &[
            format!("-DVulkanHeaders_DIR={}", headers.display()),
            "-DBUILD_CUBE=ON".to_string(),
            "-DBUILD_VULKANINFO=ON".to_string(),
            "-DBUILD_ICD=OFF".to_string(),
            "-DBUILD_TESTS=OFF".to_string(),
            "-DBUILD_WERROR=OFF".to_string(),
            "-DTOOLS_CODEGEN=OFF".to_string(),
            "-DBUILD_WSI_XCB_SUPPORT=OFF".to_string(),
            "-DBUILD_WSI_XLIB_SUPPORT=OFF".to_string(),
            "-DBUILD_WSI_WAYLAND_SUPPORT=ON".to_string(),
            "-DBUILD_WSI_DISPLAY_SUPPORT=ON".to_string(),
        ],
        &["usr/bin/vulkaninfo", "usr/bin/vkcube"],
        Some(&pkgconfig),
    )
}

fn build_mesa(repo_root: &Path) -> Result<()> {
    // Mesa's generator uses Mako. It is a build-only Python module, not a
    // shipped runtime dependency; keep the pinned wheel installation entirely
    // under the stage output so the host Python environment is never mutated.
    let python_deps = repo_root.join("out/build/mesa/python-deps");
    if !python_deps.join("mako").is_dir() {
        fs::create_dir_all(&python_deps)?;
        run_cmd(
            repo_root,
            "python3",
            &[
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "--no-deps",
                "--target",
                path_str(&python_deps)?,
                "Mako==1.3.10",
            ],
        )?;
    }
    // Mesa uses glslangValidator at build time to compile the internal BVH
    // shaders shared by RADV, ANV and lavapipe. Keep that transitive build
    // tool pinned and output-owned; none of it is copied into the runtime.
    const GLSLANG_COMMIT: &str = "8a85691a0740d390761a1008b4696f57facd02c4";
    let glslang_root = repo_root.join("out/build/mesa/glslang");
    let glslang_source = glslang_root.join("source");
    let glslang_build = glslang_root.join("build");
    let glslang_validator = glslang_build.join("StandAlone/glslangValidator");
    if !glslang_validator.is_file() {
        remove_path_if_exists(&glslang_root)?;
        fs::create_dir_all(&glslang_root)?;
        ensure_pinned_transitive_checkout(
            &glslang_source,
            "https://github.com/KhronosGroup/glslang.git",
            GLSLANG_COMMIT,
        )?;
        run_cmd(
            repo_root,
            "cmake",
            &[
                "-S",
                path_str(&glslang_source)?,
                "-B",
                path_str(&glslang_build)?,
                "-DCMAKE_BUILD_TYPE=Release",
                "-DENABLE_OPT=OFF",
                "-DENABLE_HLSL=OFF",
                "-DENABLE_GLSLANG_BINARIES=ON",
            ],
        )?;
        run_cmd(
            repo_root,
            "cmake",
            &[
                "--build",
                path_str(&glslang_build)?,
                "--target",
                "glslang-standalone",
                "--parallel",
            ],
        )?;
    }
    let checked_out = run_cmd_capture(&glslang_source, "git", &["rev-parse", "HEAD"])?;
    if checked_out.trim() != GLSLANG_COMMIT {
        bail!(
            "Mesa glslang build tool is at {}, expected {GLSLANG_COMMIT}",
            checked_out.trim()
        )
    }
    let spirv_pkgconfig = prepare_mesa_spirv_dependencies(repo_root)?;
    let rust_tools = repo_root.join("out/build/rust/install/usr/bin");
    let cbindgen_root = repo_root.join("out/build/mesa/cbindgen");
    let cbindgen = cbindgen_root.join("bin/cbindgen");
    if !cbindgen.is_file() {
        let cargo = rust_tools.join("cargo");
        let rustc = rust_tools.join("rustc");
        run_cmd_with_env_overrides(
            repo_root,
            path_str(&cargo)?,
            &[
                "install",
                "cbindgen",
                "--version",
                "0.29.4",
                "--locked",
                "--root",
                path_str(&cbindgen_root)?,
            ],
            &[
                (
                    "CARGO_HOME",
                    repo_root
                        .join("out/build/mesa/cargo-home")
                        .display()
                        .to_string(),
                ),
                ("RUSTC", rustc.display().to_string()),
            ],
        )?;
    }
    // Debian's bindgen 0.71.1 predates the Clang 22 AST behavior used by the
    // source-built MattOS LLVM and emits opaque one-byte Mesa structs with
    // contradictory layout assertions. Pin a current, known-good generator
    // beside cbindgen so Mesa's Rust/NVK bindings stay output-owned too.
    let bindgen_root = repo_root.join("out/build/mesa/bindgen");
    let bindgen = bindgen_root.join("bin/bindgen");
    if !bindgen.is_file() {
        let cargo = rust_tools.join("cargo");
        let rustc = rust_tools.join("rustc");
        run_cmd_with_env_overrides(
            repo_root,
            path_str(&cargo)?,
            &[
                "install",
                "bindgen-cli",
                "--version",
                "0.72.1",
                "--locked",
                "--root",
                path_str(&bindgen_root)?,
            ],
            &[
                (
                    "CARGO_HOME",
                    repo_root
                        .join("out/build/mesa/cargo-home")
                        .display()
                        .to_string(),
                ),
                ("RUSTC", rustc.display().to_string()),
            ],
        )?;
    }
    let glslang_path = glslang_validator
        .parent()
        .expect("glslang validator parent")
        .display()
        .to_string();
    let llvm_tools = repo_root.join("out/build/llvm/install/usr/bin");
    let wayland_tools = repo_root.join("out/build/wayland/install/usr/bin");
    build_meson_runtime(
        repo_root,
        "mesa",
        "src/system/graphics/mesa",
        &[
            "libdrm",
            "libdisplay-info",
            "libffi",
            "llvm",
            "zlib",
            "zstd",
            "systemd",
            "wayland",
            "libglvnd",
        ],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dplatforms=wayland",
            "-Degl-native-platform=wayland",
            "-Dglx=disabled",
            "-Dglvnd=enabled",
            "-Dopengl=true",
            "-Dgles1=enabled",
            "-Dgles2=enabled",
            // Keep software and QEMU renderers while covering the production
            // DRM drivers enabled by MattOS' generic modular kernel. SVGA is
            // the corresponding VMware guest renderer.
            "-Degl=enabled",
            "-Dgbm=enabled",
            "-Dgallium-drivers=radeonsi,iris,nouveau,virgl,llvmpipe,svga",
            // RADV, ANV and NVK are the hardware Vulkan implementations;
            // lavapipe and Venus provide software and virtio-gpu fallbacks.
            "-Dvulkan-drivers=amd,intel,nouveau,swrast,virtio",
            "-Dvulkan-layers=device-select",
            "-Dllvm=enabled",
            "-Dshared-llvm=enabled",
            "-Dcpp_rtti=false",
            "-Dbuild-tests=false",
            "-Denable-glcpp-tests=false",
            "-Dtools=[]",
            "-Dhtml-docs=disabled",
            "-Dzstd=enabled",
        ],
        "usr/lib/x86_64-linux-gnu/libgbm.so.1",
        &[
            ("PYTHONPATH", python_deps.display().to_string()),
            ("PKG_CONFIG_PATH", spirv_pkgconfig.display().to_string()),
            (
                "PATH",
                format!(
                    "{}:{}:{glslang_path}:{}:{}:/usr/bin:/bin",
                    bindgen_root.join("bin").display(),
                    cbindgen_root.join("bin").display(),
                    llvm_tools.display(),
                    wayland_tools.display()
                ),
            ),
        ],
    )
}

fn build_cosmic_comp(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/desktop/cosmic/cosmic-comp");
    let out_root = repo_root.join("out/build/cosmic-comp");
    let source_copy = out_root.join("source");
    let target = out_root.join("cargo-target");
    let install = out_root.join("install");
    remove_path_if_exists(&source_copy)?;
    sync_build_source(&source, &source_copy)?;
    apply_component_patches(repo_root, "cosmic-comp", &source_copy)?;
    let components = [
        "seatd",
        "libdisplay-info",
        "libinput",
        "pixman",
        "mesa",
        "libdrm",
        "xkbcommon",
        "systemd",
    ];
    let env = staged_library_environment(repo_root, &components)?;
    let library_dirs = components
        .iter()
        .map(|component| {
            repo_root
                .join("out/build")
                .join(component)
                .join("install/usr/lib/x86_64-linux-gnu")
        })
        .collect::<Vec<_>>();
    let library_dir_refs = library_dirs
        .iter()
        .map(PathBuf::as_path)
        .collect::<Vec<_>>();
    // Keep the compositor's systemd feature: it sends READY=1 only after its
    // Wayland/KMS session is initialized, which gives the installer a real
    // readiness dependency instead of a time-based socket race.
    run_cmd_with_env_overrides(
        &source_copy,
        "cargo",
        &["build", "--locked", "--release"],
        &[
            ("CARGO_TARGET_DIR", target.display().to_string()),
            (
                "PKG_CONFIG_PATH",
                env.iter()
                    .find(|(key, _)| *key == "PKG_CONFIG_PATH")
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default(),
            ),
            (
                "PKG_CONFIG_LIBDIR",
                env.iter()
                    .find(|(key, _)| *key == "PKG_CONFIG_LIBDIR")
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default(),
            ),
            (
                "LIBRARY_PATH",
                env.iter()
                    .find(|(key, _)| *key == "LIBRARY_PATH")
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default(),
            ),
            (
                "LD_LIBRARY_PATH",
                env.iter()
                    .find(|(key, _)| *key == "LD_LIBRARY_PATH")
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default(),
            ),
        ],
    )?;
    remove_path_if_exists(&install)?;
    let binary = target.join("release/cosmic-comp");
    if !binary.is_file() {
        bail!("cosmic-comp build did not produce {}", binary.display());
    }
    let installed_binary = install.join("usr/bin/cosmic-comp");
    fs::create_dir_all(
        installed_binary
            .parent()
            .expect("cosmic-comp install parent"),
    )?;
    fs::copy(&binary, &installed_binary)?;
    fs::set_permissions(&installed_binary, fs::metadata(&binary)?.permissions())?;
    for (soname, component) in [
        ("libseat.so.1", "seatd"),
        ("libdisplay-info.so.3", "libdisplay-info"),
        ("libinput.so.10", "libinput"),
        ("libpixman-1.so.0", "pixman"),
        ("libgbm.so.1", "mesa"),
        ("libxkbcommon.so.0", "xkbcommon"),
    ] {
        let library = repo_root
            .join("out/build")
            .join(component)
            .join("install/usr/lib/x86_64-linux-gnu");
        // Resolve the entire source-closed runtime closure while checking one
        // SONAME.  Checking against only that one directory made ldd reject
        // legitimate transitive MattOS dependencies as "not found".
        validate_dependency_resolves_from(&binary, soname, &library, &library_dir_refs)?;
    }
    Ok(())
}

fn cosmic_just(repo_root: &Path) -> Result<PathBuf> {
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
    Ok(just)
}

fn cosmic_component_environment(
    repo_root: &Path,
    install: &Path,
    stage: BuildStage,
) -> Result<Vec<(&'static str, String)>> {
    let native_components = cosmic_native_components(stage);
    let mut env = staged_library_environment(repo_root, &native_components)?;
    let just = cosmic_just(repo_root)?;
    let inherited_path = env
        .iter()
        .find_map(|(key, value)| (*key == "PATH").then_some(value.as_str()))
        .unwrap_or_default();
    let tool_path = std::env::join_paths(
        std::iter::once(just.parent().expect("just bin parent").to_path_buf())
            .chain(std::env::split_paths(inherited_path)),
    )?
    .to_string_lossy()
    .to_string();
    if let Some((_, value)) = env.iter_mut().find(|(key, _)| *key == "PATH") {
        *value = tool_path;
    }
    let shared_target = cosmic_shared_target(repo_root);
    fs::create_dir_all(&shared_target)?;
    env.push(("CARGO_BUILD_JOBS", "4".to_string()));
    env.push(("CARGO_INCREMENTAL", "0".to_string()));
    env.push(("CARGO_TARGET_DIR", shared_target.display().to_string()));
    env.push(("RUSTFLAGS", cosmic_source_remap_flags(repo_root)));
    env.push(("CARGO_PROFILE_RELEASE_LTO", "false".to_string()));
    env.push(("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "4".to_string()));
    env.push(("DESTDIR", install.display().to_string()));
    Ok(env)
}

fn cosmic_native_components(stage: BuildStage) -> Vec<&'static str> {
    let mut components = vec!["glibc", "gcc-runtime"];
    components.extend(
        stage_graph::direct_dependencies(stage)
            .iter()
            .copied()
            .filter(|component| *component != "formal-sysroot"),
    );
    if stage == BuildStage::CosmicUtilities {
        // btrfs-progs is built inside the installer stage and publishes its
        // development library from this nested install root.
        components.push("btrfs-progs");
    }
    if stage == BuildStage::CosmicEdit && !components.contains(&"zlib") {
        // gio-2.0.pc declares zlib as a transitive pkg-config requirement.
        // Keep the provider visible even when the scheduler supplies only the
        // component's direct native environment.
        components.push("zlib");
    }
    components
}

fn cosmic_source_remap_flags(repo_root: &Path) -> String {
    let output_sources = repo_root.join("out/build/cosmic-desktop/sources");
    let canonical_sources = repo_root.join("out/source-ownership/sources");
    format!(
        "--remap-path-prefix={}=/usr/src/mattos/cosmic-sources --remap-path-prefix={}=/usr/src/mattos/cosmic-sources --remap-path-prefix={}=/usr/src/mattos",
        output_sources.display(),
        canonical_sources.display(),
        repo_root.display(),
    )
}

fn cosmic_shared_target(repo_root: &Path) -> PathBuf {
    // crabtime derives Cargo's target root from OUT_DIR and requires the
    // conventional directory name `target`; keep all COSMIC components on
    // this shared output-owned target while satisfying that contract.
    repo_root.join("out/build/cosmic-desktop/target")
}

fn cosmic_shared_target_lock(repo_root: &Path) -> PathBuf {
    repo_root.join("out/cache/cosmic-cargo-target.lock")
}

fn patch_cosmic_profile_helper(mirror: &Path) -> Result<()> {
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
    Ok(())
}

fn patch_cosmic_just_target_path(mirror: &Path) -> Result<()> {
    let justfile = mirror.join("justfile");
    if !justfile.is_file() {
        return Ok(());
    }
    let original = fs::read_to_string(&justfile)?;
    let mut updated = original.replace(
        "bin-src := 'target' / 'release' / name",
        "bin-src := env('CARGO_TARGET_DIR', 'target') / 'release' / name",
    );
    updated = updated.replace(" --locked {{args}}", " {{args}}");
    updated = updated.replace(
        "desktop-src := 'resources' / appid + '.desktop'",
        "desktop-src := 'resources' / 'app.desktop'",
    );
    updated = updated.replace(
        "appdata-src := 'resources' / appid + '.metainfo.xml'",
        "appdata-src := 'resources' / 'app.metainfo.xml'",
    );
    if !updated.contains("\nbuild-release") && updated.contains("\nrelease *args:") {
        updated.push_str("\n# MattOS invokes the common COSMIC release recipe name.\nbuild-release *args: (release args)\n");
    }
    if updated != original {
        fs::write(justfile, updated)?;
    }
    Ok(())
}

fn run_locked_cosmic_command(
    repo_root: &Path,
    cwd: &Path,
    program: &str,
    args: &[&str],
    env: &[(&str, String)],
) -> Result<()> {
    let lock = cosmic_shared_target_lock(repo_root);
    if let Some(parent) = lock.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut locked_args = vec!["-x", path_str(&lock)?, program];
    locked_args.extend_from_slice(args);
    run_cmd_with_env_overrides(cwd, "flock", &locked_args, env)
}

fn build_cosmic_just_component(
    repo_root: &Path,
    install: &Path,
    component: &str,
    env: &[(&str, String)],
) -> Result<()> {
    // Keep the mirror path stable across the old aggregate builder and the
    // granular stage graph. Cargo fingerprints include workspace paths, so
    // moving otherwise-identical sources would throw away valid artifacts.
    let mirror = repo_root
        .join("out/build/cosmic-desktop/sources")
        .join(component);
    sync_build_source(
        &repo_root.join("src/desktop/cosmic").join(component),
        &mirror,
    )?;
    apply_component_patches(repo_root, component, &mirror)?;
    isolate_cargo_build_mirror(&mirror)?;
    patch_cosmic_just_target_path(&mirror)?;
    ensure_owned_libcosmic_mirror(repo_root, &mirror)?;
    if matches!(component, "cosmic-launcher" | "cosmic-notifications") {
        patch_cosmic_profile_helper(&mirror)?;
    }
    let just = cosmic_just(repo_root)?;
    run_locked_cosmic_command(
        repo_root,
        &mirror,
        path_str(&just)?,
        &["build-release", "--locked"],
        env,
    )?;
    let rootdir = format!("rootdir={}", install.display());
    let pop_launcher_target_dir = env
        .iter()
        .find(|(key, _)| *key == "CARGO_TARGET_DIR")
        .map(|(_, value)| format!("target-dir={}/release", value));
    let install_args = if component == "pop-launcher" {
        let mut args = vec![rootdir.as_str(), "install"];
        if let Some(target_dir) = pop_launcher_target_dir.as_deref() {
            args.insert(0, target_dir);
        }
        args
    } else {
        vec![rootdir.as_str(), "prefix=/usr", "install"]
    };
    run_cmd_with_env_overrides(&mirror, path_str(&just)?, &install_args, env)
}

/// Upstream COSMIC applications declare libcosmic as a Git dependency. Keep
/// that declaration intact in authoritative source, but make every output
/// mirror resolve it to MattOS's pinned, output-owned libcosmic tree. Without
/// this patch Cargo silently follows the upstream lockfile's older Git
/// revision, making the COSMIC Files compatibility adaptation target the wrong
/// API.
fn ensure_owned_libcosmic_mirror(repo_root: &Path, component_mirror: &Path) -> Result<()> {
    let sources = component_mirror
        .parent()
        .ok_or_else(|| anyhow!("COSMIC component mirror has no source parent"))?;
    let libcosmic = sources.join("libcosmic");
    sync_build_source(&repo_root.join("src/desktop/cosmic/libcosmic"), &libcosmic)?;
    // libcosmic retains iced as an upstream gitlink. Reconstruct that
    // declared dependency beside the owned libcosmic mirror as well; otherwise
    // the path override is incomplete and Cargo cannot resolve the workspace.
    sync_build_source(
        &repo_root.join("src/desktop/cosmic/iced"),
        &libcosmic.join("iced"),
    )?;

    let manifest = component_mirror.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(());
    }
    let raw_body = fs::read_to_string(&manifest)?;
    // Reconstruct the whole output-owned patch table on every materialization.
    // Older mirrors and some authoritative COSMIC manifests contain a
    // commented table, while older MattOS mirrors used the `.git` spelling;
    // retaining either table beside the canonical one creates duplicate TOML
    // keys or lets Cargo re-resolve the upstream git package.
    let patch_keys = [
        "[patch.\"https://github.com/pop-os/libcosmic\"]",
        "[patch.\"https://github.com/pop-os/libcosmic.git\"]",
        "[patch.'https://github.com/pop-os/libcosmic']",
        "[patch.'https://github.com/pop-os/libcosmic.git']",
    ];
    let mut body = String::new();
    let mut skipping_patch_table = false;
    for line in raw_body.lines() {
        if patch_keys.contains(&line.trim()) {
            skipping_patch_table = true;
            continue;
        }
        if skipping_patch_table && line.starts_with('[') && !line.starts_with("[[") {
            skipping_patch_table = false;
        }
        if !skipping_patch_table {
            body.push_str(line);
            body.push('\n');
        }
    }
    const MARKER: &str = "# MattOS output-owned libcosmic dependency override.";
    let declarations = body.split(MARKER).next().unwrap_or(&body);
    if !declarations.lines().any(|line| {
        let line = line.trim_start();
        !line.starts_with('#')
            && (line.starts_with("libcosmic")
                || line.starts_with("[dependencies.libcosmic]")
                || line.starts_with("[workspace.dependencies.libcosmic]"))
    }) {
        return Ok(());
    }
    if !body.contains(MARKER) {
        let mut updated = body;
        updated.push_str(&format!(
            "\n{MARKER}\n[patch.\"https://github.com/pop-os/libcosmic\"]\nlibcosmic = {{ path = \"../libcosmic\" }}\ncosmic-config = {{ path = \"../libcosmic/cosmic-config\" }}\ncosmic-config-derive = {{ path = \"../libcosmic/cosmic-config-derive\" }}\ncosmic-theme = {{ path = \"../libcosmic/cosmic-theme\" }}\niced_core = {{ path = \"../libcosmic/iced/core\" }}\niced_futures = {{ path = \"../libcosmic/iced/futures\" }}\niced_graphics = {{ path = \"../libcosmic/iced/graphics\" }}\niced_renderer = {{ path = \"../libcosmic/iced/renderer\" }}\niced_runtime = {{ path = \"../libcosmic/iced/runtime\" }}\niced_widget = {{ path = \"../libcosmic/iced/widget\" }}\niced_winit = {{ path = \"../libcosmic/iced/winit\" }}\niced_tiny_skia = {{ path = \"../libcosmic/iced/tiny_skia\" }}\niced_wgpu = {{ path = \"../libcosmic/iced/wgpu\" }}\n"
        ));
        fs::write(&manifest, updated)?;
    } else if !body.contains("cosmic-config = { path = \"../libcosmic/cosmic-config\" }") {
        let additions = "cosmic-config = { path = \"../libcosmic/cosmic-config\" }\ncosmic-config-derive = { path = \"../libcosmic/cosmic-config-derive\" }\ncosmic-theme = { path = \"../libcosmic/cosmic-theme\" }\niced_core = { path = \"../libcosmic/iced/core\" }\niced_futures = { path = \"../libcosmic/iced/futures\" }\niced_graphics = { path = \"../libcosmic/iced/graphics\" }\niced_renderer = { path = \"../libcosmic/iced/renderer\" }\niced_runtime = { path = \"../libcosmic/iced/runtime\" }\niced_widget = { path = \"../libcosmic/iced/widget\" }\niced_winit = { path = \"../libcosmic/iced/winit\" }\niced_tiny_skia = { path = \"../libcosmic/iced/tiny_skia\" }\niced_wgpu = { path = \"../libcosmic/iced/wgpu\" }";
        let updated = body.replacen(
            "libcosmic = { path = \"../libcosmic\" }",
            &format!("libcosmic = {{ path = \"../libcosmic\" }}\n{additions}"),
            1,
        );
        fs::write(&manifest, updated)?;
    }
    // The upstream lockfile records the Git package IDs. Re-resolve only the
    // libcosmic package graph so the output mirror's lockfile records the
    // pinned local sources while preserving all unrelated upstream locks.
    run_cmd(component_mirror, "cargo", &["update", "-p", "libcosmic"])?;
    Ok(())
}

fn build_cosmic_desktop_component(repo_root: &Path, stage: BuildStage) -> Result<()> {
    let id = build_stage_id(stage);
    let out_root = repo_root.join("out/build").join(id);
    let install = out_root.join("install");
    remove_path_if_exists(&install)?;
    fs::create_dir_all(&install)?;
    let env = cosmic_component_environment(repo_root, &install, stage)?;
    let just_component = match stage {
        BuildStage::CosmicSession => Some("cosmic-session"),
        BuildStage::CosmicGreeter => Some("cosmic-greeter"),
        BuildStage::CosmicPanel => Some("cosmic-panel"),
        BuildStage::CosmicApplets => Some("cosmic-applets"),
        BuildStage::CosmicAppLibrary => Some("cosmic-applibrary"),
        BuildStage::CosmicLauncher => Some("cosmic-launcher"),
        BuildStage::CosmicSettings => Some("cosmic-settings"),
        BuildStage::CosmicNotifications => Some("cosmic-notifications"),
        BuildStage::CosmicOsd => Some("cosmic-osd"),
        BuildStage::CosmicBg => Some("cosmic-bg"),
        BuildStage::CosmicFiles => Some("cosmic-files"),
        BuildStage::CosmicTerm => Some("cosmic-term"),
        BuildStage::CosmicTweaks => Some("cosmic-tweaks"),
        _ => None,
    };
    if let Some(component) = just_component {
        return build_cosmic_just_component(repo_root, &install, component, &env);
    }

    match stage {
        BuildStage::CosmicSettingsDaemon | BuildStage::CosmicWorkspaces => {
            let component = if stage == BuildStage::CosmicSettingsDaemon {
                "cosmic-settings-daemon"
            } else {
                "cosmic-workspaces"
            };
            let mirror = repo_root
                .join("out/build/cosmic-desktop/sources")
                .join(component);
            sync_build_source(
                &repo_root.join("src/desktop/cosmic").join(component),
                &mirror,
            )?;
            apply_component_patches(repo_root, component, &mirror)?;
            isolate_cargo_build_mirror(&mirror)?;
            run_locked_cosmic_command(repo_root, &mirror, "make", &["-j4"], &env)?;
            let destdir = format!("DESTDIR={}", install.display());
            run_cmd_with_env_overrides(
                &mirror,
                "make",
                &[destdir.as_str(), "prefix=/usr", "install"],
                &env,
            )?;
            if component == "cosmic-workspaces" {
                // rust-embed materializes CARGO_MANIFEST_DIR in the generated
                // asset metadata. It is output data, not authoritative source,
                // but the absolute mirror path would leak the build host into
                // the shipped ELF. Keep the generated asset layout unchanged
                // while replacing only that deterministic path prefix.
                sanitize_embedded_output_path(&install.join("usr/bin/cosmic-workspaces"), &mirror)?;
            }
        }
        BuildStage::CosmicUtilities => {
            for component in [
                "cosmic-randr",
                "cosmic-screenshot",
                "pop-launcher",
                "cosmic-calculator",
                "cosmic-storage",
                "cosmic-monitor",
                "cosmic-store",
            ] {
                build_cosmic_just_component(repo_root, &install, component, &env)?;
            }
        }
        BuildStage::Flatpak => build_flatpak(repo_root)?,
        BuildStage::CosmicPortal => {
            let component = "xdg-desktop-portal-cosmic";
            let mirror = repo_root
                .join("out/build/cosmic-desktop/sources")
                .join(component);
            sync_build_source(
                &repo_root.join("src/desktop/cosmic").join(component),
                &mirror,
            )?;
            apply_component_patches(repo_root, component, &mirror)?;
            isolate_cargo_build_mirror(&mirror)?;
            run_locked_cosmic_command(
                repo_root,
                &mirror,
                "cargo",
                &["build", "--release", "--locked", "--bin", component],
                &env,
            )?;
            let rootdir = format!("rootdir={}", install.display());
            let just = cosmic_just(repo_root)?;
            run_cmd_with_env_overrides(
                &mirror,
                path_str(&just)?,
                &[rootdir.as_str(), "prefix=/usr", "install"],
                &env,
            )?;
        }
        BuildStage::CosmicAssets => {
            let icons = out_root.join("cosmic-icons");
            sync_build_source(&repo_root.join("src/desktop/cosmic/cosmic-icons"), &icons)?;
            let rootdir = format!("rootdir={}", install.display());
            let just = cosmic_just(repo_root)?;
            run_cmd_with_env_overrides(
                &icons,
                path_str(&just)?,
                &[rootdir.as_str(), "prefix=/usr", "install"],
                &env,
            )?;
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
                    fs::copy(source, destination)?;
                }
            }
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
            // COSMIC reads system defaults from /usr/share/cosmic while
            // Initial Setup reads its selectable resources from these two
            // dedicated directories. Keep this policy layer separate from
            // all imported upstream source trees.
            copy_tree_contents(
                &repo_root.join("resources/COSMIC/defaults"),
                &install.join("usr/share/cosmic"),
            )?;
        }
        BuildStage::Greetd => {
            let mirror = repo_root.join("out/build/cosmic-desktop/sources/greetd");
            sync_build_source(&repo_root.join("src/system/session/greetd"), &mirror)?;
            isolate_cargo_build_mirror(&mirror)?;
            run_locked_cosmic_command(
                repo_root,
                &mirror,
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
                &env,
            )?;
            let target = cosmic_shared_target(repo_root).join("release");
            for binary in ["greetd", "agreety"] {
                let destination = install.join("usr/bin").join(binary);
                fs::create_dir_all(destination.parent().expect("greetd bin parent"))?;
                fs::copy(target.join(binary), &destination)?;
                set_mode(destination, 0o755)?;
            }
        }
        _ => bail!("{id} is not a granular COSMIC component stage"),
    }
    Ok(())
}

fn build_cosmic_desktop(repo_root: &Path) -> Result<()> {
    let out_root = repo_root.join("out/build/cosmic-desktop");
    let install = out_root.join("install");
    remove_path_if_exists(&install)?;
    fs::create_dir_all(&install)?;
    for component in [
        "cosmic-session",
        "cosmic-greeter",
        "cosmic-panel",
        "cosmic-applets",
        "cosmic-applibrary",
        "cosmic-launcher",
        "cosmic-settings",
        "cosmic-settings-daemon",
        "cosmic-notifications",
        "cosmic-osd",
        "cosmic-bg",
        "cosmic-workspaces",
        "cosmic-files",
        "cosmic-term",
        "cosmic-tweaks",
        "cosmic-utilities",
        "cosmic-portal",
        "cosmic-assets",
        "greetd",
    ] {
        let component_install = repo_root.join("out/build").join(component).join("install");
        if !component_install.is_dir() {
            bail!(
                "COSMIC aggregate input missing: {}",
                component_install.display()
            );
        }
        copy_tree_contents(&component_install, &install)?;
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
        "usr/bin/cosmic-ext-calculator",
        "usr/bin/cosmic-ext-storage",
        "usr/bin/cosmic-monitor",
        "usr/bin/cosmic-store",
        "usr/bin/greetd",
        "usr/share/wayland-sessions/cosmic.desktop",
        "usr/share/icons/Cosmic/index.theme",
        "usr/share/fonts/truetype/open-sans/OpenSans-Regular.ttf",
        "usr/share/fonts/truetype/noto/NotoSansMono[wdth,wght].ttf",
    ] {
        if !install.join(required).is_file() {
            bail!("COSMIC desktop aggregate did not install /{required}");
        }
    }
    Ok(())
}

fn build_cosmic_edit(repo_root: &Path) -> Result<()> {
    let out_root = repo_root.join("out/build/cosmic-edit");
    let install = out_root.join("install");
    let mirror = out_root.join("source");
    remove_path_if_exists(&install)?;
    sync_build_source(&repo_root.join("src/desktop/cosmic/cosmic-edit"), &mirror)?;
    isolate_cargo_build_mirror(&mirror)?;
    let mut env = cosmic_component_environment(repo_root, &install, BuildStage::CosmicEdit)?;
    // Keep this component's transitive GLib provider visible to pkg-config.
    // gio-2.0.pc requires zlib.pc, and the production scheduler may publish
    // the zlib stage after the initial native-environment snapshot.
    let zlib_pkgconfig =
        repo_root.join("out/build/zlib/install/usr/lib/x86_64-linux-gnu/pkgconfig");
    for key in ["PKG_CONFIG_PATH", "PKG_CONFIG_LIBDIR"] {
        if let Some((_, value)) = env.iter_mut().find(|(name, _)| *name == key) {
            let mut paths = std::env::split_paths(value).collect::<Vec<_>>();
            if !paths.iter().any(|path| path == &zlib_pkgconfig) {
                paths.push(zlib_pkgconfig.clone());
                *value = std::env::join_paths(paths)?.to_string_lossy().to_string();
            }
        }
    }
    run_locked_cosmic_command(
        repo_root,
        &mirror,
        "cargo",
        &["build", "--locked", "--release", "--bin", "cosmic-edit"],
        &env,
    )?;
    let binary = cosmic_shared_target(repo_root).join("release/cosmic-edit");
    stage_output_file(&binary, &install.join("usr/bin/cosmic-edit"), 0o755)?;
    let res = mirror.join("res");
    copy_file_preserving(
        &res.join("com.system76.CosmicEdit.desktop"),
        &install.join("usr/share/applications/com.system76.CosmicEdit.desktop"),
    )?;
    copy_file_preserving(
        &res.join("com.system76.CosmicEdit.metainfo.xml"),
        &install.join("usr/share/metainfo/com.system76.CosmicEdit.metainfo.xml"),
    )?;
    copy_tree_contents(
        &res.join("icons/hicolor"),
        &install.join("usr/share/icons/hicolor"),
    )?;
    for entry in fs::read_dir(res.join("icons"))? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .ends_with("-symbolic.svg")
        {
            copy_file_preserving(
                &entry.path(),
                &install
                    .join("usr/share/icons/hicolor/symbolic/actions")
                    .join(entry.file_name()),
            )?;
        }
    }
    Ok(())
}

fn build_cosmic_initial_setup(repo_root: &Path) -> Result<()> {
    let out_root = repo_root.join("out/build/cosmic-initial-setup");
    let install = out_root.join("install");
    // Keep this first-class COSMIC consumer in the same output-owned source
    // mirror namespace used by cargo_source_owned.py.  Using a separate
    // component/source mirror makes the dispatcher prepare one path while
    // Cargo runs from another, so locked builds cannot reconcile its copied
    // output Cargo.lock.
    let mirror = repo_root.join("out/build/cosmic-desktop/sources/cosmic-initial-setup");
    remove_path_if_exists(&install)?;
    sync_build_source(
        &repo_root.join("src/desktop/cosmic/cosmic-initial-setup"),
        &mirror,
    )?;
    isolate_cargo_build_mirror(&mirror)?;
    let env = cosmic_component_environment(repo_root, &install, BuildStage::CosmicInitialSetup)?;
    run_locked_cosmic_command(
        repo_root,
        &mirror,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--bin",
            "cosmic-initial-setup",
        ],
        &env,
    )?;
    stage_output_file(
        &cosmic_shared_target(repo_root).join("release/cosmic-initial-setup"),
        &install.join("usr/bin/cosmic-initial-setup"),
        0o755,
    )?;
    let res = mirror.join("res");
    for (source, destination) in [
        (
            "com.system76.CosmicInitialSetup.desktop",
            "usr/share/applications/com.system76.CosmicInitialSetup.desktop",
        ),
        (
            "com.system76.CosmicInitialSetup.Autostart.desktop",
            "etc/xdg/autostart/com.system76.CosmicInitialSetup.Autostart.desktop",
        ),
    ] {
        copy_file_preserving(&res.join(source), &install.join(destination))?;
    }
    copy_file_preserving(
        &res.join("icon.svg"),
        &install.join("usr/share/icons/hicolor/scalable/apps/com.system76.CosmicInitialSetup.svg"),
    )?;
    copy_file_preserving(
        &res.join("20-cosmic-initial-setup.rules"),
        &install.join("usr/share/polkit-1/rules.d/20-cosmic-initial-setup.rules"),
    )?;
    copy_tree_contents(
        &repo_root.join("resources/COSMIC/layouts"),
        &install.join("usr/share/cosmic-layouts"),
    )?;
    copy_tree_contents(
        &repo_root.join("resources/COSMIC/themes"),
        &install.join("usr/share/cosmic-themes"),
    )?;
    Ok(())
}

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
        &["glib", "libffi", "zlib", "libpng"],
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
            "-Dsystem_helper=disabled",
            "-Dsystemd=enabled",
            "-Dseccomp=disabled",
            // Never let Meson record the staged build-tree path returned by
            // find_program("fusermount3") in the shipped binary.  Flatpak
            // executes fusermount from the target package closure at this
            // stable runtime location.
            "-Dsystem_fusermount=/usr/bin/fusermount3",
        ],
        "usr/bin/flatpak",
        &[],
    )
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

fn isolate_cargo_build_mirror(source: &Path) -> Result<()> {
    let _lock = ConsumerMirrorLock::acquire(&source_lock_repo_root(source)?, source)?;
    let manifest = source.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(());
    }
    let body = fs::read_to_string(&manifest)?;
    if !body.lines().any(|line| line.trim() == "[workspace]") {
        let mut file = fs::OpenOptions::new().append(true).open(&manifest)?;
        file.write_all(b"\n# MattOS output-owned build-mirror isolation.\n[workspace]\n")?;
    }
    Ok(())
}

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

fn build_rootfs(repo_root: &Path) -> Result<()> {
    // Package/repository manifests are resolved before the rootfs key so a
    // package change cannot be hidden behind an old rootfs manifest.
    packaging::build_all_packages(repo_root)?;
    packaging::generate_repository(repo_root)?;
    let spec = build_stage_spec(BuildStage::Rootfs);
    performance::execute_cached_stage(
        repo_root,
        &spec,
        || validate_cached_rootfs(repo_root),
        || build_rootfs_atomic(repo_root),
    )
}

const BOOT_CRITICAL_MODULES: &[&str] = &[
    "nvme",
    "ahci",
    "sd_mod",
    "sr_mod",
    // VirtIO device modules do not declare their PCI transport in modules.dep;
    // load the transport explicitly before probing block and SCSI devices.
    "virtio_pci",
    "virtio_blk",
    "virtio_scsi",
    "usb_storage",
    "uas",
    "xhci_pci",
    "btrfs",
    "ext4",
];

fn module_basename(path: &str) -> Option<String> {
    let name = Path::new(path).file_name()?.to_str()?;
    for suffix in [".ko.zst", ".ko.xz", ".ko.gz", ".ko"] {
        if let Some(stem) = name.strip_suffix(suffix) {
            return Some(stem.replace('-', "_"));
        }
    }
    None
}

fn add_module_with_dependencies(
    path: &str,
    dependencies: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
    ordered: &mut Vec<String>,
) -> Result<()> {
    if ordered.iter().any(|existing| existing == path) {
        return Ok(());
    }
    if !visiting.insert(path.to_owned()) {
        bail!("cycle in kernel modules.dep at {path}");
    }
    for dependency in dependencies
        .get(path)
        .with_context(|| format!("module {path} absent from modules.dep"))?
    {
        add_module_with_dependencies(dependency, dependencies, visiting, ordered)?;
    }
    visiting.remove(path);
    ordered.push(path.to_owned());
    Ok(())
}

fn module_firmware_requirements(
    module_root: &Path,
    modules: &[String],
) -> Result<BTreeSet<String>> {
    let mut firmware = BTreeSet::new();
    for relative in modules {
        let module = module_root.join(relative);
        let output = Command::new("modinfo")
            .args(["-F", "firmware"])
            .arg(&module)
            .output()
            .with_context(|| {
                format!(
                    "failed to inspect firmware metadata for {}",
                    module.display()
                )
            })?;
        if !output.status.success() {
            bail!(
                "modinfo failed for boot-critical module {}: {}",
                module.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            )
        }
        for requirement in String::from_utf8(output.stdout)
            .context("module firmware metadata was not UTF-8")?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            firmware.insert(requirement.to_owned());
        }
    }
    Ok(firmware)
}

fn stage_boot_module_closure(repo_root: &Path, tree: &Path) -> Result<(String, usize, usize)> {
    let release = fs::read_to_string(repo_root.join("out/build/linux/kernel-release"))?
        .trim()
        .to_owned();
    let module_root = repo_root
        .join("out/build/linux/modules/usr/lib/modules")
        .join(&release);
    let mut dependencies = BTreeMap::<String, Vec<String>>::new();
    for line in fs::read_to_string(module_root.join("modules.dep"))?.lines() {
        let (module, dependency_list) = line
            .split_once(':')
            .with_context(|| format!("invalid modules.dep line {line:?}"))?;
        dependencies.insert(
            module.to_owned(),
            dependency_list
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
        );
    }
    let by_name = dependencies
        .keys()
        .filter_map(|path| module_basename(path).map(|name| (name, path.clone())))
        .collect::<BTreeMap<_, _>>();
    let mut ordered = Vec::new();
    let mut visiting = BTreeSet::new();
    for required in BOOT_CRITICAL_MODULES {
        let path = by_name
            .get(*required)
            .with_context(|| format!("boot-critical kernel module {required} was not built"))?;
        add_module_with_dependencies(path, &dependencies, &mut visiting, &mut ordered)?;
    }
    let destination_root = tree.join("usr/lib/modules").join(&release);
    for relative in &ordered {
        let destination = destination_root.join(relative);
        fs::create_dir_all(destination.parent().expect("module has parent"))?;
        fs::copy(module_root.join(relative), &destination)?;
    }
    let firmware_requirements = module_firmware_requirements(&module_root, &ordered)?;
    let firmware_source = repo_root.join("src/system/data/linux-firmware");
    for requirement in &firmware_requirements {
        if requirement
            .chars()
            .any(|character| matches!(character, '*' | '?' | '['))
        {
            bail!("boot-critical module uses unsupported firmware glob {requirement}")
        }
        let source = firmware_source.join(requirement);
        if !source.is_file() {
            bail!("boot-critical firmware {requirement} is absent from pinned linux-firmware")
        }
        let destination = tree.join("usr/lib/firmware").join(requirement);
        fs::create_dir_all(destination.parent().expect("firmware has parent"))?;
        fs::copy(&source, &destination)?;
    }
    let list = ordered
        .iter()
        .map(|path| format!("/usr/lib/modules/{release}/{path}\n"))
        .collect::<String>();
    fs::write(tree.join("modules.load"), list)?;
    Ok((release, ordered.len(), firmware_requirements.len()))
}

fn build_installer(repo_root: &Path) -> Result<()> {
    let btrfs_root = repo_root.join("out/build/btrfs-progs");
    let btrfs_source = btrfs_root.join("source");
    let btrfs_install = btrfs_root.join("install");
    sync_build_source(
        &repo_root.join("src/system/storage/btrfs-progs"),
        &btrfs_source,
    )?;
    if !btrfs_source.join("configure").is_file() {
        run_cmd(&btrfs_source, "autoreconf", &["-fiv"])?;
    }
    let btrfs_env = staged_library_environment(repo_root, &["util-linux", "zlib", "zstd"])?;
    if !btrfs_source.join("config.status").is_file() {
        run_cmd_with_env_overrides(
            &btrfs_source,
            "./configure",
            &[
                "--prefix=/usr",
                "--bindir=/usr/bin",
                "--libdir=/usr/lib/x86_64-linux-gnu",
                "--disable-documentation",
                "--disable-python",
                "--disable-convert",
                "--disable-zoned",
                "--disable-lzo",
                "--disable-libudev",
                "--disable-backtrace",
            ],
            &btrfs_env,
        )?;
    }
    run_cmd_with_env_overrides(&btrfs_source, "make", &[], &btrfs_env)?;
    remove_path_if_exists(&btrfs_install)?;
    run_cmd_with_env_overrides(
        &btrfs_source,
        "make",
        &["install", &format!("DESTDIR={}", btrfs_install.display())],
        &btrfs_env,
    )?;
    for required in ["usr/bin/btrfs", "usr/bin/mkfs.btrfs"] {
        if !btrfs_install.join(required).is_file() {
            bail!("Btrfs installer build did not produce {required}");
        }
    }
    let dosfs_root = repo_root.join("out/build/dosfstools");
    let dosfs_source = dosfs_root.join("source");
    let dosfs_build = dosfs_root.join("build");
    let dosfs_install = dosfs_root.join("install");
    sync_build_source(
        &repo_root.join("src/system/storage/dosfstools"),
        &dosfs_source,
    )?;
    if !dosfs_source.join("configure").is_file() || !dosfs_source.join("config.rpath").is_file() {
        run_cmd(&dosfs_source, "./autogen.sh", &[])?;
        remove_path_if_exists(&dosfs_build)?;
    }
    fs::create_dir_all(&dosfs_build)?;
    if !dosfs_build.join("Makefile").is_file() {
        run_cmd(
            &dosfs_build,
            path_str(&dosfs_source.join("configure"))?,
            &["--prefix=/usr", "--sbindir=/usr/sbin"],
        )?;
    }
    run_cmd(&dosfs_build, "make", &[])?;
    remove_path_if_exists(&dosfs_install)?;
    run_cmd(
        &dosfs_build,
        "make",
        &["install", &format!("DESTDIR={}", dosfs_install.display())],
    )?;
    if !dosfs_install.join("usr/sbin/mkfs.fat").is_file() {
        bail!("dosfstools installer build did not produce usr/sbin/mkfs.fat");
    }

    let e2fs_root = repo_root.join("out/build/e2fsprogs");
    let e2fs_source = e2fs_root.join("source");
    let e2fs_build = e2fs_root.join("build");
    let e2fs_install = e2fs_root.join("install");
    sync_build_source(
        &repo_root.join("src/system/storage/e2fsprogs"),
        &e2fs_source,
    )?;
    remove_path_if_exists(&e2fs_build)?;
    fs::create_dir_all(&e2fs_build)?;
    let e2fs_env = staged_library_environment(repo_root, &["util-linux"])?;
    if !e2fs_build.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &e2fs_build,
            path_str(&e2fs_source.join("configure"))?,
            &[
                "--prefix=/usr",
                "--sbindir=/usr/sbin",
                "--libdir=/usr/lib/x86_64-linux-gnu",
                "--sysconfdir=/etc",
                "--disable-nls",
                "--disable-uuidd",
                "--disable-fuse2fs",
                "--disable-fsck",
            ],
            &e2fs_env,
        )?;
    }
    run_cmd_with_env_overrides(&e2fs_build, "make", &[], &e2fs_env)?;
    remove_path_if_exists(&e2fs_install)?;
    run_cmd_with_env_overrides(
        &e2fs_build,
        "make",
        &["install", &format!("DESTDIR={}", e2fs_install.display())],
        &e2fs_env,
    )?;
    if !e2fs_install.join("usr/sbin/mkfs.ext4").is_file() {
        bail!("e2fsprogs installer build did not produce usr/sbin/mkfs.ext4");
    }
    let util_linux_lib = repo_root.join("out/build/util-linux/install/usr/lib/x86_64-linux-gnu");
    validate_dependency_resolves_from(
        &e2fs_install.join("usr/sbin/mkfs.ext4"),
        "libblkid.so.1",
        &util_linux_lib,
        &[&util_linux_lib],
    )?;
    validate_dependency_resolves_from(
        &e2fs_install.join("usr/sbin/mkfs.ext4"),
        "libuuid.so.1",
        &util_linux_lib,
        &[&util_linux_lib],
    )?;

    let installer_out = repo_root.join("out/build/installer");
    let cargo_target = installer_out.join("cargo-target");
    fs::create_dir_all(&installer_out)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/system/installer/Cargo.toml",
        ],
        &[("CARGO_TARGET_DIR", cargo_target.display().to_string())],
    )?;

    build_cosmic_installer_frontend(repo_root, &installer_out)?;

    let source = repo_root.join("src/system/installer/engine/installed-init.c");
    let compiler = repo_root.join("out/build/gcc-toolchain/install/usr/bin/gcc");
    let sysroot = repo_root.join("out/sysroot");
    let init_tree = performance::temporary_sibling(
        &repo_root.join("out/build/installed-initramfs-root"),
        "building",
    )?;
    fs::create_dir_all(&init_tree)?;
    let init = init_tree.join("init");
    let sysroot_arg = format!("--sysroot={}", sysroot.display());
    let libc_search = format!("-B{}/usr/lib/x86_64-linux-gnu/", sysroot.display());
    let gcc_search = format!(
        "-B{}/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0/",
        sysroot.display()
    );
    let libc_link = format!("-L{}/usr/lib/x86_64-linux-gnu", sysroot.display());
    let gcc_link = format!(
        "-L{}/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0",
        sysroot.display()
    );
    run_cmd(
        repo_root,
        path_str(&compiler)?,
        &[
            &sysroot_arg,
            &libc_search,
            &gcc_search,
            &libc_link,
            &gcc_link,
            "-std=c11",
            "-Os",
            "-static",
            "-s",
            "-fno-ident",
            "-Wl,--build-id=none",
            "-Wall",
            "-Wextra",
            "-Werror",
            path_str(&source)?,
            "-o",
            path_str(&init)?,
        ],
    )?;
    set_mode(init, 0o755)?;
    let (installed_module_release, installed_module_count, installed_firmware_count) =
        stage_boot_module_closure(repo_root, &init_tree)?;
    let installed_initramfs = repo_root.join("out/build/installed-initramfs.cpio.xz");
    let archive_command = format!(
        "find . -exec touch -h -d @{MATTOS_SOURCE_DATE_EPOCH} {{}} + && find . -print0 | sort -z | cpio --null -o --quiet --reproducible --owner=0:0 --format=newc | xz -1 -T1 --check=crc32 --stdout > {}",
        shell_escape(path_str(&installed_initramfs)?)
    );
    run_cmd(&init_tree, "bash", &["-lc", &archive_command])?;
    println!(
        "installed initramfs: {installed_module_count} boot-critical modules and {installed_firmware_count} required firmware files for {installed_module_release}"
    );
    remove_path_if_exists(&init_tree)?;

    let efi = installer_out.join("BOOTX64.EFI");
    run_cmd(
        repo_root,
        "grub-mkimage",
        &[
            "-O",
            "x86_64-efi",
            "-d",
            "/usr/lib/grub/x86_64-efi",
            "-p",
            "/EFI/BOOT",
            "-o",
            path_str(&efi)?,
            "part_gpt",
            "fat",
            "btrfs",
            "normal",
            "configfile",
            "search",
            "search_fs_uuid",
            "linux",
            "serial",
            "terminal",
        ],
    )?;
    if fs::metadata(&efi)?.len() < 128 * 1024 {
        bail!("generated installed-system EFI GRUB image is unexpectedly small");
    }
    Ok(())
}

fn build_cosmic_installer_frontend(repo_root: &Path, installer_out: &Path) -> Result<()> {
    let source_root = installer_out.join("cosmic-source");
    // This is an output-owned assembly mirror, not a cache. Recreate it so a
    // dependency demoted from first-class source cannot survive as stale
    // apparent vendored input. The separate cosmic-target retains Cargo's
    // incremental build products.
    remove_path_if_exists(&source_root)?;
    fs::create_dir_all(&source_root)?;
    let libcosmic = source_root.join("libcosmic");
    let iced = libcosmic.join("iced");
    let protocols = source_root.join("cosmic-protocols");
    let application = source_root.join("mattos-installer-cosmic");

    sync_build_source(&repo_root.join("src/desktop/cosmic/libcosmic"), &libcosmic)?;
    sync_build_source(&repo_root.join("src/desktop/cosmic/iced"), &iced)?;
    sync_build_source(
        &repo_root.join("src/desktop/cosmic/cosmic-protocols"),
        &protocols,
    )?;
    remove_path_if_exists(&application)?;
    fs::create_dir_all(application.join("src"))?;
    fs::copy(
        repo_root.join("src/system/installer/gui/cosmic/main.rs"),
        application.join("src/main.rs"),
    )?;
    let lock = repo_root.join("src/system/installer/gui/cosmic/Cargo.lock");
    validate_cosmic_installer_lock(&lock)?;
    fs::copy(&lock, application.join("Cargo.lock"))?;

    let template =
        fs::read_to_string(repo_root.join("src/system/installer/gui/cosmic/Cargo.toml.in"))?;
    let installer_manifest = repo_root.join("src/system/installer").canonicalize()?;
    let mut manifest = template
        .replace("@MATTOS_INSTALLER_PATH@", path_str(&installer_manifest)?)
        .replace("@LIBCOSMIC_PATH@", path_str(&libcosmic.canonicalize()?)?);
    manifest.push_str(&format!(
        "\n[patch.\"https://github.com/pop-os/cosmic-protocols\"]\ncosmic-client-toolkit = {{ path = {:?} }}\ncosmic-protocols = {{ path = {:?} }}\n",
        protocols.join("client-toolkit"), protocols
    ));
    fs::write(application.join("Cargo.toml"), manifest)?;

    let target = installer_out.join("cosmic-target");
    let xkbcommon = repo_root.join("out/build/xkbcommon/install/usr");
    let xkbcommon_lib = xkbcommon.join("lib/x86_64-linux-gnu");
    let xkbcommon_pc = xkbcommon_lib.join("pkgconfig");
    if !xkbcommon_lib.join("libxkbcommon.so.0").is_file()
        || !xkbcommon_pc.join("xkbcommon.pc").is_file()
    {
        bail!(
            "MattOS-built xkbcommon runtime/development metadata is missing; run build xkbcommon first"
        );
    }
    run_cmd_with_env_overrides(
        &application,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "Cargo.toml",
        ],
        &[
            ("CARGO_TARGET_DIR", target.display().to_string()),
            ("PKG_CONFIG_PATH", xkbcommon_pc.display().to_string()),
            ("PKG_CONFIG_LIBDIR", xkbcommon_pc.display().to_string()),
            // The .pc file has prefix=/usr.  Its sysroot is the DESTDIR root,
            // not `/usr`, otherwise pkg-config invents `/usr/usr/lib` and
            // Cargo silently falls back to a host xkbcommon.
            (
                "PKG_CONFIG_SYSROOT_DIR",
                xkbcommon
                    .parent()
                    .expect("xkbcommon install root")
                    .display()
                    .to_string(),
            ),
            ("LIBRARY_PATH", xkbcommon_lib.display().to_string()),
            ("LD_LIBRARY_PATH", xkbcommon_lib.display().to_string()),
        ],
    )?;
    let binary = target.join("release/mattos-install-cosmic");
    if !binary.is_file() {
        bail!(
            "native COSMIC installer build did not produce {}",
            binary.display()
        );
    }
    validate_dependency_resolves_from(
        &binary,
        "libxkbcommon.so.0",
        &xkbcommon_lib,
        &[&xkbcommon_lib],
    )?;
    Ok(())
}

const COSMIC_INSTALLER_LOCKED_GIT_SOURCES: &[&str] = &[
    "git+https://github.com/iced-rs/cryoglyph.git?rev=e429a025df36ab8145708acb309080ae3deec17a#e429a025df36ab8145708acb309080ae3deec17a",
    "git+https://github.com/jackpot51/rust-atomicwrites#043ab4859d53ffd3d55334685303d8df39c9f768",
    "git+https://github.com/pop-os/dbus-settings-bindings#eed01dd3609e90e3c8cd043656734c500956c793",
    "git+https://github.com/pop-os/freedesktop-icons#ab4c57b8e416c6af9297cb04d101889896fd9a92",
    "git+https://github.com/pop-os/smithay-clipboard?tag=sctk-0.20#859b02c88f45c554049a67c6ddeec1692ce0e20b",
    "git+https://github.com/pop-os/softbuffer?tag=cosmic-4.0#c2b2c19ddb38ff17495643699f97cb1f2064a1be",
    "git+https://github.com/pop-os/window_clipboard.git?tag=sctk-0.20#f68595ee0e62fbd6589f4709b5aaa5c3c7ea5f6c",
    "git+https://github.com/pop-os/winit.git?tag=cosmic-0.14#71ce08c043814514a8fd92d9d0599f115ae854e8",
    "git+https://github.com/wash2/accesskit?tag=cosmic-0.14#f0599eed5f18111228266fe3f28991cc48b5964f",
];

fn validate_cosmic_installer_lock(path: &Path) -> Result<()> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read native COSMIC lock {}", path.display()))?;
    let document: toml::Value = toml::from_str(&contents)
        .with_context(|| format!("failed to parse native COSMIC lock {}", path.display()))?;
    let packages = document
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| anyhow!("native COSMIC lock has no package records"))?;
    let mut git_sources = BTreeSet::new();
    for package in packages {
        let name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .unwrap_or("<unnamed>");
        let Some(source) = package.get("source").and_then(toml::Value::as_str) else {
            continue;
        };
        if source.starts_with("registry+") {
            let checksum = package
                .get("checksum")
                .and_then(toml::Value::as_str)
                .unwrap_or("");
            if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                bail!("native COSMIC registry package {name} lacks a SHA-256 checksum");
            }
        } else if source.starts_with("git+") {
            let revision = source
                .rsplit_once('#')
                .map(|(_, revision)| revision)
                .unwrap_or("");
            if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                bail!(
                    "native COSMIC Git package {name} is not pinned to an exact commit: {source}"
                );
            }
            git_sources.insert(source.to_string());
        }
    }
    let expected = COSMIC_INSTALLER_LOCKED_GIT_SOURCES
        .iter()
        .map(|source| (*source).to_string())
        .collect::<BTreeSet<_>>();
    if git_sources != expected {
        bail!(
            "native COSMIC Git source set differs from the reviewed lock policy\nexpected: {expected:#?}\nactual: {git_sources:#?}"
        );
    }
    Ok(())
}

fn build_rootfs_atomic(repo_root: &Path) -> Result<()> {
    let destination = repo_root.join("out/build/rootfs");
    let temp = performance::temporary_sibling(&destination, "building")?;
    let result = build_rootfs_into(repo_root, &temp);
    if let Err(error) = result {
        let _ = remove_path_if_exists(&temp);
        return Err(error);
    }
    validate_rootfs_mutable_state(&temp)?;
    validate_udev_storage_identity_support(&temp)?;
    packaging::validate_udev_hwdb_payload(repo_root, &temp)?;
    performance::atomic_replace_path(&temp, &destination)
}

fn validate_cached_rootfs(repo_root: &Path) -> Result<()> {
    let rootfs = repo_root.join("out/build/rootfs");
    validate_rootfs_mutable_state(&rootfs)?;
    validate_live_desktop_boot_contract(&rootfs)?;
    validate_udev_storage_identity_support(&rootfs)?;
    packaging::validate_udev_hwdb_payload(repo_root, &rootfs)?;
    for rel in [
        "var/lib/dpkg/status",
        "usr/share/mattos/repository/dists/trixie/Release",
        "usr/bin/sh",
        "usr/bin/bash",
        "usr/lib/systemd/systemd",
    ] {
        if !rootfs.join(rel).symlink_metadata().is_ok() {
            bail!("cached rootfs required path is missing: /{rel}");
        }
    }
    Ok(())
}

fn validate_udev_storage_identity_support(rootfs: &Path) -> Result<()> {
    let rules_path = rootfs.join("usr/lib/udev/rules.d/60-persistent-storage.rules");
    let rules = fs::read_to_string(&rules_path)
        .with_context(|| format!("failed to read {}", rules_path.display()))?;
    for required in [
        "IMPORT{builtin}=\"blkid\"",
        "disk/by-uuid/$env{ID_FS_UUID_ENC}",
        "disk/by-partuuid/$env{ID_PART_ENTRY_UUID}",
    ] {
        if !rules.contains(required) {
            bail!(
                "udev persistent-storage rules cannot materialize installed fstab identities: missing {required}"
            );
        }
    }
    let osc_profile = fs::read_to_string(rootfs.join("etc/profile.d/80-systemd-osc-context.sh"))?;
    if osc_profile.contains("PROMPT_COMMAND+=(") {
        bail!("systemd OSC profile contains Bash array syntax rejected by the MattOS login shell");
    }
    if !osc_profile.contains("command -v shopt >/dev/null 2>&1 || return 0") {
        bail!(
            "systemd OSC profile does not guard its Bash-only prompt setup by builtin availability"
        );
    }
    Ok(())
}

fn validate_rootfs_mutable_state(rootfs: &Path) -> Result<()> {
    for rel in [
        "run/dbus/system_bus_socket",
        "var/lib/dpkg/lock",
        "var/lib/dpkg/lock-frontend",
        "var/lib/apt/lists/lock",
        "var/cache/apt/archives/lock",
        "etc/udev/hwdb.bin",
    ] {
        if rootfs.join(rel).symlink_metadata().is_ok() {
            bail!("mutable lock/socket state is present in cached rootfs: /{rel}");
        }
    }
    Ok(())
}

fn validate_live_desktop_boot_contract(rootfs: &Path) -> Result<()> {
    for rel in [
        "usr/lib/systemd/system/mattos-live-graphical.target",
        "usr/lib/systemd/system/mattos.target",
        "usr/lib/systemd/system/graphical.target",
        "usr/lib/systemd/system/cosmic-greeter.service",
        "etc/systemd/system/display-manager.service",
        "etc/systemd/system/cosmic-greeter.service.d/live.conf",
        "etc/greetd/cosmic-live.toml",
        "etc/pam.d/cosmic-greeter",
        "usr/bin/greetd",
        "usr/bin/start-cosmic",
        "usr/bin/cosmic-session",
        "usr/bin/cosmic-panel",
        "usr/bin/cosmic-launcher",
        "usr/bin/cosmic-term",
        "home/mattos",
    ] {
        if !path_entry_exists(&rootfs.join(rel)) {
            bail!("graphical live boot contract is missing /{rel}")
        }
    }

    let graphical =
        fs::read_to_string(rootfs.join("usr/lib/systemd/system/mattos-live-graphical.target"))?;
    if !graphical.contains("Requires=graphical.target")
        || !graphical.contains("After=graphical.target")
    {
        bail!("graphical live target does not enter the production graphical target")
    }
    let cli = fs::read_to_string(rootfs.join("usr/lib/systemd/system/mattos.target"))?;
    if !cli.contains("Requires=multi-user.target") || cli.contains("graphical.target") {
        bail!("CLI live target must require only the non-graphical system target")
    }
    let live_config = fs::read_to_string(rootfs.join("etc/greetd/cosmic-live.toml"))?;
    for contract in [
        "[initial_session]",
        "command = \"/usr/bin/start-cosmic\"",
        "user = \"mattos\"",
        "[default_session]",
        "command = \"/usr/bin/cosmic-greeter-start\"",
    ] {
        if !live_config.contains(contract) {
            bail!("live greetd configuration is missing contract: {contract}")
        }
    }
    let override_unit =
        fs::read_to_string(rootfs.join("etc/systemd/system/cosmic-greeter.service.d/live.conf"))?;
    if !override_unit.contains("ExecStart=/usr/bin/greetd --config /etc/greetd/cosmic-live.toml") {
        bail!("live display-manager override does not select the live greetd configuration")
    }
    let pam = fs::read_to_string(rootfs.join("etc/pam.d/cosmic-greeter"))?;
    if pam
        .matches("session    optional     pam_systemd.so")
        .count()
        != 1
    {
        bail!("live COSMIC session lacks exactly one PAM/logind session hook")
    }
    let display_manager =
        fs::read_to_string(rootfs.join("usr/lib/systemd/system/cosmic-greeter.service"))?;
    if !display_manager.contains(
        "Wants=systemd-logind.service systemd-udev-trigger.service cosmic-greeter-daemon.service",
    ) {
        bail!("display manager does not pull in its greeter account service")
    }
    if path_entry_exists(
        &rootfs.join("etc/systemd/system/multi-user.target.wants/cosmic-greeter-daemon.service"),
    ) {
        bail!("CLI boot must not start the COSMIC greeter daemon through multi-user.target")
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(rootfs.join("home/mattos"))?
            .permissions()
            .mode()
            & 0o7777;
        if mode != 0o750 {
            bail!("live home has mode {mode:04o}; expected 0750")
        }
    }
    Ok(())
}

fn build_rootfs_into(repo_root: &Path, out: &Path) -> Result<()> {
    let skeleton = repo_root.join("src/rootfs/skeleton");
    fs::create_dir_all(out).with_context(|| format!("failed to create {}", out.display()))?;
    packaging::install_prototype_packages(repo_root, out)?;
    packaging::apply_live_apt_policy(repo_root, out)?;
    let release = fs::read_to_string(repo_root.join("out/build/linux/kernel-release"))?
        .trim()
        .to_owned();
    run_cmd(
        repo_root,
        "depmod",
        &["-b", path_str(out)?, "-m", "/usr/lib/modules", &release],
    )?;
    let aliases = fs::read_to_string(
        out.join("usr/lib/modules")
            .join(&release)
            .join("modules.alias"),
    )?;
    if !aliases.contains(" nvidia") || !aliases.contains(" nouveau") {
        bail!("rootfs depmod metadata does not preserve both NVIDIA and Nouveau aliases");
    }
    let package_owned = packaging::package_owned_paths(out)?;
    let package_snapshot = packaging::snapshot_package_files(out, &package_owned)?;
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
    generate_baseline_locale(repo_root, out)?;
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
        bail!("coreutils package did not install /usr/bin/coreutils")
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
            bail!("coreutils package did not install alias /usr/bin/{applet}")
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
        bail!("curl package did not install /usr/bin/curl")
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
    validate_live_desktop_boot_contract(&out)?;
    packaging::embed_repository(repo_root, &out)?;
    packaging::validate_dpkg_database(&out)?;
    performance::timed(
        "rootfs-package-audit",
        "n/a",
        "validate package-owned files after rootfs overlays",
        "rootfs-package-snapshot",
        || packaging::validate_package_snapshot(&out, &package_snapshot),
    )?;
    performance::timed(
        "rootfs-elf-audit",
        "n/a",
        "validate complete MattOS glibc and ELF runtime closure",
        "rootfs-elf-inventory",
        || validate_glibc_rootfs(repo_root, &out),
    )?;

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
        let relative = path.strip_prefix(rootfs)?;
        // Firmware may itself use ELF as a container for code executed by an
        // embedded GPU or device processor (for example NVIDIA GSP RISC-V).
        // It is data from the host CPU's perspective, not part of its dynamic
        // executable/library closure.
        if relative.starts_with("usr/lib/firmware") {
            continue;
        }
        let Some(facts) = elf_cache::inspect(repo_root, &path)? else {
            continue;
        };
        if !facts.architecture.contains("X86-64") {
            bail!(
                "ELF object /{} has unexpected architecture {}",
                relative.display(),
                facts.architecture
            );
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
        if let Some(value) = &facts.soname {
            provided.insert(value.clone());
            soname_providers
                .entry(value.clone())
                .or_default()
                .push(format!("/{}", path.strip_prefix(rootfs)?.display()));
        }
        elf_files.push((path, facts));
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
    for (path, facts) in &elf_files {
        let relative = format!("/{}", path.strip_prefix(rootfs)?.display());
        let interpreter = facts.interpreter.clone();
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

        let mut runtime_needs = Vec::new();
        for needed in &facts.needed {
            if !provided.contains(needed) {
                bail!(
                    "ELF object {relative} needs {needed}, which is absent from the MattOS rootfs"
                )
            }
            if needed == "libgcc_s.so.1" || needed == "libstdc++.so.6" {
                runtime_needs.push(needed.to_string());
            }
        }
        for value in facts.rpath.iter().chain(&facts.runpath) {
            if value.contains("/home/")
                || value.contains("/tmp/")
                || value.contains("/usr/local/")
                || value.contains("/opt/")
            {
                bail!(
                    "ELF object {relative} embeds a host-style absolute library search path: {value}"
                )
            }
        }

        let versions = |prefix: &str| {
            facts
                .symbol_versions
                .iter()
                .filter(|version| version.starts_with(prefix))
                .cloned()
                .collect::<BTreeSet<_>>()
        };
        let glibc_versions = versions("GLIBC_");
        let glibcxx_versions = versions("GLIBCXX_");
        let cxxabi_versions = versions("CXXABI_");
        let gcc_versions = versions("GCC_");
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
        bail!("procps did not install the authoritative /etc/sysctl.conf");
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
    // NetworkManager owns interface configuration. Keep the legacy networkd
    // source and unit available for recovery, but do not install an active
    // .network policy that can race NetworkManager.
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
        "NetworkManager.service",
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

    let networkd_mask = rootfs.join("etc/systemd/system/systemd-networkd.service");
    if path_entry_exists(&networkd_mask) {
        remove_path_if_exists(&networkd_mask)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("/dev/null", &networkd_mask)
        .context("failed to mask systemd-networkd")?;

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
            bail!("dbus-broker did not install authoritative user unit {rel}");
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
        bail!("dbus-broker did not install authoritative session bus policy");
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
    for stack in ["login", "su-l", "systemd-user", "sshd"] {
        let body = fs::read_to_string(rootfs.join("etc/pam.d").join(stack))
            .with_context(|| format!("failed to read effective PAM stack {stack}"))?;
        if body.matches(expected_hook).count() != 1 {
            bail!("PAM stack {stack} must contain exactly one optional pam_systemd session hook");
        }
    }
    let greeter_stack = rootfs.join("etc/pam.d/cosmic-greeter");
    if greeter_stack.is_file() {
        let body = fs::read_to_string(&greeter_stack)?;
        if body.matches(expected_hook).count() != 1 {
            bail!(
                "PAM stack cosmic-greeter must contain exactly one optional pam_systemd session hook"
            );
        }
    }
    if fs::read_to_string(rootfs.join("usr/share/pam/security/pam_env.conf"))?
        .trim()
        .is_empty()
    {
        bail!("source-built PAM environment defaults must not be empty");
    }
    for entry in fs::read_dir(rootfs.join("etc/pam.d"))? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
        if matches!(
            name,
            "login" | "su-l" | "systemd-user" | "sshd" | "cosmic-greeter"
        ) {
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
                "dbus-broker did not install authoritative /{}",
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
            "dbus-org.freedesktop.locale1.service",
            "systemd-localed.service",
        ),
        (
            "dbus-org.freedesktop.login1.service",
            "systemd-logind.service",
        ),
    ];
    for (alias, target) in aliases {
        install_systemd_service_alias(rootfs, alias, target)?;
    }

    validate_locale_service(rootfs)?;

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
    // The reference daemon may be installed solely for dbus-run-session's
    // private, process-scoped buses. It must never own the system/user bus or
    // appear under the legacy sbin path; dbus-broker remains the only
    // systemd-managed implementation.
    if rootfs.join("usr/sbin/dbus-daemon").exists() {
        bail!("legacy dbus-daemon system path found in rootfs");
    }
    for binary in ["usr/bin/dbus-broker", "usr/bin/dbus-broker-launch"] {
        validate_executable_runtime_closure(&rootfs.join(binary), rootfs)?;
    }
    if rootfs.join("usr/bin/dbus-daemon").is_file() {
        for binary in [
            "usr/bin/dbus-daemon",
            "usr/bin/dbus-run-session",
            "usr/bin/dbus-update-activation-environment",
        ] {
            validate_executable_runtime_closure(&rootfs.join(binary), rootfs)?;
        }
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
        "etc/systemd/resolved.conf",
        "etc/systemd/timesyncd.conf",
        "etc/nsswitch.conf",
        "etc/ssl/certs/ca-certificates.crt",
        "run/systemd/resolve",
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
        if !path_entry_exists(&rootfs.join(rel)) {
            bail!("required network runtime path missing: /{rel}");
        }
    }
    if !path_entry_exists(&rootfs.join("etc/systemd/system/systemd-networkd.service"))
        || fs::read_link(rootfs.join("etc/systemd/system/systemd-networkd.service"))?
            != Path::new("/dev/null")
    {
        bail!("systemd-networkd must be masked when NetworkManager is active");
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
        "usr/lib/systemd/systemd-localed",
        "usr/lib/systemd/systemd-logind",
        "usr/lib/systemd/systemd-user-runtime-dir",
        "usr/bin/systemctl",
        "usr/bin/journalctl",
        "usr/bin/busctl",
        "usr/bin/loginctl",
        "usr/bin/networkctl",
        "usr/bin/resolvectl",
        "usr/bin/timedatectl",
        "usr/bin/localectl",
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

fn validate_locale_service(rootfs: &Path) -> Result<()> {
    for rel in [
        "usr/lib/systemd/systemd-localed",
        "usr/lib/systemd/system/systemd-localed.service",
        "usr/lib/systemd/system/dbus-org.freedesktop.locale1.service",
        "usr/share/dbus-1/system-services/org.freedesktop.locale1.service",
        "usr/share/dbus-1/system.d/org.freedesktop.locale1.conf",
        "usr/bin/localectl",
    ] {
        if !rootfs.join(rel).exists() {
            bail!("systemd-localed runtime contract is missing /{rel}");
        }
    }
    Ok(())
}

fn generate_baseline_locale(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let glibc_install = repo_root.join("out/build/glibc/install");
    let loader = glibc_install.join("lib64/ld-linux-x86-64.so.2");
    let localedef = glibc_install.join("usr/bin/localedef");
    if !loader.is_file() || !localedef.is_file() {
        bail!("glibc localedef runtime is missing; cannot generate baseline locale");
    }
    fs::create_dir_all(rootfs.join("usr/lib/x86_64-linux-gnu/locale"))?;
    let library_path = std::env::join_paths([
        glibc_install.join("usr/lib/x86_64-linux-gnu"),
        glibc_install.join("lib64"),
    ])?;
    let prefix = format!("--prefix={}", rootfs.display());
    let library_path = library_path
        .to_str()
        .ok_or_else(|| anyhow!("glibc locale library path is not valid UTF-8"))?;
    let i18n_path = glibc_install.join("usr/share/i18n");
    let i18n_path = i18n_path
        .to_str()
        .ok_or_else(|| anyhow!("glibc i18n source path is not valid UTF-8"))?;
    run_cmd_with_env_overrides(
        repo_root,
        path_str(&loader)?,
        &[
            "--library-path",
            library_path,
            path_str(&localedef)?,
            &prefix,
            "-i",
            "en_US",
            "-f",
            "UTF-8",
            "--no-archive",
            "en_US.UTF-8",
        ],
        &[("I18NPATH", i18n_path.to_string())],
    )?;
    let locale_dir = rootfs.join("usr/lib/x86_64-linux-gnu/locale");
    if !locale_dir.join("en_US.utf8").exists() {
        bail!("baseline en_US.UTF-8 generation produced no compiled en_US.utf8 locale");
    }
    fs::write(rootfs.join("etc/locale.conf"), "LANG=en_US.UTF-8\n")?;
    Ok(())
}

fn resolve_coreutils_multicall(repo_root: &Path) -> Result<PathBuf> {
    let candidates = [
        repo_root.join("out/build/coreutils/cargo-target/release/coreutils"),
        repo_root.join("out/build/coreutils/cargo-target/release/uutils"),
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

fn build_live_root(repo_root: &Path) -> Result<()> {
    let spec = build_stage_spec(BuildStage::LiveRoot);
    performance::execute_cached_stage(
        repo_root,
        &spec,
        || validate_cached_live_root(repo_root),
        || build_live_root_atomic(repo_root),
    )
}

fn validate_cached_live_root(repo_root: &Path) -> Result<()> {
    validate_squashfs_image(&repo_root.join(LIVE_ROOT_IMAGE_PATH))?;
    let inventory = repo_root.join("out/reports/live-root-inventory.tsv");
    if !inventory.is_file() {
        bail!("live-root inventory is missing: {}", inventory.display());
    }
    Ok(())
}

fn has_squashfs_header(path: &Path) -> Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 4];
    Ok(file.read_exact(&mut header).is_ok() && header == *b"hsqs")
}

fn validate_squashfs_image(path: &Path) -> Result<()> {
    if !has_squashfs_header(path)? {
        bail!("live root is not a SquashFS image: {}", path.display());
    }
    if fs::metadata(path)?.len() < 1024 * 1024 {
        bail!(
            "live-root SquashFS is unexpectedly small: {}",
            path.display()
        );
    }
    let output = Command::new("unsquashfs")
        .args(["-stat"])
        .arg(path)
        .output()
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if !output.status.success() {
        bail!("unsquashfs rejected live root {}", path.display());
    }
    Ok(())
}

fn regular_file_bytes(root: &Path) -> Result<(u64, u64)> {
    fn visit(path: &Path, files: &mut u64, bytes: &mut u64) -> Result<()> {
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                visit(&path, files, bytes)?;
            } else if metadata.is_file() {
                *files += 1;
                *bytes += metadata.len();
            }
        }
        Ok(())
    }
    let mut files = 0;
    let mut bytes = 0;
    visit(root, &mut files, &mut bytes)?;
    Ok((files, bytes))
}

fn largest_regular_files(root: &Path, limit: usize) -> Result<Vec<(u64, String)>> {
    fn visit(root: &Path, path: &Path, files: &mut Vec<(u64, String)>) -> Result<()> {
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                visit(root, &path, files)?;
            } else if metadata.is_file() {
                files.push((
                    metadata.len(),
                    path.strip_prefix(root)?
                        .to_string_lossy()
                        .replace('\\', "/"),
                ));
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    files.truncate(limit);
    Ok(files)
}

fn build_live_root_atomic(repo_root: &Path) -> Result<()> {
    let rootfs = repo_root.join("out/build/rootfs");
    if !rootfs.is_dir() {
        bail!("rootfs not found; run build rootfs first");
    }
    let destination = repo_root.join(LIVE_ROOT_IMAGE_PATH);
    let temp = performance::temporary_sibling(&destination, "building")?;
    let processors = scheduler::child_job_limit().clamp(1, 4).to_string();
    let result = run_cmd(
        repo_root,
        "mksquashfs",
        &[
            path_str(&rootfs)?,
            path_str(&temp)?,
            "-noappend",
            "-comp",
            "xz",
            "-b",
            "1M",
            "-processors",
            &processors,
            "-all-root",
            "-no-progress",
            "-no-recovery",
        ],
    );
    if let Err(error) = result {
        let _ = remove_path_if_exists(&temp);
        return Err(error);
    }
    if let Err(error) = validate_squashfs_image(&temp) {
        let _ = remove_path_if_exists(&temp);
        return Err(error);
    }

    let (files, uncompressed_bytes) = regular_file_bytes(&rootfs)?;
    let compressed_bytes = fs::metadata(&temp)?.len();
    let reports = repo_root.join("out/reports");
    fs::create_dir_all(&reports)?;
    fs::write(
        reports.join("live-root-inventory.tsv"),
        format!(
            "artifact\tfilesystem\tregular_files\tuncompressed_regular_bytes\tcompressed_bytes\tordinary_payload_in_early_initramfs\n{}\tsquashfs-xz\t{}\t{}\t{}\t0\n",
            LIVE_ROOT_IMAGE_PATH, files, uncompressed_bytes, compressed_bytes
        ),
    )?;
    performance::atomic_replace_path(&temp, &destination)
}

fn build_initramfs(repo_root: &Path) -> Result<()> {
    let spec = build_stage_spec(BuildStage::Initramfs);
    performance::execute_cached_stage(
        repo_root,
        &spec,
        || validate_cached_initramfs(repo_root),
        || build_initramfs_atomic(repo_root),
    )
}

fn validate_cached_initramfs(repo_root: &Path) -> Result<()> {
    validate_early_initramfs(&repo_root.join(INITRAMFS_ARCHIVE_PATH))
}

fn has_xz_header(path: &Path) -> Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 6];
    Ok(file.read_exact(&mut header).is_ok() && header == [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00])
}

fn validate_early_initramfs(path: &Path) -> Result<()> {
    if !has_xz_header(path)? {
        bail!("initramfs is not an XZ stream: {}", path.display());
    }
    let size = fs::metadata(path)?.len();
    if size > EARLY_INITRAMFS_SIZE_LIMIT {
        bail!(
            "early initramfs is {size} bytes, above its structural limit of {EARLY_INITRAMFS_SIZE_LIMIT} bytes"
        );
    }
    let listing = Command::new("bash")
        .args([
            "-o",
            "pipefail",
            "-c",
            &format!(
                "xz -dc {} | cpio -it --quiet",
                shell_escape(path_str(path)?)
            ),
        ])
        .output()
        .with_context(|| format!("failed to inventory {}", path.display()))?;
    if !listing.status.success() {
        bail!("failed to list early initramfs {}", path.display());
    }
    let paths = String::from_utf8(listing.stdout).context("initramfs listing was not UTF-8")?;
    let normalized = paths
        .lines()
        .map(|line| line.trim_start_matches("./"))
        .collect::<Vec<_>>();
    if !normalized.contains(&"init") {
        bail!("early initramfs does not contain /init");
    }
    for forbidden in [
        "python", "clang", "llvm", "rustc", "cargo", "git", "systemd", "brush",
    ] {
        if normalized.iter().any(|path| path.contains(forbidden)) {
            bail!("general userland token {forbidden} leaked into early initramfs");
        }
    }
    Ok(())
}

fn build_initramfs_atomic(repo_root: &Path) -> Result<()> {
    let out_build = repo_root.join("out/build");
    fs::create_dir_all(&out_build).context("failed to create out/build directory")?;
    let destination = repo_root.join(INITRAMFS_ARCHIVE_PATH);
    let temp = performance::temporary_sibling(&destination, "building")?;
    let tree = performance::temporary_sibling(&out_build.join("early-initramfs-root"), "building")?;
    fs::create_dir_all(&tree)?;
    set_mode(tree.clone(), 0o755)?;
    let source = repo_root.join("src/boot/live-init.c");
    let compiler = repo_root.join("out/build/gcc-toolchain/install/usr/bin/gcc");
    let sysroot = repo_root.join("out/sysroot");
    if !source.is_file() || !compiler.is_file() || !sysroot.is_dir() {
        bail!("early-init source or MattOS compiler/sysroot is missing");
    }
    let init = tree.join("init");
    let sysroot_arg = format!("--sysroot={}", sysroot.display());
    let libc_search = format!("-B{}/usr/lib/x86_64-linux-gnu/", sysroot.display());
    let gcc_search = format!(
        "-B{}/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0/",
        sysroot.display()
    );
    let libc_link = format!("-L{}/usr/lib/x86_64-linux-gnu", sysroot.display());
    let gcc_link = format!(
        "-L{}/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0",
        sysroot.display()
    );
    if let Err(error) = run_cmd(
        repo_root,
        path_str(&compiler)?,
        &[
            &sysroot_arg,
            &libc_search,
            &gcc_search,
            &libc_link,
            &gcc_link,
            "-std=c11",
            "-Os",
            "-static",
            "-s",
            "-fno-ident",
            "-Wl,--build-id=none",
            "-Wall",
            "-Wextra",
            "-Werror",
            path_str(&source)?,
            "-o",
            path_str(&init)?,
        ],
    ) {
        let _ = remove_path_if_exists(&tree);
        return Err(error);
    }
    set_mode(init.clone(), 0o755)?;
    let (module_release, module_count, firmware_count) =
        stage_boot_module_closure(repo_root, &tree)?;
    validate_initramfs_archive_owner(INITRAMFS_ARCHIVE_OWNER)?;
    let archive_command = format!(
        "find . -exec touch -h -d @{MATTOS_SOURCE_DATE_EPOCH} {{}} + && find . -print0 | sort -z | cpio --null -o --quiet --reproducible --owner={INITRAMFS_ARCHIVE_OWNER} --format=newc | xz -1 -T1 --check=crc32 --stdout > {}",
        shell_escape(path_str(&temp)?)
    );

    if let Err(error) = run_cmd(&tree, "bash", &["-lc", &archive_command]) {
        let _ = remove_path_if_exists(&temp);
        let _ = remove_path_if_exists(&tree);
        return Err(error);
    }
    if let Err(error) = validate_early_initramfs(&temp) {
        let _ = remove_path_if_exists(&temp);
        let _ = remove_path_if_exists(&tree);
        return Err(error);
    }
    let reports = repo_root.join("out/reports");
    fs::create_dir_all(&reports)?;
    fs::write(
        reports.join("early-initramfs-inventory.tsv"),
        format!(
            "path\trole\tuncompressed_bytes\n/init\tstatic-live-bootstrap\t{}\n/usr/lib/modules/{module_release}\tboot-critical-module-closure({module_count})\t0\n/usr/lib/firmware\tboot-critical-firmware-only({firmware_count})\t0\narchive\txz-newc\t{}\n",
            fs::metadata(&init)?.len(),
            fs::metadata(&temp)?.len()
        ),
    )?;
    remove_path_if_exists(&tree)?;
    performance::atomic_replace_path(&temp, &destination)
}

fn validate_initramfs_archive_owner(owner: &str) -> Result<()> {
    if owner != "0:0" {
        bail!("unsafe initramfs archive owner {owner}; expected root ownership 0:0")
    }
    Ok(())
}

fn build_iso(repo_root: &Path) -> Result<()> {
    let spec = build_stage_spec(BuildStage::Iso);
    performance::execute_cached_stage(
        repo_root,
        &spec,
        || validate_cached_iso(repo_root),
        || build_iso_atomic(repo_root),
    )
}

fn validate_cached_iso(repo_root: &Path) -> Result<()> {
    let iso = repo_root.join("out/images/mattos-x86_64.iso");
    if fs::metadata(&iso)?.len() < 1024 * 1024 {
        bail!("cached ISO is unexpectedly small");
    }
    validate_staged_grub_config(&repo_root.join("out/build/iso/boot/grub/grub.cfg"))?;
    validate_early_initramfs(&repo_root.join("out/build/iso/boot/early-initramfs.cpio.xz"))?;
    validate_squashfs_image(&repo_root.join("out/build/iso/live/rootfs.squashfs"))?;
    let report = repo_root.join("out/reports/live-image-inventory.tsv");
    if !report.is_file() {
        bail!("live-image inventory is missing: {}", report.display());
    }
    Ok(())
}

fn write_live_image_inventory(
    repo_root: &Path,
    image_path: &Path,
    report_path: &Path,
) -> Result<()> {
    let rootfs = repo_root.join("out/build/rootfs");
    let initramfs = repo_root.join(INITRAMFS_ARCHIVE_PATH);
    let live_root = repo_root.join(LIVE_ROOT_IMAGE_PATH);
    let expanded = Command::new("xz")
        .args(["-dc"])
        .arg(&initramfs)
        .output()
        .context("failed to measure uncompressed early initramfs")?;
    if !expanded.status.success() {
        bail!("xz rejected the early initramfs while producing its size report");
    }

    let mut lines = vec!["record\tpath\tbytes\tdetail".to_string()];
    lines.push(format!(
        "artifact\t{}\t{}\tuncompressed-newc",
        INITRAMFS_ARCHIVE_PATH,
        expanded.stdout.len()
    ));
    lines.push(format!(
        "artifact\t{}\t{}\txz-newc",
        INITRAMFS_ARCHIVE_PATH,
        fs::metadata(&initramfs)?.len()
    ));
    lines.push(format!(
        "artifact\t{}\t{}\tsquashfs-xz",
        LIVE_ROOT_IMAGE_PATH,
        fs::metadata(&live_root)?.len()
    ));
    lines.push(format!(
        "artifact\tout/images/mattos-x86_64.iso\t{}\tiso9660",
        fs::metadata(image_path)?.len()
    ));
    lines.push("duplication\tordinary-root-payload-in-early-initramfs\t0\tbytes".into());

    let mut top_level = fs::read_dir(&rootfs)?.collect::<std::io::Result<Vec<_>>>()?;
    top_level.sort_by_key(|entry| entry.file_name());
    for entry in top_level {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let bytes = if metadata.is_dir() {
            regular_file_bytes(&path)?.1
        } else if metadata.is_file() {
            metadata.len()
        } else {
            continue;
        };
        lines.push(format!(
            "top-level\t{}\t{}\tlogical-regular-file-bytes",
            entry.file_name().to_string_lossy(),
            bytes
        ));
    }
    for (bytes, path) in largest_regular_files(&rootfs, 25)? {
        lines.push(format!("largest-file\t{path}\t{bytes}\tlogical-bytes"));
    }
    fs::write(report_path, lines.join("\n") + "\n")
        .with_context(|| format!("failed to write {}", report_path.display()))
}

fn build_iso_atomic(repo_root: &Path) -> Result<()> {
    let grub_src = validate_grub_config_source(repo_root)?;

    let kernel = repo_root.join("out/build/linux/build/arch/x86/boot/bzImage");
    if !kernel.exists() {
        bail!(
            "kernel image missing at {}; build kernel first",
            kernel.display()
        );
    }

    let initramfs = repo_root.join(INITRAMFS_ARCHIVE_PATH);
    if !initramfs.exists() {
        bail!(
            "initramfs missing at {}; run build initramfs",
            initramfs.display()
        );
    }
    let live_root = repo_root.join(LIVE_ROOT_IMAGE_PATH);
    if !live_root.exists() {
        bail!(
            "live root missing at {}; run build live-root",
            live_root.display()
        );
    }

    let iso_destination = repo_root.join("out/build/iso");
    let iso_root = performance::temporary_sibling(&iso_destination, "building")?;
    let grub_dir = iso_root.join("boot/grub");
    fs::create_dir_all(&grub_dir).context("failed to create ISO directory layout")?;

    fs::copy(&kernel, iso_root.join("boot/vmlinuz"))
        .context("failed to stage kernel into ISO tree")?;
    fs::copy(&initramfs, iso_root.join("boot/early-initramfs.cpio.xz"))
        .context("failed to stage initramfs into ISO tree")?;
    fs::create_dir_all(iso_root.join("live"))?;
    fs::copy(&live_root, iso_root.join("live/rootfs.squashfs"))
        .context("failed to stage live root into ISO tree")?;
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
    let image_destination = out_images.join("mattos-x86_64.iso");
    let image_temp = performance::temporary_sibling(&image_destination, "building")?;
    let build_tmp = repo_root.join("out/tmp");
    fs::create_dir_all(&build_tmp)?;
    let result = run_cmd_with_env_overrides(
        repo_root,
        "grub-mkrescue",
        &[
            "-o",
            path_str(&image_temp)?,
            path_str(&iso_root)?,
            "--modification-date=2026010100000000",
            "--set_all_file_dates",
            "2026010100000000",
        ],
        &[
            ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
            ("TMPDIR", build_tmp.display().to_string()),
        ],
    );
    if let Err(error) = result {
        let _ = remove_path_if_exists(&iso_root);
        let _ = remove_path_if_exists(&image_temp);
        return Err(error);
    }
    if fs::metadata(&image_temp)?.len() < 1024 * 1024 {
        bail!("generated ISO is unexpectedly small");
    }
    validate_dual_firmware_iso(repo_root, &image_temp)?;
    let reports = repo_root.join("out/reports");
    fs::create_dir_all(&reports)?;
    let report_destination = reports.join("live-image-inventory.tsv");
    let report_temp = performance::temporary_sibling(&report_destination, "building")?;
    write_live_image_inventory(repo_root, &image_temp, &report_temp)?;
    performance::atomic_replace_path(&iso_root, &iso_destination)?;
    performance::atomic_replace_path(&image_temp, &image_destination)?;
    performance::atomic_replace_path(&report_temp, &report_destination)
}

fn validate_dual_firmware_iso(repo_root: &Path, image: &Path) -> Result<()> {
    let report = run_cmd_capture(
        repo_root,
        "xorriso",
        &[
            "-indev",
            path_str(image)?,
            "-report_el_torito",
            "as_mkisofs",
        ],
    )?;
    if !report.contains("-b '") && !report.contains("-b ") {
        bail!("ISO has no El Torito legacy BIOS boot image");
    }
    if !report.contains("-e '") && !report.contains("-e ") {
        bail!("ISO has no El Torito UEFI boot image");
    }
    Ok(())
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
        "menuentry \"Start MattOS Live (CLI)\"",
        "menuentry \"Install MattOS\"",
        "menuentry \"Install MattOS (CLI)\"",
        GRUB_RESCUE_ENTRY,
        GRUB_EARLY_RDINIT,
        GRUB_RESCUE_MARKER,
    ] {
        if !content.contains(needle) {
            bail!(
                "staged GRUB config {} is missing required marker: {}",
                path.display(),
                needle
            );
        }
    }

    if content
        .matches("initrd /boot/early-initramfs.cpio.xz")
        .count()
        != 5
    {
        bail!("staged GRUB config must load the early initramfs for all five entries");
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
            "-drive",
            &format!(
                "file={},if=none,id=mattos-cd,media=cdrom,readonly=on",
                iso.to_str().ok_or_else(|| anyhow!("invalid ISO path"))?
            ),
            "-device",
            "virtio-scsi-pci,id=mattos-scsi",
            "-device",
            "scsi-cd,drive=mattos-cd,bus=mattos-scsi.0,bootindex=1",
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
    let scheduler_args = scheduler_command_args(args);
    command.args(&scheduler_args).current_dir(cwd);
    apply_reproducible_process_environment(&mut command);
    apply_mattos_tmp_environment(&mut command, cwd)?;
    apply_scheduler_parallelism(&mut command);
    apply_mattos_sysroot_environment(&mut command, cwd, program, &[])?;
    let display = effective_command_display(program, &scheduler_args);
    performance::run_logged_command(&mut command, &display)
}

fn run_cmd_with_env(
    cwd: &Path,
    program: &str,
    args: &[&str],
    tool_env: Option<&LocalToolEnv>,
) -> Result<()> {
    let mut cmd = Command::new(program);
    let scheduler_args = scheduler_command_args(args);
    cmd.args(&scheduler_args).current_dir(cwd);
    apply_reproducible_process_environment(&mut cmd);
    apply_mattos_tmp_environment(&mut cmd, cwd)?;
    apply_scheduler_parallelism(&mut cmd);

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

    let display = effective_command_display(program, &scheduler_args);
    let status = performance::run_logged_command(&mut cmd, &display)?;
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
    let mut cmd = Command::new(program);
    let scheduler_args = scheduler_command_args(args);
    cmd.args(&scheduler_args).current_dir(cwd);
    apply_reproducible_process_environment(&mut cmd);
    for (key, value) in env_overrides {
        cmd.env(key, value);
    }
    apply_mattos_tmp_environment(&mut cmd, cwd)?;
    apply_scheduler_parallelism(&mut cmd);
    apply_mattos_sysroot_environment(&mut cmd, cwd, program, env_overrides)?;

    let display = effective_command_display(program, &scheduler_args);
    let status = performance::run_logged_command(&mut cmd, &display)?;
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

fn apply_reproducible_process_environment(command: &mut Command) {
    command
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH);
}

fn mattos_build_tmp(repo_root: &Path) -> PathBuf {
    repo_root.join(MATTOS_BUILD_TMP_RELATIVE)
}

fn mattos_tmp_min_free_bytes() -> u64 {
    // Unit tests exercise routing, writability, and concurrency inside
    // tempfile-backed filesystems. Their result must not depend on how full
    // the host's /tmp happens to be. Production builds retain the 4 GiB guard.
    if cfg!(test) {
        0
    } else {
        MIN_MATTOS_TMP_FREE_BYTES
    }
}

fn ensure_mattos_build_tmp(repo_root: &Path) -> Result<PathBuf> {
    let directory = mattos_build_tmp(repo_root);
    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "failed to create MattOS build temp directory {}",
            directory.display()
        )
    })?;
    let free_bytes = free_bytes_at(&directory)?;
    let required_free_bytes = mattos_tmp_min_free_bytes();
    if free_bytes < required_free_bytes {
        bail!(
            "MattOS build temp directory {} has only {} free bytes; at least {} are required",
            directory.display(),
            free_bytes,
            required_free_bytes
        );
    }

    // `build all` prepares commands from multiple scheduler threads inside one
    // mattos-build process. A PID-only probe name lets those threads delete one
    // another's probe. Give every invocation a process-local unique sequence so
    // strict cleanup remains meaningful without serializing command setup.
    let sequence = MATTOS_TMP_PROBE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let probe = directory.join(format!(".write-probe-{}-{sequence}", std::process::id()));
    fs::write(&probe, b"mattos-build temp directory probe\n").with_context(|| {
        format!(
            "MattOS build temp directory is not writable: {}",
            directory.display()
        )
    })?;
    fs::remove_file(&probe).with_context(|| {
        format!(
            "failed to remove MattOS build temp probe {}",
            probe.display()
        )
    })?;
    Ok(directory)
}

fn free_bytes_at(path: &Path) -> Result<u64> {
    let path = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .context("invalid MattOS temp path")?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return Err(anyhow!(
            "failed to inspect free space for MattOS temp directory"
        ));
    }
    let stats = unsafe { stats.assume_init() };
    Ok((stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64))
}

fn apply_mattos_tmp_environment(command: &mut Command, cwd: &Path) -> Result<()> {
    let Some(repo_root) = cwd.ancestors().find(|candidate| {
        candidate
            .join("src/tools/mattos-build/Cargo.toml")
            .is_file()
    }) else {
        return Ok(());
    };
    let directory = ensure_mattos_build_tmp(repo_root)?;
    // The repository-owned directory deliberately takes precedence over a
    // caller's TMPDIR: build correctness must not depend on a full host /tmp.
    command.env("TMPDIR", directory);
    Ok(())
}

fn effective_command_display(program: &str, args: &[String]) -> String {
    let argv = std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    format!(
        "{}\n[mattos-command] child_jobs={} argv={argv:?}",
        argv.join(" "),
        scheduler::child_job_limit()
    )
}

fn scheduler_command_args(args: &[&str]) -> Vec<String> {
    // A very small cgroup memory ceiling can yield no parallel CPU grant.
    // External build tools require a positive jobs value; retain serial
    // progress while the cgroup remains the hard memory safety boundary.
    let limit = scheduler::child_job_limit().max(1);
    let experimental_limit = EXPERIMENTAL_CHILD_JOBS.with(Cell::get);
    let mut previous_sets_jobs = false;
    args.iter()
        .map(|argument| {
            let normalized =
                if previous_sets_jobs && argument.bytes().all(|byte| byte.is_ascii_digit()) {
                    experimental_limit
                        .unwrap_or_else(|| argument.parse::<usize>().unwrap().min(limit))
                        .to_string()
                } else if argument.starts_with("-j")
                    && argument.len() > 2
                    && argument[2..].bytes().all(|byte| byte.is_ascii_digit())
                {
                    format!(
                        "-j{}",
                        experimental_limit
                            .unwrap_or_else(|| argument[2..].parse::<usize>().unwrap().min(limit))
                    )
                } else if let Some(value) = argument
                    .strip_prefix("--jobs=")
                    .or_else(|| argument.strip_prefix("--parallel="))
                    .filter(|value| value.bytes().all(|byte| byte.is_ascii_digit()))
                {
                    let option = argument.split_once('=').unwrap().0;
                    format!(
                        "{option}={}",
                        experimental_limit
                            .unwrap_or_else(|| value.parse::<usize>().unwrap().min(limit))
                    )
                } else {
                    (*argument).to_string()
                };
            previous_sets_jobs = matches!(*argument, "-j" | "--jobs" | "--parallel");
            normalized
        })
        .collect()
}

fn apply_scheduler_parallelism(command: &mut Command) {
    // External build tools uniformly reject a zero job count.  A tight
    // memory admission budget may intentionally grant no parallel token, but
    // it must still permit one serial child inside the cgroup ceiling.
    let tokens = scheduler::child_job_limit().max(1).to_string();
    command
        .env("MAKEFLAGS", format!("-j{tokens}"))
        .env("CARGO_BUILD_JOBS", &tokens)
        .env("CMAKE_BUILD_PARALLEL_LEVEL", &tokens)
        .env("MESON_NUM_PROCESSES", &tokens)
        .env("NINJAFLAGS", format!("-j{tokens}"));
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
        || cwd.starts_with(repo_root.join("out/build/linux"))
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
        // Cargo fingerprints RUSTFLAGS verbatim and rustc incorporates codegen
        // options into crate identity.  Keep the linker argument independent of
        // the absolute checkout location while still resolving to this tree's
        // output-owned sysroot from Cargo's working directory.
        let relative = cwd
            .strip_prefix(repo_root)
            .context("Cargo working directory is outside the MattOS repository")?;
        let mut relative_sysroot = PathBuf::new();
        for component in relative.components() {
            if matches!(component, std::path::Component::Normal(_)) {
                relative_sysroot.push("..");
            }
        }
        relative_sysroot.push("out/sysroot");
        let rust_sysroot = format!(
            "-C link-arg=--sysroot={}",
            relative_sysroot.to_string_lossy()
        );
        let remap = format!(
            "--remap-path-prefix={}=/usr/src/mattos",
            repo_root.display()
        );
        let value = if current.contains(&rust_sysroot) {
            current
        } else if current.is_empty() {
            rust_sysroot
        } else {
            format!("{current} {rust_sysroot}")
        };
        let value = if value.contains(&remap) {
            value
        } else {
            format!("{value} {remap}")
        };
        command.env("RUSTFLAGS", value);
    }
    command.env("MATTOS_SYSROOT", &sysroot);
    Ok(())
}

fn run_cmd_output(cwd: &Path, program: &str, args: &[&str]) -> Result<Output> {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    apply_reproducible_process_environment(&mut command);
    apply_mattos_tmp_environment(&mut command, cwd)?;
    command
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
        let source = include_str!("main.rs");
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
            ("fuse3", 20.000),
            ("findutils", 57.703),
            ("gcc-compiler", 647.434),
            ("gcc-runtime", 773.452),
            ("glibc", 453.080),
            ("grep", 24.148),
            ("git", 90.000),
            ("glib", 180.000),
            ("gpgv", 20.000),
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
        let source = include_str!("main.rs");
        let start = source.find("fn build_mesa").unwrap();
        let end = source[start..].find("fn build_cosmic_comp").unwrap() + start;
        let recipe = &source[start..end];
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
        let source = include_str!("main.rs");
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
        assert!(include_str!("main.rs").contains("if !missing_required.contains(&\"pkg-config\")"));
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
        let end = source[start..].find("const GLIBC_MINIMUM_KERNEL").unwrap() + start;
        let build = &source[start..end];
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
                PathBuf::from("src/system/data/linux-firmware")
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
        let source = include_str!("main.rs");
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
            &root.join("src/tools/mattos-build/src/main.rs"),
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
        let source = include_str!("main.rs");
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
        let source = include_str!("main.rs");
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
        let source = include_str!("main.rs");
        let start = source.find("fn build_release_autotools_program").unwrap();
        let end = source[start..].find("fn build_gzip").unwrap() + start;
        let helper = &source[start..end];
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
