fn build_tar(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/tar");
    let paxutils = repo_root.join("src/build-support/paxutils");
    let gnulib = repo_root.join("src/build-support/gnulib");
    if !source.join("bootstrap").is_file() {
        bail!(
            "GNU tar source not found in {}; run upstream import tar first",
            source.display()
        );
    }
    if !paxutils.join("DISTFILES").is_file() {
        bail!(
            "GNU paxutils build support not found in {}; run upstream import paxutils first",
            paxutils.display()
        );
    }
    if !gnulib.join("gnulib-tool").is_file() {
        bail!(
            "pinned Gnulib build support not found in {}; run upstream import gnulib first",
            gnulib.display()
        );
    }
    let acl_install = repo_root.join("out/build/acl/install");
    let acl_libdir = acl_install.join("usr/lib/x86_64-linux-gnu");
    if !acl_install.join("usr/include/sys/acl.h").is_file()
        || !acl_libdir.join("libacl.so").exists()
    {
        bail!(
            "MattOS-built ACL development files missing at {}; run build acl first",
            acl_install.display()
        );
    }
    let out_root = repo_root.join("out/build/tar");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/tar.toml"))
        .context("failed to read GNU tar upstream state")?;
    let paxutils_state = fs::read_to_string(repo_root.join("upstream/state/paxutils.toml"))
        .context("failed to read paxutils upstream state")?;
    let gnulib_state = fs::read_to_string(repo_root.join("upstream/state/gnulib.toml"))
        .context("failed to read Gnulib upstream state")?;
    let acl_state = fs::read_to_string(repo_root.join("upstream/state/acl.toml"))
        .context("failed to read ACL upstream state")?;
    let options = [
        "--prefix=/usr",
        "--disable-nls",
        "--without-selinux",
        "--with-posix-acls",
    ];
    let stamp = format!(
        "{state}\n{paxutils_state}\n{gnulib_state}\n{acl_state}\n{}\n",
        options.join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    copy_imported_working_tree(repo_root, Path::new("src/userland/tar"), &source_copy)?;
    copy_imported_working_tree(
        repo_root,
        Path::new("src/build-support/paxutils"),
        &source_copy.join("paxutils"),
    )?;
    copy_imported_working_tree(
        repo_root,
        Path::new("src/build-support/gnulib"),
        &source_copy.join("gnulib"),
    )?;
    apply_component_patches(repo_root, "tar", &source_copy)?;
    if !source_copy.join("configure").is_file() {
        let gnulib_arg = format!("--gnulib-srcdir={}", source_copy.join("gnulib").display());
        run_cmd(
            &source_copy,
            "./bootstrap",
            &[
                "--gen",
                "--force",
                "--no-git",
                "--skip-po",
                "--copy",
                "--no-bootstrap-sync",
                &gnulib_arg,
            ],
        )?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    let include = acl_install.join("usr/include").display().to_string();
    let lib = acl_libdir.display().to_string();
    let pkgconfig = acl_libdir.join("pkgconfig").display().to_string();
    let configure_env = [
        ("CPPFLAGS", format!("-I{include}")),
        ("LDFLAGS", format!("-L{lib}")),
        ("LD_LIBRARY_PATH", lib.clone()),
        ("PKG_CONFIG_PATH", pkgconfig),
    ];
    if !build_dir.join("Makefile").is_file() {
        let configure = source_copy.join("configure");
        run_cmd_with_env_overrides(&build_dir, path_str(&configure)?, &options, &configure_env)?;
    }
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["-j", "4", "MAKEINFO=true"],
        &configure_env,
    )?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[
            "install",
            "MAKEINFO=true",
            &format!("DESTDIR={}", install_dir.display()),
        ],
        &configure_env,
    )?;
    let tar = install_dir.join("usr/bin/tar");
    if !tar.is_file() {
        bail!("GNU tar install did not produce {}", tar.display());
    }
    validate_dependency_resolves_from(&tar, "libacl.so.1", &acl_libdir, &[&acl_libdir])?;
    let needed = run_cmd_capture(repo_root, "readelf", &["-d", path_str(&tar)?])?;
    if needed.contains("libselinux.so") {
        bail!("MattOS GNU tar unexpectedly links against host SELinux");
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}
