use super::cache::PackageSetEntry;
use super::repository::{
    dependency_name, exact_dependency_version, validate_release_sha256, validate_repository,
    validate_repository_packages,
};
use super::*;

#[test]
fn live_root_leaves_out_only_real_development_packages_nothing_depends_on() {
    for name in live_excluded_packages() {
        assert!(
            PACKAGE_NAMES.contains(&name),
            "{name} is not a MattOS package"
        );
    }
    // Whatever the live root leaves out, installed systems get.
    let toolchain = package_specs()
        .into_iter()
        .find(|spec| spec.name == MATTOS_TOOLCHAIN_META_PACKAGE)
        .unwrap();
    assert_eq!(toolchain.depends, MATTOS_TOOLCHAIN_PACKAGES);
    // Runtime libraries the desktop links against stay in the live root.
    let live = live_package_names();
    for runtime in [
        "libllvm22",
        "libstdc++6",
        "libgcc-s1",
        "libpython3.14",
        "python3",
    ] {
        assert!(live.contains(&runtime), "{runtime} left the live root");
    }
    // Ordering the live subset fails if any live package depends on an
    // excluded one.
    let order = live_package_install_order().unwrap();
    assert_eq!(
        order.len(),
        PACKAGE_NAMES.len() - live_excluded_packages().len()
    );
    assert!(
        order
            .iter()
            .all(|name| !live_excluded_packages().contains(name))
    );
}

#[test]
fn parallel_package_work_keeps_input_order_and_reports_failures() {
    let started = std::sync::Mutex::new(Vec::new());
    let results = parallel_in_order((0..32).collect(), 4, |item: usize| {
        started.lock().unwrap().push(std::thread::current().id());
        std::thread::sleep(std::time::Duration::from_millis((32 - item as u64) % 5));
        Ok(item * 2)
    })
    .unwrap();
    assert_eq!(results, (0..32).map(|item| item * 2).collect::<Vec<_>>());
    let threads = started
        .into_inner()
        .unwrap()
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    assert!(threads.len() > 1, "work ran on one thread");

    let error = parallel_in_order((0..8).collect(), 3, |item: usize| {
        if item == 5 {
            bail!("package {item} failed")
        } else {
            Ok(item)
        }
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "package 5 failed");
    assert!(
        parallel_in_order(Vec::<usize>::new(), 4, Ok)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn package_workers_inherit_the_stage_log() {
    let log = PathBuf::from("/nonexistent/packages.log");
    let seen = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                performance::with_inherited_stage_log(
                    Some(log.clone()),
                    performance::current_stage_log,
                )
            })
            .join()
            .unwrap()
    });
    assert_eq!(seen, Some(log));
}
use std::os::unix::fs::{PermissionsExt, symlink};

#[test]
fn installer_package_cache_tracks_its_embedded_linux_kernel() {
    assert_eq!(
        package_stage_dependencies("installer"),
        ["installer", "linux"]
    );
    assert_eq!(package_stage_dependencies("btrfs-progs"), ["installer"]);
    assert_eq!(package_stage_dependencies("dosfstools"), ["installer"]);
    assert_eq!(
        package_stage_dependencies("plasma-login-manager"),
        ["plasma-login-manager"]
    );
    assert!(package_stage_dependencies("mattos-plasma-live").is_empty());
    assert_eq!(package_stage_dependencies("e2fsprogs"), ["e2fsprogs"]);
}

