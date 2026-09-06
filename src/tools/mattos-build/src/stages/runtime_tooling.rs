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

fn build_llvm(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/toolchain/llvm-project/llvm");
    let out_root = repo_root.join("out/build/llvm");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/llvm.toml"))?;
    let options = vec![
        "-G".to_string(),
        "Ninja".to_string(),
        format!("-S{}", source.display()),
        format!("-B{}", build_dir.display()),
        "-DCMAKE_BUILD_TYPE=Release".to_string(),
        "-DCMAKE_INSTALL_PREFIX=/usr".to_string(),
        // MattOS deliberately normalizes llvm-config's generated development-tree
        // roots after configuration.  Suppress Ninja's implicit CMake rerun so
        // that it cannot silently regenerate BuildVariables.inc afterward.
        "-DCMAKE_SUPPRESS_REGENERATION=ON".to_string(),
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu".to_string(),
        "-DLLVM_LIBDIR_SUFFIX=/x86_64-linux-gnu".to_string(),
        "-DLLVM_INSTALL_PACKAGE_DIR=lib/x86_64-linux-gnu/cmake/llvm".to_string(),
        "-DCLANG_INSTALL_PACKAGE_DIR=lib/x86_64-linux-gnu/cmake/clang".to_string(),
        "-DCLANG_CONFIG_FILE_SYSTEM_DIR=/etc/clang".to_string(),
        "-DLLD_INSTALL_PACKAGE_DIR=lib/x86_64-linux-gnu/cmake/lld".to_string(),
        "-DLLVM_FORCE_VC_REPOSITORY=https://github.com/llvm/llvm-project.git".to_string(),
        "-DLLVM_FORCE_VC_REVISION=ca7933e47d3a3451d81e72ac174dcb5aa28b59d1".to_string(),
        "-DLLVM_ENABLE_PROJECTS=clang;lld".to_string(),
        // AMDGPU is a userspace compiler backend required by radeonsi/RADV;
        // it does not add a MattOS CPU architecture target.
        "-DLLVM_TARGETS_TO_BUILD=X86;AArch64;RISCV;AMDGPU".to_string(),
        "-DLLVM_ENABLE_ASSERTIONS=OFF".to_string(),
        "-DLLVM_INCLUDE_TESTS=OFF".to_string(),
        "-DLLVM_INCLUDE_EXAMPLES=OFF".to_string(),
        "-DLLVM_INCLUDE_BENCHMARKS=OFF".to_string(),
        "-DLLVM_ENABLE_BINDINGS=OFF".to_string(),
        "-DLLVM_ENABLE_TERMINFO=OFF".to_string(),
        "-DLLVM_ENABLE_LIBXML2=OFF".to_string(),
        "-DLLVM_ENABLE_LIBEDIT=OFF".to_string(),
        "-DLLVM_ENABLE_ZLIB=FORCE_ON".to_string(),
        "-DLLVM_ENABLE_ZSTD=FORCE_ON".to_string(),
        "-DLLVM_BUILD_LLVM_DYLIB=ON".to_string(),
        "-DLLVM_LINK_LLVM_DYLIB=ON".to_string(),
        "-DCLANG_LINK_CLANG_DYLIB=ON".to_string(),
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    let stamp_path = out_root.join("build-stamp.txt");
    let configuration_changed =
        fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str());
    fs::create_dir_all(&out_root)?;
    // CMake records the target sysroot in the generated compile rules.  Make
    // the declared zlib/zstd development inputs visible there before either
    // configuration or compilation; relying on a host header makes a fresh
    // rebuild differ from a cache hit and can fail when the host lacks it.
    hydrate_development_sysroot(
        repo_root,
        &[
            repo_root.join("out/build/zlib/install/usr"),
            repo_root.join("out/build/zstd/install/usr"),
        ],
    )?;
    let env = staged_library_environment(repo_root, &["zlib", "zstd"])?;
    if configuration_changed || !build_dir.join("build.ninja").is_file() {
        let option_refs = options.iter().map(String::as_str).collect::<Vec<_>>();
        run_cmd_with_env_overrides(repo_root, "cmake", &option_refs, &env)?;
    }
    normalize_llvm_config_build_roots(repo_root, &build_dir)?;
    let child_jobs = scheduler::child_job_limit().to_string();
    run_cmd_with_env_overrides(&build_dir, "ninja", &["-j", &child_jobs], &env)?;
    remove_path_if_exists(&install_dir)?;
    let destdir_env = [("DESTDIR", install_dir.display().to_string())];
    run_cmd_with_env_overrides(&build_dir, "ninja", &["install"], &destdir_env)?;
    fs::copy(
        build_dir.join("bin/FileCheck"),
        install_dir.join("usr/bin/FileCheck"),
    )?;
    let clang_config_dir = install_dir.join("etc/clang");
    fs::create_dir_all(&clang_config_dir)?;
    fs::write(
        clang_config_dir.join("clang.cfg"),
        format!("--gcc-install-dir={MATTOS_GCC_INSTALL_DIR}\n"),
    )?;
    fs::write(
        clang_config_dir.join("clang++.cfg"),
        format!(
            "--gcc-install-dir={MATTOS_GCC_INSTALL_DIR}\n-isystem/usr/include/c++/15.3.0\n-isystem/usr/include/c++/15.3.0/x86_64-pc-linux-gnu\n"
        ),
    )?;
    for required in [
        "usr/bin/clang",
        "usr/bin/clang++",
        "usr/bin/ld.lld",
        "usr/bin/llvm-config",
        "usr/bin/FileCheck",
        "etc/clang/clang.cfg",
        "etc/clang/clang++.cfg",
    ] {
        if !install_dir.join(required).is_file() {
            bail!("LLVM install did not produce {required}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

/// Replace llvm-config's output-generated development-tree identities with
/// deterministic, relocatable identities before compiling the tool.
///
/// LLVM generates these two macros from the absolute CMake source and object
/// directories. They are useful only when running llvm-config from that exact
/// build tree; an installed llvm-config derives its prefix from argv[0]. Keeping
/// checkout-specific literals in the installed ELF leaks the builder path and
/// makes otherwise identical builds differ by checkout location. The imported
/// LLVM source is never changed: only CMake's output-owned generated header is
/// normalized, and the exact expected input is checked fail-closed.
fn normalize_llvm_config_build_roots(repo_root: &Path, build_dir: &Path) -> Result<()> {
    let generated = build_dir.join("tools/llvm-config/BuildVariables.inc");
    let mut contents = fs::read_to_string(&generated)
        .with_context(|| format!("read generated {}", generated.display()))?;
    let source_line = format!(
        "#define LLVM_SRC_ROOT \"{}\"",
        repo_root.join("src/toolchain/llvm-project/llvm").display()
    );
    let object_line = format!("#define LLVM_OBJ_ROOT \"{}\"", build_dir.display());
    for (actual, normalized) in [
        (
            &source_line,
            "#define LLVM_SRC_ROOT \"/usr/src/mattos/llvm\"",
        ),
        (
            &object_line,
            "#define LLVM_OBJ_ROOT \"/usr/lib/llvm-22/build\"",
        ),
    ] {
        if contents.contains(actual) {
            contents = contents.replacen(actual, normalized, 1);
        } else if !contents.contains(normalized) {
            bail!(
                "generated {} lacks expected LLVM build-root definition: {}",
                generated.display(),
                actual
            );
        }
    }
    fs::write(&generated, contents)
        .with_context(|| format!("normalize generated {}", generated.display()))?;
    Ok(())
}

fn build_rust(repo_root: &Path) -> Result<()> {
    let out_root = repo_root.join("out/build/rust");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    // Rust bootstrap compiles native build scripts (including openssl-sys)
    // against the declared target SDK.  Hydrate those headers/libraries into
    // the same sysroot used by the compiler before Cargo starts; otherwise a
    // fresh Rust rebuild can silently probe host headers and fail midway.
    hydrate_development_sysroot(
        repo_root,
        &[
            repo_root.join("out/build/openssl/install/usr"),
            repo_root.join("out/build/zlib/install/usr"),
        ],
    )?;
    let archive = ensure_verified_release_archive(
        &out_root,
        "rustc-1.97.1-src.tar.xz",
        RUST_RELEASE_ARCHIVE_URL,
        RUST_RELEASE_ARCHIVE_SHA256,
    )?;
    if !source_copy.join("x.py").is_file() {
        stage_release_source(&archive, &source_copy)?;
    }
    isolate_standalone_cargo_manifest(&source_copy.join("src/bootstrap/Cargo.toml"))?;
    isolate_standalone_cargo_manifest(
        &source_copy.join("compiler/rustc_codegen_cranelift/Cargo.toml"),
    )?;
    isolate_standalone_cargo_manifest(&source_copy.join("compiler/rustc_codegen_gcc/Cargo.toml"))?;
    let llvm_config = repo_root.join("out/build/llvm/install/usr/bin/llvm-config");
    let llvm_filecheck = repo_root.join("out/build/llvm/install/usr/bin/FileCheck");
    let gcc = repo_root.join("out/build/gcc-toolchain/install/usr/bin/gcc");
    let gxx = repo_root.join("out/build/gcc-toolchain/install/usr/bin/g++");
    let ar = repo_root.join("out/build/binutils/install/usr/bin/ar");
    let ranlib = repo_root.join("out/build/binutils/install/usr/bin/ranlib");
    let sysroot = repo_root.join("out/sysroot");
    for required in [&llvm_config, &llvm_filecheck, &gcc, &gxx, &ar, &ranlib] {
        if !required.is_file() {
            bail!("Rust bootstrap dependency missing: {}", required.display());
        }
    }
    let wrappers = out_root.join("tool-wrappers");
    fs::create_dir_all(&wrappers)?;
    let gcc_wrapper = wrappers.join("gcc");
    let gxx_wrapper = wrappers.join("g++");
    let gcc_internal = sysroot.join(MATTOS_GCC_INSTALL_DIR.trim_start_matches('/'));
    let multiarch_lib = sysroot.join("usr/lib/x86_64-linux-gnu");
    let gcc_link_lib = repo_root.join("out/build/gcc-runtime/install/usr/lib/lib64");
    let cxx_include = sysroot.join("usr/include/c++/15.3.0");
    let cxx_target_include = cxx_include.join("x86_64-pc-linux-gnu");
    for (wrapper, compiler, language_flags) in [
        (&gcc_wrapper, &gcc, String::new()),
        (
            &gxx_wrapper,
            &gxx,
            format!(
                " -isystem{} -isystem{}",
                shell_escape(path_str(&cxx_include)?),
                shell_escape(path_str(&cxx_target_include)?),
            ),
        ),
    ] {
        fs::write(
            wrapper,
            format!(
                "#!/bin/sh\nexec {} --sysroot={} -B{} -B{} -L{}{} \"$@\"\n",
                shell_escape(path_str(compiler)?),
                shell_escape(path_str(&sysroot)?),
                shell_escape(path_str(&multiarch_lib)?),
                shell_escape(path_str(&gcc_internal)?),
                shell_escape(path_str(&gcc_link_lib)?),
                language_flags,
            ),
        )?;
        set_mode(wrapper.to_path_buf(), 0o755)?;
    }
    let child_jobs = scheduler::child_job_limit();
    let config = format!(
        "profile = \"compiler\"\nchange-id = 999999\n\n[llvm]\ndownload-ci-llvm = false\n\n[build]\nbuild = \"x86_64-unknown-linux-gnu\"\nhost = [\"x86_64-unknown-linux-gnu\"]\ntarget = [\"x86_64-unknown-linux-gnu\"]\njobs = {}\ndocs = false\nsubmodules = false\nvendor = true\nlocked-deps = true\nextended = true\ntools = [\"cargo\", \"rustdoc\"]\npython = \"python3\"\n\n[install]\nprefix = \"/usr\"\nsysconfdir = \"/etc\"\n\n[rust]\nchannel = \"stable\"\ndebug = false\ndebuginfo-level = 0\nstrip = true\n\n[target.x86_64-unknown-linux-gnu]\nllvm-config = \"{}\"\nllvm-filecheck = \"{}\"\nllvm-has-rust-patches = false\ncc = \"{}\"\ncxx = \"{}\"\nar = \"{}\"\nranlib = \"{}\"\nlinker = \"{}\"\nrustflags = [\"-C\", \"link-arg=--sysroot={}\", \"--remap-path-prefix={}=/usr/src/mattos/rust\"]\n",
        child_jobs,
        llvm_config.display(),
        llvm_filecheck.display(),
        gcc_wrapper.display(),
        gxx_wrapper.display(),
        ar.display(),
        ranlib.display(),
        gcc_wrapper.display(),
        sysroot.display(),
        repo_root.display(),
    );
    fs::write(source_copy.join("bootstrap.toml"), config)?;
    run_cmd(&source_copy, "python3", &["x.py", "build", "--stage", "2"])?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &source_copy,
        "python3",
        &["x.py", "install", "--stage", "2"],
        &[("DESTDIR", install_dir.display().to_string())],
    )?;
    for required in ["usr/bin/rustc", "usr/bin/cargo", "usr/bin/rustdoc"] {
        if !install_dir.join(required).is_file() {
            bail!("Rust install did not produce {required}");
        }
    }
    Ok(())
}
