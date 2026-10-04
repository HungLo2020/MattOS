fn validate_kernel_config_policy(config: &str, policy: &KernelConfigPolicy) -> Result<()> {
    for (symbols, expected) in [
        (&policy.builtin, KernelConfigState::Builtin),
        (&policy.module, KernelConfigState::Module),
    ] {
        for symbol in symbols {
            let actual = kernel_config_state(config, symbol)
                .with_context(|| format!("kernel policy symbol {symbol} is absent"))?;
            if actual != expected {
                bail!("kernel policy requires {symbol}={expected:?}, found {actual:?}");
            }
        }
    }
    for symbol in &policy.unsupported {
        if let Some(actual @ (KernelConfigState::Builtin | KernelConfigState::Module)) =
            kernel_config_state(config, symbol)
        {
            bail!("kernel policy requires {symbol}=Unsupported, found {actual:?}");
        }
    }
    for prefix in &policy.unsupported_prefixes {
        if let Some(line) = config.lines().find(|line| {
            line.starts_with(prefix)
                && !line.starts_with("CONFIG_PATA_TIMINGS=")
                && (line.ends_with("=y") || line.ends_with("=m"))
        }) {
            bail!("kernel legacy-family policy rejects {line}");
        }
    }
    let modules = config.lines().filter(|line| line.ends_with("=m")).count();
    if modules < policy.minimum_module_symbols {
        bail!(
            "kernel generic coverage regressed to {modules} module symbols; policy requires at least {}",
            policy.minimum_module_symbols
        );
    }
    Ok(())
}

fn read_kernel_config_policy(repo_root: &Path) -> Result<KernelConfigPolicy> {
    let path = repo_root.join("src/kernel/config/x86_64_mattos.policy.toml");
    toml::from_str(&fs::read_to_string(&path)?)
        .with_context(|| format!("parse kernel configuration policy {}", path.display()))
}

fn kernel_source_worktree_identity(repo_root: &Path) -> Result<String> {
    let relative = "src/kernel/linux";
    let diff = Command::new("git")
        .args([source_identity::NO_ATTRIBUTES, "diff", "--binary", "HEAD", "--", relative])
        .current_dir(repo_root)
        .output()?;
    if !diff.status.success() {
        bail!("git could not inspect the Linux working tree");
    }
    let untracked = Command::new("git")
        .args(["ls-files", "-z", "--others"])
        .args(source_identity::mattos_exclude_arguments(repo_root)?)
        .args(["--", relative])
        .current_dir(repo_root)
        .output()?;
    if !untracked.status.success() {
        bail!("git could not inspect untracked Linux inputs");
    }
    let mut hasher = Sha256Hasher::new();
    hasher.update(fs::read(repo_root.join("upstream/state/linux.toml"))?);
    hasher.update(&diff.stdout);
    for raw in untracked
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let path = PathBuf::from(String::from_utf8_lossy(raw).into_owned());
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(fs::read(repo_root.join(path))?);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn build_kernel(repo_root: &Path) -> Result<()> {
    assert_kernel_build_path_safe(repo_root)?;
    let linux = repo_root.join("src/kernel/linux");
    let config = repo_root.join("src/kernel/config/x86_64_mattos.config");
    if !linux.join("Makefile").exists() {
        bail!(
            "kernel source not found in {}; run import first",
            linux.display()
        );
    }
    if !config.exists() {
        bail!(
            "kernel config missing at {}; add configuration first",
            config.display()
        );
    }

    let out_root = repo_root.join("out/build/linux");
    let source = out_root.join("source");
    let build = out_root.join("build");
    let source_identity = kernel_source_worktree_identity(repo_root)?;
    let source_identity_path = out_root.join("source-identity");
    let source_changed =
        fs::read_to_string(&source_identity_path).ok().as_deref() != Some(source_identity.as_str());
    // Kbuild appends an object's genksyms CRCs to its .cmd file after the
    // object is written, and nothing rechecks them: a build killed in between
    // leaves an up-to-date object whose exports get CRC 0 in every later
    // build of this tree.  A tree an unfinished build touched is discarded.
    let incomplete_marker = out_root.join("build-incomplete");
    let interrupted = incomplete_marker.exists();
    fs::create_dir_all(&out_root)?;
    if source_changed || interrupted {
        remove_path_if_exists(&source)?;
        remove_path_if_exists(&build)?;
    }
    remove_path_if_exists(&out_root.join("modules"))?;
    if !source.is_dir() {
        copy_imported_working_tree(repo_root, Path::new("src/kernel/linux"), &source)?;
        fs::write(&source_identity_path, &source_identity)?;
    }
    fs::create_dir_all(&build).with_context(|| format!("failed to create {}", build.display()))?;
    fs::write(&incomplete_marker, "")?;
    install_module_signing_key(repo_root, &build)?;

    let config_text = fs::read_to_string(&config)
        .with_context(|| format!("failed to read {}", config.display()))?;
    let policy = read_kernel_config_policy(repo_root)?;
    validate_kernel_config_policy(&config_text, &policy)?;
    fs::write(build.join(".config"), config_text)
        .with_context(|| format!("failed to stage kernel config from {}", config.display()))?;

    let env = local_tool_env(repo_root);
    if let Some(env) = &env {
        println!(
            "Using local rootless toolchain from {}",
            env.tool_root.display()
        );
    }
    let output_arg = format!("O={}", build.display());
    // Target code is compiled by the MattOS-built GCC and Binutils; HOSTCC
    // remains the host compiler for Kbuild's build-machine helpers.
    let cross_compile = format!("CROSS_COMPILE={}", kernel_cross_compile(repo_root)?);
    // The kernel does not consume SOURCE_DATE_EPOCH directly for all of its
    // generated metadata.  Pin the release banner and built-in initramfs cpio
    // mtimes explicitly; otherwise two healthy builds differ only by their
    // wall-clock build time and the GNU build ID derived from it.
    //
    // Kconfig also probes host tools ($(RUSTC), $(BINDGEN), $(PAHOLE)) and
    // records their versions in the resolved config and kheaders.ko even
    // though MattOS enables neither Rust nor BTF; a missing tool resolves to
    // 0/n on every build machine.
    let kernel_reproducible_args = [
        "KBUILD_BUILD_TIMESTAMP=2026-01-01 00:00:00 UTC",
        "KBUILD_BUILD_USER=mattos",
        "KBUILD_BUILD_HOST=mattos-build",
        "KBUILD_BUILD_VERSION=1",
        "KCONFIG_NOTIMESTAMP=1",
        "RUSTC=false",
        "BINDGEN=false",
        "PAHOLE=false",
    ];
    // Kernel modules can retain __FILE__ paths in modinfo/debug-adjacent
    // strings even when normal ELF debug sections are not packaged. Keep the
    // source path deterministic and non-host-specific at compile time; this
    // also covers compressed .ko.zst payloads that the package audit inspects.
    //
    // Kbuild itself adds `-fmacro-prefix-map=$(srcroot)/=` to
    // KBUILD_CPPFLAGS for out-of-tree builds. That mapping is intentionally
    // relative, but it is not suitable for MattOS package validation: it can
    // leave absolute include paths in __FILE__ data when the compiler is
    // invoked with an absolute source path. Supply the complete CPPFLAGS
    // value on the make command line so Kbuild does not append its empty
    // replacement map; the command-line assignment is the only generic way
    // to replace that built-in flag without patching the vendored kernel.
    let kernel_cppflags_map = format!(
        "KBUILD_CPPFLAGS=-D__KERNEL__ -fmacro-prefix-map={}/=/usr/src/mattos/linux/ -I{}/arch/x86/entry/vdso/vdso64/..",
        source.display(),
        source.display()
    );
    let kernel_cflags_map = format!(
        "KCFLAGS=-ffile-prefix-map={}=/usr/src/mattos/linux -fdebug-prefix-map={}=/usr/src/mattos/linux",
        source.display(),
        source.display()
    );
    let mut olddefconfig_args = vec![output_arg.as_str(), cross_compile.as_str(), "olddefconfig"];
    olddefconfig_args.extend(kernel_reproducible_args);
    olddefconfig_args.push(kernel_cppflags_map.as_str());
    olddefconfig_args.push(kernel_cflags_map.as_str());
    run_cmd_with_env(&source, "make", &olddefconfig_args, env.as_ref())?;
    let resolved_config = fs::read_to_string(build.join(".config"))?;
    validate_kernel_config_policy(&resolved_config, &policy)?;
    validate_module_signing_config(&resolved_config)?;
    let mut build_args = vec![output_arg.as_str(), cross_compile.as_str(), "-j", "4"];
    build_args.extend(kernel_reproducible_args);
    build_args.push(kernel_cppflags_map.as_str());
    build_args.push(kernel_cflags_map.as_str());
    run_cmd_with_env(&source, "make", &build_args, env.as_ref()).context("kernel build failed")?;
    validate_kernel_symbol_versions(&fs::read_to_string(build.join("Module.symvers"))?)?;
    remove_path_if_exists(&incomplete_marker)?;

    let bz = build.join("arch/x86/boot/bzImage");
    if !bz.exists() {
        bail!("kernel build finished without bzImage at {}", bz.display())
    }
    let modules = out_root.join("modules");
    fs::create_dir_all(&modules)?;
    let release = fs::read_to_string(build.join("include/config/kernel.release"))?
        .trim()
        .to_owned();
    let module_dir = modules.join("usr/lib/modules").join(&release);
    let modlib = format!("MODLIB={}", module_dir.display());
    let mut modules_install_args = vec![
        output_arg.as_str(),
        cross_compile.as_str(),
        "modules_install",
        modlib.as_str(),
        "DEPMOD=true",
    ];
    modules_install_args.extend(kernel_reproducible_args);
    modules_install_args.push(kernel_cppflags_map.as_str());
    modules_install_args.push(kernel_cflags_map.as_str());
    run_cmd_with_env(&source, "make", &modules_install_args, env.as_ref())?;
    for link in ["build", "source"] {
        remove_path_if_exists(&module_dir.join(link))?;
    }
    normalize_kernel_module_paths(repo_root, &modules)?;
    // After normalization, which rewrites module bytes a signature covers.
    sign_kernel_modules(repo_root, &build, &modules)?;
    run_cmd(
        repo_root,
        "depmod",
        &[
            "-b",
            path_str(&modules)?,
            "-m",
            "/usr/lib/modules",
            &release,
        ],
    )?;
    for metadata in ["modules.dep", "modules.alias", "modules.builtin"] {
        if !module_dir.join(metadata).is_file() {
            bail!("kernel modules_install/depmod did not produce {metadata}");
        }
    }
    let mut module_files = Vec::new();
    collect_regular_files(&module_dir, &mut module_files)?;
    let module_count = module_files
        .iter()
        .filter(|path| path.to_string_lossy().ends_with(".ko.zst"))
        .count();
    if module_count < 500 {
        bail!("generic kernel produced only {module_count} compressed modules");
    }
    fs::write(out_root.join("kernel-release"), format!("{release}\n"))?;
    Ok(())
}

/// With CONFIG_MODVERSIONS every export has a genksyms CRC.  modpost only
/// warns when one is missing and records 0, which makes the modules that
/// import it differ from a healthy build's.
fn validate_kernel_symbol_versions(module_symvers: &str) -> Result<()> {
    let missing = module_symvers
        .lines()
        .filter(|line| line.starts_with("0x00000000\t"))
        .filter_map(|line| line.split('\t').nth(1))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "kernel exports have no symbol version CRC (a stale object from an interrupted build?): {}",
            missing.join(", ")
        );
    }
    Ok(())
}

/// UBSAN records source locations in loadable modules using the compiler's
/// absolute source names. Prefix-map options normalize ordinary debug and
/// macro locations, but GCC deliberately keeps the original path in its UBSAN
/// metadata. Normalize that metadata after modules_install while retaining the
/// instrumentation and ELF layout. The replacement is deliberately the same
/// byte length as the checkout prefix, so offsets, relocations, and module
/// behavior are unchanged; the extra slashes are harmless in diagnostic paths.
/// The committed module-signing key: a private key and its certificate in one
/// PEM, deliberately public.  A fixed key keeps the kernel reproducible (with
/// no key supplied, Kbuild generates a random one and embeds its certificate
/// in the kernel image) and lets every module, including out-of-tree ones,
/// carry a valid signature, so loading one does not taint the kernel.  Being
/// public, a signature proves nothing about who built a module; signatures
/// are not enforced (`CONFIG_MODULE_SIG_FORCE` is off).
const MODULE_SIGNING_KEY: &str = "src/kernel/config/module-signing-key.pem";
/// Where the key is staged in the kernel build tree, as the config names it.
/// Not Kbuild's default `certs/signing_key.pem`, which Kbuild would replace
/// with a generated key.
const KERNEL_MODULE_SIGNING_KEY: &str = "certs/mattos-module-signing-key.pem";
const MODULE_SIGNATURE_MAGIC: &[u8] = b"~Module signature appended~\n";

