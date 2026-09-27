use super::*;
use crate::cache_manifest::{
    STAGE_MANIFEST_SCHEMA_VERSION, StageInputDetails, StageInputs, StageManifest,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn write_file(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

fn isolated(stage: BuildStage) -> performance::StageSpec {
    let mut spec = build_stage_spec(stage);
    spec.dependencies.clear();
    spec.tools.clear();
    spec
}

fn materialize_inputs(root: &Path, specs: &[(&str, performance::StageSpec)]) {
    for (_, spec) in specs {
        for path in spec.source_inputs.iter().chain(&spec.configuration_inputs) {
            let absolute = root.join(path);
            if absolute.symlink_metadata().is_err() {
                fs::create_dir_all(absolute).unwrap();
            }
        }
    }
}

fn publish_dependency(root: &Path, stage: &str, input: &str, output: &str) {
    performance::write_stage_manifest(
        root,
        &StageManifest {
            schema_version: STAGE_MANIFEST_SCHEMA_VERSION,
            stage: stage.to_string(),
            inputs: StageInputs {
                source_digest: input.to_string(),
                configuration_digest: String::new(),
                tool_digest: String::new(),
                build_provenance_digest: String::new(),
                environment_digest: String::new(),
                dependency_digests: BTreeMap::new(),
                full_digest: input.to_string(),
            },
            input_details: StageInputDetails::default(),
            expected_outputs: Vec::new(),
            output_content_digest: output.to_string(),
        },
    )
    .unwrap();
}

#[test]
fn real_stage_specs_invalidate_only_representative_input_owners() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    for (path, body) in [
        ("src/userland/brush/src/test.rs", "brush one\n"),
        ("src/system/libc/glibc/test.c", "glibc one\n"),
        ("src/kernel/linux/kernel/test.c", "linux one\n"),
        ("src/kernel/linux/include/uapi/linux/test.h", "uapi one\n"),
        ("src/kernel/config/x86_64_mattos.config", "CONFIG_TEST=y\n"),
        ("src/toolchain/gcc/gcc/test.c", "gcc one\n"),
        ("src/system/libraries/zlib/test.c", "zlib one\n"),
        ("src/system/units/test.service", "rootfs one\n"),
        ("out/packages/inventory.toml", "packages one\n"),
        (AUTHORITATIVE_GRUB_CFG, "grub one\n"),
    ] {
        write_file(&root.join(path), body);
    }
    let mut linux_headers = linux_headers_stage_spec();
    linux_headers.dependencies.clear();
    linux_headers.tools.clear();
    let specs = [
        ("brush", isolated(BuildStage::Brush)),
        ("linux", isolated(BuildStage::Kernel)),
        ("glibc", isolated(BuildStage::Glibc)),
        ("linux-headers", linux_headers),
        ("gcc-runtime", isolated(BuildStage::GccRuntime)),
        ("gcc-toolchain", isolated(BuildStage::GccToolchain)),
        ("zlib", isolated(BuildStage::Zlib)),
        ("rootfs", isolated(BuildStage::Rootfs)),
        ("live-root", isolated(BuildStage::LiveRoot)),
        ("initramfs", isolated(BuildStage::Initramfs)),
        ("iso", isolated(BuildStage::Iso)),
    ];
    materialize_inputs(root, &specs);
    let snapshot = || {
        specs
            .iter()
            .map(|(name, spec)| {
                (
                    *name,
                    performance::compute_stage_inputs(root, spec)
                        .unwrap()
                        .full_digest,
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let assert_change = |before: &BTreeMap<&str, String>, expected: &[&str]| {
        let after = snapshot();
        let changed = after
            .iter()
            .filter_map(|(name, digest)| (before.get(name) != Some(digest)).then_some(*name))
            .collect::<BTreeSet<_>>();
        assert_eq!(changed, expected.iter().copied().collect());
        after
    };

    let before = snapshot();
    write_file(&root.join("src/userland/brush/src/test.rs"), "brush two\n");
    let before = assert_change(&before, &["brush"]);
    write_file(&root.join("src/system/libc/glibc/test.c"), "glibc two\n");
    let before = assert_change(&before, &["glibc"]);
    write_file(&root.join("src/kernel/linux/kernel/test.c"), "linux two\n");
    let before = assert_change(&before, &["linux"]);
    write_file(
        &root.join("src/kernel/config/x86_64_mattos.config"),
        "CONFIG_TEST=n\n",
    );
    let before = assert_change(&before, &["linux"]);
    write_file(
        &root.join("src/kernel/linux/include/uapi/linux/test.h"),
        "uapi two\n",
    );
    let before = assert_change(&before, &["glibc", "linux", "linux-headers"]);
    write_file(&root.join("src/toolchain/gcc/gcc/test.c"), "gcc two\n");
    let before = assert_change(&before, &["gcc-runtime", "gcc-toolchain"]);
    write_file(&root.join("src/system/libraries/zlib/test.c"), "zlib two\n");
    let before = assert_change(&before, &["zlib"]);
    write_file(&root.join("src/system/units/test.service"), "rootfs two\n");
    let before = assert_change(&before, &["rootfs"]);
    write_file(&root.join("out/packages/inventory.toml"), "packages two\n");
    let before = assert_change(&before, &["rootfs"]);
    write_file(&root.join(AUTHORITATIVE_GRUB_CFG), "grub changed\n");
    assert_change(&before, &["iso"]);
}

#[test]
fn real_stage_specs_track_tool_recipe_and_dependency_output_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let tool = root.join("fixture-tool");
    write_file(&tool, "#!/bin/sh\necho version-one\n");
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();

    let mut brush = build_stage_spec(BuildStage::Brush);
    brush.source_inputs.clear();
    brush.configuration_inputs.clear();
    brush.dependencies.clear();
    brush.tools = vec![tool.to_string_lossy().into_owned()];
    brush.outputs = vec!["out/build/brush/install/usr/bin/brush".into()];
    let first = performance::compute_stage_inputs(root, &brush).unwrap();
    // Replacing an executable by rename avoids ETXTBSY when the identity
    // probe's interpreter still has the old script open.
    let replacement = root.join("fixture-tool.next");
    write_file(&replacement, "#!/bin/sh\necho version-two\n");
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    fs::rename(replacement, &tool).unwrap();
    let second = performance::compute_stage_inputs(root, &brush).unwrap();
    assert_eq!(first.tool_digest, second.tool_digest);
    assert_ne!(
        first.build_provenance_digest,
        second.build_provenance_digest
    );

    let mut revised = brush.clone();
    revised.recipe.push_str(":revision-two");
    let third = performance::compute_stage_inputs(root, &revised).unwrap();
    assert_ne!(second.full_digest, third.full_digest);

    let mut initramfs = build_stage_spec(BuildStage::Initramfs);
    initramfs.dependencies.clear();
    write_file(
        &root.join("src/boot/live-init.c"),
        "int main(void) { return 0; }\n",
    );
    write_file(&root.join("src/boot/module-loader.h"), "/* loader */\n");
    write_file(
        &root.join("src/system/data/linux-firmware/WHENCE"),
        "fixture firmware provenance\n",
    );
    write_file(
        &root.join("src/tools/mattos-build/src/stages/image.rs"),
        "fixture image policy\n",
    );
    let initramfs_before = performance::compute_stage_inputs(root, &initramfs).unwrap();
    initramfs.recipe.push_str(":revision-two");
    let initramfs_after = performance::compute_stage_inputs(root, &initramfs).unwrap();
    assert_ne!(initramfs_before.full_digest, initramfs_after.full_digest);

    publish_dependency(root, "formal-sysroot", "input-one", "same-sysroot-bytes");
    publish_dependency(root, "linux", "input-one", "same-linux-bytes");
    let initramfs = build_stage_spec(BuildStage::Initramfs);
    let before = performance::compute_stage_inputs(root, &initramfs).unwrap();
    publish_dependency(root, "formal-sysroot", "input-two", "same-sysroot-bytes");
    let identical = performance::compute_stage_inputs(root, &initramfs).unwrap();
    assert_eq!(before.full_digest, identical.full_digest);
    publish_dependency(
        root,
        "formal-sysroot",
        "input-three",
        "changed-sysroot-bytes",
    );
    let changed = performance::compute_stage_inputs(root, &initramfs).unwrap();
    assert_ne!(identical.full_digest, changed.full_digest);

    publish_dependency(root, "linux", "linux-input", "kernel-bytes");
    publish_dependency(root, "live-root", "root-input", "live-root-bytes");
    publish_dependency(root, "initramfs", "input-one", "same-initramfs-bytes");
    write_file(&root.join(AUTHORITATIVE_GRUB_CFG), "grub\n");
    let iso = build_stage_spec(BuildStage::Iso);
    let iso_before = performance::compute_stage_inputs(root, &iso).unwrap();
    publish_dependency(root, "initramfs", "input-two", "same-initramfs-bytes");
    let iso_identical = performance::compute_stage_inputs(root, &iso).unwrap();
    assert_eq!(iso_before.full_digest, iso_identical.full_digest);
    publish_dependency(root, "initramfs", "input-three", "changed-initramfs-bytes");
    let iso_changed = performance::compute_stage_inputs(root, &iso).unwrap();
    assert_ne!(iso_identical.full_digest, iso_changed.full_digest);
}

#[test]
fn package_rootfs_initramfs_and_iso_contracts_follow_consumed_artifacts() {
    let rootfs = build_stage_spec(BuildStage::Rootfs);
    assert!(
        rootfs
            .configuration_inputs
            .contains(&PathBuf::from("out/packages/inventory.toml"))
    );
    assert!(rootfs.dependencies.contains(&"repository".to_string()));

    let initramfs = build_stage_spec(BuildStage::Initramfs);
    assert!(initramfs.configuration_inputs.is_empty());
    assert_eq!(initramfs.dependencies, ["formal-sysroot", "linux"]);
    assert_eq!(
        initramfs.source_inputs,
        [
            PathBuf::from("src/boot/live-init.c"),
            PathBuf::from("src/boot/module-loader.h"),
            PathBuf::from("src/system/data/linux-firmware"),
            PathBuf::from("src/tools/mattos-build/src/stages/image.rs")
        ]
    );
    assert_eq!(
        initramfs.outputs,
        [PathBuf::from("out/build/early-initramfs.cpio.xz")]
    );
    assert_eq!(initramfs.tools, ["gcc", "cpio", "xz", "modinfo"]);

    let live_root = build_stage_spec(BuildStage::LiveRoot);
    assert_eq!(live_root.dependencies, ["rootfs"]);
    assert!(
        live_root
            .outputs
            .contains(&PathBuf::from("out/build/live-root.squashfs"))
    );
    assert_eq!(live_root.tools, ["mksquashfs", "unsquashfs"]);

    let iso = build_stage_spec(BuildStage::Iso);
    assert_eq!(
        iso.source_inputs,
        [
            PathBuf::from(AUTHORITATIVE_GRUB_CFG),
            PathBuf::from("src/tools/mattos-build/src/stages/image.rs"),
        ]
    );
    assert!(iso.dependencies.contains(&"linux".to_string()));
    assert!(iso.dependencies.contains(&"live-root".to_string()));
    assert!(iso.dependencies.contains(&"initramfs".to_string()));
}

#[test]
fn cold_build_concurrency_groups_preserve_barriers_and_output_ownership() {
    let graph = crate::stage_graph::dependency_map();
    assert!(graph["cross-toolchain"].is_empty());
    assert_eq!(graph["glibc"], ["cross-toolchain"].into_iter().collect());
    assert_eq!(
        graph["gcc-runtime"],
        ["cross-toolchain", "glibc", "linux-headers"].into_iter().collect()
    );
    assert_eq!(graph["linux"], ["cross-toolchain"].into_iter().collect());
    assert_eq!(
        graph["binutils"],
        ["cross-toolchain", "gcc-runtime"].into_iter().collect()
    );
    assert_eq!(
        graph["gcc-compiler"],
        ["binutils", "cross-toolchain", "gcc-runtime"].into_iter().collect()
    );
    assert_eq!(
        graph["formal-sysroot"],
        ["gcc-runtime", "glibc", "linux-headers"]
            .into_iter()
            .collect()
    );
    assert_eq!(graph["repository"], ["packages"].into_iter().collect());
    assert!(graph["rootfs"].contains("repository"));
    assert_eq!(graph["live-root"], ["rootfs"].into_iter().collect());
    assert_eq!(
        graph["initramfs"],
        ["formal-sysroot", "linux"].into_iter().collect()
    );
    assert_eq!(
        graph["iso"],
        ["grub", "initramfs", "linux", "live-root", "repository"]
            .into_iter()
            .collect()
    );

    // Cargo-built userland additionally waits for the MattOS rustc.
    for stage in [
        BuildStage::Brush,
        BuildStage::Coreutils,
        BuildStage::Grep,
        BuildStage::Sed,
        BuildStage::Findutils,
        BuildStage::Diffutils,
        BuildStage::Init,
    ] {
        assert_eq!(build_stage_spec(stage).dependencies, ["formal-sysroot", "rust"]);
    }
    let independent_after_sysroot = [
        BuildStage::Expat,
        BuildStage::Libcap,
        BuildStage::Attr,
        BuildStage::Zlib,
        BuildStage::Bzip2,
        BuildStage::Lz4,
        BuildStage::Xz,
        BuildStage::Xxhash,
        BuildStage::Zstd,
        BuildStage::Pcre2,
        BuildStage::Libxcrypt,
        BuildStage::Libmd,
        BuildStage::Ncurses,
        BuildStage::Iputils,
    ];
    let specs = independent_after_sysroot
        .iter()
        .map(|stage| build_stage_spec(*stage))
        .collect::<Vec<_>>();
    for spec in &specs {
        assert_eq!(spec.dependencies, ["formal-sysroot"]);
    }
    for (index, left) in specs.iter().enumerate() {
        for right in &specs[index + 1..] {
            for left_output in &left.outputs {
                for right_output in &right.outputs {
                    assert!(
                        !left_output.starts_with(right_output)
                            && !right_output.starts_with(left_output),
                        "concurrent outputs overlap: {} and {}",
                        left_output.display(),
                        right_output.display()
                    );
                }
            }
        }
    }
}

#[test]
fn meson_runtime_reconfigures_disposable_state_before_reuse() {
    let helper = include_str!("stages/helpers/meson.rs");
    let reusable_tree = helper
        .find("if build_dir.join(\"build.ninja\").is_file()")
        .expect("Meson helper must distinguish an existing disposable build tree");
    let reconfigure = helper[reusable_tree..]
        .find("\"--reconfigure\"")
        .expect("existing Meson trees must be reconfigured before reuse");
    let compile = helper[reusable_tree..]
        .find("\"compile\"")
        .expect("Meson helper must compile after configuring");
    let install = helper[reusable_tree..]
        .find("\"install\"")
        .expect("Meson helper must install after compiling");
    assert!(reconfigure < compile);
    assert!(compile < install);
}

#[test]
fn iputils_reconfigures_its_disposable_meson_tree_before_installing() {
    let source = include_str!("stages/networking.rs");
    let start = source
        .find("fn build_iputils(")
        .expect("iputils recipe must remain available");
    let end = start
        + source[start..]
            .find("\nfn curl_configure_options")
            .expect("iputils recipe boundary must remain available");
    let body = &source[start..end];
    let reconfigure = body
        .find("\"--reconfigure\"")
        .expect("iputils must reconfigure an existing Meson tree");
    let install = body
        .find("remove_path_if_exists(&install_dir)")
        .expect("iputils must reset its staged install output after compiling");
    assert!(reconfigure < install);
}

#[test]
fn file_stage_disables_undeclared_libseccomp_instead_of_using_host_headers() {
    let helper = include_str!("stages/helpers/autotools.rs");
    let start = helper
        .find("fn build_file(")
        .expect("file recipe must remain available");
    let end = start
        + helper[start..]
            .find("\nfn build_less")
            .expect("file recipe boundary must remain available");
    assert!(helper[start..end].contains("--disable-libseccomp"));
}

#[test]
fn autotools_host_triplets_resolve_to_mattos_compiler_wrappers() {
    // Autoconf probes `<host>-gcc` on PATH; only the MattOS triplet has
    // target toolchain wrappers, so the generic triplet picks a host compiler.
    for source in [
        include_str!("stages/flatpak.rs"),
        include_str!("stages/networking.rs"),
        include_str!("stages/graphics.rs"),
        include_str!("stages/helpers/autotools.rs"),
    ] {
        assert!(!source.contains("\"--host=x86_64-linux-gnu\""));
    }
}

#[test]
fn gdk_pixbuf_declares_glibs_pcre2_link_requirement() {
    let stage = build_stage_spec(BuildStage::GdkPixbuf);
    assert!(stage.dependencies.contains(&"pcre2".to_string()));
    let source = include_str!("stages/flatpak.rs");
    let start = source.find("fn build_gdk_pixbuf(").unwrap();
    let end = start + source[start..].find("\nfn build_gpgme").unwrap();
    assert!(source[start..end].contains("\"pcre2\""));
}

#[test]
fn gstreamer_base_declares_glibs_pcre2_link_requirement() {
    let stage = build_stage_spec(BuildStage::GstreamerBase);
    assert!(stage.dependencies.contains(&"pcre2".to_string()));
    let source = include_str!("stages/graphics.rs");
    let start = source.find("fn build_gstreamer_base(").unwrap();
    let end = start
        + source[start..]
            .find("\nfn build_")
            .unwrap_or(source[start..].len());
    assert!(source[start..end].contains("\"pcre2\""));
}

#[test]
fn corrected_native_library_recipes_keep_required_inputs_and_disable_tests() {
    let libpng = build_stage_spec(BuildStage::Libpng);
    assert!(libpng.dependencies.contains(&"zlib".to_string()));

    let flatpak = include_str!("stages/flatpak.rs");
    let libfyaml_start = flatpak.find("fn build_libfyaml(").unwrap();
    let libfyaml_end = libfyaml_start
        + flatpak[libfyaml_start..]
            .find("\nfn build_libxmlb")
            .unwrap();
    let libfyaml = &flatpak[libfyaml_start..libfyaml_end];
    assert!(libfyaml.contains("-DBUILD_TESTING=OFF"));
    assert!(!libfyaml.contains("FYAML_BUILD_TESTS"));
    assert!(libfyaml.contains("LIBFYAML_RELEASE_ARCHIVE_SHA256"));
    assert!(libfyaml.contains("libfyaml-0.9.6/cmake/config.h.in"));

    let autotools = include_str!("stages/helpers/autotools.rs");
    let icu_start = autotools.find("if component == \"icu\"").unwrap();
    let icu_end = icu_start
        + autotools[icu_start..]
            .find("\n    if component == \"libcanberra\"")
            .unwrap();
    let icu = &autotools[icu_start..icu_end];
    assert!(icu.contains("root.join(\"license.html\")"));
    assert!(icu.contains("out_root.join(\"LICENSE\")"));

    let runtime = include_str!("stages/runtime_libraries.rs");
    let sndfile_start = runtime.find("fn build_libsndfile(").unwrap();
    let sndfile_end = sndfile_start
        + runtime[sndfile_start..]
            .find("\nfn build_libgudev")
            .unwrap();
    let sndfile = &runtime[sndfile_start..sndfile_end];
    assert!(sndfile.contains("LIBSNDFILE_RELEASE_ARCHIVE_SHA256"));
    assert!(sndfile.contains("libsndfile-1.2.2/include/sndfile.h"));
}

#[test]
fn non_qt_cmake_stages_do_not_require_the_qt_opengl_bridge() {
    let source = include_str!("stages/kde_foundation.rs");
    for function in ["build_plasma_wayland_protocols", "build_yaml_cpp"] {
        let start = source.find(&format!("fn {function}(")).unwrap();
        let end = start
            + source[start..]
                .find("\nfn ")
                .unwrap_or(source[start..].len());
        assert!(source[start..end].contains("build_non_qt_cmake"));
    }
    let helper_start = source.find("fn build_cmake_component(").unwrap();
    let helper_end = helper_start
        + source[helper_start..]
            .find("\nfn build_kcoreaddons")
            .unwrap();
    let helper = &source[helper_start..helper_end];
    assert!(helper.contains("if qt_integration"));
    assert!(helper.contains("isolated_target_cmake_args(repo_root, &prefixes)"));

    let protocols = build_stage_spec(BuildStage::PlasmaWaylandProtocols);
    assert_eq!(protocols.dependencies, vec!["formal-sysroot".to_string()]);
}

#[test]
fn wayland_protocols_uses_the_owned_native_scanner_closure() {
    let wayland = build_stage_spec(BuildStage::Wayland);
    assert!(wayland.dependencies.contains(&"expat".to_string()));

    let protocols = build_stage_spec(BuildStage::WaylandProtocols);
    for dependency in ["wayland", "expat", "libffi"] {
        assert!(protocols.dependencies.contains(&dependency.to_string()));
    }

    let source = include_str!("stages/kde_foundation.rs");
    let start = source.find("fn build_wayland_protocols(").unwrap();
    let end = start + source[start..].find("\nfn build_polkit_qt6").unwrap();
    let recipe = &source[start..end];
    assert!(
        recipe.contains(
            "staged_library_environment(repo_root, &[\"wayland\", \"expat\", \"libffi\"])"
        )
    );
    assert!(recipe.contains("usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml"));
    assert!(!recipe.contains("std::process::Command"));
}

#[test]
fn libarchive_uses_the_owned_libmd_dependency() {
    assert!(stage_graph::direct_dependencies(BuildStage::Libarchive).contains(&"libmd"));
    let recipe =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/stages/flatpak.rs"))
            .unwrap();
    assert!(
        recipe.contains("&[\"zlib\", \"zstd\", \"bzip2\", \"xz\", \"lz4\", \"libcap\", \"libmd\"]")
    );
}

#[test]
fn autotools_regeneration_uses_owned_inputs_for_cryptsetup_and_openssh() {
    let recipe = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/stages/helpers/autotools.rs"),
    )
    .unwrap();
    assert!(recipe.contains("component == \"openssh\""));
    assert!(recipe.contains("component == \"cryptsetup\""));
    assert!(recipe.contains("src/build-support/autoconf-archive/m4"));
    assert!(recipe.contains("src/system/libraries/glib/m4macros"));
    assert!(recipe.contains("src/system/security/gpgme/src"));
}

#[test]
fn xorg_autotools_regeneration_uses_declared_dependency_macros() {
    let recipe = include_str!("stages/graphics.rs");
    let start = recipe.find("fn build_xorg_autotools_component(").unwrap();
    let end = start + recipe[start..].find("\nfn build_x11_compat").unwrap();
    let helper = &recipe[start..end];
    assert!(helper.contains("for dependency in dependencies"));
    assert!(helper.contains("install/usr/share/aclocal"));
    assert!(helper.contains("std::env::join_paths(aclocal_paths)"));
}

#[test]
fn modemmanager_header_generation_has_no_host_python_module_dependency() {
    let recipe = include_str!("stages/plasma_apps.rs");
    let start = recipe.find("fn build_modemmanager(").unwrap();
    let end = start + recipe[start..].find("\nfn build_modemmanager_qt").unwrap();
    let modemmanager = &recipe[start..end];
    assert!(modemmanager.contains("import xml.etree.ElementTree as etree"));
    assert!(modemmanager.contains("header-generator.xsl"));
    assert!(!modemmanager.contains("from lxml"));
}

#[test]
fn breeze_icons_uses_a_pinned_output_owned_lxml() {
    let recipe = include_str!("stages/kde_foundation.rs");
    assert!(
        recipe.contains("component == \"breeze-icons\" && !python_deps.join(\"lxml\").is_dir()")
    );
    assert!(recipe.contains("\"lxml==6.0.2\""));
    assert!(
        recipe.contains("environment.push((\"PYTHONPATH\", python_deps.display().to_string()))")
    );
}

#[test]
fn kcoreaddons_declares_its_enabled_qml_dependency() {
    assert!(stage_graph::direct_dependencies(BuildStage::KCoreAddons).contains(&"qtdeclarative"));
    let recipe = include_str!("stages/kde_foundation.rs");
    let start = recipe.find("fn build_kcoreaddons(").unwrap();
    let end = start + recipe[start..].find("\nfn build_ki18n").unwrap();
    let kcoreaddons = &recipe[start..end];
    assert!(kcoreaddons.contains("\"qtdeclarative\""));
    assert!(kcoreaddons.contains("-DKCOREADDONS_USE_QML=ON"));
}

#[test]
fn kcompletion_disables_the_optional_qt_designer_plugin() {
    let recipe = include_str!("stages/kde_foundation.rs");
    let start = recipe.find("fn build_kcompletion(").unwrap();
    let end = start + recipe[start..].find("\nfn build_kcodecs").unwrap();
    assert!(recipe[start..end].contains("-DBUILD_DESIGNERPLUGIN=OFF"));
}

#[test]
fn ksystemstats_declares_its_intel_gpu_libdrm_dependency() {
    assert!(stage_graph::direct_dependencies(BuildStage::KSystemStats).contains(&"libdrm"));

    let recipe = include_str!("stages/plasma_apps.rs");
    let start = recipe.find("fn build_ksystemstats(").unwrap();
    let end = start
        + recipe[start..]
            .find("\nfn build_plasma_systemmonitor")
            .unwrap();
    assert!(recipe[start..end].contains("\"libdrm\""));
}

#[test]
fn plasma_desktop_materializes_xkeyboard_config_before_configure() {
    let recipe = include_str!("stages/plasma.rs");
    let start = recipe.find("fn build_plasma_component(").unwrap();
    let body = &recipe[start..recipe.find("fn build_plasma_framework").unwrap()];
    assert!(body.contains("stage == \"plasma-desktop\""));
    assert!(body.contains("build_xkeyboard_config(repo_root)?"));
    assert!(body.contains("\"xkeyboard-config\""));
}

#[test]
fn tzdata_build_supplies_ignored_license_in_disposable_tree() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging.find("fn stage_tzdata(").unwrap();
    let end = start + staging[start..].find("\n}\n\n").unwrap() + 2;
    let body = &staging[start..end];
    assert!(body.contains("build_source.join(\"LICENSE\")"));
    assert!(
        body.contains("fs::copy(source.join(\"README\"), &license)")
            || body.contains("fs::copy(source.join(\"README\"), &license)?")
    );
}

#[test]
fn linux_firmware_staging_handles_ignored_aggregate_license() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging.find("fn stage_linux_firmware(").unwrap();
    let end = start + staging[start..].find("\n}\n\n").unwrap() + 2;
    let body = &staging[start..end];
    assert!(body.contains("let license = source.join(\"LICENSE\")"));
    assert!(body.contains("documentation.join(\"LICENSE\")"));
    assert!(body.contains("source.join(\"README.md\")"));
}

#[test]
fn wireless_regdb_staging_handles_ignored_license() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging.find("pub(crate) fn stage_wireless_regdb(").unwrap();
    let end = start + staging[start..].find("\n}\n\n").unwrap() + 2;
    let body = &staging[start..end];
    assert!(body.contains("let license = source.join(\"LICENSE\")"));
    assert!(body.contains("source.join(\"README\")"));
}

