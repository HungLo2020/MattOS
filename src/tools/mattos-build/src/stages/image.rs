fn build_rootfs(repo_root: &Path) -> Result<()> {
    // Package/repository manifests are resolved before the rootfs key so a
    // package change cannot be hidden behind an old rootfs manifest.
    packaging::build_all_packages(repo_root)?;
    packaging::generate_repository(repo_root)?;
    let spec = build_stage_spec(BuildStage::Rootfs);
    performance::execute_cached_stage(
        repo_root,
        &spec,
        || validate_cached_rootfs(repo_root),
        || build_rootfs_atomic(repo_root),
    )
}

const BOOT_CRITICAL_MODULES: &[&str] = &[
    "nvme",
    "ahci",
    "sd_mod",
    "sr_mod",
    // VirtIO device modules do not declare their PCI transport in modules.dep;
    // load the transport explicitly before probing block and SCSI devices.
    "virtio_pci",
    "virtio_blk",
    "virtio_scsi",
    "usb_storage",
    "uas",
    "xhci_pci",
    "btrfs",
    "ext4",
];

fn module_basename(path: &str) -> Option<String> {
    let name = Path::new(path).file_name()?.to_str()?;
    for suffix in [".ko.zst", ".ko.xz", ".ko.gz", ".ko"] {
        if let Some(stem) = name.strip_suffix(suffix) {
            return Some(stem.replace('-', "_"));
        }
    }
    None
}

fn add_module_with_dependencies(
    path: &str,
    dependencies: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
    ordered: &mut Vec<String>,
) -> Result<()> {
    if ordered.iter().any(|existing| existing == path) {
        return Ok(());
    }
    if !visiting.insert(path.to_owned()) {
        bail!("cycle in kernel modules.dep at {path}");
    }
    for dependency in dependencies
        .get(path)
        .with_context(|| format!("module {path} absent from modules.dep"))?
    {
        add_module_with_dependencies(dependency, dependencies, visiting, ordered)?;
    }
    visiting.remove(path);
    ordered.push(path.to_owned());
    Ok(())
}

fn module_firmware_requirements(
    module_root: &Path,
    modules: &[String],
) -> Result<BTreeSet<String>> {
    let mut firmware = BTreeSet::new();
    for relative in modules {
        let module = module_root.join(relative);
        let output = Command::new("modinfo")
            .args(["-F", "firmware"])
            .arg(&module)
            .output()
            .with_context(|| {
                format!(
                    "failed to inspect firmware metadata for {}",
                    module.display()
                )
            })?;
        if !output.status.success() {
            bail!(
                "modinfo failed for boot-critical module {}: {}",
                module.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            )
        }
        for requirement in String::from_utf8(output.stdout)
            .context("module firmware metadata was not UTF-8")?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            firmware.insert(requirement.to_owned());
        }
    }
    Ok(firmware)
}

fn stage_boot_module_closure(repo_root: &Path, tree: &Path) -> Result<(String, usize, usize)> {
    let release = fs::read_to_string(repo_root.join("out/build/linux/kernel-release"))?
        .trim()
        .to_owned();
    let module_root = repo_root
        .join("out/build/linux/modules/usr/lib/modules")
        .join(&release);
    let mut dependencies = BTreeMap::<String, Vec<String>>::new();
    for line in fs::read_to_string(module_root.join("modules.dep"))?.lines() {
        let (module, dependency_list) = line
            .split_once(':')
            .with_context(|| format!("invalid modules.dep line {line:?}"))?;
        dependencies.insert(
            module.to_owned(),
            dependency_list
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
        );
    }
    let by_name = dependencies
        .keys()
        .filter_map(|path| module_basename(path).map(|name| (name, path.clone())))
        .collect::<BTreeMap<_, _>>();
    let mut ordered = Vec::new();
    let mut visiting = BTreeSet::new();
    for required in BOOT_CRITICAL_MODULES {
        let path = by_name
            .get(*required)
            .with_context(|| format!("boot-critical kernel module {required} was not built"))?;
        add_module_with_dependencies(path, &dependencies, &mut visiting, &mut ordered)?;
    }
    let destination_root = tree.join("usr/lib/modules").join(&release);
    for relative in &ordered {
        let destination = destination_root.join(relative);
        fs::create_dir_all(destination.parent().expect("module has parent"))?;
        fs::copy(module_root.join(relative), &destination)?;
    }
    let firmware_requirements = module_firmware_requirements(&module_root, &ordered)?;
    let firmware_source = repo_root.join("src/system/data/linux-firmware");
    for requirement in &firmware_requirements {
        if requirement
            .chars()
            .any(|character| matches!(character, '*' | '?' | '['))
        {
            bail!("boot-critical module uses unsupported firmware glob {requirement}")
        }
        let source = firmware_source.join(requirement);
        if !source.is_file() {
            bail!("boot-critical firmware {requirement} is absent from pinned linux-firmware")
        }
        let destination = tree.join("usr/lib/firmware").join(requirement);
        fs::create_dir_all(destination.parent().expect("firmware has parent"))?;
        fs::copy(&source, &destination)?;
    }
    let list = ordered
        .iter()
        .map(|path| format!("/usr/lib/modules/{release}/{path}\n"))
        .collect::<String>();
    fs::write(tree.join("modules.load"), list)?;
    Ok((release, ordered.len(), firmware_requirements.len()))
}

fn build_installer(repo_root: &Path) -> Result<()> {
    let btrfs_root = repo_root.join("out/build/btrfs-progs");
    let btrfs_source = btrfs_root.join("source");
    let btrfs_install = btrfs_root.join("install");
    sync_build_source(
        &repo_root.join("src/system/storage/btrfs-progs"),
        &btrfs_source,
    )?;
    if !btrfs_source.join("configure").is_file() {
        run_cmd(&btrfs_source, "autoreconf", &["-fiv"])?;
    }
    let btrfs_env = staged_library_environment(repo_root, &["util-linux", "zlib", "zstd"])?;
    if !btrfs_source.join("config.status").is_file() {
        run_cmd_with_env_overrides(
            &btrfs_source,
            "./configure",
            &[
                "--prefix=/usr",
                "--bindir=/usr/bin",
                "--libdir=/usr/lib/x86_64-linux-gnu",
                "--disable-documentation",
                "--disable-python",
                "--disable-convert",
                "--disable-zoned",
                "--disable-lzo",
                "--disable-libudev",
                "--disable-backtrace",
            ],
            &btrfs_env,
        )?;
    }
    run_cmd_with_env_overrides(&btrfs_source, "make", &[], &btrfs_env)?;
    remove_path_if_exists(&btrfs_install)?;
    run_cmd_with_env_overrides(
        &btrfs_source,
        "make",
        &["install", &format!("DESTDIR={}", btrfs_install.display())],
        &btrfs_env,
    )?;
    for required in ["usr/bin/btrfs", "usr/bin/mkfs.btrfs"] {
        if !btrfs_install.join(required).is_file() {
            bail!("Btrfs installer build did not produce {required}");
        }
    }
    let dosfs_root = repo_root.join("out/build/dosfstools");
    let dosfs_source = dosfs_root.join("source");
    let dosfs_build = dosfs_root.join("build");
    let dosfs_install = dosfs_root.join("install");
    sync_build_source(
        &repo_root.join("src/system/storage/dosfstools"),
        &dosfs_source,
    )?;
    if !dosfs_source.join("configure").is_file() || !dosfs_source.join("config.rpath").is_file() {
        run_cmd(&dosfs_source, "./autogen.sh", &[])?;
        remove_path_if_exists(&dosfs_build)?;
    }
    fs::create_dir_all(&dosfs_build)?;
    if !dosfs_build.join("Makefile").is_file() {
        run_cmd(
            &dosfs_build,
            path_str(&dosfs_source.join("configure"))?,
            &["--prefix=/usr", "--sbindir=/usr/sbin"],
        )?;
    }
    run_cmd(&dosfs_build, "make", &[])?;
    remove_path_if_exists(&dosfs_install)?;
    run_cmd(
        &dosfs_build,
        "make",
        &["install", &format!("DESTDIR={}", dosfs_install.display())],
    )?;
    if !dosfs_install.join("usr/sbin/mkfs.fat").is_file() {
        bail!("dosfstools installer build did not produce usr/sbin/mkfs.fat");
    }

    let e2fs_root = repo_root.join("out/build/e2fsprogs");
    let e2fs_source = e2fs_root.join("source");
    let e2fs_build = e2fs_root.join("build");
    let e2fs_install = e2fs_root.join("install");
    sync_build_source(
        &repo_root.join("src/system/storage/e2fsprogs"),
        &e2fs_source,
    )?;
    remove_path_if_exists(&e2fs_build)?;
    fs::create_dir_all(&e2fs_build)?;
    let e2fs_env = staged_library_environment(repo_root, &["util-linux"])?;
    if !e2fs_build.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &e2fs_build,
            path_str(&e2fs_source.join("configure"))?,
            &[
                "--prefix=/usr",
                "--sbindir=/usr/sbin",
                "--libdir=/usr/lib/x86_64-linux-gnu",
                "--sysconfdir=/etc",
                "--disable-nls",
                "--disable-uuidd",
                "--disable-fuse2fs",
                "--disable-fsck",
            ],
            &e2fs_env,
        )?;
    }
    run_cmd_with_env_overrides(&e2fs_build, "make", &[], &e2fs_env)?;
    remove_path_if_exists(&e2fs_install)?;
    run_cmd_with_env_overrides(
        &e2fs_build,
        "make",
        &["install", &format!("DESTDIR={}", e2fs_install.display())],
        &e2fs_env,
    )?;
    if !e2fs_install.join("usr/sbin/mkfs.ext4").is_file() {
        bail!("e2fsprogs installer build did not produce usr/sbin/mkfs.ext4");
    }
    let util_linux_lib = repo_root.join("out/build/util-linux/install/usr/lib/x86_64-linux-gnu");
    validate_dependency_resolves_from(
        &e2fs_install.join("usr/sbin/mkfs.ext4"),
        "libblkid.so.1",
        &util_linux_lib,
        &[&util_linux_lib],
    )?;
    validate_dependency_resolves_from(
        &e2fs_install.join("usr/sbin/mkfs.ext4"),
        "libuuid.so.1",
        &util_linux_lib,
        &[&util_linux_lib],
    )?;

    let installer_out = repo_root.join("out/build/installer");
    let cargo_target = installer_out.join("cargo-target");
    fs::create_dir_all(&installer_out)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "src/system/installer/Cargo.toml",
        ],
        &[("CARGO_TARGET_DIR", cargo_target.display().to_string())],
    )?;

    build_cosmic_installer_frontend(repo_root, &installer_out)?;

    let source = repo_root.join("src/system/installer/engine/installed-init.c");
    let compiler = repo_root.join("out/build/gcc-toolchain/install/usr/bin/gcc");
    let sysroot = repo_root.join("out/sysroot");
    let init_tree = performance::temporary_sibling(
        &repo_root.join("out/build/installed-initramfs-root"),
        "building",
    )?;
    fs::create_dir_all(&init_tree)?;
    let init = init_tree.join("init");
    let sysroot_arg = format!("--sysroot={}", sysroot.display());
    let libc_search = format!("-B{}/usr/lib/x86_64-linux-gnu/", sysroot.display());
    let gcc_search = format!(
        "-B{}/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0/",
        sysroot.display()
    );
    let libc_link = format!("-L{}/usr/lib/x86_64-linux-gnu", sysroot.display());
    let gcc_link = format!(
        "-L{}/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0",
        sysroot.display()
    );
    run_cmd(
        repo_root,
        path_str(&compiler)?,
        &[
            &sysroot_arg,
            &libc_search,
            &gcc_search,
            &libc_link,
            &gcc_link,
            "-std=c11",
            "-Os",
            "-static",
            "-s",
            "-fno-ident",
            "-Wl,--build-id=none",
            "-Wall",
            "-Wextra",
            "-Werror",
            path_str(&source)?,
            "-o",
            path_str(&init)?,
        ],
    )?;
    set_mode(init, 0o755)?;
    let (installed_module_release, installed_module_count, installed_firmware_count) =
        stage_boot_module_closure(repo_root, &init_tree)?;
    let installed_initramfs = repo_root.join("out/build/installed-initramfs.cpio.xz");
    let archive_command = format!(
        "find . -exec touch -h -d @{MATTOS_SOURCE_DATE_EPOCH} {{}} + && find . -print0 | sort -z | cpio --null -o --quiet --reproducible --owner=0:0 --format=newc | xz -1 -T1 --check=crc32 --stdout > {}",
        shell_escape(path_str(&installed_initramfs)?)
    );
    run_cmd(&init_tree, "bash", &["-lc", &archive_command])?;
    println!(
        "installed initramfs: {installed_module_count} boot-critical modules and {installed_firmware_count} required firmware files for {installed_module_release}"
    );
    remove_path_if_exists(&init_tree)?;

    let efi = installer_out.join("BOOTX64.EFI");
    run_cmd(
        repo_root,
        "grub-mkimage",
        &[
            "-O",
            "x86_64-efi",
            "-d",
            "/usr/lib/grub/x86_64-efi",
            "-p",
            "/EFI/BOOT",
            "-o",
            path_str(&efi)?,
            "part_gpt",
            "fat",
            "btrfs",
            "normal",
            "configfile",
            "search",
            "search_fs_uuid",
            "linux",
            "serial",
            "terminal",
        ],
    )?;
    if fs::metadata(&efi)?.len() < 128 * 1024 {
        bail!("generated installed-system EFI GRUB image is unexpectedly small");
    }
    Ok(())
}

fn build_cosmic_installer_frontend(repo_root: &Path, installer_out: &Path) -> Result<()> {
    let source_root = installer_out.join("cosmic-source");
    // This is an output-owned assembly mirror, not a cache. Recreate it so a
    // dependency demoted from first-class source cannot survive as stale
    // apparent vendored input. The separate cosmic-target retains Cargo's
    // incremental build products.
    remove_path_if_exists(&source_root)?;
    fs::create_dir_all(&source_root)?;
    let libcosmic = source_root.join("libcosmic");
    let iced = libcosmic.join("iced");
    let protocols = source_root.join("cosmic-protocols");
    let application = source_root.join("mattos-installer-cosmic");

    sync_build_source(&repo_root.join("src/desktop/cosmic/libcosmic"), &libcosmic)?;
    sync_build_source(&repo_root.join("src/desktop/cosmic/iced"), &iced)?;
    sync_build_source(
        &repo_root.join("src/desktop/cosmic/cosmic-protocols"),
        &protocols,
    )?;
    remove_path_if_exists(&application)?;
    fs::create_dir_all(application.join("src"))?;
    fs::copy(
        repo_root.join("src/system/installer/gui/cosmic/main.rs"),
        application.join("src/main.rs"),
    )?;
    let lock = repo_root.join("src/system/installer/gui/cosmic/Cargo.lock");
    validate_cosmic_installer_lock(&lock)?;
    fs::copy(&lock, application.join("Cargo.lock"))?;

    let template =
        fs::read_to_string(repo_root.join("src/system/installer/gui/cosmic/Cargo.toml.in"))?;
    let installer_manifest = repo_root.join("src/system/installer").canonicalize()?;
    let mut manifest = template
        .replace("@MATTOS_INSTALLER_PATH@", path_str(&installer_manifest)?)
        .replace("@LIBCOSMIC_PATH@", path_str(&libcosmic.canonicalize()?)?);
    manifest.push_str(&format!(
        "\n[patch.\"https://github.com/pop-os/cosmic-protocols\"]\ncosmic-client-toolkit = {{ path = {:?} }}\ncosmic-protocols = {{ path = {:?} }}\n",
        protocols.join("client-toolkit"), protocols
    ));
    fs::write(application.join("Cargo.toml"), manifest)?;

    let target = installer_out.join("cosmic-target");
    let xkbcommon = repo_root.join("out/build/xkbcommon/install/usr");
    let xkbcommon_lib = xkbcommon.join("lib/x86_64-linux-gnu");
    let xkbcommon_pc = xkbcommon_lib.join("pkgconfig");
    if !xkbcommon_lib.join("libxkbcommon.so.0").is_file()
        || !xkbcommon_pc.join("xkbcommon.pc").is_file()
    {
        bail!(
            "MattOS-built xkbcommon runtime/development metadata is missing; run build xkbcommon first"
        );
    }
    run_cmd_with_env_overrides(
        &application,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "Cargo.toml",
        ],
        &[
            ("CARGO_TARGET_DIR", target.display().to_string()),
            ("PKG_CONFIG_PATH", xkbcommon_pc.display().to_string()),
            ("PKG_CONFIG_LIBDIR", xkbcommon_pc.display().to_string()),
            // The .pc file has prefix=/usr.  Its sysroot is the DESTDIR root,
            // not `/usr`, otherwise pkg-config invents `/usr/usr/lib` and
            // Cargo silently falls back to a host xkbcommon.
            (
                "PKG_CONFIG_SYSROOT_DIR",
                xkbcommon
                    .parent()
                    .expect("xkbcommon install root")
                    .display()
                    .to_string(),
            ),
            ("LIBRARY_PATH", xkbcommon_lib.display().to_string()),
            ("LD_LIBRARY_PATH", xkbcommon_lib.display().to_string()),
        ],
    )?;
    let binary = target.join("release/mattos-install-cosmic");
    if !binary.is_file() {
        bail!(
            "native COSMIC installer build did not produce {}",
            binary.display()
        );
    }
    validate_dependency_resolves_from(
        &binary,
        "libxkbcommon.so.0",
        &xkbcommon_lib,
        &[&xkbcommon_lib],
    )?;
    Ok(())
}