fn install_module_signing_key(repo_root: &Path, kernel_build: &Path) -> Result<()> {
    let key = fs::read(repo_root.join(MODULE_SIGNING_KEY))
        .with_context(|| format!("read the module signing key {MODULE_SIGNING_KEY}"))?;
    let staged = kernel_build.join(KERNEL_MODULE_SIGNING_KEY);
    // Rewriting an unchanged key would make Kbuild relink the kernel.
    if fs::read(&staged).ok().as_deref() != Some(key.as_slice()) {
        fs::create_dir_all(staged.parent().expect("the key path has a parent"))?;
        fs::write(&staged, key)?;
    }
    Ok(())
}

fn validate_module_signing_config(config: &str) -> Result<()> {
    let key = format!("CONFIG_MODULE_SIG_KEY=\"{KERNEL_MODULE_SIGNING_KEY}\"");
    for required in ["CONFIG_MODULE_SIG=y", key.as_str()] {
        if !config.lines().any(|line| line == required) {
            bail!("kernel configuration must set {required} to sign modules with {MODULE_SIGNING_KEY}");
        }
    }
    if config.lines().any(|line| line == "CONFIG_MODULE_SIG_ALL=y") {
        bail!("CONFIG_MODULE_SIG_ALL must stay off: modules are signed after path normalization");
    }
    Ok(())
}

/// Appends a signature, made with the kernel build's `sign-file` and the
/// committed key, to every unsigned `.ko` or `.ko.zst` module under `root`.
/// Compressed modules are decompressed, signed and recompressed as Kbuild
/// compresses them.
fn sign_kernel_modules(repo_root: &Path, kernel_build: &Path, root: &Path) -> Result<()> {
    let sign_file = kernel_build.join("scripts/sign-file");
    let key = kernel_build.join(KERNEL_MODULE_SIGNING_KEY);
    let certificate = kernel_build.join("certs/signing_key.x509");
    for required in [&sign_file, &key, &certificate] {
        if !required.is_file() {
            bail!("module signing needs {}; build the kernel first", required.display());
        }
    }
    let scratch = repo_root.join("out/tmp");
    fs::create_dir_all(&scratch)?;
    let raw = scratch.join(format!(".mattos-module-sign-{}.ko", std::process::id()));
    let mut files = Vec::new();
    collect_regular_files(root, &mut files)?;
    let mut signed = 0usize;
    for path in files {
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
        let compressed = name.ends_with(".ko.zst");
        if !compressed && !name.ends_with(".ko") {
            continue;
        }
        let payload = if compressed {
            let output = Command::new("zstd")
                .args(["--quiet", "--decompress", "--stdout"])
                .arg(&path)
                .output()
                .with_context(|| format!("decompress kernel module {}", path.display()))?;
            if !output.status.success() {
                bail!("could not decompress kernel module {}", path.display());
            }
            output.stdout
        } else {
            fs::read(&path)?
        };
        if payload.ends_with(MODULE_SIGNATURE_MAGIC) {
            continue;
        }
        fs::write(&raw, &payload)?;
        run_cmd(
            repo_root,
            path_str(&sign_file)?,
            &["sha512", path_str(&key)?, path_str(&certificate)?, path_str(&raw)?],
        )?;
        if compressed {
            let output = Command::new("zstd")
                .args(["--quiet", "--compress", "--stdout"])
                .arg(&raw)
                .output()
                .with_context(|| format!("recompress kernel module {}", path.display()))?;
            if !output.status.success() {
                bail!("could not recompress kernel module {}", path.display());
            }
            fs::write(&path, output.stdout)?;
        } else {
            fs::copy(&raw, &path)?;
        }
        signed += 1;
    }
    remove_path_if_exists(&raw)?;
    println!("signed {signed} kernel module(s) under {}", root.display());
    Ok(())
}

fn normalize_kernel_module_paths(repo_root: &Path, modules: &Path) -> Result<()> {
    let checkout_prefix = repo_root.to_string_lossy().into_owned();
    let stable_prefix = "/usr/src/mattos";
    if stable_prefix.len() > checkout_prefix.len() {
        bail!(
            "stable kernel source prefix is longer than checkout prefix: {} > {}",
            stable_prefix.len(),
            checkout_prefix.len()
        );
    }
    let replacement = {
        let mut value = stable_prefix.as_bytes().to_vec();
        value.resize(checkout_prefix.len(), b'/');
        value
    };
    let old = checkout_prefix.as_bytes();
    let mut files = Vec::new();
    collect_regular_files(modules, &mut files)?;
    let mut normalized = 0usize;
    for (index, path) in files.into_iter().enumerate() {
        let extension = path.extension().and_then(OsStr::to_str);
        if !matches!(extension, Some("ko") | Some("zst")) {
            continue;
        }
        let mut payload = if extension == Some("zst") {
            let output = Command::new("zstd")
                .args(["--quiet", "--decompress", "--stdout"])
                .arg(&path)
                .output()
                .with_context(|| format!("decompress kernel module {}", path.display()))?;
            if !output.status.success() {
                bail!("could not decompress kernel module {}", path.display());
            }
            output.stdout
        } else {
            fs::read(&path)?
        };
        if !replace_fixed_length_prefixes(&mut payload, old, &replacement) {
            continue;
        }
        if extension == Some("zst") {
            let raw_path = repo_root
                .join("out/tmp")
                .join(format!(".mattos-kernel-normalize-{}-{index}.raw", std::process::id()));
            fs::create_dir_all(raw_path.parent().expect("temporary file has a parent"))?;
            fs::write(&raw_path, &payload)?;
            let child = Command::new("zstd")
                .args(["--quiet", "--compress", "--stdout"])
                .stdout(Stdio::piped())
                .arg(&raw_path)
                .spawn()
                .with_context(|| format!("recompress kernel module {}", path.display()))?;
            let output = child.wait_with_output()?;
            remove_path_if_exists(&raw_path)?;
            if !output.status.success() {
                bail!("could not recompress kernel module {}", path.display());
            }
            fs::write(&path, output.stdout)?;
        } else {
            fs::write(&path, payload)?;
        }
        normalized += 1;
    }
    if normalized > 0 {
        println!("normalized host source paths in {normalized} kernel module payload(s)");
    }
    Ok(())
}

fn replace_fixed_length_prefixes(bytes: &mut [u8], old: &[u8], replacement: &[u8]) -> bool {
    if old.is_empty() || old.len() != replacement.len() || bytes.len() < old.len() {
        return false;
    }
    let mut changed = false;
    for offset in 0..=bytes.len() - old.len() {
        if &bytes[offset..offset + old.len()] == old {
            bytes[offset..offset + old.len()].copy_from_slice(replacement);
            changed = true;
        }
    }
    changed
}

const GLIBC_MINIMUM_KERNEL: &str = "5.10.0";
const MATTOS_SOURCE_DATE_EPOCH: &str = "1767225600";

fn build_glibc(repo_root: &Path) -> Result<()> {
    // Userland kernel headers come from the separately pinned `linux-uapi`
    // import, never from the kernel MattOS boots (see `linux_x86_uapi_inputs`).
    let linux = repo_root.join("src/kernel/linux-uapi");
    let source = repo_root.join("src/system/libc/glibc");
    let output = repo_root.join("out/build/glibc");
    let build = output.join("build");
    let install = output.join("install");
    let sysroot = repo_root.join("out/sysroot");
    // The kernel UAPI headers are exported into glibc's own output, and glibc
    // is configured against exactly that tree: the shared sysroot also holds
    // other stages' headers (libstdc++, hydrated development files).
    let headers_root = output.join("linux-headers/usr");
    if !linux.join("Makefile").is_file() {
        bail!(
            "Linux UAPI source not found at {}; run `mattos-build upstream import linux-uapi`",
            linux.display()
        )
    }
    if !source.join("configure").is_file() {
        bail!(
            "glibc source not found at {}; run `mattos-build upstream import glibc`",
            source.display()
        )
    }

    if !cross_toolchain_bin(repo_root)
        .join(format!("{TOOLCHAIN_TARGET}-gcc"))
        .is_file()
    {
        bail!("glibc requires the MattOS pass-1 cross compiler; build cross-toolchain first")
    }
    // The sysroot is shared: gcc-runtime installs its runtime libraries and
    // C++ headers there, and later stages hydrate development files into it.
    // Replace only glibc's own previous contribution, never the whole
    // directory, which would destroy other stages' published outputs and
    // force them to rebuild even when glibc's output is unchanged.
    remove_previous_sysroot_contribution(&sysroot, &[install.clone(), output.join("linux-headers")])?;
    remove_path_if_exists(&output)?;
    fs::create_dir_all(&build)?;
    fs::create_dir_all(&install)?;
    fs::create_dir_all(&headers_root)?;
    // glibc is compiled by the source-built pass-1 GCC.  It is configured as
    // a cross build because pass 1 has no C library to link test programs
    // against; BUILD_CC compiles only build-time helpers that never ship.
    let (pass1_cc, pass1_cxx) = write_pass1_compiler_wrappers(repo_root)?;
    let mut cross_path = vec![cross_toolchain_bin(repo_root)];
    if let Some(host_path) = std::env::var_os("PATH") {
        cross_path.extend(std::env::split_paths(&host_path));
    }
    let cross_path = std::env::join_paths(cross_path)?
        .to_string_lossy()
        .into_owned();

    let linux_source = output.join("linux-source");
    let linux_build = output.join("linux-build");
    copy_imported_working_tree(repo_root, Path::new("src/kernel/linux-uapi"), &linux_source)?;
    fs::create_dir_all(&linux_build)?;
    let output_arg = format!("O={}", linux_build.display());
    let headers_arg = format!("INSTALL_HDR_PATH={}", headers_root.display());
    run_cmd(
        &linux_source,
        "make",
        &[
            output_arg.as_str(),
            "ARCH=x86",
            "headers_install",
            headers_arg.as_str(),
        ],
    )
    .context("Linux UAPI header generation failed")?;
    if !headers_root.join("include/linux/version.h").is_file()
        || !headers_root.join("include/asm/unistd.h").is_file()
    {
        bail!("Linux headers_install did not create the required UAPI header tree")
    }
    copy_tree_contents(&headers_root, &sysroot.join("usr"))?;
    let mut uapi_files = Vec::new();
    collect_regular_files(&output.join("linux-headers/usr/include"), &mut uapi_files)?;
    // The Linux commit the UAPI headers are exported from: the imported pin,
    // never a copy of it that could drift when the kernel is re-imported.
    let linux_revision = read_sync_state(repo_root, "linux-uapi")?
        .context("upstream/state/linux-uapi.toml is missing; import the Linux UAPI source first")?
        .imported_commit;
    let mut uapi_inventory = format!(
        "revision={linux_revision}\narchitecture=x86\ncommand=make ARCH=x86 headers_install\n\n"
    );
    for path in uapi_files {
        uapi_inventory.push_str(
            path.strip_prefix(output.join("linux-headers"))?
                .to_string_lossy()
                .as_ref(),
        );
        uapi_inventory.push('\n');
    }
    fs::write(output.join("linux-headers-inventory.txt"), uapi_inventory)?;

    let configure = source.join("configure");
    let headers = headers_root.join("include");
    let glibc_cflags = format!(
        "-O2 -g0 -ffile-prefix-map={}=/usr/src/mattos/glibc -fdebug-prefix-map={}=/usr/src/mattos/glibc",
        repo_root.display(),
        repo_root.display()
    );
    let configure_text = format!(
        "CC='{}' CXX='{}' BUILD_CC=gcc CFLAGS='{}' {} \\\n+  --prefix=/usr \\\n+  --libdir=/usr/lib/x86_64-linux-gnu \\\n+  --libexecdir=/usr/libexec \\\n+  --build=x86_64-build-linux-gnu \\\n+  --host=x86_64-pc-linux-gnu \\\n+  --enable-kernel={} \\\n+  --with-headers={} \\\n+  --without-selinux \\\n+  --disable-werror \\\n+  --disable-profile \\\n+  --disable-build-nscd \\\n+  --disable-nscd \\\n+  --enable-stack-protector=strong \\\n+  --enable-bind-now\n",
        pass1_cc.display(),
        pass1_cxx.display(),
        glibc_cflags,
        configure.display(),
        GLIBC_MINIMUM_KERNEL,
        headers.display()
    );
    fs::write(output.join("configure-invocation.txt"), &configure_text)?;
    fs::write(
        output.join("kernel-headers-source.txt"),
        format!("source=src/kernel/linux-uapi\nrevision={linux_revision}\nmethod=make ARCH=x86 headers_install\n"),
    )?;

    let configure_program = configure
        .to_str()
        .ok_or_else(|| anyhow!("glibc configure path is not UTF-8"))?;
    let headers_option = format!("--with-headers={}", headers.display());
    let kernel_option = format!("--enable-kernel={GLIBC_MINIMUM_KERNEL}");
    let configure_args = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--libexecdir=/usr/libexec",
        "--build=x86_64-build-linux-gnu",
        "--host=x86_64-pc-linux-gnu",
        kernel_option.as_str(),
        headers_option.as_str(),
        "--without-selinux",
        "--disable-werror",
        "--disable-profile",
        "--disable-build-nscd",
        "--disable-nscd",
        "--enable-stack-protector=strong",
        "--enable-bind-now",
    ];
    let configure_env = [
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("LC_ALL", "C".to_string()),
        ("TZ", "UTC".to_string()),
        ("CFLAGS", glibc_cflags),
        ("CC", path_str(&pass1_cc)?.to_string()),
        ("CXX", path_str(&pass1_cxx)?.to_string()),
        ("BUILD_CC", "gcc".to_string()),
        ("PATH", cross_path.clone()),
        ("libc_cv_slibdir", "/usr/lib/x86_64-linux-gnu".to_string()),
        ("libc_cv_rtlddir", "/lib64".to_string()),
    ];
    run_cmd_with_env_overrides(&build, configure_program, &configure_args, &configure_env)
        .context("glibc configure failed")?;

    let config_make = fs::read_to_string(build.join("config.make"))?;
    if !config_make.contains(&format!("sysheaders = {}", headers.display())) {
        bail!("glibc config.make does not select the controlled MattOS UAPI headers")
    }
    let selected_cc = format!("CC = {}", pass1_cc.display());
    if !config_make.lines().any(|line| line.trim() == selected_cc) {
        bail!("glibc config.make does not select the MattOS pass-1 compiler")
    }
    run_cmd_with_env_overrides(
        &build,
        "make",
        &["-j", "4"],
        &[
            ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
            ("LC_ALL", "C".to_string()),
            ("TZ", "UTC".to_string()),
            ("PATH", cross_path.clone()),
        ],
    )
    .context("glibc build failed")?;
    let install_root = format!("install_root={}", install.display());
    run_cmd_with_env_overrides(
        &build,
        "make",
        &["install", install_root.as_str()],
        &[
            ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
            ("LC_ALL", "C".to_string()),
            ("TZ", "UTC".to_string()),
            ("PATH", cross_path),
        ],
    )
    .context("glibc install failed")?;

    for relative in [
        "lib64/ld-linux-x86-64.so.2",
        "usr/lib/x86_64-linux-gnu/libc.so.6",
        "usr/lib/x86_64-linux-gnu/libm.so.6",
        "usr/lib/x86_64-linux-gnu/libnss_files.so.2",
        "usr/lib/x86_64-linux-gnu/libnss_dns.so.2",
        "usr/lib/x86_64-linux-gnu/libresolv.so.2",
        "usr/bin/getent",
    ] {
        if !install.join(relative).is_file() {
            bail!("glibc install is missing required artifact /{relative}")
        }
    }
    copy_tree_contents(&install, &sysroot)?;
    println!(
        "glibc runtime and development sysroot installed in {}",
        sysroot.display()
    );
    Ok(())
}