#[test]
fn wireless_regdb_keeps_the_upstream_verification_key_in_the_import() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/system/data/wireless-regdb/wens.key.pub.pem");
    assert!(source.is_file());
}

#[test]
fn acl_uses_library_path_without_encoding_disposable_attr_runpath() {
    let source = include_str!("stages/foundation_libraries.rs");
    let start = source.find("fn build_acl(").unwrap();
    let end = start
        + source[start..]
            .find("\nfn ensure_acl_release_archive")
            .unwrap();
    let body = &source[start..end];
    assert!(body.contains("acl-link-isolation-v2"));
    assert!(body.contains("LIBRARY_PATH"));
    assert!(body.contains("libtool configuration"));
    assert!(!body.contains("\"LDFLAGS\""));
}

#[test]
fn libffi_package_uses_retained_license_notice() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging
        .find("\"libffi8\" => stage_imported_soname_library")
        .unwrap();
    let end = start + staging[start..].find("\n        )?,").unwrap();
    assert!(staging[start..end].contains("libffi/libffi/README.md"));
    assert!(staging.contains("usr/share/doc/libffi-dev/copyright"));
}

#[test]
fn highway_package_uses_retained_bsd_license_notice() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging.find("\"libhwy1\" => stage_multimedia_sdk").unwrap();
    let end = start + staging[start..].find("\n        )?,").unwrap();
    assert!(staging[start..end].contains("highway/LICENSE-BSD3"));
}

