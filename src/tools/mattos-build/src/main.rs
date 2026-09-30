/// The release of the kernel MattOS boots: the vendored Linux Makefile's
/// `VERSION.PATCHLEVEL.SUBLEVEL` and `EXTRAVERSION` plus the configured
/// `CONFIG_LOCALVERSION`.  Debian-style kernel packages embed it in their
/// names (`linux-modules-<release>`), so every such name and path is derived
/// from this one macro; `kernel_release_matches_the_vendored_kernel_and_data`
/// checks it against the kernel source and the package metadata files.
macro_rules! mattos_kernel_release {
    () => {
        "7.2.8-mattos"
    };
}

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
mod jobserver;
mod packaging;
mod performance;
mod recipe_projection;
mod resources;
mod rust_items;
mod scheduler;
mod source_identity;
mod stage_cache;
mod stage_graph;
mod stage_memory;
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
const MATTOS_KERNEL_RELEASE: &str = mattos_kernel_release!();
const LINUX_MODULES_PACKAGE: &str = concat!("linux-modules-", mattos_kernel_release!());
const NVIDIA_OPEN_MODULES_PACKAGE: &str =
    concat!("linux-modules-nvidia-595-open-", mattos_kernel_release!());
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
const SED_PROVIDER: &str = "sed";
const DASH_PROVIDER: &str = "dash";
const MAWK_PROVIDER: &str = "mawk";
const RSYNC_PROVIDER: &str = "rsync";
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
// GNU sed, dash and rsync keep their generated configure scripts out of Git;
// each official release archive supplies them at the exact imported tag.
const SED_RELEASE_ARCHIVE_URL: &str = "https://ftp.gnu.org/gnu/sed/sed-4.10.tar.xz";
const SED_RELEASE_ARCHIVE_SHA256: &str =
    "b8e72182b2ec96a3574e2998c47b7aaa64cc20ce000d8e9ac313cc07cecf28c7";
const DASH_RELEASE_ARCHIVE_URL: &str =
    "http://gondor.apana.org.au/~herbert/dash/files/dash-0.5.13.5.tar.gz";
const DASH_RELEASE_ARCHIVE_SHA256: &str =
    "40090101a2a491f13e901d3d48e90414f26634628b9bfff35ff540363c227a7d";
const RSYNC_RELEASE_ARCHIVE_URL: &str = "https://download.samba.org/pub/rsync/src/rsync-3.5.1.tar.gz";
const RSYNC_RELEASE_ARCHIVE_SHA256: &str =
    "c55f9c9dc10fb8bec397b399a0fdded53cc9a2d8e30891bb0d63724d25c37bef";
const M4_RELEASE_ARCHIVE_URL: &str = "https://ftp.gnu.org/gnu/m4/m4-1.4.21.tar.xz";
const M4_RELEASE_ARCHIVE_SHA256: &str =
    "f25c6ab51548a73a75558742fb031e0625d6485fe5f9155949d6486a2408ab66";
const AUTOCONF_RELEASE_ARCHIVE_URL: &str = "https://ftp.gnu.org/gnu/autoconf/autoconf-2.73.tar.xz";
const AUTOCONF_RELEASE_ARCHIVE_SHA256: &str =
    "9fd672b1c8425fac2fa67fa0477b990987268b90ff36d5f016dae57be0d6b52e";
const AUTOMAKE_RELEASE_ARCHIVE_URL: &str = "https://ftp.gnu.org/gnu/automake/automake-1.19.tar.xz";
const AUTOMAKE_RELEASE_ARCHIVE_SHA256: &str =
    "e3e2c2e3abf37898138db5b6c1d1dc35c9160c5978be7947d2c741705251d445";
const LIBTOOL_RELEASE_ARCHIVE_URL: &str = "https://ftp.gnu.org/gnu/libtool/libtool-2.6.2.tar.xz";
const LIBTOOL_RELEASE_ARCHIVE_SHA256: &str =
    "2ef1067c16c97db930fd740cc9bc3d3ba9a583804ae5ac42cc3e8719e49e191e";
const RUST_RELEASE_ARCHIVE_URL: &str = "https://static.rust-lang.org/dist/rustc-1.97.1-src.tar.xz";
const RUST_RELEASE_ARCHIVE_SHA256: &str =
    "0ed06fdaffd4722a7702e0b4eebfafc897ab8f513e8e1b247cdd7e5c6df6ded2";
