// SMB client tools and their source-owned cryptographic runtime closure.
fn build_cifs_utils(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "cifs-utils",
        "src/system/network/cifs-utils",
        &["keyutils", "libcap", "talloc"],
        &[
            "--prefix=/usr",
            "--sbindir=/usr/sbin",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "ROOTSBINDIR=/usr/sbin",
            "--disable-cifsupcall",
            "--disable-cifsidmap",
            "--disable-cifsacl",
            "--disable-pam",
            "--disable-man",
            "--disable-pythontools",
        ],
        &["usr/sbin/mount.cifs", "usr/bin/cifscreds"],
    )
}

/// Release archives provide generated Autoconf/Gnulib inputs; retained Git
/// source is overlaid on top, so editing the authoritative import changes builds.
fn build_smb_crypto(
    repo_root: &Path,
    component: &str,
    version: &str,
    extension: &str,
    url: &str,
    sha256: &str,
    dependencies: &[&str],
    extra: &[&str],
    output: &str,
) -> Result<()> {
    let root = repo_root.join("out/build").join(component);
    let source = root.join("source");
    let build = root.join("build");
    let install = root.join("install");
    fs::create_dir_all(&root)?;
    let archive = ensure_verified_release_archive(
        &root,
        &format!("{component}-{version}.tar.{extension}"),
        url,
        sha256,
    )?;
    remove_path_if_exists(&source)?;
    remove_path_if_exists(&build)?;
    remove_path_if_exists(&install)?;
    stage_release_source(&archive, &source)?;
    run_cmd(
        repo_root,
        "rsync",
        &[
            "-a",
            "--chmod=go-w",
            &format!(
                "{}/",
                repo_root
                    .join("src/system/security")
                    .join(component)
                    .display()
            ),
            &format!("{}/", source.display()),
        ],
    )?;
    normalize_smb_source_times(&source)?;
    fs::create_dir_all(&build)?;
    let env = staged_library_environment(repo_root, dependencies)?;
    let mut options = vec![
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--disable-maintainer-mode",
    ];
    options.extend_from_slice(extra);
    run_cmd_with_env_overrides(&build, path_str(&source.join("configure"))?, &options, &env)?;
    run_cmd_with_env_overrides(&build, "make", &[], &env)?;
    run_cmd_with_env_overrides(
        &build,
        "make",
        &["install", &format!("DESTDIR={}", install.display())],
        &env,
    )?;
    // Libtool consumers otherwise discover staged .la files and hardcode
    // their output-owned location into dependent runtime search paths.
    remove_staged_libtool_archives(&install)?;
    if !install.join(output).is_file() {
        bail!("{component}: missing {output}");
    }
    Ok(())
}

fn normalize_smb_source_times(root: &Path) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            normalize_smb_source_times(&path)?;
        } else if entry.file_type()?.is_file() {
            let epoch = filetime::FileTime::from_unix_time(1_767_225_600, 0);
            filetime::set_file_times(&path, epoch, epoch)?;
        }
    }
    Ok(())
}

fn build_nettle(repo_root: &Path) -> Result<()> {
    build_smb_crypto(
        repo_root,
        "nettle",
        "3.10.2",
        "gz",
        NETTLE_RELEASE_ARCHIVE_URL,
        NETTLE_RELEASE_ARCHIVE_SHA256,
        &["gmp"],
        &["--enable-shared", "--disable-documentation"],
        "usr/lib/x86_64-linux-gnu/libnettle.so.8",
    )
}
fn build_libtasn1(repo_root: &Path) -> Result<()> {
    build_smb_crypto(
        repo_root,
        "libtasn1",
        "4.21.0",
        "gz",
        LIBTASN1_RELEASE_ARCHIVE_URL,
        LIBTASN1_RELEASE_ARCHIVE_SHA256,
        &[],
        &["--disable-doc", "--disable-nls", "--disable-tools"],
        "usr/lib/x86_64-linux-gnu/libtasn1.so.6",
    )
}
fn build_gnutls(repo_root: &Path) -> Result<()> {
    build_smb_crypto(
        repo_root,
        "gnutls",
        "3.8.13",
        "xz",
        GNUTLS_RELEASE_ARCHIVE_URL,
        GNUTLS_RELEASE_ARCHIVE_SHA256,
        &["nettle", "libtasn1", "gmp", "zlib"],
        &[
            "--disable-doc",
            "--disable-tests",
            "--disable-full-test-suite",
            "--disable-tools",
            "--disable-nls",
            "--disable-cxx",
            "--disable-guile",
            "--without-p11-kit",
            "--without-idn",
            "--without-tpm",
            "--without-tpm2",
            "--with-included-unistring",
            "--disable-rpath",
        ],
        "usr/lib/x86_64-linux-gnu/libgnutls.so.30",
    )
}