#[test]
fn pulseaudio_package_uses_retained_lgpl_license_notice() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging
        .find("\"libpulse0\" => stage_multimedia_sdk")
        .unwrap();
    let end = start + staging[start..].find("\n        )?,").unwrap();
    assert!(staging[start..end].contains("pulseaudio/LGPL"));
}

#[test]
fn wireplumber_package_uses_retained_license_notice() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging
        .find("\"wireplumber\" => stage_multimedia_sdk")
        .unwrap();
    let end = start + staging[start..].find("\n        )?,").unwrap();
    assert!(staging[start..end].contains("wireplumber/README.rst"));
}

#[test]
fn lcms2_package_uses_retained_license_notice() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging
        .find("\"liblcms2-2\" => stage_imported_soname_library")
        .unwrap();
    let end = start + staging[start..].find("\n        )?,").unwrap();
    assert!(staging[start..end].contains("lcms2/README.md"));
}

#[test]
fn zxing_package_uses_retained_license_notice() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging
        .find("\"libzxing4\" => stage_multimedia_sdk")
        .unwrap();
    let end = start + staging[start..].find("\n        )?,").unwrap();
    assert!(staging[start..end].contains("zxing-cpp/README.md"));
}

#[test]
fn yaml_cpp_package_uses_retained_license_notice() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging.find("fn stage_kde_module(").unwrap();
    let end = start + staging[start..].find("fn stage_multimedia_sdk(").unwrap();
    assert!(staging[start..end].contains("component == \"yaml-cpp\""));
    assert!(staging[start..end].contains("README.md"));
}

