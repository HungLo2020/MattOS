fn build_cpython(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/development/python/cpython");
    let out_root = repo_root.join("out/build/cpython");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/cpython.toml"))?;
    let openssl = repo_root.join("out/build/openssl/install/usr");
    let options = vec![
        "--prefix=/usr".to_string(),
        "--libdir=/usr/lib/x86_64-linux-gnu".to_string(),
        "--enable-shared".to_string(),
        "--without-static-libpython".to_string(),
        "--with-ensurepip=install".to_string(),
        "--with-system-expat".to_string(),
        "--disable-test-modules".to_string(),
        format!("--with-openssl={}", openssl.display()),
    ];
    let stamp = format!(
        "{state}\n{}\nlib-dynload=/usr/lib/python3.14/lib-dynload\noptional-modules=no-gdbm,no-readline,no-sqlite3,no-tk,no-uuid\n",
        options.join("\n")
    );
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    fs::create_dir_all(&build_dir)?;
    let mut env = staged_library_environment(
        repo_root,
        &[
            "openssl", "zlib", "bzip2", "xz", "expat", "ncurses", "libffi",
        ],
    )?;
    env.push(("PYTHON_FOR_BUILD", "python3".to_string()));
    if !build_dir.join("Makefile").is_file() {
        let option_refs = options.iter().map(String::as_str).collect::<Vec<_>>();
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            &option_refs,
            &env,
        )?;
    }
    restore_cpython_getpath_vpath(&build_dir)?;
    // A prior interrupted run may have already produced a normalized getpath
    // object. Force the bootstrap interpreter back to the real output-mirror
    // source path before any remaining frozen-module generation.
    remove_path_if_exists(&build_dir.join("Modules/getpath.o"))?;
    let child_jobs = scheduler::child_job_limit().to_string();
    run_cmd_with_env_overrides(&build_dir, "make", &["_bootstrap_python"], &env)?;
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", &child_jobs], &env)?;
    // The bootstrap interpreter needs the real source VPATH while producing
    // frozen modules. Once generation is complete, rebuild only the owning
    // getpath object and its consumers with the deterministic installed-tree
    // fallback before publishing libpython.
    normalize_cpython_getpath_vpath(&build_dir)?;
    remove_path_if_exists(&build_dir.join("Modules/getpath.o"))?;
    // Frozen headers were completed by the real-VPATH bootstrap pass above.
    // Do not rebuild the bootstrap interpreter (and thereby make those headers
    // stale) while relinking only the installed shared library.
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["FREEZE_MODULE_DEPS=", "libpython3.14.so"],
        &env,
    )?;
    let normalized_libpython = out_root.join("libpython3.14.so.1.0.normalized");
    fs::copy(
        build_dir.join("libpython3.14.so.1.0"),
        &normalized_libpython,
    )?;
    // CPython's install recipes also execute the bootstrap interpreter. Put
    // that private build tool back on its real output-mirror path; the
    // installed library is restored from the valid normalized link above.
    restore_cpython_getpath_vpath(&build_dir)?;
    remove_path_if_exists(&build_dir.join("Modules/getpath.o"))?;
    run_cmd_with_env_overrides(&build_dir, "make", &["_bootstrap_python"], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env,
    )?;
    fs::copy(
        &normalized_libpython,
        install_dir.join("usr/lib/x86_64-linux-gnu/libpython3.14.so.1.0"),
    )?;
    // CPython applies --libdir to both libpython and extension modules, but its
    // installed path configuration searches for extension modules below the
    // platform-independent standard-library root. Keep libpython in Debian's
    // multiarch directory while publishing lib-dynload where python3 searches.
    let multiarch_dynload = install_dir.join("usr/lib/x86_64-linux-gnu/python3.14/lib-dynload");
    let runtime_dynload = install_dir.join("usr/lib/python3.14/lib-dynload");
    if multiarch_dynload.is_dir() {
        remove_path_if_exists(&runtime_dynload)?;
        if let Some(parent) = runtime_dynload.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&multiarch_dynload, &runtime_dynload)?;
    }
    for required in [
        "usr/bin/python3",
        "usr/lib/x86_64-linux-gnu/libpython3.14.so.1.0",
        "usr/lib/python3.14/os.py",
        "usr/lib/python3.14/lib-dynload/_ctypes.cpython-314-x86_64-linux-gnu.so",
        "usr/include/python3.14/Python.h",
    ] {
        if !install_dir.join(required).exists() {
            bail!("CPython install did not produce {required}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

/// Keep Make's real VPATH for source discovery while preventing CPython's
/// generated getpath object from compiling that checkout path into libpython.
/// Installed Python resolves its standard library from the executable prefix;
/// this macro is only a development-tree fallback.
fn normalize_cpython_getpath_vpath(build_dir: &Path) -> Result<()> {
    let makefile = build_dir.join("Makefile");
    let mut contents = fs::read_to_string(&makefile)
        .with_context(|| format!("read generated {}", makefile.display()))?;
    let original = "-DVPATH='\"$(VPATH)\"'";
    let normalized = "-DVPATH='\"/usr/src/mattos/cpython\"'";
    if contents.contains(original) {
        contents = contents.replacen(original, normalized, 1);
    } else if !contents.contains(normalized) {
        bail!(
            "generated {} lacks expected CPython getpath VPATH definition",
            makefile.display()
        );
    }
    fs::write(&makefile, contents)
        .with_context(|| format!("normalize generated {}", makefile.display()))?;
    Ok(())
}

fn restore_cpython_getpath_vpath(build_dir: &Path) -> Result<()> {
    let makefile = build_dir.join("Makefile");
    let mut contents = fs::read_to_string(&makefile)
        .with_context(|| format!("read generated {}", makefile.display()))?;
    let original = "-DVPATH='\"$(VPATH)\"'";
    let normalized = "-DVPATH='\"/usr/src/mattos/cpython\"'";
    if contents.contains(normalized) {
        contents = contents.replacen(normalized, original, 1);
        fs::write(&makefile, contents)
            .with_context(|| format!("restore generated {}", makefile.display()))?;
    } else if !contents.contains(original) {
        bail!(
            "generated {} lacks expected CPython getpath VPATH definition",
            makefile.display()
        );
    }
    Ok(())
}
