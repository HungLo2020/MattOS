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

fn build_material_cursors(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/desktop/themes/material-cursors");
    let out = repo_root.join("out/build/material-cursors");
    let selected_source = out.join("source/material_light_cursors");
    let dist = out.join("dist");
    let build = out.join("build");
    let home = out.join("home");
    let install = out.join("install/usr/share/icons/material_light_cursors");
    let xcursorgen_source = repo_root.join("src/build-tools/xcursorgen");
    let host_tools = repo_root.join("out/host-tools/xcursorgen-1.0.9");
    let host_bin = host_tools.join("bin");
    let host_xcursorgen = host_bin.join("xcursorgen");
    remove_path_if_exists(&out)?;
    remove_path_if_exists(&host_tools)?;
    fs::create_dir_all(&selected_source)?;
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&host_bin)?;
    packaging::copy_tree_preserving(
        &source.join("src/material_light_cursors"),
        &selected_source,
    )?;

    // xcursorgen is a source-pinned host build tool, not a MattOS runtime
    // package. Its host dependencies and output never enter target ownership.
    let meson_build = out.join("xcursorgen-build");
    let host_pkgconfig = "/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig";
    let mut setup = Command::new("meson");
    setup
        .arg("setup")
        .arg(&meson_build)
        .arg(&xcursorgen_source)
        .arg(format!("--prefix={}", host_tools.display()))
        .arg("--bindir=bin")
        .arg("--libdir=lib")
        .arg("--buildtype=release")
        .arg("--strip")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("CC", "gcc")
        .env("PKG_CONFIG_PATH", "")
        .env("PKG_CONFIG_LIBDIR", host_pkgconfig);
    let status = performance::run_logged_command(&mut setup, "xcursorgen host meson setup")?;
    if !status.success() {
        bail!("pinned xcursorgen host-tool configuration failed: {status}");
    }
    let mut compile = Command::new("meson");
    compile
        .arg("compile")
        .arg("-C")
        .arg(&meson_build)
        .arg("--jobs")
        .arg("2")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("CC", "gcc")
        .env("PKG_CONFIG_PATH", "")
        .env("PKG_CONFIG_LIBDIR", host_pkgconfig);
    let status = performance::run_logged_command(&mut compile, "xcursorgen host meson compile")?;
    if !status.success() {
        bail!("pinned xcursorgen host-tool compilation failed: {status}");
    }
    let mut install_tool = Command::new("meson");
    install_tool
        .arg("install")
        .arg("-C")
        .arg(&meson_build)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC");
    let status = performance::run_logged_command(&mut install_tool, "xcursorgen host meson install")?;
    if !status.success() || !host_xcursorgen.is_file() {
        bail!("pinned xcursorgen host-tool installation failed: {status}");
    }

    // The upstream cursor script uses Inkscape's historical CLI. Adapt that
    // interface to the required ImageMagick SVG renderer in an output-owned
    // wrapper, leaving the imported source pristine.
    let inkscape_wrapper = host_bin.join("inkscape");
    fs::write(
        &inkscape_wrapper,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
    echo 'Inkscape 1.4.3-compatible (ImageMagick SVG renderer)'
    exit 0
fi
output= width= height= input=
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o|-e) output=$2; shift 2 ;;
        -w) width=$2; shift 2 ;;
        -h) height=$2; shift 2 ;;
        -z) shift ;;
        *) input=$1; shift ;;
    esac
done
[ -n "$output" ] && [ -n "$width" ] && [ -n "$height" ] && [ -n "$input" ] || exit 2
exec /usr/bin/magick -background none "$input" -resize "${width}x${height}" "$output"
"#,
    )?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&inkscape_wrapper, fs::Permissions::from_mode(0o755))?;

    // Generate all raster/cursor files beneath out/, never in vendored source.
    let mut command = Command::new("bash");
    command
        .arg(source.join("build.sh"))
        .current_dir(&source)
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", host_bin.display()))
        .env("HOME", &home)
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH)
        .env("SRC_DIR", out.join("source"))
        .env("OUT_DIR", &dist)
        .env("BUILD_DIR", &build)
        .env("ALIASES", source.join("src/cursorList"))
        .env("CONFIG_DIR", source.join("src/config"));
    let status = performance::run_logged_command(&mut command, "material-cursors build.sh")?;
    if !status.success() {
        bail!("upstream material-cursors asset compilation failed: {status}");
    }
    packaging::copy_tree_preserving(&dist.join("material_light_cursors"), &install)?;
    for required in ["index.theme", "cursors/left_ptr", "cursors/wait"] {
        if !install.join(required).is_file() {
            bail!("material-cursors output is missing required {required}");
        }
    }
    Ok(())
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
