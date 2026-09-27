// Rust compiler, standard library, rustdoc and Cargo.  Kept separate from the
// LLVM and CPython recipes so a change to one does not invalidate the
// others' stage caches.

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
            // Cargo's curl-sys links libcurl; it must be MattOS curl, never
            // the host's (which a host pkg-config search would select).
            repo_root.join("out/build/curl/install/usr"),
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
    let toolchain = require_mattos_target_toolchain(repo_root)?;
    let gcc_wrapper = toolchain.tool("gcc");
    let gxx_wrapper = toolchain.tool("g++");
    let ar = toolchain.tool("ar");
    let ranlib = toolchain.tool("ranlib");
    let sysroot = repo_root.join("out/sysroot");
    for required in [&llvm_config, &llvm_filecheck] {
        if !required.is_file() {
            bail!("Rust bootstrap dependency missing: {}", required.display());
        }
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
    // Native-library probes (pkg-config in openssl-sys, curl-sys, ...) may only
    // see the MattOS sysroot, not host `.pc` files.
    let pkg_config_env = [
        (
            "PKG_CONFIG_LIBDIR",
            format!(
                "{}:{}",
                sysroot.join("usr/lib/x86_64-linux-gnu/pkgconfig").display(),
                sysroot.join("usr/share/pkgconfig").display()
            ),
        ),
        ("PKG_CONFIG_PATH", String::new()),
        ("PKG_CONFIG_SYSROOT_DIR", sysroot.display().to_string()),
    ];
    run_cmd_with_env_overrides(
        &source_copy,
        "python3",
        &["x.py", "build", "--stage", "2"],
        &pkg_config_env,
    )?;
    remove_path_if_exists(&install_dir)?;
    let mut install_env = pkg_config_env.to_vec();
    install_env.push(("DESTDIR", install_dir.display().to_string()));
    run_cmd_with_env_overrides(
        &source_copy,
        "python3",
        &["x.py", "install", "--stage", "2"],
        &install_env,
    )?;
    for required in ["usr/bin/rustc", "usr/bin/cargo", "usr/bin/rustdoc"] {
        if !install_dir.join(required).is_file() {
            bail!("Rust install did not produce {required}");
        }
    }
    Ok(())
}
