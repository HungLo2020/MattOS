fn build_pkgconf(repo_root: &Path) -> Result<()> {
    // libpkgconf is linked statically: MattOS ships one pkgconf package
    // rather than Debian's pkgconf/pkgconf-bin/libpkgconf3 split.
    build_meson_runtime(
        repo_root,
        "pkgconf",
        "src/build-tools/pkgconf",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Ddefault_library=static",
            "-Dwith-pkg-config-dir=/usr/local/lib/x86_64-linux-gnu/pkgconfig:/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig:/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig",
            "-Dwith-system-libdir=/usr/lib/x86_64-linux-gnu:/usr/lib:/lib",
            "-Dwith-system-includedir=/usr/include",
        ],
        "usr/bin/pkgconf",
        &[],
    )
}

fn build_cmake(repo_root: &Path) -> Result<()> {
    // CMake's own bundled third-party libraries (curl, libarchive, zlib, ...)
    // are part of the vendored tree; only OpenSSL comes from MattOS, named
    // explicitly so FindOpenSSL can never select the build host's copy.
    let openssl = repo_root.join("out/build/openssl/install/usr");
    let include = format!("-DOPENSSL_INCLUDE_DIR={}", openssl.join("include").display());
    let crypto = format!(
        "-DOPENSSL_CRYPTO_LIBRARY={}",
        openssl.join("lib/x86_64-linux-gnu/libcrypto.so").display()
    );
    let ssl = format!(
        "-DOPENSSL_SSL_LIBRARY={}",
        openssl.join("lib/x86_64-linux-gnu/libssl.so").display()
    );
    // Everything else is found only inside the MattOS sysroot, never on the
    // build host (the bundled curl otherwise probes for host libidn2).
    let root = format!("-DCMAKE_FIND_ROOT_PATH={}", repo_root.join("out/sysroot").display());
    build_cmake_runtime(
        repo_root,
        "cmake",
        "src/build-tools/cmake",
        &["openssl"],
        &[
            &root,
            "-DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER",
            "-DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY",
            "-DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY",
            "-DCMAKE_FIND_ROOT_PATH_MODE_PACKAGE=ONLY",
            "-DUSE_LIBIDN2=OFF",
            "-DCURL_USE_LIBPSL=OFF",
            "-DCURL_USE_LIBSSH2=OFF",
            "-DCURL_BROTLI=OFF",
            "-DCURL_USE_GSSAPI=OFF",
            "-DCMAKE_INSTALL_PREFIX=/usr",
            "-DCMAKE_BUILD_TYPE=Release",
            "-DCMAKE_USE_SYSTEM_LIBRARIES=OFF",
            "-DCMAKE_USE_OPENSSL=ON",
            &include,
            &crypto,
            &ssl,
            "-DBUILD_TESTING=OFF",
            "-DBUILD_CursesDialog=OFF",
            "-DBUILD_QtDialog=OFF",
            "-DSPHINX_MAN=OFF",
            "-DSPHINX_HTML=OFF",
            "-DCMake_BUILD_LTO=OFF",
        ],
        "usr/bin/cmake",
    )
}

/// Perl's major.minor, the versioned library directory name Debian uses.
const PERL_API_VERSION: &str = "5.44";

