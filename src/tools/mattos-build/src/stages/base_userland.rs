fn build_brush(repo_root: &Path) -> Result<()> {
    let source_relative = Path::new("src/userland/brush");
    let brush = repo_root.join(source_relative);
    if !brush.join("Cargo.toml").exists() {
        bail!(
            "brush source not found in {}; run import first",
            brush.display()
        );
    }
    let out_root = repo_root.join("out/build/brush");
    let source_copy = out_root.join("source");
    let target = out_root.join("cargo-target");
    copy_imported_working_tree(repo_root, source_relative, &source_copy)?;
    apply_component_patches(repo_root, "brush", &source_copy)?;
    run_cmd_with_env_overrides(
        &source_copy,
        "cargo",
        &["build", "--locked", "--release", "-p", "brush"],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
    )
}

fn build_coreutils(repo_root: &Path) -> Result<()> {
    let coreutils = repo_root.join("src/userland/coreutils");
    if !coreutils.join("Cargo.toml").exists() {
        bail!(
            "coreutils source not found in {}; run import first",
            coreutils.display()
        );
    }
    let target = repo_root.join("out/build/coreutils/cargo-target");
    run_cmd_with_env_overrides(
        &coreutils,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "-p",
            "coreutils",
            "--no-default-features",
            "--features",
            "unix",
        ],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
    )
}

fn build_grep(repo_root: &Path) -> Result<()> {
    let grep = repo_root.join("src/userland/grep");
    if !grep.join("Cargo.toml").exists() {
        bail!(
            "grep source not found in {}; run import first",
            grep.display()
        );
    }
    let target = repo_root.join("out/build/grep/cargo-target");
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/userland/grep/Cargo.toml",
            "--bin",
            "grep",
        ],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
    )
}

fn build_sed(repo_root: &Path) -> Result<()> {
    let sed = repo_root.join("src/userland/sed");
    if !sed.join("Cargo.toml").exists() {
        bail!(
            "sed source not found in {}; run import first",
            sed.display()
        );
    }
    let target = repo_root.join("out/build/sed/cargo-target");
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/userland/sed/Cargo.toml",
            "--bin",
            "sed",
        ],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
    )
}

fn build_findutils(repo_root: &Path) -> Result<()> {
    let findutils = repo_root.join("src/userland/findutils");
    if !findutils.join("Cargo.toml").exists() {
        bail!(
            "findutils source not found in {}; run import first",
            findutils.display()
        );
    }
    let target = repo_root.join("out/build/findutils/cargo-target");
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/userland/findutils/Cargo.toml",
            "--bins",
        ],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
    )
}

fn build_diffutils(repo_root: &Path) -> Result<()> {
    let diffutils = repo_root.join("src/userland/diffutils");
    if !diffutils.join("Cargo.toml").exists() {
        bail!(
            "diffutils source not found in {}; run import first",
            diffutils.display()
        );
    }
    let target = repo_root.join("out/build/diffutils/cargo-target");
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/userland/diffutils/Cargo.toml",
            "--bin",
            "diffutils",
        ],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
    )
}

fn build_init(repo_root: &Path) -> Result<()> {
    run_cmd(
        repo_root,
        "cargo",
        &[
            "build",
            "--release",
            "--manifest-path",
            "src/userland/init/Cargo.toml",
        ],
    )
}

