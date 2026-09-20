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
    assert!(graph["linux"].is_empty());
    assert!(graph["glibc"].is_empty());
    assert_eq!(
        graph["gcc-runtime"],
        ["glibc", "linux-headers"].into_iter().collect()
    );
    assert_eq!(graph["binutils"], ["gcc-runtime"].into_iter().collect());
    assert_eq!(
        graph["gcc-compiler"],
        ["binutils", "gcc-runtime"].into_iter().collect()
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
        ["grub", "initramfs", "linux", "live-root"]
            .into_iter()
            .collect()
    );

    let independent_after_sysroot = [
        BuildStage::Brush,
        BuildStage::Coreutils,
        BuildStage::Grep,
        BuildStage::Sed,
        BuildStage::Findutils,
        BuildStage::Diffutils,
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
        BuildStage::Init,
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
    assert!(helper.contains("isolated_target_cmake_args(&prefixes)"));

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