fn build_samba(repo_root: &Path) -> Result<()> {
    // PIDL needs this host-only Perl parser generator. Keep it output-owned;
    // no CPAN installation or host runtime library enters the target image.
    let yapp = repo_root.join("out/build/samba-host-tools");
    fs::create_dir_all(&yapp)?;
    let archive = ensure_verified_release_archive(
        &yapp,
        "Parse-Yapp-1.21.tar.gz",
        "https://cpan.metacpan.org/authors/id/W/WB/WBRASWELL/Parse-Yapp-1.21.tar.gz",
        "3810e998308fba2e0f4f26043035032b027ce51ce5c8a52a8b8e340ca65f13e5",
    )?;
    let yapp_source = yapp.join("source");
    stage_release_source(&archive, &yapp_source)?;
    run_cmd(
        &yapp_source,
        "perl",
        &[
            "Makefile.PL",
            &format!("INSTALL_BASE={}", yapp.join("install").display()),
        ],
    )?;
    run_cmd(&yapp_source, "make", &[])?;
    run_cmd(&yapp_source, "make", &["test", "install"])?;
    let root = repo_root.join("out/build/samba");
    let source = root.join("source");
    let install = root.join("install");
    let complete_install = root.join("complete-install");
    let validation = root.join("validation");
    remove_path_if_exists(&source)?;
    remove_path_if_exists(&install)?;
    remove_path_if_exists(&complete_install)?;
    remove_path_if_exists(&validation)?;
    sync_build_source(&repo_root.join("src/system/network/samba"), &source)?;
    let mut env = staged_library_environment(
        repo_root,
        &[
            "gnutls",
            "nettle",
            "libtasn1",
            "gmp",
            "popt",
            "zlib",
            "libcap",
            "keyutils",
            "libxcrypt",
        ],
    )?;
    env.push(("PYTHONDONTWRITEBYTECODE", "1".into()));
    env.push(("PYTHONHASHSEED", "1".into()));
    env.push(("WAF_MAKE", "1".into()));
    let path = env
        .iter_mut()
        .find(|(name, _)| *name == "PATH")
        .context("staged tool PATH")?;
    path.1 = format!("{}:{}", yapp.join("install/bin").display(), path.1);
    env.push((
        "PERL5LIB",
        yapp.join("install/lib/perl5").display().to_string(),
    ));
    run_cmd_with_env_overrides(
        &source,
        "./configure",
        &[
            "--enable-fhs",
            "--prefix=/usr",
            "--sysconfdir=/etc/samba",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--with-privatelibdir=/usr/lib/x86_64-linux-gnu/samba",
            "--localstatedir=/var",
            "--with-lockdir=/run/samba",
            "--without-ad-dc",
            "--without-ldb-lmdb",
            "--without-ads",
            "--without-winbind",
            "--without-ldap",
            "--without-pam",
            "--disable-python",
            "--without-json",
            "--without-regedit",
            "--without-libarchive",
            "--without-quotas",
            "--without-systemd",
            "--without-utmp",
            "--without-dmapi",
            "--without-cluster-support",
            "--without-syslog",
            "--without-smb1-server",
            "--disable-spotlight",
            "--disable-wsp",
            "--disable-cups",
            "--without-acl-support",
            "--private-libraries=ALL",
            "--bundled-libraries=ALL",
            "--with-shared-modules=!DEFAULT",
        ],
        &env,
    )?;
    // Build the server only as a test fixture; it is never a package payload.
    run_cmd_with_env_overrides(
        &source,
        "python3",
        &[
            "buildtools/bin/waf",
            "build",
            "--targets=client/smbclient,smbd/smbd,smbpasswd",
        ],
        &env,
    )?;
    run_cmd_with_env_overrides(
        &source,
        "python3",
        &[
            "buildtools/bin/waf",
            "install",
            &format!("--destdir={}", complete_install.display()),
        ],
        &env,
    )?;
    // Waf's partial install omits dynamically generated installation tasks.
    // Use its complete output-owned install, then retain only the executable
    // and its actual private-library closure. No server or other utility is
    // present in the authoritative client install tree or package.
    stage_samba_binary_closure(&complete_install, &install, &["usr/bin/smbclient"])?;
    stage_samba_binary_closure(
        &complete_install,
        &validation,
        &["usr/sbin/smbd", "usr/bin/smbpasswd"],
    )?;
    Ok(())
}

