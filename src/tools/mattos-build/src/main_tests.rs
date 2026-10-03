use super::*;

#[test]
fn autotools_regenerates_configure_for_new_or_stale_inputs_only() {
    assert!(autotools_configure_needs_regeneration(false, true, false));
    assert!(autotools_configure_needs_regeneration(true, false, false));
    assert!(autotools_configure_needs_regeneration(true, true, true));
    assert!(!autotools_configure_needs_regeneration(true, true, false));
    assert!(autotools_build_tree_needs_reset(false, true, false, false));
    assert!(autotools_build_tree_needs_reset(true, true, true, false));
    assert!(autotools_build_tree_needs_reset(true, true, false, true));
    assert!(!autotools_build_tree_needs_reset(true, true, false, false));
}

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
    let autotools =
        root.join("out/build/autotools-fixture/install/usr/lib/x86_64-linux-gnu/pkgconfig");
    let meson = root.join("out/build/meson-fixture/install/usr/lib/x86_64-linux-gnu/pkgconfig");
    fs::create_dir_all(&autotools).unwrap();
    fs::create_dir_all(&meson).unwrap();
    let autotools_pc = autotools.join("autotools-fixture.pc");
    let meson_pc = meson.join("meson-fixture.pc");
    fs::write(
        &autotools_pc,
        "prefix=/usr\nlibdir=/usr/lib/x86_64-linux-gnu\n",
    )
    .unwrap();
    fs::write(&meson_pc, "prefix=/usr\nincludedir=/usr/include\n").unwrap();
    write_pkgconfig_overlay_fixture_manifest(root, "autotools-fixture", "autotools-output");
    write_pkgconfig_overlay_fixture_manifest(root, "meson-fixture", "meson-output");

    let sources = vec![
        (
            "autotools-fixture".to_string(),
            "lib".to_string(),
            autotools.clone(),
        ),
        (
            "meson-fixture".to_string(),
            "lib".to_string(),
            meson.clone(),
        ),
    ];
    let before_autotools = fs::read(&autotools_pc).unwrap();
    let before_meson = fs::read(&meson_pc).unwrap();
    let first = staged_pkgconfig_overlay(root, &sources).unwrap();
    let second = staged_pkgconfig_overlay(root, &sources).unwrap();

    assert_eq!(
        first, second,
        "identical consumers reuse one immutable overlay"
    );
    assert_eq!(fs::read(&autotools_pc).unwrap(), before_autotools);
    assert_eq!(fs::read(&meson_pc).unwrap(), before_meson);
    assert_eq!(
        fs::read_to_string(first[0].join("autotools-fixture.pc")).unwrap(),
        format!(
            "prefix={}\nlibdir=${{prefix}}/lib/x86_64-linux-gnu\n",
            root.join("out/build/autotools-fixture/install/usr")
                .display()
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
fn staged_pkgconfig_rewrite_rebases_descriptors_without_a_prefix_variable() {
    let prefix = Path::new("/tmp/mattos-stage/apt/usr");
    let apt_pkg = "libdir=/usr/lib/x86_64-linux-gnu\nincludedir=/usr/include\n\nName: apt-pkg\nCflags: -I${includedir}\n";
    let rewritten = rewrite_pkgconfig_for_staged_consumer(apt_pkg, prefix);
    assert!(rewritten.contains("libdir=/tmp/mattos-stage/apt/usr/lib/x86_64-linux-gnu\n"));
    assert!(rewritten.contains("includedir=/tmp/mattos-stage/apt/usr/include\n"));
    assert!(!rewritten.contains("${prefix}"));
    assert_eq!(rewrite_pkgconfig_for_staged_consumer(&rewritten, prefix), rewritten);
}

#[test]
fn component_cmake_options_override_helper_values_and_compose_flags() {
    let mut command = vec![
        "-DQt6Core5Compat_DIR=/view/Qt6Core5Compat".to_owned(),
        "-DCMAKE_EXE_LINKER_FLAGS=-L/staged/lib".to_owned(),
        "-DBUILD_TESTING=OFF".to_owned(),
    ];
    apply_component_cmake_options(
        &mut command,
        &[
            "-DQt6Core5Compat_DIR=/qt5compat/Qt6Core5Compat",
            "-DCMAKE_EXE_LINKER_FLAGS=-Wl,-rpath-link,/pulse",
            "-DWITH_FOO=ON",
        ],
    );
    assert_eq!(
        command.iter().filter(|argument| argument.starts_with("-DQt6Core5Compat_DIR=")).collect::<Vec<_>>(),
        ["-DQt6Core5Compat_DIR=/qt5compat/Qt6Core5Compat"]
    );
    assert!(command.contains(&"-DCMAKE_EXE_LINKER_FLAGS=-L/staged/lib -Wl,-rpath-link,/pulse".to_owned()));
    assert_eq!(command.last().map(String::as_str), Some("-DWITH_FOO=ON"));
}

#[test]
fn kde_cmake_keeps_upstream_install_rpaths_without_staged_link_paths() {
    let mut command = vec!["-DCMAKE_INSTALL_RPATH=".to_owned(), "-DCMAKE_SKIP_RPATH=ON".to_owned()];
    keep_upstream_install_rpaths(&mut command);
    assert!(!command.contains(&"-DCMAKE_SKIP_RPATH=ON".to_owned()));
    for required in [
        "-DCMAKE_INSTALL_RPATH=",
        "-DCMAKE_SKIP_RPATH=OFF",
        "-DCMAKE_INSTALL_RPATH_USE_LINK_PATH=OFF",
        "-DKDE_SKIP_RPATH_SETTINGS=TRUE",
        "-DQT_NO_QML_PLUGIN_RPATH=TRUE",
    ] {
        assert!(command.contains(&required.to_owned()), "missing {required}");
    }
}

#[test]
fn stage_keys_cover_recipe_files_their_recipe_calls_into_without_listing() {
    // Layer Shell Qt lists only plasma.rs, but its recipe runs the shared KDE
    // CMake helper (kde_foundation.rs), Qt's target arguments (qt.rs) and
    // shared helpers; a change to any of them must change its key.
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().unwrap();
    let inputs = crate::stage_inputs::source_inputs(crate::stage_graph::BuildStage::LayerShellQt);
    let implicit =
        crate::recipe_projection::implicit_recipe_inputs(&repo_root, "layer-shell-qt", &inputs).unwrap();
    let names = implicit
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    for required in ["kde_foundation.rs", "qt.rs"] {
        assert!(names.iter().any(|name| name == required), "{required} missing from {names:?}");
    }
    assert!(!names.iter().any(|name| name == "plasma.rs"), "listed input repeated: {names:?}");
    // The shared helpers it runs (command execution, source mirroring) are
    // covered as well, so a behavior change there rebuilds it without a
    // manual recipe revision.
    assert!(implicit.iter().any(|path| path.to_string_lossy().contains("/stages/helpers/")));
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
fn source_mirrors_have_the_modes_of_a_umask_022_checkout() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir()?;
    let source = temp.path().join("source");
    let mirror = temp.path().join("mirror");
    fs::create_dir_all(source.join("dir"))?;
    fs::write(source.join("dir/script"), "#!/bin/sh\n")?;
    fs::write(source.join("dir/data"), "data")?;
    for (path, mode) in [("dir", 0o775), ("dir/script", 0o775), ("dir/data", 0o664)] {
        fs::set_permissions(source.join(path), fs::Permissions::from_mode(mode))?;
    }
    let source_arg = format!("{}/", source.display());
    let mirror_arg = format!("{}/", mirror.display());
    let mut args = SOURCE_MIRROR_RSYNC_FLAGS.to_vec();
    args.extend([source_arg.as_str(), mirror_arg.as_str()]);
    assert!(Command::new("rsync").args(&args).status()?.success());
    for (path, mode) in [("dir", 0o755), ("dir/script", 0o755), ("dir/data", 0o644)] {
        let actual = fs::metadata(mirror.join(path))?.permissions().mode() & 0o7777;
        assert_eq!(actual, mode, "{path}");
    }
    Ok(())
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
        validate_experimental_child_jobs_with_budget(BuildStage::Libcap, Some(8), budget).is_err()
    );
    assert!(
        validate_experimental_child_jobs_with_budget(BuildStage::Apt, Some(1), budget).is_err()
    );
    assert!(
        validate_experimental_child_jobs_with_budget(BuildStage::Glibc, Some(13), budget).is_err()
    );
    assert!(
        validate_experimental_child_jobs_with_budget(BuildStage::Glibc, Some(8), budget).is_ok()
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
    let source = include_str!("stages/flatpak.rs");
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
    let source = include_str!("stages/flatpak.rs");
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
        assert!(
            !flatpak.contains(forbidden),
            "Flatpak still contains Firefox integration: {forbidden}"
        );
    }
    for unit in [
        "mattos-flatpak-system-update.service",
        "mattos-flatpak-system-update.timer",
        "mattos-flatpak-user-update.service",
        "mattos-flatpak-user-update.timer",
    ] {
        let body =
            fs::read_to_string(root.join("src/system/packages/config/flatpak").join(unit)).unwrap();
        if unit.ends_with(".timer") {
            assert!(body.contains("Persistent=true"));
            assert!(body.contains("RandomizedDelaySec="));
        } else {
            assert!(
                body.contains("flatpak"),
                "update unit {unit} is not Flatpak-backed"
            );
            assert!(body.contains("--noninteractive"));
            assert!(body.contains("--assumeyes"));
        }
    }
}

#[test]
fn flatpak_stage_publishes_the_target_rooted_installer_helper() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let helper =
        std::fs::read_to_string(root.join("src/system/installer/flatpak-target-install.c"))
            .unwrap();
    for required in [
        "flatpak_installation_new_for_path",
        "flatpak_transaction_new_for_installation",
        "flatpak_transaction_add_install",
        "flatpak_transaction_run",
        "var",
        "lib",
        "flatpak",
    ] {
        assert!(
            helper.contains(required),
            "target-install helper omits {required}"
        );
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
    let resources = root.join("src/system/packages/config/apt/units");
    let service = std::fs::read_to_string(resources.join("mattos-apt-daily.service")).unwrap();
    let timer = std::fs::read_to_string(resources.join("mattos-apt-daily.timer")).unwrap();
    assert!(service.contains("ExecStart=/usr/bin/apt-get update"));
    for forbidden in ["upgrade", "dist-upgrade", " install"] {
        assert!(
            !service.contains(forbidden),
            "APT metadata service contains {forbidden}"
        );
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
        assert!(
            timer.contains(required),
            "APT metadata timer omits {required}"
        );
    }
}

#[test]
fn all_parallel_stages_follow_dynamic_scheduler_grants() {
    for stage in build_plan(BuildStage::All) {
        let expected = if stage == BuildStage::Libcap {
            scheduler::ChildJobPolicy::Serial
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
    assert!(by_id["cross-toolchain"].dependencies.is_empty());
    assert_eq!(by_id["glibc"].dependencies, ["cross-toolchain"]);
    assert_eq!(
        by_id["gcc-runtime"].dependencies,
        ["cross-toolchain", "glibc"]
    );
    assert_eq!(by_id["linux"].dependencies, ["cross-toolchain"]);
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
        ["grub", "initramfs", "linux", "live-root", "rootfs"]
    );

    let mut durations: BTreeMap<String, f64> = BTreeMap::from([
        ("libnl", 32.0),
        ("wpa-supplicant", 42.0),
        ("grub", 180.0),
        ("acl", 16.862),
        ("apt", 168.123),
        ("attr", 12.009),
        ("binutils", 144.898),
        ("brush", 262.577),
        ("bzip2", 3.025),
        ("coreutils", 283.164),
        ("cozy", 30.000),
        ("pop-launcher", 60.000),
        ("greetd", 30.000),
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
        ("packagekit-qt", 60.000),
        ("haruna", 60.000),
        ("mpvqt", 60.000),
        ("discover", 60.000),
        ("gwenview", 60.000),
        ("kimageannotator", 60.000),
        ("kcolorpicker", 60.000),
        ("elisa", 60.000),
        ("partitionmanager", 60.000),
        ("kwalletmanager", 60.000),
        ("kcalc", 60.000),
        ("shared-mime-info", 15.000),
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
        ("qtbase", 2_500.000),
        ("qtlocation", 300.000),
        ("qttools", 300.000),
        ("kconfigwidgets", 90.000),
        ("kcolorscheme", 90.000),
        ("kwindowsystem", 90.000),
        ("kguiaddons", 90.000),
        ("kiconthemes", 90.000),
        ("kirigami", 120.000),
        ("kdecoration", 90.000),
        ("kcmutils", 90.000),
        ("kxmlgui", 90.000),
        ("kglobalaccel", 90.000),
        ("karchive", 90.000),
        ("kwayland", 90.000),
        ("breeze-icons", 90.000),
        ("kbookmarks", 90.000),
        ("kcompletion", 90.000),
        ("kitemviews", 90.000),
        ("kitemmodels", 90.000),
        ("kjobwidgets", 90.000),
        ("kservice", 90.000),
        ("kparts", 90.000),
        ("solid", 90.000),
        ("kded", 90.000),
        ("plasma-framework", 120.000),
        ("plasma-activities", 90.000),
        ("plasma-activities-stats", 90.000),
        ("kwin", 300.000),
        ("ksysguard", 90.000),
        ("plasma-workspace", 300.000),
        ("plasma-desktop", 300.000),
        ("breeze", 90.000),
        ("qtsvg", 90.000),
        ("qtwayland", 300.000),
        ("qtpositioning", 300.000),
        ("qtdeclarative", 1_000.000),
        ("qt5compat", 120.000),
        ("qtshadertools", 120.000),
        ("kcoreaddons", 90.000),
        ("kconfig", 90.000),
        ("kdbusaddons", 90.000),
        ("kauth", 90.000),
        ("ki18n", 90.000),
        ("kwidgetsaddons", 90.000),
        ("polkit-qt-1", 60.000),
        ("yaml-cpp", 30.000),
        ("kpmcore", 120.000),
        ("qtsvg", 90.000),
        ("qtwayland", 300.000),
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
    // Keep this behavioral scheduler smoke test resilient as the explicit
    // production graph grows: newly added stages get a conservative
    // synthetic duration, while the measured entries above retain their
    // historical estimates.
    for node in &nodes {
        durations.entry(node.id.clone()).or_insert(60.0);
    }
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
        (
            BuildStage::GstreamerBase,
            "out/build/gstreamer-base/install",
        ),
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
        upstream_committed_at_utc: Some("2026-01-01T00:00:00Z".to_string()),
    };

    write_sync_state(root, "linux", &state).expect("write state");
    let loaded = read_sync_state(root, "linux")
        .expect("read state")
        .expect("present");
    assert_eq!(loaded.repo, state.repo);
    assert_eq!(loaded.branch, state.branch);
    assert_eq!(loaded.imported_commit, state.imported_commit);
    assert_eq!(loaded.upstream_committed_at_utc, state.upstream_committed_at_utc);
}

#[test]
fn sync_refuses_an_edited_vendored_file_and_names_it() {
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

    let imported = read_sync_state(root, "linux").unwrap().unwrap().imported_commit;
    let error = import_component(root, &comp, true).unwrap_err().to_string();
    assert!(error.contains("no longer matches its import"), "{error}");
    assert!(error.contains("modified README"), "the edited file is named: {error}");
    let local = fs::read_to_string(root.join("src/kernel/linux/README")).expect("read file");
    assert_eq!(local, "local\n", "a refused sync leaves the tree untouched");
    assert_eq!(read_sync_state(root, "linux").unwrap().unwrap().imported_commit, imported);
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

    import_sources(root, false, Some("grep".to_string()), false).expect("import should succeed");
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
    symlink("normal file.txt", upstream_root.join("normal-link")).expect("create upstream symlink");
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
fn sync_refuses_an_untracked_file_added_to_a_vendored_tree() {
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
    let error = import_component(root, &comp, true).unwrap_err().to_string();
    assert!(error.contains("added local-only.txt"), "{error}");
    assert_eq!(
        fs::read_to_string(root.join("src/userland/grep/local-only.txt")).expect("read local file"),
        "local edit\n"
    );
    assert!(!root.join("src/userland/grep/NEWS").exists(), "the update was not applied");
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
    clear_directory_contents_except(root, &[]).expect("clear");
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
    symlink("shared.c", root.join("arch/x86/kernel/shared-link")).expect("create retained symlink");

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
    // Upstream's attributes are not imported, so MattOS's Git neither
    // converts the attributed file on commit nor on checkout.
    assert!(!imported.join(".gitattributes").exists());
    write(&root.join(".gitattributes"), "* -text\n");
    run_ok(root, "git", &["add", "-A"]);
    run_ok(root, "git", &["commit", "-m", "vendor"]);
    fs::remove_file(imported.join("windows.bat")).expect("remove checked-out file");
    run_ok(root, "git", &["checkout", "--", "src/imported/example/windows.bat"]);
    assert_eq!(
        fs::read(imported.join("windows.bat")).expect("checked-out source"),
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
    let selected =
        select_components(&components, false, Some("brush".to_string())).expect("select component");
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
    clear_directory_contents_except(&path, &[]).expect("clear missing");
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
        "set default=0\nmenuentry \"Start MattOS Live\" { linux /boot/vmlinuz rdinit=/init }\nmenuentry \"Start MattOS Live (CLI)\" { linux /boot/vmlinuz rdinit=/init }\nmenuentry \"Install MattOS (CLI)\" { linux /boot/vmlinuz rdinit=/init }\n",
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
        "menuentry \"Start MattOS Live\" { linux /boot/vmlinuz rdinit=/init initrd /boot/early-initramfs.cpio.xz }\nmenuentry \"Start MattOS Live (CLI)\" { linux /boot/vmlinuz rdinit=/init initrd /boot/early-initramfs.cpio.xz }\nmenuentry \"Install MattOS (CLI)\" { linux /boot/vmlinuz rdinit=/init initrd /boot/early-initramfs.cpio.xz }\nmenuentry \"MattOS Rescue\" { linux /boot/vmlinuz rdinit=/init mattos.rescue=1 initrd /boot/early-initramfs.cpio.xz }\nmenuentry \"MattOS AMD graphics diagnostics (CLI)\" { linux /boot/vmlinuz rdinit=/init initrd /boot/early-initramfs.cpio.xz }\n",
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
        "mattos.mode=install-cli",
    ] {
        assert!(grub.contains(mode), "GRUB omits explicit {mode} contract");
    }
    assert!(!grub.contains("systemd.unit="));

    let dispatcher = include_str!("../../../boot/live-init.c");
    for target in [
        "mattos-live-graphical.target",
        "mattos.target",
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

    let live_greetd = include_str!("../../../system/session/plasma/plasma-live.toml");
    assert!(live_greetd.contains("[initial_session]"));
    assert!(live_greetd.contains("command = \"/usr/bin/start-plasma\""));
    assert!(live_greetd.contains("user = \"mattos\""));
    let manager_pam = include_str!("../../../system/session/plasma-login-manager/plasmalogin.pam");
    assert!(manager_pam.contains("pam_unix.so"));
    assert!(manager_pam.contains("pam_systemd.so"));
    let manager_session =
        include_str!("../../../system/session/plasma-login-manager/mattos-plasma.desktop");
    assert!(manager_session.contains("Exec=/usr/bin/start-plasma"));
    let plasma_greeter = include_str!("../../../system/session/plasma/mattos-plasma-greeter");
    assert!(plasma_greeter.contains("exec /usr/bin/agreety --cmd /usr/bin/start-plasma"));
    let live_override = include_str!(
        "../../../system/profiles/live/etc/systemd/system/plasma-greeter.service.d/live.conf"
    );
    assert!(
        live_override.contains("ExecStart=/usr/bin/greetd --config /etc/greetd/plasma-live.toml")
    );
}

#[test]
fn plasma_live_session_uses_the_systemd_user_bus_and_fallback_autostart() {
    let launcher = include_str!("../../../system/session/plasma/start-plasma");
    assert!(
        launcher.contains("export DBUS_SESSION_BUS_ADDRESS=\"unix:path=${XDG_RUNTIME_DIR}/bus\"")
    );
    assert!(launcher.contains("plasma-dbus-run-session-if-needed"));
    assert!(launcher.contains("/usr/plugins:/usr/lib/x86_64-linux-gnu/plugins"));
    assert!(!launcher.contains("mattos-private-session.conf"));

    let staging = crate::build_system_tests::packaging_source();
    assert!(staging.contains("let autostart = install_root.join(\"etc/xdg/autostart\")"));
    assert!(
        staging.contains("copy_tree_preserving(&autostart, &staging.join(\"etc/xdg/autostart\"))")
    );
}

#[test]
fn flatpak_package_owns_signed_flathub_policy_without_application_overrides() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let descriptor = root.join("src/system/packages/config/flatpak/flathub.flatpakrepo");
    let policy = std::fs::read_to_string(&descriptor)
        .expect("MattOS must retain the packaged Flathub descriptor");
    assert!(policy.contains("[Flatpak Repo]"));
    assert!(policy.contains("Url=https://dl.flathub.org/repo/"));
    let key = policy
        .split_once("GPGKey=")
        .expect("Flathub policy must embed its verification key")
        .1
        .trim();
    assert!(
        key.len() > 3_000,
        "Flathub policy must retain the full pinned public key"
    );
    let package_source = crate::build_system_tests::packaging_source();
    assert!(package_source.contains("usr/share/flatpak/remotes.d/flathub.flatpakrepo"));
    assert!(package_source.contains("stage_flatpak_system_remote"));
    assert!(package_source.contains("var/lib/flatpak/repo"));
    assert!(
        !root
            .join("src/rootfs/skeleton/etc/skel/.local/share/flatpak/overrides")
            .exists()
    );
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
    let source = crate::build_system_tests::source_item("collect_artifact_records")
        + &crate::build_system_tests::source_item("extract_efi_image_record");
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
        upstream_committed_at_utc: Some("2026-01-01T00:00:00Z".to_string()),
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
    assert!(
        include_str!("commands/doctor.rs")
            .contains("if !missing_required.contains(&\"pkg-config\")")
    );
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
    let source = crate::build_system_tests::source_item("build_kernel");
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
    let source = concat!(
        include_str!("stages/runtime_tooling.rs"),
        include_str!("stages/llvm_toolchain.rs"),
        include_str!("stages/rust_toolchain.rs")
    );
    let start = source.find("fn build_rust").unwrap();
    let end = source.len();
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
fn imported_source_mirror_follows_mattos_ignore_rules_and_preserves_source() {
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
    // Upstream's nested ignore rules do not hide files from the mirror, just
    // as they do not hide them from the source digest; only MattOS's root
    // rules exclude residue.  (The tracking audit rejects such residue.)
    assert!(mirror.join("target/generated.o").exists());
    write(&root.join(".gitignore"), "/src/imported/example/target/\n");
    copy_imported_working_tree(root, Path::new("src/imported/example"), &mirror)
        .expect("recreate source mirror");
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
fn sync_refuses_local_edits_even_where_upstream_did_not_change() {
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

    let error = import_component(root, &comp, true).unwrap_err().to_string();
    assert!(error.contains("modified meson.build"), "{error}");
    let local =
        fs::read_to_string(root.join("src/system/systemd/meson.build")).expect("read local file");
    assert_eq!(local, "local change\n");
    assert!(!root.join("src/system/systemd/README").exists());
}

#[test]
fn a_refused_sync_never_writes_conflict_markers_or_advances_state() {
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

    let imported = read_sync_state(root, "systemd").unwrap().unwrap().imported_commit;
    assert!(import_component(root, &comp, true).is_err());
    let content = fs::read_to_string(root.join("src/system/systemd/meson.build")).expect("read");
    assert_eq!(content, "local\n");
    assert!(!content.contains("<<<<<<<"));
    assert_eq!(read_sync_state(root, "systemd").unwrap().unwrap().imported_commit, imported);
}

#[test]
fn build_plan_all_includes_uutils_stages() {
    let plan = build_plan(BuildStage::All);
    assert_eq!(plan[0], BuildStage::CrossToolchain);
    assert_eq!(plan[1], BuildStage::Glibc);
    assert_eq!(plan[2], BuildStage::GccRuntime);
    assert_eq!(plan[3], BuildStage::Kernel);
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
        // PIE, CET, build IDs, hash style and --disable-multilib come
        // from the defaults shared with the build compilers.
        "configure_args.extend_from_slice(MATTOS_GCC_DEFAULTS)",
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
    let source = include_str!("stages/helpers/command.rs");
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
    let end = source[start..].find("\nfn ").unwrap() + start;
    let build = &source[start..end];
    for required in [
        "--with-sysroot=",
        "--with-build-sysroot=",
        "--with-as=",
        "--with-ld=",
        "--enable-languages=c,c++",
        "MATTOS_GCC_DEFAULTS",
        "--disable-analyzer",
        "--disable-libsanitizer",
        "--disable-libquadmath",
        "--disable-libstdcxx-pch",
        "all-gcc",
        "all-target-libgcc",
        "all-target-libstdc++-v3",
        "install-target-libgcc",
        "install-target-libstdc++-v3",
        "CFLAGS_FOR_TARGET",
        "CXXFLAGS_FOR_TARGET",
        "TARGET_COMPILER_INSTALL",
    ] {
        assert!(
            build.contains(required),
            "missing GCC runtime setting {required}"
        );
    }
    for forbidden in [
        "-I/usr/include",
        "-L/usr/lib ",
        "--with-system-zlib",
        "Path::new(\"g++\")",
    ] {
        assert!(
            !build.contains(forbidden),
            "forbidden GCC target install/input {forbidden}"
        );
    }
    // The shipped runtime tree receives target libraries only; the
    // compiler is installed solely into the private toolchain prefix.
    let runtime_start = build.find("DESTDIR={}\", raw_install").unwrap();
    let runtime_end = build[runtime_start..]
        .find("GCC runtime install failed")
        .unwrap()
        + runtime_start;
    assert!(!build[runtime_start..runtime_end].contains("install-gcc"));
    let toolchain_install = build.find("let mut toolchain_targets").unwrap();
    assert!(build[toolchain_install..].contains("toolchain_destdir.as_str(), \"install-gcc\""));
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
        (
            "sed",
            "4.10",
            "89b7a2224d4faa9d8baf76094b1232ad1477ef3e",
            SED_RELEASE_ARCHIVE_URL,
            SED_RELEASE_ARCHIVE_SHA256,
        ),
        (
            "dash",
            "0.5.13.5",
            "037bbdfd330017c368caf6242f977974123239b5",
            DASH_RELEASE_ARCHIVE_URL,
            DASH_RELEASE_ARCHIVE_SHA256,
        ),
        (
            "rsync",
            "3.5.1",
            "04355d27b7386d7de0e6bd5e79c556223210f700",
            RSYNC_RELEASE_ARCHIVE_URL,
            RSYNC_RELEASE_ARCHIVE_SHA256,
        ),
        (
            "m4",
            "1.4.21",
            "fe2f13ab9ab9b3e712c6529f0b2a49a81feb6ce2",
            M4_RELEASE_ARCHIVE_URL,
            M4_RELEASE_ARCHIVE_SHA256,
        ),
        (
            "autoconf",
            "2.73",
            "44d712a26b0e14931bf2df57e2c9b80a2747dfce",
            AUTOCONF_RELEASE_ARCHIVE_URL,
            AUTOCONF_RELEASE_ARCHIVE_SHA256,
        ),
        (
            "automake",
            "1.19",
            "e82d2d34d4626445565bc131f6580b59597d0c62",
            AUTOMAKE_RELEASE_ARCHIVE_URL,
            AUTOMAKE_RELEASE_ARCHIVE_SHA256,
        ),
        (
            "libtool",
            "2.6.2",
            "309bb53a8adfb22c6e5869cc8da049bf123e5438",
            LIBTOOL_RELEASE_ARCHIVE_URL,
            LIBTOOL_RELEASE_ARCHIVE_SHA256,
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
        15
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

    let source = concat!(
        include_str!("stages/runtime_tooling.rs"),
        include_str!("stages/llvm_toolchain.rs"),
        include_str!("stages/rust_toolchain.rs")
    );
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

    let rust_end = source.len();
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
        "require_mattos_target_toolchain",
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

    let source = concat!(
        include_str!("stages/runtime_tooling.rs"),
        include_str!("stages/llvm_toolchain.rs"),
        include_str!("stages/rust_toolchain.rs")
    );
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

    let source = concat!(
        include_str!("stages/runtime_tooling.rs"),
        include_str!("stages/llvm_toolchain.rs"),
        include_str!("stages/rust_toolchain.rs")
    );
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
    assert!(
        manifest.contains(
            "sha256 = \"a5049e836e578d76d424b075246aafb18a3d4f2f2f08f3447d7ccb8484811a59\"",
        )
    );
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
    let builder = include_str!("stages/foundation_libraries.rs");
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
    let builder = include_str!("stages/foundation_libraries.rs");
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
    let builder = include_str!("stages/libraries.rs");
    let start = builder.find("fn build_bzip2").unwrap();
    let end = builder[start..].find("fn build_lz4").unwrap() + start;
    let bzip2_build = &builder[start..end];

    // bzip2's Makefile otherwise inherits the host CFLAGS, including a
    // possible -g.  Rebuilding forces stale objects out while the maps
    // make every output-owned source mirror look identical to the linker.
    assert!(
        bzip2_build.contains("\"-B\",\n            \"-f\",\n            \"Makefile-libbz2_so\"")
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
    assert!(body.contains("PROMPT_COMMAND=\"__systemd_osc_context_precmdline;${PROMPT_COMMAND}\""));
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
        (
            "login",
            "session    required     pam_env.so readenv=1 envfile=/etc/default/locale\nsession    optional     pam_systemd.so\n",
        ),
        (
            "su-l",
            "session    required     pam_env.so readenv=1 envfile=/etc/default/locale\nsession    optional     pam_systemd.so\n",
        ),
        ("su", "session    required     pam_unix.so\n"),
        ("sudo", "session    required     pam_unix.so\n"),
        ("passwd", "password   required     pam_unix.so\n"),
        (
            "systemd-user",
            "account    required     pam_unix.so\nsession    required     pam_unix.so\nsession    optional     pam_systemd.so\n",
        ),
        (
            "sshd",
            "auth       required     pam_unix.so\nsession    required     pam_env.so readenv=1 envfile=/etc/default/locale\nsession    required     pam_unix.so\nsession    optional     pam_systemd.so\n",
        ),
        ("plasma-greeter", "session    optional     pam_systemd.so\n"),
        (
            "plasmalogin",
            "auth       required     pam_unix.so\nsession    optional     pam_systemd.so\n",
        ),
        (
            "plasmalogin-autologin",
            "auth       required     pam_permit.so\nsession    optional     pam_systemd.so\n",
        ),
        (
            "plasmalogin-greeter",
            "auth       required     pam_permit.so\nsession    optional     pam_systemd.so\n",
        ),
    ] {
        write(&rootfs.join("etc/pam.d").join(stack), body);
    }
    write(
        &rootfs.join("usr/share/pam/security/pam_env.conf"),
        "# MattOS source-built PAM environment defaults.\n",
    );
    fs::create_dir_all(rootfs.join("etc/default")).expect("default dir");
    std::os::unix::fs::symlink("../locale.conf", rootfs.join("etc/default/locale"))
        .expect("default locale link");
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
        "usr/bin/pkexec",
        "usr/bin/fusermount3",
        "usr/lib/polkit-1/polkit-agent-helper-1",
        "usr/bin/newuidmap",
        "usr/bin/newgidmap",
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
    // Rootless containers: the subordinate-ID databases exist (empty) so
    // useradd allocates ranges, and the mapping helpers are setuid.
    assert_eq!(fs::read_to_string(root.join("etc/subuid")).unwrap(), "");
    assert_eq!(fs::read_to_string(root.join("etc/subgid")).unwrap(), "");
    assert_eq!(fs::metadata(root.join("usr/bin/newuidmap")).unwrap().permissions().mode() & 0o7777, 0o4755);
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
        "usr/bin/pkexec",
        "usr/bin/fusermount3",
        "usr/lib/polkit-1/polkit-agent-helper-1",
        "usr/bin/newuidmap",
        "usr/bin/newgidmap",
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
        fs::read_link(rootfs.join("usr/lib/systemd/system/dbus-org.freedesktop.network1.service"))
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
    let error =
        install_systemd_service_alias(tmp.path(), "dbus-org.example.service", "missing.service")
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
        &rootfs.join("etc/systemd/resolved.conf.d/10-mattos.conf"),
        "[Resolve]\nDNSStubListener=yes\n",
    );
    write(
        &rootfs.join("etc/systemd/timesyncd.conf.d/10-mattos.conf"),
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

#[test]
fn plasma_wayland_build_keeps_the_real_pager_without_x11_headers() {
    let implementation = include_str!("stages/kde_foundation.rs");
    assert!(implementation.contains("applets/pager/pagermodel.cpp"));
    assert!(implementation.contains("#if HAVE_X11\\n#include <xwindowtasksmodel.h>\\n#endif"));
    assert!(implementation.contains("plasma_pager_qmimedata_header_mirror(&contents)"));
    let plasma = include_str!("stages/plasma.rs");
    assert!(plasma.contains("-DWITH_X11=ON"));
    assert!(plasma.contains("-DWITH_X11_SESSION=OFF"));
    assert!(plasma.contains("usr/bin/ksmserver"));
    assert!(plasma.contains("plasma-ksmserver.service"));
    assert!(stage_graph::direct_dependencies(BuildStage::PlasmaWorkspace).contains(&"x11-compat"));
    let x11_sources = stage_inputs::source_inputs(BuildStage::X11Compat);
    for source in [
        "src/system/graphics/libice",
        "src/system/graphics/libsm",
        "src/tools/mattos-build/src/stages/graphics.rs",
    ] {
        assert!(
            x11_sources.contains(&PathBuf::from(source)),
            "missing {source}"
        );
    }
    assert!(packaging::PACKAGE_NAMES.contains(&"libice6"));
    assert!(packaging::PACKAGE_NAMES.contains(&"libsm6"));
}

/// An upstream repository with one commit holding `files`; returns its commit.
fn upstream_with(root: &Path, files: &[(&str, &str)]) -> String {
    init_git_repo(root);
    for (path, body) in files {
        write(&root.join(path), body);
    }
    run_ok(root, "git", &["add", "-A"]);
    run_ok(root, "git", &["commit", "-q", "-m", "upstream"]);
    run_cmd_capture(root, "git", &["rev-parse", "HEAD"]).unwrap().trim().to_string()
}

fn workspace_with_sources(root: &Path, sources: &str) {
    init_git_repo(root);
    let sources = if sources.is_empty() {
        "[[component]]\nname = \"unrelated\"\nrepo = \"x\"\nbranch = \"main\"\npath = \"src/unrelated\"\nsync = \"copy\"\n"
    } else {
        sources
    };
    write(&root.join("upstream/sources.toml"), sources);
    run_ok(root, "git", &["add", "-A"]);
    run_ok(root, "git", &["commit", "-q", "-m", "workspace"]);
}

fn component(name: &str, repo: &Path, revision: &str, path: &str) -> ComponentDef {
    ComponentDef {
        name: name.to_string(),
        repo: repo.to_string_lossy().to_string(),
        branch: "main".to_string(),
        revision: Some(revision.to_string()),
        path: path.to_string(),
        sync: "copy".to_string(),
    }
}

#[test]
fn an_update_needs_no_shared_history_even_across_a_repository_move() {
    // The old importer merged the prior and new commits, which required their
    // common history.  A verified, unedited tree is simply replaced, so the
    // new pin may even come from a different repository.
    let old = tempfile::tempdir().unwrap();
    let old_commit = upstream_with(old.path(), &[("Makefile", "VERSION = 1\n")]);
    let new = tempfile::tempdir().unwrap();
    let new_commit = upstream_with(new.path(), &[("Makefile", "VERSION = 2\n"), ("NEWS", "two\n")]);
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    workspace_with_sources(root, "");
    import_component(root, &component("kernel", old.path(), &old_commit, "src/kernel"), false).unwrap();
    run_ok(root, "git", &["add", "-A"]);
    run_ok(root, "git", &["commit", "-q", "-m", "import"]);

    import_component(root, &component("kernel", new.path(), &new_commit, "src/kernel"), true).unwrap();
    assert_eq!(fs::read_to_string(root.join("src/kernel/Makefile")).unwrap(), "VERSION = 2\n");
    let state = read_sync_state(root, "kernel").unwrap().unwrap();
    assert_eq!(state.imported_commit, new_commit);
    assert_eq!(state.repo, new.path().to_string_lossy());
}

#[test]
fn syncing_a_parent_preserves_components_nested_inside_it() {
    let parent = tempfile::tempdir().unwrap();
    let first = upstream_with(parent.path(), &[("meson.build", "one\n")]);
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    workspace_with_sources(
        root,
        "[[component]]\nname = \"ostree\"\nrepo = \"x\"\nbranch = \"main\"\npath = \"src/ostree\"\nsync = \"copy\"\n\n[[component]]\nname = \"gvdb\"\nrepo = \"x\"\nbranch = \"main\"\npath = \"src/ostree/subprojects/gvdb\"\nsync = \"copy\"\n",
    );
    import_component(root, &component("ostree", parent.path(), &first, "src/ostree"), false).unwrap();
    write(&root.join("src/ostree/subprojects/gvdb/gvdb.c"), "nested component\n");
    run_ok(root, "git", &["add", "-A"]);
    run_ok(root, "git", &["commit", "-q", "-m", "import"]);

    write(&parent.path().join("meson.build"), "two\n");
    run_ok(parent.path(), "git", &["commit", "-q", "-am", "two"]);
    let second = run_cmd_capture(parent.path(), "git", &["rev-parse", "HEAD"]).unwrap().trim().to_string();
    import_component(root, &component("ostree", parent.path(), &second, "src/ostree"), true).unwrap();
    assert_eq!(fs::read_to_string(root.join("src/ostree/meson.build")).unwrap(), "two\n");
    assert_eq!(
        fs::read_to_string(root.join("src/ostree/subprojects/gvdb/gvdb.c")).unwrap(),
        "nested component\n",
        "the nested component is neither verified nor cleared"
    );
}

#[test]
fn ignored_residue_is_removed_but_ignored_upstream_files_are_verified_and_kept() {
    let upstream = tempfile::tempdir().unwrap();
    let first = upstream_with(upstream.path(), &[("configure", "#!/bin/sh\n"), ("LICENSE", "terms\n")]);
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    workspace_with_sources(root, "");
    write(&root.join(".gitignore"), "LICENSE\n*.o\n");
    import_component(root, &component("zlib", upstream.path(), &first, "src/zlib"), false).unwrap();
    run_ok(root, "git", &["add", "-A"]);
    run_ok(root, "git", &["commit", "-q", "-m", "import"]);
    // A build left an object file behind in the vendored tree.
    write(&root.join("src/zlib/adler32.o"), "object\n");

    write(&upstream.path().join("configure"), "#!/bin/sh\nexit 0\n");
    run_ok(upstream.path(), "git", &["commit", "-q", "-am", "two"]);
    let second = run_cmd_capture(upstream.path(), "git", &["rev-parse", "HEAD"]).unwrap().trim().to_string();
    import_component(root, &component("zlib", upstream.path(), &second, "src/zlib"), true).unwrap();
    assert!(!root.join("src/zlib/adler32.o").exists(), "residue is removed");
    assert_eq!(fs::read_to_string(root.join("src/zlib/LICENSE")).unwrap(), "terms\n");

    // An edit to the ignored upstream file is still a local modification.
    write(&root.join("src/zlib/LICENSE"), "edited\n");
    let error = import_component(root, &component("zlib", upstream.path(), &second, "src/zlib"), true)
        .unwrap_err()
        .to_string();
    assert!(error.contains("modified LICENSE"), "{error}");
}

#[test]
fn retained_paths_policies_are_applied_on_import_and_sync() {
    let upstream = tempfile::tempdir().unwrap();
    let revision = upstream_with(
        upstream.path(),
        &[("fonts/ttf/Sans.ttf", "font\n"), ("OFL.txt", "license\n"), ("sources/Sans.glyphs", "design\n")],
    );
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let policy = format!(
        "schema_version = 1\ncomponent = \"open-sans\"\nupstream_commit = \"{revision}\"\nretained_paths = [\"fonts/ttf\", \"OFL.txt\"]\nreason = \"Runtime fonts and license only.\"\n"
    );
    workspace_with_sources(
        root,
        "[[component]]\nname = \"open-sans\"\nrepo = \"x\"\nbranch = \"main\"\npath = \"src/fonts/open-sans\"\nsync = \"copy\"\nintentional_omission_policy = \"upstream/policies/open-sans.toml\"\n",
    );
    write(&root.join("upstream/policies/open-sans.toml"), &policy);
    let comp = component("open-sans", upstream.path(), &revision, "src/fonts/open-sans");
    import_component(root, &comp, false).unwrap();
    let imported = root.join("src/fonts/open-sans");
    assert!(imported.join("fonts/ttf/Sans.ttf").is_file() && imported.join("OFL.txt").is_file());
    assert!(!imported.join("sources").exists(), "omitted upstream paths are not imported");
    let state = read_sync_state(root, "open-sans").unwrap().unwrap();
    assert_eq!(state.imported_tree_digest_algorithm, SELECTED_IMPORTED_TREE_DIGEST_ALGORITHM);
    // The digest is the provenance audit's: selected ls-tree records.
    let expected = {
        let listing = run_cmd_capture(upstream.path(), "git", &["ls-tree", "-r", "HEAD"]).unwrap();
        let mut digest = Sha256Hasher::new();
        for line in listing.lines().filter(|line| !line.ends_with("sources/Sans.glyphs")) {
            digest.update(line.as_bytes());
            digest.update([0]);
        }
        format!("{:x}", digest.finalize())
    };
    assert_eq!(state.imported_tree_digest, expected);
    run_ok(root, "git", &["add", "-A"]);
    run_ok(root, "git", &["commit", "-q", "-m", "import"]);
    import_component(root, &comp, true).expect("an unedited selected tree re-syncs");

    // A retained path that disappears upstream is an error, not a silent shrink.
    let renamed = policy.replace("\"OFL.txt\"", "\"LICENSE.txt\"");
    write(&root.join("upstream/policies/open-sans.toml"), &renamed);
    let error = import_component(root, &comp, true).unwrap_err().to_string();
    assert!(error.contains("retained path does not exist upstream: LICENSE.txt"), "{error}");
}
