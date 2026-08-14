use clap::ValueEnum;
#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, ValueEnum)]
pub(crate) enum BuildStage {
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
    Gzip,
    Patch,
    File,
    Less,
    Git,
    Openssh,
    Libffi,
    Wayland,
    Xkbcommon,
    Libseat,
    LibdisplayInfo,
    Libevdev,
    Libinput,
    Pixman,
    Libdrm,
    Mesa,
    CosmicComp,
    Python,
    Llvm,
    Rust,
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
    Installer,
    Rootfs,
    LiveRoot,
    Initramfs,
    Iso,
    All,
}

pub(crate) fn stage_id(stage: BuildStage) -> &'static str {
    match stage {
        BuildStage::Kernel => "linux",
        BuildStage::Glibc => "glibc",
        BuildStage::GccRuntime => "gcc-runtime",
        BuildStage::Binutils => "binutils",
        BuildStage::GccToolchain => "gcc-compiler",
        BuildStage::Make => "make",
        BuildStage::Brush => "brush",
        BuildStage::Coreutils => "coreutils",
        BuildStage::Grep => "grep",
        BuildStage::Sed => "sed",
        BuildStage::Findutils => "findutils",
        BuildStage::Diffutils => "diffutils",
        BuildStage::Gzip => "gzip",
        BuildStage::Patch => "patch",
        BuildStage::File => "file",
        BuildStage::Less => "less",
        BuildStage::Git => "git",
        BuildStage::Openssh => "openssh",
        BuildStage::Libffi => "libffi",
        BuildStage::Wayland => "wayland",
        BuildStage::Xkbcommon => "xkbcommon",
        BuildStage::Libseat => "seatd",
        BuildStage::LibdisplayInfo => "libdisplay-info",
        BuildStage::Libevdev => "libevdev",
        BuildStage::Libinput => "libinput",
        BuildStage::Pixman => "pixman",
        BuildStage::Libdrm => "libdrm",
        BuildStage::Mesa => "mesa",
        BuildStage::CosmicComp => "cosmic-comp",
        BuildStage::Python => "cpython",
        BuildStage::Llvm => "llvm",
        BuildStage::Rust => "rust",
        BuildStage::Kmod => "kmod",
        BuildStage::Procps => "procps-ng",
        BuildStage::Ncurses => "ncurses",
        BuildStage::Iproute2 => "iproute2",
        BuildStage::Iputils => "iputils",
        BuildStage::Curl => "curl",
        BuildStage::Expat => "expat",
        BuildStage::Libcap => "libcap",
        BuildStage::Attr => "attr",
        BuildStage::Tar => "tar",
        BuildStage::Acl => "acl",
        BuildStage::Zlib => "zlib",
        BuildStage::Bzip2 => "bzip2",
        BuildStage::Lz4 => "lz4",
        BuildStage::Xz => "xz",
        BuildStage::Xxhash => "xxhash",
        BuildStage::Zstd => "zstd",
        BuildStage::Openssl => "openssl",
        BuildStage::Elfutils => "elfutils",
        BuildStage::Pcre2 => "pcre2",
        BuildStage::Selinux => "selinux",
        BuildStage::Libxcrypt => "libxcrypt",
        BuildStage::Libmd => "libmd",
        BuildStage::Libbsd => "libbsd",
        BuildStage::Pam => "linux-pam",
        BuildStage::Shadow => "shadow",
        BuildStage::SudoRs => "sudo-rs",
        BuildStage::UtilLinux => "util-linux",
        BuildStage::Systemd => "systemd",
        BuildStage::DbusBroker => "dbus-broker",
        BuildStage::Dpkg => "dpkg",
        BuildStage::Apt => "apt",
        BuildStage::Init => "init",
        BuildStage::Installer => "installer",
        BuildStage::Rootfs => "rootfs",
        BuildStage::LiveRoot => "live-root",
        BuildStage::Initramfs => "initramfs",
        BuildStage::Iso => "iso",
        BuildStage::All => "all",
    }
}