const COSMIC_INSTALLER_LOCKED_GIT_SOURCES: &[&str] = &[
    "git+https://github.com/iced-rs/cryoglyph.git?rev=e429a025df36ab8145708acb309080ae3deec17a#e429a025df36ab8145708acb309080ae3deec17a",
    "git+https://github.com/jackpot51/rust-atomicwrites#043ab4859d53ffd3d55334685303d8df39c9f768",
    "git+https://github.com/pop-os/dbus-settings-bindings#eed01dd3609e90e3c8cd043656734c500956c793",
    "git+https://github.com/pop-os/freedesktop-icons#ab4c57b8e416c6af9297cb04d101889896fd9a92",
    "git+https://github.com/pop-os/smithay-clipboard?tag=sctk-0.20#859b02c88f45c554049a67c6ddeec1692ce0e20b",
    "git+https://github.com/pop-os/softbuffer?tag=cosmic-4.0#c2b2c19ddb38ff17495643699f97cb1f2064a1be",
    "git+https://github.com/pop-os/window_clipboard.git?tag=sctk-0.20#f68595ee0e62fbd6589f4709b5aaa5c3c7ea5f6c",
    "git+https://github.com/pop-os/winit.git?tag=cosmic-0.14#71ce08c043814514a8fd92d9d0599f115ae854e8",
    "git+https://github.com/wash2/accesskit?tag=cosmic-0.14#f0599eed5f18111228266fe3f28991cc48b5964f",
];

fn validate_cosmic_installer_lock(path: &Path) -> Result<()> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read native COSMIC lock {}", path.display()))?;
    let document: toml::Value = toml::from_str(&contents)
        .with_context(|| format!("failed to parse native COSMIC lock {}", path.display()))?;
    let packages = document
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| anyhow!("native COSMIC lock has no package records"))?;
    let mut git_sources = BTreeSet::new();
    for package in packages {
        let name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .unwrap_or("<unnamed>");
        let Some(source) = package.get("source").and_then(toml::Value::as_str) else {
            continue;
        };
        if source.starts_with("registry+") {
            let checksum = package
                .get("checksum")
                .and_then(toml::Value::as_str)
                .unwrap_or("");
            if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                bail!("native COSMIC registry package {name} lacks a SHA-256 checksum");
            }
        } else if source.starts_with("git+") {
            let revision = source
                .rsplit_once('#')
                .map(|(_, revision)| revision)
                .unwrap_or("");
            if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                bail!(
                    "native COSMIC Git package {name} is not pinned to an exact commit: {source}"
                );
            }
            git_sources.insert(source.to_string());
        }
    }
    let expected = COSMIC_INSTALLER_LOCKED_GIT_SOURCES
        .iter()
        .map(|source| (*source).to_string())
        .collect::<BTreeSet<_>>();
    if git_sources != expected {
        bail!(
            "native COSMIC Git source set differs from the reviewed lock policy\nexpected: {expected:#?}\nactual: {git_sources:#?}"
        );
    }
    Ok(())
}

fn build_rootfs_atomic(repo_root: &Path) -> Result<()> {
    let destination = repo_root.join("out/build/rootfs");
    let temp = performance::temporary_sibling(&destination, "building")?;
    let result = build_rootfs_into(repo_root, &temp);
    if let Err(error) = result {
        let _ = remove_path_if_exists(&temp);
        return Err(error);
    }
    validate_rootfs_mutable_state(&temp)?;
    validate_udev_storage_identity_support(&temp)?;
    packaging::validate_udev_hwdb_payload(repo_root, &temp)?;
    performance::atomic_replace_path(&temp, &destination)
}

fn validate_cached_rootfs(repo_root: &Path) -> Result<()> {
    let rootfs = repo_root.join("out/build/rootfs");
    validate_rootfs_mutable_state(&rootfs)?;
    validate_live_desktop_boot_contract(&rootfs)?;
    validate_udev_storage_identity_support(&rootfs)?;
    packaging::validate_udev_hwdb_payload(repo_root, &rootfs)?;
    for rel in [
        "var/lib/dpkg/status",
        "usr/share/mattos/repository/dists/trixie/Release",
        "usr/bin/sh",
        "usr/bin/bash",
        "usr/lib/systemd/systemd",
    ] {
        if !rootfs.join(rel).symlink_metadata().is_ok() {
            bail!("cached rootfs required path is missing: /{rel}");
        }
    }
    Ok(())
}

fn validate_udev_storage_identity_support(rootfs: &Path) -> Result<()> {
    let rules_path = rootfs.join("usr/lib/udev/rules.d/60-persistent-storage.rules");
    let rules = fs::read_to_string(&rules_path)
        .with_context(|| format!("failed to read {}", rules_path.display()))?;
    for required in [
        "IMPORT{builtin}=\"blkid\"",
        "disk/by-uuid/$env{ID_FS_UUID_ENC}",
        "disk/by-partuuid/$env{ID_PART_ENTRY_UUID}",
    ] {
        if !rules.contains(required) {
            bail!(
                "udev persistent-storage rules cannot materialize installed fstab identities: missing {required}"
            );
        }
    }
    let osc_profile = fs::read_to_string(rootfs.join("etc/profile.d/80-systemd-osc-context.sh"))?;
    if osc_profile.contains("PROMPT_COMMAND+=(") {
        bail!("systemd OSC profile contains Bash array syntax rejected by the MattOS login shell");
    }
    if !osc_profile.contains("command -v shopt >/dev/null 2>&1 || return 0") {
        bail!(
            "systemd OSC profile does not guard its Bash-only prompt setup by builtin availability"
        );
    }
    Ok(())
}

fn validate_rootfs_mutable_state(rootfs: &Path) -> Result<()> {
    for rel in [
        "run/dbus/system_bus_socket",
        "var/lib/dpkg/lock",
        "var/lib/dpkg/lock-frontend",
        "var/lib/apt/lists/lock",
        "var/cache/apt/archives/lock",
        "etc/udev/hwdb.bin",
    ] {
        if rootfs.join(rel).symlink_metadata().is_ok() {
            bail!("mutable lock/socket state is present in cached rootfs: /{rel}");
        }
    }
    Ok(())
}

fn validate_live_desktop_boot_contract(rootfs: &Path) -> Result<()> {
    for rel in [
        "usr/lib/systemd/system/mattos-live-graphical.target",
        "usr/lib/systemd/system/mattos.target",
        "usr/lib/systemd/system/graphical.target",
        "usr/lib/systemd/system/cosmic-greeter.service",
        "etc/systemd/system/display-manager.service",
        "etc/systemd/system/cosmic-greeter.service.d/live.conf",
        "etc/greetd/cosmic-live.toml",
        "etc/pam.d/cosmic-greeter",
        "usr/bin/greetd",
        "usr/bin/start-cosmic",
        "usr/bin/cosmic-session",
        "usr/bin/cosmic-panel",
        "usr/bin/cosmic-launcher",
        "usr/bin/cosmic-term",
        "home/mattos",
    ] {
        if !path_entry_exists(&rootfs.join(rel)) {
            bail!("graphical live boot contract is missing /{rel}")
        }
    }

    let graphical =
        fs::read_to_string(rootfs.join("usr/lib/systemd/system/mattos-live-graphical.target"))?;
    if !graphical.contains("Requires=graphical.target")
        || !graphical.contains("After=graphical.target")
    {
        bail!("graphical live target does not enter the production graphical target")
    }
    let cli = fs::read_to_string(rootfs.join("usr/lib/systemd/system/mattos.target"))?;
    if !cli.contains("Requires=multi-user.target") || cli.contains("graphical.target") {
        bail!("CLI live target must require only the non-graphical system target")
    }
    let live_config = fs::read_to_string(rootfs.join("etc/greetd/cosmic-live.toml"))?;
    for contract in [
        "[initial_session]",
        "command = \"/usr/bin/start-cosmic\"",
        "user = \"mattos\"",
        "[default_session]",
        "command = \"/usr/bin/cosmic-greeter-start\"",
    ] {
        if !live_config.contains(contract) {
            bail!("live greetd configuration is missing contract: {contract}")
        }
    }
    let override_unit =
        fs::read_to_string(rootfs.join("etc/systemd/system/cosmic-greeter.service.d/live.conf"))?;
    if !override_unit.contains("ExecStart=/usr/bin/greetd --config /etc/greetd/cosmic-live.toml") {
        bail!("live display-manager override does not select the live greetd configuration")
    }
    let pam = fs::read_to_string(rootfs.join("etc/pam.d/cosmic-greeter"))?;
    if pam
        .matches("session    optional     pam_systemd.so")
        .count()
        != 1
    {
        bail!("live COSMIC session lacks exactly one PAM/logind session hook")
    }
    let display_manager =
        fs::read_to_string(rootfs.join("usr/lib/systemd/system/cosmic-greeter.service"))?;
    if !display_manager.contains(
        "Wants=systemd-logind.service systemd-udev-trigger.service cosmic-greeter-daemon.service",
    ) {
        bail!("display manager does not pull in its greeter account service")
    }
    if path_entry_exists(
        &rootfs.join("etc/systemd/system/multi-user.target.wants/cosmic-greeter-daemon.service"),
    ) {
        bail!("CLI boot must not start the COSMIC greeter daemon through multi-user.target")
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(rootfs.join("home/mattos"))?
            .permissions()
            .mode()
            & 0o7777;
        if mode != 0o750 {
            bail!("live home has mode {mode:04o}; expected 0750")
        }
    }
    Ok(())
}