/// Removes from the shared `sysroot` the files a stage installed there last
/// time, as recorded by its previous output trees (each laid out relative to
/// the sysroot).  A sysroot entry is removed only while it is still that
/// stage's copy: another stage's replacement at the same path is kept.
fn remove_previous_sysroot_contribution(sysroot: &Path, previous_trees: &[PathBuf]) -> Result<()> {
    fn walk(tree: &Path, relative: &Path, sysroot: &Path) -> Result<()> {
        let Ok(entries) = fs::read_dir(tree.join(relative)) else {
            return Ok(());
        };
        for entry in entries {
            let child = relative.join(entry?.file_name());
            let ours = tree.join(&child);
            let installed = sysroot.join(&child);
            let (Ok(ours_meta), Ok(installed_meta)) =
                (fs::symlink_metadata(&ours), fs::symlink_metadata(&installed))
            else {
                continue;
            };
            if ours_meta.is_dir() && !ours_meta.file_type().is_symlink() {
                walk(tree, &child, sysroot)?;
            } else if ours_meta.file_type().is_symlink() {
                if installed_meta.file_type().is_symlink() && fs::read_link(&ours)? == fs::read_link(&installed)? {
                    fs::remove_file(&installed)?;
                }
            } else if installed_meta.is_file()
                && installed_meta.len() == ours_meta.len()
                && fs::read(&ours)? == fs::read(&installed)?
            {
                fs::remove_file(&installed)?;
            }
        }
        Ok(())
    }
    for tree in previous_trees {
        walk(tree, Path::new(""), sysroot)?;
    }
    Ok(())
}

const GCC_RUNTIME_TARGET: &str = "x86_64-pc-linux-gnu";
const GCC_RUNTIME_LIBSTDCXX_ABI: &str = "libstdc++.so.6.0.34";
const GCC_RUNTIME_REPRESENTATIVE_CONSUMERS: &[&str] = &[
    "usr/bin/apt",
    "usr/bin/apt-get",
    "usr/bin/dpkg",
    "usr/bin/curl",
    "usr/lib/systemd/systemd",
    "usr/bin/dbus-broker",
    "usr/bin/brush",
    "usr/bin/sudo",
    "usr/bin/login",
    "usr/libexec/mattos/rescue-init",
];

fn run_gcc_bootstrap_command(
    cwd: &Path,
    program: &Path,
    args: &[&str],
    env: &[(&str, String)],
) -> Result<()> {
    let mut command = Command::new(program);
    let scheduler_args = scheduler_command_args(args);
    command.current_dir(cwd).args(&scheduler_args);
    apply_reproducible_process_environment(&mut command);
    for (key, value) in env {
        command.env(key, value);
    }
    apply_scheduler_parallelism(&mut command);
    let display = effective_command_display(&program.display().to_string(), &scheduler_args);
    let status = performance::run_logged_command(&mut command, &display)?;
    if !status.success() {
        bail!(
            "GCC bootstrap command failed with {status}: {} {}",
            program.display(),
            args.join(" ")
        )
    }
    Ok(())
}

fn find_unique_file_named(root: &Path, name: &str) -> Result<PathBuf> {
    let mut files = Vec::new();
    collect_regular_files(root, &mut files)?;
    let matches = files
        .into_iter()
        .filter(|path| path.file_name().and_then(OsStr::to_str) == Some(name))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        bail!(
            "expected exactly one {name} below {}, found {}",
            root.display(),
            matches.len()
        )
    }
    Ok(matches.into_iter().next().unwrap())
}

fn elf_version_names(path: &Path, prefixes: &[&str]) -> Result<BTreeSet<String>> {
    let output = Command::new("readelf")
        .args(["--version-info"])
        .arg(path)
        .output()
        .with_context(|| format!("failed to inspect symbol versions in {}", path.display()))?;
    if !output.status.success() {
        bail!(
            "readelf cannot inspect symbol versions in {}",
            path.display()
        )
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut versions = BTreeSet::new();
    for word in text.split_whitespace() {
        for prefix in prefixes {
            if let Some(start) = word.find(prefix) {
                versions.insert(
                    word[start..]
                        .trim_matches(|ch: char| {
                            !ch.is_ascii_alphanumeric() && ch != '_' && ch != '.'
                        })
                        .to_string(),
                );
            }
        }
    }
    Ok(versions)
}

fn elf_needed_names(path: &Path) -> Result<BTreeSet<String>> {
    let output = Command::new("readelf")
        .args(["-d"])
        .arg(path)
        .output()
        .with_context(|| {
            format!(
                "failed to inspect dynamic dependencies in {}",
                path.display()
            )
        })?;
    if !output.status.success() {
        bail!(
            "readelf cannot inspect dynamic dependencies in {}",
            path.display()
        )
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("(NEEDED)"))
        .filter_map(|line| {
            line.split('[')
                .nth(1)
                .and_then(|part| part.split(']').next())
                .map(str::to_string)
        })
        .collect())
}

fn validate_gcc_runtime_consumers(repo_root: &Path, sysroot: &Path, runtime: &Path) -> Result<()> {
    let existing_rootfs = repo_root.join("out/build/rootfs");
    if !GCC_RUNTIME_REPRESENTATIVE_CONSUMERS
        .iter()
        .all(|relative| existing_rootfs.join(relative).is_file())
    {
        println!(
            "previous rootfs is unavailable; representative GCC runtime loader checks are deferred to final rootfs validation"
        );
        return Ok(());
    }
    let loader = sysroot.join("lib64/ld-linux-x86-64.so.2");
    let library_path = std::env::join_paths([
        runtime.to_path_buf(),
        existing_rootfs.join("usr/lib/x86_64-linux-gnu"),
        existing_rootfs.join("usr/lib/x86_64-linux-gnu/systemd"),
        existing_rootfs.join("usr/lib"),
    ])?;
    for relative in GCC_RUNTIME_REPRESENTATIVE_CONSUMERS {
        let program = existing_rootfs.join(relative);
        let listed = Command::new(&loader)
            .arg("--library-path")
            .arg(&library_path)
            .arg("--list")
            .arg(&program)
            .output()
            .with_context(|| format!("failed isolated loader validation for /{relative}"))?;
        let output = format!(
            "{}{}",
            String::from_utf8_lossy(&listed.stdout),
            String::from_utf8_lossy(&listed.stderr)
        );
        if !listed.status.success() || output.contains("not found") {
            bail!("isolated GCC runtime loader validation failed for /{relative}: {output}")
        }
        if output.lines().any(|line| {
            line.split("=>")
                .nth(1)
                .and_then(|part| part.split_whitespace().next())
                .is_some_and(|resolved| {
                    resolved.starts_with('/')
                        && !Path::new(resolved).starts_with(runtime)
                        && !Path::new(resolved).starts_with(&existing_rootfs)
                        && !Path::new(resolved).starts_with(sysroot)
                })
        }) {
            bail!(
                "isolated GCC runtime loader validation used a host library for /{relative}: {output}"
            )
        }
    }
    let rescue = existing_rootfs.join("usr/libexec/mattos/rescue-init");
    if !elf_needed_names(&rescue)?.contains("libgcc_s.so.1") {
        bail!("Rust rescue-init no longer preserves its libgcc_s unwind dependency")
    }
    println!(
        "validated {} representative consumers against the MattOS GCC runtime before rootfs replacement",
        GCC_RUNTIME_REPRESENTATIVE_CONSUMERS.len()
    );
    Ok(())
}