#[test]
fn flatpak_package_owns_the_complete_target_built_runtime_closure() {
    let specs = package_specs();
    let flatpak = specs
        .iter()
        .find(|spec| spec.name == "flatpak")
        .expect("flatpak package spec");
    assert_eq!(flatpak.source_component, "flatpak");
    assert_eq!(
        package_stage_dependencies("flatpak"),
        [
            "flatpak",
            "ostree",
            "gpgme",
            "gdk-pixbuf",
            "appstream",
            "json-glib",
            "libxmlb",
            "libfyaml",
            "fuse3",
            "libxml2",
            "libpng",
            "bubblewrap",
            "xdg-dbus-proxy",
        ]
    );
    assert!(package_source_roots("flatpak").contains(&"src/system/packages/flatpak"));
    assert!(package_source_roots("flatpak").contains(&"src/system/packages/ostree"));
    assert!(package_source_roots("flatpak").contains(&"src/system/security/bubblewrap"));
    assert!(package_source_roots("flatpak").contains(&"src/system/packages/xdg-dbus-proxy"));
    assert!(
        package_source_roots("flatpak").contains(&"src/system/installer/flatpak-target-install.c")
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    assert!(
        !root
            .join("src/system/packages/config/flatpak")
            .join("firefox.toml")
            .exists()
    );
}

#[test]
fn apt_package_cache_tracks_repository_policy_configuration() {
    assert_eq!(
        package_configuration_roots("apt"),
        ["src/system/packages/config/apt"]
    );
    assert_eq!(
        package_configuration_roots("flatpak"),
        ["src/system/packages/config/flatpak"]
    );
    assert!(
        !crate::stage_inputs::source_inputs(crate::BuildStage::Flatpak)
            .iter()
            .any(|p| p.starts_with("src/system/packages/config/flatpak"))
    );
}

#[test]
fn portal_package_consumes_flatpak_owned_bubblewrap_without_copying_it() {
    let specs = package_specs();
    let portal = specs
        .iter()
        .find(|spec| spec.name == "xdg-desktop-portal")
        .expect("portal package spec");
    assert!(portal.depends.contains(&"flatpak"));
    assert_eq!(
        package_stage_dependencies("xdg-desktop-portal"),
        ["xdg-desktop-portal", "gstreamer", "gstreamer-base"]
    );
    assert!(
        !package_source_roots("xdg-desktop-portal").contains(&"src/system/security/bubblewrap")
    );

    let root = tempfile::tempdir().unwrap();
    for (component, relative) in [
        ("bubblewrap", "usr/bin/bwrap"),
        ("gstreamer", "usr/lib/libgstreamer-1.0.so.0"),
        ("gstreamer-base", "usr/lib/libgstpbutils-1.0.so.0"),
        ("xdg-desktop-portal", "usr/libexec/xdg-desktop-portal"),
    ] {
        let path = root
            .path()
            .join("out/build")
            .join(component)
            .join("install")
            .join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, component).unwrap();
    }
    let staging = root.path().join("staging");
    stage_xdg_desktop_portal(root.path(), &staging).unwrap();
    assert!(!staging.join("usr/bin/bwrap").exists());
    assert!(staging.join("usr/libexec/xdg-desktop-portal").is_file());
}

#[test]
fn uidmap_helpers_stay_setuid_after_normalization() {
    let root = tempfile::tempdir().unwrap();
    for name in ["newuidmap", "newgidmap"] {
        let helper = root.path().join("usr/bin").join(name);
        fs::create_dir_all(helper.parent().unwrap()).unwrap();
        fs::write(&helper, "shadow helper\n").unwrap();
        set_mode(helper, 0o4755).unwrap();
    }
    normalize_package_modes(root.path()).unwrap();
    for name in ["newuidmap", "newgidmap"] {
        assert_eq!(
            fs::metadata(root.path().join("usr/bin").join(name)).unwrap().permissions().mode() & 0o7777,
            0o4755,
            "rootless containers map subordinate IDs through a setuid {name}"
        );
    }
}

#[test]
fn flatpak_document_portal_keeps_fusermount_privileged_after_normalization() {
    let root = tempfile::tempdir().unwrap();
    let helper = root.path().join("usr/bin/fusermount3");
    fs::create_dir_all(helper.parent().unwrap()).unwrap();
    fs::write(&helper, "target-owned fuse helper\n").unwrap();
    set_mode(helper.clone(), 0o755).unwrap();

    normalize_package_modes(root.path()).unwrap();

    assert_eq!(
        fs::metadata(helper).unwrap().permissions().mode() & 0o7777,
        0o4755,
        "xdg-document-portal requires a setuid fusermount3 to mount /run/user/$UID/doc"
    );
}