fn build_linux_pam(repo_root: &Path) -> Result<()> {
    let pam_src = repo_root.join("src/system/auth/linux-pam");
    if !pam_src.join("meson.build").exists() {
        bail!(
            "linux-pam source not found in {}; run upstream import linux-pam first",
            pam_src.display()
        );
    }

    let out_root = repo_root.join("out/build/linux-pam");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let options_path = out_root.join("meson-options.txt");
    let libxcrypt = repo_root.join("out/build/libxcrypt/install/usr");
    let libxcrypt_lib = libxcrypt.join("lib/x86_64-linux-gnu");
    if !libxcrypt.join("include/crypt.h").is_file() || !libxcrypt_lib.join("libcrypt.so").exists() {
        bail!("MattOS-built libxcrypt development files are missing; run build libxcrypt first");
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;

    let options = linux_pam_meson_options();
    let env_overrides = [
        (
            "CPPFLAGS",
            format!("-I{}", libxcrypt.join("include").display()),
        ),
        ("LDFLAGS", format!("-L{}", libxcrypt_lib.display())),
        ("LIBRARY_PATH", libxcrypt_lib.display().to_string()),
        ("LD_LIBRARY_PATH", libxcrypt_lib.display().to_string()),
        (
            "PKG_CONFIG_PATH",
            libxcrypt_lib.join("pkgconfig").display().to_string(),
        ),
    ];
    let options_text = format!(
        "{}\n{}\n",
        options.join("\n"),
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let existing_options = fs::read_to_string(&options_path).ok();
    let needs_reconfigure = existing_options.as_deref() != Some(options_text.as_str());
    if needs_reconfigure && build_dir.exists() {
        fs::remove_dir_all(&build_dir)
            .with_context(|| format!("failed to reset {}", build_dir.display()))?;
    }
    let configured = build_dir.join("build.ninja").exists();

    if !configured {
        let mut setup_args = vec![
            "setup".to_string(),
            build_dir.display().to_string(),
            pam_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
    } else {
        // Meson build.dat is not portable across Meson versions. Reconfigure
        // an existing tree on every invocation so a host Meson upgrade cannot
        // leave this stage with stale serialized build data.
        let mut setup_args = vec![
            "setup".to_string(),
            "--reconfigure".to_string(),
            build_dir.display().to_string(),
            pam_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        if needs_reconfigure {
            fs::write(&options_path, &options_text)
                .with_context(|| format!("failed to write {}", options_path.display()))?;
        }
    }

    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &[
            "compile",
            "-C",
            build_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid linux-pam build dir"))?,
        ],
        &env_overrides,
    )?;

    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)
            .with_context(|| format!("failed to clean {}", install_dir.display()))?;
    }
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;

    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            build_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid linux-pam build dir"))?,
            "--no-rebuild",
            "--destdir",
            install_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid linux-pam install dir"))?,
        ],
        &env_overrides,
    )?;

    let pam_lib = install_dir.join("usr/lib/x86_64-linux-gnu/libpam.so.0");
    if !pam_lib.exists() {
        bail!("linux-pam install did not produce {}", pam_lib.display());
    }
    for rel in [
        "usr/lib/x86_64-linux-gnu/security/pam_unix.so",
        "usr/sbin/unix_chkpwd",
    ] {
        validate_dependency_resolves_from(
            &install_dir.join(rel),
            "libcrypt.so.1",
            &libxcrypt_lib,
            &[&libxcrypt_lib],
        )?;
    }
    println!("Linux-PAM libcrypt origin: {}", libxcrypt_lib.display());

    Ok(())
}

fn linux_pam_meson_options() -> Vec<String> {
    vec![
        "--prefix=/usr".to_string(),
        "--sysconfdir=/etc".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        "-Ddocs=disabled".to_string(),
        "-Di18n=disabled".to_string(),
        "-Daudit=disabled".to_string(),
        "-Dselinux=disabled".to_string(),
        "-Dlogind=disabled".to_string(),
        "-Delogind=disabled".to_string(),
        "-Deconf=disabled".to_string(),
        "-Dexamples=false".to_string(),
        "-Dxtests=false".to_string(),
        "-Dsecuredir=/usr/lib/x86_64-linux-gnu/security".to_string(),
    ]
}