fn build_rootfs_into(repo_root: &Path, out: &Path) -> Result<()> {
    let skeleton = repo_root.join("src/rootfs/skeleton");
    fs::create_dir_all(out).with_context(|| format!("failed to create {}", out.display()))?;
    packaging::install_prototype_packages(repo_root, out)?;
    packaging::apply_live_apt_policy(repo_root, out)?;
    let release = fs::read_to_string(repo_root.join("out/build/linux/kernel-release"))?
        .trim()
        .to_owned();
    run_cmd(
        repo_root,
        "depmod",
        &["-b", path_str(out)?, "-m", "/usr/lib/modules", &release],
    )?;
    let aliases = fs::read_to_string(
        out.join("usr/lib/modules")
            .join(&release)
            .join("modules.alias"),
    )?;
    if !aliases.contains(" nvidia") || !aliases.contains(" nouveau") {
        bail!("rootfs depmod metadata does not preserve both NVIDIA and Nouveau aliases");
    }
    let package_owned = packaging::package_owned_paths(out)?;
    let package_snapshot = packaging::snapshot_package_files(out, &package_owned)?;
    for rel in LEGACY_SKELETON_FILES {
        packaging::reject_legacy_collision(&package_owned, Path::new(rel))?;
        let source = skeleton.join(rel);
        let destination = out.join(rel);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "failed to install legacy skeleton file {}",
                source.display()
            )
        })?;
    }
    set_mode(out.join("usr/libexec/mattos/brush-login"), 0o755)?;
    set_mode(out.join("usr/libexec/mattos/validate-shell-env"), 0o755)?;
    fs::create_dir_all(out.join("root")).context("failed to create /root in rootfs")?;
    set_mode(out.join("root"), 0o700)?;
    fs::create_dir_all(out.join("home")).context("failed to create /home in rootfs")?;
    fs::create_dir_all(out.join("run")).context("failed to create /run in rootfs")?;
    fs::create_dir_all(out.join("var/log")).context("failed to create /var/log in rootfs")?;
    fs::create_dir_all(out.join("var/tmp")).context("failed to create /var/tmp in rootfs")?;
    fs::create_dir_all(out.join("etc/systemd/system"))
        .context("failed to create /etc/systemd/system")?;
    fs::create_dir_all(out.join("usr/libexec/mattos"))
        .context("failed to create rescue init dir")?;
    fs::write(out.join("etc/machine-id"), "").context("failed to create /etc/machine-id")?;

    let systemd_install = repo_root.join("out/build/systemd/install");
    let systemd_pid1 = systemd_install.join("usr/lib/systemd/systemd");
    if !systemd_pid1.exists() {
        bail!(
            "systemd install output missing at {}; run build systemd first",
            systemd_pid1.display()
        );
    }
    copy_tree_excluding_package_owned(&systemd_install, &out, &package_owned)?;
    copy_systemd_runtime_dependencies(&out)?;
    generate_baseline_locale(repo_root, out)?;
    let pam_systemd = out.join(SYSTEMD_PAM_MODULE_REL);
    if !pam_systemd.is_file() {
        bail!(
            "systemd PAM module missing at {}; ensure the imported systemd build has PAM enabled",
            pam_systemd.display()
        );
    }
    copy_runtime_dependencies(&pam_systemd, &out)?;
    verify_required_pam_modules(&out)?;
    apply_live_profile(repo_root, &out)?;
    validate_account_database(&out)?;
    enforce_auth_file_modes(&out)?;
    validate_auth_file_modes(&out)?;
    install_mattos_system_units(repo_root, &out)?;
    install_network_configuration(repo_root, &out)?;

    let init_bin = repo_root.join("target/release/mattos-init");
    if !init_bin.exists() {
        bail!(
            "init binary missing at {}; run build init first",
            init_bin.display()
        );
    }

    let rescue_init = out.join("usr/libexec/mattos/rescue-init");
    fs::copy(&init_bin, &rescue_init).with_context(|| {
        format!(
            "failed to copy rescue init binary from {} into rootfs",
            init_bin.display()
        )
    })?;
    copy_runtime_dependencies(&rescue_init, &out)?;
    let mut inventory = UserlandInventory::default();
    inventory.add_implemented(UTIL_LINUX_PROVIDER, "agetty");
    inventory.add_implemented(UTIL_LINUX_PROVIDER, "login");
    inventory.add_implemented(UTIL_LINUX_PROVIDER, "su");
    inventory.add_compiled(UTIL_LINUX_PROVIDER, "agetty");
    inventory.add_compiled(UTIL_LINUX_PROVIDER, "login");
    inventory.add_compiled(UTIL_LINUX_PROVIDER, "su");
    inventory.add_installed(UTIL_LINUX_PROVIDER, "agetty");
    inventory.add_installed(UTIL_LINUX_PROVIDER, "login");
    inventory.add_installed(UTIL_LINUX_PROVIDER, "su");

    for module in [
        "libpam",
        "pam_unix",
        "pam_env",
        "pam_nologin",
        "pam_rootok",
        "pam_permit",
        "pam_deny",
        "pam_shells",
        "pam_securetty",
        "pam_systemd",
    ] {
        inventory.add_implemented(LINUX_PAM_PROVIDER, module);
        inventory.add_compiled(LINUX_PAM_PROVIDER, module);
        inventory.add_installed(LINUX_PAM_PROVIDER, module);
    }

    for cmd in [
        "passwd", "useradd", "usermod", "userdel", "groupadd", "groupmod", "groupdel", "chpasswd",
        "chage", "newgrp",
    ] {
        inventory.add_implemented(SHADOW_PROVIDER, cmd);
        inventory.add_compiled(SHADOW_PROVIDER, cmd);
        inventory.add_installed(SHADOW_PROVIDER, cmd);
    }
    inventory.add_implemented(SUDO_RS_PROVIDER, "sudo");
    inventory.add_compiled(SUDO_RS_PROVIDER, "sudo");
    inventory.add_installed(SUDO_RS_PROVIDER, "sudo");

    let brush_dst = out.join("usr/bin/brush");
    if !brush_dst.is_file() {
        bail!("mattos-brush package did not install /usr/bin/brush")
    }
    copy_runtime_dependencies(&brush_dst, &out)?;
    inventory.add_implemented("brush", "brush");
    inventory.add_compiled("brush", "brush");
    inventory.add_installed("brush", "brush");

    let coreutils_multicall = resolve_coreutils_multicall(repo_root)?;
    let coreutils_dst = out.join("usr/bin/coreutils");
    if !coreutils_dst.is_file() {
        bail!("coreutils package did not install /usr/bin/coreutils")
    }
    copy_runtime_dependencies(&coreutils_dst, &out)?;

    let coreutils_applets = list_coreutils_applets(&coreutils_multicall)?;
    for applet in &coreutils_applets {
        inventory.add_implemented(COREUTILS_PROVIDER, applet);
        inventory.add_compiled(COREUTILS_PROVIDER, applet);
    }
    let component_commands: BTreeSet<&str> = COMPONENT_INSTALL_MANIFESTS
        .iter()
        .flat_map(|manifest| manifest.binaries.iter().map(|binary| binary.command_name))
        .collect();
    let installed_coreutils_applets: Vec<String> = coreutils_applets
        .iter()
        .filter(|applet| !component_commands.contains(applet.as_str()))
        .cloned()
        .collect();
    for applet in &installed_coreutils_applets {
        if !path_entry_exists(&out.join("usr/bin").join(applet)) {
            bail!("coreutils package did not install alias /usr/bin/{applet}")
        }
        inventory.add_installed(COREUTILS_PROVIDER, applet);
    }
    for applet in coreutils_applets
        .iter()
        .filter(|applet| component_commands.contains(applet.as_str()))
    {
        inventory.add_excluded(COREUTILS_PROVIDER, applet);
    }

    for spec in USERLAND_BINARY_INSTALLS {
        install_userland_binary(repo_root, &out, spec)?;
        inventory.add_implemented(spec.provider, spec.command_name);
        inventory.add_compiled(spec.provider, spec.command_name);
        inventory.add_installed(spec.provider, spec.command_name);
    }

    create_command_aliases(&out, "diffutils", DIFFUTILS_AVAILABLE_ALIASES)?;
    for alias in DIFFUTILS_AVAILABLE_ALIASES {
        inventory.add_implemented(DIFFUTILS_PROVIDER, alias);
        inventory.add_installed(DIFFUTILS_PROVIDER, alias);
    }
    for expected in DIFFUTILS_EXPECTED_COMMANDS {
        if !DIFFUTILS_AVAILABLE_ALIASES.contains(expected) {
            inventory.add_failed(DIFFUTILS_PROVIDER, expected, "not implemented upstream");
        }
    }

    let component_provider_commands = install_component_manifests(repo_root, &out, &mut inventory)?;
    let curl_dst = out.join("usr/bin/curl");
    if !curl_dst.is_file() {
        bail!("curl package did not install /usr/bin/curl")
    }
    copy_runtime_dependencies(&curl_dst, &out)?;
    inventory.add_implemented(CURL_PROVIDER, "curl");
    inventory.add_compiled(CURL_PROVIDER, "curl");
    inventory.add_installed(CURL_PROVIDER, "curl");
    install_component_configuration(repo_root, &out)?;
    install_user_session_configuration(repo_root, &out)?;
    install_dbus_configuration(repo_root, &out)?;
    for command in [
        "busctl",
        "loginctl",
        "networkctl",
        "resolvectl",
        "timedatectl",
    ] {
        inventory.add_implemented(SYSTEMD_PROVIDER, command);
        inventory.add_compiled(SYSTEMD_PROVIDER, command);
        inventory.add_installed(SYSTEMD_PROVIDER, command);
    }

    let mut provider_commands = BTreeMap::<&str, Vec<String>>::new();
    provider_commands.insert(COREUTILS_PROVIDER, installed_coreutils_applets.clone());
    for spec in USERLAND_BINARY_INSTALLS {
        provider_commands
            .entry(spec.provider)
            .or_default()
            .push(spec.command_name.to_string());
    }
    provider_commands
        .entry(DIFFUTILS_PROVIDER)
        .or_default()
        .extend(DIFFUTILS_AVAILABLE_ALIASES.iter().map(|s| s.to_string()));
    for (provider, commands) in component_provider_commands {
        provider_commands.insert(provider, commands);
    }
    provider_commands.insert(CURL_PROVIDER, vec!["curl".to_string()]);
    provider_commands.insert(
        SYSTEMD_PROVIDER,
        vec![
            "busctl".to_string(),
            "loginctl".to_string(),
            "networkctl".to_string(),
            "resolvectl".to_string(),
            "timedatectl".to_string(),
        ],
    );
    validate_no_duplicate_commands(&provider_commands)?;

    for expected in [
        "grep",
        "sed",
        "find",
        "xargs",
        "diff",
        "cmp",
        "login",
        "su",
        "passwd",
        "sudo",
        "useradd",
        "usermod",
        "userdel",
        "groupadd",
        "groupmod",
        "groupdel",
        "chpasswd",
        "getent",
        "modprobe",
        "lsmod",
        "ps",
        "top",
        "free",
        "uptime",
        "pgrep",
        "pkill",
        "clear",
        "tput",
        "infocmp",
        "ip",
        "ss",
        "bridge",
        "tc",
        "ping",
        "tracepath",
        "curl",
        "sh",
        "bash",
        "dbus-broker",
        "dbus-broker-launch",
        "busctl",
        "loginctl",
        "networkctl",
        "resolvectl",
        "timedatectl",
    ] {
        let path = out.join("usr/bin").join(expected);
        let alt = out.join("usr/sbin").join(expected);
        if !path_entry_exists(&path) && !path_entry_exists(&alt) {
            bail!(
                "required command {} missing from rootfs at {}",
                expected,
                path.display()
            )
        }
    }

    inventory.add_installed("brush", "sh");
    inventory.add_installed("brush", "bash");
    inventory.add_excluded(DIFFUTILS_PROVIDER, "diff3");
    inventory.add_excluded(DIFFUTILS_PROVIDER, "sdiff");
    write_userland_inventory(&out, &inventory)?;
    validate_live_desktop_boot_contract(&out)?;
    packaging::embed_repository(repo_root, &out)?;
    packaging::validate_dpkg_database(&out)?;
    performance::timed(
        "rootfs-package-audit",
        "n/a",
        "validate package-owned files after rootfs overlays",
        "rootfs-package-snapshot",
        || packaging::validate_package_snapshot(&out, &package_snapshot),
    )?;
    performance::timed(
        "rootfs-elf-audit",
        "n/a",
        "validate complete MattOS glibc and ELF runtime closure",
        "rootfs-elf-inventory",
        || validate_glibc_rootfs(repo_root, &out),
    )?;

    Ok(())
}

fn validate_glibc_rootfs(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let expected_loader = "/lib64/ld-linux-x86-64.so.2";
    let loader = rootfs.join("usr/lib64/ld-linux-x86-64.so.2");
    let libc = rootfs.join("usr/lib/x86_64-linux-gnu/libc.so.6");
    let libm = rootfs.join("usr/lib/x86_64-linux-gnu/libm.so.6");
    for (installed, built) in [
        (
            &loader,
            repo_root.join("out/build/glibc/install/lib64/ld-linux-x86-64.so.2"),
        ),
        (
            &libc,
            repo_root.join("out/build/glibc/install/usr/lib/x86_64-linux-gnu/libc.so.6"),
        ),
        (
            &libm,
            repo_root.join("out/build/glibc/install/usr/lib/x86_64-linux-gnu/libm.so.6"),
        ),
    ] {
        if !installed.is_file() || fs::read(installed)? != fs::read(&built)? {
            bail!(
                "rootfs glibc artifact {} does not exactly match MattOS build output {}",
                installed.display(),
                built.display()
            )
        }
    }
    for (installed, built) in [
        (
            rootfs.join("usr/lib/x86_64-linux-gnu/libgcc_s.so.1"),
            repo_root.join("out/build/gcc-runtime/runtime/usr/lib/x86_64-linux-gnu/libgcc_s.so.1"),
        ),
        (
            rootfs.join("usr/lib/x86_64-linux-gnu/libstdc++.so.6"),
            repo_root.join("out/build/gcc-runtime/runtime/usr/lib/x86_64-linux-gnu/libstdc++.so.6"),
        ),
    ] {
        if !installed.is_file() || !built.is_file() || fs::read(&installed)? != fs::read(&built)? {
            bail!(
                "rootfs GCC runtime {} does not exactly match MattOS build output {}",
                installed.display(),
                built.display()
            )
        }
    }

    let mut files = Vec::new();
    collect_regular_files(rootfs, &mut files)?;
    let mut elf_files = Vec::new();
    let mut provided = BTreeSet::new();
    let mut soname_providers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in files {
        let relative = path.strip_prefix(rootfs)?;
        // Firmware may itself use ELF as a container for code executed by an
        // embedded GPU or device processor (for example NVIDIA GSP RISC-V).
        // It is data from the host CPU's perspective, not part of its dynamic
        // executable/library closure.
        if relative.starts_with("usr/lib/firmware") {
            continue;
        }
        // Flatpak's bundled OSTree repository contains application/runtime
        // objects, including ELF files and arbitrary data, beneath the host
        // rootfs. They are not MattOS host ELF providers and must not enter
        // the rootfs SONAME/dependency namespace.
        if relative.starts_with("var/lib/flatpak") {
            continue;
        }
        let Some(facts) = elf_cache::inspect(repo_root, &path)? else {
            continue;
        };
        if !facts.architecture.contains("X86-64") {
            bail!(
                "ELF object /{} has unexpected architecture {}",
                relative.display(),
                facts.architecture
            );
        }
        let bytes = fs::read(&path)?;
        let build_root = repo_root.to_string_lossy();
        if bytes
            .windows(build_root.len())
            .any(|window| window == build_root.as_bytes())
        {
            bail!(
                "ELF object /{} embeds the host build root {}",
                path.strip_prefix(rootfs)?.display(),
                repo_root.display()
            )
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            provided.insert(name.to_string());
        }
        if let Some(value) = &facts.soname {
            provided.insert(value.clone());
            soname_providers
                .entry(value.clone())
                .or_default()
                .push(format!("/{}", path.strip_prefix(rootfs)?.display()));
        }
        elf_files.push((path, facts));
    }
    provided.insert("linux-vdso.so.1".to_string());
    provided.insert("ld-linux-x86-64.so.2".to_string());
    for (soname, paths) in &soname_providers {
        if paths.len() > 1 {
            bail!("duplicate SONAME provider {soname}: {}", paths.join(", "))
        }
    }

    let mut package_owners = BTreeMap::new();
    let info = rootfs.join("var/lib/dpkg/info");
    if info.is_dir() {
        for entry in fs::read_dir(&info)? {
            let path = entry?.path();
            if path.extension().and_then(|part| part.to_str()) != Some("list") {
                continue;
            }
            let package = path
                .file_stem()
                .and_then(|part| part.to_str())
                .unwrap_or("unknown")
                .to_string();
            for installed in fs::read_to_string(&path)?.lines() {
                package_owners.insert(installed.to_string(), package.clone());
            }
        }
    }

    let library_path = std::env::join_paths([
        rootfs.join("usr/lib/x86_64-linux-gnu"),
        rootfs.join("usr/lib/x86_64-linux-gnu/systemd"),
        rootfs.join("usr/lib"),
    ])?;
    let mut rows = Vec::new();
    let mut gcc_runtime_consumers = Vec::new();
    let mut executable_count = 0usize;
    for (path, facts) in &elf_files {
        let relative = format!("/{}", path.strip_prefix(rootfs)?.display());
        let interpreter = facts.interpreter.clone();
        if let Some(actual) = &interpreter {
            executable_count += 1;
            if actual != expected_loader {
                bail!("ELF executable {relative} uses unexpected interpreter {actual}")
            }
            let listed = Command::new(&loader)
                .arg("--library-path")
                .arg(&library_path)
                .arg("--list")
                .arg(path)
                .output()
                .with_context(|| format!("failed to invoke MattOS loader for {relative}"))?;
            if !listed.status.success() {
                bail!(
                    "MattOS loader cannot resolve {relative}: {}",
                    String::from_utf8_lossy(&listed.stderr)
                )
            }
            let listing = String::from_utf8_lossy(&listed.stdout);
            if listing.contains("not found") {
                bail!("MattOS loader reports an unresolved library for {relative}: {listing}")
            }
            for line in listing.lines().filter(|line| line.contains("=>")) {
                let resolved = line
                    .split("=>")
                    .nth(1)
                    .and_then(|part| part.split_whitespace().next())
                    .unwrap_or_default();
                if resolved.starts_with('/') && !Path::new(resolved).starts_with(rootfs) {
                    bail!("{relative} resolves a runtime library from host path {resolved}")
                }
            }
        }

        let mut runtime_needs = Vec::new();
        for needed in &facts.needed {
            if !provided.contains(needed) {
                bail!(
                    "ELF object {relative} needs {needed}, which is absent from the MattOS rootfs"
                )
            }
            if needed == "libgcc_s.so.1" || needed == "libstdc++.so.6" {
                runtime_needs.push(needed.to_string());
            }
        }
        for value in facts.rpath.iter().chain(&facts.runpath) {
            if value.contains("/home/")
                || value.contains("/tmp/")
                || value.contains("/usr/local/")
                || value.contains("/opt/")
            {
                bail!(
                    "ELF object {relative} embeds a host-style absolute library search path: {value}"
                )
            }
        }

        let versions = |prefix: &str| {
            facts
                .symbol_versions
                .iter()
                .filter(|version| version.starts_with(prefix))
                .cloned()
                .collect::<BTreeSet<_>>()
        };
        let glibc_versions = versions("GLIBC_");
        let glibcxx_versions = versions("GLIBCXX_");
        let cxxabi_versions = versions("CXXABI_");
        let gcc_versions = versions("GCC_");
        let owner = package_owners
            .get(&relative)
            .cloned()
            .unwrap_or_else(|| "legacy-source-stage".to_string());
        let build_stage = if relative == "/usr/libexec/mattos/rescue-init" {
            "init"
        } else if relative.starts_with("/usr/lib/systemd/")
            || relative.contains("libnss_systemd")
            || relative.contains("libnss_resolve")
        {
            "systemd"
        } else {
            owner.trim_start_matches("mattos-")
        };
        let glibc_versions = glibc_versions.into_iter().collect::<Vec<_>>().join(",");
        let glibcxx_versions = glibcxx_versions.into_iter().collect::<Vec<_>>().join(",");
        let cxxabi_versions = cxxabi_versions.into_iter().collect::<Vec<_>>().join(",");
        let gcc_versions = gcc_versions.into_iter().collect::<Vec<_>>().join(",");
        for needed in runtime_needs {
            gcc_runtime_consumers.push(format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                relative, owner, needed, glibcxx_versions, cxxabi_versions, gcc_versions
            ));
        }
        rows.push(format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\tvalidated",
            relative,
            owner,
            build_stage,
            interpreter.as_deref().unwrap_or("-"),
            glibc_versions,
            glibcxx_versions,
            cxxabi_versions,
            gcc_versions
        ));
    }
    rows.sort();
    let reports = repo_root.join("out/reports");
    fs::create_dir_all(&reports)?;
    fs::write(
        reports.join("elf-runtime-inventory.tsv"),
        format!(
            "path\towner\tbuild_stage\tinterpreter\tglibc_versions\tglibcxx_versions\tcxxabi_versions\tgcc_versions\trebuild_status\n{}\n",
            rows.join("\n")
        ),
    )?;
    gcc_runtime_consumers.sort();
    fs::write(
        reports.join("gcc-runtime-consumers.tsv"),
        format!(
            "path\towner\tneeded_runtime\tglibcxx_versions\tcxxabi_versions\tgcc_versions\n{}\n",
            gcc_runtime_consumers.join("\n")
        ),
    )?;
    println!(
        "validated {} ELF objects ({} dynamic executables) with MattOS glibc",
        elf_files.len(),
        executable_count
    );
    Ok(())
}

fn collect_regular_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            collect_regular_files(&path, files)?;
        } else if metadata.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn install_component_manifests(
    repo_root: &Path,
    rootfs: &Path,
    inventory: &mut UserlandInventory,
) -> Result<BTreeMap<&'static str, Vec<String>>> {
    let mut providers = BTreeMap::new();

    for manifest in COMPONENT_INSTALL_MANIFESTS {
        if manifest.provider == CURL_PROVIDER {
            continue;
        }
        let install_root = repo_root.join(manifest.install_root_rel);
        let mut commands = Vec::new();
        for binary in manifest.binaries {
            let source = install_root.join(binary.source_rel);
            let destination = rootfs.join(binary.destination_rel);
            if !source.is_file() {
                bail!("component executable missing at {}", source.display());
            }
            if !destination.is_file() {
                bail!(
                    "package did not install required component executable /{}",
                    binary.destination_rel
                );
            }
            inventory.add_implemented(manifest.provider, binary.command_name);
            inventory.add_compiled(manifest.provider, binary.command_name);
            inventory.add_installed(manifest.provider, binary.command_name);
            commands.push(binary.command_name.to_string());
        }
        providers.insert(manifest.provider, commands);
    }

    Ok(providers)
}