#[test]
fn xkbcommon_package_uses_retained_license_notice() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging.find("\"libxkbcommon0\" => {").unwrap();
    let end = start
        + staging[start..]
            .find("\n        }\n        \"libxml2-16\"")
            .unwrap();
    assert!(staging[start..end].contains("xkbcommon/README.md"));
}

#[test]
fn seatd_package_uses_retained_license_notice() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging
        .find("\"libseat1\" => stage_imported_soname_library")
        .unwrap();
    let end = start + staging[start..].find("\n        )?,").unwrap();
    assert!(staging[start..end].contains("seatd/README.md"));
}

#[test]
fn display_info_package_uses_retained_license_notice() {
    let staging = include_str!("packaging/staging.rs");
    let start = staging
        .find("\"libdisplay-info3\" => stage_imported_soname_library")
        .unwrap();
    let end = start + staging[start..].find("\n        )?,").unwrap();
    assert!(staging[start..end].contains("libdisplay-info/README.md"));
}

#[test]
fn mesa_remaps_rust_source_paths_in_disposable_builds() {
    let graphics = include_str!("stages/graphics.rs");
    let start = graphics.find("fn build_mesa(").unwrap();
    assert!(graphics[start..].contains("RUSTFLAGS"));
    assert!(graphics[start..].contains("remap-path-prefix"));
}