fn build_shadow(repo_root: &Path) -> Result<()> {
    let shadow_src = repo_root.join("src/system/auth/shadow");
    if !shadow_src.join("configure.ac").exists() {
        bail!(
            "shadow source not found in {}; run upstream import shadow first",
            shadow_src.display()
        );
    }

    let out_root = repo_root.join("out/build/shadow");
    let source = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp = build_dir.join("config.stamp");
    let man_po_makefile = ensure_shadow_man_po_makefile(repo_root)?;
    remove_path_if_exists(&out_root)?;
    copy_imported_working_tree(repo_root, Path::new("src/system/auth/shadow"), &source)?;
    fs::copy(&man_po_makefile, source.join("man/po/Makefile.in")).with_context(|| {
        format!(
            "failed to stage {} into output-owned Shadow source mirror",
            man_po_makefile.display()
        )
    })?;
    if !source.join("configure").exists() {
        run_cmd(&source, "autoreconf", &["-v", "-f", "-i"])?;
    }
    let configure_args = [
        "--prefix=/usr",
        "--sysconfdir=/etc",
        "--disable-nls",
        "--with-libpam",
        "--with-libbsd",
        "--without-selinux",
        "--disable-logind",
        "--with-yescrypt",
        "--without-btrfs",
        "--without-nscd",
        "--without-sssd",
    ];
    let pam_install = repo_root.join("out/build/linux-pam/install");
    let pam_include = pam_install.join("usr/include");
    let pam_lib = pam_install.join("usr/lib/x86_64-linux-gnu");
    let pam_pkgconfig = pam_lib.join("pkgconfig");
    let libbsd_install = repo_root.join("out/build/libbsd/install/usr");
    let libbsd_include = libbsd_install.join("include");
    let libbsd_lib = libbsd_install.join("lib/x86_64-linux-gnu");
    let libmd_lib = repo_root.join("out/build/libmd/install/usr/lib/x86_64-linux-gnu");
    let libxcrypt_install = repo_root.join("out/build/libxcrypt/install/usr");
    let libxcrypt_lib = libxcrypt_install.join("lib/x86_64-linux-gnu");
    if !pam_include.join("security/pam_appl.h").exists() || !pam_lib.join("libpam.so").exists() {
        bail!(
            "linux-pam development files missing at {}; run build pam first",
            pam_install.display()
        );
    }
    if !libbsd_include.join("bsd/readpassphrase.h").is_file()
        || !libbsd_lib.join("libbsd.so").exists()
        || !libmd_lib.join("libmd.so").exists()
    {
        bail!(
            "MattOS-built libbsd/libmd development files missing; run build libmd and build libbsd first"
        );
    }
    if !libxcrypt_install.join("include/crypt.h").is_file()
        || !libxcrypt_lib.join("libcrypt.so").exists()
    {
        bail!("MattOS-built libxcrypt development files missing; run build libxcrypt first");
    }
    let library_path = std::env::join_paths([&pam_lib, &libbsd_lib, &libmd_lib, &libxcrypt_lib])?
        .to_string_lossy()
        .to_string();
    let pkgconfig_path = std::env::join_paths([
        &pam_pkgconfig,
        &libbsd_lib.join("pkgconfig"),
        &libmd_lib.join("pkgconfig"),
        &libxcrypt_lib.join("pkgconfig"),
    ])?
    .to_string_lossy()
    .to_string();
    let env_overrides = vec![
        (
            "CPPFLAGS",
            format!(
                "-I{} -I{} -I{} -I{} -DLIBBSD_OVERLAY",
                pam_include.display(),
                libbsd_include.display(),
                libbsd_include.join("bsd").display(),
                libxcrypt_install.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!(
                "-L{} -L{} -L{} -L{}",
                pam_lib.display(),
                libbsd_lib.display(),
                libmd_lib.display(),
                libxcrypt_lib.display()
            ),
        ),
        (
            "LIBBSD_CFLAGS",
            format!(
                "-I{} -DLIBBSD_OVERLAY",
                libbsd_include.join("bsd").display()
            ),
        ),
        ("LIBBSD_LIBS", format!("-L{} -lbsd", libbsd_lib.display())),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        ("PKG_CONFIG_PATH", pkgconfig_path),
    ];
    let config_text = format!(
        "{}\n{}",
        configure_args.join("\n"),
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if stamp.exists() && fs::read_to_string(&stamp).ok().as_deref() != Some(config_text.as_str()) {
        fs::remove_dir_all(&build_dir)
            .with_context(|| format!("failed to reset {}", build_dir.display()))?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;

    if !stamp.exists() {
        run_cmd_with_env_overrides(
            &build_dir,
            source
                .join("configure")
                .to_str()
                .ok_or_else(|| anyhow!("invalid shadow configure path"))?,
            &configure_args,
            &env_overrides,
        )?;
        fs::write(&stamp, &config_text)
            .with_context(|| format!("failed to write {}", stamp.display()))?;
    }

    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env_overrides)?;

    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)
            .with_context(|| format!("failed to clean {}", install_dir.display()))?;
    }
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;

    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[
            "install",
            &format!(
                "DESTDIR={}",
                install_dir
                    .to_str()
                    .ok_or_else(|| anyhow!("invalid shadow install dir"))?
            ),
        ],
        &env_overrides,
    )?;

    let passwd_bin = install_dir.join("usr/bin/passwd");
    if !passwd_bin.exists() {
        bail!("shadow install did not produce {}", passwd_bin.display());
    }
    let shadow_lib_dirs: [&Path; 3] = [&libbsd_lib, &libmd_lib, &libxcrypt_lib];
    for rel in [
        "usr/bin/chage",
        "usr/bin/newgrp",
        "usr/bin/passwd",
        "usr/sbin/chpasswd",
        "usr/sbin/groupadd",
        "usr/sbin/groupdel",
        "usr/sbin/groupmod",
        "usr/sbin/useradd",
        "usr/sbin/userdel",
        "usr/sbin/usermod",
    ] {
        validate_dependency_resolves_from(
            &install_dir.join(rel),
            "libbsd.so.0",
            &libbsd_lib,
            &shadow_lib_dirs,
        )?;
    }
    for rel in ["usr/bin/newgrp", "usr/bin/passwd", "usr/sbin/chpasswd"] {
        validate_dependency_resolves_from(
            &install_dir.join(rel),
            "libcrypt.so.1",
            &libxcrypt_lib,
            &shadow_lib_dirs,
        )?;
    }
    println!(
        "Shadow origins: libbsd={} transitive-libmd={} libcrypt={}",
        libbsd_lib.display(),
        libmd_lib.display(),
        libxcrypt_lib.display()
    );

    Ok(())
}