#[test]
fn xwayland_package_carries_the_owned_keyboard_compiler_and_layout_data() {
    let specs = package_specs();
    let xwayland = specs
        .iter()
        .find(|spec| spec.name == "xwayland")
        .expect("xwayland package spec");

    // xkbcomp is a runtime helper, deliberately kept out of Xwayland's
    // compile graph; it is composed into the same package instead.
    assert_eq!(
        package_stage_dependencies("xwayland"),
        [
            "xwayland",
            "libepoxy",
            "libfontenc",
            "libxfont",
            "libxcvt",
            "libxshmfence",
            "libxkbfile",
            "xkbcomp",
        ]
    );
    assert!(package_source_roots("xwayland").contains(&"src/system/graphics/xkbcomp"));
    assert!(xwayland.depends.contains(&"xkb-data"));
    assert_eq!(
        crate::stage_graph::direct_dependencies(crate::stage_graph::BuildStage::Xwayland),
        [
            "formal-sysroot",
            "x11-compat",
            "pixman",
            "wayland",
            "libffi",
            "xkbcommon",
            "libxkbfile",
            "libxfont",
            "libfontenc",
            "freetype",
            "zlib",
            "libxcvt",
            "libxshmfence",
            "libepoxy",
            "libdrm",
            "libglvnd",
            "libmd",
            "mesa",
        ]
    );
    assert!(!package_source_roots("xwayland").contains(&"src/system/libraries/freetype"));
    assert_eq!(
        package_source_roots("freetype"),
        ["src/system/libraries/freetype"]
    );
    assert_eq!(
        package_source_roots("fontconfig"),
        ["src/system/libraries/fontconfig"]
    );
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

#[test]
fn package_set_boundary_is_exact_and_detects_artifact_or_input_changes() {
    let entry = PackageSetEntry {
        package: "example".into(),
        cache_key: "input-a".into(),
        artifact_path: "out/packages/amd64/example.deb".into(),
        artifact_sha256: "artifact-a".into(),
    };
    let manifest = PackageSetManifest {
        schema_version: PACKAGE_SET_SCHEMA_VERSION,
        policy: package_set_policy().into(),
        packages: vec![entry.clone()],
    };
    assert_eq!(manifest.packages, vec![entry.clone()]);

    let mut changed_input = entry.clone();
    changed_input.cache_key = "input-b".into();
    assert_ne!(manifest.packages, vec![changed_input]);

    let mut changed_artifact = entry;
    changed_artifact.artifact_sha256 = "artifact-b".into();
    assert_ne!(manifest.packages, vec![changed_artifact]);
}

#[test]
fn package_set_policy_is_a_distinct_cache_boundary() {
    assert!(package_set_policy().contains("approved-package-cache-manifests"));
    assert_ne!(package_set_policy(), "package-cache-v1");
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
    let build = crate::build_system_tests::source_item("build_dpkg");
    assert!(
        build.find("sync_build_source").unwrap()
            < build.find("stage_missing_dpkg_source_inputs").unwrap()
    );
}

#[test]
fn apt_disables_checkout_dependent_build_rpath_padding() {
    let build = crate::build_system_tests::source_item("build_apt");
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
    let live = fs::read_to_string(config.join("00-mattos-local.sources")).unwrap();
    let installed = fs::read_to_string(config.join("installed/00-mattos-local.sources")).unwrap();
    assert!(live.contains("Trusted: yes"));
    assert!(!live.contains("Enabled: no"));
    assert!(installed.contains("Enabled: yes"));
    assert!(installed.contains("Trusted: yes"));
    assert!("00-mattos-local.sources" < "mattos-hosted.sources");
    let hosted = fs::read_to_string(config.join("installed/mattos-hosted.sources")).unwrap();
    let debian = fs::read_to_string(config.join("installed/debian-trixie.sources")).unwrap();
    assert!(hosted.contains("Enabled: yes"));
    assert!(debian.contains("Enabled: no"));
    assert!(debian.contains("Suites: trixie-security"));
}

#[test]
fn installer_has_an_owned_xkbcommon_runtime() {
    let specs = package_specs();
    let xkbcommon = specs
        .iter()
        .find(|spec| spec.name == "libxkbcommon0")
        .expect("xkbcommon runtime package must exist");
    assert_eq!(xkbcommon.source_component, "xkbcommon");
    assert_eq!(
        xkbcommon.depends,
        &["libc6", "xkb-data", "libxcb1", "libxml2-16"]
    );

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
    assert!(installer.provides.contains(&"mattos-installer-cli"));
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
        NVIDIA_OPEN_MODULES_PACKAGE,
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
            .contains(&NVIDIA_OPEN_MODULES_PACKAGE)
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
        &fs::read_to_string(root.join("src/system/graphics/nvidia-driver/manifest.toml")).unwrap(),
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
    assert!(
        root.join(
            "src/system/graphics/nvidia-open-gpu-kernel-modules/kernel-open/nvidia/nvidia.Kbuild"
        )
        .is_file()
    );
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
        "xwayland",
        "xdg-desktop-portal",
    ] {
        assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
    }
    assert_eq!(PACKAGE_NAMES.len(), 368);
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
        let path = staging.path().join("usr/share/iso-codes/json").join(name);
        assert!(path.is_file(), "missing {name}");
        assert!(!fs::read_to_string(path).unwrap().is_empty());
    }
    assert!(
        staging
            .path()
            .join("usr/share/doc/iso-codes/PROVENANCE.md")
            .is_file()
    );
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
        "dash",
        "sed",
        "mawk",
        "rsync",
        "grep",
        "findutils",
        "diffutils",
    ] {
        assert!(specs.iter().any(|spec| spec.name == name), "missing {name}");
    }
    assert_eq!(PACKAGE_NAMES.len(), 368);
    // The uutils search and comparison commands left mattos-base-runtime for
    // their Debian package names; the base profile still installs them.
    let base = specs.iter().find(|spec| spec.name == "mattos-base").unwrap();
    for name in ["grep", "findutils", "diffutils"] {
        let spec = specs.iter().find(|spec| spec.name == name).unwrap();
        assert!(spec.essential, "{name} is essential");
        assert_eq!(spec.replaces, &["mattos-base-runtime"], "{name} takes over its files");
        assert!(base.depends.contains(&name), "mattos-base lacks {name}");
    }
    assert_eq!(package_stage_dependencies("mattos-profiles"), &["systemd", "init"]);
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
    assert_eq!(package_recipe_revision("libpam-modules"), 2);
    // Flatpak's package payload includes MattOS's signed Flathub policy,
    // a minimal initialized OSTree layout, and the target-rooted optional
    // install helper, and no aggregate Info index. Keep this expectation
    // aligned with that contract.
    assert_eq!(package_recipe_revision("flatpak"), 10);
    let ssh_service = include_str!("../../../../system/network/openssh/ssh.service");
    assert!(ssh_service.contains("\nType=notify\n"));
    assert!(ssh_service.contains("ExecStart=/usr/sbin/sshd -D"));
    assert!(OPENSSH_SERVER_RUNTIME_PATHS.contains(&"usr/lib/openssh/sshd-session"));
    assert!(OPENSSH_SERVER_RUNTIME_PATHS.contains(&"usr/lib/openssh/sshd-auth"));
    let util_digest = package_definition_digest(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        util,
    )
    .unwrap();
    let gzip = specs.iter().find(|spec| spec.name == "gzip").unwrap();
    assert_ne!(
        util_digest,
        package_definition_digest(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            gzip
        )
        .unwrap()
    );
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
    assert_eq!(PACKAGE_NAMES.len(), 368);
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
            .any(|spec| matches!(spec.name, "tcl" | "bash"))
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
        include_str!("../../../../system/systemd/units/systemd-hwdb-update.service.in");
    for required in [
        "ConditionPathExists=|!{{UDEVLIBEXECDIR}}/hwdb.bin",
        "ConditionPathExists=|/etc/udev/hwdb.bin",
        "ConditionDirectoryNotEmpty=|/etc/udev/hwdb.d/",
    ] {
        assert!(imported_unit.contains(required));
    }
    // The compile invocation and payload checks are exercised directly
    // in packaging::staging::system::udev_hwdb_tests.
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
        validate_migrated_bootstrap_absent(&["/usr/bin/tar\t/usr/bin/tar\treason\thash".into()])
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
fn brush_package_owns_bash_but_not_sh() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    let source = repo.join("out/build/brush/cargo-target/release/brush");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "source-built brush\n").unwrap();
    let staging = repo.join("staging");

    stage_brush(repo, &staging).unwrap();

    // /usr/bin/sh belongs to dash.
    assert!(fs::symlink_metadata(staging.join("usr/bin/sh")).is_err());
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
        &fs::read_to_string(temp.path().join("out/reports/bootstrap-runtime-audit.toml")).unwrap(),
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
    fs::write(
        temp.path().join("consumer.c"),
        "extern int mattos_audit_symbol(void); int main(void) { return mattos_audit_symbol(); }\n",
    )
    .unwrap();
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
        validate_repository_packages(&repository_packages(Some("Depends: libapt-pkg7.0 (= 1)\n")))
            .is_ok()
    );
    assert!(
        validate_repository_packages(&repository_packages(Some("Depends: mattos-missing\n")))
            .is_err()
    );
    assert!(
        validate_repository_packages(&repository_packages(Some("Depends: libapt-pkg7.0 (= 2)\n")))
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
    let sources = include_str!("../../../../system/packages/config/apt/00-mattos-local.sources");
    let config = include_str!("../../../../system/packages/config/apt/01mattos");
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
    let metadata = include_str!("../../../../system/network/ca-bundle.toml");
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
    let main = crate::build_system_tests::tool_source();
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

#[test]
fn polkit_authentication_has_a_complete_self_contained_pam_policy() {
    let policy = include_str!("../../../../system/auth/config/pam.d/polkit-1");
    assert!(policy.contains("auth required pam_unix.so"));
    assert!(policy.contains("account required pam_unix.so"));
    assert!(!policy.contains("@include"));
    assert!(!policy.contains("pam_permit"));
    let packages = package_specs();
    let polkit = packages.iter().find(|p| p.name == "polkit").unwrap();
    for dependency in ["libpam0g", "libpam-modules", "libpam-runtime"] {
        assert!(polkit.depends.contains(&dependency));
    }
    assert!(
        package_configuration_roots("libpam-runtime").contains(&"src/system/auth/config/pam.d")
    );
    let network_recipe = include_str!("../stages/system_services.rs");
    assert!(
        network_recipe.contains("-Dpolkit_agent_helper_1=/usr/lib/polkit-1/polkit-agent-helper-1")
    );
}

#[test]
fn package_staging_cannot_mutate_a_cached_stage_input_tree() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("cached-stage/install");
    let staging = temp.path().join("package-staging");
    let descriptor = source.join("usr/lib/x86_64-linux-gnu/pkgconfig/example.pc");
    fs::create_dir_all(descriptor.parent().unwrap()).unwrap();
    fs::write(
        &descriptor,
        "prefix=/usr\nlibdir=${prefix}/lib/x86_64-linux-gnu\n",
    )
    .unwrap();
    let before = sha256_file(&descriptor).unwrap();

    copy_tree_preserving(&source, &staging).unwrap();
    normalize_tree_timestamps(&staging).unwrap();
    normalize_package_modes(&staging).unwrap();

    assert_eq!(sha256_file(&descriptor).unwrap(), before);
    assert_eq!(
        fs::read_to_string(staging.join("usr/lib/x86_64-linux-gnu/pkgconfig/example.pc")).unwrap(),
        "prefix=/usr\nlibdir=${prefix}/lib/x86_64-linux-gnu\n"
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
    let copied_status = fs::metadata(staging.join("usr/libexec/tool-core/tool-status")).unwrap();
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
        "usr/bin/pkexec",
        "usr/lib/polkit-1/polkit-agent-helper-1",
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
        "usr/bin/pkexec",
        "usr/lib/polkit-1/polkit-agent-helper-1",
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
fn protected_pins_must_match_the_manifest_exactly_in_well_formed_records() {
    let protected = ["libc6".to_string(), "gpgv".to_string()];
    let record =
        |names: &str| format!("Package: {names}\nPin: release o=Debian\nPin-Priority: -1\n");
    let good = format!(
        "Package: *\nPin: release o=Debian,n=trixie\nPin-Priority: 500\n\n{}\n{}",
        record("libc6"),
        record("gpgv")
    );
    validate_protected_pins("test", &good, &protected).unwrap();
    let missing = record("libc6");
    assert!(
        validate_protected_pins("test", &missing, &protected)
            .unwrap_err()
            .to_string()
            .contains("missing gpgv")
    );
    let extra = record("libc6 gpgv curl");
    assert!(
        validate_protected_pins("test", &extra, &protected)
            .unwrap_err()
            .to_string()
            .contains("curl")
    );
    let twice = format!("{}\n{}", record("libc6 gpgv"), record("libc6"));
    assert!(
        validate_protected_pins("test", &twice, &protected)
            .unwrap_err()
            .to_string()
            .contains("more than once")
    );
    // A comment does not separate records: two stanzas run together.
    let merged = format!("{}# Explanation: next\n{}", record("libc6"), record("gpgv"));
    assert!(
        validate_protected_pins("test", &merged, &protected)
            .unwrap_err()
            .to_string()
            .contains("exactly one")
    );
}

#[test]
fn repository_apt_preferences_protect_exactly_the_manifest() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let manifest: ProtectedPackageManifest = toml::from_str(
        &fs::read_to_string(root.join("src/system/packages/debian-compat/protected.toml")).unwrap(),
    )
    .unwrap();
    for file in ["00mattos-priority", "installed/00mattos-priority"] {
        let preferences =
            fs::read_to_string(root.join("src/system/packages/config/apt").join(file)).unwrap();
        validate_protected_pins(file, &preferences, &manifest.packages).unwrap();
    }
}

#[test]
fn live_and_installed_local_repository_sources_share_one_file_name() {
    // The installer overwrites the live source with the installed policy;
    // a different name would leave installed APT listing the local
    // repository twice.
    let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../system/packages/config/apt");
    assert!(config.join("00-mattos-local.sources").is_file());
    assert!(config.join("installed/00-mattos-local.sources").is_file());
    let mut local_sources = Vec::new();
    for entry in fs::read_dir(&config).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|extension| extension == "sources")
            && fs::read_to_string(&path)
                .unwrap()
                .contains("file:/usr/share/mattos/repository")
        {
            local_sources.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert_eq!(local_sources, ["00-mattos-local.sources"]);
}

#[test]
fn permanent_packages_exclude_live_profile_and_foreign_sources() {
    let files = [
        "etc/os-release",
        "etc/profile",
        "etc/apt/sources.list.d/00-mattos-local.sources",
    ];
    assert!(files.iter().all(|path| !path.contains("live-profile")));
    let sources = include_str!("../../../../system/packages/config/apt/00-mattos-local.sources");
    assert!(sources.contains("file:/usr/share/mattos/repository"));
    assert!(
        !sources.contains("debian") && !sources.contains("ubuntu") && !sources.contains("http:")
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
        ("libICE-1.1.2", "1.1.2"),
        ("libSM-1.2.6", "1.2.6"),
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
fn x11_compat_package_versions_follow_their_pinned_component_releases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let specs = package_specs();
    for (name, expected) in [
        ("libxau6", "1:1.0.12-1mattos1"),
        ("libxdmcp6", "1:1.1.5-1mattos1"),
        ("libice6", "1.1.2-1mattos1"),
        ("libsm6", "1.2.6-1mattos1"),
        ("libxi6", "1.8.3-1mattos1"),
        ("libxrender1", "0.9.12-1mattos1"),
        ("libxtst6", "1.2.5-1mattos1"),
        ("libxcursor1", "1:1.2.3-1mattos1"),
        ("libxft2", "2.3.9-1mattos1"),
        ("libxcb1", "1.17.0-1mattos1"),
        ("libx11-6", "2:1.8.12-1mattos1"),
        ("libxext6", "2:1.3.6-1mattos1"),
        ("libxfixes3", "6.0.2-1mattos1"),
    ] {
        let spec = specs.iter().find(|spec| spec.name == name).unwrap();
        assert_eq!(package_version(&root, spec).unwrap(), expected);
    }
}

#[test]
fn compatibility_manifest_pins_and_read_only_publisher_validate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    validate_debian_compatibility(&root).unwrap();
    let compatibility: DebianCompatibilityManifest = toml::from_str(
        &fs::read_to_string(root.join("src/system/packages/debian-compat/trixie.toml")).unwrap(),
    )
    .unwrap();
    let libxcb = compatibility
        .package
        .iter()
        .find(|package| package.mattos_name == "libxcb1")
        .unwrap();
    assert!(
        libxcb
            .owned_paths
            .iter()
            .any(|path| path == "/usr/lib/x86_64-linux-gnu/libxcb-xinput.so.0")
    );
    let preferences =
        fs::read_to_string(root.join("src/system/packages/config/apt/00mattos-priority")).unwrap();
    let local = preferences
        .find("Pin: release o=MattOS,l=MattOS Local,n=trixie\nPin-Priority: 990")
        .unwrap();
    let hosted = preferences
        .find("Pin: release o=MattOS,l=MattOS,n=trixie\nPin-Priority: 990")
        .unwrap();
    let debian = preferences.find("Pin-Priority: 500").unwrap();
    let blocked = preferences.find("Pin-Priority: -1").unwrap();
    assert!(local < hosted && hosted < debian && debian < blocked);
    assert!(!root.join("src/infrastructure/LinuxScripts/.git").exists());
    assert_eq!(
        sha256_file(
            &root.join("src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py")
        )
        .unwrap(),
        "0b0be18e1164481612aa41ab6300c301b5d1088f86f9f653aed5a516ed50f35c"
    );
}

#[test]
fn publish_plan_selects_mattos_repository_without_executing_manager() {
    let command = format_publish_command(
        Path::new("src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py"),
        &[PathBuf::from("out/packages/amd64/example_1.0_amd64.deb")],
    )
    .unwrap();
    assert_eq!(
        command,
        "python3 src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py --repo mattos upload out/packages/amd64/example_1.0_amd64.deb"
    );
}

#[test]
fn apt_sources_enable_hosted_mattos_and_never_trust_debian() {
    let hosted = include_str!("../../../../system/packages/config/apt/mattos-hosted.sources");
    let debian = include_str!("../../../../system/packages/config/apt/debian-trixie.sources");
    let installed_preferences =
        include_str!("../../../../system/packages/config/apt/installed/00mattos-priority");
    assert!(hosted.contains("Enabled: yes"));
    assert!(hosted.contains("https://packages.mattsherfey.com"));
    assert!(hosted.contains("Signed-By:"));
    assert!(debian.contains("Enabled: no"));
    assert!(debian.contains("https://deb.debian.org/debian"));
    assert!(debian.contains("Signed-By:"));
    assert!(!hosted.contains("Trusted: yes"));
    assert!(!debian.contains("Trusted: yes"));
    assert!(
        installed_preferences
            .contains("Pin: release o=MattOS,l=MattOS Local,n=trixie\nPin-Priority: 990")
    );
    assert!(
        installed_preferences
            .contains("Pin: release o=MattOS,l=MattOS,n=trixie\nPin-Priority: 990")
    );
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
        validate_publication_artifact_location(&approved.canonicalize().unwrap(), &text).is_err()
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
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let libc_before = package_definition_digest(&repo_root, libc).unwrap();
    let coreutils_before = package_definition_digest(&repo_root, coreutils).unwrap();
    let mut changed = libc.clone();
    changed.description = "changed test description";
    assert_ne!(
        libc_before,
        package_definition_digest(&repo_root, &changed).unwrap()
    );
    assert_eq!(
        coreutils_before,
        package_definition_digest(&repo_root, coreutils).unwrap()
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
    performance::atomic_write_json(&package_cache_manifest_path(root, "mattos-test"), &manifest)
        .unwrap();
    validate_package_cache(root, &spec, "1.0-1mattos1", &staging, &artifact, &input).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&artifact)
        .unwrap()
        .write_all(b"corrupt")
        .unwrap();
    assert!(
        validate_package_cache(root, &spec, "1.0-1mattos1", &staging, &artifact, &input,).is_err()
    );
}

#[test]
fn signed_flatpak_policy_seeds_a_minimal_readable_system_remote() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let descriptor = root.join("src/system/packages/config/flatpak/flathub.flatpakrepo");
    let temporary = tempfile::tempdir().unwrap();
    stage_flatpak_system_remote(&descriptor, temporary.path()).unwrap();

    let repo = temporary.path().join("var/lib/flatpak/repo");
    let config = fs::read_to_string(repo.join("config")).unwrap();
    assert!(config.contains("mode=bare-user-only"));
    assert!(config.contains("xa.applied-remotes=flathub;"));
    assert!(config.contains("[remote \"flathub\"]"));
    assert!(config.contains("url=https://dl.flathub.org/repo/"));
    assert!(config.contains("gpg-verify=true"));
    assert!(config.contains("gpg-verify-summary=true"));
    for directory in [
        "objects",
        "refs",
        "refs/heads",
        "refs/remotes",
        "state",
        "tmp",
        "extensions",
    ] {
        assert!(
            repo.join(directory).is_dir(),
            "missing OSTree {directory} directory"
        );
    }
    assert!(
        fs::metadata(repo.join("flathub.trustedkeys.gpg"))
            .unwrap()
            .len()
            > 1_000,
        "the seeded key must be the decoded full Flathub public key"
    );
}