fn build_gcc_runtime(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/toolchain/gcc");
    let output = repo_root.join("out/build/gcc-runtime");
    let build = output.join("build");
    let raw_install = output.join("install");
    let runtime = output.join("runtime/usr/lib/x86_64-linux-gnu");
    let sysroot = repo_root.join("out/sysroot");
    if !source.join("configure").is_file() {
        bail!(
            "GCC source not found at {}; run `mattos-build upstream import gcc`",
            source.display()
        )
    }
    if !sysroot.join("usr/lib/x86_64-linux-gnu/libc.so.6").is_file()
        || !sysroot.join("usr/include/stdio.h").is_file()
    {
        bail!("GCC runtime build requires the completed MattOS glibc sysroot")
    }
    let cross_as = cross_binutil(repo_root, "as");
    let cross_ld = cross_binutil(repo_root, "ld");
    let prereq_install = repo_root
        .join(CROSS_TOOLCHAIN_OUTPUT)
        .join("prerequisite-install");
    if !cross_as.is_file() || !cross_ld.is_file() || !prereq_install.join("lib/libgmp.a").is_file() {
        bail!("GCC runtime build requires the MattOS cross-toolchain stage")
    }
    let toolchain_install = repo_root.join(TARGET_COMPILER_INSTALL);

    remove_path_if_exists(&output)?;
    fs::create_dir_all(&build)?;
    fs::create_dir_all(&raw_install)?;
    fs::create_dir_all(&runtime)?;

    let configure = source.join("configure");
    let sysroot_option = format!("--with-sysroot={}", sysroot.display());
    let build_sysroot_option = format!("--with-build-sysroot={}", sysroot.display());
    let build_triplet = format!("--build={GCC_RUNTIME_TARGET}");
    let host_triplet = format!("--host={GCC_RUNTIME_TARGET}");
    let target_triplet = format!("--target={GCC_RUNTIME_TARGET}");
    let with_as = format!("--with-as={}", cross_as.display());
    let with_ld = format!("--with-ld={}", cross_ld.display());
    let with_gmp = format!("--with-gmp={}", prereq_install.display());
    let with_mpfr = format!("--with-mpfr={}", prereq_install.display());
    let with_mpc = format!("--with-mpc={}", prereq_install.display());
    // One build yields both the shipped runtime libraries and the complete
    // host-running MattOS compiler that later stages use (installed to
    // `TARGET_COMPILER_INSTALL`).  The compiler proper is built by the host
    // compiler; libgcc, libstdc++, and libgomp are built by that new GCC.
    let mut configure_args = vec![
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--libexecdir=/usr/libexec",
        "--with-toolexeclibdir=/usr/lib/x86_64-linux-gnu",
        build_triplet.as_str(),
        host_triplet.as_str(),
        target_triplet.as_str(),
        sysroot_option.as_str(),
        build_sysroot_option.as_str(),
        with_as.as_str(),
        with_ld.as_str(),
        with_gmp.as_str(),
        with_mpfr.as_str(),
        with_mpc.as_str(),
        "--enable-languages=c,c++",
        "--enable-lto",
        "--disable-bootstrap",
        "--disable-checking",
        "--disable-analyzer",
        "--enable-shared",
        "--enable-threads=posix",
        "--disable-libsanitizer",
        "--disable-libssp",
        "--disable-libquadmath",
        "--disable-libatomic",
        "--disable-libvtv",
        "--disable-libcc1",
        "--disable-plugin",
        "--disable-libstdcxx-pch",
    ];
    configure_args.extend_from_slice(MATTOS_GCC_DEFAULTS);
    let prefix_map = format!(
        "-O2 -g0 -ffile-prefix-map={}=/usr/src/mattos/gcc -fdebug-prefix-map={}=/usr/src/mattos/gcc",
        repo_root.display(),
        repo_root.display()
    );
    let mut path = vec![cross_toolchain_bin(repo_root)];
    if let Some(host_path) = std::env::var_os("PATH") {
        path.extend(std::env::split_paths(&host_path));
    }
    let env = [
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("LC_ALL", "C".to_string()),
        ("TZ", "UTC".to_string()),
        ("CFLAGS_FOR_TARGET", prefix_map.clone()),
        ("CXXFLAGS_FOR_TARGET", prefix_map),
        ("LDFLAGS_FOR_TARGET", "-Wl,-z,relro -Wl,-z,now".to_string()),
        ("PATH", std::env::join_paths(path)?.to_string_lossy().into_owned()),
        ("CC", "gcc".to_string()),
        ("CXX", "g++".to_string()),
        ("CFLAGS", "-O2 -g0".to_string()),
        ("CXXFLAGS", "-O2 -g0".to_string()),
    ];
    let runtime_targets = [
        "install-target-libgcc",
        "install-target-libstdc++-v3",
        "install-target-libgomp",
    ];
    fs::write(
        output.join("configure-invocation.txt"),
        format!(
            "SOURCE_DATE_EPOCH={} LC_ALL=C TZ=UTC CC=gcc CXX=g++ CFLAGS_FOR_TARGET='{}' CXXFLAGS_FOR_TARGET='{}' LDFLAGS_FOR_TARGET='-Wl,-z,relro -Wl,-z,now' {} {}\nmake all-gcc all-lto-plugin all-target-libgcc all-target-libstdc++-v3 all-target-libgomp\nmake DESTDIR={} {}\nmake DESTDIR={} install-gcc install-lto-plugin {}\n",
            MATTOS_SOURCE_DATE_EPOCH,
            env[3].1,
            env[4].1,
            configure.display(),
            configure_args.join(" "),
            raw_install.display(),
            runtime_targets.join(" "),
            toolchain_install.display(),
            runtime_targets.join(" "),
        ),
    )?;
    run_gcc_bootstrap_command(&build, &configure, &configure_args, &env)
        .context("GCC runtime configure failed")?;
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &[
            "all-gcc",
            "all-lto-plugin",
            "all-target-libgcc",
            "all-target-libstdc++-v3",
            "all-target-libgomp",
        ],
        &env,
    )
    .context("GCC runtime build failed")?;
    let destdir = format!("DESTDIR={}", raw_install.display());
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &[
            destdir.as_str(),
            "install-target-libgcc",
            "install-target-libstdc++-v3",
            "install-target-libgomp",
        ],
        &env,
    )
    .context("GCC runtime install failed")?;
    remove_path_if_exists(&toolchain_install)?;
    let toolchain_destdir = format!("DESTDIR={}", toolchain_install.display());
    let mut toolchain_targets = vec![toolchain_destdir.as_str(), "install-gcc", "install-lto-plugin"];
    toolchain_targets.extend(runtime_targets);
    run_gcc_bootstrap_command(&build, Path::new("make"), &toolchain_targets, &env)
        .context("MattOS target compiler install failed")?;

    let libgcc = find_unique_file_named(&raw_install, "libgcc_s.so.1")?;
    let libstdcxx = find_unique_file_named(&raw_install, GCC_RUNTIME_LIBSTDCXX_ABI)?;
    // The install tree also contains the SONAME symlink; select the real
    // versioned ELF so the staging layer can own the payload deterministically.
    let libgomp = find_unique_file_named(&raw_install, "libgomp.so.1.0.0")?;
    fs::copy(&libgcc, runtime.join("libgcc_s.so.1"))?;
    fs::copy(&libstdcxx, runtime.join(GCC_RUNTIME_LIBSTDCXX_ABI))?;
    fs::copy(&libgomp, runtime.join("libgomp.so.1"))?;
    std::os::unix::fs::symlink(GCC_RUNTIME_LIBSTDCXX_ABI, runtime.join("libstdc++.so.6"))?;

    let libgcc_needed = elf_needed_names(&runtime.join("libgcc_s.so.1"))?;
    let libstdcxx_needed = elf_needed_names(&runtime.join(GCC_RUNTIME_LIBSTDCXX_ABI))?;
    if !libgcc_needed.is_subset(&BTreeSet::from([
        "libc.so.6".to_string(),
        "ld-linux-x86-64.so.2".to_string(),
    ])) {
        bail!("MattOS libgcc_s has unexpected runtime dependencies: {libgcc_needed:?}")
    }
    if !libstdcxx_needed.is_subset(&BTreeSet::from([
        "libc.so.6".to_string(),
        "libm.so.6".to_string(),
        "libgcc_s.so.1".to_string(),
        "ld-linux-x86-64.so.2".to_string(),
    ])) {
        bail!("MattOS libstdc++ has unexpected runtime dependencies: {libstdcxx_needed:?}")
    }

    let gcc_versions = elf_version_names(&runtime.join("libgcc_s.so.1"), &["GCC_"])?;
    let cxx_versions = elf_version_names(
        &runtime.join(GCC_RUNTIME_LIBSTDCXX_ABI),
        &["GLIBCXX_", "CXXABI_"],
    )?;
    for required in ["GCC_3.0", "GCC_4.2.0", "GCC_14.0.0"] {
        if !gcc_versions.contains(required) {
            bail!("MattOS libgcc_s is missing required ABI node {required}")
        }
    }
    for required in ["GLIBCXX_3.4.34", "CXXABI_1.3.15"] {
        if !cxx_versions.contains(required) {
            bail!("MattOS libstdc++ is missing required ABI node {required}")
        }
    }
    fs::write(
        output.join("runtime-abi.tsv"),
        format!(
            "library\tversion_nodes\nlibgcc_s.so.1\t{}\nlibstdc++.so.6\t{}\n",
            gcc_versions.into_iter().collect::<Vec<_>>().join(","),
            cxx_versions.into_iter().collect::<Vec<_>>().join(",")
        ),
    )?;

    copy_tree_contents(&output.join("runtime"), &sysroot)?;

    let raw_usr = raw_install.join("usr");
    copy_tree_contents(
        &raw_usr.join("include/c++"),
        &sysroot.join("usr/include/c++"),
    )?;
    copy_tree_contents(
        &raw_usr.join("lib/x86_64-linux-gnu/gcc"),
        &sysroot.join("usr/lib/x86_64-linux-gnu/gcc"),
    )?;
    let raw_cxx_libdir = raw_usr.join("lib/lib64");
    let target_libdir = sysroot.join("usr/lib/x86_64-linux-gnu");
    for name in ["libstdc++.a", "libsupc++.a"] {
        fs::copy(raw_cxx_libdir.join(name), target_libdir.join(name))?;
    }
    remove_path_if_exists(&target_libdir.join("libstdc++.so"))?;
    std::os::unix::fs::symlink("libstdc++.so.6", target_libdir.join("libstdc++.so"))?;
    fs::write(
        output.join("development-files.txt"),
        "usr/include/c++/15.3.0\nusr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0\nusr/lib/x86_64-linux-gnu/libstdc++.so\nusr/lib/x86_64-linux-gnu/libstdc++.a\nusr/lib/x86_64-linux-gnu/libsupc++.a\n",
    )?;

    let validation_source = output.join("cxx-unwind-validation.cc");
    let validation_binary = output.join("cxx-unwind-validation");
    fs::write(
        &validation_source,
        "#include <iostream>\n#include <stdexcept>\n#include <string>\nint main() { try { throw std::runtime_error(std::string(\"mattos\")); } catch (const std::exception &e) { std::cout << \"caught:\" << e.what() << '\\n'; return 0; } return 1; }\n",
    )?;
    let toolchain = require_mattos_target_toolchain(repo_root)?;
    let library_flag = format!("-L{}", runtime.display());
    let rpath_link = format!("-Wl,-rpath-link,{}", runtime.display());
    let validation_source_arg = path_str(&validation_source)?;
    let validation_binary_arg = path_str(&validation_binary)?;
    run_gcc_bootstrap_command(
        repo_root,
        &toolchain.tool("g++"),
        &[
            library_flag.as_str(),
            rpath_link.as_str(),
            "-Wl,--dynamic-linker=/lib64/ld-linux-x86-64.so.2",
            validation_source_arg,
            "-o",
            validation_binary_arg,
        ],
        &env,
    )?;
    let loader = sysroot.join("lib64/ld-linux-x86-64.so.2");
    let library_path =
        std::env::join_paths([runtime.clone(), sysroot.join("usr/lib/x86_64-linux-gnu")])?;
    let validation = Command::new(&loader)
        .arg("--library-path")
        .arg(&library_path)
        .arg(&validation_binary)
        .output()?;
    if !validation.status.success()
        || String::from_utf8_lossy(&validation.stdout).trim() != "caught:mattos"
    {
        bail!(
            "MattOS GCC runtime C++ exception validation failed: {}{}",
            String::from_utf8_lossy(&validation.stdout),
            String::from_utf8_lossy(&validation.stderr)
        )
    }
    validate_gcc_runtime_consumers(repo_root, &sysroot, &runtime)?;
    println!(
        "GCC runtime-only build installed libgcc_s.so.1 and {} into {}",
        GCC_RUNTIME_LIBSTDCXX_ABI,
        runtime.display()
    );
    Ok(())
}

const TOOLCHAIN_BUILD: &str = "x86_64-build-linux-gnu";
const TOOLCHAIN_TARGET: &str = "x86_64-pc-linux-gnu";
const GCC_TOOLCHAIN_VERSION: &str = "15.3.0";
const BINUTILS_UPSTREAM_COMMIT: &str = "5e56594815854de5eca35c7c04b11705d0f19c02";
const BINUTILS_UPSTREAM_MIRROR: &str = "https://git.sr.ht/~sourceware/binutils-gdb";
const BINUTILS_SYSROFF_SHA256: &str =
    "cfb4453d4514513d18f1cc2f98fcb97fcce2273b39a31df9507c20dbc5abc3d8";