#[cfg(test)]
fn inspect_and_stage_executable(
    source: &Path,
    destination: &Path,
    rootfs: &Path,
    install_roots: &[PathBuf],
    library_dirs: &[PathBuf],
) -> Result<()> {
    if !source.exists() {
        bail!("component executable missing at {}", source.display());
    }
    let file_output = Command::new("file")
        .arg("-L")
        .arg(source)
        .output()
        .with_context(|| format!("failed to inspect {} with file", source.display()))?;
    if !file_output.status.success() {
        bail!("file inspection failed for {}", source.display());
    }
    let file_text = String::from_utf8_lossy(&file_output.stdout);
    if !file_text.contains("ELF") {
        bail!(
            "expected an ELF executable, file reported: {}",
            file_text.trim()
        );
    }
    let readelf = Command::new("readelf")
        .args(["-d"])
        .arg(source)
        .output()
        .with_context(|| format!("failed to inspect {} with readelf", source.display()))?;
    if !readelf.status.success() {
        bail!("readelf inspection failed for {}", source.display());
    }

    let library_path = std::env::join_paths(library_dirs)
        .context("failed to construct component LD_LIBRARY_PATH")?;
    let ldd = Command::new("ldd")
        .arg(source)
        .env("LD_LIBRARY_PATH", library_path)
        .output()
        .with_context(|| format!("failed to inspect {} with ldd", source.display()))?;
    if !ldd.status.success() {
        bail!("ldd inspection failed for {}", source.display());
    }
    let ldd_text = String::from_utf8(ldd.stdout).context("ldd output was not UTF-8")?;
    if ldd_text.contains("not found") {
        bail!(
            "unresolved runtime dependency for {}:\n{}",
            source.display(),
            ldd_text
        );
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(source, destination)
        .with_context(|| format!("failed to stage {}", source.display()))?;
    for token in ldd_text
        .split_whitespace()
        .filter(|token| token.starts_with('/'))
    {
        let dependency = Path::new(token);
        if dependency.exists() {
            stage_resolved_dependency(dependency, rootfs, install_roots)?;
        }
    }
    println!("inspected and staged {}", destination.display());
    Ok(())
}

#[cfg(test)]
fn stage_resolved_dependency(
    source: &Path,
    rootfs: &Path,
    install_roots: &[PathBuf],
) -> Result<()> {
    let relative = install_roots
        .iter()
        .find_map(|root| source.strip_prefix(root).ok().map(Path::to_path_buf))
        .or_else(|| source.strip_prefix("/").ok().map(Path::to_path_buf))
        .ok_or_else(|| {
            anyhow!(
                "cannot map runtime dependency {} into rootfs",
                source.display()
            )
        })?;
    let destination = rootfs.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(source, &destination)
        .with_context(|| format!("failed to stage runtime dependency {}", source.display()))?;
    Ok(())
}

fn install_component_configuration(repo_root: &Path, rootfs: &Path) -> Result<()> {
    for directory in [
        "etc/depmod.d",
        "etc/modprobe.d",
        "etc/modules-load.d",
        "usr/lib/depmod.d",
        "usr/lib/modprobe.d",
        "usr/lib/modules-load.d",
        "etc/sysctl.d",
    ] {
        fs::create_dir_all(rootfs.join(directory))
            .with_context(|| format!("failed to create /{directory}"))?;
    }
    let sysctl_source = repo_root.join("src/userland/procps-ng/sysctl.conf");
    if fs::read(&sysctl_source)? != fs::read(rootfs.join("etc/sysctl.conf"))? {
        bail!("procps did not install the authoritative /etc/sysctl.conf");
    }

    let source_db = repo_root.join("out/build/ncurses/install/usr/share/terminfo");
    verify_terminfo_entries(&source_db)?;
    verify_terminfo_entries(&rootfs.join("usr/share/terminfo"))?;
    Ok(())
}

fn verify_terminfo_entries(database: &Path) -> Result<()> {
    for terminal in TERMINFO_ENTRIES {
        if terminfo_entry_path(database, terminal).is_none() {
            bail!(
                "terminfo database {} lacks required entry {terminal}",
                database.display()
            );
        }
    }
    Ok(())
}

fn terminfo_entry_path(database: &Path, terminal: &str) -> Option<PathBuf> {
    let first = terminal.as_bytes().first().copied()?;
    let candidates = [
        database.join(char::from(first).to_string()).join(terminal),
        database.join(format!("{first:x}")).join(terminal),
    ];
    candidates.into_iter().find(|path| path.exists())
}

fn install_mattos_system_units(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let units_src = repo_root.join("src/system/units");
    if !units_src.exists() {
        bail!(
            "MattOS systemd units missing at {}; expected MattOS-owned units",
            units_src.display()
        );
    }
    let units_dst = rootfs.join("usr/lib/systemd/system");
    fs::create_dir_all(&units_dst)
        .with_context(|| format!("failed to create {}", units_dst.display()))?;
    copy_tree_excluding_dotgit(&units_src, &units_dst)?;

    let default_target = rootfs.join("etc/systemd/system/default.target");
    if default_target.exists() {
        fs::remove_file(&default_target)
            .with_context(|| format!("failed to remove {}", default_target.display()))?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("/usr/lib/systemd/system/mattos.target", &default_target)
        .with_context(|| format!("failed to create {}", default_target.display()))?;

    let getty_wants = rootfs.join("etc/systemd/system/getty.target.wants");
    fs::create_dir_all(&getty_wants)
        .with_context(|| format!("failed to create {}", getty_wants.display()))?;
    let tty1_getty = getty_wants.join("getty@tty1.service");
    if tty1_getty.exists() {
        fs::remove_file(&tty1_getty)
            .with_context(|| format!("failed to remove {}", tty1_getty.display()))?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("/usr/lib/systemd/system/getty@.service", &tty1_getty)
        .with_context(|| format!("failed to create {}", tty1_getty.display()))?;

    for masked in ["ldconfig.service", "mattos-shell.service"] {
        let mask = rootfs.join("etc/systemd/system").join(masked);
        if mask.exists() {
            fs::remove_file(&mask)
                .with_context(|| format!("failed to remove {}", mask.display()))?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink("/dev/null", &mask)
            .with_context(|| format!("failed to create {}", mask.display()))?;
    }

    Ok(())
}

fn install_network_configuration(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let source = repo_root.join("src/system/network");
    if !source.join("network/20-mattos-wired.network").exists() {
        bail!(
            "MattOS network configuration missing at {}",
            source.display()
        );
    }
    // NetworkManager owns interface configuration. Keep the legacy networkd
    // source and unit available for recovery, but do not install an active
    // .network policy that can race NetworkManager.
    for (source_name, destination) in [
        ("resolved.conf", "etc/systemd/resolved.conf"),
        ("timesyncd.conf", "etc/systemd/timesyncd.conf"),
        ("nsswitch.conf", "etc/nsswitch.conf"),
        ("hosts", "etc/hosts"),
        ("networks", "etc/networks"),
        (
            "99-mattos-network.conf",
            "etc/sysctl.d/99-mattos-network.conf",
        ),
    ] {
        let target = rootfs.join(destination);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::copy(source.join(source_name), &target)
            .with_context(|| format!("failed to install network configuration {source_name}"))?;
    }

    fs::create_dir_all(rootfs.join("run/systemd/resolve"))
        .context("failed to create /run/systemd/resolve")?;
    let resolv_conf = rootfs.join("etc/resolv.conf");
    if path_entry_exists(&resolv_conf) {
        remove_path_if_exists(&resolv_conf)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("/run/systemd/resolve/stub-resolv.conf", &resolv_conf)
        .context("failed to create resolved-managed /etc/resolv.conf")?;

    let wants = rootfs.join("etc/systemd/system/multi-user.target.wants");
    fs::create_dir_all(&wants).with_context(|| format!("failed to create {}", wants.display()))?;
    for service in [
        "NetworkManager.service",
        "systemd-resolved.service",
        "systemd-timesyncd.service",
    ] {
        let unit = rootfs.join("usr/lib/systemd/system").join(service);
        if !unit.exists() {
            bail!("required networking unit missing at {}", unit.display());
        }
        let link = wants.join(service);
        if path_entry_exists(&link) {
            remove_path_if_exists(&link)?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(format!("/usr/lib/systemd/system/{service}"), &link)
            .with_context(|| format!("failed to enable {service}"))?;
    }

    let networkd_mask = rootfs.join("etc/systemd/system/systemd-networkd.service");
    if path_entry_exists(&networkd_mask) {
        remove_path_if_exists(&networkd_mask)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("/dev/null", &networkd_mask)
        .context("failed to mask systemd-networkd")?;

    validate_network_configuration(rootfs)
}

fn install_user_session_configuration(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let source = repo_root.join("src/system/session");
    let units_source = source.join("user-units");
    let dbus_config = source.join("dbus/session.conf");
    for required in [
        units_source.join("dbus.socket"),
        units_source.join("dbus-broker.service"),
        dbus_config.clone(),
    ] {
        if !required.is_file() {
            bail!("MattOS user-session unit missing at {}", required.display());
        }
    }

    let user_units = rootfs.join("usr/lib/systemd/user");
    fs::create_dir_all(&user_units)
        .with_context(|| format!("failed to create {}", user_units.display()))?;
    for rel in ["dbus.socket", "dbus-broker.service"] {
        if fs::read(units_source.join(rel))? != fs::read(user_units.join(rel))? {
            bail!("dbus-broker did not install authoritative user unit {rel}");
        }
    }
    for rel in ["dbus.socket", "dbus-broker.service"] {
        set_mode(user_units.join(rel), 0o644)?;
    }

    let dbus_alias = user_units.join("dbus.service");
    if path_entry_exists(&dbus_alias) {
        remove_path_if_exists(&dbus_alias)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("dbus-broker.service", &dbus_alias)
        .context("failed to create user dbus.service alias")?;

    let sockets_wants = user_units.join("sockets.target.wants");
    fs::create_dir_all(&sockets_wants)
        .with_context(|| format!("failed to create {}", sockets_wants.display()))?;
    let socket_link = sockets_wants.join("dbus.socket");
    if path_entry_exists(&socket_link) {
        remove_path_if_exists(&socket_link)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("../dbus.socket", &socket_link)
        .context("failed to enable the per-user D-Bus socket")?;

    for directory in [
        "usr/share/dbus-1/session.d",
        "usr/share/dbus-1/services",
        "etc/dbus-1/session.d",
    ] {
        fs::create_dir_all(rootfs.join(directory))
            .with_context(|| format!("failed to create /{directory}"))?;
    }
    if fs::read(&dbus_config)? != fs::read(rootfs.join("usr/share/dbus-1/session.conf"))? {
        bail!("dbus-broker did not install authoritative session bus policy");
    }
    set_mode(rootfs.join("usr/share/dbus-1/session.conf"), 0o644)?;

    // MattOS supplies a deliberately small effective systemd-user PAM stack in /etc.
    // Remove the imported vendor fallback, which references optional PAM modules that
    // are outside the current image's authentication scope.
    let vendor_systemd_user = rootfs.join("usr/lib/pam.d/systemd-user");
    if path_entry_exists(&vendor_systemd_user) {
        remove_path_if_exists(&vendor_systemd_user)?;
    }

    validate_user_session_configuration(rootfs)
}

fn validate_user_session_configuration(rootfs: &Path) -> Result<()> {
    for rel in [
        SYSTEMD_PAM_MODULE_REL,
        "etc/pam.d/login",
        "etc/pam.d/su-l",
        "etc/pam.d/systemd-user",
        "usr/lib/systemd/system/systemd-logind.service",
        "usr/lib/systemd/system/user@.service",
        "usr/lib/systemd/system/user-runtime-dir@.service",
        "usr/lib/systemd/systemd-user-runtime-dir",
        "usr/lib/systemd/user/basic.target",
        "usr/lib/systemd/user/default.target",
        "usr/lib/systemd/user/sockets.target",
        "usr/lib/systemd/user/dbus.socket",
        "usr/lib/systemd/user/dbus-broker.service",
        "usr/lib/systemd/user/dbus.service",
        "usr/lib/systemd/user/sockets.target.wants/dbus.socket",
        "usr/share/dbus-1/session.conf",
        "usr/share/dbus-1/session.d",
        "usr/share/dbus-1/services",
        "etc/dbus-1/session.d",
        "usr/lib/systemd/user-environment-generators/30-systemd-environment-d-generator",
    ] {
        if !path_entry_exists(&rootfs.join(rel)) {
            bail!("required user-session rootfs path missing: /{rel}");
        }
    }

    let expected_hook = "session    optional     pam_systemd.so";
    for stack in ["login", "su-l", "systemd-user", "sshd"] {
        let body = fs::read_to_string(rootfs.join("etc/pam.d").join(stack))
            .with_context(|| format!("failed to read effective PAM stack {stack}"))?;
        if body.matches(expected_hook).count() != 1 {
            bail!("PAM stack {stack} must contain exactly one optional pam_systemd session hook");
        }
    }
    let greeter_stack = rootfs.join("etc/pam.d/cosmic-greeter");
    if greeter_stack.is_file() {
        let body = fs::read_to_string(&greeter_stack)?;
        if body.matches(expected_hook).count() != 1 {
            bail!(
                "PAM stack cosmic-greeter must contain exactly one optional pam_systemd session hook"
            );
        }
    }
    if fs::read_to_string(rootfs.join("usr/share/pam/security/pam_env.conf"))?
        .trim()
        .is_empty()
    {
        bail!("source-built PAM environment defaults must not be empty");
    }
    for entry in fs::read_dir(rootfs.join("etc/pam.d"))? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
        if matches!(
            name,
            "login" | "su-l" | "systemd-user" | "sshd" | "cosmic-greeter"
        ) {
            continue;
        }
        if fs::read_to_string(&path)?.contains("pam_systemd.so") {
            bail!("pam_systemd is present in inappropriate PAM stack {name}");
        }
    }

    let user_socket = fs::read_to_string(rootfs.join("usr/lib/systemd/user/dbus.socket"))?;
    if user_socket.matches("ListenStream=%t/bus").count() != 1
        || user_socket.contains("/run/dbus/system_bus_socket")
    {
        bail!("user dbus.socket must own only the per-user %t/bus endpoint");
    }
    let user_broker = fs::read_to_string(rootfs.join("usr/lib/systemd/user/dbus-broker.service"))?;
    if user_broker
        .matches("ExecStart=/usr/bin/dbus-broker-launch --scope user")
        .count()
        != 1
        || user_broker.contains("--scope system")
    {
        bail!("user dbus-broker.service must launch exactly one user-scope broker");
    }
    let session_policy = fs::read_to_string(rootfs.join("usr/share/dbus-1/session.conf"))?;
    for required in [
        "<type>session</type>",
        "<auth>EXTERNAL</auth>",
        "<standard_session_servicedirs/>",
        "<allow own=\"*\"/>",
    ] {
        if !session_policy.contains(required) {
            bail!("per-user D-Bus policy is missing required contract: {required}");
        }
    }
    if session_policy.contains("<type>system</type>")
        || session_policy.contains("<user>messagebus</user>")
        || session_policy.contains("/run/dbus/system_bus_socket")
    {
        bail!("per-user D-Bus policy must remain separate from the system bus");
    }

    for rel in [
        "etc/pam.d/login",
        "etc/pam.d/su-l",
        "etc/pam.d/systemd-user",
        "usr/lib/systemd/user/dbus.socket",
        "usr/lib/systemd/user/dbus-broker.service",
        "usr/share/dbus-1/session.conf",
    ] {
        let body = fs::read_to_string(rootfs.join(rel))?;
        if body.contains("/run/user/1000") || body.contains("user@1000") {
            bail!("generic user-session configuration hardcodes the live UID in /{rel}");
        }
    }
    if path_entry_exists(&rootfs.join("run/user")) {
        bail!("stale /run/user content must not be baked into the staged rootfs");
    }

    validate_executable_runtime_closure(&rootfs.join(SYSTEMD_PAM_MODULE_REL), rootfs)?;
    validate_executable_runtime_closure(
        &rootfs.join("usr/lib/systemd/systemd-user-runtime-dir"),
        rootfs,
    )?;
    Ok(())
}

fn install_dbus_configuration(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let source = repo_root.join("src/system/dbus");
    let config_source = source.join("config/system.conf");
    let sysusers_source = source.join("config/dbus.conf");
    let units_source = source.join("units");
    for required in [
        &config_source,
        &sysusers_source,
        &units_source.join("dbus.socket"),
        &units_source.join("dbus-broker.service"),
    ] {
        if !required.exists() {
            bail!(
                "MattOS D-Bus integration file missing at {}",
                required.display()
            );
        }
    }

    for directory in [
        "etc/dbus-1/system.d",
        "usr/share/dbus-1/system-services",
        "usr/share/dbus-1/system.d",
        "usr/lib/sysusers.d",
        "usr/lib/systemd/system",
    ] {
        fs::create_dir_all(rootfs.join(directory))
            .with_context(|| format!("failed to create /{directory}"))?;
    }
    for (source, destination) in [
        (&config_source, rootfs.join("etc/dbus-1/system.conf")),
        (
            &sysusers_source,
            rootfs.join("usr/lib/sysusers.d/dbus.conf"),
        ),
        (
            &units_source.join("dbus.socket"),
            rootfs.join("usr/lib/systemd/system/dbus.socket"),
        ),
        (
            &units_source.join("dbus-broker.service"),
            rootfs.join("usr/lib/systemd/system/dbus-broker.service"),
        ),
    ] {
        if fs::read(source)? != fs::read(&destination)? {
            bail!(
                "dbus-broker did not install authoritative /{}",
                destination.strip_prefix(rootfs)?.display()
            );
        }
    }
    for rel in [
        "etc/dbus-1/system.conf",
        "usr/lib/sysusers.d/dbus.conf",
        "usr/lib/systemd/system/dbus.socket",
        "usr/lib/systemd/system/dbus-broker.service",
    ] {
        set_mode(rootfs.join(rel), 0o644)?;
    }

    let aliases = [
        ("dbus.service", "dbus-broker.service"),
        (
            "dbus-org.freedesktop.network1.service",
            "systemd-networkd.service",
        ),
        (
            "dbus-org.freedesktop.resolve1.service",
            "systemd-resolved.service",
        ),
        (
            "dbus-org.freedesktop.timesync1.service",
            "systemd-timesyncd.service",
        ),
        (
            "dbus-org.freedesktop.timedate1.service",
            "systemd-timedated.service",
        ),
        (
            "dbus-org.freedesktop.locale1.service",
            "systemd-localed.service",
        ),
        (
            "dbus-org.freedesktop.login1.service",
            "systemd-logind.service",
        ),
    ];
    for (alias, target) in aliases {
        install_systemd_service_alias(rootfs, alias, target)?;
    }

    validate_locale_service(rootfs)?;

    let sockets_wants = rootfs.join("etc/systemd/system/sockets.target.wants");
    fs::create_dir_all(&sockets_wants)
        .with_context(|| format!("failed to create {}", sockets_wants.display()))?;
    let socket_link = sockets_wants.join("dbus.socket");
    if path_entry_exists(&socket_link) {
        remove_path_if_exists(&socket_link)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("/usr/lib/systemd/system/dbus.socket", &socket_link)
        .context("failed to enable dbus.socket")?;

    validate_dbus_configuration(rootfs)
}

#[cfg(unix)]
fn install_systemd_service_alias(rootfs: &Path, alias: &str, target: &str) -> Result<()> {
    use std::os::unix::fs::symlink;

    let unit_dir = rootfs.join("usr/lib/systemd/system");
    let target_path = unit_dir.join(target);
    if !target_path.is_file() {
        bail!("refusing D-Bus alias {alias}: target unit {target} is missing");
    }
    let alias_path = unit_dir.join(alias);
    if path_entry_exists(&alias_path) {
        remove_path_if_exists(&alias_path)?;
    }
    symlink(target, &alias_path)
        .with_context(|| format!("failed to create D-Bus service alias {alias} -> {target}"))?;
    Ok(())
}

#[cfg(not(unix))]
fn install_systemd_service_alias(_rootfs: &Path, _alias: &str, _target: &str) -> Result<()> {
    bail!("systemd service alias installation requires a Unix host")
}

fn validate_dbus_configuration(rootfs: &Path) -> Result<()> {
    for rel in [
        "usr/bin/dbus-broker",
        "usr/bin/dbus-broker-launch",
        "usr/bin/busctl",
        "etc/dbus-1/system.conf",
        "etc/dbus-1/system.d",
        "usr/share/dbus-1/system-services",
        "usr/share/dbus-1/system.d",
        "usr/lib/sysusers.d/dbus.conf",
        "usr/lib/systemd/system/dbus.socket",
        "usr/lib/systemd/system/dbus-broker.service",
        "etc/systemd/system/sockets.target.wants/dbus.socket",
    ] {
        if !path_entry_exists(&rootfs.join(rel)) {
            bail!("required D-Bus rootfs path missing: /{rel}");
        }
    }

    let system_conf = fs::read_to_string(rootfs.join("etc/dbus-1/system.conf"))
        .context("failed to read installed system.conf")?;
    for required in [
        "<user>messagebus</user>",
        "<deny own=\"*\"/>",
        "<deny send_type=\"method_call\"/>",
        "<includedir>/usr/share/dbus-1/system.d</includedir>",
        "<includedir>/etc/dbus-1/system.d</includedir>",
    ] {
        if !system_conf.contains(required) {
            bail!("system-bus policy is missing required boundary: {required}");
        }
    }
    if system_conf.contains("<allow own=\"*\"/>") {
        bail!("system-bus policy must not allow arbitrary name ownership");
    }

    let socket_unit = fs::read_to_string(rootfs.join("usr/lib/systemd/system/dbus.socket"))
        .context("failed to read dbus.socket")?;
    if socket_unit
        .matches("ListenStream=/run/dbus/system_bus_socket")
        .count()
        != 1
    {
        bail!("dbus.socket must own exactly one conventional system-bus socket");
    }
    if path_entry_exists(&rootfs.join("run/dbus/system_bus_socket")) {
        bail!("stale system-bus socket must not be present in rootfs staging");
    }
    // The reference daemon may be installed solely for dbus-run-session's
    // private, process-scoped buses. It must never own the system/user bus or
    // appear under the legacy sbin path; dbus-broker remains the only
    // systemd-managed implementation.
    if rootfs.join("usr/sbin/dbus-daemon").exists() {
        bail!("legacy dbus-daemon system path found in rootfs");
    }
    for binary in ["usr/bin/dbus-broker", "usr/bin/dbus-broker-launch"] {
        validate_executable_runtime_closure(&rootfs.join(binary), rootfs)?;
    }
    if rootfs.join("usr/bin/dbus-daemon").is_file() {
        for binary in [
            "usr/bin/dbus-daemon",
            "usr/bin/dbus-run-session",
            "usr/bin/dbus-update-activation-environment",
        ] {
            validate_executable_runtime_closure(&rootfs.join(binary), rootfs)?;
        }
    }

    let broker_unit = fs::read_to_string(rootfs.join("usr/lib/systemd/system/dbus-broker.service"))
        .context("failed to read dbus-broker.service")?;
    if broker_unit
        .matches("ExecStart=/usr/bin/dbus-broker-launch")
        .count()
        != 1
        || broker_unit.contains("dbus-daemon")
    {
        bail!("dbus-broker.service must launch exactly one system-bus implementation");
    }

    let sysusers = fs::read_to_string(rootfs.join("usr/lib/sysusers.d/dbus.conf"))
        .context("failed to read dbus sysusers definition")?;
    let fields: Vec<&str> = sysusers.split_whitespace().collect();
    if fields.get(0) != Some(&"u!")
        || fields.get(1) != Some(&"messagebus")
        || fields.get(2) != Some(&"195")
    {
        bail!("messagebus sysusers definition must pin UID/GID 195");
    }
    for entry in fs::read_dir(rootfs.join("usr/lib/sysusers.d"))? {
        let path = entry?.path();
        if path.ends_with("dbus.conf") || !path.is_file() {
            continue;
        }
        let body = fs::read_to_string(&path).unwrap_or_default();
        for line in body.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.get(2) == Some(&"195") {
                bail!("messagebus UID/GID 195 collides with {}", path.display());
            }
        }
    }

    for name in [
        "systemd1",
        "network1",
        "resolve1",
        "timesync1",
        "timedate1",
        "login1",
    ] {
        let policy = rootfs.join(format!(
            "usr/share/dbus-1/system.d/org.freedesktop.{name}.conf"
        ));
        let service = rootfs.join(format!(
            "usr/share/dbus-1/system-services/org.freedesktop.{name}.service"
        ));
        if !policy.is_file() || !service.is_file() {
            bail!("D-Bus policy/service descriptor missing for org.freedesktop.{name}");
        }
    }

    for (alias, target) in [
        ("dbus.service", "dbus-broker.service"),
        (
            "dbus-org.freedesktop.network1.service",
            "systemd-networkd.service",
        ),
        (
            "dbus-org.freedesktop.resolve1.service",
            "systemd-resolved.service",
        ),
        (
            "dbus-org.freedesktop.timesync1.service",
            "systemd-timesyncd.service",
        ),
        (
            "dbus-org.freedesktop.timedate1.service",
            "systemd-timedated.service",
        ),
        (
            "dbus-org.freedesktop.login1.service",
            "systemd-logind.service",
        ),
    ] {
        let path = rootfs.join("usr/lib/systemd/system").join(alias);
        let actual =
            fs::read_link(&path).with_context(|| format!("missing D-Bus service alias {alias}"))?;
        if actual != Path::new(target) {
            bail!(
                "invalid D-Bus alias {alias}: expected {target}, got {}",
                actual.display()
            );
        }
    }

    Ok(())
}

fn validate_executable_runtime_closure(binary: &Path, rootfs: &Path) -> Result<()> {
    let file = Command::new("file")
        .args(["-L", path_str(binary)?])
        .output()
        .with_context(|| format!("failed to inspect {} with file", binary.display()))?;
    if !file.status.success() || !String::from_utf8_lossy(&file.stdout).contains("ELF") {
        bail!(
            "runtime closure target is not an ELF executable: {}",
            binary.display()
        );
    }
    let readelf = Command::new("readelf")
        .args(["-d", path_str(binary)?])
        .output()
        .with_context(|| format!("failed to inspect {} with readelf", binary.display()))?;
    if !readelf.status.success() {
        bail!(
            "readelf failed for runtime closure target {}",
            binary.display()
        );
    }

    let library_dirs = [
        rootfs.join("usr/lib/x86_64-linux-gnu"),
        rootfs.join("usr/lib/x86_64-linux-gnu/systemd"),
        rootfs.join("lib/x86_64-linux-gnu"),
        rootfs.join("usr/lib"),
        rootfs.join("lib"),
    ];
    let library_path = std::env::join_paths(library_dirs.iter())
        .context("failed to construct rootfs runtime library path")?;
    let ldd = Command::new("ldd")
        .arg(binary)
        .env("LD_LIBRARY_PATH", library_path)
        .output()
        .with_context(|| format!("failed to inspect {} with ldd", binary.display()))?;
    let ldd_text = String::from_utf8(ldd.stdout).context("ldd output was not UTF-8")?;
    if !ldd.status.success() || ldd_text.contains("not found") {
        bail!(
            "unresolved runtime dependency for {}:\n{}",
            binary.display(),
            ldd_text
        );
    }
    for token in ldd_text
        .split_whitespace()
        .filter(|token| token.starts_with('/'))
    {
        let dependency = Path::new(token);
        let staged = if dependency.starts_with(rootfs) {
            dependency.to_path_buf()
        } else {
            rootfs.join(dependency.strip_prefix("/").unwrap_or(dependency))
        };
        if !staged.exists() {
            bail!(
                "runtime dependency {} for {} is not staged at {}",
                dependency.display(),
                binary.display(),
                staged.display()
            );
        }
    }
    Ok(())
}

fn validate_network_configuration(rootfs: &Path) -> Result<()> {
    for rel in [
        "etc/systemd/resolved.conf",
        "etc/systemd/timesyncd.conf",
        "etc/nsswitch.conf",
        "etc/ssl/certs/ca-certificates.crt",
        "run/systemd/resolve",
        "usr/sbin/NetworkManager",
        "usr/bin/nmcli",
        "usr/lib/systemd/system/NetworkManager.service",
        "usr/lib/systemd/system/NetworkManager-wait-online.service",
        "usr/lib/systemd/systemd-resolved",
        "usr/lib/systemd/systemd-timesyncd",
        "usr/lib/x86_64-linux-gnu/libnss_resolve.so.2",
        "etc/systemd/system/multi-user.target.wants/NetworkManager.service",
        "etc/systemd/system/multi-user.target.wants/systemd-resolved.service",
        "etc/systemd/system/multi-user.target.wants/systemd-timesyncd.service",
    ] {
        if !path_entry_exists(&rootfs.join(rel)) {
            bail!("required network runtime path missing: /{rel}");
        }
    }
    if !path_entry_exists(&rootfs.join("etc/systemd/system/systemd-networkd.service"))
        || fs::read_link(rootfs.join("etc/systemd/system/systemd-networkd.service"))?
            != Path::new("/dev/null")
    {
        bail!("systemd-networkd must be masked when NetworkManager is active");
    }
    let nsswitch = fs::read_to_string(rootfs.join("etc/nsswitch.conf"))?;
    for database in ["passwd:", "group:", "shadow:", "hosts:", "networks:"] {
        if !nsswitch.lines().any(|line| line.starts_with(database)) {
            bail!("nsswitch configuration lacks {database}");
        }
    }
    if !nsswitch
        .lines()
        .any(|line| line.starts_with("hosts:") && line.contains("resolve"))
    {
        bail!("nsswitch hosts database does not use systemd-resolved");
    }
    let ca_bundle = fs::read(rootfs.join("etc/ssl/certs/ca-certificates.crt"))?;
    if ca_bundle.len() < 100_000
        || !ca_bundle
            .windows(27)
            .any(|window| window == b"-----BEGIN CERTIFICATE-----")
    {
        bail!("CA bundle is missing or does not contain PEM certificates");
    }
    #[cfg(unix)]
    {
        let target = fs::read_link(rootfs.join("etc/resolv.conf"))?;
        if target != Path::new("/run/systemd/resolve/stub-resolv.conf") {
            bail!(
                "/etc/resolv.conf has unexpected target {}",
                target.display()
            );
        }
    }

    let account_ids = [
        ("systemd-network", 192_u32),
        ("systemd-resolve", 193_u32),
        ("systemd-timesync", 194_u32),
    ];
    let passwd = fs::read_to_string(rootfs.join("etc/passwd"))?;
    let group = fs::read_to_string(rootfs.join("etc/group"))?;
    for (name, id) in account_ids {
        let id_field = format!(":{id}:");
        if passwd.lines().any(|line| line.contains(&id_field))
            || group.lines().any(|line| line.contains(&id_field))
        {
            bail!("network service account {name} ID {id} collides with a static account");
        }
        let sysusers = rootfs
            .join("usr/lib/sysusers.d")
            .join(format!("{name}.conf"));
        let body = fs::read_to_string(&sysusers)
            .with_context(|| format!("missing sysusers definition for {name}"))?;
        if !body.lines().any(|line| {
            line.contains(name) && line.split_whitespace().any(|field| field == id.to_string())
        }) {
            bail!("sysusers definition for {name} does not pin ID {id}");
        }
    }
    Ok(())
}

fn apply_live_profile(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let live_src = repo_root.join("src/system/profiles/live");
    if !live_src.exists() {
        bail!(
            "MattOS live profile missing at {}; expected live profile overlay",
            live_src.display()
        );
    }
    copy_tree_excluding_dotgit(&live_src, rootfs)?;

    let notice_script = rootfs.join("etc/profile.d/10-mattos-live-notice.sh");
    if notice_script.exists() {
        set_mode(notice_script, 0o755)?;
    }

    Ok(())
}

fn verify_required_pam_modules(rootfs: &Path) -> Result<()> {
    let security_dirs = [
        rootfs.join("usr/lib/x86_64-linux-gnu/security"),
        rootfs.join("usr/lib/security"),
    ];
    for module in REQUIRED_PAM_MODULES {
        let mut found = false;
        for dir in &security_dirs {
            if dir.join(module).exists() {
                found = true;
                break;
            }
        }
        if !found {
            bail!(
                "required PAM module {} missing from rootfs security dirs",
                module
            );
        }
    }

    Ok(())
}

fn enforce_auth_file_modes(rootfs: &Path) -> Result<()> {
    for (rel, mode) in [
        ("etc/shadow", 0o600),
        ("etc/gshadow", 0o600),
        ("etc/passwd", 0o644),
        ("etc/group", 0o644),
        ("etc/sudoers", 0o440),
        ("usr/bin/login", 0o4755),
        ("usr/bin/su", 0o4755),
        ("usr/bin/passwd", 0o4755),
        ("usr/bin/sudo", 0o4755),
        // Flatpak's document portal invokes this target-owned helper to
        // mount /run/user/$UID/doc.  dpkg/fakeroot rootfs assembly can lose
        // special modes, so the final image establishes the runtime
        // contract explicitly just like the authentication helpers above.
        ("usr/bin/fusermount3", 0o4755),
        // Flatpak system AppStream and installation authorization uses the
        // polkit authentication helper, whose upstream contract is setuid
        // root.  Package/rootfs copying can otherwise reduce it to 0755.
        ("usr/lib/polkit-1/polkit-agent-helper-1", 0o4755),
    ] {
        let path = rootfs.join(rel);
        if !path.exists() {
            bail!("expected auth file missing at {}", path.display());
        }
        set_mode(path, mode)?;
    }

    let sudoers_dir = rootfs.join("etc/sudoers.d");
    if !sudoers_dir.exists() {
        bail!(
            "expected sudoers include dir missing at {}",
            sudoers_dir.display()
        );
    }
    set_mode(sudoers_dir, 0o750)?;

    for rel in ["etc/sudoers.d/00-mattos-live", "etc/sudoers.d/README"] {
        let path = rootfs.join(rel);
        if path.exists() {
            set_mode(path, 0o440)?;
        }
    }

    let root_home = rootfs.join("root");
    if root_home.exists() {
        set_mode(root_home, 0o700)?;
    }
    let live_home = rootfs.join("home/mattos");
    if live_home.exists() {
        set_mode(live_home, 0o750)?;
    }

    Ok(())
}

#[cfg(unix)]
fn validate_auth_file_modes(rootfs: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    for (rel, expected_mode) in [
        ("etc/shadow", 0o600),
        ("etc/gshadow", 0o600),
        ("etc/passwd", 0o644),
        ("etc/group", 0o644),
        ("etc/sudoers", 0o440),
        ("etc/sudoers.d", 0o750),
        ("usr/bin/login", 0o4755),
        ("usr/bin/su", 0o4755),
        ("usr/bin/passwd", 0o4755),
        ("usr/bin/sudo", 0o4755),
        ("usr/bin/fusermount3", 0o4755),
        ("usr/lib/polkit-1/polkit-agent-helper-1", 0o4755),
        ("root", 0o700),
        ("home/mattos", 0o750),
    ] {
        let path = rootfs.join(rel);
        let actual_mode = fs::metadata(&path)
            .with_context(|| format!("failed to stat security-sensitive path {}", path.display()))?
            .permissions()
            .mode()
            & 0o7777;
        if actual_mode != expected_mode {
            bail!(
                "unsafe mode {:04o} on {}; expected {:04o}",
                actual_mode,
                path.display(),
                expected_mode
            );
        }
    }

    for rel in ["etc/sudoers.d/00-mattos-live", "etc/sudoers.d/README"] {
        let path = rootfs.join(rel);
        if path.exists() {
            let actual_mode = fs::metadata(&path)
                .with_context(|| format!("failed to stat {}", path.display()))?
                .permissions()
                .mode()
                & 0o7777;
            if actual_mode != 0o440 {
                bail!(
                    "unsafe mode {:04o} on {}; expected 0440",
                    actual_mode,
                    path.display()
                );
            }
        }
    }

    Ok(())
}

#[cfg(not(unix))]
fn validate_auth_file_modes(_rootfs: &Path) -> Result<()> {
    bail!("authentication file-mode validation requires a Unix host")
}

fn validate_account_database(rootfs: &Path) -> Result<()> {
    let passwd_path = rootfs.join("etc/passwd");
    let group_path = rootfs.join("etc/group");
    let shadow_path = rootfs.join("etc/shadow");
    let gshadow_path = rootfs.join("etc/gshadow");

    for path in [&passwd_path, &group_path, &shadow_path, &gshadow_path] {
        if !path.exists() {
            bail!(
                "required account database file missing at {}",
                path.display()
            );
        }
    }

    let passwd_body = fs::read_to_string(&passwd_path)
        .with_context(|| format!("failed to read {}", passwd_path.display()))?;
    let group_body = fs::read_to_string(&group_path)
        .with_context(|| format!("failed to read {}", group_path.display()))?;

    if passwd_body.contains("matt-alienware") || passwd_body.contains("matt:") {
        bail!("passwd file appears to contain host developer username leakage")
    }

    let mut seen_uids = BTreeSet::<u32>::new();
    let mut seen_gids = BTreeSet::<u32>::new();
    let mut saw_root = false;
    let mut saw_live = false;

    for line in passwd_body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() != 7 {
            bail!("invalid passwd entry format: {line}");
        }
        let user = parts[0];
        let uid = parts[2]
            .parse::<u32>()
            .with_context(|| format!("invalid uid in passwd entry: {line}"))?;
        let gid = parts[3]
            .parse::<u32>()
            .with_context(|| format!("invalid gid in passwd entry: {line}"))?;

        if !seen_uids.insert(uid) {
            bail!("duplicate uid detected in passwd: {uid}")
        }

        if user == "root" {
            saw_root = true;
            if uid != 0 || gid != 0 || parts[5] != "/root" || parts[6] != "/bin/brush" {
                bail!("root account entry does not match expected MattOS policy")
            }
        }

        if user == "mattos" {
            saw_live = true;
            if uid != 1000 || gid != 1000 || parts[5] != "/home/mattos" || parts[6] != "/bin/brush"
            {
                bail!("live user mattos entry does not match expected MattOS policy")
            }
        }
    }

    if !saw_root {
        bail!("root account missing from passwd")
    }
    if !saw_live {
        bail!("live user mattos missing from passwd")
    }

    let mut saw_sudo_group = false;
    for line in group_body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() != 4 {
            bail!("invalid group entry format: {line}");
        }
        let name = parts[0];
        let gid = parts[2]
            .parse::<u32>()
            .with_context(|| format!("invalid gid in group entry: {line}"))?;
        if !seen_gids.insert(gid) {
            bail!("duplicate gid detected in group: {gid}")
        }
        if name == "sudo" {
            saw_sudo_group = true;
            if !parts[3].split(',').any(|m| m == "mattos") {
                bail!("sudo group exists but mattos is not a member")
            }
        }
    }

    if !saw_sudo_group {
        bail!("sudo administrative group missing from group database")
    }

    Ok(())
}

fn set_mode(path: PathBuf, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        let perms = std::os::unix::fs::PermissionsExt::from_mode(mode);
        fs::set_permissions(&path, perms)
            .with_context(|| format!("failed to set mode {:o} on {}", mode, path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}

fn copy_systemd_runtime_dependencies(rootfs: &Path) -> Result<()> {
    let mut binaries = Vec::new();
    for rel in [
        "usr/lib/systemd/systemd",
        "usr/lib/systemd/systemd-journald",
        "usr/lib/systemd/systemd-udevd",
        "usr/lib/systemd/systemd-networkd",
        "usr/lib/systemd/systemd-resolved",
        "usr/lib/systemd/systemd-timesyncd",
        "usr/lib/systemd/systemd-timedated",
        "usr/lib/systemd/systemd-localed",
        "usr/lib/systemd/systemd-logind",
        "usr/lib/systemd/systemd-user-runtime-dir",
        "usr/bin/systemctl",
        "usr/bin/journalctl",
        "usr/bin/busctl",
        "usr/bin/loginctl",
        "usr/bin/networkctl",
        "usr/bin/resolvectl",
        "usr/bin/timedatectl",
        "usr/bin/localectl",
    ] {
        let p = rootfs.join(rel);
        if p.exists() {
            binaries.push(p);
        }
    }

    for bin in binaries {
        copy_runtime_dependencies(&bin, rootfs)?;
    }
    Ok(())
}

fn validate_locale_service(rootfs: &Path) -> Result<()> {
    for rel in [
        "usr/lib/systemd/systemd-localed",
        "usr/lib/systemd/system/systemd-localed.service",
        "usr/lib/systemd/system/dbus-org.freedesktop.locale1.service",
        "usr/share/dbus-1/system-services/org.freedesktop.locale1.service",
        "usr/share/dbus-1/system.d/org.freedesktop.locale1.conf",
        "usr/bin/localectl",
    ] {
        if !rootfs.join(rel).exists() {
            bail!("systemd-localed runtime contract is missing /{rel}");
        }
    }
    Ok(())
}

fn generate_baseline_locale(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let glibc_install = repo_root.join("out/build/glibc/install");
    let loader = glibc_install.join("lib64/ld-linux-x86-64.so.2");
    let localedef = glibc_install.join("usr/bin/localedef");
    if !loader.is_file() || !localedef.is_file() {
        bail!("glibc localedef runtime is missing; cannot generate baseline locale");
    }
    fs::create_dir_all(rootfs.join("usr/lib/x86_64-linux-gnu/locale"))?;
    let library_path = std::env::join_paths([
        glibc_install.join("usr/lib/x86_64-linux-gnu"),
        glibc_install.join("lib64"),
    ])?;
    let prefix = format!("--prefix={}", rootfs.display());
    let library_path = library_path
        .to_str()
        .ok_or_else(|| anyhow!("glibc locale library path is not valid UTF-8"))?;
    let i18n_path = glibc_install.join("usr/share/i18n");
    let i18n_path = i18n_path
        .to_str()
        .ok_or_else(|| anyhow!("glibc i18n source path is not valid UTF-8"))?;
    run_cmd_with_env_overrides(
        repo_root,
        path_str(&loader)?,
        &[
            "--library-path",
            library_path,
            path_str(&localedef)?,
            &prefix,
            "-i",
            "en_US",
            "-f",
            "UTF-8",
            "--no-archive",
            "en_US.UTF-8",
        ],
        &[("I18NPATH", i18n_path.to_string())],
    )?;
    let locale_dir = rootfs.join("usr/lib/x86_64-linux-gnu/locale");
    if !locale_dir.join("en_US.utf8").exists() {
        bail!("baseline en_US.UTF-8 generation produced no compiled en_US.utf8 locale");
    }
    fs::write(rootfs.join("etc/locale.conf"), "LANG=en_US.UTF-8\n")?;
    Ok(())
}

fn resolve_coreutils_multicall(repo_root: &Path) -> Result<PathBuf> {
    let candidates = [
        repo_root.join("out/build/coreutils/cargo-target/release/coreutils"),
        repo_root.join("out/build/coreutils/cargo-target/release/uutils"),
    ];
    candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .ok_or_else(|| anyhow!("coreutils multicall binary not found; run build coreutils first"))
}

fn list_coreutils_applets(coreutils_multicall: &Path) -> Result<Vec<String>> {
    let output = Command::new(coreutils_multicall)
        .arg("--list")
        .output()
        .with_context(|| format!("failed to run {} --list", coreutils_multicall.display()))?;
    if !output.status.success() {
        bail!("coreutils --list failed with status {}", output.status)
    }

    let raw = String::from_utf8(output.stdout).context("coreutils --list output was not UTF-8")?;
    let mut applets: Vec<String> = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('<') && *line != "uutils")
        .map(ToOwned::to_owned)
        .collect();
    applets.sort();
    applets.dedup();
    if applets.is_empty() {
        bail!("coreutils --list returned no applets")
    }
    Ok(applets)
}

fn install_userland_binary(
    repo_root: &Path,
    rootfs: &Path,
    spec: &BinaryInstallSpec,
) -> Result<()> {
    let source = repo_root.join(spec.source_rel);
    if !source.exists() {
        bail!(
            "{} binary missing at {}; run the matching build stage first",
            spec.command_name,
            source.display()
        )
    }

    let dst = rootfs.join("usr/bin").join(spec.install_name);
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(&source, &dst)
        .with_context(|| format!("failed to copy {} into rootfs", source.display()))?;
    copy_runtime_dependencies(&dst, rootfs)?;
    Ok(())
}

#[cfg(unix)]
fn create_command_aliases(rootfs: &Path, target_binary: &str, aliases: &[&str]) -> Result<()> {
    use std::os::unix::fs::symlink;

    let usr_bin = rootfs.join("usr/bin");
    for alias in aliases {
        let link = usr_bin.join(alias);
        if path_entry_exists(&link) {
            fs::remove_file(&link)
                .with_context(|| format!("failed to remove existing alias {}", link.display()))?;
        }
        symlink(format!("/bin/{target_binary}"), &link)
            .with_context(|| format!("failed to create alias {}", link.display()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn create_command_aliases(_rootfs: &Path, _target_binary: &str, _aliases: &[&str]) -> Result<()> {
    bail!("command alias generation requires Unix symlink support")
}

fn validate_no_duplicate_commands(provider_commands: &BTreeMap<&str, Vec<String>>) -> Result<()> {
    let mut owners = BTreeMap::<String, Vec<&str>>::new();
    for (provider, commands) in provider_commands {
        for command in commands {
            owners.entry(command.clone()).or_default().push(provider);
        }
    }

    let duplicates: Vec<String> = owners
        .iter()
        .filter_map(|(cmd, providers)| {
            if providers.len() > 1 {
                Some(format!("{} [{}]", cmd, providers.join(", ")))
            } else {
                None
            }
        })
        .collect();

    if !duplicates.is_empty() {
        bail!(
            "duplicate command ownership detected: {}",
            duplicates.join("; ")
        )
    }

    Ok(())
}

fn path_entry_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn write_userland_inventory(rootfs: &Path, inventory: &UserlandInventory) -> Result<()> {
    let path = rootfs.join(USERLAND_INVENTORY_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut lines = Vec::new();
    lines.push("# MattOS userland command inventory".to_string());
    lines.push("# format: provider:command".to_string());
    lines.push(String::new());
    lines.push("[implemented_upstream]".to_string());
    for entry in &inventory.implemented_upstream {
        lines.push(entry.clone());
    }
    lines.push(String::new());
    lines.push("[compiled]".to_string());
    for entry in &inventory.compiled {
        lines.push(entry.clone());
    }
    lines.push(String::new());
    lines.push("[installed]".to_string());
    for entry in &inventory.installed {
        lines.push(entry.clone());
    }
    lines.push(String::new());
    lines.push("[intentionally_excluded]".to_string());
    for entry in &inventory.intentionally_excluded {
        lines.push(entry.clone());
    }
    lines.push(String::new());
    lines.push("[failed_compatibility]".to_string());
    for entry in &inventory.failed_compatibility {
        lines.push(entry.clone());
    }

    fs::write(&path, lines.join("\n") + "\n")
        .with_context(|| format!("failed to write {}", path.display()))
}

fn build_live_root(repo_root: &Path) -> Result<()> {
    let spec = build_stage_spec(BuildStage::LiveRoot);
    performance::execute_cached_stage(
        repo_root,
        &spec,
        || validate_cached_live_root(repo_root),
        || build_live_root_atomic(repo_root),
    )
}

fn validate_cached_live_root(repo_root: &Path) -> Result<()> {
    validate_squashfs_image(&repo_root.join(LIVE_ROOT_IMAGE_PATH))?;
    let inventory = repo_root.join("out/reports/live-root-inventory.tsv");
    if !inventory.is_file() {
        bail!("live-root inventory is missing: {}", inventory.display());
    }
    Ok(())
}

fn has_squashfs_header(path: &Path) -> Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 4];
    Ok(file.read_exact(&mut header).is_ok() && header == *b"hsqs")
}

fn validate_squashfs_image(path: &Path) -> Result<()> {
    if !has_squashfs_header(path)? {
        bail!("live root is not a SquashFS image: {}", path.display());
    }
    if fs::metadata(path)?.len() < 1024 * 1024 {
        bail!(
            "live-root SquashFS is unexpectedly small: {}",
            path.display()
        );
    }
    let output = Command::new("unsquashfs")
        .args(["-stat"])
        .arg(path)
        .output()
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if !output.status.success() {
        bail!("unsquashfs rejected live root {}", path.display());
    }
    Ok(())
}

fn regular_file_bytes(root: &Path) -> Result<(u64, u64)> {
    fn visit(path: &Path, files: &mut u64, bytes: &mut u64) -> Result<()> {
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                visit(&path, files, bytes)?;
            } else if metadata.is_file() {
                *files += 1;
                *bytes += metadata.len();
            }
        }
        Ok(())
    }
    let mut files = 0;
    let mut bytes = 0;
    visit(root, &mut files, &mut bytes)?;
    Ok((files, bytes))
}

fn largest_regular_files(root: &Path, limit: usize) -> Result<Vec<(u64, String)>> {
    fn visit(root: &Path, path: &Path, files: &mut Vec<(u64, String)>) -> Result<()> {
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                visit(root, &path, files)?;
            } else if metadata.is_file() {
                files.push((
                    metadata.len(),
                    path.strip_prefix(root)?
                        .to_string_lossy()
                        .replace('\\', "/"),
                ));
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    files.truncate(limit);
    Ok(files)
}

fn build_live_root_atomic(repo_root: &Path) -> Result<()> {
    let rootfs = repo_root.join("out/build/rootfs");
    if !rootfs.is_dir() {
        bail!("rootfs not found; run build rootfs first");
    }
    let destination = repo_root.join(LIVE_ROOT_IMAGE_PATH);
    let temp = performance::temporary_sibling(&destination, "building")?;
    let processors = scheduler::child_job_limit().clamp(1, 4).to_string();
    let result = run_cmd(
        repo_root,
        "mksquashfs",
        &[
            path_str(&rootfs)?,
            path_str(&temp)?,
            "-noappend",
            "-comp",
            LIVE_ROOT_SQUASHFS_COMPRESSION,
            "-Xcompression-level",
            LIVE_ROOT_SQUASHFS_LEVEL,
            "-b",
            "1M",
            "-processors",
            &processors,
            "-all-root",
            "-no-progress",
            "-no-recovery",
        ],
    );
    if let Err(error) = result {
        let _ = remove_path_if_exists(&temp);
        return Err(error);
    }
    if let Err(error) = validate_squashfs_image(&temp) {
        let _ = remove_path_if_exists(&temp);
        return Err(error);
    }

    let (files, uncompressed_bytes) = regular_file_bytes(&rootfs)?;
    let compressed_bytes = fs::metadata(&temp)?.len();
    let reports = repo_root.join("out/reports");
    fs::create_dir_all(&reports)?;
    fs::write(
        reports.join("live-root-inventory.tsv"),
        format!(
            "artifact\tfilesystem\tregular_files\tuncompressed_regular_bytes\tcompressed_bytes\tordinary_payload_in_early_initramfs\n{}\tsquashfs-zstd-level12\t{}\t{}\t{}\t0\n",
            LIVE_ROOT_IMAGE_PATH, files, uncompressed_bytes, compressed_bytes
        ),
    )?;
    performance::atomic_replace_path(&temp, &destination)
}

fn build_initramfs(repo_root: &Path) -> Result<()> {
    let spec = build_stage_spec(BuildStage::Initramfs);
    performance::execute_cached_stage(
        repo_root,
        &spec,
        || validate_cached_initramfs(repo_root),
        || build_initramfs_atomic(repo_root),
    )
}

fn validate_cached_initramfs(repo_root: &Path) -> Result<()> {
    validate_early_initramfs(&repo_root.join(INITRAMFS_ARCHIVE_PATH))
}

fn has_xz_header(path: &Path) -> Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 6];
    Ok(file.read_exact(&mut header).is_ok() && header == [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00])
}

fn validate_early_initramfs(path: &Path) -> Result<()> {
    if !has_xz_header(path)? {
        bail!("initramfs is not an XZ stream: {}", path.display());
    }
    let size = fs::metadata(path)?.len();
    if size > EARLY_INITRAMFS_SIZE_LIMIT {
        bail!(
            "early initramfs is {size} bytes, above its structural limit of {EARLY_INITRAMFS_SIZE_LIMIT} bytes"
        );
    }
    let listing = Command::new("bash")
        .args([
            "-o",
            "pipefail",
            "-c",
            &format!(
                "xz -dc {} | cpio -it --quiet",
                shell_escape(path_str(path)?)
            ),
        ])
        .output()
        .with_context(|| format!("failed to inventory {}", path.display()))?;
    if !listing.status.success() {
        bail!("failed to list early initramfs {}", path.display());
    }
    let paths = String::from_utf8(listing.stdout).context("initramfs listing was not UTF-8")?;
    let normalized = paths
        .lines()
        .map(|line| line.trim_start_matches("./"))
        .collect::<Vec<_>>();
    if !normalized.contains(&"init") {
        bail!("early initramfs does not contain /init");
    }
    for forbidden in [
        "python", "clang", "llvm", "rustc", "cargo", "git", "systemd", "brush",
    ] {
        if normalized.iter().any(|path| path.contains(forbidden)) {
            bail!("general userland token {forbidden} leaked into early initramfs");
        }
    }
    Ok(())
}

fn build_initramfs_atomic(repo_root: &Path) -> Result<()> {
    let out_build = repo_root.join("out/build");
    fs::create_dir_all(&out_build).context("failed to create out/build directory")?;
    let destination = repo_root.join(INITRAMFS_ARCHIVE_PATH);
    let temp = performance::temporary_sibling(&destination, "building")?;
    let tree = performance::temporary_sibling(&out_build.join("early-initramfs-root"), "building")?;
    fs::create_dir_all(&tree)?;
    set_mode(tree.clone(), 0o755)?;
    let source = repo_root.join("src/boot/live-init.c");
    let compiler = repo_root.join("out/build/gcc-toolchain/install/usr/bin/gcc");
    let sysroot = repo_root.join("out/sysroot");
    if !source.is_file() || !compiler.is_file() || !sysroot.is_dir() {
        bail!("early-init source or MattOS compiler/sysroot is missing");
    }
    let init = tree.join("init");
    let sysroot_arg = format!("--sysroot={}", sysroot.display());
    let libc_search = format!("-B{}/usr/lib/x86_64-linux-gnu/", sysroot.display());
    let gcc_search = format!(
        "-B{}/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0/",
        sysroot.display()
    );
    let libc_link = format!("-L{}/usr/lib/x86_64-linux-gnu", sysroot.display());
    let gcc_link = format!(
        "-L{}/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0",
        sysroot.display()
    );
    if let Err(error) = run_cmd(
        repo_root,
        path_str(&compiler)?,
        &[
            &sysroot_arg,
            &libc_search,
            &gcc_search,
            &libc_link,
            &gcc_link,
            "-std=c11",
            "-Os",
            "-static",
            "-s",
            "-fno-ident",
            "-Wl,--build-id=none",
            "-Wall",
            "-Wextra",
            "-Werror",
            path_str(&source)?,
            "-o",
            path_str(&init)?,
        ],
    ) {
        let _ = remove_path_if_exists(&tree);
        return Err(error);
    }
    set_mode(init.clone(), 0o755)?;
    let (module_release, module_count, firmware_count) =
        stage_boot_module_closure(repo_root, &tree)?;
    validate_initramfs_archive_owner(INITRAMFS_ARCHIVE_OWNER)?;
    let archive_command = format!(
        "find . -exec touch -h -d @{MATTOS_SOURCE_DATE_EPOCH} {{}} + && find . -print0 | sort -z | cpio --null -o --quiet --reproducible --owner={INITRAMFS_ARCHIVE_OWNER} --format=newc | xz -1 -T1 --check=crc32 --stdout > {}",
        shell_escape(path_str(&temp)?)
    );

    if let Err(error) = run_cmd(&tree, "bash", &["-lc", &archive_command]) {
        let _ = remove_path_if_exists(&temp);
        let _ = remove_path_if_exists(&tree);
        return Err(error);
    }
    if let Err(error) = validate_early_initramfs(&temp) {
        let _ = remove_path_if_exists(&temp);
        let _ = remove_path_if_exists(&tree);
        return Err(error);
    }
    let reports = repo_root.join("out/reports");
    fs::create_dir_all(&reports)?;
    fs::write(
        reports.join("early-initramfs-inventory.tsv"),
        format!(
            "path\trole\tuncompressed_bytes\n/init\tstatic-live-bootstrap\t{}\n/usr/lib/modules/{module_release}\tboot-critical-module-closure({module_count})\t0\n/usr/lib/firmware\tboot-critical-firmware-only({firmware_count})\t0\narchive\txz-newc\t{}\n",
            fs::metadata(&init)?.len(),
            fs::metadata(&temp)?.len()
        ),
    )?;
    remove_path_if_exists(&tree)?;
    performance::atomic_replace_path(&temp, &destination)
}

fn validate_initramfs_archive_owner(owner: &str) -> Result<()> {
    if owner != "0:0" {
        bail!("unsafe initramfs archive owner {owner}; expected root ownership 0:0")
    }
    Ok(())
}

fn build_iso(repo_root: &Path) -> Result<()> {
    let spec = build_stage_spec(BuildStage::Iso);
    performance::execute_cached_stage(
        repo_root,
        &spec,
        || validate_cached_iso(repo_root),
        || build_iso_atomic(repo_root),
    )
}

fn validate_cached_iso(repo_root: &Path) -> Result<()> {
    let iso = repo_root.join("out/images/mattos-x86_64.iso");
    if fs::metadata(&iso)?.len() < 1024 * 1024 {
        bail!("cached ISO is unexpectedly small");
    }
    validate_staged_grub_config(&repo_root.join("out/build/iso/boot/grub/grub.cfg"))?;
    validate_early_initramfs(&repo_root.join("out/build/iso/boot/early-initramfs.cpio.xz"))?;
    validate_squashfs_image(&repo_root.join("out/build/iso/live/rootfs.squashfs"))?;
    let report = repo_root.join("out/reports/live-image-inventory.tsv");
    if !report.is_file() {
        bail!("live-image inventory is missing: {}", report.display());
    }
    Ok(())
}

fn write_live_image_inventory(
    repo_root: &Path,
    image_path: &Path,
    report_path: &Path,
) -> Result<()> {
    let rootfs = repo_root.join("out/build/rootfs");
    let initramfs = repo_root.join(INITRAMFS_ARCHIVE_PATH);
    let live_root = repo_root.join(LIVE_ROOT_IMAGE_PATH);
    let expanded = Command::new("xz")
        .args(["-dc"])
        .arg(&initramfs)
        .output()
        .context("failed to measure uncompressed early initramfs")?;
    if !expanded.status.success() {
        bail!("xz rejected the early initramfs while producing its size report");
    }

    let mut lines = vec!["record\tpath\tbytes\tdetail".to_string()];
    lines.push(format!(
        "artifact\t{}\t{}\tuncompressed-newc",
        INITRAMFS_ARCHIVE_PATH,
        expanded.stdout.len()
    ));
    lines.push(format!(
        "artifact\t{}\t{}\txz-newc",
        INITRAMFS_ARCHIVE_PATH,
        fs::metadata(&initramfs)?.len()
    ));
    lines.push(format!(
        "artifact\t{}\t{}\tsquashfs-zstd-level12",
        LIVE_ROOT_IMAGE_PATH,
        fs::metadata(&live_root)?.len()
    ));
    lines.push(format!(
        "artifact\tout/images/mattos-x86_64.iso\t{}\tiso9660",
        fs::metadata(image_path)?.len()
    ));
    lines.push("duplication\tordinary-root-payload-in-early-initramfs\t0\tbytes".into());

    let mut top_level = fs::read_dir(&rootfs)?.collect::<std::io::Result<Vec<_>>>()?;
    top_level.sort_by_key(|entry| entry.file_name());
    for entry in top_level {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let bytes = if metadata.is_dir() {
            regular_file_bytes(&path)?.1
        } else if metadata.is_file() {
            metadata.len()
        } else {
            continue;
        };
        lines.push(format!(
            "top-level\t{}\t{}\tlogical-regular-file-bytes",
            entry.file_name().to_string_lossy(),
            bytes
        ));
    }
    for (bytes, path) in largest_regular_files(&rootfs, 25)? {
        lines.push(format!("largest-file\t{path}\t{bytes}\tlogical-bytes"));
    }
    fs::write(report_path, lines.join("\n") + "\n")
        .with_context(|| format!("failed to write {}", report_path.display()))
}

fn build_iso_atomic(repo_root: &Path) -> Result<()> {
    let grub_src = validate_grub_config_source(repo_root)?;

    let kernel = repo_root.join("out/build/linux/build/arch/x86/boot/bzImage");
    if !kernel.exists() {
        bail!(
            "kernel image missing at {}; build kernel first",
            kernel.display()
        );
    }

    let initramfs = repo_root.join(INITRAMFS_ARCHIVE_PATH);
    if !initramfs.exists() {
        bail!(
            "initramfs missing at {}; run build initramfs",
            initramfs.display()
        );
    }
    let live_root = repo_root.join(LIVE_ROOT_IMAGE_PATH);
    if !live_root.exists() {
        bail!(
            "live root missing at {}; run build live-root",
            live_root.display()
        );
    }

    let iso_destination = repo_root.join("out/build/iso");
    let iso_root = performance::temporary_sibling(&iso_destination, "building")?;
    let grub_dir = iso_root.join("boot/grub");
    fs::create_dir_all(&grub_dir).context("failed to create ISO directory layout")?;

    fs::copy(&kernel, iso_root.join("boot/vmlinuz"))
        .context("failed to stage kernel into ISO tree")?;
    fs::copy(&initramfs, iso_root.join("boot/early-initramfs.cpio.xz"))
        .context("failed to stage initramfs into ISO tree")?;
    fs::create_dir_all(iso_root.join("live"))?;
    fs::copy(&live_root, iso_root.join("live/rootfs.squashfs"))
        .context("failed to stage live root into ISO tree")?;
    let staged_grub_cfg = grub_dir.join("grub.cfg");
    fs::copy(&grub_src, &staged_grub_cfg).context("failed to copy grub config")?;
    validate_staged_grub_config(&staged_grub_cfg)?;

    let src_grub_text = fs::read_to_string(&grub_src)
        .with_context(|| format!("failed to read {}", grub_src.display()))?;
    let staged_grub_text = fs::read_to_string(&staged_grub_cfg)
        .with_context(|| format!("failed to read {}", staged_grub_cfg.display()))?;
    if src_grub_text != staged_grub_text {
        bail!(
            "staged GRUB config at {} differs from authoritative source {}",
            staged_grub_cfg.display(),
            grub_src.display()
        );
    }

    let out_images = repo_root.join("out/images");
    fs::create_dir_all(&out_images).context("failed to create out/images")?;
    let image_destination = out_images.join("mattos-x86_64.iso");
    let image_temp = performance::temporary_sibling(&image_destination, "building")?;
    let build_tmp = repo_root.join("out/tmp");
    fs::create_dir_all(&build_tmp)?;
    let result = run_cmd_with_env_overrides(
        repo_root,
        "grub-mkrescue",
        &[
            "-o",
            path_str(&image_temp)?,
            path_str(&iso_root)?,
            "--modification-date=2026010100000000",
            "--set_all_file_dates",
            "2026010100000000",
            // The bundled offline Flatpak OSTree closure can make the live
            // SquashFS exceed ISO9660's traditional 4 GiB single-file limit.
            // ISO9660 level 3 uses multi-extent files while retaining the
            // hybrid GRUB image; this xorriso build does not support UDF.
            "-iso-level",
            "3",
        ],
        &[
            ("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH.to_string()),
            ("TMPDIR", build_tmp.display().to_string()),
        ],
    );
    if let Err(error) = result {
        let _ = remove_path_if_exists(&iso_root);
        let _ = remove_path_if_exists(&image_temp);
        return Err(error);
    }
    if fs::metadata(&image_temp)?.len() < 1024 * 1024 {
        bail!("generated ISO is unexpectedly small");
    }
    validate_dual_firmware_iso(repo_root, &image_temp)?;
    let reports = repo_root.join("out/reports");
    fs::create_dir_all(&reports)?;
    let report_destination = reports.join("live-image-inventory.tsv");
    let report_temp = performance::temporary_sibling(&report_destination, "building")?;
    write_live_image_inventory(repo_root, &image_temp, &report_temp)?;
    performance::atomic_replace_path(&iso_root, &iso_destination)?;
    performance::atomic_replace_path(&image_temp, &image_destination)?;
    performance::atomic_replace_path(&report_temp, &report_destination)?;
    // Refresh the report from the image just published so successful builds
    // cannot leave historical SquashFS/ISO sizes behind.
    report_artifacts(repo_root)
}

fn validate_dual_firmware_iso(repo_root: &Path, image: &Path) -> Result<()> {
    let report = run_cmd_capture(
        repo_root,
        "xorriso",
        &[
            "-indev",
            path_str(image)?,
            "-report_el_torito",
            "as_mkisofs",
        ],
    )?;
    if !report.contains("-b '") && !report.contains("-b ") {
        bail!("ISO has no El Torito legacy BIOS boot image");
    }
    if !report.contains("-e '") && !report.contains("-e ") {
        bail!("ISO has no El Torito UEFI boot image");
    }
    Ok(())
}

fn validate_grub_config_source(repo_root: &Path) -> Result<PathBuf> {
    let authoritative = repo_root.join(AUTHORITATIVE_GRUB_CFG);
    if !authoritative.exists() {
        bail!(
            "authoritative GRUB config missing at {}; expected single source at {}",
            authoritative.display(),
            AUTHORITATIVE_GRUB_CFG
        );
    }

    for obsolete in OBSOLETE_GRUB_CFG_PATHS {
        let obsolete_path = repo_root.join(obsolete);
        if obsolete_path.exists() {
            bail!(
                "obsolete GRUB config path detected at {}; remove stale duplicate and keep only {}",
                obsolete_path.display(),
                AUTHORITATIVE_GRUB_CFG
            );
        }
    }

    Ok(authoritative)
}

fn validate_staged_grub_config(path: &Path) -> Result<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read staged grub config {}", path.display()))?;

    for needle in [
        GRUB_SYSTEMD_ENTRY,
        "menuentry \"Start MattOS Live (CLI)\"",
        "menuentry \"Install MattOS\"",
        "menuentry \"Install MattOS (CLI)\"",
        GRUB_RESCUE_ENTRY,
        GRUB_EARLY_RDINIT,
        GRUB_RESCUE_MARKER,
    ] {
        if !content.contains(needle) {
            bail!(
                "staged GRUB config {} is missing required marker: {}",
                path.display(),
                needle
            );
        }
    }

    if content
        .matches("initrd /boot/early-initramfs.cpio.xz")
        .count()
        != 5
    {
        bail!("staged GRUB config must load the early initramfs for all five entries");
    }

    Ok(())
}

fn run_qemu(repo_root: &Path) -> Result<()> {
    let iso = repo_root.join("out/images/mattos-x86_64.iso");
    if !iso.exists() {
        bail!("ISO missing at {}; run build iso first", iso.display());
    }
    let logs = repo_root.join("out/logs");
    fs::create_dir_all(&logs).context("failed to create out/logs")?;
    let log_path = logs.join("qemu-boot.log");
    let serial_log_path = logs.join("qemu-serial.log");
    let serial_arg = format!(
        "file:{}",
        serial_log_path
            .to_str()
            .ok_or_else(|| anyhow!("invalid qemu serial log path"))?
    );

    run_cmd(
        repo_root,
        "qemu-system-x86_64",
        &[
            "-m",
            "1024",
            "-drive",
            &format!(
                "file={},if=none,id=mattos-cd,media=cdrom,readonly=on",
                iso.to_str().ok_or_else(|| anyhow!("invalid ISO path"))?
            ),
            "-device",
            "virtio-scsi-pci,id=mattos-scsi",
            "-device",
            "scsi-cd,drive=mattos-cd,bus=mattos-scsi.0,bootindex=1",
            "-boot",
            "d",
            "-serial",
            serial_arg.as_str(),
            "-D",
            log_path
                .to_str()
                .ok_or_else(|| anyhow!("invalid qemu log path"))?,
        ],
    )
}

fn copy_runtime_dependencies(binary: &Path, rootfs: &Path) -> Result<()> {
    let library_path = std::env::join_paths([
        rootfs.join("usr/lib/x86_64-linux-gnu"),
        rootfs.join("usr/lib/x86_64-linux-gnu/systemd"),
        rootfs.join("lib/x86_64-linux-gnu"),
        rootfs.join("usr/lib"),
        rootfs.join("lib"),
    ])
    .context("failed to construct rootfs runtime library path")?;
    let output = Command::new("ldd")
        .arg(binary)
        .env("LD_LIBRARY_PATH", library_path)
        .output()
        .with_context(|| {
            format!(
                "failed to inspect runtime dependencies for {}",
                binary.display()
            )
        })?;
    if !output.status.success() {
        return Ok(());
    }
    let text = String::from_utf8(output.stdout).context("ldd output was not UTF-8")?;

    for token in text.split_whitespace() {
        if !token.starts_with('/') {
            continue;
        }
        let src = Path::new(token);
        if !src.exists() {
            continue;
        }
        if src.starts_with(rootfs) {
            continue;
        }
        let rel = src.strip_prefix("/").unwrap_or(src);
        let dst = rootfs.join(rel);
        if dst.exists() {
            continue;
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::copy(src, &dst)
            .with_context(|| format!("failed to copy runtime dependency {}", src.display()))?;
    }

    Ok(())
}

fn run_cmd(cwd: &Path, program: &str, args: &[&str]) -> Result<()> {
    let status = run_cmd_status(cwd, program, args)?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "command failed with status {status}: {} {}",
            program,
            args.join(" ")
        )
    }
}

fn run_cmd_status(cwd: &Path, program: &str, args: &[&str]) -> Result<std::process::ExitStatus> {
    let mut command = Command::new(program);
    let scheduler_args = scheduler_command_args(args);
    command.args(&scheduler_args).current_dir(cwd);
    apply_reproducible_process_environment(&mut command);
    apply_mattos_tmp_environment(&mut command, cwd)?;
    apply_scheduler_parallelism(&mut command);
    apply_mattos_sysroot_environment(&mut command, cwd, program, &[])?;
    let display = effective_command_display(program, &scheduler_args);
    performance::run_logged_command(&mut command, &display)
}

fn run_cmd_with_env(
    cwd: &Path,
    program: &str,
    args: &[&str],
    tool_env: Option<&LocalToolEnv>,
) -> Result<()> {
    let mut cmd = Command::new(program);
    let scheduler_args = scheduler_command_args(args);
    cmd.args(&scheduler_args).current_dir(cwd);
    apply_reproducible_process_environment(&mut cmd);
    apply_mattos_tmp_environment(&mut cmd, cwd)?;
    apply_scheduler_parallelism(&mut cmd);

    if let Some(env) = tool_env {
        let current_path = std::env::var("PATH").unwrap_or_default();
        let composed_path = format!("{}:{}", env.tool_bin_dir.display(), current_path);
        let current_ld = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let composed_ld = if current_ld.is_empty() {
            env.tool_lib_dir.display().to_string()
        } else {
            format!("{}:{current_ld}", env.tool_lib_dir.display())
        };
        let include = env.tool_include_dir.display().to_string();
        let lib = env.tool_lib_dir.display().to_string();

        cmd.env("PATH", composed_path)
            .env("LD_LIBRARY_PATH", composed_ld)
            .env(
                "BISON_PKGDATADIR",
                env.bison_pkg_data_dir.display().to_string(),
            )
            .env("M4", env.m4_bin.display().to_string())
            .env("CFLAGS", format!("-I{include}"))
            .env("HOSTCFLAGS", format!("-I{include}"))
            .env("LDFLAGS", format!("-L{lib}"))
            .env("HOSTLDFLAGS", format!("-L{lib}"));
    }
    apply_mattos_sysroot_environment(&mut cmd, cwd, program, &[])?;

    let display = effective_command_display(program, &scheduler_args);
    let status = performance::run_logged_command(&mut cmd, &display)?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "command failed with status {status}: {} {}",
            program,
            args.join(" ")
        )
    }
}

fn run_cmd_with_env_overrides(
    cwd: &Path,
    program: &str,
    args: &[&str],
    env_overrides: &[(&str, String)],
) -> Result<()> {
    let mut cmd = Command::new(program);
    let scheduler_args = scheduler_command_args(args);
    cmd.args(&scheduler_args).current_dir(cwd);
    apply_reproducible_process_environment(&mut cmd);
    for (key, value) in env_overrides {
        cmd.env(key, value);
    }
    apply_mattos_tmp_environment(&mut cmd, cwd)?;
    apply_scheduler_parallelism(&mut cmd);
    apply_mattos_sysroot_environment(&mut cmd, cwd, program, env_overrides)?;

    let display = effective_command_display(program, &scheduler_args);
    let status = performance::run_logged_command(&mut cmd, &display)?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "command failed with status {status}: {} {}",
            program,
            args.join(" ")
        )
    }
}

fn apply_reproducible_process_environment(command: &mut Command) {
    command
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH);
}

fn mattos_build_tmp(repo_root: &Path) -> PathBuf {
    repo_root.join(MATTOS_BUILD_TMP_RELATIVE)
}

fn mattos_tmp_min_free_bytes() -> u64 {
    // Unit tests exercise routing, writability, and concurrency inside
    // tempfile-backed filesystems. Their result must not depend on how full
    // the host's /tmp happens to be. Production builds retain the 4 GiB guard.
    if cfg!(test) {
        0
    } else {
        MIN_MATTOS_TMP_FREE_BYTES
    }
}

fn ensure_mattos_build_tmp(repo_root: &Path) -> Result<PathBuf> {
    let directory = mattos_build_tmp(repo_root);
    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "failed to create MattOS build temp directory {}",
            directory.display()
        )
    })?;
    let free_bytes = free_bytes_at(&directory)?;
    let required_free_bytes = mattos_tmp_min_free_bytes();
    if free_bytes < required_free_bytes {
        bail!(
            "MattOS build temp directory {} has only {} free bytes; at least {} are required",
            directory.display(),
            free_bytes,
            required_free_bytes
        );
    }

    // `build all` prepares commands from multiple scheduler threads inside one
    // mattos-build process. A PID-only probe name lets those threads delete one
    // another's probe. Give every invocation a process-local unique sequence so
    // strict cleanup remains meaningful without serializing command setup.
    let sequence = MATTOS_TMP_PROBE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let probe = directory.join(format!(".write-probe-{}-{sequence}", std::process::id()));
    fs::write(&probe, b"mattos-build temp directory probe\n").with_context(|| {
        format!(
            "MattOS build temp directory is not writable: {}",
            directory.display()
        )
    })?;
    fs::remove_file(&probe).with_context(|| {
        format!(
            "failed to remove MattOS build temp probe {}",
            probe.display()
        )
    })?;
    Ok(directory)
}