fn ensure_shadow_man_po_makefile(repo_root: &Path) -> Result<PathBuf> {
    let cache = repo_root
        .join("out/cache/shadow")
        .join(SHADOW_UPSTREAM_COMMIT);
    let file = cache.join("man-po-Makefile.in");
    if file.is_file() {
        let actual = performance::sha256_file(&file)?;
        if actual != SHADOW_MAN_PO_MAKEFILE_SHA256 {
            bail!(
                "cached Shadow man/po/Makefile.in checksum mismatch: expected {}, got {} at {}",
                SHADOW_MAN_PO_MAKEFILE_SHA256,
                actual,
                file.display()
            );
        }
        return Ok(file);
    }

    fs::create_dir_all(&cache).with_context(|| format!("failed to create {}", cache.display()))?;
    let git_dir = repo_root.join("out/cache/shadow/upstream.git");
    if !git_dir.is_dir() {
        run_cmd(repo_root, "git", &["init", "--bare", path_str(&git_dir)?])?;
    }
    let git_dir_arg = format!("--git-dir={}", git_dir.display());
    run_cmd(
        repo_root,
        "git",
        &[
            git_dir_arg.as_str(),
            "fetch",
            "--depth=1",
            SHADOW_UPSTREAM_REPOSITORY,
            SHADOW_UPSTREAM_COMMIT,
        ],
    )?;
    let object = format!("{SHADOW_UPSTREAM_COMMIT}:man/po/Makefile.in");
    let output = Command::new("git")
        .args([git_dir_arg.as_str(), "show", object.as_str()])
        .output()
        .context("failed to read man/po/Makefile.in from pinned Shadow commit")?;
    if !output.status.success() {
        bail!(
            "pinned Shadow commit did not provide man/po/Makefile.in: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let temp = file.with_extension("tmp");
    fs::write(&temp, &output.stdout)
        .with_context(|| format!("failed to write {}", temp.display()))?;
    let actual = performance::sha256_file(&temp)?;
    if actual != SHADOW_MAN_PO_MAKEFILE_SHA256 {
        let _ = fs::remove_file(&temp);
        bail!(
            "downloaded Shadow man/po/Makefile.in checksum mismatch: expected {}, got {}",
            SHADOW_MAN_PO_MAKEFILE_SHA256,
            actual
        );
    }
    fs::rename(&temp, &file).with_context(|| format!("failed to publish {}", file.display()))?;
    Ok(file)
}

fn build_sudo_rs(repo_root: &Path) -> Result<()> {
    let sudo_src = repo_root.join("src/system/auth/sudo-rs");
    if !sudo_src.join("Cargo.toml").exists() {
        bail!(
            "sudo-rs source not found in {}; run upstream import sudo-rs first",
            sudo_src.display()
        );
    }

    let pam_install = repo_root.join("out/build/linux-pam/install");
    let pam_lib = pam_install.join("usr/lib/x86_64-linux-gnu");
    if !pam_lib.join("libpam.so").exists() && !pam_lib.join("libpam.so.0").exists() {
        bail!(
            "linux-pam libraries missing at {}; run build pam first",
            pam_lib.display()
        );
    }
    let current_rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
    let rustflags = if current_rustflags.is_empty() {
        format!("-L native={}", pam_lib.display())
    } else {
        format!("-L native={} {current_rustflags}", pam_lib.display())
    };
    let current_library_path = std::env::var("LIBRARY_PATH").unwrap_or_default();
    let library_path = if current_library_path.is_empty() {
        pam_lib.display().to_string()
    } else {
        format!("{}:{current_library_path}", pam_lib.display())
    };
    let target = repo_root.join("out/build/sudo-rs/cargo-target");
    let env_overrides = vec![
        ("RUSTFLAGS", rustflags),
        ("LIBRARY_PATH", library_path),
        ("CARGO_TARGET_DIR", target.display().to_string()),
    ];

    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/system/auth/sudo-rs/Cargo.toml",
            "--bin",
            "sudo",
            "--bin",
            "visudo",
        ],
        &env_overrides,
    )?;

    let out_root = repo_root.join("out/build/sudo-rs");
    let install_dir = out_root.join("install");
    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)
            .with_context(|| format!("failed to clean {}", install_dir.display()))?;
    }
    fs::create_dir_all(install_dir.join("usr/bin"))
        .with_context(|| format!("failed to create {}", install_dir.join("usr/bin").display()))?;

    for bin in ["sudo", "visudo"] {
        let src = target.join("release").join(bin);
        if !src.exists() {
            bail!("sudo-rs build did not produce {}", src.display());
        }
        let dst = install_dir.join("usr/bin").join(bin);
        fs::copy(&src, &dst).with_context(|| format!("failed to copy {}", src.display()))?;
    }

    Ok(())
}