pub(crate) fn direct_dependencies(stage: BuildStage) -> &'static [&'static str] {
    match stage {
        BuildStage::Kernel | BuildStage::Glibc | BuildStage::All => &[],
        BuildStage::GccRuntime => &["glibc", "linux-headers"],
        BuildStage::Binutils => &["gcc-runtime"],
        BuildStage::GccToolchain => &["binutils", "gcc-runtime"],
        BuildStage::Make => &["gcc-compiler", "binutils", "gcc-runtime"],
        BuildStage::Acl => &["formal-sysroot", "attr"],
        BuildStage::Openssl | BuildStage::Elfutils => &["formal-sysroot", "zlib", "zstd"],
        BuildStage::Selinux => &["formal-sysroot", "pcre2"],
        BuildStage::Libbsd => &["formal-sysroot", "libmd"],
        BuildStage::Tar => &["formal-sysroot", "acl", "attr"],
        BuildStage::File => &["formal-sysroot", "zlib"],
        BuildStage::Less => &["formal-sysroot", "ncurses", "pcre2"],
        BuildStage::Git => &[
            "formal-sysroot",
            "curl",
            "expat",
            "openssl",
            "zlib",
            "zstd",
            "pcre2",
        ],
        BuildStage::Openssh => &[
            "formal-sysroot",
            "openssl",
            "zlib",
            "zstd",
            "linux-pam",
            "libxcrypt",
        ],
        BuildStage::Libffi => &["formal-sysroot"],
        BuildStage::Wayland => &["formal-sysroot", "libffi"],
        BuildStage::Xkbcommon => &["formal-sysroot"],
        BuildStage::Libseat | BuildStage::Libevdev | BuildStage::Pixman => &["formal-sysroot"],
        BuildStage::LibdisplayInfo => &["formal-sysroot"],
        BuildStage::Libinput => &["formal-sysroot", "libevdev", "systemd"],
        BuildStage::Libdrm => &["formal-sysroot"],
        BuildStage::Mesa => &["formal-sysroot", "libdrm", "libdisplay-info", "llvm", "zlib"],
        BuildStage::CosmicComp => &[
            "formal-sysroot", "seatd", "libdisplay-info", "libinput", "pixman", "mesa",
            "wayland", "xkbcommon", "systemd",
        ],
        BuildStage::Python => &[
            "formal-sysroot",
            "libffi",
            "openssl",
            "zlib",
            "bzip2",
            "xz",
            "expat",
            "ncurses",
        ],
        BuildStage::Llvm => &["formal-sysroot", "zlib", "zstd"],
        BuildStage::Rust => &["formal-sysroot", "llvm", "openssl", "zlib"],
        BuildStage::Procps => &["formal-sysroot", "ncurses"],
        BuildStage::Iproute2 => &[
            "formal-sysroot",
            "libcap",
            "zlib",
            "zstd",
            "elfutils",
            "pcre2",
            "selinux",
        ],
        BuildStage::Curl => &["formal-sysroot", "openssl", "zlib", "zstd"],
        BuildStage::Pam => &["formal-sysroot", "libxcrypt"],
        BuildStage::UtilLinux => &[
            "formal-sysroot",
            "linux-pam",
            "selinux",
            "pcre2",
            "ncurses",
        ],
        BuildStage::Shadow => &[
            "formal-sysroot",
            "linux-pam",
            "libbsd",
            "libmd",
            "libxcrypt",
        ],
        BuildStage::SudoRs => &["formal-sysroot", "linux-pam"],
        BuildStage::Systemd => &[
            "formal-sysroot",
            "kmod",
            "util-linux",
            "linux-pam",
            "libcap",
            "openssl",
            "pcre2",
        ],
        BuildStage::DbusBroker => &["formal-sysroot", "systemd", "expat"],
        BuildStage::Dpkg => &[
            "formal-sysroot",
            "zlib",
            "bzip2",
            "xz",
            "zstd",
            "libmd",
            "selinux",
            "pcre2",
        ],
        BuildStage::Apt => &[
            "formal-sysroot",
            "dpkg",
            "openssl",
            "zlib",
            "bzip2",
            "xz",
            "zstd",
            "systemd",
        ],
        BuildStage::Installer => &["formal-sysroot", "util-linux", "zlib", "zstd", "linux", "wayland", "xkbcommon", "cosmic-comp"],
        BuildStage::Rootfs => &[
            "apt",
            "dpkg",
            "systemd",
            "dbus-broker",
            "grep",
            "sed",
            "findutils",
            "diffutils",
            "init",
            "installer",
            "repository",
        ],
        BuildStage::LiveRoot => &["rootfs"],
        BuildStage::Initramfs => &["formal-sysroot"],
        BuildStage::Iso => &["linux", "live-root", "initramfs"],
        _ => &["formal-sysroot"],
    }
}