#[test]
fn release_tags_of_every_style_name_their_upstream_version() {
    for (tag, version) in [
        ("openssl-3.5.8", "3.5.8"),
        ("v6.26.0", "6.26.0"),
        ("releases/gcc-15.3.0", "15.3.0"),
        ("binutils-2_46_1", "2.46.1"),
        ("curl-8_22_0", "8.22.0"),
        ("R_2_8_5", "2.8.5"),
        ("FILE5_48", "5.48"),
        ("libnl3_12_0", "3.12.0"),
        ("hostap_2_12", "2.12"),
        ("lcms2.16", "2.16"),
        ("libXfont2-2.0.9", "2.0.9"),
        ("xcb-util-renderutil-0.3.10", "0.3.10"),
        ("VER-2-14-3", "2.14.3"),
        ("V3-6-2", "3.6.2"),
        ("n8.1.2", "8.1.2"),
        ("release-78.3", "78.3"),
        ("popt-1.19-release", "1.19"),
        ("json-c-0.19-20260627", "0.19"),
        ("V_10_5_P1", "10.5p1"),
        ("2026d", "2026d"),
        ("20260916", "20260916"),
        ("238", "238"),
        ("v704", "704"),
        ("master-2026-09-03", "2026.09.03"),
        ("v7.2-rc5", "7.2~rc5"),
        ("v7.2.8", "7.2.8"),
    ] {
        assert_eq!(release_version_from_branch(tag).as_deref(), Some(version), "{tag}");
    }
    for moving in ["main", "master", "trunk"] {
        assert_eq!(release_version_from_branch(moving), None);
    }
}