fn free_bytes_at(path: &Path) -> Result<u64> {
    let path = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .context("invalid MattOS temp path")?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return Err(anyhow!(
            "failed to inspect free space for MattOS temp directory"
        ));
    }
    let stats = unsafe { stats.assume_init() };
    Ok((stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64))
}

fn apply_mattos_tmp_environment(command: &mut Command, cwd: &Path) -> Result<()> {
    let Some(repo_root) = cwd.ancestors().find(|candidate| {
        candidate
            .join("src/tools/mattos-build/Cargo.toml")
            .is_file()
    }) else {
        return Ok(());
    };
    let directory = ensure_mattos_build_tmp(repo_root)?;
    // The repository-owned directory deliberately takes precedence over a
    // caller's TMPDIR: build correctness must not depend on a full host /tmp.
    command.env("TMPDIR", directory);
    Ok(())
}

fn effective_command_display(program: &str, args: &[String]) -> String {
    let argv = std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    format!(
        "{}\n[mattos-command] child_jobs={} argv={argv:?}",
        argv.join(" "),
        scheduler::child_job_limit()
    )
}

fn scheduler_command_args(args: &[&str]) -> Vec<String> {
    // A very small cgroup memory ceiling can yield no parallel CPU grant.
    // External build tools require a positive jobs value; retain serial
    // progress while the cgroup remains the hard memory safety boundary.
    let limit = scheduler::child_job_limit().max(1);
    let experimental_limit = EXPERIMENTAL_CHILD_JOBS.with(Cell::get);
    let mut previous_sets_jobs = false;
    args.iter()
        .map(|argument| {
            let normalized =
                if previous_sets_jobs && argument.bytes().all(|byte| byte.is_ascii_digit()) {
                    experimental_limit
                        .unwrap_or_else(|| argument.parse::<usize>().unwrap().min(limit))
                        .to_string()
                } else if argument.starts_with("-j")
                    && argument.len() > 2
                    && argument[2..].bytes().all(|byte| byte.is_ascii_digit())
                {
                    format!(
                        "-j{}",
                        experimental_limit
                            .unwrap_or_else(|| argument[2..].parse::<usize>().unwrap().min(limit))
                    )
                } else if let Some(value) = argument
                    .strip_prefix("--jobs=")
                    .or_else(|| argument.strip_prefix("--parallel="))
                    .filter(|value| value.bytes().all(|byte| byte.is_ascii_digit()))
                {
                    let option = argument.split_once('=').unwrap().0;
                    format!(
                        "{option}={}",
                        experimental_limit
                            .unwrap_or_else(|| value.parse::<usize>().unwrap().min(limit))
                    )
                } else {
                    (*argument).to_string()
                };
            previous_sets_jobs = matches!(*argument, "-j" | "--jobs" | "--parallel");
            normalized
        })
        .collect()
}