#[test]
fn systemd_does_not_sysroot_output_owned_pkgconfig_paths() {
    let recipe = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/stages/system_runtime.rs"),
    )
    .unwrap();
    let systemd_recipe = recipe
        .split("fn patch_systemd_osc_profile_for_posix_login_shell")
        .next()
        .unwrap();
    assert!(!systemd_recipe.contains("PKG_CONFIG_SYSROOT_DIR"));
}

#[test]
fn cmake_runtime_reconfigures_stale_host_discovery_from_target_prefixes() {
    let source = include_str!("stages/graphics.rs");
    let start = source.find("fn build_vulkan_cmake(").unwrap();
    let end = start + source[start..].find("\nfn build_vulkan_headers(").unwrap();
    let helper = &source[start..end];
    assert!(helper.contains("cmake-prefix-path={cmake_prefix_path}"));
    assert!(helper.contains("-DCMAKE_PREFIX_PATH={cmake_prefix_path}"));
    assert!(helper.contains("CMAKE_FIND_USE_SYSTEM_PACKAGE_REGISTRY=OFF"));
    assert!(helper.contains("release-sha256={}"));
    assert!(helper.contains("release archive did not produce output-mirror file"));
}

#[test]
fn mattos_cross_linker_stays_native_mode_for_dt_needed_search() {
    // A cross-mode ld ignores LD_LIBRARY_PATH/DT_RUNPATH when resolving
    // DT_NEEDED; MattOS recipes (e.g. libgcrypt's tests) rely on them.
    let source = include_str!("stages/toolchain.rs");
    let cross = &source[source.find("fn build_cross_toolchain").unwrap()..];
    let cross = &cross[..cross.find("\nfn ").unwrap()];
    let binutils = &cross[cross.find("let binutils_args").unwrap()..];
    let binutils = &binutils[..binutils.find("];").unwrap()];
    assert!(binutils.contains("native_host.as_str()"));
    assert!(binutils.contains("program_prefix.as_str()"));
    assert!(binutils.contains("--enable-new-dtags"));
    assert!(!binutils.contains("host_triplet"));
}

#[test]
fn toolchain_wrappers_do_not_depend_on_how_the_working_directory_is_spelled() {
    let temporary = tempfile::tempdir().unwrap();
    let repo = temporary.path();
    crate::install_fake_mattos_target_toolchain(repo);
    fs::create_dir_all(repo.join("src/tools/mattos-build")).unwrap();
    fs::write(repo.join("src/tools/mattos-build/Cargo.toml"), "").unwrap();
    fs::create_dir_all(repo.join("out/sysroot/usr/include")).unwrap();
    fs::write(repo.join("out/sysroot/usr/include/stdio.h"), "").unwrap();
    let wrapper = repo.join("out/toolchain/bin/gcc");
    let mut command = std::process::Command::new("true");
    crate::apply_mattos_sysroot_environment(&mut command, repo, "make", &[]).unwrap();
    let direct = fs::read_to_string(&wrapper).unwrap();
    let key = crate::target_toolchain_cache_identity(repo, "zlib").unwrap();
    // As a test run from the crate directory spells the checkout.
    let dotted = repo.join("src/tools/mattos-build/../../..");
    let mut command = std::process::Command::new("true");
    crate::apply_mattos_sysroot_environment(&mut command, &dotted, "make", &[]).unwrap();
    assert_eq!(fs::read_to_string(&wrapper).unwrap(), direct);
    assert!(!direct.contains("/../"));
    assert_eq!(crate::target_toolchain_cache_identity(repo, "zlib").unwrap(), key);
}