#[test]
fn every_release_tag_in_the_source_manifest_yields_a_version() {
    // A tag that parses to no version silently becomes a snapshot version
    // (`0~git.<commit>`), whose order follows the commit hash: an update
    // could look like a downgrade to apt on installed systems.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let sources: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("upstream/sources.toml")).unwrap()).unwrap();
    let unparsed = sources["component"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|component| {
            let branch = component["branch"].as_str()?;
            let moving = matches!(branch, "main" | "master" | "trunk" | "develop" | "next")
                || branch.starts_with("stable-");
            (!moving && release_version_from_branch(branch).is_none())
                .then(|| format!("{} ({branch})", component["name"].as_str().unwrap_or("?")))
        })
        .collect::<Vec<_>>();
    assert!(unparsed.is_empty(), "release tags without a version: {unparsed:?}");
}

#[test]
fn builds_report_compatibility_entries_whose_recorded_version_drifted() {
    let root = tempfile::tempdir().unwrap();
    let entry = |name: &str, version: &str| {
        format!(
            "[[package]]\ndebian_name = \"{name}\"\nmattos_name = \"{name}\"\nsource_component = \"x\"\nowned_paths = [\"/x\"]\nprovided_abi_or_commands = [\"x\"]\nprotected = false\ncurrent_mattos_version = \"{version}\"\nexpected_debian_role = \"x\"\nclassification = \"debian-compatible\"\nknown_gaps = [\"x\"]\n"
        )
    };
    let manifest = format!(
        "schema_version = 1\nsuite = \"trixie\"\narchitecture = \"amd64\"\npolicy = \"p\"\nversion_policy = \"v\"\n\n{}\n{}",
        entry("tzdata", "2026a-1mattos1"),
        entry("zlib1g", "1.3.2-1mattos1")
    );
    let path = root.path().join("src/system/packages/debian-compat/trixie.toml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, manifest).unwrap();
    let built = |name: &str, version: &str| PackageInventoryEntry {
        name: name.to_string(),
        version: version.to_string(),
        architecture: ARCH.to_string(),
        artifact_path: String::new(),
        source_component: "x".to_string(),
        dependencies: Vec::new(),
        runtime_libraries: Vec::new(),
        file_count: 0,
        sha256: String::new(),
    };
    let inventory = PackageInventory {
        package: vec![built("tzdata", "2026d-1mattos1"), built("zlib1g", "1.3.2-1mattos1")],
    };
    let warning = stale_compatibility_versions(root.path(), &inventory).unwrap().unwrap();
    assert!(warning.contains("1 entry"), "{warning}");
    assert!(warning.contains("tzdata: records 2026a-1mattos1, built 2026d-1mattos1"), "{warning}");
    assert!(!warning.contains("zlib1g"));
}