fn build_util_linux(repo_root: &Path) -> Result<()> {
    let authoritative_source = repo_root.join("src/userland/util-linux");
    if !authoritative_source.join("meson.build").exists() {
        bail!(
            "util-linux source not found in {}; run upstream import util-linux first",
            authoritative_source.display()
        );
    }

    let out_root = repo_root.join("out/build/util-linux");
    let util_linux_src = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let options_path = out_root.join("meson-options.txt");
    let env_path = out_root.join("meson-env.txt");
    let pam_install = repo_root.join("out/build/linux-pam/install");
    let pam_pkgconfig = pam_install.join("usr/lib/x86_64-linux-gnu/pkgconfig");
    let pam_include = pam_install.join("usr/include");
    let pam_lib = pam_install.join("usr/lib/x86_64-linux-gnu");
    let selinux_install = repo_root.join("out/build/selinux/install/usr");
    let selinux_pkgconfig = selinux_install.join("lib/x86_64-linux-gnu/pkgconfig");
    let selinux_include = selinux_install.join("include");
    let selinux_lib = selinux_install.join("lib/x86_64-linux-gnu");
    let pcre2_install = repo_root.join("out/build/pcre2/install/usr");
    let pcre2_pkgconfig = pcre2_install.join("lib/x86_64-linux-gnu/pkgconfig");
    let pcre2_include = pcre2_install.join("include");
    let pcre2_lib = pcre2_install.join("lib/x86_64-linux-gnu");
    let ncurses_install = repo_root.join("out/build/ncurses/install/usr");
    let ncurses_pkgconfig = ncurses_install.join("lib/x86_64-linux-gnu/pkgconfig");
    let ncurses_include = ncurses_install.join("include");
    let ncurses_lib = ncurses_install.join("lib/x86_64-linux-gnu");
    if !pam_pkgconfig.exists() {
        bail!(
            "linux-pam pkg-config directory missing at {}; run build pam first",
            pam_pkgconfig.display()
        );
    }
    if !selinux_lib.join("libselinux.so.1").exists() || !pcre2_lib.join("libpcre2-8.so.0").exists()
    {
        bail!("staged SELinux/PCRE2 libraries are missing; run build selinux first");
    }

    let current_pkg_config = std::env::var("PKG_CONFIG_PATH").unwrap_or_default();
    let staged_pkg_config = std::env::join_paths([
        &pam_pkgconfig,
        &selinux_pkgconfig,
        &pcre2_pkgconfig,
        &ncurses_pkgconfig,
    ])?
    .to_string_lossy()
    .to_string();
    let pkg_config_path = if current_pkg_config.is_empty() {
        staged_pkg_config
    } else {
        format!("{staged_pkg_config}:{current_pkg_config}")
    };
    let current_cflags = std::env::var("CFLAGS").unwrap_or_default();
    let staged_cflags = format!(
        "-I{} -I{} -I{} -I{}",
        pam_include.display(),
        selinux_include.display(),
        pcre2_include.display(),
        ncurses_include.display()
    );
    let cflags = if current_cflags.is_empty() {
        staged_cflags
    } else {
        format!("{staged_cflags} {current_cflags}")
    };
    let current_ldflags = std::env::var("LDFLAGS").unwrap_or_default();
    let staged_ldflags = format!(
        "-L{} -L{} -L{} -L{}",
        pam_lib.display(),
        selinux_lib.display(),
        pcre2_lib.display(),
        ncurses_lib.display()
    );
    let ldflags = if current_ldflags.is_empty() {
        staged_ldflags
    } else {
        format!("{staged_ldflags} {current_ldflags}")
    };
    let library_path = std::env::join_paths([&pam_lib, &selinux_lib, &pcre2_lib, &ncurses_lib])?
        .to_string_lossy()
        .to_string();
    let env_overrides = vec![
        ("PKG_CONFIG_PATH", pkg_config_path),
        ("CFLAGS", cflags),
        ("LDFLAGS", ldflags),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
    ];
    let env_text = format!(
        "{}\n",
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let existing_env = fs::read_to_string(&env_path).ok();
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&authoritative_source, &util_linux_src)?;
    apply_component_patches(repo_root, "util-linux", &util_linux_src)?;

    let options = util_linux_meson_options();
    let options_text = format!(
        "policy=base-userland-output-mirror-v2\n{}\n",
        options.join("\n")
    );
    let existing_options = fs::read_to_string(&options_path).ok();
    let needs_reconfigure = existing_options.as_deref() != Some(options_text.as_str());
    let env_changed = existing_env.as_deref() != Some(env_text.as_str());
    let mut configured = build_dir.join("build.ninja").exists();

    if configured && (env_changed || needs_reconfigure) {
        fs::remove_dir_all(&build_dir)
            .with_context(|| format!("failed to reset {}", build_dir.display()))?;
        configured = false;
    }

    if !configured {
        let mut setup_args = vec![
            "setup".to_string(),
            build_dir.display().to_string(),
            util_linux_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
        fs::write(&env_path, &env_text)
            .with_context(|| format!("failed to write {}", env_path.display()))?;
    }

    run_cmd_with_env_overrides(
        repo_root,
        "ninja",
        &[
            "-C",
            build_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid util-linux build dir"))?,
        ],
        &env_overrides,
    )?;

    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)
            .with_context(|| format!("failed to clean {}", install_dir.display()))?;
    }
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;

    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            build_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid util-linux build dir"))?,
            "--no-rebuild",
            "--destdir",
            install_dir
                .to_str()
                .ok_or_else(|| anyhow!("invalid util-linux install dir"))?,
        ],
        &env_overrides,
    )?;
    rewrite_staged_pkgconfig_files(&install_dir)?;

    for path in [
        install_dir.join("usr/sbin/agetty"),
        install_dir.join("usr/bin/login"),
        install_dir.join("usr/bin/su"),
        install_dir.join("usr/bin/mount"),
        install_dir.join("usr/bin/umount"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libblkid.so.1"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libmount.so.1"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libsmartcols.so.1"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libuuid.so.1"),
        install_dir.join("usr/lib/x86_64-linux-gnu/libfdisk.so.1"),
        install_dir.join("usr/bin/lsblk"),
        install_dir.join("usr/bin/dmesg"),
        install_dir.join("usr/sbin/fdisk"),
        install_dir.join("usr/sbin/sfdisk"),
        install_dir.join("usr/sbin/cfdisk"),
        install_dir.join("usr/sbin/wipefs"),
    ] {
        if !path.exists() {
            bail!("util-linux install did not produce {}", path.display());
        }
    }
    let util_linux_lib = install_dir.join("usr/lib/x86_64-linux-gnu");
    let runtime_dirs: [&Path; 4] = [&util_linux_lib, &selinux_lib, &pcre2_lib, &pam_lib];
    validate_dependency_resolves_from(
        &install_dir.join("usr/lib/x86_64-linux-gnu/libmount.so.1"),
        "libblkid.so.1",
        &util_linux_lib,
        &runtime_dirs,
    )?;
    validate_dependency_resolves_from(
        &install_dir.join("usr/bin/mount"),
        "libmount.so.1",
        &util_linux_lib,
        &runtime_dirs,
    )?;
    let mount_strings = run_cmd_capture(
        repo_root,
        "strings",
        &[path_str(&install_dir.join("usr/bin/mount"))?],
    )?;
    if !mount_strings.contains("libselinux.so.1") {
        bail!("util-linux mount lost its configured SELinux compatibility loader");
    }

    Ok(())
}