fn write_executable_script(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

// MattOS target code is compiled only by source-built GCC and Binutils.  The
// host compiler is a stage-0 input used for exactly three things: building
// the cross toolchain below, building the host-running compiler proper in the
// gcc-runtime stage, and compiling build-machine helpers (`*_FOR_BUILD`,
// `BUILD_CC`, `HOSTCC`) that execute during a build and are never installed.

/// Stage-0 cross toolchain: MattOS Binutils and a libc-less pass-1 GCC, both
/// host-running and targeting MattOS, used to compile glibc.
const CROSS_TOOLCHAIN_OUTPUT: &str = "out/build/cross-toolchain";
/// Relocatable install of the complete MattOS-built GCC produced by the
/// gcc-runtime stage.  Every later target build compiles through it.
const TARGET_COMPILER_INSTALL: &str = "out/build/gcc-runtime/toolchain";
/// Wrapper directories derived only from the fixed paths above.
const TARGET_TOOL_WRAPPERS: &str = "out/toolchain/bin";
const CROSS_BINUTILS_TOOLS: &[&str] = &[
    "addr2line", "ar", "as", "c++filt", "elfedit", "ld", "ld.bfd", "nm", "objcopy", "objdump",
    "ranlib", "readelf", "size", "strings", "strip",
];
/// Multiarch directories resolved inside the configured sysroot (`=`), so the
/// cross linker finds MattOS libraries and their DT_NEEDED dependencies.
const CROSS_LINKER_LIB_PATH: &str =
    "=/usr/lib/x86_64-linux-gnu:=/lib/x86_64-linux-gnu:=/usr/lib:=/lib";

/// MattOS was previously compiled and validated with the Ubuntu host GCC,
/// whose distribution-patched defaults enable hardening that upstream GCC
/// leaves off.  Reproduce those defaults for userland target code as a specs
/// overlay (each clause yields to an explicit caller option) so that moving to
/// the source-built compiler does not silently weaken shipped binaries.  Like
/// Ubuntu's `distro_defaults`, the clauses live in the preprocessor options so
/// feature probes (`-E -dM`, e.g. glibc's `__CET__` check) see what
/// compilation will do.
/// PIE, build IDs, and GNU hash style are configured into GCC directly.
const MATTOS_HARDENING_SPECS: &str = "\
*cpp_unique_options:
+ %{!fno-stack-protector:%{!fstack-protector-explicit:%{!fstack-protector-all:%{!ffreestanding:%{!nostdlib:%{!fstack-protector:%{!fstack-protector-strong:-fstack-protector-strong}}}}}}} %{!Wformat:%{!Wformat=2:%{!Wformat=0:%{!Wall:-Wformat} %{!Wno-format-security:-Wformat-security}}}} %{!fno-stack-clash-protection:-fstack-clash-protection} %{!m16:%{!m32:%{!fcf-protection*:%{!fno-cf-protection:-fcf-protection}}}} %{!fzero-init-padding-bits*:-fzero-init-padding-bits=all} %{!Wno-bidi-chars:%{!Wbidi-chars*:-Wbidi-chars=any}} %{!O0:%{O*:%{!D_FORTIFY_SOURCE:%{!D_FORTIFY_SOURCE=*:%{!U_FORTIFY_SOURCE:-D_FORTIFY_SOURCE=3}}}}}

*link:
+ %{!fsanitize=*:--as-needed} %{!r:-z relro} %{static|shared|r|no-pie:;:-z now}

";

/// GCC configuration shared by the pass-1 and final compilers so both apply
/// the same code-generation and link defaults.
const MATTOS_GCC_DEFAULTS: &[&str] = &[
    "--enable-default-pie",
    "--enable-cet",
    "--enable-linker-build-id",
    "--with-linker-hash-style=gnu",
    "--disable-multilib",
    "--disable-nls",
    "--disable-werror",
    "--without-isl",
    "--without-zstd",
];

fn cross_toolchain_bin(repo_root: &Path) -> PathBuf {
    repo_root.join(CROSS_TOOLCHAIN_OUTPUT).join("install/bin")
}

fn cross_binutil(repo_root: &Path, tool: &str) -> PathBuf {
    cross_toolchain_bin(repo_root).join(format!("{TOOLCHAIN_TARGET}-{tool}"))
}

fn hardening_specs_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(CROSS_TOOLCHAIN_OUTPUT)
        .join("mattos-hardening.specs")
}

fn target_compiler_driver(repo_root: &Path, driver: &str) -> PathBuf {
    repo_root
        .join(TARGET_COMPILER_INSTALL)
        .join("usr/bin")
        .join(driver)
}

fn write_script_if_changed(path: &Path, body: &str) -> Result<()> {
    if fs::read_to_string(path).ok().as_deref() != Some(body) {
        write_executable_script(path, body)?;
    }
    Ok(())
}

fn link_binutils(repo_root: &Path, directory: &Path) -> Result<()> {
    for tool in CROSS_BINUTILS_TOOLS {
        let target = cross_binutil(repo_root, tool);
        if !target.is_file() {
            continue;
        }
        for name in [format!("{TOOLCHAIN_TARGET}-{tool}"), (*tool).to_string()] {
            let link = directory.join(name);
            if fs::read_link(&link).ok().as_deref() != Some(target.as_path()) {
                remove_path_if_exists(&link)?;
                std::os::unix::fs::symlink(&target, &link)?;
            }
        }
    }
    Ok(())
}

/// Userland C/C++ wrappers around a MattOS-built compiler driver: MattOS
/// sysroot, multiarch start files and libraries, and the hardening defaults.
fn userland_compiler_script(repo_root: &Path, driver: &Path) -> Result<String> {
    let sysroot = repo_root.join("out/sysroot");
    let multiarch = sysroot.join("usr/lib/x86_64-linux-gnu");
    Ok(format!(
        "#!/bin/sh\n# MattOS target compiler: source-built GCC and Binutils.\nexec {} -specs={} --sysroot={} -B{}/ -L{} {} \"$@\"\n",
        shell_escape(path_str(driver)?),
        shell_escape(path_str(&hardening_specs_path(repo_root))?),
        shell_escape(path_str(&sysroot)?),
        shell_escape(path_str(&multiarch)?),
        shell_escape(path_str(&multiarch)?),
        // Callers' later, more specific maps take precedence.
        shell_escape(&format!("-ffile-prefix-map={}=/usr/src/mattos", path_str(repo_root)?)),
    ))
}

/// Pass-1 wrappers used only to compile glibc (see `build_glibc`).
fn write_pass1_compiler_wrappers(repo_root: &Path) -> Result<(PathBuf, PathBuf)> {
    let directory = repo_root.join(CROSS_TOOLCHAIN_OUTPUT).join("pass1-bin");
    fs::create_dir_all(&directory)?;
    let mut wrappers = Vec::new();
    for driver in ["gcc", "g++"] {
        let raw = cross_toolchain_bin(repo_root).join(format!("{TOOLCHAIN_TARGET}-{driver}"));
        let wrapper = directory.join(format!("{TOOLCHAIN_TARGET}-{driver}"));
        write_script_if_changed(&wrapper, &userland_compiler_script(repo_root, &raw)?)?;
        wrappers.push(wrapper);
    }
    Ok((wrappers[0].clone(), wrappers[1].clone()))
}

/// Kbuild `CROSS_COMPILE` prefix for the kernel and its out-of-tree modules:
/// the libc-independent MattOS pass-1 GCC and MattOS Binutils.  The kernel
/// selects all of its own code-generation flags, so no userland defaults or
/// sysroot apply, and a glibc change never forces a kernel rebuild.
fn kernel_cross_compile(repo_root: &Path) -> Result<String> {
    let bin = cross_toolchain_bin(repo_root);
    if !bin.join(format!("{TOOLCHAIN_TARGET}-gcc")).is_file() {
        bail!("the MattOS kernel compiler is missing; build the cross-toolchain stage first")
    }
    Ok(format!("{}/{TOOLCHAIN_TARGET}-", bin.display()))
}

/// The MattOS userland target toolchain as seen by later build stages.
#[derive(Debug, Clone)]
struct TargetToolchain {
    /// Userland wrappers (`gcc`, `cc`, `g++`, `c++`, `cpp`, triplet-prefixed
    /// forms, and MattOS Binutils) placed first on a build's PATH.
    bin: PathBuf,
}

impl TargetToolchain {
    fn tool(&self, name: &str) -> PathBuf {
        self.bin.join(name)
    }
}

/// Returns the MattOS target toolchain once the gcc-runtime stage has
/// installed it, creating its stable wrapper directories on first use.
fn mattos_target_toolchain(repo_root: &Path) -> Result<Option<TargetToolchain>> {
    let gcc = target_compiler_driver(repo_root, "gcc");
    if !gcc.is_file()
        || !cross_binutil(repo_root, "ld").is_file()
        || !hardening_specs_path(repo_root).is_file()
    {
        return Ok(None);
    }
    let bin = repo_root.join(TARGET_TOOL_WRAPPERS);
    fs::create_dir_all(&bin)?;
    for (driver, names) in [
        ("gcc", &["gcc", "cc"][..]),
        ("g++", &["g++", "c++"][..]),
        ("cpp", &["cpp"][..]),
    ] {
        let raw = target_compiler_driver(repo_root, driver);
        let script = userland_compiler_script(repo_root, &raw)?;
        for name in names {
            write_script_if_changed(&bin.join(name), &script)?;
            write_script_if_changed(&bin.join(format!("{TOOLCHAIN_TARGET}-{name}")), &script)?;
        }
    }
    for tool in ["gcc-ar", "gcc-nm", "gcc-ranlib"] {
        let raw = target_compiler_driver(repo_root, tool);
        write_script_if_changed(
            &bin.join(tool),
            &format!("#!/bin/sh\nexec {} \"$@\"\n", shell_escape(path_str(&raw)?)),
        )?;
    }
    link_binutils(repo_root, &bin)?;
    Ok(Some(TargetToolchain { bin }))
}

fn require_mattos_target_toolchain(repo_root: &Path) -> Result<TargetToolchain> {
    mattos_target_toolchain(repo_root)?.context(
        "the MattOS-built target compiler is missing; build the cross-toolchain and gcc-runtime stages first",
    )
}

/// Environment for autotools-style builds whose host is MattOS.
fn toolchain_environment(toolchain: &TargetToolchain) -> Result<Vec<(&'static str, String)>> {
    let tool = |name: &str| path_str(&toolchain.tool(name)).map(str::to_string);
    let mut paths = vec![toolchain.bin.clone()];
    if let Some(host_path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&host_path));
    }
    Ok(vec![
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("LC_ALL", "C".to_string()),
        ("TZ", "UTC".to_string()),
        (
            "PATH",
            std::env::join_paths(paths)?.to_string_lossy().into_owned(),
        ),
        ("CC", tool(&format!("{TOOLCHAIN_TARGET}-gcc"))?),
        ("CXX", tool(&format!("{TOOLCHAIN_TARGET}-g++"))?),
        ("AR", tool("ar")?),
        ("AS", tool("as")?),
        ("LD", tool("ld")?),
        ("NM", tool("nm")?),
        ("RANLIB", tool("ranlib")?),
        ("STRIP", tool("strip")?),
        // Build-machine helpers run during the build and are never shipped.
        ("CC_FOR_BUILD", "gcc".to_string()),
        ("CXX_FOR_BUILD", "g++".to_string()),
    ])
}

/// glibc release the pass-1 compiler is configured for, so it selects the
/// glibc TLS stack-protector guard and other libc-provided features.
const CROSS_TOOLCHAIN_GLIBC_VERSION: &str = "2.43";

fn verify_cross_toolchain_glibc_version(repo_root: &Path) -> Result<()> {
    let version_h = fs::read_to_string(repo_root.join("src/system/libc/glibc/version.h"))
        .context("failed to read the imported glibc version")?;
    let expected = format!("#define VERSION \"{CROSS_TOOLCHAIN_GLIBC_VERSION}\"");
    if !version_h.lines().any(|line| line.trim() == expected) {
        bail!(
            "pass-1 GCC is configured for glibc {CROSS_TOOLCHAIN_GLIBC_VERSION}, but the imported glibc version differs; update CROSS_TOOLCHAIN_GLIBC_VERSION"
        )
    }
    Ok(())
}

