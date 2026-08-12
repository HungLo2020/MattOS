use super::*;
use crate::cache_manifest::{StageInputDetails, StageInputs, StageManifest, STAGE_MANIFEST_SCHEMA_VERSION};
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
                    performance::compute_stage_inputs(root, spec).unwrap().full_digest,
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
    write_file(&root.join("src/kernel/linux/include/uapi/linux/test.h"), "uapi two\n");
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
    let first = performance::compute_stage_inputs(root, &brush).unwrap();
    write_file(&tool, "#!/bin/sh\necho version-two\n");
    let second = performance::compute_stage_inputs(root, &brush).unwrap();
    assert_ne!(first.tool_digest, second.tool_digest);

    let mut revised = brush.clone();
    revised.recipe.push_str(":revision-two");
    let third = performance::compute_stage_inputs(root, &revised).unwrap();
    assert_ne!(second.full_digest, third.full_digest);

    let mut initramfs = build_stage_spec(BuildStage::Initramfs);
    initramfs.dependencies.clear();
    write_file(&root.join("src/boot/live-init.c"), "int main(void) { return 0; }\n");
    let initramfs_before = performance::compute_stage_inputs(root, &initramfs).unwrap();
    initramfs.recipe.push_str(":revision-two");
    let initramfs_after = performance::compute_stage_inputs(root, &initramfs).unwrap();
    assert_ne!(initramfs_before.full_digest, initramfs_after.full_digest);

    publish_dependency(root, "formal-sysroot", "input-one", "same-sysroot-bytes");
    let initramfs = build_stage_spec(BuildStage::Initramfs);
    let before = performance::compute_stage_inputs(root, &initramfs).unwrap();
    publish_dependency(root, "formal-sysroot", "input-two", "same-sysroot-bytes");
    let identical = performance::compute_stage_inputs(root, &initramfs).unwrap();
    assert_eq!(before.full_digest, identical.full_digest);
    publish_dependency(root, "formal-sysroot", "input-three", "changed-sysroot-bytes");
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
    assert!(rootfs.configuration_inputs.contains(&PathBuf::from("out/packages/inventory.toml")));
    assert!(rootfs.dependencies.contains(&"repository".to_string()));

    let initramfs = build_stage_spec(BuildStage::Initramfs);
    assert!(initramfs.configuration_inputs.is_empty());
    assert_eq!(initramfs.dependencies, ["formal-sysroot"]);
    assert_eq!(initramfs.source_inputs, [PathBuf::from("src/boot/live-init.c")]);
    assert_eq!(initramfs.outputs, [PathBuf::from("out/build/early-initramfs.cpio.xz")]);
    assert_eq!(initramfs.tools, ["gcc", "cpio", "xz"]);

    let live_root = build_stage_spec(BuildStage::LiveRoot);
    assert_eq!(live_root.dependencies, ["rootfs"]);
    assert!(live_root.outputs.contains(&PathBuf::from("out/build/live-root.squashfs")));
    assert_eq!(live_root.tools, ["mksquashfs", "unsquashfs"]);

    let iso = build_stage_spec(BuildStage::Iso);
    assert_eq!(iso.source_inputs, [PathBuf::from(AUTHORITATIVE_GRUB_CFG)]);
    assert!(iso.dependencies.contains(&"linux".to_string()));
    assert!(iso.dependencies.contains(&"live-root".to_string()));
    assert!(iso.dependencies.contains(&"initramfs".to_string()));
}

#[test]
fn cold_build_concurrency_groups_preserve_barriers_and_output_ownership() {
    let graph = crate::stage_graph::dependency_map();
    assert!(graph["linux"].is_empty());
    assert!(graph["glibc"].is_empty());
    assert_eq!(graph["gcc-runtime"], ["glibc", "linux-headers"].into_iter().collect());
    assert_eq!(graph["binutils"], ["gcc-runtime"].into_iter().collect());
    assert_eq!(
        graph["gcc-compiler"],
        ["binutils", "gcc-runtime"].into_iter().collect()
    );
    assert_eq!(
        graph["formal-sysroot"],
        ["gcc-runtime", "glibc", "linux-headers"].into_iter().collect()
    );
    assert_eq!(graph["repository"], ["packages"].into_iter().collect());
    assert!(graph["rootfs"].contains("repository"));
    assert_eq!(graph["live-root"], ["rootfs"].into_iter().collect());
    assert_eq!(graph["initramfs"], ["formal-sysroot"].into_iter().collect());
    assert_eq!(graph["iso"], ["initramfs", "linux", "live-root"].into_iter().collect());

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