#[test]
fn mattos_toolchain_path_preserves_the_last_path_override() {
    let temporary = tempfile::tempdir().unwrap();
    let repo = temporary.path();
    crate::install_fake_mattos_target_toolchain(repo);
    fs::create_dir_all(repo.join("src/tools/mattos-build")).unwrap();
    fs::write(repo.join("src/tools/mattos-build/Cargo.toml"), "").unwrap();
    fs::create_dir_all(repo.join("out/sysroot/usr/include")).unwrap();
    fs::write(repo.join("out/sysroot/usr/include/stdio.h"), "").unwrap();
    let cwd = repo.join("out/build/example");
    fs::create_dir_all(&cwd).unwrap();
    let mut command = std::process::Command::new("true");
    crate::apply_mattos_sysroot_environment(
        &mut command,
        &cwd,
        "meson",
        &[
            ("PATH", "/usr/bin".to_string()),
            ("PATH", "/recipe/shims:/usr/bin".to_string()),
        ],
    )
    .unwrap();
    let path = command
        .get_envs()
        .find(|(key, _)| *key == std::ffi::OsStr::new("PATH"))
        .and_then(|(_, value)| value)
        .unwrap()
        .to_owned();
    let entries = std::env::split_paths(&path).collect::<Vec<_>>();
    assert_eq!(entries[1], std::path::PathBuf::from("/recipe/shims"));
}

#[test]
fn stage_workspace_from_another_target_toolchain_is_discarded() {
    let temporary = tempfile::tempdir().unwrap();
    let repo = temporary.path();
    // No toolchain yet: nothing is touched.
    let stale = repo.join("out/build/zlib/build/object.o");
    fs::create_dir_all(stale.parent().unwrap()).unwrap();
    fs::write(&stale, "host-compiled").unwrap();
    assert!(crate::prepare_stage_workspace_for_toolchain(repo, BuildStage::Zlib)
        .unwrap()
        .is_none());
    assert!(stale.is_file());

    for relative in [
        "out/build/cross-toolchain/configure-invocation.txt",
        "out/build/cross-toolchain/mattos-hardening.specs",
        "out/build/gcc-runtime/configure-invocation.txt",
    ] {
        fs::create_dir_all(repo.join(relative).parent().unwrap()).unwrap();
        fs::write(repo.join(relative), relative).unwrap();
    }
    // Unmarked (pre-existing) workspace is discarded.
    let (marker, identity) =
        crate::prepare_stage_workspace_for_toolchain(repo, BuildStage::Zlib)
            .unwrap()
            .unwrap();
    assert!(!stale.exists());
    // Markers stay out of the workspace: rootfs publishes out/build/rootfs.
    let workspace = repo.join("out/build/zlib");
    assert!(!marker.starts_with(&workspace));
    // A workspace marked with the current toolchain is kept, and a legacy
    // in-workspace marker is migrated rather than treated as unmarked.
    fs::create_dir_all(&workspace).unwrap();
    fs::write(workspace.join(".mattos-target-toolchain"), &identity).unwrap();
    fs::write(&workspace.join("keep"), "").unwrap();
    crate::prepare_stage_workspace_for_toolchain(repo, BuildStage::Zlib).unwrap();
    assert!(workspace.join("keep").is_file());
    assert!(!workspace.join(".mattos-target-toolchain").exists());
    assert_eq!(fs::read_to_string(&marker).unwrap(), identity);
    crate::prepare_stage_workspace_for_toolchain(repo, BuildStage::Zlib).unwrap();
    assert!(workspace.join("keep").is_file());
    // A compiler change discards it again.
    fs::write(repo.join("out/build/gcc-runtime/configure-invocation.txt"), "new gcc").unwrap();
    crate::prepare_stage_workspace_for_toolchain(repo, BuildStage::Zlib).unwrap();
    assert!(!workspace.join("keep").exists());
    // Toolchain stages manage their own workspaces.
    assert!(crate::prepare_stage_workspace_for_toolchain(repo, BuildStage::Glibc)
        .unwrap()
        .is_none());
}

fn bare_stage_spec(id: &str) -> crate::cache_manifest::StageSpec {
    crate::cache_manifest::StageSpec {
        id: id.to_string(),
        source_inputs: Vec::new(),
        configuration_inputs: Vec::new(),
        tools: Vec::new(),
        dependencies: Vec::new(),
        outputs: Vec::new(),
        recipe: format!("mattos-build-stage:{id}:schema={STAGE_MANIFEST_SCHEMA_VERSION}"),
    }
}

fn install_fake_target_toolchain(repo: &std::path::Path) {
    for relative in [
        "out/build/cross-toolchain/configure-invocation.txt",
        "out/build/cross-toolchain/mattos-hardening.specs",
        "out/build/gcc-runtime/configure-invocation.txt",
    ] {
        fs::create_dir_all(repo.join(relative).parent().unwrap()).unwrap();
        fs::write(repo.join(relative), relative).unwrap();
    }
    fs::create_dir_all(repo.join("out/toolchain/bin")).unwrap();
    fs::write(repo.join("out/toolchain/bin/gcc"), "#!/bin/sh\nexec real-gcc \"$@\"\n").unwrap();
}

#[test]
fn target_toolchain_is_part_of_target_stage_cache_keys() {
    let temporary = tempfile::tempdir().unwrap();
    let repo = temporary.path();
    // Before the toolchain exists there is nothing to key on.
    assert_eq!(crate::target_toolchain_cache_identity(repo, "zlib").unwrap(), None);
    install_fake_target_toolchain(repo);
    let first = crate::target_toolchain_cache_identity(repo, "zlib").unwrap().unwrap();
    // The toolchain's own stages are not compiled by it.
    // Exemptions are matched by real stage identifiers (the kernel is `linux`).
    for stage in [BuildStage::Kernel, BuildStage::Glibc, BuildStage::GccRuntime, BuildStage::CrossToolchain] {
        let stage = crate::build_stage_id(stage);
        assert_eq!(crate::target_toolchain_cache_identity(repo, stage).unwrap(), None);
    }
    for stage in ["formal-sysroot", "packages", "repository"] {
        assert_eq!(crate::target_toolchain_cache_identity(repo, stage).unwrap(), None);
    }
    // A wrapper change alone (sysroot bytes untouched) changes the key.
    fs::write(repo.join("out/toolchain/bin/gcc"), "#!/bin/sh\nexec real-gcc -O3 \"$@\"\n").unwrap();
    let wrapper_changed = crate::target_toolchain_cache_identity(repo, "zlib").unwrap().unwrap();
    assert_ne!(first, wrapper_changed);
    // Unrelated files beside the wrappers do not.
    fs::write(repo.join("out/toolchain/bin/stray-helper"), "unrelated").unwrap();
    assert_eq!(
        crate::target_toolchain_cache_identity(repo, "zlib").unwrap().unwrap(),
        wrapper_changed
    );

    // It is recorded as a dependency of every target stage evaluation.
    let evaluation = crate::performance::compute_stage_evaluation(repo, &bare_stage_spec("zlib")).unwrap();
    assert_eq!(
        evaluation.inputs.dependency_digests.get(crate::TARGET_TOOLCHAIN_CACHE_KEY),
        Some(&wrapper_changed)
    );
}

