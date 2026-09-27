// LLVM, Clang and LLD.  Kept separate from the Rust and CPython recipes so a
// change to one does not invalidate the others' stage caches.

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
        // MattOS images are x86_64-only.  AMDGPU is a userspace compiler
        // backend required by radeonsi/RADV, not a CPU target.  Add AArch64
        // or RISCV here when MattOS gains those architectures.
        "-DLLVM_TARGETS_TO_BUILD=X86;AMDGPU".to_string(),
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
        // Linking libLLVM, libclang-cpp and the tools with GNU ld takes several
        // GB each; unbounded concurrent links are the build's largest memory
        // peak.  A Ninja link pool keeps compile parallelism while capping it.
        "-DLLVM_PARALLEL_LINK_JOBS=2".to_string(),
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