pub(crate) fn all_build_stages() -> &'static [BuildStage] {
    &[
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
        BuildStage::Gzip,
        BuildStage::Patch,
        BuildStage::File,
        BuildStage::Less,
        BuildStage::Git,
        BuildStage::Openssh,
        BuildStage::Libffi,
        BuildStage::Wayland,
        BuildStage::Xkbcommon,
        BuildStage::Libseat,
        BuildStage::LibdisplayInfo,
        BuildStage::Libevdev,
        BuildStage::Libinput,
        BuildStage::Pixman,
        BuildStage::Libdrm,
        BuildStage::Mesa,
        BuildStage::CosmicComp,
        BuildStage::Python,
        BuildStage::Llvm,
        BuildStage::Rust,
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
        BuildStage::Installer,
        BuildStage::Rootfs,
        BuildStage::LiveRoot,
        BuildStage::Initramfs,
        BuildStage::Iso,
    ]
}

pub(crate) fn build_plan(stage: BuildStage) -> Vec<BuildStage> {
    if stage == BuildStage::All {
        all_build_stages().to_vec()
    } else {
        vec![stage]
    }
}

#[cfg(test)]
pub(crate) fn dependency_map() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    let mut dependencies = all_build_stages()
        .iter()
        .map(|stage| {
            (
                stage_id(*stage),
                direct_dependencies(*stage).iter().copied().collect(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    dependencies.insert("linux-headers", ["glibc"].into_iter().collect());
    dependencies.insert(
        "formal-sysroot",
        ["linux-headers", "glibc", "gcc-runtime"]
            .into_iter()
            .collect(),
    );
    let package_producers = all_build_stages()
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
        .map(stage_id)
        .collect::<BTreeSet<_>>();
    dependencies.insert("packages", package_producers);
    dependencies.insert("repository", ["packages"].into_iter().collect());
    dependencies.entry("rootfs").or_default().insert("packages");
    dependencies
}

#[cfg(test)]
pub(crate) fn downstream_invalidation(changed_outputs: &[&'static str]) -> BTreeSet<&'static str> {
    let graph = dependency_map();
    let mut invalidated = changed_outputs.iter().copied().collect::<BTreeSet<_>>();
    let mut queue = changed_outputs.iter().copied().collect::<VecDeque<_>>();
    while let Some(changed) = queue.pop_front() {
        for (stage, dependencies) in &graph {
            if dependencies.contains(changed) && invalidated.insert(stage) {
                queue.push_back(stage);
            }
        }
    }
    invalidated
}

#[cfg(test)]
fn actual_rebuilds(
    direct_input_owners: &[&'static str],
    changed_outputs: &[&'static str],
) -> BTreeSet<&'static str> {
    let graph = dependency_map();
    let changed_outputs = changed_outputs.iter().copied().collect::<BTreeSet<_>>();
    let mut rebuilt = direct_input_owners.iter().copied().collect::<BTreeSet<_>>();
    let mut queue = direct_input_owners.iter().copied().collect::<VecDeque<_>>();
    while let Some(completed) = queue.pop_front() {
        if !changed_outputs.contains(completed) {
            continue;
        }
        for (stage, dependencies) in &graph {
            if dependencies.contains(completed) && rebuilt.insert(stage) {
                queue.push_back(stage);
            }
        }
    }
    rebuilt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_references_only_known_stages() {
        let graph = dependency_map();
        for (stage, dependencies) in &graph {
            for dependency in dependencies {
                assert!(
                    graph.contains_key(dependency),
                    "{stage} depends on unknown {dependency}"
                );
            }
        }
    }

    #[test]
    fn byte_identical_dependency_rebuild_does_not_invalidate_consumers() {
        assert!(downstream_invalidation(&[]).is_empty());
    }

    #[test]
    fn representative_output_cascades_are_exact() {
        assert_eq!(
            downstream_invalidation(&["brush"]),
            [
                "brush",
                "packages",
                "repository",
                "rootfs",
                "live-root",
                "iso"
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(
            downstream_invalidation(&["linux"]),
            ["linux", "installer", "packages", "repository", "rootfs", "live-root", "iso"]
                .into_iter()
                .collect()
        );
        assert_eq!(
            downstream_invalidation(&["repository"]),
            ["repository", "rootfs", "live-root", "iso"]
                .into_iter()
                .collect()
        );
        assert_eq!(
            downstream_invalidation(&["rootfs"]),
            ["rootfs", "live-root", "iso"].into_iter().collect()
        );
        assert_eq!(
            downstream_invalidation(&["initramfs"]),
            ["initramfs", "iso"].into_iter().collect()
        );
        assert_eq!(
            downstream_invalidation(&["git"]),
            ["git", "packages", "repository", "rootfs", "live-root", "iso"]
                .into_iter()
                .collect()
        );
        assert_eq!(
            downstream_invalidation(&["cpython"]),
            ["cpython", "packages", "repository", "rootfs", "live-root", "iso"]
                .into_iter()
                .collect()
        );
        assert_eq!(
            downstream_invalidation(&["llvm"]),
            ["llvm", "rust", "packages", "repository", "rootfs", "live-root", "iso"]
                .into_iter()
                .collect()
        );
        for unrelated in [
            "linux",
            "glibc",
            "gcc-runtime",
            "gcc-compiler",
            "binutils",
            "curl",
            "openssl",
            "zlib",
        ] {
            assert!(
                !downstream_invalidation(&["git"]).contains(unrelated),
                "Git invalidation escaped into unrelated stage {unrelated}"
            );
        }
    }

    #[derive(Clone, Copy)]
    enum SyntheticChange {
        None,
        IrrelevantSource,
        RelevantInput(&'static str),
        DependencyInputWithIdenticalOutput(&'static str),
        DependencyOutput(&'static str),
        MissingOutput(&'static str),
        CorruptedOutput(&'static str),
        Recipe(&'static str),
    }

    fn expected_misses(change: SyntheticChange) -> BTreeSet<&'static str> {
        match change {
            SyntheticChange::None
            | SyntheticChange::IrrelevantSource
            | SyntheticChange::DependencyInputWithIdenticalOutput(_) => BTreeSet::new(),
            SyntheticChange::RelevantInput(stage)
            | SyntheticChange::DependencyOutput(stage)
            | SyntheticChange::MissingOutput(stage)
            | SyntheticChange::CorruptedOutput(stage)
            | SyntheticChange::Recipe(stage) => downstream_invalidation(&[stage]),
        }
    }

    #[test]
    fn incremental_cache_scenario_matrix_asserts_misses_and_hits() {
        let scenarios = [
            ("no change", SyntheticChange::None, BTreeSet::new()),
            (
                "irrelevant source",
                SyntheticChange::IrrelevantSource,
                BTreeSet::new(),
            ),
            (
                "relevant source",
                SyntheticChange::RelevantInput("brush"),
                downstream_invalidation(&["brush"]),
            ),
            (
                "configuration",
                SyntheticChange::RelevantInput("linux"),
                downstream_invalidation(&["linux"]),
            ),
            (
                "dependency output",
                SyntheticChange::DependencyOutput("zlib"),
                downstream_invalidation(&["zlib"]),
            ),
            (
                "dependency input, identical output",
                SyntheticChange::DependencyInputWithIdenticalOutput("zlib"),
                BTreeSet::new(),
            ),
            (
                "missing output",
                SyntheticChange::MissingOutput("binutils"),
                downstream_invalidation(&["binutils"]),
            ),
            (
                "corrupt output",
                SyntheticChange::CorruptedOutput("initramfs"),
                downstream_invalidation(&["initramfs"]),
            ),
            (
                "package only",
                SyntheticChange::RelevantInput("packages"),
                downstream_invalidation(&["packages"]),
            ),
            (
                "rootfs only",
                SyntheticChange::RelevantInput("rootfs"),
                downstream_invalidation(&["rootfs"]),
            ),
            (
                "Linux only",
                SyntheticChange::RelevantInput("linux"),
                downstream_invalidation(&["linux"]),
            ),
            (
                "build recipe",
                SyntheticChange::Recipe("gcc-compiler"),
                downstream_invalidation(&["gcc-compiler"]),
            ),
        ];
        let all = dependency_map().keys().copied().collect::<BTreeSet<_>>();
        for (name, change, expected) in scenarios {
            let misses = expected_misses(change);
            assert_eq!(misses, expected, "wrong misses for {name}");
            let hits = all.difference(&misses).copied().collect::<BTreeSet<_>>();
            assert_eq!(
                hits.len() + misses.len(),
                all.len(),
                "incomplete hit/miss partition for {name}"
            );
            assert!(
                hits.is_disjoint(&misses),
                "stage both hit and missed for {name}"
            );
        }
    }

    #[test]
    fn dependency_input_change_with_identical_bytes_keeps_consumers_hot() {
        let change = SyntheticChange::DependencyInputWithIdenticalOutput("glibc");
        assert!(expected_misses(change).is_empty());
        let SyntheticChange::DependencyInputWithIdenticalOutput(rebuilt) = change else {
            unreachable!()
        };
        assert_eq!(rebuilt, "glibc");
    }

    #[test]
    fn representative_cascade_report() {
        let scenarios: &[(&str, &[&str], usize, &[&str])] = &[
            ("Brush source", &["brush"], 6, &["zlib", "linux"]),
            ("glibc source", &["glibc"], 63, &["linux"]),
            ("Linux x86_64 config", &["linux"], 7, &["glibc", "brush"]),
            (
                "Linux x86_64 UAPI source",
                &["linux", "glibc", "linux-headers"],
                64,
                &[],
            ),
            (
                "GCC source",
                &["gcc-runtime", "gcc-compiler"],
                61,
                &["linux", "glibc", "linux-headers"],
            ),
            ("zlib shared library", &["zlib"], 21, &["brush", "linux"]),
            ("package metadata", &["packages"], 5, &["brush", "zlib"]),
            ("repository policy", &["repository"], 4, &["packages", "brush"]),
            ("rootfs configuration", &["rootfs"], 3, &["repository", "packages"]),
            ("initramfs configuration", &["initramfs"], 2, &["rootfs", "packages"]),
            ("live-root recipe", &["live-root"], 2, &["rootfs", "packages"]),
        ];
        for (name, changed, expected_count, unrelated_hits) in scenarios {
            let invalidated = downstream_invalidation(changed);
            println!("{name}: {} stage(s): {invalidated:?}", invalidated.len());
            assert!(changed.iter().all(|stage| invalidated.contains(stage)));
            assert_eq!(invalidated.len(), *expected_count, "closure changed for {name}");
            assert!(
                unrelated_hits.iter().all(|stage| !invalidated.contains(stage)),
                "unrelated stage invalidated for {name}"
            );
        }
    }

    #[test]
    fn representative_incremental_rebuilds_distinguish_candidates_from_changed_bytes() {
        struct Scenario {
            name: &'static str,
            owners: &'static [&'static str],
            all_rebuilt_outputs_change: bool,
            unrelated_hits: &'static [&'static str],
        }
        let scenarios = [
            Scenario {
                name: "Brush source, identical binary",
                owners: &["brush"],
                all_rebuilt_outputs_change: false,
                unrelated_hits: &["zlib", "linux", "packages", "rootfs"],
            },
            Scenario {
                name: "Brush source, changed binary",
                owners: &["brush"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["zlib", "linux", "glibc"],
            },
            Scenario {
                name: "glibc source, identical publication",
                owners: &["glibc"],
                all_rebuilt_outputs_change: false,
                unrelated_hits: &["linux", "gcc-runtime", "packages", "rootfs"],
            },
            Scenario {
                name: "glibc source, changed publication",
                owners: &["glibc"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["linux"],
            },
            Scenario {
                name: "Linux source, changed kernel",
                owners: &["linux"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["glibc", "linux-headers", "brush"],
            },
            Scenario {
                name: "Linux config, changed kernel",
                owners: &["linux"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["glibc", "linux-headers", "brush"],
            },
            Scenario {
                name: "Linux UAPI, changed kernel and headers",
                owners: &["linux", "glibc", "linux-headers"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &[],
            },
            Scenario {
                name: "GCC source, identical runtime and compiler",
                owners: &["gcc-runtime", "gcc-compiler"],
                all_rebuilt_outputs_change: false,
                unrelated_hits: &["linux", "glibc", "binutils", "packages", "rootfs"],
            },
            Scenario {
                name: "GCC source, changed runtime and compiler",
                owners: &["gcc-runtime", "gcc-compiler"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["linux", "glibc", "linux-headers"],
            },
            Scenario {
                name: "zlib source, changed library",
                owners: &["zlib"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["brush", "linux", "glibc"],
            },
            Scenario {
                name: "package metadata, changed package and inventory",
                owners: &["packages"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["brush", "zlib", "linux", "glibc"],
            },
            Scenario {
                name: "rootfs configuration, identical rootfs",
                owners: &["rootfs"],
                all_rebuilt_outputs_change: false,
                unrelated_hits: &["packages", "repository", "initramfs", "iso"],
            },
            Scenario {
                name: "rootfs configuration, changed rootfs",
                owners: &["rootfs"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["packages", "repository", "linux"],
            },
            Scenario {
                name: "initramfs recipe, changed archive",
                owners: &["initramfs"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["rootfs", "packages", "repository", "linux"],
            },
        ];
        let all = dependency_map().keys().copied().collect::<BTreeSet<_>>();
        for scenario in scenarios {
            let candidates = scenario
                .owners
                .iter()
                .flat_map(|owner| downstream_invalidation(&[*owner]))
                .collect::<BTreeSet<_>>();
            let changed_outputs = if scenario.all_rebuilt_outputs_change {
                candidates.iter().copied().collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let required = actual_rebuilds(scenario.owners, &changed_outputs);
            let expected_required = if scenario.all_rebuilt_outputs_change {
                candidates.clone()
            } else {
                scenario.owners.iter().copied().collect()
            };
            assert_eq!(required, expected_required, "wrong rebuilds for {}", scenario.name);
            assert!(required.is_subset(&candidates), "rebuild escaped candidate closure for {}", scenario.name);
            let hits = all.difference(&required).copied().collect::<BTreeSet<_>>();
            assert!(
                scenario.unrelated_hits.iter().all(|stage| hits.contains(stage)),
                "unrelated stage rebuilt for {}",
                scenario.name
            );
        }
    }
}