#[test]
fn target_toolchain_key_does_not_depend_on_the_checkout_location() {
    let identities = ["first", "second"].map(|name| {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path().join(name);
        install_fake_target_toolchain(&repo);
        let checkout = repo.display().to_string();
        fs::write(
            repo.join("out/toolchain/bin/gcc"),
            format!("#!/bin/sh\nexec {checkout}/out/build/gcc-runtime/toolchain/usr/bin/gcc \"$@\"\n"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            format!("{checkout}/out/build/cross-toolchain/install/bin/x86_64-pc-linux-gnu-ld"),
            repo.join("out/toolchain/bin/ld"),
        )
        .unwrap();
        crate::target_toolchain_cache_identity(&repo, "zlib").unwrap().unwrap()
    });
    assert_eq!(identities[0], identities[1]);
}

#[test]
fn pre_toolchain_key_manifests_migrate_only_with_a_matching_workspace_marker() {
    let temporary = tempfile::tempdir().unwrap();
    let repo = temporary.path();
    install_fake_target_toolchain(repo);
    let current = crate::performance::compute_stage_evaluation(repo, &bare_stage_spec("zlib")).unwrap();
    // A manifest written before the key existed: identical except for it.
    let (mut inputs, mut details) = (current.inputs.clone(), current.details.clone());
    inputs.dependency_digests.remove(crate::TARGET_TOOLCHAIN_CACHE_KEY);
    details.dependencies.remove(crate::TARGET_TOOLCHAIN_CACHE_KEY);
    let manifest = StageManifest {
        schema_version: STAGE_MANIFEST_SCHEMA_VERSION,
        stage: "zlib".to_string(),
        inputs,
        input_details: details,
        expected_outputs: Vec::new(),
        output_content_digest: String::new(),
    };
    let migrate = || crate::performance::can_migrate_narrowed_manifest(repo, &current, &manifest).unwrap();
    // No evidence of which toolchain built it: rebuild.
    assert!(!migrate());
    // The workspace guard recorded a different toolchain: rebuild.
    let (marker, identity) = crate::prepare_stage_workspace_for_toolchain(repo, BuildStage::Zlib)
        .unwrap()
        .unwrap();
    fs::create_dir_all(marker.parent().unwrap()).unwrap();
    fs::write(&marker, "another toolchain").unwrap();
    assert!(!migrate());
    // Built by the current toolchain: adopt the key without rebuilding.
    fs::write(&marker, &identity).unwrap();
    assert!(migrate());
    // A manifest that recorded a *different* toolchain key never migrates.
    let mut stale = manifest.clone();
    stale
        .inputs
        .dependency_digests
        .insert(crate::TARGET_TOOLCHAIN_CACHE_KEY.to_string(), "old".to_string());
    assert!(!crate::performance::can_migrate_narrowed_manifest(repo, &current, &stale).unwrap());
    // A key from an earlier definition is replaced like a missing one.
    let mut earlier = manifest.clone();
    earlier
        .inputs
        .dependency_digests
        .insert("<target-toolchain>".to_string(), "v1".to_string());
    assert!(crate::performance::can_migrate_narrowed_manifest(repo, &current, &earlier).unwrap());
}

#[test]
fn meson_stages_with_rust_code_depend_on_and_reconfigure_for_the_mattos_rustc() {
    // Meson records the Rust compiler at configure time; these recipes bind
    // their build directories to the rust stage output as well as ordering.
    for stage in [BuildStage::DbusBroker, BuildStage::Gstreamer, BuildStage::Mesa] {
        assert!(build_stage_spec(stage).dependencies.contains(&"rust".to_string()), "{stage:?}");
    }
    let graphics = include_str!("stages/graphics.rs");
    for recipe in ["fn build_mesa(", "fn build_gstreamer("] {
        let start = graphics.find(recipe).unwrap();
        let body = &graphics[start..start + graphics[start..].find("\n}\n").unwrap()];
        assert!(body.contains("\"rust\""), "{recipe} does not bind its Meson directory to rust");
    }
    assert!(include_str!("stages/system_runtime.rs").contains("[\"expat\", \"systemd\", \"rust\"]"));
}

#[test]
fn shared_command_infrastructure_is_not_an_image_stage_input() {
    // Editing run_cmd and the build environment must not rebuild rootfs,
    // live-root, initramfs and the ISO (about six minutes) by itself.
    let helpers = PathBuf::from("src/tools/mattos-build/src/stages/helpers/command.rs");
    for stage in crate::stage_graph::build_plan(BuildStage::All) {
        assert!(!crate::stage_inputs::source_inputs(stage).contains(&helpers), "{stage:?}");
    }
    let image = include_str!("stages/image.rs");
    for generic in ["fn run_cmd(", "fn apply_mattos_sysroot_environment(", "fn apply_ccache_environment("] {
        assert!(!image.contains(generic), "{generic} moved back into image.rs");
    }
}

#[test]
fn iso_carries_the_package_repository_and_links_large_payloads() {
    assert!(build_stage_spec(BuildStage::Iso).dependencies.contains(&"repository".to_string()));
    let image = include_str!("stages/image.rs");
    let start = image.find("fn build_iso_atomic(").unwrap();
    let body = &image[start..start + image[start..].find("\n}\n").unwrap()];
    assert!(body.contains("link_or_copy(&live_root"));
    assert!(body.contains("link_tree(&repository"));
    assert!(!body.contains("fs::copy(&live_root"));
}

#[test]
fn link_tree_hard_links_files_and_preserves_layout() {
    use std::os::unix::fs::MetadataExt;
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("repository");
    fs::create_dir_all(source.join("dists/trixie")).unwrap();
    fs::write(source.join("dists/trixie/Release"), "release").unwrap();
    std::os::unix::fs::symlink("dists", source.join("alias")).unwrap();
    let destination = temporary.path().join("iso/mattos/repository");
    crate::link_tree(&source, &destination).unwrap();
    let copied = destination.join("dists/trixie/Release");
    assert_eq!(fs::read_to_string(&copied).unwrap(), "release");
    assert_eq!(
        fs::metadata(&copied).unwrap().ino(),
        fs::metadata(source.join("dists/trixie/Release")).unwrap().ino()
    );
    assert_eq!(fs::read_link(destination.join("alias")).unwrap(), PathBuf::from("dists"));
}

#[test]
fn text_login_pam_stacks_load_the_system_locale() {
    let line = "session    required     pam_env.so readenv=1 envfile=/etc/default/locale";
    for (stack, body) in [
        ("login", include_str!("../../../system/auth/config/pam.d/login")),
        ("su-l", include_str!("../../../system/auth/config/pam.d/su-l")),
        ("sshd", include_str!("../../../system/network/openssh/ssh-pam")),
    ] {
        assert!(body.lines().any(|candidate| candidate == line), "{stack} lacks the locale envfile");
    }
    let image = include_str!("stages/image.rs");
    assert!(image.contains("symlink(\"../locale.conf\", &default_locale)"));
}

#[test]
fn jobserver_clients_drop_fixed_job_counts_but_keep_serial_requests() {
    let strip = |args: &[&str]| crate::strip_job_count_arguments(args);
    assert_eq!(strip(&["-C", "src", "-j", "4", "all"]), ["-C", "src", "all"]);
    assert_eq!(strip(&["-j4", "install"]), ["install"]);
    assert_eq!(strip(&["--build", ".", "--parallel", "8"]), ["--build", "."]);
    assert_eq!(strip(&["--jobs=6", "x"]), ["x"]);
    // Explicit serialization is recipe semantics (e.g. lvm2's racy install).
    assert_eq!(strip(&["-j1", "install_device-mapper"]), ["-j1", "install_device-mapper"]);
    assert_eq!(strip(&["-j", "1", "t"]), ["-j", "1", "t"]);
    // Non-count uses are untouched.
    assert_eq!(strip(&["-j", "--foo", "-jobs"]), ["-j", "--foo", "-jobs"]);
}

#[test]
fn llvm_and_rust_recipes_are_separate_cache_inputs() {
    let llvm = crate::stage_inputs::source_inputs(BuildStage::Llvm);
    let rust = crate::stage_inputs::source_inputs(BuildStage::Rust);
    let has = |inputs: &[std::path::PathBuf], file: &str| {
        inputs.iter().any(|path| path.ends_with(file))
    };
    assert!(has(&llvm, "llvm_toolchain.rs") && !has(&llvm, "rust_toolchain.rs"));
    assert!(has(&rust, "rust_toolchain.rs") && !has(&rust, "llvm_toolchain.rs"));
    assert!(!has(&llvm, "runtime_tooling.rs") && !has(&rust, "runtime_tooling.rs"));
}

#[test]
fn ccache_masquerades_every_compiler_name_only_once_a_toolchain_exists() {
    let temporary = tempfile::tempdir().unwrap();
    let repo = temporary.path();
    let ccache = repo.join("host/ccache");
    fs::create_dir_all(ccache.parent().unwrap()).unwrap();
    fs::write(&ccache, "").unwrap();
    // No MattOS toolchain identity yet: no masquerade.
    assert!(crate::ccache_masquerade_for(repo, &ccache).unwrap().is_none());
    for relative in [
        "out/build/cross-toolchain/configure-invocation.txt",
        "out/build/cross-toolchain/mattos-hardening.specs",
        "out/build/gcc-runtime/configure-invocation.txt",
    ] {
        fs::create_dir_all(repo.join(relative).parent().unwrap()).unwrap();
        fs::write(repo.join(relative), relative).unwrap();
    }
    let directory = crate::ccache_masquerade_for(repo, &ccache).unwrap().unwrap();
    for name in ["gcc", "g++", "cc", "c++", "x86_64-pc-linux-gnu-gcc", "x86_64-pc-linux-gnu-g++"] {
        assert_eq!(fs::read_link(directory.join(name)).unwrap(), ccache, "{name}");
    }
    // The cache is keyed to the toolchain identity, not the wrapper script.
    let mut command = std::process::Command::new("true");
    crate::apply_ccache_environment(&mut command, repo).unwrap();
    let check = command
        .get_envs()
        .find(|(key, _)| *key == std::ffi::OsStr::new("CCACHE_COMPILERCHECK"))
        .and_then(|(_, value)| value)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let identity = crate::target_toolchain_identity(repo).unwrap().unwrap();
    assert_eq!(check, format!("string:{identity}"));
}

#[test]
fn cmake_configure_commands_get_the_ccache_launcher_but_builds_do_not() {
    let temporary = tempfile::tempdir().unwrap();
    let repo = temporary.path();
    for relative in [
        "out/build/cross-toolchain/configure-invocation.txt",
        "out/build/cross-toolchain/mattos-hardening.specs",
        "out/build/gcc-runtime/configure-invocation.txt",
    ] {
        fs::create_dir_all(repo.join(relative).parent().unwrap()).unwrap();
        fs::write(repo.join(relative), relative).unwrap();
    }
    let has_launcher = |args: &[&str]| {
        let mut command = std::process::Command::new("cmake");
        command.args(args);
        crate::add_cmake_ccache_launcher(&mut command, "cmake", repo).unwrap();
        command
            .get_args()
            .any(|argument| argument.to_string_lossy().starts_with("-DCMAKE_CXX_COMPILER_LAUNCHER="))
    };
    // Only asserts wiring when a host ccache exists on this machine.
    if crate::host_ccache().is_some() {
        assert!(has_launcher(&["-S", "src", "-B", "build", "-G", "Ninja"]));
    }
    assert!(!has_launcher(&["--build", "build"]));
    assert!(!has_launcher(&["--install", "build"]));
}

#[test]
fn temporary_suffixes_are_unique_across_threads_of_one_process() {
    let suffixes = std::thread::scope(|scope| {
        (0..8)
            .map(|_| scope.spawn(crate::performance::unique_temporary_suffix))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<BTreeSet<_>>()
    });
    assert_eq!(suffixes.len(), 8);
    let pid = std::process::id().to_string();
    assert!(suffixes.iter().all(|suffix| suffix.starts_with(&format!("{pid}-"))));
}

#[test]
fn rootfs_audit_rejects_objects_from_any_non_mattos_compiler() {
    let expected = [
        "GCC: (GNU) 15.3.0".to_string(),
        "rustc version 1.97.1 (8bab26f4f 2026-07-14) (built from a source tarball)".to_string(),
        "clang version 22.1.8 (https://github.com/llvm/llvm-project.git ca7933e)".to_string(),
    ];
    let check = |comment: &[u8]| crate::foreign_compiler_signature_in_comment(comment, &expected);
    let mattos = b"GCC: (GNU) 15.3.0\0rustc version 1.97.1 (8bab26f4f 2026-07-14) (built from a source tarball)\0\
clang version 22.1.8 (https://github.com/llvm/llvm-project.git ca7933e)\0";
    assert_eq!(check(mattos), None);
    assert_eq!(
        check(b"GCC: (GNU) 15.3.0\0GCC: (Ubuntu 15.2.0-16ubuntu1) 15.2.0\0").as_deref(),
        Some("GCC: (Ubuntu 15.2.0-16ubuntu1) 15.2.0")
    );
    // Host rustc, and the Clang-built objects in upstream's prebuilt std.
    assert_eq!(
        check(b"GCC: (GNU) 15.3.0\0rustc version 1.94.0 (4a4ef493e 2026-03-02)\0").as_deref(),
        Some("rustc version 1.94.0 (4a4ef493e 2026-03-02)")
    );
    assert_eq!(
        check(b"clang version 21.1.0-rc2\0").as_deref(),
        Some("clang version 21.1.0-rc2")
    );
    // Other producers (linkers) are not compiler identifications.
    assert_eq!(check(b"Linker: LLD 22.1.8\0"), None);
}

#[test]
fn elf_comment_section_is_read_from_section_headers_only() {
    // This test binary carries a `.comment` naming the compiler that built it.
    let executable = fs::read(std::env::current_exe().unwrap()).unwrap();
    let comment = crate::elf_comment_section(&executable).unwrap();
    assert!(memchr::memmem::find(comment, b"rustc version ").is_some());
    // Data that merely contains a compiler string is not a `.comment` entry.
    assert_eq!(crate::elf_comment_section(b"clang version 21\0"), None);
}

#[test]
fn package_cache_key_follows_its_own_producer_stage_install_tree() {
    let temporary = tempfile::tempdir().unwrap();
    let repo = temporary.path();
    // A component with no explicit package-stage mapping.
    let binary = repo.join("out/build/unmapped-component/install/usr/bin/tool");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, "built by the host compiler").unwrap();
    let before =
        crate::packaging::package_stage_dependency_digest(repo, "unmapped-component").unwrap();
    fs::write(&binary, "built by the MattOS compiler").unwrap();
    let after =
        crate::packaging::package_stage_dependency_digest(repo, "unmapped-component").unwrap();
    assert_ne!(before, after, "rebuilt payload must invalidate the package cache");
}