fn apply_scheduler_parallelism(command: &mut Command) {
    // External build tools uniformly reject a zero job count.  A tight
    // memory admission budget may intentionally grant no parallel token, but
    // it must still permit one serial child inside the cgroup ceiling.
    let tokens = scheduler::child_job_limit().max(1).to_string();
    command
        .env("MAKEFLAGS", format!("-j{tokens}"))
        .env("CARGO_BUILD_JOBS", &tokens)
        .env("CMAKE_BUILD_PARALLEL_LEVEL", &tokens)
        .env("MESON_NUM_PROCESSES", &tokens)
        .env("NINJAFLAGS", format!("-j{tokens}"));
}

fn apply_mattos_sysroot_environment(
    command: &mut Command,
    cwd: &Path,
    program: &str,
    overrides: &[(&str, String)],
) -> Result<()> {
    let Some(repo_root) = cwd.ancestors().find(|candidate| {
        candidate
            .join("src/tools/mattos-build/Cargo.toml")
            .is_file()
    }) else {
        return Ok(());
    };
    let sysroot = repo_root.join("out/sysroot");
    if !sysroot.join("usr/include/stdio.h").is_file()
        || cwd.starts_with(repo_root.join("src/kernel/linux"))
        || cwd.starts_with(repo_root.join("out/build/linux"))
        || cwd.starts_with(repo_root.join("out/build/glibc"))
        || cwd.starts_with(repo_root.join("src/system/libc/glibc"))
    {
        return Ok(());
    }
    let sysroot_flag = format!("--sysroot={}", sysroot.display());
    let value_for = |key: &str| {
        overrides
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var(key).ok())
            .unwrap_or_default()
    };
    for key in ["CPPFLAGS", "CFLAGS", "CXXFLAGS", "LDFLAGS"] {
        let current = value_for(key);
        let mut value = if current.split_whitespace().any(|flag| flag == sysroot_flag) {
            current
        } else if current.is_empty() {
            sysroot_flag.clone()
        } else {
            format!("{current} {sysroot_flag}")
        };
        if matches!(key, "CFLAGS" | "CXXFLAGS") {
            let prefix_map = format!("-ffile-prefix-map={}=/usr/src/mattos", repo_root.display());
            if !value.split_whitespace().any(|flag| flag == prefix_map) {
                value.push_str(&format!(
                    " {prefix_map} -fdebug-prefix-map={}=/usr/src/mattos -fmacro-prefix-map={}=/usr/src/mattos",
                    repo_root.display(),
                    repo_root.display()
                ));
            }
        }
        command.env(key, value);
    }
    if program == "cargo" {
        let current = value_for("RUSTFLAGS");
        // Cargo fingerprints RUSTFLAGS verbatim and rustc incorporates codegen
        // options into crate identity.  Keep the linker argument independent of
        // the absolute checkout location while still resolving to this tree's
        // output-owned sysroot from Cargo's working directory.
        let relative = cwd
            .strip_prefix(repo_root)
            .context("Cargo working directory is outside the MattOS repository")?;
        let mut relative_sysroot = PathBuf::new();
        for component in relative.components() {
            if matches!(component, std::path::Component::Normal(_)) {
                relative_sysroot.push("..");
            }
        }
        relative_sysroot.push("out/sysroot");
        let rust_sysroot = format!(
            "-C link-arg=--sysroot={}",
            relative_sysroot.to_string_lossy()
        );
        let remap = format!(
            "--remap-path-prefix={}=/usr/src/mattos",
            repo_root.display()
        );
        let value = if current.contains(&rust_sysroot) {
            current
        } else if current.is_empty() {
            rust_sysroot
        } else {
            format!("{current} {rust_sysroot}")
        };
        let value = if value.contains(&remap) {
            value
        } else {
            format!("{value} {remap}")
        };
        command.env("RUSTFLAGS", value);
    }
    command.env("MATTOS_SYSROOT", &sysroot);
    Ok(())
}

fn run_cmd_output(cwd: &Path, program: &str, args: &[&str]) -> Result<Output> {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    apply_reproducible_process_environment(&mut command);
    apply_mattos_tmp_environment(&mut command, cwd)?;
    command
        .output()
        .with_context(|| format!("failed to spawn command: {program}"))
}

fn run_cmd_capture(cwd: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = run_cmd_output(cwd, program, args)?;
    if !output.status.success() {
        bail!(
            "command failed with status {}: {} {}",
            output.status,
            program,
            args.join(" ")
        );
    }
    let text = String::from_utf8(output.stdout).context("stdout was not valid UTF-8")?;
    Ok(text)
}
