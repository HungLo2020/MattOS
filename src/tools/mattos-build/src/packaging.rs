use super::*;
use clap::Subcommand;
use filetime::{set_file_times, set_symlink_file_times, FileTime};
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
const PACKAGE_NAMES: &[&str] = &[
    "mattos-filesystem",
    "mattos-bootstrap-runtime",
    "mattos-base-files",
    "mattos-ca-certificates",
    "mattos-brush",
    "mattos-coreutils",
    "mattos-curl",
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
            name: "mattos-bootstrap-runtime",
            description: "Temporary host-derived runtime closure for MattOS bootstrap packages",
            source_component: "bootstrap-runtime",
            depends: &["mattos-filesystem"],
            provides: &["mattos-runtime-abi"],
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
            description: "Brush shell built for MattOS",
            source_component: "brush",
            depends: &["mattos-filesystem", "mattos-bootstrap-runtime"],
            provides: &["mattos-shell"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "required",
        },
        PackageSpec {
            name: "mattos-coreutils",
            description: "uutils core utilities built for MattOS",
            source_component: "coreutils",
            depends: &["mattos-filesystem", "mattos-bootstrap-runtime"],
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
                "mattos-bootstrap-runtime",
                "mattos-ca-certificates",
            ],
            provides: &["curl"],
            conflicts: &["curl"],
            replaces: &["curl"],
            essential: false,
            priority: "optional",
        },
        PackageSpec {
            name: "mattos-dpkg",
            description: "dpkg binary package management runtime built for MattOS",
            source_component: "dpkg",
            depends: &["mattos-filesystem", "mattos-bootstrap-runtime"],
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
            depends: &["mattos-bootstrap-runtime", "mattos-libudev1"],
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
                "mattos-bootstrap-runtime",
                "mattos-ca-certificates",
                "mattos-dpkg",
                "mattos-libapt-pkg",
                "mattos-libudev1",
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
            depends: &["mattos-bootstrap-runtime"],
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
            depends: &["mattos-bootstrap-runtime", "mattos-libtinfow6"],
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
            depends: &[
                "mattos-bootstrap-runtime",
                "mattos-libtinfow6",
                "mattos-terminfo",
            ],
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
            depends: &["mattos-bootstrap-runtime"],
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
            depends: &["mattos-bootstrap-runtime", "mattos-libkmod2"],
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
            depends: &["mattos-bootstrap-runtime"],
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
                "mattos-bootstrap-runtime",
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
            depends: &["mattos-bootstrap-runtime"],
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
            depends: &["mattos-bootstrap-runtime"],
            provides: &["libudev1"],
            conflicts: &[],
            replaces: &[],
            essential: false,
            priority: "important",
        },
        PackageSpec {
            name: "mattos-dbus-broker",
            description: "D-Bus message broker and MattOS bus policy",
            source_component: "dbus-broker",
            depends: &["mattos-bootstrap-runtime", "mattos-libsystemd0"],
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
            depends: &["mattos-bootstrap-runtime"],
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
            depends: &["mattos-bootstrap-runtime", "mattos-libpam0"],
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
            depends: &["mattos-bootstrap-runtime", "mattos-libpam0"],
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
            depends: &[
                "mattos-bootstrap-runtime",
                "mattos-libpam0",
                "mattos-pam-modules",
            ],
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
                "mattos-bootstrap-runtime",
                "mattos-libpam0",
                "mattos-libpam-misc0",
                "mattos-pam-runtime",
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
            depends: &[
                "mattos-bootstrap-runtime",
                "mattos-libpam0",
                "mattos-pam-runtime",
            ],
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
                "mattos-bootstrap-runtime",
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
            depends: &["mattos-bootstrap-runtime"],
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
            depends: &["mattos-bootstrap-runtime"],
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
    let names: BTreeSet<&str> = specs.iter().map(|spec| spec.name).collect();
    let mut remaining: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for spec in &specs {
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
        let next = PACKAGE_NAMES.iter().copied().find(|name| {
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
        PackageCommands::Status => print_inventory(repo_root),
    }
}

pub(crate) fn build_all_packages(repo_root: &Path) -> Result<()> {
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
        "mattos-bootstrap-runtime" => stage_bootstrap_runtime(repo_root, &staging)?,
        "mattos-base-files" => stage_base_files(repo_root, &staging)?,
        "mattos-ca-certificates" => stage_ca_certificates(repo_root, &staging)?,
        "mattos-brush" => {
            let source = repo_root.join("src/userland/brush/target/release/brush");
            stage_executable(&source, &staging.join("usr/bin/brush"), 0o755)?;
        }
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

    let version = package_version(repo_root, spec)?;
    validate_debian_version(&version)?;
    let runtime_libraries = runtime_libraries_for_spec(repo_root, spec)?;
    write_provenance(repo_root, &staging, spec, &version, &runtime_libraries)?;
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

fn stage_bootstrap_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let apt_install = repo_root.join("out/build/apt/install");
    let curl_install = repo_root.join("out/build/curl/install");
    let systemd_install = repo_root.join("out/build/systemd/install");
    let library_dirs = [
        apt_install.join("usr/lib/x86_64-linux-gnu"),
        curl_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "ncurses").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "kmod").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "procps-ng").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "linux-pam").join("usr/lib/x86_64-linux-gnu"),
    ];
    let library_path = std::env::join_paths(library_dirs.iter())?;
    let mut inputs = vec![
        repo_root.join("src/userland/brush/target/release/brush"),
        resolve_coreutils_multicall(repo_root)?,
        PathBuf::from("/usr/bin/tar"),
        curl_install.join("usr/bin/curl"),
        curl_install.join("usr/lib/x86_64-linux-gnu/libcurl.so.4.8.0"),
    ];
    for rel in DPKG_RUNTIME_PATHS {
        inputs.push(repo_root.join("out/build/dpkg/install").join(rel));
    }
    for rel in [
        "usr/bin/apt",
        "usr/bin/apt-cache",
        "usr/bin/apt-config",
        "usr/bin/apt-get",
        "usr/bin/apt-mark",
        "usr/lib/apt/apt-helper",
        "usr/lib/apt/methods/copy",
        "usr/lib/apt/methods/file",
        "usr/lib/apt/methods/store",
        "usr/lib/x86_64-linux-gnu/libapt-pkg.so.7.0.0",
        "usr/lib/x86_64-linux-gnu/libapt-private.so.0.0.0",
    ] {
        inputs.push(apt_install.join(rel));
    }
    for (component, paths) in [
        ("ncurses", NCURSES_RUNTIME_PATHS),
        ("kmod", KMOD_RUNTIME_PATHS),
        ("procps-ng", PROCPS_RUNTIME_PATHS),
        ("shadow", SHADOW_RUNTIME_PATHS),
        ("util-linux", UTIL_LINUX_AUTH_PATHS),
        ("iproute2", IPROUTE2_RUNTIME_PATHS),
        ("iputils", IPUTILS_RUNTIME_PATHS),
    ] {
        let install = component_install(repo_root, component);
        for rel in paths {
            inputs.push(install.join(rel));
        }
    }
    for rel in ["usr/bin/sudo", "usr/bin/visudo"] {
        inputs.push(component_install(repo_root, "sudo-rs").join(rel));
    }
    for rel in ["usr/bin/dbus-broker", "usr/bin/dbus-broker-launch"] {
        inputs.push(component_install(repo_root, "dbus-broker").join(rel));
    }
    for rel in [
        "usr/sbin/unix_chkpwd",
        "usr/lib/x86_64-linux-gnu/libpam.so.0.85.1",
        "usr/lib/x86_64-linux-gnu/libpam_misc.so.0.82.1",
    ] {
        inputs.push(component_install(repo_root, "linux-pam").join(rel));
    }
    for module in PAM_MODULES {
        inputs.push(
            component_install(repo_root, "linux-pam")
                .join("usr/lib/x86_64-linux-gnu/security")
                .join(module),
        );
    }
    for (component, library) in [
        ("ncurses", "libtinfow.so.6.6"),
        ("ncurses", "libncursesw.so.6.6"),
        ("kmod", "libkmod.so.2.5.1"),
        ("procps-ng", "libproc2.so.1.0.1"),
        ("systemd", "libsystemd.so.0.44.0"),
        ("systemd", "libudev.so.1.7.14"),
    ] {
        inputs.push(
            component_install(repo_root, component)
                .join("usr/lib/x86_64-linux-gnu")
                .join(library),
        );
    }

    let mut dependencies = BTreeSet::new();
    for input in inputs {
        if !input.is_file() {
            bail!("runtime closure input missing at {}", input.display())
        }
        for dependency in ldd_dependency_paths(&input, &library_path)? {
            dependencies.insert(dependency);
        }
    }

    let mut manifest = Vec::new();
    stage_executable(
        Path::new("/usr/bin/tar"),
        &staging.join("usr/bin/tar"),
        0o755,
    )?;
    manifest.push(format!(
        "/usr/bin/tar\t/usr/bin/tar\tbootstrap archive extraction\t{}",
        sha256_file(&staging.join("usr/bin/tar"))?
    ));
    for source in dependencies {
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("invalid runtime library path {}", source.display()))?;
        if [
            "libapt-pkg.so",
            "libapt-private.so",
            "libcurl.so",
            "libtinfow.so",
            "libncursesw.so",
            "libkmod.so",
            "libproc2.so",
            "libsystemd.so",
            "libudev.so",
            "libpam.so",
            "libpam_misc.so",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        let destination_rel = if name.starts_with("ld-linux-") {
            PathBuf::from("usr/lib64").join(name)
        } else {
            PathBuf::from("usr/lib/x86_64-linux-gnu").join(name)
        };
        let destination = staging.join(&destination_rel);
        if destination.is_file() {
            if sha256_file(&destination)? != sha256_file(&source)? {
                bail!(
                    "different runtime libraries resolve to /{}",
                    destination_rel.display()
                )
            }
            continue;
        }
        copy_preserving(&source, &destination)?;
        let source_display = source.strip_prefix(repo_root).unwrap_or(&source).display();
        manifest.push(format!(
            "/{}\t{}\tELF dependency not yet source-packaged\t{}",
            destination_rel.display(),
            source_display,
            sha256_file(&destination)?
        ));
    }
    manifest.sort();
    fs::create_dir_all(staging.join("usr/share/doc/mattos-bootstrap-runtime"))?;
    fs::write(
        staging.join("usr/share/doc/mattos-bootstrap-runtime/runtime-files.tsv"),
        format!(
            "destination\tsource\treason\tsha256\n{}\n",
            manifest.join("\n")
        ),
    )?;
    Ok(())
}

