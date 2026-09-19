fn build_greetd(repo_root: &Path) -> Result<()> {
    let root = repo_root.join("out/build/greetd");
    let source = root.join("source");
    let target = root.join("cargo-target");
    let install = root.join("install");
    sync_build_source(&repo_root.join("src/system/session/greetd"), &source)?;
    isolate_cargo_build_mirror(&source)?;
    remove_path_if_exists(&install)?;
    run_cmd_with_env_overrides(
        &source,
        "cargo",
        &[
            "build", "--locked", "--release", "-p", "greetd", "-p", "agreety",
        ],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
    )?;
    for binary in ["greetd", "agreety"] {
        stage_output_file(
            &target.join("release").join(binary),
            &install.join("usr/bin").join(binary),
            0o755,
        )?;
    }
    Ok(())
}