fn build_perl(repo_root: &Path) -> Result<()> {
    // Perl's Configure only builds inside its source tree, so the build runs
    // in an output-owned mirror.  Every path and identity Configure would
    // otherwise probe from the build host is given explicitly: Config.pm
    // ships in the package and must describe a MattOS system, not the
    // checkout (no build-root library paths, host name or host uname).
    let out_root = repo_root.join("out/build/perl");
    let source = out_root.join("source");
    let install = out_root.join("install");
    remove_path_if_exists(&source)?;
    fs::create_dir_all(&out_root)?;
    sync_build_source(&repo_root.join("src/development/perl"), &source)?;
    let api = PERL_API_VERSION;
    let options = [
        "-des".to_string(),
        "-Dcc=gcc".to_string(),
        "-Dprefix=/usr".to_string(),
        "-Dvendorprefix=/usr".to_string(),
        "-Dsiteprefix=/usr/local".to_string(),
        format!("-Dprivlib=/usr/share/perl/{api}"),
        format!("-Darchlib=/usr/lib/x86_64-linux-gnu/perl/{api}"),
        "-Dvendorlib=/usr/share/perl5".to_string(),
        format!("-Dvendorarch=/usr/lib/x86_64-linux-gnu/perl5/{api}"),
        format!("-Dsitelib=/usr/local/share/perl/{api}"),
        format!("-Dsitearch=/usr/local/lib/x86_64-linux-gnu/perl/{api}"),
        "-Dsitebin=/usr/local/bin".to_string(),
        "-Dsitescript=/usr/local/bin".to_string(),
        "-Dman1dir=/usr/share/man/man1".to_string(),
        "-Dman1ext=1".to_string(),
        "-Dman3dir=none".to_string(),
        "-Dsiteman1dir=/usr/local/man/man1".to_string(),
        "-Dsiteman3dir=none".to_string(),
        "-Dvendorman3dir=none".to_string(),
        "-Dinc_version_list=none".to_string(),
        "-Dstartperl=#!/usr/bin/perl".to_string(),
        "-Dperlpath=/usr/bin/perl".to_string(),
        "-Dpager=/usr/bin/less".to_string(),
        "-Dusethreads".to_string(),
        "-Uuseshrplib".to_string(),
        "-Doptimize=-O2".to_string(),
        // Library search paths as they are on the installed system; the
        // compiler wrapper supplies the MattOS sysroot at build time.
        "-Dlocincpth=".to_string(),
        "-Dloclibpth=".to_string(),
        "-Dlibpth=/usr/lib/x86_64-linux-gnu /usr/lib /lib/x86_64-linux-gnu /lib".to_string(),
        "-Dplibpth=/usr/lib/x86_64-linux-gnu /usr/lib".to_string(),
        "-Dglibpth=/usr/lib/x86_64-linux-gnu /usr/lib".to_string(),
        // Only libraries MattOS provides: the build host's Berkeley DB and
        // GDBM must never be probed into the link line.
        "-Dlibswanted=m crypt pthread dl c".to_string(),
        "-Dnoextensions=DB_File GDBM_File NDBM_File ODBM_File".to_string(),
        "-Dcf_by=MattOS".to_string(),
        "-Dcf_email=root@localhost".to_string(),
        "-Dperladmin=root@localhost".to_string(),
        "-Dmyhostname=localhost".to_string(),
        "-Dmydomain=.localdomain".to_string(),
        "-Dmyuname=linux mattos".to_string(),
        "-Dosvers=mattos".to_string(),
        "-Dcf_time=Thu Jan  1 00:00:00 UTC 1970".to_string(),
    ];
    let option_refs = options.iter().map(String::as_str).collect::<Vec<_>>();
    run_cmd_with_env_overrides(&source, "sh", &[&["Configure"], option_refs.as_slice()].concat(), &[])?;
    // Configure records the wall-clock time whatever -Dcf_time says, and
    // perlbug embeds patchlevel.h's modification time: pin both, then let
    // `Configure -S` regenerate the files derived from config.sh.
    let config_sh = source.join("config.sh");
    let fixed_time = "cf_time='Thu Jan  1 00:00:00 UTC 1970'";
    let config = fs::read_to_string(&config_sh)?
        .lines()
        .map(|line| {
            if line.starts_with("cf_time=") {
                fixed_time
            } else if line.starts_with("# Configuration time:") {
                "# Configuration time: Thu Jan  1 00:00:00 UTC 1970"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&config_sh, format!("{config}\n"))?;
    run_cmd_with_env_overrides(&source, "sh", &["Configure", "-S"], &[])?;
    filetime::set_file_mtime(
        source.join("patchlevel.h"),
        filetime::FileTime::from_unix_time(MATTOS_SOURCE_DATE_EPOCH.parse()?, 0),
    )?;
    run_cmd_with_env_overrides(&source, "make", &["-j", "4"], &[])?;
    remove_path_if_exists(&install)?;
    run_cmd_with_env_overrides(
        &source,
        "make",
        &["install", &format!("DESTDIR={}", install.display())],
        &[],
    )?;
    relocate_toolchain_configuration(
        repo_root,
        &[
            install.join(format!("usr/lib/x86_64-linux-gnu/perl/{api}/Config.pm")),
            install.join(format!("usr/lib/x86_64-linux-gnu/perl/{api}/Config_heavy.pl")),
        ],
    )?;
    for relative in [
        "usr/bin/perl".to_string(),
        format!("usr/share/perl/{api}/strict.pm"),
        format!("usr/lib/x86_64-linux-gnu/perl/{api}/Config.pm"),
    ] {
        if !install.join(&relative).is_file() {
            bail!("perl install did not produce {relative}");
        }
    }
    Ok(())
}

fn build_m4(repo_root: &Path) -> Result<()> {
    build_release_autotools_program(
        repo_root,
        "m4",
        "m4-1.4.21.tar.xz",
        M4_RELEASE_ARCHIVE_URL,
        M4_RELEASE_ARCHIVE_SHA256,
        &[],
        &["--prefix=/usr", "--disable-nls"],
        &["usr/bin/m4"],
    )
}

// Autoconf, Automake and Libtool are Perl and shell scripts.  Their builds
// run the build host's m4 and perl, while the installed scripts name the
// MattOS /usr/bin/m4 and /usr/bin/perl their packages depend on.
fn build_autoconf(repo_root: &Path) -> Result<()> {
    // Autoconf's frozen .m4f files record where each macro file was read
    // from.  Building in the source tree makes those locations relative
    // (`lib/autoconf/c.m4`) instead of naming the build root, and the frozen
    // format's length-prefixed records cannot be rewritten afterwards.
    let out_root = repo_root.join("out/build/autoconf");
    let source = out_root.join("source");
    let install = out_root.join("install");
    fs::create_dir_all(&out_root)?;
    let archive = ensure_verified_release_archive(
        &out_root,
        "autoconf-2.73.tar.xz",
        AUTOCONF_RELEASE_ARCHIVE_URL,
        AUTOCONF_RELEASE_ARCHIVE_SHA256,
    )?;
    remove_path_if_exists(&out_root.join("build"))?;
    stage_release_source(&archive, &source)?;
    run_cmd_with_env_overrides(
        &source,
        "./configure",
        &["--prefix=/usr", "M4=/usr/bin/m4", "PERL=/usr/bin/perl"],
        &[],
    )?;
    run_cmd_with_env_overrides(&source, "make", &["-j", "4"], &[])?;
    remove_path_if_exists(&install)?;
    run_cmd_with_env_overrides(
        &source,
        "make",
        &["install", &format!("DESTDIR={}", install.display())],
        &[],
    )?;
    let root = repo_root.display().to_string();
    for relative in ["usr/bin/autoconf", "usr/bin/autoreconf", "usr/share/autoconf/autoconf/autoconf.m4f"] {
        if !install.join(relative).is_file() {
            bail!("autoconf install did not produce {relative}");
        }
    }
    for frozen in ["autoconf/autoconf.m4f", "autotest/autotest.m4f", "m4sugar/m4sh.m4f", "m4sugar/m4sugar.m4f"] {
        let path = install.join("usr/share/autoconf").join(frozen);
        if fs::read(&path)?.windows(root.len()).any(|window| window == root.as_bytes()) {
            bail!("{} names the build root", path.display());
        }
    }
    Ok(())
}

fn build_automake(repo_root: &Path) -> Result<()> {
    build_release_autotools_program(
        repo_root,
        "automake",
        "automake-1.19.tar.xz",
        AUTOMAKE_RELEASE_ARCHIVE_URL,
        AUTOMAKE_RELEASE_ARCHIVE_SHA256,
        &[],
        &["--prefix=/usr", "PERL=/usr/bin/perl"],
        &["usr/bin/automake", "usr/bin/aclocal"],
    )
}

fn build_libtool(repo_root: &Path) -> Result<()> {
    // Only the libtool scripts and macros: libltdl is not installed (Debian
    // ships it separately, and nothing in MattOS links it).
    build_release_autotools_program(
        repo_root,
        "libtool",
        "libtool-2.6.2.tar.xz",
        LIBTOOL_RELEASE_ARCHIVE_URL,
        LIBTOOL_RELEASE_ARCHIVE_SHA256,
        &[],
        &[
            "--prefix=/usr",
            "--disable-ltdl-install",
            "M4=/usr/bin/m4",
            "SED=/usr/bin/sed",
            "GREP=/usr/bin/grep",
            // Otherwise read from the build host's /etc/ld.so.conf.
            "lt_cv_sys_lib_dlsearch_path_spec=/lib/x86_64-linux-gnu /usr/lib/x86_64-linux-gnu /lib /usr/lib /usr/local/lib",
        ],
        &["usr/bin/libtool", "usr/bin/libtoolize", "usr/share/aclocal/libtool.m4"],
    )?;
    relocate_toolchain_configuration(
        repo_root,
        &[repo_root.join("out/build/libtool/install/usr/bin/libtool")],
    )
}

/// Rewrites a shipped toolchain description (Perl's Config, the generated
/// libtool script) from the build's MattOS toolchain to the same toolchain
/// as installed: sysroot and toolchain-tree paths become system paths, the
/// compiler wrappers and the cross linker become `/usr/bin` tools, and the
/// build-only `--sysroot` and prefix-map flags are dropped.  A build-root
/// path left over afterwards fails the build.
fn relocate_toolchain_configuration(repo_root: &Path, files: &[PathBuf]) -> Result<()> {
    let root = repo_root.display().to_string();
    let dropped = [
        format!("--sysroot={root}/out/sysroot"),
        format!("-ffile-prefix-map={root}=/usr/src/mattos"),
        format!("-fdebug-prefix-map={root}=/usr/src/mattos"),
        format!("-fmacro-prefix-map={root}=/usr/src/mattos"),
    ];
    let mapped = [
        (format!("{root}/out/build/cross-toolchain/install/bin/x86_64-pc-linux-gnu-"), "/usr/bin/".to_string()),
        (format!("{root}/out/toolchain/bin/"), "/usr/bin/".to_string()),
        (format!("{root}/out/build/gcc-runtime/toolchain/usr/bin/../"), "/usr/".to_string()),
        (format!("{root}/out/build/gcc-runtime/toolchain/"), "/".to_string()),
        (format!("{root}/out/sysroot/"), "/".to_string()),
    ];
    for file in files {
        let mut text = fs::read_to_string(file)
            .with_context(|| format!("read toolchain configuration {}", file.display()))?;
        for flag in &dropped {
            text = text
                .replace(&format!("{flag} "), "")
                .replace(&format!(" {flag}"), "")
                .replace(flag.as_str(), "");
        }
        for (from, to) in &mapped {
            text = text.replace(from.as_str(), to);
        }
        if let Some(line) = text.lines().find(|line| line.contains(&root)) {
            bail!("{} still names the build root: {line}", file.display());
        }
        // Perl installs its Config read-only; keep the installed mode.
        let permissions = fs::metadata(file)?.permissions();
        let mut writable = permissions.clone();
        std::os::unix::fs::PermissionsExt::set_mode(&mut writable, 0o644);
        fs::set_permissions(file, writable)?;
        fs::write(file, text)?;
        fs::set_permissions(file, permissions)?;
    }
    Ok(())
}

fn build_ninja(repo_root: &Path) -> Result<()> {
    build_cmake_runtime(
        repo_root,
        "ninja",
        "src/build-tools/ninja",
        &[],
        &[
            "-DCMAKE_INSTALL_PREFIX=/usr",
            "-DCMAKE_BUILD_TYPE=Release",
            "-DBUILD_TESTING=OFF",
            "-DNINJA_BUILD_BINARY=ON",
        ],
        "usr/bin/ninja",
    )
}
