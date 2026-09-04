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
        .args(["diff", "--binary", "HEAD", "--", relative])
        .current_dir(repo_root)
        .output()?;
    if !diff.status.success() {
        bail!("git could not inspect the Linux working tree");
    }
    let untracked = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--others",
            "--exclude-standard",
            "--",
            relative,
        ])
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
    fs::create_dir_all(&out_root)?;
    if source_changed {
        remove_path_if_exists(&source)?;
        remove_path_if_exists(&build)?;
    }
    remove_path_if_exists(&out_root.join("modules"))?;
    if !source.is_dir() {
        copy_imported_working_tree(repo_root, Path::new("src/kernel/linux"), &source)?;
        fs::write(&source_identity_path, &source_identity)?;
    }
    fs::create_dir_all(&build).with_context(|| format!("failed to create {}", build.display()))?;

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
    // The kernel does not consume SOURCE_DATE_EPOCH directly for all of its
    // generated metadata.  Pin the release banner and built-in initramfs cpio
    // mtimes explicitly; otherwise two healthy builds differ only by their
    // wall-clock build time and the GNU build ID derived from it.
    let kernel_reproducible_args = [
        "KBUILD_BUILD_TIMESTAMP=2026-01-01 00:00:00 UTC",
        "KBUILD_BUILD_USER=mattos",
        "KBUILD_BUILD_HOST=mattos-build",
        "KBUILD_BUILD_VERSION=1",
        "KCONFIG_NOTIMESTAMP=1",
    ];
    let mut olddefconfig_args = vec![output_arg.as_str(), "olddefconfig"];
    olddefconfig_args.extend(kernel_reproducible_args);
    run_cmd_with_env(&source, "make", &olddefconfig_args, env.as_ref())?;
    validate_kernel_config_policy(&fs::read_to_string(build.join(".config"))?, &policy)?;
    let mut build_args = vec![output_arg.as_str(), "-j", "4"];
    build_args.extend(kernel_reproducible_args);
    run_cmd_with_env(&source, "make", &build_args, env.as_ref()).context("kernel build failed")?;

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
        "modules_install",
        modlib.as_str(),
        "DEPMOD=true",
    ];
    modules_install_args.extend(kernel_reproducible_args);
    run_cmd_with_env(&source, "make", &modules_install_args, env.as_ref())?;
    for link in ["build", "source"] {
        remove_path_if_exists(&module_dir.join(link))?;
    }
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

const GLIBC_MINIMUM_KERNEL: &str = "5.10.0";
const MATTOS_SOURCE_DATE_EPOCH: &str = "1767225600";

