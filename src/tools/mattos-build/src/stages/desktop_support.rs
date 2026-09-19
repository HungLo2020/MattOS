const POP_FIRA_RUNTIME_FONTS: &[&str] = &[
    "FiraSans-Regular.otf",
    "FiraSans-Bold.otf",
    "FiraSans-Italic.otf",
    "FiraSans-BoldItalic.otf",
    "FiraMono-Regular.otf",
    "FiraMono-Medium.otf",
    "FiraMono-Bold.otf",
];

fn build_pop_fonts(repo_root: &Path) -> Result<()> {
    let install = repo_root.join("out/build/pop-fonts/install");
    remove_path_if_exists(&install)?;
    let source = repo_root.join("src/desktop/fonts/pop-fonts/fira");
    let destination = install.join("usr/share/fonts/opentype/fira");
    for font in POP_FIRA_RUNTIME_FONTS {
        stage_output_file(&source.join(font), &destination.join(font), 0o644)?;
    }
    stage_output_file(
        &source.join("SIL Open Font License.txt"),
        &install.join("usr/share/doc/fonts-fira/copyright"),
        0o644,
    )
}

fn build_cozy(repo_root: &Path) -> Result<()> {
    let out_root = repo_root.join("out/build/cozy");
    let install = out_root.join("install");
    let mirror = out_root.join("source");
    remove_path_if_exists(&install)?;
    sync_build_source(&repo_root.join("src/userland/cozy"), &mirror)?;
    isolate_cargo_build_mirror(&mirror)?;
    let target = out_root.join("cargo-target");
    run_cmd_with_env_overrides(
        &mirror,
        "cargo",
        &["build", "--locked", "--release", "--bin", "cozy"],
        &[
            ("CARGO_TARGET_DIR", target.display().to_string()),
            ("CARGO_BUILD_JOBS", "4".to_string()),
            ("CARGO_INCREMENTAL", "0".to_string()),
            (
                "RUSTFLAGS",
                format!(
                    "--remap-path-prefix={}=/usr/src/mattos",
                    repo_root.display()
                ),
            ),
        ],
    )?;
    stage_output_file(
        &target.join("release/cozy"),
        &install.join("usr/bin/cozy"),
        0o755,
    )
}