fn ldd_dependency_paths(binary: &Path, library_path: &std::ffi::OsStr) -> Result<Vec<PathBuf>> {
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
    let text = String::from_utf8(output.stdout)?;
    if !output.status.success() || text.contains("not found") {
        bail!(
            "unresolved runtime dependency for {}:\n{text}",
            binary.display()
        )
    }
    let mut paths = BTreeSet::new();
    for token in text
        .split_whitespace()
        .filter(|token| token.starts_with('/'))
    {
        let trimmed = token.trim_end_matches(['(', ':']);
        let path = PathBuf::from(trimmed);
        if path.is_file() {
            paths.insert(path);
        }
    }
    Ok(paths.into_iter().collect())
}

fn package_dependencies(repo_root: &Path, spec: &PackageSpec) -> Result<Vec<String>> {
    let specs = package_specs();
    spec.depends
        .iter()
        .map(|dependency| {
            if dependency.starts_with("mattos-") {
                let target = specs
                    .iter()
                    .find(|candidate| candidate.name == *dependency)
                    .ok_or_else(|| anyhow!("unknown dependency {dependency}"))?;
                Ok(format!(
                    "{dependency} (= {})",
                    package_version(repo_root, target)?
                ))
            } else {
                Ok((*dependency).to_string())
            }
        })
        .collect()
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
        "mattos-filesystem" | "mattos-base-files" | "mattos-bootstrap-runtime" => "0.1".to_string(),
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
        "mattos-dbus-broker" => component_snapshot_version(repo_root, "dbus-broker")?,
        "mattos-libpam0" | "mattos-libpam-misc0" | "mattos-pam-modules" | "mattos-pam-runtime" => {
            component_snapshot_version(repo_root, "linux-pam")?
        }
        "mattos-shadow" => component_snapshot_version(repo_root, "shadow")?,
        "mattos-sudo-rs" => {
            cargo_package_version(&repo_root.join("src/system/auth/sudo-rs/Cargo.toml"))?
        }
        "mattos-util-linux-auth" => component_snapshot_version(repo_root, "util-linux")?,
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
        "bootstrap-runtime" => (
            "host and existing MattOS component install trees; see runtime-files.tsv".to_string(),
            "transitional bootstrap inputs".to_string(),
            "per-file SHA-256 manifest".to_string(),
            "ldd closure of all packaged ELF runtimes".to_string(),
        ),
        component @ ("ncurses" | "kmod" | "procps-ng" | "systemd" | "dbus-broker" | "linux-pam" | "shadow" | "sudo-rs" | "util-linux" | "iproute2" | "iputils") => {
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
            ldd_sonames(
                &install.join("usr/bin/curl"),
                Some(&install.join("usr/lib/x86_64-linux-gnu")),
            )
        }
        "mattos-bootstrap-runtime" => {
            let manifest = fs::read_to_string(repo_root.join(
                "out/packages/staging/mattos-bootstrap-runtime/usr/share/doc/mattos-bootstrap-runtime/runtime-files.tsv",
            ))?;
            Ok(manifest
                .lines()
                .skip(1)
                .filter_map(|line| line.split('\t').next())
                .filter_map(|path| Path::new(path).file_name()?.to_str().map(str::to_string))
                .collect())
        }
        "mattos-dpkg" => {
            let install = repo_root.join("out/build/dpkg/install");
            ldd_sonames_many(
                &[
                    install.join("usr/bin/dpkg"),
                    install.join("usr/bin/dpkg-deb"),
                    install.join("usr/bin/dpkg-query"),
                    install.join("usr/bin/dpkg-divert"),
                    install.join("usr/bin/dpkg-statoverride"),
                    install.join("usr/bin/dpkg-trigger"),
                    install.join("usr/bin/update-alternatives"),
                    install.join("usr/sbin/start-stop-daemon"),
                ],
                &[],
            )
        }
        "mattos-libapt-pkg" => {
            let install = repo_root.join("out/build/apt/install");
            let systemd = repo_root.join("out/build/systemd/install/usr/lib/x86_64-linux-gnu");
            ldd_sonames_many(
                &[install.join("usr/lib/x86_64-linux-gnu/libapt-pkg.so.7.0.0")],
                &[install.join("usr/lib/x86_64-linux-gnu"), systemd],
            )
        }
        "mattos-apt" => {
            let install = repo_root.join("out/build/apt/install");
            let systemd = repo_root.join("out/build/systemd/install/usr/lib/x86_64-linux-gnu");
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
                &[install.join("usr/lib/x86_64-linux-gnu"), systemd],
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
            if output.status.success() {
                binaries.push(path.to_path_buf());
            }
        }
        Ok(())
    })?;
    let library_dirs = [
        staging.join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "apt").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "curl").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "ncurses").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "kmod").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "procps-ng").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "linux-pam").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "systemd").join("usr/lib/x86_64-linux-gnu"),
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
    for spec in specs {
        let root = staging_root.join(spec.name);
        walk_tree(&root, &mut |path, metadata| {
            if !metadata.is_dir() && !path.starts_with(root.join("DEBIAN")) {
                if let Some(name) = path.file_name().and_then(OsStr::to_str) {
                    owners.entry(name.to_string()).or_insert(spec.name);
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
            let owner = owners
                .get(name)
                .ok_or_else(|| anyhow!("{} has unowned runtime dependency {name}", spec.name))?;
            if *owner != spec.name && !spec.depends.contains(owner) {
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
        if spec.provides.is_empty() { "<none>".to_string() } else { spec.provides.join(", ") },
        if spec.conflicts.is_empty() { "<none>".to_string() } else { spec.conflicts.join(", ") },
        if spec.replaces.is_empty() { "<none>".to_string() } else { spec.replaces.join(", ") },
        if conffiles.is_empty() { "<none>".to_string() } else { conffiles.join(", ") },
        if entry.runtime_libraries.is_empty() {
            "<none>".to_string()
        } else {
            entry.runtime_libraries.join(", ")
        },
        if shared_libraries.is_empty() { "<none>".to_string() } else { shared_libraries.join(", ") },
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
            bail!("duplicate repository package/version/architecture: {name} {version} {architecture}")
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
        ("/usr/bin/curl", "mattos-curl"),
        ("/usr/bin/ls", "mattos-coreutils"),
        ("/usr/bin/tar", "mattos-bootstrap-runtime"),
        ("/usr/bin/dpkg", "mattos-dpkg"),
        ("/usr/bin/apt", "mattos-apt"),
        ("/usr/bin/apt-get", "mattos-apt"),
        ("/usr/lib/apt/methods/file", "mattos-apt"),
        (
            "/usr/lib/x86_64-linux-gnu/libapt-pkg.so.7.0",
            "mattos-libapt-pkg",
        ),
        (
            "/usr/lib/x86_64-linux-gnu/libstdc++.so.6",
            "mattos-bootstrap-runtime",
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
    run_cmd(
        &build,
        path_str(&configure)?,
        &[
            "--prefix=/usr",
            "--sysconfdir=/etc",
            "--localstatedir=/var",
            "--libexecdir=/usr/libexec",
            "--disable-dselect",
            "--disable-nls",
        ],
    )?;
    run_cmd(&build, "make", &["-j", "4"])?;
    fs::create_dir_all(&install)?;
    run_cmd(
        &build,
        "make",
        &["install", &format!("DESTDIR={}", install.display())],
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
    println!("built imported dpkg into {}", install.display());
    Ok(())
}

pub(crate) fn build_apt(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/packages/apt");
    if !source.join("CMakeLists.txt").is_file() {
        bail!("APT source missing; run upstream import apt");
    }
    let out = repo_root.join("out/build/apt");
    let source_copy = out.join("source");
    let build = out.join("build");
    let install = out.join("install");
    remove_path_if_exists(&source_copy)?;
    remove_path_if_exists(&build)?;
    remove_path_if_exists(&install)?;
    fs::create_dir_all(&out)?;
    sync_build_source(&source, &source_copy)?;
    run_cmd(
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
            "-DUSE_NLS=OFF",
        ],
    )?;
    run_cmd(
        repo_root,
        "cmake",
        &["--build", path_str(&build)?, "--parallel", "4"],
    )?;
    fs::create_dir_all(&install)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build)?],
        &[("DESTDIR", install.display().to_string())],
    )?;
    for rel in ["usr/bin/apt", "usr/bin/apt-cache", "usr/bin/apt-get"] {
        if !install.join(rel).is_file() {
            bail!("APT build did not produce {rel}");
        }
    }
    println!("built imported APT into {}", install.display());
    Ok(())
}

fn relative_display(root: &Path, path: &Path) -> Result<String> {
    Ok(path.strip_prefix(root)?.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

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
                let extra = if *name == "mattos-apt" {
                    extra_apt_field.unwrap_or("")
                } else {
                    ""
                };
                format!("Package: {name}\nVersion: 1\nArchitecture: amd64\n{extra}\n")
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
            &["mattos-bootstrap-runtime (= 0.1-1mattos1)".into()],
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
            "mattos-bootstrap-runtime",
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
            "mattos-iproute2",
            "mattos-iputils",
        ] {
            assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
        }
        assert_eq!(PACKAGE_NAMES.len(), 30);
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
        let mut body = repository_packages(Some("Depends: mattos-runtime-abi\n"));
        body = body.replace(
            "Package: mattos-bootstrap-runtime\n",
            "Package: mattos-bootstrap-runtime\nProvides: mattos-runtime-abi\n",
        );
        assert!(validate_repository_packages(&body).is_ok());
    }

    #[test]
    fn repository_dependency_closure_rejects_missing_and_wrong_exact_versions() {
        assert!(validate_repository_packages(&repository_packages(Some(
            "Depends: mattos-libapt-pkg (= 1)\n"
        )))
        .is_ok());
        assert!(validate_repository_packages(&repository_packages(Some(
            "Depends: mattos-missing\n"
        )))
        .is_err());
        assert!(validate_repository_packages(&repository_packages(Some(
            "Depends: mattos-libapt-pkg (= 2)\n"
        )))
        .is_err());
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
        assert!(APT_CONFFILES
            .iter()
            .all(|path| path.starts_with("/etc/apt/")));
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
        assert!(position("mattos-filesystem") < position("mattos-bootstrap-runtime"));
        assert!(position("mattos-bootstrap-runtime") < position("mattos-dpkg"));
        assert!(position("mattos-dpkg") < position("mattos-apt"));
        assert!(position("mattos-libapt-pkg") < position("mattos-apt"));
        assert!(position("mattos-libudev1") < position("mattos-libapt-pkg"));
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
