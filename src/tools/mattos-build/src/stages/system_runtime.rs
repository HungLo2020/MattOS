// Core system runtime services: systemd, D-Bus, and the D-Bus broker.
// Included into the crate root to preserve the existing recipe visibility.
fn build_systemd(repo_root: &Path) -> Result<()> {
    let systemd_src = repo_root.join("src/system/systemd");
    if !systemd_src.join("meson.build").exists() {
        bail!(
            "systemd source not found in {}; run upstream import systemd first",
            systemd_src.display()
        );
    }

    let out_root = repo_root.join("out/build/systemd");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let options_path = out_root.join("meson-options.txt");
    let env_path = out_root.join("meson-env.txt");
    let kmod_install = repo_root.join("out/build/kmod/install/usr");
    if !kmod_install
        .join("lib/x86_64-linux-gnu/libkmod.so.2")
        .exists()
    {
        bail!(
            "kmod development files missing at {}; run build kmod first",
            kmod_install.display()
        );
    }
    let util_linux_install = repo_root.join("out/build/util-linux/install/usr");
    let util_linux_lib = util_linux_install.join("lib/x86_64-linux-gnu");
    if !util_linux_lib.join("libmount.so.1").exists()
        || !util_linux_lib.join("pkgconfig/mount.pc").exists()
    {
        bail!(
            "util-linux libmount development files missing at {}; run build util-linux first",
            util_linux_install.display()
        );
    }
    let dependency_installs = [
        repo_root.join("out/build/dbus/install/usr"),
        repo_root.join("out/build/zlib/install/usr"),
        repo_root.join("out/build/bzip2/install/usr"),
        repo_root.join("out/build/lz4/install/usr"),
        repo_root.join("out/build/xz/install/usr"),
        repo_root.join("out/build/zstd/install/usr"),
        repo_root.join("out/build/elfutils/install/usr"),
        repo_root.join("out/build/pcre2/install/usr"),
        repo_root.join("out/build/selinux/install/usr"),
        repo_root.join("out/build/selinux/sepol-install/usr"),
        repo_root.join("out/build/libxcrypt/install/usr"),
        repo_root.join("out/build/linux-pam/install/usr"),
    ];
    for install in &dependency_installs {
        if !install.join("include").is_dir() || !install.join("lib/x86_64-linux-gnu").is_dir() {
            bail!(
                "systemd source-built dependency is incomplete at {}",
                install.display()
            );
        }
    }
    let mut include_dirs = vec![
        kmod_install.join("include"),
        util_linux_install.join("include"),
    ];
    include_dirs.extend(
        dependency_installs
            .iter()
            .map(|install| install.join("include")),
    );
    let mut library_dirs = vec![
        kmod_install.join("lib/x86_64-linux-gnu"),
        util_linux_lib.clone(),
    ];
    library_dirs.extend(
        dependency_installs
            .iter()
            .map(|install| install.join("lib/x86_64-linux-gnu")),
    );
    let mut sysroot_installs = dependency_installs.to_vec();
    sysroot_installs.push(kmod_install.clone());
    sysroot_installs.push(util_linux_install.clone());
    hydrate_development_sysroot(repo_root, &sysroot_installs)?;
    let pkgconfig_dirs = library_dirs
        .iter()
        .map(|library| library.join("pkgconfig"))
        .filter(|directory| directory.is_dir())
        .collect::<Vec<_>>();
    let system_library_path = std::env::join_paths(library_dirs.iter())?
        .to_string_lossy()
        .to_string();
    let pkgconfig_path = std::env::join_paths(pkgconfig_dirs.iter())?
        .to_string_lossy()
        .to_string();
    let mut cflags = include_dirs
        .iter()
        .map(|include| format!("-I{}", include.display()))
        .collect::<Vec<_>>()
        .join(" ");
    cflags.push_str(&format!(
        " -ffile-prefix-map={}=/usr/src/mattos -fdebug-prefix-map={}=/usr/src/mattos -fmacro-prefix-map={}=/usr/src/mattos",
        repo_root.display(),
        repo_root.display(),
        repo_root.display()
    ));
    let ldflags = library_dirs
        .iter()
        .flat_map(|library| {
            [
                format!("-L{}", library.display()),
                format!("-Wl,-rpath-link,{}", library.display()),
            ]
        })
        .collect::<Vec<_>>()
        .join(" ");
    let env_overrides = vec![
        ("PKG_CONFIG_PATH", pkgconfig_path.clone()),
        ("PKG_CONFIG_LIBDIR", pkgconfig_path),
        (
            "PKG_CONFIG_SYSROOT_DIR",
            repo_root.join("out/sysroot").display().to_string(),
        ),
        ("CFLAGS", cflags),
        ("LDFLAGS", ldflags),
        ("LIBRARY_PATH", system_library_path.clone()),
        ("LD_LIBRARY_PATH", system_library_path),
    ];
    let env_text = format!(
        "{}\n",
        env_overrides
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;

    let options = systemd_meson_options();
    let options_text = format!("{}\n", options.join("\n"));
    let existing_options = fs::read_to_string(&options_path).ok();
    let needs_reconfigure = existing_options.as_deref() != Some(options_text.as_str());
    let mut configured = build_dir.join("build.ninja").exists();
    if configured && fs::read_to_string(&env_path).ok().as_deref() != Some(env_text.as_str()) {
        remove_path_if_exists(&build_dir)?;
        configured = false;
    }

    if !configured {
        let mut setup_args = vec![
            "setup".to_string(),
            build_dir.display().to_string(),
            systemd_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
        fs::write(&env_path, &env_text)
            .with_context(|| format!("failed to write {}", env_path.display()))?;
    } else if needs_reconfigure {
        let mut setup_args = vec![
            "setup".to_string(),
            "--reconfigure".to_string(),
            build_dir.display().to_string(),
            systemd_src.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
        fs::write(&options_path, &options_text)
            .with_context(|| format!("failed to write {}", options_path.display()))?;
        fs::write(&env_path, &env_text)
            .with_context(|| format!("failed to write {}", env_path.display()))?;
    }

    let ninja_args = vec![
        "-C",
        build_dir
            .to_str()
            .ok_or_else(|| anyhow!("invalid build dir"))?,
    ];
    run_cmd_with_env_overrides(repo_root, "ninja", &ninja_args, &env_overrides)?;

    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)
            .with_context(|| format!("failed to clean {}", install_dir.display()))?;
    }
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;

    let install_args = vec![
        "install",
        "-C",
        build_dir
            .to_str()
            .ok_or_else(|| anyhow!("invalid build dir"))?,
        "--no-rebuild",
        "--destdir",
        install_dir
            .to_str()
            .ok_or_else(|| anyhow!("invalid install dir"))?,
    ];
    run_cmd_with_env_overrides(repo_root, "meson", &install_args, &env_overrides)?;
    rewrite_staged_pkgconfig_files(&install_dir)?;

    patch_systemd_osc_profile_for_posix_login_shell(&install_dir)?;

    let pid1 = install_dir.join("usr/lib/systemd/systemd");
    if !pid1.exists() {
        bail!("systemd install did not produce {}", pid1.display());
    }

    Ok(())
}

fn patch_systemd_osc_profile_for_posix_login_shell(install_dir: &Path) -> Result<()> {
    let path = install_dir.join("etc/profile.d/80-systemd-osc-context.sh");
    let body =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let bash_guard = "# Not bash?\n[ -n \"${BASH_VERSION:-}\" ] || return 0";
    let guarded = "# MattOS can inherit BASH_VERSION into a POSIX login shell. Verify the\n# required Bash builtin itself before parsing the interactive prompt setup.\ncommand -v shopt >/dev/null 2>&1 || return 0";
    let upstream = "    [ -n \"$(declare -p PROMPT_COMMAND 2>/dev/null)\" ] || PROMPT_COMMAND+=('')\n\n    # Whenever a new prompt is shown, close the previous command, and prepare new command\n    PROMPT_COMMAND+=(__systemd_osc_context_precmdline)";
    let replacement = "    # MattOS login commands are launched by a POSIX shell. Array assignment\n    # syntax is rejected while parsing even when this Bash-only branch is not\n    # executed, so preserve the hook with a scalar PROMPT_COMMAND instead.\n    if [ -n \"${PROMPT_COMMAND:-}\" ]; then\n        PROMPT_COMMAND=\"__systemd_osc_context_precmdline;${PROMPT_COMMAND}\"\n    else\n        PROMPT_COMMAND=__systemd_osc_context_precmdline\n    fi";
    if !body.contains(bash_guard) || !body.contains(upstream) {
        bail!("systemd OSC profile no longer matches the reviewed POSIX-shell compatibility patch");
    }
    let body = body.replacen(bash_guard, guarded, 1);
    fs::write(&path, body.replacen(upstream, replacement, 1))
        .with_context(|| format!("failed to patch {}", path.display()))
}

fn systemd_meson_options() -> Vec<String> {
    vec![
        "--prefix=/usr".to_string(),
        "--sysconfdir=/etc".to_string(),
        "--localstatedir=/var".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        "-Dmode=release".to_string(),
        "-Dtests=false".to_string(),
        "-Dman=disabled".to_string(),
        "-Dhtml=disabled".to_string(),
        "-Dtranslations=false".to_string(),
        "-Dnetworkd=true".to_string(),
        "-Dresolve=true".to_string(),
        "-Dtimesyncd=true".to_string(),
        "-Dsystemd-network-uid=192".to_string(),
        "-Dsystemd-resolve-uid=193".to_string(),
        "-Dsystemd-timesync-uid=194".to_string(),
        "-Dhomed=disabled".to_string(),
        "-Dportabled=false".to_string(),
        // mattos-compat uses the target-owned nspawn binary to run isolated
        // distro userlands; keep it in the systemd stage and package it from
        // that output rather than relying on the host implementation.
        "-Dnspawn=enabled".to_string(),
        "-Dbootloader=disabled".to_string(),
        "-Dfirstboot=false".to_string(),
        "-Drepart=disabled".to_string(),
        "-Doomd=false".to_string(),
        "-Duserdb=false".to_string(),
        "-Dremote=disabled".to_string(),
        "-Dsysupdate=disabled".to_string(),
        "-Dsysupdated=disabled".to_string(),
        "-Dsysinstall=false".to_string(),
        "-Dimportd=disabled".to_string(),
        "-Dvmspawn=disabled".to_string(),
        "-Dcoredump=false".to_string(),
        "-Dpstore=false".to_string(),
        "-Dmachined=false".to_string(),
        "-Dhostnamed=false".to_string(),
        // COSMIC Initial Setup uses the standard org.freedesktop.locale1 API
        // to read and apply the selected system locale.
        "-Dlocaled=true".to_string(),
        "-Dtimedated=true".to_string(),
        "-Dnsresourced=false".to_string(),
        "-Ddefault-network=false".to_string(),
        "-Ddbus=enabled".to_string(),
        // The target dbus-1.pc is queried under PKG_CONFIG_SYSROOT_DIR while
        // configuring systemd.  Do not let its absolute host/sysroot paths
        // become Meson install destinations; these are target filesystem
        // paths in the finished systemd package.
        "-Ddbussessionservicedir=/usr/share/dbus-1/services".to_string(),
        "-Ddbussystemservicedir=/usr/share/dbus-1/system-services".to_string(),
        "-Ddbus-interfaces-dir=/usr/share/dbus-1/interfaces".to_string(),
        "-Ddbuspolicydir=/usr/share/dbus-1/system.d".to_string(),
        "-Dglib=disabled".to_string(),
        "-Dseccomp=disabled".to_string(),
        "-Dselinux=enabled".to_string(),
        "-Dacl=disabled".to_string(),
        "-Daudit=disabled".to_string(),
        // udev must probe filesystem and GPT metadata so the stable
        // /dev/disk/by-{uuid,partuuid} names used by installed fstab entries
        // exist during coldplug.
        "-Dblkid=enabled".to_string(),
        "-Dkmod=enabled".to_string(),
        "-Dlibmount=enabled".to_string(),
        "-Dpam=enabled".to_string(),
        "-Dlibcrypt=enabled".to_string(),
        "-Dlibcryptsetup=disabled".to_string(),
        "-Dopenssl=disabled".to_string(),
        "-Dlibidn2=disabled".to_string(),
        "-Dgnutls=disabled".to_string(),
        "-Dlibfido2=disabled".to_string(),
        "-Dtpm=false".to_string(),
        "-Dtpm2=disabled".to_string(),
        "-Dqrencode=disabled".to_string(),
        "-Delfutils=disabled".to_string(),
        "-Dzlib=enabled".to_string(),
        "-Dbzip2=enabled".to_string(),
        "-Dxz=enabled".to_string(),
        "-Dlz4=enabled".to_string(),
        "-Dzstd=enabled".to_string(),
        "-Dxkbcommon=disabled".to_string(),
        "-Dpcre2=enabled".to_string(),
        "-Dbpf-framework=disabled".to_string(),
        "-Dvmlinux-h=disabled".to_string(),
        "-Dkernel-install=false".to_string(),
        "-Danalyze=false".to_string(),
        "-Dcreate-log-dirs=false".to_string(),
        "-Djournal-storage-default=volatile".to_string(),
    ]
}

fn build_dbus(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "dbus",
        "src/system/dbus/dbus",
        &["expat"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Dmessage_bus=true",
            "-Dtools=true",
            "-Dinstalled_tests=false",
            "-Dintrusive_tests=false",
            "-Dmodular_tests=disabled",
            "-Ddoxygen_docs=disabled",
            "-Dducktype_docs=disabled",
            "-Dqt_help=disabled",
            "-Dapparmor=disabled",
            "-Dselinux=disabled",
            "-Dlibaudit=disabled",
            "-Dsystemd=disabled",
        ],
        "usr/bin/dbus-run-session",
        &[],
    )?;
    let dbus_usr = repo_root.join("out/build/dbus/install/usr");
    rewrite_pkgconfig_prefixes(&dbus_usr.join("lib/x86_64-linux-gnu/pkgconfig"), &dbus_usr)?;
    for required in [
        "usr/lib/x86_64-linux-gnu/libdbus-1.so.3",
        "usr/bin/dbus-daemon",
        "usr/bin/dbus-run-session",
    ] {
        if !repo_root
            .join("out/build/dbus/install")
            .join(required)
            .is_file()
        {
            bail!("D-Bus build did not install /{required}");
        }
    }
    Ok(())
}


