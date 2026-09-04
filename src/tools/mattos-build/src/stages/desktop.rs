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
    // `staged_library_environment` intentionally replaces PATH with the
    // target-native tool directories.  COSMIC recipes invoke Cargo indirectly
    // through `just`, so preserve the source-ownership dispatcher explicitly:
    // otherwise those nested Cargo calls bypass output-mirror preparation and
    // can discover an unreconciled Cargo.lock after the stage cache planned a
    // normal rebuild.
    let dispatcher = repo_root.join("out/source-ownership/bin/cargo");
    let tool_path = cosmic_recipe_tool_path(&dispatcher, &just, inherited_path)?;
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

fn cosmic_recipe_tool_path(dispatcher: &Path, just: &Path, inherited_path: &str) -> Result<String> {
    let dispatcher_bin = dispatcher.parent().ok_or_else(|| {
        anyhow!(
            "source-ownership Cargo dispatcher has no parent: {}",
            dispatcher.display()
        )
    })?;
    if !dispatcher.is_file() {
        bail!(
            "COSMIC build requires the source-ownership Cargo dispatcher: {}",
            dispatcher.display()
        );
    }
    Ok(std::env::join_paths(
        [
            dispatcher_bin.to_path_buf(),
            just.parent().expect("just bin parent").to_path_buf(),
        ]
        .into_iter()
        .chain(std::env::split_paths(inherited_path)),
    )?
    .to_string_lossy()
    .to_string())
}

fn cosmic_native_components(stage: BuildStage) -> Vec<&'static str> {
    let mut components = vec!["glibc", "gcc-runtime"];
    components.extend(
        stage_graph::direct_dependencies(stage)
            .iter()
            .copied()
            .filter(|component| *component != "formal-sysroot"),
    );
    if stage == BuildStage::CosmicStorage {
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
            ] {
                copy_tree_contents(
                    &repo_root.join("out/build").join(component).join("install"),
                    &install,
                )?;
            }
        }
        BuildStage::CosmicRandr
        | BuildStage::CosmicScreenshot
        | BuildStage::PopLauncher
        | BuildStage::CosmicCalculator
        | BuildStage::CosmicStorage
        | BuildStage::CosmicMonitor => {
            let component = stage_graph::stage_id(stage);
            build_cosmic_just_component(repo_root, &install, component, &env)?;
        }
        BuildStage::CosmicStore => {
            build_cosmic_just_component(repo_root, &install, "cosmic-store", &env)?;
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

#[cfg(test)]
mod desktop_tests {
    use super::*;

    #[test]
    fn cosmic_recipe_path_keeps_owned_cargo_ahead_of_just_and_native_tools() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let dispatcher = root.join("out/source-ownership/bin/cargo");
        let just = root.join("tools/just/bin/just");
        fs::create_dir_all(dispatcher.parent().unwrap()).unwrap();
        fs::create_dir_all(just.parent().unwrap()).unwrap();
        fs::write(&dispatcher, "dispatcher").unwrap();
        fs::write(&just, "just").unwrap();

        let inherited = std::env::join_paths([root.join("native/bin")]).unwrap();
        let path = cosmic_recipe_tool_path(&dispatcher, &just, &inherited.to_string_lossy())
            .unwrap();
        let entries = std::env::split_paths(&path).collect::<Vec<_>>();
        assert_eq!(entries[0], dispatcher.parent().unwrap());
        assert_eq!(entries[1], just.parent().unwrap());
        assert_eq!(entries[2], root.join("native/bin"));
    }

    #[test]
    fn cosmic_recipe_path_fails_closed_without_the_owned_dispatcher() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let dispatcher = root.join("out/source-ownership/bin/cargo");
        let just = root.join("tools/just/bin/just");
        fs::create_dir_all(just.parent().unwrap()).unwrap();
        fs::write(&just, "just").unwrap();

        let error = cosmic_recipe_tool_path(&dispatcher, &just, "").unwrap_err();
        assert!(error.to_string().contains("source-ownership Cargo dispatcher"));
    }
}