fn build_glibc(repo_root: &Path) -> Result<()> {
    let linux = repo_root.join("src/kernel/linux");
    let source = repo_root.join("src/system/libc/glibc");
    let output = repo_root.join("out/build/glibc");
    let build = output.join("build");
    let install = output.join("install");
    let sysroot = repo_root.join("out/sysroot");
    let headers_root = sysroot.join("usr");
    if !linux.join("Makefile").is_file() {
        bail!(
            "Linux source not found at {}; import it first",
            linux.display()
        )
    }
    if !source.join("configure").is_file() {
        bail!(
            "glibc source not found at {}; run `mattos-build upstream import glibc`",
            source.display()
        )
    }

    remove_path_if_exists(&output)?;
    remove_path_if_exists(&sysroot)?;
    fs::create_dir_all(&build)?;
    fs::create_dir_all(&install)?;
    fs::create_dir_all(&headers_root)?;

    let linux_source = output.join("linux-source");
    let linux_build = output.join("linux-build");
    copy_imported_working_tree(repo_root, Path::new("src/kernel/linux"), &linux_source)?;
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
    if !sysroot.join("usr/include/linux/version.h").is_file()
        || !sysroot.join("usr/include/asm/unistd.h").is_file()
    {
        bail!("Linux headers_install did not create the required UAPI header tree")
    }
    copy_tree_contents(
        &sysroot.join("usr/include"),
        &output.join("linux-headers/usr/include"),
    )?;
    let mut uapi_files = Vec::new();
    collect_regular_files(&output.join("linux-headers/usr/include"), &mut uapi_files)?;
    let mut uapi_inventory = String::from(
        "revision=f17f39c917cd4aac09db1a6a083ef5ec09b4924d\narchitecture=x86\ncommand=make ARCH=x86 headers_install\n\n",
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
    let headers = sysroot.join("usr/include");
    let glibc_cflags = format!(
        "-O2 -g0 -ffile-prefix-map={}=/usr/src/mattos/glibc -fdebug-prefix-map={}=/usr/src/mattos/glibc",
        repo_root.display(),
        repo_root.display()
    );
    let configure_text = format!(
        "CFLAGS='{}' {} \\\n+  --prefix=/usr \\\n+  --libdir=/usr/lib/x86_64-linux-gnu \\\n+  --libexecdir=/usr/libexec \\\n+  --build=x86_64-pc-linux-gnu \\\n+  --host=x86_64-pc-linux-gnu \\\n+  --enable-kernel={} \\\n+  --with-headers={} \\\n+  --without-selinux \\\n+  --disable-werror \\\n+  --disable-profile \\\n+  --disable-build-nscd \\\n+  --disable-nscd \\\n+  --enable-stack-protector=strong \\\n+  --enable-bind-now\n",
        glibc_cflags,
        configure.display(),
        GLIBC_MINIMUM_KERNEL,
        headers.display()
    );
    fs::write(output.join("configure-invocation.txt"), &configure_text)?;
    fs::write(
        output.join("kernel-headers-source.txt"),
        "source=src/kernel/linux\nrevision=f17f39c917cd4aac09db1a6a083ef5ec09b4924d\nmethod=make ARCH=x86 headers_install\n",
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
        "--build=x86_64-pc-linux-gnu",
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
        ("libc_cv_slibdir", "/usr/lib/x86_64-linux-gnu".to_string()),
        ("libc_cv_rtlddir", "/lib64".to_string()),
    ];
    run_cmd_with_env_overrides(&build, configure_program, &configure_args, &configure_env)
        .context("glibc configure failed")?;

    let config_make = fs::read_to_string(build.join("config.make"))?;
    if !config_make.contains(&format!("sysheaders = {}", headers.display())) {
        bail!("glibc config.make does not select the controlled MattOS UAPI headers")
    }
    run_cmd_with_env_overrides(
        &build,
        "make",
        &["-j", "4"],
        &[
            ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
            ("LC_ALL", "C".to_string()),
            ("TZ", "UTC".to_string()),
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
    let configure_args = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--libexecdir=/usr/libexec",
        "--with-toolexeclibdir=/usr/lib/x86_64-linux-gnu",
        build_triplet.as_str(),
        host_triplet.as_str(),
        target_triplet.as_str(),
        sysroot_option.as_str(),
        build_sysroot_option.as_str(),
        "--enable-languages=c,c++",
        "--disable-bootstrap",
        "--disable-multilib",
        "--disable-nls",
        "--disable-werror",
        "--disable-checking",
        "--disable-analyzer",
        "--enable-shared",
        "--enable-threads=posix",
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
        "--without-isl",
        "--with-system-zlib",
    ];
    let prefix_map = format!(
        "-O2 -g0 -ffile-prefix-map={}=/usr/src/mattos/gcc -fdebug-prefix-map={}=/usr/src/mattos/gcc",
        repo_root.display(),
        repo_root.display()
    );
    let env = [
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("LC_ALL", "C".to_string()),
        ("TZ", "UTC".to_string()),
        ("CFLAGS_FOR_TARGET", prefix_map.clone()),
        ("CXXFLAGS_FOR_TARGET", prefix_map),
        ("LDFLAGS_FOR_TARGET", "-Wl,-z,relro -Wl,-z,now".to_string()),
    ];
    fs::write(
        output.join("configure-invocation.txt"),
        format!(
            "SOURCE_DATE_EPOCH={} LC_ALL=C TZ=UTC CFLAGS_FOR_TARGET='{}' CXXFLAGS_FOR_TARGET='{}' LDFLAGS_FOR_TARGET='-Wl,-z,relro -Wl,-z,now' {} {}\nmake all-target-libgcc all-target-libstdc++-v3\nmake DESTDIR={} install-target-libgcc install-target-libstdc++-v3\n",
            MATTOS_SOURCE_DATE_EPOCH,
            env[3].1,
            env[4].1,
            configure.display(),
            configure_args.join(" "),
            raw_install.display()
        ),
    )?;
    run_gcc_bootstrap_command(&build, &configure, &configure_args, &env)
        .context("GCC runtime configure failed")?;
    run_gcc_bootstrap_command(
        &build,
        Path::new("make"),
        &["all-target-libgcc", "all-target-libstdc++-v3"],
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
        ],
        &env,
    )
    .context("GCC runtime install failed")?;

    let libgcc = find_unique_file_named(&raw_install, "libgcc_s.so.1")?;
    let libstdcxx = find_unique_file_named(&raw_install, GCC_RUNTIME_LIBSTDCXX_ABI)?;
    fs::copy(&libgcc, runtime.join("libgcc_s.so.1"))?;
    fs::copy(&libstdcxx, runtime.join(GCC_RUNTIME_LIBSTDCXX_ABI))?;
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
    let sysroot_flag = format!("--sysroot={}", sysroot.display());
    let library_flag = format!("-L{}", runtime.display());
    let rpath_link = format!("-Wl,-rpath-link,{}", runtime.display());
    let validation_source_arg = path_str(&validation_source)?;
    let validation_binary_arg = path_str(&validation_binary)?;
    run_gcc_bootstrap_command(
        repo_root,
        Path::new("g++"),
        &[
            sysroot_flag.as_str(),
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

fn write_sysroot_compiler_wrappers(
    repo_root: &Path,
    directory: &Path,
    binutils: &Path,
) -> Result<(PathBuf, PathBuf)> {
    let sysroot = repo_root.join("out/sysroot");
    let map = format!(
        "-O2 -g0 -ffile-prefix-map={}=/usr/src/mattos -fdebug-prefix-map={}=/usr/src/mattos",
        repo_root.display(),
        repo_root.display()
    );
    let gcc = directory.join(format!("{TOOLCHAIN_TARGET}-gcc"));
    let gxx = directory.join(format!("{TOOLCHAIN_TARGET}-g++"));
    let target_lib = sysroot.join("usr/lib/x86_64-linux-gnu");
    let target_gcc = target_lib
        .join("gcc")
        .join(TOOLCHAIN_TARGET)
        .join(GCC_TOOLCHAIN_VERSION);
    let common = format!(
        "--sysroot={} -B{}/ -B{}/ -B{}/ -L{} {}",
        shell_escape(path_str(&sysroot)?),
        shell_escape(path_str(binutils)?),
        shell_escape(path_str(&target_gcc)?),
        shell_escape(path_str(&target_lib)?),
        shell_escape(path_str(&target_lib)?),
        map
    );
    write_executable_script(
        &gcc,
        &format!("#!/bin/sh\nexec /usr/bin/gcc {common} \"$@\"\n"),
    )?;
    write_executable_script(
        &gxx,
        &format!("#!/bin/sh\nexec /usr/bin/g++ {common} \"$@\"\n"),
    )?;
    Ok((gcc, gxx))
}

fn toolchain_environment(
    cc: &Path,
    cxx: &Path,
    binutils: &Path,
) -> Result<Vec<(&'static str, String)>> {
    let tool = |name: &str| path_str(&binutils.join(name)).map(str::to_string);
    let mut paths = vec![
        cc.parent()
            .context("toolchain compiler wrapper has no parent directory")?
            .to_path_buf(),
    ];
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
        ("CC", path_str(cc)?.to_string()),
        ("CXX", path_str(cxx)?.to_string()),
        ("AR", tool("ar")?),
        ("AS", tool("as")?),
        ("LD", tool("ld")?),
        ("NM", tool("nm")?),
        ("RANLIB", tool("ranlib")?),
        ("STRIP", tool("strip")?),
        ("CC_FOR_BUILD", "/usr/bin/gcc".to_string()),
        ("CXX_FOR_BUILD", "/usr/bin/g++".to_string()),
    ])
}

fn build_binutils(repo_root: &Path) -> Result<()> {
    let imported_source = repo_root.join("src/toolchain/binutils");
    let output = repo_root.join("out/build/binutils");
    let source = output.join("source");
    let cross_build = output.join("cross-build");
    let cross_install = output.join("cross-install");
    let native_build = output.join("native-build");
    let native_install = output.join("install");
    let wrapper_dir = output.join("bootstrap-bin");
    if !imported_source.join("configure").is_file() {
        bail!(
            "Binutils source is missing at {}",
            imported_source.display()
        )
    }
    if !repo_root.join("out/sysroot/usr/include/stdio.h").is_file() {
        bail!("Binutils requires the completed MattOS development sysroot")
    }
    let sysroff_info = ensure_binutils_sysroff_info(repo_root)?;
    remove_path_if_exists(&output)?;
    copy_imported_working_tree(repo_root, Path::new("src/toolchain/binutils"), &source)?;
    fs::copy(&sysroff_info, source.join("binutils/sysroff.info")).with_context(|| {
        format!(
            "failed to stage {} into output-owned Binutils source mirror",
            sysroff_info.display()
        )
    })?;
    for directory in [&cross_build, &cross_install, &native_build, &native_install] {
        fs::create_dir_all(directory)?;
    }

    let configure = source.join("configure");
    let sysroot = repo_root.join("out/sysroot");
    let sysroot_arg = format!("--with-sysroot={}", sysroot.display());
    let cross_prefix = format!("--prefix={}", cross_install.join("usr").display());
    let cross_args = [
        cross_prefix.as_str(),
        "--build=x86_64-pc-linux-gnu",
        "--host=x86_64-pc-linux-gnu",
        "--target=x86_64-pc-linux-gnu",
        sysroot_arg.as_str(),
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
    let reproducible_env = [
        ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
        ("LC_ALL", "C".to_string()),
        ("TZ", "UTC".to_string()),
        ("CFLAGS", "-O2 -g0".to_string()),
        ("CXXFLAGS", "-O2 -g0".to_string()),
    ];
    run_gcc_bootstrap_command(&cross_build, &configure, &cross_args, &reproducible_env)
        .context("Binutils bootstrap configure failed")?;
    run_gcc_bootstrap_command(
        &cross_build,
        Path::new("make"),
        &["-j", "4", "all-binutils", "all-gas", "all-ld"],
        &reproducible_env,
    )
    .context("Binutils bootstrap build failed")?;
    run_gcc_bootstrap_command(
        &cross_build,
        Path::new("make"),
        &["install-binutils", "install-gas", "install-ld"],
        &reproducible_env,
    )?;

    let cross_bin = cross_install.join("usr/bin");
    let (cc, cxx) = write_sysroot_compiler_wrappers(repo_root, &wrapper_dir, &cross_bin)?;
    let native_env = toolchain_environment(&cc, &cxx, &cross_bin)?;
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
            "bootstrap: {} {}\nnative: CC={} CXX={} {} {}\n",
            configure.display(),
            cross_args.join(" "),
            cc.display(),
            cxx.display(),
            configure.display(),
            native_args.join(" ")
        ),
    )?;
    println!("built source-native Binutils for {TOOLCHAIN_TARGET}");
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
    configure_extra: &[String],
    env: &[(&str, String)],
) -> Result<()> {
    fs::create_dir_all(build)?;
    let prefix = format!("--prefix={}", install.display());
    let mut owned_args = vec![
        prefix,
        format!("--build={TOOLCHAIN_BUILD}"),
        format!("--host={TOOLCHAIN_TARGET}"),
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
    let binutils = repo_root.join("out/build/binutils/cross-install/usr/bin");
    if !repo_root.join("src/toolchain/gcc/configure").is_file() {
        bail!("GCC source is missing; import the pinned GCC component first")
    }
    if !binutils.join("as").is_file() || !binutils.join("ld").is_file() {
        bail!("GCC toolchain build requires the Binutils bootstrap tools")
    }
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
    let wrappers = output.join("bootstrap-bin");
    let (cc, cxx) = write_sysroot_compiler_wrappers(repo_root, &wrappers, &binutils)?;
    let env = toolchain_environment(&cc, &cxx, &binutils)?;
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
        &[],
        &env,
    )?;
    let prereq_with_gmp = format!("--with-gmp={}", prereq_install.display());
    build_static_prerequisite(
        &mpfr_source,
        &output.join("prerequisite-build/mpfr"),
        &prereq_install,
        std::slice::from_ref(&prereq_with_gmp),
        &env,
    )?;
    let prereq_with_mpfr = format!("--with-mpfr={}", prereq_install.display());
    build_static_prerequisite(
        &mpc_source,
        &output.join("prerequisite-build/mpc"),
        &prereq_install,
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
    let configure_args = [
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
        "--without-isl",
        "--without-zstd",
        "--enable-languages=c,c++",
        "--enable-default-pie",
        "--disable-bootstrap",
        "--disable-multilib",
        "--disable-nls",
        "--disable-werror",
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
    let mut gcc_env = env.clone();
    // GCC feeds the selected linker command into `checksum-options`, which is
    // then hashed into cc1/cc1plus for PCH compatibility.  An absolute wrapper
    // path therefore makes otherwise identical compilers checkout-dependent.
    // The wrapper directory is already first in PATH, so use stable basenames
    // for the compiler proper while retaining absolute paths for prerequisite
    // builds that execute from several different working directories.
    let cc_name = cc
        .file_name()
        .and_then(OsStr::to_str)
        .context("GCC bootstrap C wrapper has no UTF-8 basename")?
        .to_string();
    let cxx_name = cxx
        .file_name()
        .and_then(OsStr::to_str)
        .context("GCC bootstrap C++ wrapper has no UTF-8 basename")?
        .to_string();
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
            "CC={} CXX={} CC_FOR_BUILD=/usr/bin/gcc CXX_FOR_BUILD=/usr/bin/g++ {} {}\nmake all-gcc\nmake DESTDIR={} install-gcc\n",
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
    let binutils = repo_root.join("out/build/binutils/cross-install/usr/bin");
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
    let wrappers = output.join("bootstrap-bin");
    let (cc, cxx) = write_sysroot_compiler_wrappers(repo_root, &wrappers, &binutils)?;
    let mut env = toolchain_environment(&cc, &cxx, &binutils)?;
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

