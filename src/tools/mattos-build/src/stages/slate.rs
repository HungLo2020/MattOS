fn build_slate(repo_root: &Path) -> Result<()> {
    build_slate_binary(repo_root, "slate", Vec::new())
}

fn build_slate_gui(repo_root: &Path) -> Result<()> {
    let root = repo_root.join("out/build/slate-gui");
    fs::create_dir_all(&root)?;
    let environment = slate_gui_environment(repo_root, &root)?;
    build_slate_binary(repo_root, "slate-gui", environment)
}

fn slate_gui_environment(repo_root: &Path, root: &Path) -> Result<Vec<(&'static str, String)>> {
    // The cmake crate accepts a toolchain file. Reuse the same closed
    // target search policy as MattOS's other Qt consumers: no host Qt,
    // headers, libraries or embedded output-directory RPATHs.
    let components = ["qtdeclarative", "qtshadertools"];
    let mut prefixes = kde_target_prefixes(repo_root, &components);
    let opengl = qt_opengl_bridge(repo_root)?;
    prefixes.push(opengl.clone());
    let toolchain = root.join("mattos-toolchain.cmake");
    let mut contents = String::new();
    let mut arguments = isolated_target_cmake_args(repo_root, &prefixes)?;
    arguments.extend([
        "-DCMAKE_FIND_PACKAGE_PREFER_CONFIG=ON".to_string(),
        format!("-DOpenGL_DIR={}", opengl.display()),
        // Standalone Qt modules have separate source-owned prefixes.
        // Allow searching those prefixes while the target-only root
        // modes above continue to prohibit any host package fallback.
        "-DQT_DISABLE_NO_DEFAULT_PATH_IN_QT_PACKAGES=ON".to_string(),
        // Slate embeds interpreted QML through rcc, not Qt's QML
        // compilation/type-registration macros. MattOS Qt Declarative
        // does not install a Qt6QmlTools CMake package. Required moc/rcc
        // targets still come from the staged Qt6CoreTools package.
        "-DQT_ALLOW_MISSING_TOOLS_PACKAGES=ON".to_string(),
    ]);
    for argument in arguments {
        if let Some((name, value)) = argument.strip_prefix("-D").and_then(|s| s.split_once('=')) {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
            contents.push_str(&format!(
                "set({name} \"{escaped}\" CACHE STRING \"MattOS target policy\" FORCE)\n"
            ));
        }
    }
    fs::write(&toolchain, contents)?;
    let mut environment = kde_target_environment(repo_root, &root, &components, true)?;
    environment.push(("CMAKE_TOOLCHAIN_FILE", toolchain.display().to_string()));
    environment.push(("CMAKE_GENERATOR", "Ninja".to_string()));
    Ok(environment)
}

fn build_slate_binary(
    repo_root: &Path,
    binary: &str,
    mut environment: Vec<(&str, String)>,
) -> Result<()> {
    let root = repo_root.join("out/build").join(binary);
    let source = root.join("source");
    let install = root.join("install");
    sync_build_source(&repo_root.join("src/userland/slate"), &source)?;
    isolate_cargo_build_mirror(&source)?;
    let target = root.join("cargo-target");
    environment.push(("CARGO_TARGET_DIR", target.display().to_string()));
    environment.push(("CARGO_INCREMENTAL", "0".to_string()));
    run_cmd_with_env_overrides(
        &source,
        "cargo",
        &["build", "--locked", "--release", "-p", binary],
        &environment,
    )?;
    remove_path_if_exists(&install)?;
    stage_output_file(
        &target.join("release").join(binary),
        &install.join("usr/bin").join(binary),
        0o755,
    )
}