fn util_linux_meson_options() -> Vec<String> {
    vec![
        "--prefix=/usr".to_string(),
        "--sbindir=/usr/sbin".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        "--auto-features=disabled".to_string(),
        "-Dbuild-agetty=enabled".to_string(),
        "-Dbuild-login=enabled".to_string(),
        "-Dbuild-su=enabled".to_string(),
        "-Dbuild-libblkid=enabled".to_string(),
        "-Dbuild-libmount=enabled".to_string(),
        "-Dbuild-libsmartcols=enabled".to_string(),
        "-Dbuild-libuuid=enabled".to_string(),
        "-Dbuild-libfdisk=enabled".to_string(),
        "-Dbuild-mount=enabled".to_string(),
        "-Dbuild-fdisks=enabled".to_string(),
        "-Dbuild-losetup=enabled".to_string(),
        "-Dbuild-lsns=enabled".to_string(),
        "-Dbuild-wipefs=enabled".to_string(),
        "-Dbuild-mountpoint=enabled".to_string(),
        "-Dbuild-unshare=enabled".to_string(),
        "-Dbuild-nsenter=enabled".to_string(),
        "-Dbuild-blockdev=enabled".to_string(),
        "-Dbuild-lsblk=enabled".to_string(),
        "-Dbuild-lslocks=enabled".to_string(),
        "-Dbuild-findmnt=enabled".to_string(),
        "-Dbuild-flock=enabled".to_string(),
        "-Dbuild-dmesg=enabled".to_string(),
        "-Dbuild-schedutils=enabled".to_string(),
        "-Dbuild-prlimit=enabled".to_string(),
        "-Dbuild-lscpu=enabled".to_string(),
        "-Dncursesw=enabled".to_string(),
        "-Dselinux=enabled".to_string(),
        "-Dsystemd=disabled".to_string(),
        "-Dnls=disabled".to_string(),
        "-Dbuild-bash-completion=disabled".to_string(),
        "-Dbuild-python=disabled".to_string(),
        "-Dbuild-pylibmount=disabled".to_string(),
    ]
}