fn stage_samba_binary_closure(
    complete_install: &Path,
    install: &Path,
    binaries: &[&str],
) -> Result<()> {
    let mut pending = Vec::new();
    for binary in binaries {
        let source = complete_install.join(binary);
        stage_output_file(&source, &install.join(binary), 0o755)?;
        pending.push(source);
    }
    let private = Path::new("usr/lib/x86_64-linux-gnu/samba");
    let mut copied = BTreeSet::new();
    while let Some(binary) = pending.pop() {
        for name in elf_needed_names(&binary)? {
            let path = complete_install.join(private).join(&name);
            if path.is_file() && copied.insert(name.clone()) {
                stage_output_file(&path, &install.join(private).join(&name), 0o755)?;
                pending.push(path);
            } else if name.ends_with("-private-samba.so") && !path.is_file() {
                bail!("Samba client private library is missing: {name}");
            }
        }
    }
    Ok(())
}

fn build_talloc(repo_root: &Path) -> Result<()> {
    let root = repo_root.join("out/build/talloc");
    let source = root.join("source");
    let install = root.join("install");
    remove_path_if_exists(&source)?;
    remove_path_if_exists(&install)?;
    sync_build_source(&repo_root.join("src/system/network/samba"), &source)?;
    let mut env = staged_library_environment(repo_root, &[])?;
    env.push(("PYTHON", "python3".into()));
    env.push(("PYTHONHASHSEED", "1".into()));
    env.push(("WAF_MAKE", "1".into()));
    let cwd = source.join("lib/talloc");
    run_cmd_with_env_overrides(
        &cwd,
        "./configure",
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-python",
        ],
        &env,
    )?;
    run_cmd_with_env_overrides(
        &cwd,
        "python3",
        &["../../buildtools/bin/waf", "build"],
        &env,
    )?;
    run_cmd_with_env_overrides(
        &cwd,
        "python3",
        &[
            "../../buildtools/bin/waf",
            "install",
            &format!("--destdir={}", install.display()),
        ],
        &env,
    )?;
    Ok(())
}

fn build_mattos_upgrade(repo_root: &Path) -> Result<()> {
    let root = repo_root.join("out/build/mattos-upgrade");
    // This is owned workspace code, like the installer. Retain the locked
    // workspace's dependency resolution; write all compiler output under out/.
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/system/release/Cargo.toml",
        ],
        &[(
            "CARGO_TARGET_DIR",
            root.join("cargo-target").display().to_string(),
        )],
    )?;
    stage_output_file(
        &root.join("cargo-target/release/mattos-upgrade"),
        &root.join("install/usr/bin/mattos-upgrade"),
        0o755,
    )?;
    Ok(())
}