fn build_dbus_broker(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/dbus/dbus-broker");
    if !source.join("meson.build").exists() {
        bail!(
            "dbus-broker source not found in {}; run upstream import dbus-broker first",
            source.display()
        );
    }

    let systemd_install = repo_root.join("out/build/systemd/install/usr");
    let systemd_lib = systemd_install.join("lib/x86_64-linux-gnu");
    let systemd_lib_pc = systemd_lib.join("pkgconfig");
    let systemd_share_pc = systemd_install.join("share/pkgconfig");
    if !systemd_lib.join("libsystemd.so").exists()
        || !systemd_lib_pc.join("libsystemd.pc").exists()
        || !systemd_share_pc.join("systemd.pc").exists()
    {
        bail!(
            "systemd development files missing at {}; run build systemd first",
            systemd_install.display()
        );
    }
    let expat_install = repo_root.join("out/build/expat/install/usr");
    let expat_lib = expat_install.join("lib/x86_64-linux-gnu");
    let expat_pc = expat_lib.join("pkgconfig");
    if !expat_lib.join("libexpat.so").exists() || !expat_pc.join("expat.pc").is_file() {
        bail!(
            "MattOS-built Expat development files missing at {}; run build expat first",
            expat_install.display()
        );
    }

    let out_root = repo_root.join("out/build/dbus-broker");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let options = vec![
        "--prefix=/usr".to_string(),
        "--bindir=bin".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        "--buildtype=release".to_string(),
        "--wrap-mode=forcefallback".to_string(),
        "-Dlauncher=true".to_string(),
        "-Dtests=false".to_string(),
        "-Ddocs=false".to_string(),
        "-Ddoctest=false".to_string(),
        "-Dreference-test=false".to_string(),
        "-Daudit=false".to_string(),
        "-Dapparmor=false".to_string(),
        "-Dselinux=false".to_string(),
        "-Dunstable=false".to_string(),
    ];
    let pkg_config_path = std::env::join_paths([&expat_pc, &systemd_lib_pc, &systemd_share_pc])
        .context("failed to construct dbus-broker PKG_CONFIG_PATH")?
        .to_string_lossy()
        .to_string();
    hydrate_development_sysroot(repo_root, &[expat_install.clone(), systemd_install.clone()])?;
    let env_overrides = vec![
        ("PKG_CONFIG_PATH", pkg_config_path.clone()),
        ("PKG_CONFIG_LIBDIR", pkg_config_path),
        (
            "PKG_CONFIG_SYSROOT_DIR",
            repo_root.join("out/sysroot").display().to_string(),
        ),
        (
            "CFLAGS",
            format!(
                "-I{} -I{}",
                expat_install.join("include").display(),
                systemd_install.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!("-L{} -L{}", expat_lib.display(), systemd_lib.display()),
        ),
        (
            "LIBRARY_PATH",
            std::env::join_paths([&expat_lib, &systemd_lib])?
                .to_string_lossy()
                .to_string(),
        ),
        (
            "LD_LIBRARY_PATH",
            std::env::join_paths([&expat_lib, &systemd_lib])?
                .to_string_lossy()
                .to_string(),
        ),
    ];
    let state = fs::read_to_string(repo_root.join("upstream/state/dbus-broker.toml"))
        .context("failed to read dbus-broker upstream state")?;
    let expat_state = fs::read_to_string(repo_root.join("upstream/state/expat.toml"))
        .context("failed to read Expat upstream state")?;
    let dependency_outputs = ["expat", "systemd"]
        .iter()
        .map(|dependency| {
            let manifest = stage_cache::read_stage_manifest(repo_root, dependency)
                .with_context(|| format!("failed to read {dependency} dependency manifest"))?;
            Ok::<_, anyhow::Error>(format!("{dependency}={}", manifest.output_content_digest))
        })
        .collect::<Result<Vec<_>>>()?;
    let stamp = format!(
        "{state}\n{expat_state}\n{}\n{}\ndependency-outputs={}\n",
        options.join("\n"),
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n"),
        dependency_outputs.join(",")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }

    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    copy_imported_working_tree(
        repo_root,
        Path::new("src/system/dbus/dbus-broker"),
        &source_copy,
    )?;
    apply_component_patches(repo_root, "dbus-broker", &source_copy)?;
    if !build_dir.join("build.ninja").exists() {
        let mut setup_args = vec![
            "setup".to_string(),
            build_dir.display().to_string(),
            source_copy.display().to_string(),
        ];
        setup_args.extend(options.clone());
        let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
        run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
    }

    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &["compile", "-C", path_str(&build_dir)?],
        &env_overrides,
    )?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd_with_env_overrides(
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
        &env_overrides,
    )?;

    for rel in [
        "usr/bin/dbus-broker",
        "usr/bin/dbus-broker-launch",
        "usr/lib/systemd/system/dbus-broker.service",
    ] {
        if !install_dir.join(rel).exists() {
            bail!("dbus-broker install did not produce {rel}");
        }
    }
    validate_dependency_resolves_from(
        &install_dir.join("usr/bin/dbus-broker-launch"),
        "libexpat.so.1",
        &expat_lib,
        &[&expat_lib, &systemd_lib],
    )?;
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