fn build_kmod(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/kmod");
    if !source.join("meson.build").exists() {
        bail!(
            "kmod source not found in {}; run upstream import kmod first",
            source.display()
        );
    }

    let out_root = repo_root.join("out/build/kmod");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let options_path = out_root.join("meson-options.txt");
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    let options = kmod_meson_options();
    let options_text = format!("{}\n", options.join("\n"));
    let configured = build_dir.join("build.ninja").exists();
    let changed = fs::read_to_string(&options_path).ok().as_deref() != Some(options_text.as_str());

    if !configured {
        let mut args = vec!["setup".to_string()];
        args.push(build_dir.display().to_string());
        args.push(source.display().to_string());
        args.extend(options.clone());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_cmd(repo_root, "meson", &refs)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
    } else {
        // Meson serializes version-sensitive state in build.dat.  Its
        // build.ninja sentinel can outlive a host Meson update, after which
        // `meson compile` rejects the old serialized model.  Reconfigure this
        // disposable build tree before every requested kmod rebuild.
        let mut args = vec!["setup".to_string(), "--reconfigure".to_string()];
        args.push(build_dir.display().to_string());
        args.push(source.display().to_string());
        args.extend(options.clone());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_cmd(repo_root, "meson", &refs)?;
        if changed {
            fs::write(&options_path, &options_text)
                .with_context(|| format!("failed to write {}", options_path.display()))?;
        }
    }

    run_cmd(
        repo_root,
        "meson",
        &["compile", "-C", path_str(&build_dir)?],
    )?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            path_str(&build_dir)?,
            "--no-rebuild",
            "--destdir",
            path_str(&install_dir)?,
        ],
    )?;
    for command in KMOD_BINARIES {
        let path = install_dir.join(command.source_rel);
        if !path_entry_exists(&path) {
            bail!("kmod install did not produce {}", path.display());
        }
    }
    Ok(())
}

fn kmod_meson_options() -> Vec<String> {
    vec![
        "--prefix=/usr".to_string(),
        "--sbindir=/usr/sbin".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        "--sysconfdir=/etc".to_string(),
        "--auto-features=disabled".to_string(),
        "-Dzstd=disabled".to_string(),
        "-Dxz=disabled".to_string(),
        "-Dzlib=disabled".to_string(),
        "-Dopenssl=disabled".to_string(),
        "-Dmbedtls=disabled".to_string(),
        "-Ddlopen=[]".to_string(),
        "-Dtools=true".to_string(),
        "-Dlogging=true".to_string(),
        "-Dbuild-tests=false".to_string(),
        "-Dmanpages=false".to_string(),
        "-Ddocs=false".to_string(),
    ]
}