const MATTOS_GCC_INSTALL_DIR: &str = "/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0";
const LESS_RELEASE_ARCHIVE_URL: &str = "https://www.greenwoodsoftware.com/less/less-704.tar.gz";
const LESS_RELEASE_ARCHIVE_SHA256: &str =
    "20a0b0a2bb2525fa53c7eee9beb854b4c9cf172eabb209af7020743547bfe9fb";
const LIBSNDFILE_RELEASE_ARCHIVE_URL: &str =
    "https://github.com/libsndfile/libsndfile/releases/download/1.2.2/libsndfile-1.2.2.tar.xz";
const LIBSNDFILE_RELEASE_ARCHIVE_SHA256: &str =
    "3799ca9924d3125038880367bf1468e53a1b7e3686a934f098b7e1d286cdb80e";
const LIBFYAML_RELEASE_ARCHIVE_URL: &str =
    "https://github.com/pantoniou/libfyaml/releases/download/v0.9.6/libfyaml-0.9.6.tar.gz";
const LIBFYAML_RELEASE_ARCHIVE_SHA256: &str =
    "a59cc3331e2eb903ec36933ad52a45888041cac31e44f553a00511131242c483";
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
    "pam_limits.so",
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
const SED_BINARIES: &[ComponentBinarySpec] = &[component_binary("usr/bin/sed", "sed")];
const DASH_BINARIES: &[ComponentBinarySpec] = &[component_binary("usr/bin/dash", "dash")];
const MAWK_BINARIES: &[ComponentBinarySpec] = &[component_binary("usr/bin/mawk", "mawk")];
const RSYNC_BINARIES: &[ComponentBinarySpec] = &[component_binary("usr/bin/rsync", "rsync")];
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
        provider: SED_PROVIDER,
        install_root_rel: "out/build/sed/install",
        binaries: SED_BINARIES,
    },
    ComponentInstallManifest {
        provider: DASH_PROVIDER,
        install_root_rel: "out/build/dash/install",
        binaries: DASH_BINARIES,
    },
    ComponentInstallManifest {
        provider: MAWK_PROVIDER,
        install_root_rel: "out/build/mawk/install",
        binaries: MAWK_BINARIES,
    },
    ComponentInstallManifest {
        provider: RSYNC_PROVIDER,
        install_root_rel: "out/build/rsync/install",
        binaries: RSYNC_BINARIES,
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
        /// Keep building every stage that does not depend on a failed stage,
        /// then report all failures (only with `all`).
        #[arg(long)]
        keep_going: bool,
    },
    Image,
    /// Build (or reuse) the MattOS builder container image for third-party
    /// package recipes: out/images/mattos-builder.oci.tar.
    BuilderImage,
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

/// A standalone intentional-omission policy: a component imports either the
/// listed upstream paths (`retained_paths`) or one upstream subtree whose
/// prefix is stripped (`upstream_subtree`).  The provenance audit
/// (`DevUtils/audits/test_vendored_source_provenance.py`) applies the same
/// rules.
#[derive(Debug, Deserialize, Clone)]
struct OmissionPolicy {
    schema_version: u32,
    component: String,
    upstream_commit: String,
    reason: String,
    #[serde(default)]
    upstream_subtree: Option<String>,
    #[serde(default)]
    retained_paths: Option<Vec<String>>,
    #[serde(default)]
    expected_runtime_files: Option<Vec<String>>,
}