/// Builds the stage-0 cross toolchain with the host compiler: MattOS Binutils
/// and a libc-less pass-1 GCC targeting MattOS with `out/sysroot` as its
/// system root.  Nothing produced here is installed into MattOS; pass 1 exists
/// only to compile glibc, and the Binutils are reused by every later stage.
fn build_cross_toolchain(repo_root: &Path) -> Result<()> {
    let output = repo_root.join(CROSS_TOOLCHAIN_OUTPUT);
    let install = output.join("install");
    let prereq_install = output.join("prerequisite-install");
    let sysroot = repo_root.join("out/sysroot");
    if !repo_root.join("src/toolchain/gcc/configure").is_file() {
        bail!("GCC source is missing; import the pinned GCC component first")
    }
    verify_cross_toolchain_glibc_version(repo_root)?;
    remove_path_if_exists(&output)?;
    fs::create_dir_all(&install)?;
    fs::create_dir_all(&prereq_install)?;

    let mut path = vec![install.join("bin")];
    if let Some(host_path) = std::env::var_os("PATH") {
        path.extend(std::env::split_paths(&host_path));
    }
    let host_env = vec![
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("LC_ALL", "C".to_string()),
        ("TZ", "UTC".to_string()),
        ("PATH", std::env::join_paths(path)?.to_string_lossy().into_owned()),
        ("CC", "gcc".to_string()),
        ("CXX", "g++".to_string()),
        ("CFLAGS", "-O2 -g0".to_string()),
        ("CXXFLAGS", "-O2 -g0".to_string()),
    ];
    let prefix = format!("--prefix={}", install.display());
    let sysroot_arg = format!("--with-sysroot={}", sysroot.display());
    let build_triplet = format!("--build={TOOLCHAIN_BUILD}");
    let host_triplet = format!("--host={TOOLCHAIN_BUILD}");
    let target_triplet = format!("--target={TOOLCHAIN_TARGET}");

    let binutils_source = output.join("binutils-source");
    let binutils_build = output.join("binutils-build");
    stage_binutils_source(repo_root, &binutils_source)?;
    fs::create_dir_all(&binutils_build)?;
    let lib_path = format!("--with-lib-path={CROSS_LINKER_LIB_PATH}");
    // Binutils are configured with host == target so ld runs in native mode
    // inside the sysroot: like the distribution linker MattOS was previously
    // built with, it resolves DT_NEEDED dependencies through DT_RUNPATH/
    // DT_RPATH, LD_LIBRARY_PATH, and `$sysroot/etc/ld.so.conf`.  A cross-mode
    // ld skips those searches and breaks links (e.g. libgcrypt's tests) that
    // rely on them.  The program prefix keeps the triplet-qualified names.
    let native_build = format!("--build={TOOLCHAIN_TARGET}");
    let native_host = format!("--host={TOOLCHAIN_TARGET}");
    let program_prefix = format!("--program-prefix={TOOLCHAIN_TARGET}-");
    let binutils_args = [
        prefix.as_str(),
        native_build.as_str(),
        native_host.as_str(),
        target_triplet.as_str(),
        program_prefix.as_str(),
        sysroot_arg.as_str(),
        lib_path.as_str(),
        // Emit DT_RUNPATH, not DT_RPATH, like Debian/Ubuntu binutils: an
        // RPATH cannot be overridden by LD_LIBRARY_PATH and resolves before
        // the loader's --library-path used by rootfs validation.
        "--enable-new-dtags",
        "--disable-nls",
        "--disable-werror",
        "--disable-gdb",
        "--disable-gdbserver",
        "--disable-gprofng",
        "--disable-gold",
        "--disable-sim",
        "--without-zstd",
        "--enable-deterministic-archives",
    ];
    let binutils_configure = binutils_source.join("configure");
    run_gcc_bootstrap_command(&binutils_build, &binutils_configure, &binutils_args, &host_env)
        .context("MattOS cross Binutils configure failed")?;
    run_gcc_bootstrap_command(
        &binutils_build,
        Path::new("make"),
        &["all-binutils", "all-gas", "all-ld"],
        &host_env,
    )
    .context("MattOS cross Binutils build failed")?;
    run_gcc_bootstrap_command(
        &binutils_build,
        Path::new("make"),
        &["install-binutils", "install-gas", "install-ld"],
        &host_env,
    )?;
    for tool in ["as", "ld", "ar", "nm", "ranlib", "objcopy", "objdump", "readelf", "strip"] {
        if !cross_binutil(repo_root, tool).is_file() {
            bail!("MattOS cross Binutils did not install {TOOLCHAIN_TARGET}-{tool}")
        }
    }

    // GCC's arithmetic prerequisites are linked statically into the
    // host-running compilers only; they are never MattOS runtimes.
    let prereq_sources = prepare_gcc_prerequisite_sources(repo_root, &output)?;
    // The pinned GMP/MPFR/MPC configure probes predate C23 prototypes.
    let mut prereq_env = host_env.clone();
    prereq_env.extend([
        ("CFLAGS", "-O2 -g0 -std=gnu17".to_string()),
        ("CXXFLAGS", "-O2 -g0 -std=gnu++17".to_string()),
    ]);
    let with_gmp = format!("--with-gmp={}", prereq_install.display());
    let with_mpfr = format!("--with-mpfr={}", prereq_install.display());
    let with_mpc = format!("--with-mpc={}", prereq_install.display());
    build_static_prerequisite(
        &prereq_sources.join("gmp-6.2.1"),
        &output.join("prerequisite-build/gmp"),
        &prereq_install,
        TOOLCHAIN_BUILD,
        &[],
        &prereq_env,
    )?;
    build_static_prerequisite(
        &prereq_sources.join("mpfr-4.1.0"),
        &output.join("prerequisite-build/mpfr"),
        &prereq_install,
        TOOLCHAIN_BUILD,
        std::slice::from_ref(&with_gmp),
        &prereq_env,
    )?;
    build_static_prerequisite(
        &prereq_sources.join("mpc-1.2.1"),
        &output.join("prerequisite-build/mpc"),
        &prereq_install,
        TOOLCHAIN_BUILD,
        &[with_gmp.clone(), with_mpfr.clone()],
        &prereq_env,
    )?;

    let pass1_build = output.join("gcc-pass1-build");
    fs::create_dir_all(&pass1_build)?;
    let glibc_version = format!("--with-glibc-version={CROSS_TOOLCHAIN_GLIBC_VERSION}");
    let mut pass1_args = vec![
        prefix.as_str(),
        build_triplet.as_str(),
        host_triplet.as_str(),
        target_triplet.as_str(),
        sysroot_arg.as_str(),
        glibc_version.as_str(),
        "--with-newlib",
        "--without-headers",
        with_gmp.as_str(),
        with_mpfr.as_str(),
        with_mpc.as_str(),
        "--enable-languages=c,c++",
        "--disable-bootstrap",
        "--disable-shared",
        "--disable-threads",
        "--disable-libatomic",
        "--disable-libgomp",
        "--disable-libquadmath",
        "--disable-libssp",
        "--disable-libvtv",
        "--disable-libstdcxx",
        "--disable-libsanitizer",
        "--disable-libcc1",
        "--disable-lto",
        "--disable-plugin",
        "--disable-analyzer",
    ];
    pass1_args.extend_from_slice(MATTOS_GCC_DEFAULTS);
    let target_prefix_map = format!(
        "-O2 -g0 -ffile-prefix-map={}=/usr/src/mattos/gcc -fdebug-prefix-map={}=/usr/src/mattos/gcc",
        repo_root.display(),
        repo_root.display()
    );
    let mut pass1_env = host_env.clone();
    pass1_env.extend([
        ("CFLAGS_FOR_TARGET", target_prefix_map.clone()),
        ("CXXFLAGS_FOR_TARGET", target_prefix_map),
    ]);
    let gcc_configure = repo_root.join("src/toolchain/gcc/configure");
    run_gcc_bootstrap_command(&pass1_build, &gcc_configure, &pass1_args, &pass1_env)
        .context("MattOS pass-1 GCC configure failed")?;
    run_gcc_bootstrap_command(
        &pass1_build,
        Path::new("make"),
        &["all-gcc", "all-target-libgcc"],
        &pass1_env,
    )
    .context("MattOS pass-1 GCC build failed")?;
    run_gcc_bootstrap_command(
        &pass1_build,
        Path::new("make"),
        &["install-gcc", "install-target-libgcc"],
        &pass1_env,
    )?;
    fs::write(hardening_specs_path(repo_root), MATTOS_HARDENING_SPECS)?;

    // Prove pass 1 targets MattOS with its own assembler and the specs parse.
    let probe_source = output.join("pass1-probe.c");
    let probe_object = output.join("pass1-probe.o");
    fs::write(&probe_source, "int mattos_pass1_probe(int value) { return value + 1; }\n")?;
    let specs = format!("-specs={}", hardening_specs_path(repo_root).display());
    run_gcc_bootstrap_command(
        &output,
        &install.join(format!("bin/{TOOLCHAIN_TARGET}-gcc")),
        &[
            specs.as_str(),
            "-O2",
            "-ffreestanding",
            "-c",
            path_str(&probe_source)?,
            "-o",
            path_str(&probe_object)?,
        ],
        &pass1_env,
    )
    .context("MattOS pass-1 GCC could not compile a probe object")?;
    let header = Command::new(cross_binutil(repo_root, "readelf"))
        .arg("-h")
        .arg(&probe_object)
        .output()?;
    if !String::from_utf8_lossy(&header.stdout).contains("Advanced Micro Devices X86-64") {
        bail!("MattOS pass-1 GCC did not produce an x86-64 ELF object")
    }
    fs::write(
        output.join("configure-invocation.txt"),
        format!(
            "binutils: {} {}\ngcc-pass1: {} {}\nmake all-gcc all-target-libgcc\n",
            binutils_configure.display(),
            binutils_args.join(" "),
            gcc_configure.display(),
            pass1_args.join(" "),
        ),
    )?;
    println!("built MattOS cross Binutils and pass-1 GCC for {TOOLCHAIN_TARGET}");
    Ok(())
}

fn build_binutils(repo_root: &Path) -> Result<()> {
    let imported_source = repo_root.join("src/toolchain/binutils");
    let output = repo_root.join("out/build/binutils");
    let source = output.join("source");
    let native_build = output.join("native-build");
    let native_install = output.join("install");
    if !imported_source.join("configure").is_file() {
        bail!(
            "Binutils source is missing at {}",
            imported_source.display()
        )
    }
    if !repo_root.join("out/sysroot/usr/include/stdio.h").is_file() {
        bail!("Binutils requires the completed MattOS development sysroot")
    }
    let toolchain = require_mattos_target_toolchain(repo_root)?;
    remove_path_if_exists(&output)?;
    stage_binutils_source(repo_root, &source)?;
    for directory in [&native_build, &native_install] {
        fs::create_dir_all(directory)?;
    }

    let configure = source.join("configure");
    let native_env = toolchain_environment(&toolchain)?;
    let native_args = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--build=x86_64-build-linux-gnu",
        "--host=x86_64-pc-linux-gnu",
        "--target=x86_64-pc-linux-gnu",
        "--with-sysroot=/",
        "--with-build-sysroot=../../sysroot",
        "--disable-nls",
        "--disable-werror",
        "--disable-gdb",
        "--disable-gdbserver",
        "--disable-gprofng",
        "--disable-gold",
        "--disable-sim",
        "--without-zstd",
        "--enable-deterministic-archives",
    ];
    run_gcc_bootstrap_command(&native_build, &configure, &native_args, &native_env)
        .context("MattOS-native Binutils configure failed")?;
    run_gcc_bootstrap_command(
        &native_build,
        Path::new("make"),
        &["-j", "4", "all-binutils", "all-gas", "all-ld"],
        &native_env,
    )
    .context("MattOS-native Binutils build failed")?;
    let destdir = format!("DESTDIR={}", native_install.display());
    run_gcc_bootstrap_command(
        &native_build,
        Path::new("make"),
        &[
            destdir.as_str(),
            "install-binutils",
            "install-gas",
            "install-ld",
        ],
        &native_env,
    )?;
    let tools = [
        "addr2line",
        "ar",
        "as",
        "c++filt",
        "elfedit",
        "ld",
        "nm",
        "objcopy",
        "objdump",
        "ranlib",
        "readelf",
        "size",
        "strings",
        "strip",
    ];
    for tool in tools {
        if !native_install.join("usr/bin").join(tool).is_file() {
            bail!("MattOS-native Binutils did not install /usr/bin/{tool}")
        }
    }
    fs::write(
        output.join("configure-invocation.txt"),
        format!(
            "native: CC={} CXX={} {} {}\n",
            toolchain.tool(&format!("{TOOLCHAIN_TARGET}-gcc")).display(),
            toolchain.tool(&format!("{TOOLCHAIN_TARGET}-g++")).display(),
            configure.display(),
            native_args.join(" ")
        ),
    )?;
    println!("built source-native Binutils for {TOOLCHAIN_TARGET}");
    Ok(())
}

/// Output-owned Binutils source mirror including the pinned generated manual
/// that the imported tree omits.
fn stage_binutils_source(repo_root: &Path, source: &Path) -> Result<()> {
    let sysroff_info = ensure_binutils_sysroff_info(repo_root)?;
    copy_imported_working_tree(repo_root, Path::new("src/toolchain/binutils"), source)?;
    fs::copy(&sysroff_info, source.join("binutils/sysroff.info")).with_context(|| {
        format!(
            "failed to stage {} into output-owned Binutils source mirror",
            sysroff_info.display()
        )
    })?;
    Ok(())
}

