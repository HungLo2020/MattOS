fn build_bzip2(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/bzip2");
    if !source.join("Makefile-libbz2_so").is_file() {
        bail!(
            "bzip2 source not found in {}; run upstream import bzip2 first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/bzip2");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/bzip2.toml"))
        .context("failed to read bzip2 upstream state")?;
    let stamp = format!("{state}\nMakefile-libbz2_so\n");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    let cflags = format!(
        "-O2 -g0 -fPIC -ffile-prefix-map={}=/usr/src/mattos/bzip2 -fdebug-prefix-map={}=/usr/src/mattos/bzip2 -fmacro-prefix-map={}=/usr/src/mattos/bzip2",
        repo_root.display(),
        repo_root.display(),
        repo_root.display()
    );
    // Makefile-libbz2_so assigns CFLAGS with `=`, so an environment variable
    // alone is deliberately insufficient.  A make command-line assignment has
    // precedence and keeps the imported Makefile untouched.
    let cflags_override = format!("CFLAGS={cflags}");
    run_cmd_with_env_overrides(
        &source_copy,
        "make",
        &[
            "-B",
            "-f",
            "Makefile-libbz2_so",
            "-j",
            "4",
            &cflags_override,
        ],
        &[("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string())],
    )?;
    run_cmd_with_env_overrides(
        &source_copy,
        "make",
        &["-B", "-j", "4", &cflags_override],
        &[("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string())],
    )?;
    remove_path_if_exists(&install_dir)?;
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    let includedir = install_dir.join("usr/include");
    fs::create_dir_all(&libdir)?;
    fs::create_dir_all(&includedir)?;
    fs::copy(
        source_copy.join("libbz2.so.1.0.8"),
        libdir.join("libbz2.so.1.0.8"),
    )?;
    std::os::unix::fs::symlink("libbz2.so.1.0.8", libdir.join("libbz2.so.1.0"))?;
    std::os::unix::fs::symlink("libbz2.so.1.0", libdir.join("libbz2.so"))?;
    fs::copy(source_copy.join("bzlib.h"), includedir.join("bzlib.h"))?;
    let bindir = install_dir.join("usr/bin");
    fs::create_dir_all(&bindir)?;
    for binary in ["bzip2", "bzip2recover"] {
        fs::copy(source_copy.join(binary), bindir.join(binary))?;
        set_mode(bindir.join(binary), 0o755)?;
    }
    std::os::unix::fs::symlink("bzip2", bindir.join("bunzip2"))?;
    std::os::unix::fs::symlink("bzip2", bindir.join("bzcat"))?;
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_lz4(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/lz4");
    if !source.join("lib/Makefile").is_file() {
        bail!(
            "LZ4 source not found in {}; run upstream import lz4 first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/lz4");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/lz4.toml"))
        .context("failed to read LZ4 upstream state")?;
    let stamp = format!("{state}\nmake lib\n");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    let library_source = source_copy.join("lib");
    run_cmd(&library_source, "make", &["-j", "4", "lib"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        &library_source,
        "make",
        &[
            "install",
            &format!("DESTDIR={}", install_dir.display()),
            "PREFIX=/usr",
            "LIBDIR=/usr/lib/x86_64-linux-gnu",
        ],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/liblz4.so.1");
    if !soname.exists() {
        bail!("LZ4 install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_xz(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/xz");
    if !source.join("configure.ac").is_file() {
        bail!(
            "XZ Utils source not found in {}; run upstream import xz first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/xz");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/xz.toml"))
        .context("failed to read XZ Utils upstream state")?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--disable-nls",
        "--disable-doc",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "./autogen.sh", &["--no-po4a"])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
        )?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/liblzma.so.5");
    if !soname.exists() {
        bail!("XZ Utils install did not produce {}", soname.display());
    }
    for binary in ["xz", "unxz", "xzcat"] {
        if !install_dir.join("usr/bin").join(binary).exists() {
            bail!("XZ Utils install did not produce usr/bin/{binary}");
        }
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_xxhash(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/xxhash");
    if !source.join("Makefile").is_file() {
        bail!(
            "xxHash source not found in {}; run upstream import xxhash first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/xxhash");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/xxhash.toml"))
        .context("failed to read xxHash upstream state")?;
    let stamp = format!("{state}\nmake libxxhash\n");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    run_cmd(&source_copy, "make", &["-j", "4", "libxxhash"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        &source_copy,
        "make",
        &[
            "install_libxxhash",
            "install_libxxhash.includes",
            "install_libxxhash.pc",
            &format!("DESTDIR={}", install_dir.display()),
            "PREFIX=/usr",
            "LIBDIR=/usr/lib/x86_64-linux-gnu",
            "INCLUDEDIR=/usr/include",
            "PKGCONFIGDIR=/usr/lib/x86_64-linux-gnu/pkgconfig",
        ],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libxxhash.so.0");
    if !soname.exists() {
        bail!("xxHash install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_zstd(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/zstd");
    if !source.join("build/cmake/CMakeLists.txt").is_file() {
        bail!(
            "Zstandard source not found in {}; run upstream import zstd first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/zstd");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/zstd.toml"))
        .context("failed to read Zstandard upstream state")?;
    let options = [
        "-G",
        "Ninja",
        "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_INSTALL_PREFIX=/usr",
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
        "-DZSTD_BUILD_PROGRAMS=ON",
        "-DZSTD_BUILD_TESTS=OFF",
        // Upstream's CLI links its static library by design; the MattOS
        // runtime package still publishes only the shared SONAME.
        "-DZSTD_BUILD_STATIC=ON",
        "-DZSTD_BUILD_SHARED=ON",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    if !build_dir.join("build.ninja").is_file() {
        let cmake_source = source.join("build/cmake");
        let mut args = vec!["-S", path_str(&cmake_source)?, "-B", path_str(&build_dir)?];
        args.extend(options);
        run_cmd(repo_root, "cmake", &args)?;
    }
    run_cmd(
        repo_root,
        "cmake",
        &["--build", path_str(&build_dir)?, "--parallel", "4"],
    )?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build_dir)?],
        &[("DESTDIR", install_dir.display().to_string())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libzstd.so.1");
    if !soname.exists() {
        bail!("Zstandard install did not produce {}", soname.display());
    }
    if !install_dir.join("usr/bin/zstd").is_file() {
        bail!("Zstandard install did not produce usr/bin/zstd");
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_gpg_autotools_library(
    repo_root: &Path,
    component: &str,
    dependency_components: &[&str],
    expected_soname: &str,
) -> Result<()> {
    let source = repo_root.join("src/system/security").join(component);
    let out_root = repo_root.join("out/build").join(component);
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(
        repo_root
            .join("upstream/state")
            .join(format!("{component}.toml")),
    )
    .with_context(|| format!("failed to read {component} upstream state"))?;

    let mut include_dirs = Vec::new();
    let mut library_dirs = Vec::new();
    let mut pkgconfig_dirs = Vec::new();
    for dependency in dependency_components {
        let usr = repo_root
            .join("out/build")
            .join(dependency)
            .join("install/usr");
        include_dirs.push(usr.join("include"));
        library_dirs.push(usr.join("lib/x86_64-linux-gnu"));
        pkgconfig_dirs.push(usr.join("lib/x86_64-linux-gnu/pkgconfig"));
    }
    let cppflags = include_dirs
        .iter()
        .map(|path| format!("-I{}", path.display()))
        .collect::<Vec<_>>()
        .join(" ");
    let ldflags = library_dirs
        .iter()
        .map(|path| format!("-L{}", path.display()))
        .collect::<Vec<_>>()
        .join(" ");
    let library_path = std::env::join_paths(&library_dirs)?
        .to_string_lossy()
        .to_string();
    let pkgconfig_path = std::env::join_paths(&pkgconfig_dirs)?
        .to_string_lossy()
        .to_string();
    let mut tool_path = dependency_components
        .iter()
        .map(|dependency| {
            repo_root
                .join("out/build")
                .join(dependency)
                .join("install/usr/bin")
        })
        .collect::<Vec<_>>();
    tool_path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let tool_path = std::env::join_paths(tool_path)?
        .to_string_lossy()
        .to_string();
    let env_overrides = [
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("CPPFLAGS", cppflags),
        ("LDFLAGS", ldflags),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        ("PKG_CONFIG_PATH", pkgconfig_path),
        ("PATH", tool_path),
    ];
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--disable-doc",
        "--disable-tests",
        "--disable-nls",
    ];
    let stamp = format!(
        "{state}\n{}\n{}\n",
        options.join("\n"),
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
        remove_path_if_exists(&install_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "autoreconf", &["-fi"])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
            &env_overrides,
        )?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env_overrides)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env_overrides,
    )?;
    let soname = install_dir
        .join("usr/lib/x86_64-linux-gnu")
        .join(expected_soname);
    if !soname.exists() {
        bail!("{component} install did not produce {}", soname.display());
    }
    remove_path_if_exists(&install_dir.join(format!("usr/lib/x86_64-linux-gnu/{component}.la")))?;
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_gpgv(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/security/gnupg");
    let out_root = repo_root.join("out/build/gpgv");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let dependencies = [
        "libgpg-error",
        "libgcrypt",
        "libassuan",
        "libksba",
        "npth",
        "zlib",
    ];
    let mut include_dirs = Vec::new();
    let mut library_dirs = Vec::new();
    let mut pkgconfig_dirs = Vec::new();
    for dependency in dependencies {
        let usr = repo_root
            .join("out/build")
            .join(dependency)
            .join("install/usr");
        include_dirs.push(usr.join("include"));
        library_dirs.push(usr.join("lib/x86_64-linux-gnu"));
        pkgconfig_dirs.push(usr.join("lib/x86_64-linux-gnu/pkgconfig"));
    }
    let library_path = std::env::join_paths(&library_dirs)?
        .to_string_lossy()
        .to_string();
    let mut tool_path = dependencies
        .iter()
        .map(|dependency| {
            repo_root
                .join("out/build")
                .join(dependency)
                .join("install/usr/bin")
        })
        .collect::<Vec<_>>();
    tool_path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let tool_path = std::env::join_paths(tool_path)?
        .to_string_lossy()
        .to_string();
    let env_overrides = [
        (
            "CPPFLAGS",
            include_dirs
                .iter()
                .map(|path| format!("-I{}", path.display()))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            "LDFLAGS",
            library_dirs
                .iter()
                .map(|path| format!("-L{}", path.display()))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths(&pkgconfig_dirs)?
                .to_string_lossy()
                .to_string(),
        ),
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("PATH", tool_path),
    ];
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-doc",
        "--disable-tests",
        "--disable-nls",
        "--disable-ldap",
        "--disable-card-support",
        "--disable-ntbtls",
        "--disable-gnutls",
        "--disable-sqlite",
        "--disable-bzip2",
    ];
    let state = fs::read_to_string(repo_root.join("upstream/state/gnupg.toml"))
        .context("failed to read GnuPG upstream state")?;
    let stamp = format!(
        "{state}\n{}\n{}\n",
        options.join("\n"),
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
        remove_path_if_exists(&install_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "autoreconf", &["-fi"])?;
    }
    let common_dir = source_copy.join("common");
    run_cmd(
        &common_dir,
        "sh",
        &[
            "-c",
            "awk -f exaudit.awk audit.h | awk -f mkstrtable.awk -v textidx=3 -v nogettext=1 -v pkg_namespace=eventstr_ > audit-events.h && awk -f exstatus.awk status.h | awk -f mkstrtable.awk -v textidx=3 -v nogettext=1 -v pkg_namespace=statusstr_ > status-codes.h",
        ],
    )?;
    run_cmd(
        &source_copy.join("regexp"),
        "sh",
        &[
            "-c",
            "awk -f parse-unidata.awk UnicodeData.txt > _unicode_mapping.c",
        ],
    )?;
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
            &env_overrides,
        )?;
    }
    let build_common_dir = build_dir.join("common");
    fs::create_dir_all(&build_common_dir)?;
    for generated in ["audit-events.h", "status-codes.h"] {
        fs::copy(
            source_copy.join("common").join(generated),
            build_common_dir.join(generated),
        )?;
    }
    fs::create_dir_all(build_dir.join("regexp"))?;
    fs::copy(
        source_copy.join("regexp/_unicode_mapping.c"),
        build_dir.join("regexp/_unicode_mapping.c"),
    )?;
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env_overrides)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env_overrides,
    )?;
    if !install_dir.join("usr/bin/gpgv").is_file() {
        bail!("GnuPG install did not produce usr/bin/gpgv");
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}

fn build_openssl(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/openssl");
    if !source.join("Configure").is_file() {
        bail!(
            "OpenSSL source not found in {}; run upstream import openssl first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/openssl");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let zlib = repo_root.join("out/build/zlib/install/usr");
    let zstd = repo_root.join("out/build/zstd/install/usr");
    let zlib_lib = zlib.join("lib/x86_64-linux-gnu");
    let zstd_lib = zstd.join("lib/x86_64-linux-gnu");
    if !zlib_lib.join("libz.so").exists() || !zstd_lib.join("libzstd.so").exists() {
        bail!("MattOS OpenSSL dependencies are missing; run build zlib and build zstd first")
    }
    let state = fs::read_to_string(repo_root.join("upstream/state/openssl.toml"))
        .context("failed to read OpenSSL upstream state")?;
    let options = openssl_configure_options(&zlib, &zstd);
    let library_path = std::env::join_paths([&zlib_lib, &zstd_lib])?
        .to_string_lossy()
        .to_string();
    let env = [
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        (
            "CPPFLAGS",
            format!(
                "-I{} -I{}",
                zlib.join("include").display(),
                zstd.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!("-L{} -L{}", zlib_lib.display(), zstd_lib.display()),
        ),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths([zlib_lib.join("pkgconfig"), zstd_lib.join("pkgconfig")])?
                .to_string_lossy()
                .to_string(),
        ),
    ];
    let stamp = format!(
        "{state}\n{}\n{}\n",
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
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        let option_refs = options.iter().map(String::as_str).collect::<Vec<_>>();
        run_cmd_with_env_overrides(
            &build_dir,
            "perl",
            &[path_str(&source_copy.join("Configure"))?]
                .into_iter()
                .chain(option_refs)
                .collect::<Vec<_>>()
                .as_slice(),
            &env,
        )?;
    }
    let build_info = Command::new("perl")
        .arg(source_copy.join("util/mkbuildinf.pl"))
        .arg("gcc -O2 -fPIC")
        .arg("linux-x86_64")
        .env("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string())
        .output()
        .context("failed to generate sanitized OpenSSL build information")?;
    if !build_info.status.success() {
        bail!(
            "OpenSSL build-information generator failed: {}",
            String::from_utf8_lossy(&build_info.stderr)
        )
    }
    let build_info_path = build_dir.join("crypto/buildinf.h");
    fs::create_dir_all(
        build_info_path
            .parent()
            .ok_or_else(|| anyhow!("invalid OpenSSL build-information path"))?,
    )?;
    fs::write(&build_info_path, build_info.stdout)
        .with_context(|| format!("failed to write {}", build_info_path.display()))?;
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install_sw", &format!("DESTDIR={}", install_dir.display())],
        &env,
    )?;
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    for soname in ["libcrypto.so.3", "libssl.so.3"] {
        if !libdir.join(soname).exists() {
            bail!(
                "OpenSSL install did not produce {}",
                libdir.join(soname).display()
            )
        }
    }
    let search_dirs: [&Path; 3] = [&libdir, &zlib_lib, &zstd_lib];
    validate_dependency_resolves_from(
        &libdir.join("libcrypto.so.3"),
        "libz.so.1",
        &zlib_lib,
        &search_dirs,
    )?;
    validate_dependency_resolves_from(
        &libdir.join("libcrypto.so.3"),
        "libzstd.so.1",
        &zstd_lib,
        &search_dirs,
    )?;
    validate_dependency_resolves_from(
        &libdir.join("libssl.so.3"),
        "libcrypto.so.3",
        &libdir,
        &search_dirs,
    )?;
    fs::write(&stamp_path, stamp)?;
    println!(
        "OpenSSL origins: zlib={} zstd={}; OPENSSLDIR=/etc/ssl",
        zlib_lib.display(),
        zstd_lib.display()
    );
    Ok(())
}

fn openssl_configure_options(zlib: &Path, zstd: &Path) -> Vec<String> {
    let zlib_lib = zlib.join("lib/x86_64-linux-gnu");
    let zstd_lib = zstd.join("lib/x86_64-linux-gnu");
    vec![
        "linux-x86_64".to_string(),
        "shared".to_string(),
        "zlib".to_string(),
        "enable-zstd".to_string(),
        "no-tests".to_string(),
        "no-docs".to_string(),
        "no-apps".to_string(),
        "no-legacy".to_string(),
        "no-module".to_string(),
        "--prefix=/usr".to_string(),
        "--openssldir=/etc/ssl".to_string(),
        "--libdir=lib/x86_64-linux-gnu".to_string(),
        format!("--with-zlib-include={}", zlib.join("include").display()),
        format!("--with-zlib-lib={}", zlib_lib.display()),
        format!("--with-zstd-include={}", zstd.join("include").display()),
        format!("--with-zstd-lib={}", zstd_lib.display()),
    ]
}

fn build_elfutils(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/elfutils");
    if !source.join("configure.ac").is_file() {
        bail!(
            "elfutils source not found in {}; run upstream import elfutils first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/elfutils");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let zlib = repo_root.join("out/build/zlib/install/usr");
    let zstd = repo_root.join("out/build/zstd/install/usr");
    let zlib_lib = zlib.join("lib/x86_64-linux-gnu");
    let zstd_lib = zstd.join("lib/x86_64-linux-gnu");
    if !zlib_lib.join("libz.so").exists() || !zstd_lib.join("libzstd.so").exists() {
        bail!("MattOS elfutils dependencies are missing; run build zlib and build zstd first")
    }
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--enable-maintainer-mode",
        "--disable-nls",
        "--disable-libdebuginfod",
        "--disable-debuginfod",
        "--disable-demangler",
        "--with-zlib",
        "--with-zstd",
        "--without-bzlib",
        "--without-lzma",
    ];
    let library_path = std::env::join_paths([&zlib_lib, &zstd_lib])?
        .to_string_lossy()
        .to_string();
    let env = [
        (
            "CPPFLAGS",
            format!(
                "-I{} -I{}",
                zlib.join("include").display(),
                zstd.join("include").display()
            ),
        ),
        (
            "LDFLAGS",
            format!("-L{} -L{}", zlib_lib.display(), zstd_lib.display()),
        ),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths([zlib_lib.join("pkgconfig"), zstd_lib.join("pkgconfig")])?
                .to_string_lossy()
                .to_string(),
        ),
    ];
    let state = fs::read_to_string(repo_root.join("upstream/state/elfutils.toml"))
        .context("failed to read elfutils upstream state")?;
    let stamp = format!(
        "{state}\n{}\n{}\n",
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
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "autoreconf", &["-fi"])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
            &env,
        )?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-C", "lib", "-j", "4"], &env)?;
    run_cmd_with_env_overrides(&build_dir, "make", &["-C", "libelf", "-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &[
            "-C",
            "libelf",
            "install",
            &format!("DESTDIR={}", install_dir.display()),
        ],
        &env,
    )?;
    let pkgconfig = install_dir.join("usr/lib/x86_64-linux-gnu/pkgconfig");
    fs::create_dir_all(&pkgconfig)?;
    fs::copy(
        build_dir.join("config/libelf.pc"),
        pkgconfig.join("libelf.pc"),
    )?;
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    if !libdir.join("libelf.so.1").exists() {
        bail!(
            "elfutils install did not produce {}",
            libdir.join("libelf.so.1").display()
        )
    }
    let search_dirs: [&Path; 3] = [&libdir, &zlib_lib, &zstd_lib];
    validate_dependency_resolves_from(
        &libdir.join("libelf.so.1"),
        "libz.so.1",
        &zlib_lib,
        &search_dirs,
    )?;
    validate_dependency_resolves_from(
        &libdir.join("libelf.so.1"),
        "libzstd.so.1",
        &zstd_lib,
        &search_dirs,
    )?;
    fs::write(&stamp_path, stamp)?;
    println!(
        "libelf origins: zlib={} zstd={}",
        zlib_lib.display(),
        zstd_lib.display()
    );
    Ok(())
}

fn build_pcre2(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/pcre2");
    if !source.join("CMakeLists.txt").is_file() {
        bail!(
            "PCRE2 source not found in {}; run upstream import pcre2 first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/pcre2");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/pcre2.toml"))
        .context("failed to read PCRE2 upstream state")?;
    let sljit = repo_root.join("src/build-support/sljit");
    if !sljit.join("sljit_src/sljitLir.c").is_file() {
        bail!("PCRE2 SLJIT source is missing; run upstream import sljit first");
    }
    let sljit_state = fs::read_to_string(repo_root.join("upstream/state/sljit.toml"))
        .context("failed to read SLJIT upstream state")?;
    let options = [
        "-G",
        "Ninja",
        "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_INSTALL_PREFIX=/usr",
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
        "-DBUILD_SHARED_LIBS=ON",
        "-DBUILD_STATIC_LIBS=OFF",
        "-DPCRE2_BUILD_PCRE2_8=ON",
        "-DPCRE2_BUILD_PCRE2_16=OFF",
        "-DPCRE2_BUILD_PCRE2_32=OFF",
        "-DPCRE2_BUILD_PCRE2GREP=OFF",
        "-DPCRE2_BUILD_TESTS=OFF",
        "-DPCRE2_SUPPORT_JIT=ON",
        "-DPCRE2_SUPPORT_UNICODE=ON",
        "-DPCRE2_SYMVERS=ON",
    ];
    let stamp = format!("{state}\n{sljit_state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    copy_imported_working_tree(
        repo_root,
        Path::new("src/system/libraries/pcre2"),
        &source_copy,
    )?;
    copy_imported_working_tree(
        repo_root,
        Path::new("src/build-support/sljit"),
        &source_copy.join("deps/sljit"),
    )?;
    if !build_dir.join("build.ninja").is_file() {
        let mut args = vec!["-S", path_str(&source_copy)?, "-B", path_str(&build_dir)?];
        args.extend(options);
        run_cmd(repo_root, "cmake", &args)?;
    }
    run_cmd(
        repo_root,
        "cmake",
        &["--build", path_str(&build_dir)?, "--parallel", "4"],
    )?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build_dir)?],
        &[("DESTDIR", install_dir.display().to_string())],
    )?;
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    let soname = libdir.join("libpcre2-8.so.0");
    if !soname.exists() {
        bail!("PCRE2 install did not produce {}", soname.display());
    }
    for unwanted in ["libpcre2-16.so", "libpcre2-32.so"] {
        if libdir.join(unwanted).exists() {
            bail!("PCRE2 unexpectedly built non-runtime variant {unwanted}");
        }
    }
    fs::write(&stamp_path, stamp)?;
    println!("PCRE2 origin: {}", install_dir.display());
    Ok(())
}

fn build_selinux(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/security/selinux");
    if !source.join("libselinux/src/Makefile").is_file() {
        bail!(
            "SELinux source not found in {}; run upstream import selinux first",
            source.display()
        );
    }
    let pcre2 = repo_root.join("out/build/pcre2/install/usr");
    let pcre2_lib = pcre2.join("lib/x86_64-linux-gnu");
    if !pcre2.join("include/pcre2.h").is_file() || !pcre2_lib.join("libpcre2-8.so").exists() {
        bail!("MattOS-built PCRE2 development files are missing; run build pcre2 first");
    }
    let out_root = repo_root.join("out/build/selinux");
    let source_copy = out_root.join("source");
    let sepol_install = out_root.join("sepol-install");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/selinux.toml"))
        .context("failed to read SELinux upstream state")?;
    let pcre2_state = fs::read_to_string(repo_root.join("upstream/state/pcre2.toml"))
        .context("failed to read PCRE2 upstream state")?;
    let make_vars = [
        "PREFIX=/usr".to_string(),
        "LIBDIR=/usr/lib/x86_64-linux-gnu".to_string(),
        "SHLIBDIR=/usr/lib/x86_64-linux-gnu".to_string(),
        "USE_PCRE2=y".to_string(),
        "DISABLE_SETRANS=y".to_string(),
        "DISABLE_RPM=y".to_string(),
        format!(
            "PCRE_CFLAGS=-DUSE_PCRE2 -DPCRE2_CODE_UNIT_WIDTH=8 -I{}",
            pcre2.join("include").display()
        ),
        format!("PCRE_LDLIBS=-L{} -lpcre2-8", pcre2_lib.display()),
    ];
    let sepol_make_vars = [
        "PREFIX=/usr".to_string(),
        "LIBDIR=/usr/lib/x86_64-linux-gnu".to_string(),
        "SHLIBDIR=/usr/lib/x86_64-linux-gnu".to_string(),
        "DISABLE_CIL=y".to_string(),
        "DISABLE_SHARED=y".to_string(),
    ];
    let library_path = pcre2_lib.display().to_string();
    let env = [
        ("LDFLAGS", format!("-L{}", pcre2_lib.display())),
        ("LIBRARY_PATH", library_path.clone()),
        ("LD_LIBRARY_PATH", library_path),
        (
            "PKG_CONFIG_PATH",
            pcre2_lib.join("pkgconfig").display().to_string(),
        ),
    ];
    let stamp = format!(
        "{state}\n{pcre2_state}\n{}\n{}\n{}\n",
        make_vars.join("\n"),
        sepol_make_vars.join("\n"),
        env.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    let libsepol = source_copy.join("libsepol");
    let mut sepol_build_args = vec!["-C", "src", "-j", "4", "all"];
    sepol_build_args.extend(sepol_make_vars.iter().map(String::as_str));
    run_cmd(&libsepol, "make", &sepol_build_args)?;
    remove_path_if_exists(&sepol_install)?;
    let sepol_destdir = format!("DESTDIR={}", sepol_install.display());
    let mut sepol_install_args = vec!["-C", "src", "install", sepol_destdir.as_str()];
    sepol_install_args.extend(sepol_make_vars.iter().map(String::as_str));
    run_cmd(&libsepol, "make", &sepol_install_args)?;
    run_cmd(
        &libsepol,
        "make",
        &[
            "-C",
            "include",
            "install",
            sepol_destdir.as_str(),
            "PREFIX=/usr",
        ],
    )?;
    let sepol_lib = sepol_install.join("usr/lib/x86_64-linux-gnu");
    if !sepol_install.join("usr/include/sepol/sepol.h").is_file()
        || !sepol_lib.join("libsepol.a").is_file()
    {
        bail!("MattOS-built libsepol development files are incomplete");
    }
    copy_tree_contents(
        &sepol_install.join("usr/include"),
        &repo_root.join("out/sysroot/usr/include"),
    )?;
    copy_tree_contents(
        &sepol_lib,
        &repo_root.join("out/sysroot/usr/lib/x86_64-linux-gnu"),
    )?;
    let libselinux = source_copy.join("libselinux");
    let mut build_args = vec!["-C", "src", "-j", "4", "all"];
    build_args.extend(make_vars.iter().map(String::as_str));
    run_cmd_with_env_overrides(&libselinux, "make", &build_args, &env)?;
    remove_path_if_exists(&install_dir)?;
    let destdir = format!("DESTDIR={}", install_dir.display());
    let mut install_args = vec!["-C", "src", "install", destdir.as_str()];
    install_args.extend(make_vars.iter().map(String::as_str));
    run_cmd_with_env_overrides(&libselinux, "make", &install_args, &env)?;
    run_cmd(
        &libselinux,
        "make",
        &["-C", "include", "install", destdir.as_str(), "PREFIX=/usr"],
    )?;
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    let soname = libdir.join("libselinux.so.1");
    if !soname.exists() {
        bail!("SELinux install did not produce {}", soname.display());
    }
    validate_dependency_resolves_from(&soname, "libpcre2-8.so.0", &pcre2_lib, &[&pcre2_lib])?;
    let dynamic = run_cmd_capture(repo_root, "readelf", &["-d", path_str(&soname)?])?;
    if dynamic.contains("libsepol.so") {
        bail!("libselinux unexpectedly retained a dynamic libsepol dependency");
    }
    fs::write(&stamp_path, stamp)?;
    println!(
        "libselinux origin: {}; PCRE2 origin: {}",
        install_dir.display(),
        pcre2_lib.display()
    );
    Ok(())
}

const LIBXCRYPT_REQUIRED_SYMBOL_VERSIONS: &[&str] =
    &["GLIBC_2.2.5", "XCRYPT_2.0", "XCRYPT_4.3", "XCRYPT_4.4"];

fn libxcrypt_configure_options() -> [&'static str; 7] {
    [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--enable-shared",
        "--enable-hashes=all",
        "--enable-obsolete-api=glibc",
        "--disable-xcrypt-compat-files",
    ]
}

fn build_libxcrypt(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/libxcrypt");
    if !source.join("configure.ac").is_file() {
        bail!(
            "libxcrypt source not found in {}; run upstream import libxcrypt first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/libxcrypt");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/libxcrypt.toml"))
        .context("failed to read libxcrypt upstream state")?;
    let options = libxcrypt_configure_options();
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    apply_component_patches(repo_root, "libxcrypt", &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "./autogen.sh", &[])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
        )?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    run_cmd(&build_dir, "make", &["check", "-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libcrypt.so.1");
    if !soname.exists() {
        bail!("libxcrypt install did not produce {}", soname.display());
    }
    let versions = run_cmd_capture(
        repo_root,
        "readelf",
        &["--version-info", path_str(&soname)?],
    )?;
    for required in LIBXCRYPT_REQUIRED_SYMBOL_VERSIONS {
        if !versions.contains(required) {
            bail!("libxcrypt is missing required symbol version {required}");
        }
    }
    fs::write(&stamp_path, stamp)?;
    println!(
        "libxcrypt origin: {}; yescrypt covered by upstream check suite",
        install_dir.display()
    );
    Ok(())
}

fn build_libmd(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/libmd");
    if !source.join("configure.ac").is_file() {
        bail!(
            "libmd source not found in {}; run upstream import libmd first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/libmd");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/libmd.toml"))
        .context("failed to read libmd upstream state")?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    fs::write(source_copy.join(".dist-version"), "1.2.0\n")?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "./autogen", &[])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
        )?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libmd.so.0");
    if !soname.exists() {
        bail!("libmd install did not produce {}", soname.display());
    }
    remove_path_if_exists(&install_dir.join("usr/lib/x86_64-linux-gnu/libmd.la"))?;
    fs::write(&stamp_path, stamp)?;
    println!("libmd origin: {}", install_dir.display());
    Ok(())
}

fn build_libbsd(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/libbsd");
    if !source.join("configure.ac").is_file() {
        bail!(
            "libbsd source not found in {}; run upstream import libbsd first",
            source.display()
        );
    }
    let libmd_install = repo_root.join("out/build/libmd/install/usr");
    let libmd_lib = libmd_install.join("lib/x86_64-linux-gnu");
    if !libmd_install.join("include/md5.h").is_file() || !libmd_lib.join("libmd.so").exists() {
        bail!(
            "MattOS-built libmd development files missing at {}; run build libmd first",
            libmd_install.display()
        );
    }
    let out_root = repo_root.join("out/build/libbsd");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/libbsd.toml"))
        .context("failed to read libbsd upstream state")?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
    ];
    let env_overrides = [
        (
            "CPPFLAGS",
            format!("-I{}", libmd_install.join("include").display()),
        ),
        ("LDFLAGS", format!("-L{}", libmd_lib.display())),
        ("LIBRARY_PATH", libmd_lib.display().to_string()),
        ("LD_LIBRARY_PATH", libmd_lib.display().to_string()),
        (
            "PKG_CONFIG_PATH",
            libmd_lib.join("pkgconfig").display().to_string(),
        ),
    ];
    let stamp = format!(
        "{state}\n{}\n{}\n",
        options.join("\n"),
        env_overrides
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    fs::write(source_copy.join(".dist-version"), "0.12.2\n")?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "./autogen", &[])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
            &env_overrides,
        )?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env_overrides)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env_overrides,
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libbsd.so.0");
    if !soname.exists() {
        bail!("libbsd install did not produce {}", soname.display());
    }
    let libdir = install_dir.join("usr/lib/x86_64-linux-gnu");
    let linker_name = libdir.join("libbsd.so");
    let versioned_target = fs::read_link(&soname).context("libbsd SONAME link is not a symlink")?;
    remove_path_if_exists(&linker_name)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&versioned_target, &linker_name)?;
    remove_path_if_exists(&libdir.join("libbsd.la"))?;
    validate_dependency_resolves_from(&soname, "libmd.so.0", &libmd_lib, &[&libmd_lib])?;
    fs::write(&stamp_path, stamp)?;
    println!(
        "libbsd origin: {}; libmd origin: {}",
        install_dir.display(),
        libmd_lib.display()
    );
    Ok(())
}

fn build_libndp(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/network/libndp");
    let out_root = repo_root.join("out/build/libndp");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/libndp.toml"))?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--disable-nls",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
        remove_path_if_exists(&install_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "autoreconf", &["-fi"])?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &options,
        )?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
    )?;
    if !install_dir
        .join("usr/lib/x86_64-linux-gnu/libndp.so.0")
        .exists()
        || !install_dir
            .join("usr/lib/x86_64-linux-gnu/pkgconfig/libndp.pc")
            .exists()
    {
        bail!("libndp install did not produce its runtime library and pkg-config metadata");
    }
    remove_path_if_exists(&install_dir.join("usr/lib/x86_64-linux-gnu/libndp.la"))?;
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_readline(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "readline",
        "src/system/userland/readline",
        &["ncurses"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
            "--with-curses",
        ],
        &[
            "usr/lib/x86_64-linux-gnu/libreadline.so.8",
            "usr/lib/x86_64-linux-gnu/pkgconfig/readline.pc",
        ],
    )?;
    let pc =
        repo_root.join("out/build/readline/install/usr/lib/x86_64-linux-gnu/pkgconfig/readline.pc");
    let body = fs::read_to_string(&pc)?
        .lines()
        .map(|line| {
            if line.starts_with("Libs:") && !line.contains("-lncursesw") {
                format!("{line} -lncursesw")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(pc, body)?;
    Ok(())
}