impl OmissionPolicy {
    /// The imported path of `upstream_path`, or `None` when it is omitted.
    fn map(&self, upstream_path: &str) -> Option<String> {
        if let Some(subtree) = &self.upstream_subtree {
            let prefix = format!("{}/", subtree.trim_end_matches('/'));
            return upstream_path
                .strip_prefix(&prefix)
                .filter(|relative| !relative.is_empty())
                .map(str::to_string);
        }
        self.retained_paths
            .iter()
            .flatten()
            .any(|selector| {
                let selector = selector.trim_end_matches('/');
                upstream_path == selector || upstream_path.starts_with(&format!("{selector}/"))
            })
            .then(|| upstream_path.to_string())
    }
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
    /// Committer time of `imported_commit`, which orders snapshot package
    /// versions (`component_snapshot_version`).  Absent in state written
    /// before it was recorded; a re-sync at the same pin records it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    upstream_committed_at_utc: Option<String>,
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

/// The file-creation mask every build step runs under.  Stage outputs
/// record file and directory modes, so without a fixed mask they depended on
/// the invoking shell: a caller with umask 002 produced group-writable
/// directories, changing gcc-runtime's output digest and rebuilding the
/// whole system from byte-identical files.
const BUILD_UMASK: libc::mode_t = 0o022;

/// Sets `BUILD_UMASK` for this process and every command it runs.
fn apply_build_umask() {
    // SAFETY: umask only sets this process's file-creation mask; child
    // processes (compilers, make, install) inherit it.
    unsafe { libc::umask(BUILD_UMASK) };
}

fn main() -> Result<()> {
    apply_build_umask();
    let cli = Cli::parse();
    let repo_root = std::env::current_dir().context("unable to determine current directory")?;
    ensure_private_cache_root(&repo_root)?;
    prepare_source_ownership_tool_environment(&repo_root)?;
    ensure_mattos_build_tmp(&repo_root)?;
    if let Commands::Build {
        stage,
        experimental_child_jobs,
        keep_going,
    } = &cli.command
    {
        if *keep_going && stage.is_some_and(|stage| stage != BuildStage::All) {
            bail!("--keep-going applies to scheduled `build all` runs; a single stage has nothing to continue with");
        }
        validate_experimental_child_jobs(
            stage.unwrap_or(BuildStage::All),
            *experimental_child_jobs,
        )?;
    }
    let timing_command = match &cli.command {
        Commands::Build {
            stage,
            experimental_child_jobs,
            ..
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
            keep_going,
        } => build(
            &repo_root,
            stage.unwrap_or(BuildStage::All),
            experimental_child_jobs,
            if keep_going {
                scheduler::FailurePolicy::KeepGoing
            } else {
                scheduler::FailurePolicy::FailFast
            },
        ),
        Commands::Image => build_image(&repo_root),
        Commands::BuilderImage => packaging::build_builder_image(&repo_root),
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
    if let Some(report) = stage_cache::take_tool_drift_report() {
        eprint!("{report}");
    }
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
    failure_policy: scheduler::FailurePolicy,
) -> Result<()> {
    if stage == BuildStage::All {
        return build_all_scheduled(repo_root, failure_policy);
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
        BuildStage::CrossToolchain
            | BuildStage::Glibc
            | BuildStage::GccRuntime
            | BuildStage::Binutils
            | BuildStage::GccToolchain
            | BuildStage::Make
            | BuildStage::Apt
    ) {
        bail!(
            "--experimental-child-jobs is restricted to isolated cross-toolchain, glibc, gcc-runtime, binutils, gcc-toolchain, make, or apt builds"
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

fn build_all_scheduled(repo_root: &Path, failure_policy: scheduler::FailurePolicy) -> Result<()> {
    prune_derived_source_mirror_artifacts(repo_root)?;
    let stages = build_plan(BuildStage::All);
    let nodes = scheduled_build_nodes(&stages);
    let snapshot = resources::discover();
    let budget = snapshot.budget();
    scheduler::execute_with_policy(nodes, budget, failure_policy, |id, context| {
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
                        // Packages and the repository are produced inside
                        // the rootfs stage's scheduled action.
                        "packages" | "repository" => "rootfs",
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
include!("stages/build_tools.rs");
include!("stages/helpers/native.rs");
include!("stages/helpers/pkgconfig.rs");
include!("stages/helpers/autotools.rs");
include!("stages/helpers/meson.rs");
include!("stages/graphics.rs");
include!("stages/qt.rs");
include!("stages/kde_foundation.rs");
include!("stages/calamares.rs");
include!("stages/plasma.rs");
include!("stages/plasma_apps.rs");
include!("stages/desktop.rs");
include!("stages/flatpak.rs");
include!("stages/system_services.rs");
include!("stages/wifi.rs");
include!("stages/grub.rs");
include!("stages/foundation_libraries.rs");
include!("stages/networking.rs");
include!("stages/system_runtime.rs");
include!("stages/runtime_libraries.rs");
include!("stages/desktop_support.rs");
include!("stages/archive_tools.rs");

fn stage_output_file(source: &Path, destination: &Path, mode: u32) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)
        .with_context(|| format!("failed to stage {}", source.display()))?;
    set_mode(destination.to_path_buf(), mode)
}

include!("stages/helpers/cargo.rs");
include!("stages/runtime_tooling.rs");
include!("stages/llvm_toolchain.rs");
include!("stages/rust_toolchain.rs");
include!("stages/libraries.rs");

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

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("invalid path {}", path.display()))
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

include!("stages/image.rs");
include!("stages/helpers/command.rs");
#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
