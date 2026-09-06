// Networking and transfer recipes.  This file is included into the crate root
// so the recipes retain the existing low-risk visibility and helper access.
fn build_iproute2(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/iproute2");
    if !source.join("Makefile").exists() {
        bail!(
            "iproute2 source not found in {}; run upstream import iproute2 first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/iproute2");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let libcap_install = repo_root.join("out/build/libcap/install/usr");
    let libcap_lib = libcap_install.join("lib/x86_64-linux-gnu");
    let libcap_pc = libcap_lib.join("pkgconfig");
    let libelf_install = repo_root.join("out/build/elfutils/install/usr");
    let libelf_lib = libelf_install.join("lib/x86_64-linux-gnu");
    let zlib_install = repo_root.join("out/build/zlib/install/usr");
    let zlib_lib = zlib_install.join("lib/x86_64-linux-gnu");
    let zstd_install = repo_root.join("out/build/zstd/install/usr");
    let zstd_lib = zstd_install.join("lib/x86_64-linux-gnu");
    let selinux_install = repo_root.join("out/build/selinux/install/usr");
    let selinux_lib = selinux_install.join("lib/x86_64-linux-gnu");
    let pcre2_install = repo_root.join("out/build/pcre2/install/usr");
    let pcre2_lib = pcre2_install.join("lib/x86_64-linux-gnu");
    if !libcap_lib.join("libcap.so").exists()
        || !libcap_pc.join("libcap.pc").is_file()
        || !libelf_lib.join("libelf.so").exists()
        || !libelf_lib.join("pkgconfig/libelf.pc").is_file()
        || !zlib_lib.join("libz.so").exists()
        || !zstd_lib.join("libzstd.so").exists()
        || !selinux_lib.join("libselinux.so").exists()
        || !pcre2_lib.join("libpcre2-8.so").exists()
    {
        bail!(
            "MattOS iproute2 development files are missing; run build libcap, elfutils, zlib, zstd, pcre2, and selinux first"
        );
    }
    let library_path = std::env::join_paths([
        &libcap_lib,
        &libelf_lib,
        &zlib_lib,
        &zstd_lib,
        &selinux_lib,
        &pcre2_lib,
    ])?
    .to_string_lossy()
    .to_string();
    let env = vec![
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths([
                libcap_pc,
                libelf_lib.join("pkgconfig"),
                zlib_lib.join("pkgconfig"),
                zstd_lib.join("pkgconfig"),
                selinux_lib.join("pkgconfig"),
                pcre2_lib.join("pkgconfig"),
            ])?
            .to_string_lossy()
            .to_string(),
        ),
        (
            "CPPFLAGS",
            format!(
                "-I{} -I{} -I{} -I{} -I{} -I{}",
                libcap_install.join("include").display(),
                libelf_install.join("include").display(),
                zlib_install.join("include").display(),
                zstd_install.join("include").display(),
                selinux_install.join("include").display(),
                pcre2_install.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!(
                "-L{} -L{} -L{} -L{} -L{} -L{}",
                libcap_lib.display(),
                libelf_lib.display(),
                zlib_lib.display(),
                zstd_lib.display(),
                selinux_lib.display(),
                pcre2_lib.display()
            ),
        ),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
    ];
    let state = fs::read_to_string(repo_root.join("upstream/state/iproute2.toml"))
        .context("failed to read iproute2 upstream state")?;
    let libcap_state = fs::read_to_string(repo_root.join("upstream/state/libcap.toml"))
        .context("failed to read libcap upstream state")?;
    let libelf_state = fs::read_to_string(repo_root.join("upstream/state/elfutils.toml"))
        .context("failed to read elfutils upstream state")?;
    let zstd_state = fs::read_to_string(repo_root.join("upstream/state/zstd.toml"))
        .context("failed to read Zstandard upstream state")?;
    let selinux_state = fs::read_to_string(repo_root.join("upstream/state/selinux.toml"))
        .context("failed to read SELinux upstream state")?;
    let pcre2_state = fs::read_to_string(repo_root.join("upstream/state/pcre2.toml"))
        .context("failed to read PCRE2 upstream state")?;
    let stamp = format!(
        "{state}\n{libcap_state}\n{libelf_state}\n{zstd_state}\n{selinux_state}\n{pcre2_state}\nPREFIX=/usr\nSBINDIR=/usr/sbin\nLIBDIR=/usr/lib/x86_64-linux-gnu\nSHARED_LIBS=n\n{}\n",
        env.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &build_dir)?;
    if !build_dir.join("config.mk").exists() {
        run_cmd_with_env_overrides(
            &build_dir,
            "./configure",
            &["--prefix=/usr", "--libdir=/usr/lib/x86_64-linux-gnu"],
            &env,
        )?;
    }
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[
            "-j",
            "4",
            "PREFIX=/usr",
            "SBINDIR=/usr/sbin",
            "SHARED_LIBS=n",
        ],
        &env,
    )?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    let destdir = format!("DESTDIR={}", install_dir.display());
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[
            "install",
            &destdir,
            "PREFIX=/usr",
            "SBINDIR=/usr/sbin",
            "SHARED_LIBS=n",
        ],
        &env,
    )?;
    let runtime_dirs: [&Path; 6] = [
        &libcap_lib,
        &libelf_lib,
        &zlib_lib,
        &zstd_lib,
        &selinux_lib,
        &pcre2_lib,
    ];
    for binary in IPROUTE2_BINARIES {
        let installed = install_dir.join(binary.source_rel);
        if !installed.exists() {
            bail!("iproute2 install did not produce {}", binary.source_rel);
        }
        validate_dependency_resolves_from(&installed, "libcap.so.2", &libcap_lib, &runtime_dirs)?;
    }
    for rel in ["usr/sbin/ip", "usr/sbin/tc"] {
        let installed = install_dir.join(rel);
        validate_dependency_resolves_from(&installed, "libelf.so.1", &libelf_lib, &runtime_dirs)?;
        validate_dependency_resolves_from(&installed, "libzstd.so.1", &zstd_lib, &runtime_dirs)?;
    }
    for rel in ["usr/sbin/ip", "usr/sbin/ss"] {
        validate_dependency_resolves_from(
            &install_dir.join(rel),
            "libselinux.so.1",
            &selinux_lib,
            &runtime_dirs,
        )?;
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_iputils(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/iputils");
    if !source.join("meson.build").exists() {
        bail!(
            "iputils source not found in {}; run upstream import iputils first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/iputils");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let options_path = out_root.join("meson-options.txt");
    let options = vec![
        "--prefix=/usr",
        "--bindir=bin",
        "--sbindir=sbin",
        "-DUSE_CAP=false",
        "-DUSE_IDN=false",
        "-DUSE_GETTEXT=false",
        "-DBUILD_ARPING=false",
        "-DBUILD_CLOCKDIFF=false",
        "-DBUILD_PING=true",
        "-DBUILD_TRACEPATH=true",
        "-DBUILD_MANS=false",
        "-DBUILD_HTML_MANS=false",
        "-DNO_SETCAP_OR_SUID=true",
        "-DINSTALL_SYSTEMD_UNITS=false",
        "-DSKIP_TESTS=true",
    ];
    let options_text = format!("{}\n", options.join("\n"));
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    let configured = build_dir.join("build.ninja").exists();
    if !configured {
        let mut args = vec!["setup", path_str(&build_dir)?, path_str(&source)?];
        args.extend(options.iter().copied());
        run_cmd(repo_root, "meson", &args)?;
    } else {
        // Meson persists version-sensitive state in build.dat.  The normal
        // MattOS cache intentionally permits reuse of completed artifacts
        // across a host Meson update, but a dependency-output miss may still
        // need to enter this disposable build directory.  Reconfigure first
        // so a newer Meson can safely compile and install instead of failing
        // late in `meson install` while reading an older build.dat.
        let mut args = vec![
            "setup",
            "--reconfigure",
            path_str(&build_dir)?,
            path_str(&source)?,
        ];
        args.extend(options.iter().copied());
        run_cmd(repo_root, "meson", &args)?;
    }
    fs::write(&options_path, &options_text)
        .with_context(|| format!("failed to write {}", options_path.display()))?;
    run_cmd(repo_root, "ninja", &["-C", path_str(&build_dir)?])?;
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
    for binary in IPUTILS_BINARIES {
        if !install_dir.join(binary.source_rel).exists() {
            bail!("iputils install did not produce {}", binary.source_rel);
        }
    }
    Ok(())
}

fn curl_configure_options() -> Vec<&'static str> {
    vec![
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--sysconfdir=/etc",
        "--with-openssl",
        "--with-ca-bundle=/etc/ssl/certs/ca-certificates.crt",
        "--without-ca-path",
        "--enable-http",
        "--disable-static",
        "--enable-shared",
        "--disable-ipv6",
        "--disable-threaded-resolver",
        "--disable-manual",
        "--disable-docs",
        "--disable-libcurl-option",
        "--disable-ipfs",
        "--disable-websockets",
        "--disable-ftp",
        "--disable-file",
        "--disable-ldap",
        "--disable-ldaps",
        "--disable-rtsp",
        "--disable-dict",
        "--disable-telnet",
        "--disable-tftp",
        "--disable-pop3",
        "--disable-imap",
        "--disable-smb",
        "--disable-smtp",
        "--disable-gopher",
        "--disable-mqtt",
        "--without-libpsl",
        "--without-zlib",
        "--without-brotli",
        "--without-zstd",
        "--without-libidn2",
        "--without-nghttp2",
        "--without-ngtcp2",
        "--without-nghttp3",
        "--without-libssh2",
        "--disable-dependency-tracking",
    ]
}

fn build_curl(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/curl");
    if !source.join("configure.ac").exists() {
        bail!(
            "curl source not found in {}; run upstream import curl first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/curl");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/curl.toml"))
        .context("failed to read curl upstream state")?;
    let openssl = repo_root.join("out/build/openssl/install/usr");
    let openssl_lib = openssl.join("lib/x86_64-linux-gnu");
    let zlib = repo_root.join("out/build/zlib/install/usr");
    let zlib_lib = zlib.join("lib/x86_64-linux-gnu");
    let zstd = repo_root.join("out/build/zstd/install/usr");
    let zstd_lib = zstd.join("lib/x86_64-linux-gnu");
    if !openssl_lib.join("libcrypto.so").exists()
        || !openssl_lib.join("libssl.so").exists()
        || !zlib_lib.join("libz.so").exists()
        || !zstd_lib.join("libzstd.so").exists()
    {
        bail!("MattOS curl TLS dependencies are missing; run build openssl, zlib, and zstd first")
    }
    let options = curl_configure_options();
    let openssl_state = fs::read_to_string(repo_root.join("upstream/state/openssl.toml"))
        .context("failed to read OpenSSL upstream state")?;
    let library_path = std::env::join_paths([&openssl_lib, &zlib_lib, &zstd_lib])?
        .to_string_lossy()
        .to_string();
    let env = [
        (
            "CPPFLAGS",
            format!(
                "-I{} -I{} -I{}",
                openssl.join("include").display(),
                zlib.join("include").display(),
                zstd.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!(
                "-L{} -L{} -L{}",
                openssl_lib.display(),
                zlib_lib.display(),
                zstd_lib.display()
            ),
        ),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths([
                openssl_lib.join("pkgconfig"),
                zlib_lib.join("pkgconfig"),
                zstd_lib.join("pkgconfig"),
            ])?
            .to_string_lossy()
            .to_string(),
        ),
    ];
    let stamp = format!(
        "{state}\n{openssl_state}\n{}\n{}\n",
        options.join("\n"),
        env.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").exists() {
        run_cmd(&source_copy, "autoreconf", &["-fi"])?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").exists() {
        let configure = source_copy.join("configure");
        run_cmd_with_env_overrides(&build_dir, path_str(&configure)?, &options, &env)?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    let destdir = format!("DESTDIR={}", install_dir.display());
    run_cmd_with_env_overrides(&build_dir, "make", &["install", &destdir], &env)?;
    for binary in CURL_BINARIES {
        if !install_dir.join(binary.source_rel).exists() {
            bail!("curl install did not produce {}", binary.source_rel);
        }
    }
    let runtime_dirs: [&Path; 3] = [&openssl_lib, &zlib_lib, &zstd_lib];
    let libcurl = install_dir.join("usr/lib/x86_64-linux-gnu/libcurl.so.4.8.0");
    validate_dependency_resolves_from(&libcurl, "libssl.so.3", &openssl_lib, &runtime_dirs)?;
    validate_dependency_resolves_from(&libcurl, "libcrypto.so.3", &openssl_lib, &runtime_dirs)?;
    validate_dependency_resolves_from(&libcurl, "libzstd.so.1", &zstd_lib, &runtime_dirs)?;
    // This is a build-private libtool convenience archive. Leaving it in
    // the staged install lets downstream libtool consumers embed this
    // checkout's absolute staging path as an ELF RUNPATH. The libcurl .so
    // and pkg-config metadata are the target-facing interface.
    remove_path_if_exists(&install_dir.join("usr/lib/x86_64-linux-gnu/libcurl.la"))?;
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