fn ensure_binutils_sysroff_info(repo_root: &Path) -> Result<PathBuf> {
    let cache = repo_root
        .join("out/cache/binutils")
        .join(BINUTILS_UPSTREAM_COMMIT);
    let file = cache.join("sysroff.info");
    if file.is_file() {
        let actual = performance::sha256_file(&file)?;
        if actual != BINUTILS_SYSROFF_SHA256 {
            bail!(
                "cached Binutils sysroff.info checksum mismatch: expected {}, got {} at {}",
                BINUTILS_SYSROFF_SHA256,
                actual,
                file.display()
            );
        }
        return Ok(file);
    }

    fs::create_dir_all(&cache).with_context(|| format!("failed to create {}", cache.display()))?;
    let git_dir = repo_root.join("out/cache/binutils/upstream.git");
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
            BINUTILS_UPSTREAM_MIRROR,
            BINUTILS_UPSTREAM_COMMIT,
        ],
    )?;
    let object = format!("{BINUTILS_UPSTREAM_COMMIT}:binutils/sysroff.info");
    let output = Command::new("git")
        .args([git_dir_arg.as_str(), "show", object.as_str()])
        .output()
        .context("failed to read sysroff.info from pinned Binutils commit")?;
    if !output.status.success() {
        bail!(
            "pinned Binutils commit did not provide binutils/sysroff.info: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let temp = file.with_extension("info.tmp");
    fs::write(&temp, &output.stdout)
        .with_context(|| format!("failed to write {}", temp.display()))?;
    let actual = performance::sha256_file(&temp)?;
    if actual != BINUTILS_SYSROFF_SHA256 {
        let _ = fs::remove_file(&temp);
        bail!(
            "downloaded Binutils sysroff.info checksum mismatch: expected {}, got {}",
            BINUTILS_SYSROFF_SHA256,
            actual
        );
    }
    fs::rename(&temp, &file).with_context(|| format!("failed to publish {}", file.display()))?;
    Ok(file)
}

fn prepare_gcc_prerequisite_sources(repo_root: &Path, output: &Path) -> Result<PathBuf> {
    let source = repo_root.join("src/toolchain/gcc");
    let driver = output.join("prerequisite-fetch");
    // Keep checksum-verified prerequisite archives and extracted sources outside
    // the disposable stage directory so a warmed tree remains buildable offline.
    let cache = repo_root.join("out/cache/gcc-prerequisites");
    fs::create_dir_all(driver.join("gcc"))?;
    fs::create_dir_all(driver.join("contrib"))?;
    fs::create_dir_all(&cache)?;
    for relative in [
        "gcc/BASE-VER",
        "contrib/download_prerequisites",
        "contrib/prerequisites.sha512",
    ] {
        let destination = driver.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let metadata = fs::metadata(source.join(relative))?;
        fs::copy(source.join(relative), &destination)?;
        preserve_permissions(&metadata, &destination)?;
    }
    let directory = format!("--directory={}", cache.display());
    run_gcc_bootstrap_command(
        &driver,
        Path::new("./contrib/download_prerequisites"),
        &[directory.as_str(), "--no-isl", "--sha512"],
        &[("LC_ALL", "C".to_string()), ("TZ", "UTC".to_string())],
    )?;
    Ok(cache)
}

fn build_static_prerequisite(
    source: &Path,
    build: &Path,
    install: &Path,
    host: &str,
    configure_extra: &[String],
    env: &[(&str, String)],
) -> Result<()> {
    fs::create_dir_all(build)?;
    let prefix = format!("--prefix={}", install.display());
    let mut owned_args = vec![
        prefix,
        format!("--build={TOOLCHAIN_BUILD}"),
        format!("--host={host}"),
        "--disable-shared".to_string(),
        "--enable-static".to_string(),
    ];
    owned_args.extend_from_slice(configure_extra);
    let args = owned_args.iter().map(String::as_str).collect::<Vec<_>>();
    run_gcc_bootstrap_command(build, &source.join("configure"), &args, env)?;
    // MAKEFLAGS is installed from the scheduler's launch-time child-job grant.
    // Do not retain a recipe-local cap here: these prerequisite builds are part
    // of the GCC compiler stage and must use the same authoritative grant.
    run_gcc_bootstrap_command(build, Path::new("make"), &[], env)?;
    run_gcc_bootstrap_command(build, Path::new("make"), &["install"], env)?;
    Ok(())
}

fn log_gcc_info_index_boundary(label: &str, install: &Path) -> Result<()> {
    let index = install.join("usr/share/info/dir");
    let state = match fs::symlink_metadata(&index) {
        Ok(metadata) => format!(
            "exists type={:?} size={}",
            metadata.file_type(),
            metadata.len()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "absent".to_string(),
        Err(error) => format!("metadata-error={error}"),
    };
    performance::append_active_stage_log(&format!(
        "gcc-info-normalization boundary={label} install={} index={} {state}",
        install.display(),
        index.display()
    ))
}

fn build_gcc_toolchain(repo_root: &Path) -> Result<()> {
    let output = repo_root.join("out/build/gcc-toolchain");
    let build = output.join("build");
    let install = output.join("install");
    let prereq_install = output.join("prerequisite-install");
    performance::trace_log_context("build_gcc_toolchain-entry");
    log_gcc_info_index_boundary("build_gcc_toolchain-entry", &install)?;
    if !repo_root.join("src/toolchain/gcc/configure").is_file() {
        bail!("GCC source is missing; import the pinned GCC component first")
    }
    let toolchain = require_mattos_target_toolchain(repo_root)?;
    remove_path_if_exists(&output)?;
    fs::create_dir_all(&build)?;
    fs::create_dir_all(&install)?;
    fs::create_dir_all(&prereq_install)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            "../prerequisite-install",
            build.join("prerequisite-install"),
        )?;
        std::os::unix::fs::symlink("../../sysroot", output.join("mattos-sysroot"))?;
        std::os::unix::fs::symlink("../mattos-sysroot", build.join("mattos-sysroot"))?;
    }
    let env = toolchain_environment(&toolchain)?;
    let mut env = env;
    env.extend([
        ("CFLAGS", "-O2 -g0 -std=gnu17".to_string()),
        ("CXXFLAGS", "-O2 -g0 -std=gnu++17".to_string()),
    ]);
    let prereq_sources = prepare_gcc_prerequisite_sources(repo_root, &output)?;
    let gmp_source = prereq_sources.join("gmp-6.2.1");
    let mpfr_source = prereq_sources.join("mpfr-4.1.0");
    let mpc_source = prereq_sources.join("mpc-1.2.1");
    build_static_prerequisite(
        &gmp_source,
        &output.join("prerequisite-build/gmp"),
        &prereq_install,
        TOOLCHAIN_TARGET,
        &[],
        &env,
    )?;
    let prereq_with_gmp = format!("--with-gmp={}", prereq_install.display());
    build_static_prerequisite(
        &mpfr_source,
        &output.join("prerequisite-build/mpfr"),
        &prereq_install,
        TOOLCHAIN_TARGET,
        std::slice::from_ref(&prereq_with_gmp),
        &env,
    )?;
    let prereq_with_mpfr = format!("--with-mpfr={}", prereq_install.display());
    build_static_prerequisite(
        &mpc_source,
        &output.join("prerequisite-build/mpc"),
        &prereq_install,
        TOOLCHAIN_TARGET,
        &[prereq_with_gmp, prereq_with_mpfr],
        &env,
    )?;

    // Invoke GCC through a stable relative path and use stable relative
    // prerequisite prefixes. GCC exposes its configure command in `gcc -v`,
    // so absolute workspace paths here would contaminate the installed driver.
    let configure = PathBuf::from("../../../../src/toolchain/gcc/configure");
    let with_gmp = "--with-gmp=../prerequisite-install".to_string();
    let with_mpfr = "--with-mpfr=../prerequisite-install".to_string();
    let with_mpc = "--with-mpc=../prerequisite-install".to_string();
    let mut configure_args = vec![
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--libexecdir=/usr/libexec",
        "--build=x86_64-build-linux-gnu",
        "--host=x86_64-pc-linux-gnu",
        "--target=x86_64-pc-linux-gnu",
        "--with-sysroot=/",
        "--with-build-sysroot=../mattos-sysroot",
        "--with-native-system-header-dir=/usr/include",
        "--with-as=/usr/bin/as",
        "--with-ld=/usr/bin/ld",
        with_gmp.as_str(),
        with_mpfr.as_str(),
        with_mpc.as_str(),
        "--enable-languages=c,c++",
        "--disable-bootstrap",
        "--disable-checking",
        "--disable-analyzer",
        "--disable-libsanitizer",
        "--disable-libssp",
        "--disable-libquadmath",
        "--disable-libgomp",
        "--disable-libatomic",
        "--disable-libvtv",
        "--disable-libcc1",
        "--disable-lto",
        "--disable-plugin",
        "--disable-libstdcxx-pch",
    ];
    // The compiler installed systems use gets the same code-generation
    // defaults (PIE, CET, build IDs, GNU hash style) as the compilers that
    // build MattOS itself; its hardening specs are installed below.
    configure_args.extend_from_slice(MATTOS_GCC_DEFAULTS);
    let mut gcc_env = env.clone();
    // GCC feeds the selected linker command into `checksum-options`, which is
    // then hashed into cc1/cc1plus for PCH compatibility.  An absolute wrapper
    // path therefore makes otherwise identical compilers checkout-dependent.
    // The wrapper directory is already first in PATH, so use stable basenames
    // for the compiler proper while retaining absolute paths for prerequisite
    // builds that execute from several different working directories.
    let cc_name = format!("{TOOLCHAIN_TARGET}-gcc");
    let cxx_name = format!("{TOOLCHAIN_TARGET}-g++");
    gcc_env.extend([
        ("CC", cc_name.clone()),
        ("CXX", cxx_name.clone()),
        ("CFLAGS", "-O2 -g0".to_string()),
        ("CXXFLAGS", "-O2 -g0".to_string()),
        ("LDFLAGS", "-Wl,-z,relro -Wl,-z,now".to_string()),
    ]);
    run_gcc_bootstrap_command(&build, &configure, &configure_args, &gcc_env)
        .context("MattOS-native GCC configure failed")?;
    run_gcc_bootstrap_command(&build, Path::new("make"), &["all-gcc"], &gcc_env)
        .context("MattOS-native GCC compiler build failed")?;
    let destdir = format!("DESTDIR={}", install.display());
    log_gcc_info_index_boundary("before-install-gcc", &install)?;
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &[destdir.as_str(), "install-gcc"],
        &gcc_env,
    )?;
    log_gcc_info_index_boundary("after-install-gcc", &install)?;
    // `install-gcc` invokes install-info for several manuals.  That shared
    // index is updated by parallel install rules and can omit/reorder entries
    // between otherwise identical builds.  The individual .info manuals are
    // authoritative; Debian-compatible package installation regenerates the
    // directory index through install-info, so do not publish this transient
    // build-time index.
    let info_dir_index = install.join("usr/share/info/dir");
    log_gcc_info_index_boundary("before-normalization", &install)?;
    remove_path_if_exists(&info_dir_index)?;
    // After its built-in specs, GCC reads `<libdir>/gcc/<target>/specs` as an
    // overlay, so installing the MattOS hardening specs there makes the
    // shipped compiler harden every program it builds, exactly like the
    // wrappers that build MattOS (`-specs=`). The versioned
    // `<target>/<version>/specs` must not be used: GCC treats that file as a
    // complete replacement for its built-in specs and would drop defaults
    // such as --build-id, --eh-frame-hdr and --hash-style=gnu.
    let native_specs = install
        .join("usr/lib/x86_64-linux-gnu/gcc")
        .join(TOOLCHAIN_TARGET)
        .join("specs");
    let native_library = native_specs.parent().context("GCC specs path has no parent")?;
    if !native_library.join(GCC_TOOLCHAIN_VERSION).is_dir() {
        bail!(
            "native GCC library directory {} is missing; cannot install its hardening specs",
            native_library.join(GCC_TOOLCHAIN_VERSION).display()
        );
    }
    fs::write(&native_specs, MATTOS_HARDENING_SPECS)?;
    log_gcc_info_index_boundary("after-normalization", &install)?;
    for relative in [
        "usr/bin/gcc",
        "usr/bin/g++",
        "usr/bin/cpp",
        "usr/libexec/gcc/x86_64-pc-linux-gnu/15.3.0/cc1",
        "usr/libexec/gcc/x86_64-pc-linux-gnu/15.3.0/cc1plus",
        "usr/libexec/gcc/x86_64-pc-linux-gnu/15.3.0/collect2",
    ] {
        if !install.join(relative).is_file() {
            bail!("MattOS-native GCC did not install /{relative}")
        }
    }
    for helper in ["cc1", "cc1plus", "collect2"] {
        let needed = elf_needed_names(
            &install
                .join("usr/libexec/gcc")
                .join(TOOLCHAIN_TARGET)
                .join(GCC_TOOLCHAIN_VERSION)
                .join(helper),
        )?;
        if needed.iter().any(|name| {
            name.starts_with("libgmp")
                || name.starts_with("libmpfr")
                || name.starts_with("libmpc")
                || name.starts_with("libzstd")
        }) {
            bail!("installed GCC helper {helper} leaks bootstrap libraries: {needed:?}")
        }
    }
    let mut installed_files = Vec::new();
    collect_regular_files(&install, &mut installed_files)?;
    let build_root = repo_root.to_string_lossy();
    for file in installed_files {
        let header = Command::new("readelf").args(["-h"]).arg(&file).output()?;
        if !header.status.success() {
            continue;
        }
        let bytes = fs::read(&file)?;
        if bytes
            .windows(build_root.len())
            .any(|window| window == build_root.as_bytes())
        {
            bail!(
                "installed GCC ELF {} embeds the host build root",
                file.display()
            )
        }
        let dynamic = Command::new("readelf").args(["-d"]).arg(&file).output()?;
        let dynamic = String::from_utf8_lossy(&dynamic.stdout);
        if dynamic
            .lines()
            .any(|line| line.contains("(RPATH)") || line.contains("(RUNPATH)"))
        {
            bail!(
                "installed GCC ELF {} contains RPATH/RUNPATH",
                file.display()
            )
        }
    }
    fs::write(
        output.join("configure-invocation.txt"),
        format!(
            "CC={} CXX={} CC_FOR_BUILD=gcc CXX_FOR_BUILD=g++ {} {}\nmake all-gcc\nmake DESTDIR={} install-gcc\n",
            cc_name,
            cxx_name,
            configure.display(),
            configure_args.join(" "),
            install.display()
        ),
    )?;
    println!("built source-native GCC C/C++ compiler for {TOOLCHAIN_TARGET}");
    Ok(())
}