fn build_ncurses(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/terminal/ncurses");
    let configure = source.join("configure");
    if !configure.exists() {
        bail!(
            "ncurses source not found in {}; run upstream import ncurses first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/ncurses");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp = out_root.join("configure-options.txt");
    let options = ncurses_configure_options();
    let options_text = format!("{}\n", options.join("\n"));
    if build_dir.join("Makefile").exists()
        && fs::read_to_string(&stamp).ok().as_deref() != Some(options_text.as_str())
    {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").exists() {
        run_cmd(&build_dir, path_str(&configure)?, &options)?;
        fs::write(&stamp, &options_text)
            .with_context(|| format!("failed to write {}", stamp.display()))?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        &build_dir,
        "make",
        &[&format!("DESTDIR={}", install_dir.display()), "install"],
    )?;
    for command in NCURSES_BINARIES {
        let path = install_dir.join(command.source_rel);
        if !path.exists() {
            bail!("ncurses install did not produce {}", path.display());
        }
    }
    verify_terminfo_entries(&install_dir.join("usr/share/terminfo"))?;
    Ok(())
}

fn ncurses_configure_options() -> Vec<&'static str> {
    vec![
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--with-shared",
        "--without-normal",
        "--without-debug",
        "--without-ada",
        "--without-cxx",
        "--without-cxx-binding",
        "--without-tests",
        "--without-manpages",
        "--disable-stripping",
        "--enable-widec",
        "--with-termlib",
        "--enable-pc-files",
        "--with-pkg-config-libdir=/usr/lib/x86_64-linux-gnu/pkgconfig",
    ]
}

fn build_procps(repo_root: &Path) -> Result<()> {
    let imported_source = repo_root.join("src/userland/procps-ng");
    if !imported_source.join("configure.ac").exists() {
        bail!(
            "procps-ng source not found in {}; run upstream import procps-ng first",
            imported_source.display()
        );
    }
    let out_root = repo_root.join("out/build/procps-ng");
    let source = out_root.join("source");
    remove_path_if_exists(&out_root)?;
    copy_imported_working_tree(repo_root, Path::new("src/userland/procps-ng"), &source)?;
    if !source.join("configure").exists() {
        run_cmd(&source, "./autogen.sh", &[])?;
    }
    let ncurses_install = repo_root.join("out/build/ncurses/install/usr");
    if !ncurses_install
        .join("lib/x86_64-linux-gnu/libncursesw.so.6")
        .exists()
    {
        bail!(
            "ncurses runtime missing at {}; run build ncurses first",
            ncurses_install.display()
        );
    }
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp = out_root.join("configure-options.txt");
    let options = procps_configure_options();
    let env = vec![
        (
            "PKG_CONFIG_PATH",
            ncurses_install
                .join("lib/x86_64-linux-gnu/pkgconfig")
                .display()
                .to_string(),
        ),
        (
            "CPPFLAGS",
            format!("-I{}", ncurses_install.join("include").display()),
        ),
        (
            "LDFLAGS",
            format!(
                "-L{}",
                ncurses_install.join("lib/x86_64-linux-gnu").display()
            ),
        ),
        (
            "NCURSES_CFLAGS",
            format!("-I{}", ncurses_install.join("include").display()),
        ),
        (
            "NCURSES_LIBS",
            format!(
                "-L{} -lncursesw -ltinfow",
                ncurses_install.join("lib/x86_64-linux-gnu").display()
            ),
        ),
    ];
    let stamp_text = format!(
        "{}\n{}\n",
        options.join("\n"),
        env.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if build_dir.join("Makefile").exists()
        && fs::read_to_string(&stamp).ok().as_deref() != Some(stamp_text.as_str())
    {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").exists() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source.join("configure"))?,
            &options,
            &env,
        )?;
        fs::write(&stamp, &stamp_text)
            .with_context(|| format!("failed to write {}", stamp.display()))?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[&format!("DESTDIR={}", install_dir.display()), "install"],
        &env,
    )?;
    for command in PROCPS_BINARIES {
        let path = install_dir.join(command.source_rel);
        if !path.exists() {
            bail!("procps-ng install did not produce {}", path.display());
        }
    }
    Ok(())
}

fn procps_configure_options() -> Vec<&'static str> {
    vec![
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--sysconfdir=/etc",
        "--disable-nls",
        "--without-systemd",
        "--without-elogind",
        "--disable-numa",
        "--disable-kill",
        "--disable-pidwait",
        "--disable-examples",
        "--disable-static",
    ]
}

const SOURCE_MIRROR_RSYNC_FLAGS: &[&str] = &[
    "-a",
    "--delete",
    "--delete-excluded",
    "--exclude=.git/",
    "--exclude=target/",
    "--exclude=__pycache__/",
    "--exclude=*.pyc",
];