fn build_make(repo_root: &Path) -> Result<()> {
    let imported = repo_root.join("src/build-tools/make");
    let gnulib = repo_root.join("src/build-support/gnulib");
    let output = repo_root.join("out/build/make");
    let source = output.join("source");
    let build = output.join("build");
    let install = output.join("install");
    if !imported.join("bootstrap").is_file() {
        bail!("GNU Make source is missing at {}", imported.display())
    }
    if !gnulib.join("gnulib-tool").is_file() {
        bail!("pinned Gnulib source is missing at {}", gnulib.display())
    }
    remove_path_if_exists(&output)?;
    fs::create_dir_all(&source)?;
    fs::create_dir_all(&build)?;
    fs::create_dir_all(&install)?;
    copy_tree_contents(&imported, &source)?;
    let gnulib_arg = format!("--gnulib-srcdir={}", gnulib.display());
    run_gcc_bootstrap_command(
        &source,
        Path::new("./bootstrap"),
        &[
            "--gen",
            "--no-git",
            "--no-bootstrap-sync",
            "--copy",
            gnulib_arg.as_str(),
        ],
        &[
            ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
            ("LC_ALL", "C".to_string()),
            ("TZ", "UTC".to_string()),
        ],
    )?;
    let toolchain = require_mattos_target_toolchain(repo_root)?;
    let mut env = toolchain_environment(&toolchain)?;
    env.extend([
        ("CC", format!("{TOOLCHAIN_TARGET}-gcc")),
        ("CXX", format!("{TOOLCHAIN_TARGET}-g++")),
        ("CFLAGS", "-O2 -g0".to_string()),
        ("LDFLAGS", "-Wl,-z,relro -Wl,-z,now".to_string()),
    ]);
    let configure_args = [
        "--prefix=/usr",
        "--build=x86_64-build-linux-gnu",
        "--host=x86_64-pc-linux-gnu",
        "--disable-nls",
    ];
    run_gcc_bootstrap_command(&build, &source.join("configure"), &configure_args, &env)?;
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &["-j", "4", "MAKE_MAINTAINER_MODE=", "MAKE_CFLAGS="],
        &env,
    )?;
    let destdir = format!("DESTDIR={}", install.display());
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &[
            destdir.as_str(),
            "MAKE_MAINTAINER_MODE=",
            "MAKE_CFLAGS=",
            "install",
        ],
        &env,
    )?;
    if !install.join("usr/bin/make").is_file() {
        bail!("MattOS-native GNU Make did not install /usr/bin/make")
    }
    fs::write(
        output.join("configure-invocation.txt"),
        format!(
            "gnulib={}\nCC={} {} {}\nmake -j4 MAKE_MAINTAINER_MODE= MAKE_CFLAGS=\nmake DESTDIR={} MAKE_MAINTAINER_MODE= MAKE_CFLAGS= install\n",
            gnulib.display(),
            format!("{TOOLCHAIN_TARGET}-gcc"),
            source.join("configure").display(),
            configure_args.join(" "),
            install.display()
        ),
    )?;
    println!("built source-native GNU Make for {TOOLCHAIN_TARGET}");
    Ok(())
}

fn copy_tree_contents(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&from)?;
        if metadata.is_dir() {
            if fs::symlink_metadata(&to)
                .map(|existing| !existing.is_dir() || existing.file_type().is_symlink())
                .unwrap_or(false)
            {
                remove_path_if_exists(&to)?;
            }
            copy_tree_contents(&from, &to)?;
        } else if metadata.file_type().is_symlink() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            remove_path_if_exists(&to)?;
            std::os::unix::fs::symlink(fs::read_link(&from)?, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            if fs::symlink_metadata(&to)
                .map(|existing| existing.is_dir() || existing.file_type().is_symlink())
                .unwrap_or(false)
            {
                remove_path_if_exists(&to)?;
            }
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
        }
    }
    Ok(())
}

fn hydrate_development_sysroot(repo_root: &Path, installs: &[PathBuf]) -> Result<()> {
    let sysroot = repo_root.join("out/sysroot/usr");
    for install in installs {
        let include = install.join("include");
        if include.is_dir() {
            copy_tree_contents(&include, &sysroot.join("include"))?;
        }
        let library = install.join("lib/x86_64-linux-gnu");
        if library.is_dir() {
            copy_tree_contents(&library, &sysroot.join("lib/x86_64-linux-gnu"))?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct LocalToolEnv {
    tool_root: PathBuf,
    tool_bin_dir: PathBuf,
    tool_lib_dir: PathBuf,
    tool_include_dir: PathBuf,
    bison_pkg_data_dir: PathBuf,
    m4_bin: PathBuf,
}

fn local_tool_env(repo_root: &Path) -> Option<LocalToolEnv> {
    let root = repo_root.join(".tools/rootless/usr");
    let bin = root.join("bin");
    let lib = root.join("lib/x86_64-linux-gnu");
    let include = root.join("include");
    let bison_pkg = root.join("share/bison");
    let m4 = bin.join("m4");
    if bin.exists() && lib.exists() && include.exists() && bison_pkg.exists() && m4.exists() {
        Some(LocalToolEnv {
            tool_root: root,
            tool_bin_dir: bin,
            tool_lib_dir: lib,
            tool_include_dir: include,
            bison_pkg_data_dir: bison_pkg,
            m4_bin: m4,
        })
    } else {
        None
    }
}

fn assert_kernel_build_path_safe(repo_root: &Path) -> Result<()> {
    if cfg!(unix) && std::env::var("WSL_DISTRO_NAME").is_ok() {
        let root = repo_root.to_string_lossy();
        if root.starts_with("/mnt/") {
            bail!(
                "refusing kernel build from Windows-mounted path {}. Use Linux filesystem path like ~/src/MattOS",
                repo_root.display()
            )
        }
    }
    Ok(())
}

#[cfg(test)]
fn install_fake_mattos_target_toolchain(repo_root: &Path) {
    for path in [
        target_compiler_driver(repo_root, "gcc"),
        cross_binutil(repo_root, "ld"),
        cross_toolchain_bin(repo_root).join(format!("{TOOLCHAIN_TARGET}-gcc")),
        hardening_specs_path(repo_root),
    ] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "").unwrap();
    }
}

#[cfg(test)]
mod target_toolchain_tests {
    use super::*;

    #[test]
    fn target_toolchain_is_absent_until_gcc_runtime_installs_it() {
        let temporary = tempfile::tempdir().unwrap();
        assert!(mattos_target_toolchain(temporary.path()).unwrap().is_none());
        assert!(require_mattos_target_toolchain(temporary.path()).is_err());
        assert!(kernel_cross_compile(temporary.path()).is_err());
    }

    #[test]
    fn userland_wrappers_use_mattos_gcc_sysroot_and_hardening_specs() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        install_fake_mattos_target_toolchain(repo);
        let toolchain = mattos_target_toolchain(repo).unwrap().unwrap();
        let driver = target_compiler_driver(repo, "gcc");
        for name in ["gcc", "cc", "x86_64-pc-linux-gnu-gcc"] {
            let script = fs::read_to_string(toolchain.tool(name)).unwrap();
            assert!(script.contains(&format!("exec {}", driver.display())), "{name}: {script}");
            assert!(script.contains(&format!("-specs={}", hardening_specs_path(repo).display())));
            assert!(script.contains(&format!("--sysroot={}", repo.join("out/sysroot").display())));
            assert!(script.contains(&format!(
                "-B{}/",
                repo.join("out/sysroot/usr/lib/x86_64-linux-gnu").display()
            )));
            assert!(!script.contains("exec /usr/bin/gcc"));
        }
        let gxx = fs::read_to_string(toolchain.tool("c++")).unwrap();
        assert!(gxx.contains(&target_compiler_driver(repo, "g++").display().to_string()));
        for tool in ["ld", "x86_64-pc-linux-gnu-ld"] {
            assert_eq!(
                fs::read_link(toolchain.tool(tool)).unwrap(),
                cross_binutil(repo, "ld")
            );
        }
        assert_eq!(
            kernel_cross_compile(repo).unwrap(),
            format!("{}/x86_64-pc-linux-gnu-", cross_toolchain_bin(repo).display())
        );
    }

    #[test]
    fn hardening_specs_reproduce_validated_distribution_defaults() {
        for required in [
            "-fstack-protector-strong",
            "-fstack-clash-protection",
            "-fcf-protection",
            "-Wformat-security",
            "-D_FORTIFY_SOURCE=3",
            "--as-needed",
            "-z relro",
            "-z now",
        ] {
            assert!(MATTOS_HARDENING_SPECS.contains(required), "missing {required}");
        }
        // Every default must yield to an explicit caller choice.
        for opt_out in [
            "!fno-stack-protector",
            "!fno-stack-clash-protection",
            "!fno-cf-protection",
            "!U_FORTIFY_SOURCE",
            "!ffreestanding",
        ] {
            assert!(MATTOS_HARDENING_SPECS.contains(opt_out), "missing opt-out {opt_out}");
        }
        for configured in ["--enable-default-pie", "--enable-linker-build-id", "--with-linker-hash-style=gnu"] {
            assert!(MATTOS_GCC_DEFAULTS.contains(&configured));
        }
    }

    #[test]
    fn target_builds_never_select_the_host_compiler() {
        let source = include_str!("toolchain.rs");
        let production = &source[..source.find("#[cfg(test)]").unwrap()];
        assert!(!production.contains("exec /usr/bin/gcc"));
        assert!(!production.contains("exec /usr/bin/g++"));
        assert!(!production.contains("binutils/cross-install"));
        let glibc = &production[production.find("fn build_glibc").unwrap()..];
        let glibc = &glibc[..glibc.find("\nfn ").unwrap()];
        assert!(glibc.contains("write_pass1_compiler_wrappers"));
        assert!(glibc.contains("--build=x86_64-build-linux-gnu"));
        let kernel = &production[production.find("fn build_kernel").unwrap()..];
        let kernel = &kernel[..kernel.find("\nfn ").unwrap()];
        assert!(kernel.contains("CROSS_COMPILE="));
    }

    #[test]
    fn sysroot_environment_puts_mattos_toolchain_first_on_path() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        install_fake_mattos_target_toolchain(repo);
        fs::create_dir_all(repo.join("src/tools/mattos-build")).unwrap();
        fs::write(repo.join("src/tools/mattos-build/Cargo.toml"), "").unwrap();
        fs::create_dir_all(repo.join("out/sysroot/usr/include")).unwrap();
        fs::write(repo.join("out/sysroot/usr/include/stdio.h"), "").unwrap();
        let cwd = repo.join("out/build/example");
        fs::create_dir_all(&cwd).unwrap();
        let mut command = Command::new("true");
        apply_mattos_sysroot_environment(
            &mut command,
            &cwd,
            "make",
            &[("PATH", "/usr/local/bin:/usr/bin".to_string())],
        )
        .unwrap();
        let path = command
            .get_envs()
            .find(|(key, _)| *key == OsStr::new("PATH"))
            .and_then(|(_, value)| value)
            .unwrap()
            .to_owned();
        let entries = std::env::split_paths(&path).collect::<Vec<_>>();
        assert_eq!(entries[0], repo.join(TARGET_TOOL_WRAPPERS));
        assert_eq!(entries[1..], [PathBuf::from("/usr/local/bin"), PathBuf::from("/usr/bin")]);
    }
}
