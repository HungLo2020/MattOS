use super::*;

// Package audit and ELF/runtime ownership validation.  This module intentionally
// reuses the parent package walkers and the shared ELF fact infrastructure rather
// than introducing parallel payload inspection policy.

pub(crate) fn validate_no_mutable_package_state(staging: &Path) -> Result<()> {
    for forbidden in [
        "var/lib/dpkg/status",
        "var/lib/dpkg/available",
        "var/lib/dpkg/lock",
        "var/lib/dpkg/lock-frontend",
        "var/lib/apt/lists/lock",
        "var/cache/apt/archives/lock",
    ] {
        if path_entry_exists(&staging.join(forbidden)) {
            bail!("mutable package-manager state must not be packaged: /{forbidden}")
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn validate_migrated_bootstrap_absent(manifest: &[String]) -> Result<()> {
    for row in manifest {
        let destination = row.split('\t').next();
        if destination == Some("/usr/bin/tar") {
            bail!("migrated GNU tar remains in mattos-bootstrap-runtime")
        }
        if let Some(name) = destination
            .and_then(|path| Path::new(path).file_name())
            .and_then(OsStr::to_str)
        {
            if MIGRATED_BOOTSTRAP_SONAME_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                bail!("migrated runtime library {name} remains in mattos-bootstrap-runtime")
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn command_text(program: &str, args: &[&str], path: &Path) -> Result<Option<String>> {
    let output = Command::new(program)
        .args(args)
        .arg(path)
        .output()
        .with_context(|| format!("failed to run {program} on {}", path.display()))?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8(output.stdout)?))
}

#[cfg(test)]
fn dynamic_values(text: &str, label: &str) -> Vec<String> {
    let marker = format!("{label}: [");
    text.lines()
        .filter_map(|line| {
            line.split_once(&marker)
                .map(|(_, value)| value.trim_end_matches(']').to_string())
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn bootstrap_source_attribution(
    name: &str,
) -> (
    Option<&'static str>,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match name {
        "tar" => (Some("GNU tar"), "A", "tar", "medium", "high"),
        "libattr.so.1" => (
            Some("Linux extended attributes"),
            "A",
            "libattr1",
            "low",
            "high",
        ),
        "libacl.so.1" => (Some("Linux ACL utilities"), "A", "libacl1", "low", "high"),
        "libbsd.so.0" => (Some("libbsd"), "A", "libbsd0", "low", "high"),
        "libbz2.so.1.0" => (Some("bzip2"), "A", "libbz2-1.0", "low", "high"),
        "libc.so.6" | "libm.so.6" | "ld-linux-x86-64.so.2" => (
            Some("glibc"),
            "D",
            "future MattOS libc runtime",
            "very-high",
            "high",
        ),
        "libcap.so.2" => (Some("libcap"), "A", "libcap2", "low", "high"),
        "libcrypt.so.1" => (Some("libxcrypt"), "A", "libcrypt1", "medium", "high"),
        "libcrypto.so.3" => (Some("OpenSSL"), "A", "mattos-libcrypto3", "high", "high"),
        "libssl.so.3" => (Some("OpenSSL"), "A", "libssl3t64", "high", "high"),
        "libelf.so.1" => (Some("elfutils"), "A", "libelf1t64", "medium", "high"),
        "libexpat.so.1" => (Some("Expat"), "A", "libexpat1", "low", "high"),
        "libgcc_s.so.1" => (
            Some("GCC runtime"),
            "D",
            "future MattOS compiler runtime",
            "very-high",
            "high",
        ),
        "liblz4.so.1" => (Some("LZ4"), "C", "liblz4-1", "low", "high"),
        "liblzma.so.5" => (Some("XZ Utils"), "C", "liblzma5", "low", "high"),
        "libmd.so.0" => (Some("libmd"), "A", "libmd0", "low", "high"),
        "libpcre2-8.so.0" => (Some("PCRE2"), "A", "libpcre2-8-0", "medium", "high"),
        "libselinux.so.1" => (
            Some("SELinux userspace"),
            "A",
            "libselinux1",
            "high",
            "high",
        ),
        "libstdc++.so.6" => (
            Some("GCC libstdc++ runtime"),
            "D",
            "future MattOS C++ runtime",
            "very-high",
            "high",
        ),
        "libxxhash.so.0" => (Some("xxHash"), "C", "libxxhash0", "low", "high"),
        "libz.so.1" => (Some("zlib"), "A", "zlib1g", "low", "high"),
        "libzstd.so.1" => (Some("Zstandard"), "A", "libzstd1", "low", "high"),
        _ => (None, "E", "unresolved", "unknown", "low"),
    }
}

#[cfg(test)]
pub(crate) fn bootstrap_consumers(repo_root: &Path) -> Result<BTreeMap<String, Vec<BootstrapConsumer>>> {
    let staging_root = repo_root.join("out/packages/staging");
    let mut graph = BTreeMap::<String, Vec<BootstrapConsumer>>::new();
    if !staging_root.is_dir() {
        bail!(
            "package staging tree missing at {}; build packages first",
            staging_root.display()
        );
    }
    let mut package_dirs = fs::read_dir(&staging_root)?.collect::<std::io::Result<Vec<_>>>()?;
    package_dirs.sort_by_key(|entry| entry.file_name());
    for package_entry in package_dirs {
        let package_root = package_entry.path();
        if !package_root.is_dir() {
            continue;
        }
        let package = package_entry.file_name().to_string_lossy().to_string();
        walk_tree(&package_root, &mut |path, metadata| {
            if !metadata.is_file() || path.starts_with(package_root.join("DEBIAN")) {
                return Ok(());
            }
            let Some(dynamic) = command_text("readelf", &["-d"], path)? else {
                return Ok(());
            };
            let consumer_path = format!("/{}", path.strip_prefix(&package_root)?.display());
            for needed in dynamic_values(&dynamic, "Shared library") {
                graph.entry(needed).or_default().push(BootstrapConsumer {
                    package: package.clone(),
                    path: consumer_path.clone(),
                });
            }
            Ok(())
        })?;
    }
    for consumers in graph.values_mut() {
        consumers.sort_by(|a, b| (&a.package, &a.path).cmp(&(&b.package, &b.path)));
        consumers.dedup_by(|a, b| a.package == b.package && a.path == b.path);
    }
    Ok(graph)
}

#[cfg(test)]
pub(crate) fn confirmed_host_package(source: &Path) -> Result<Option<String>> {
    let canonical = fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let output = Command::new("dpkg-query")
        .args(["-S"])
        .arg(&canonical)
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(String::from_utf8(output.stdout)?
        .lines()
        .next()
        .and_then(|line| {
            line.split_once(": ")
                .map(|(package, _)| package.to_string())
        }))
}

pub(crate) fn generate_bootstrap_audit(repo_root: &Path) -> Result<()> {
    let report = BootstrapAuditReport {
        schema_version: 1,
        package: "retired".to_string(),
        snapshot: "runtime-source-closure-complete".to_string(),
        entry_count: 0,
        payload_bytes: 0,
        classification_totals: BTreeMap::new(),
        entries: Vec::new(),
    };
    let reports = repo_root.join("out/reports");
    fs::create_dir_all(&reports)?;
    let destination = reports.join("bootstrap-runtime-audit.toml");
    fs::write(&destination, toml::to_string_pretty(&report)?)?;
    println!(
        "generated zero-entry retired bootstrap audit at {}",
        destination.display()
    );
    Ok(())
}


pub(crate) fn runtime_libraries_for_spec(repo_root: &Path, spec: &PackageSpec) -> Result<Vec<String>> {
    match spec.name {
        "mattos-brush" => ldd_sonames(
            &repo_root.join("out/build/brush/cargo-target/release/brush"),
            None,
        ),
        "coreutils" => ldd_sonames(&resolve_coreutils_multicall(repo_root)?, None),
        "curl" => {
            let install = repo_root.join("out/build/curl/install");
            let openssl = repo_root.join("out/build/openssl/install/usr/lib/x86_64-linux-gnu");
            let zlib = repo_root.join("out/build/zlib/install/usr/lib/x86_64-linux-gnu");
            let zstd = repo_root.join("out/build/zstd/install/usr/lib/x86_64-linux-gnu");
            ldd_sonames_many(
                &[
                    install.join("usr/bin/curl"),
                    install.join("usr/lib/x86_64-linux-gnu/libcurl.so.4.8.0"),
                ],
                &[
                    install.join("usr/lib/x86_64-linux-gnu"),
                    openssl,
                    zlib,
                    zstd,
                ],
            )
        }
        name if matches!(
            name,
            "libc6"
                | "libc-bin"
                | "libgcc-s1"
                | "libstdc++6"
                | "binutils"
                | "mattos-gcc-common"
                | "cpp"
                | "gcc"
                | "g++"
                | "make"
        ) =>
        {
            runtime_libraries_in_staging(repo_root, name)
        }
        "dpkg" => {
            let install = repo_root.join("out/build/dpkg/install");
            let zlib = repo_root.join("out/build/zlib/install/usr/lib/x86_64-linux-gnu");
            let bzip2 = repo_root.join("out/build/bzip2/install/usr/lib/x86_64-linux-gnu");
            let xz = repo_root.join("out/build/xz/install/usr/lib/x86_64-linux-gnu");
            let zstd = repo_root.join("out/build/zstd/install/usr/lib/x86_64-linux-gnu");
            let libmd = repo_root.join("out/build/libmd/install/usr/lib/x86_64-linux-gnu");
            let selinux = repo_root.join("out/build/selinux/install/usr/lib/x86_64-linux-gnu");
            let pcre2 = repo_root.join("out/build/pcre2/install/usr/lib/x86_64-linux-gnu");
            ldd_sonames_many(
                &[
                    install.join("usr/bin/dpkg"),
                    install.join("usr/bin/dpkg-deb"),
                    install.join("usr/bin/dpkg-query"),
                    install.join("usr/bin/dpkg-divert"),
                    install.join("usr/bin/dpkg-realpath"),
                    install.join("usr/bin/dpkg-split"),
                    install.join("usr/bin/dpkg-statoverride"),
                    install.join("usr/bin/dpkg-trigger"),
                    install.join("usr/bin/update-alternatives"),
                    install.join("usr/sbin/start-stop-daemon"),
                ],
                &[zlib, bzip2, xz, zstd, libmd, selinux, pcre2],
            )
        }
        "libapt-pkg7.0" => {
            let install = repo_root.join("out/build/apt/install");
            let systemd = repo_root.join("out/build/systemd/install/usr/lib/x86_64-linux-gnu");
            let zlib = repo_root.join("out/build/zlib/install/usr/lib/x86_64-linux-gnu");
            let bzip2 = repo_root.join("out/build/bzip2/install/usr/lib/x86_64-linux-gnu");
            let lz4 = repo_root.join("out/build/lz4/install/usr/lib/x86_64-linux-gnu");
            let xz = repo_root.join("out/build/xz/install/usr/lib/x86_64-linux-gnu");
            let xxhash = repo_root.join("out/build/xxhash/install/usr/lib/x86_64-linux-gnu");
            let zstd = repo_root.join("out/build/zstd/install/usr/lib/x86_64-linux-gnu");
            let openssl = repo_root.join("out/build/openssl/install/usr/lib/x86_64-linux-gnu");
            ldd_sonames_many(
                &[install.join("usr/lib/x86_64-linux-gnu/libapt-pkg.so.7.0.0")],
                &[
                    install.join("usr/lib/x86_64-linux-gnu"),
                    systemd,
                    zlib,
                    bzip2,
                    lz4,
                    xz,
                    xxhash,
                    zstd,
                    openssl,
                ],
            )
        }
        "apt" => {
            let install = repo_root.join("out/build/apt/install");
            let systemd = repo_root.join("out/build/systemd/install/usr/lib/x86_64-linux-gnu");
            let zlib = repo_root.join("out/build/zlib/install/usr/lib/x86_64-linux-gnu");
            let bzip2 = repo_root.join("out/build/bzip2/install/usr/lib/x86_64-linux-gnu");
            let lz4 = repo_root.join("out/build/lz4/install/usr/lib/x86_64-linux-gnu");
            let xz = repo_root.join("out/build/xz/install/usr/lib/x86_64-linux-gnu");
            let xxhash = repo_root.join("out/build/xxhash/install/usr/lib/x86_64-linux-gnu");
            let zstd = repo_root.join("out/build/zstd/install/usr/lib/x86_64-linux-gnu");
            let openssl = repo_root.join("out/build/openssl/install/usr/lib/x86_64-linux-gnu");
            ldd_sonames_many(
                &[
                    install.join("usr/bin/apt"),
                    install.join("usr/bin/apt-cache"),
                    install.join("usr/bin/apt-config"),
                    install.join("usr/bin/apt-get"),
                    install.join("usr/bin/apt-mark"),
                    install.join("usr/lib/apt/apt-helper"),
                    install.join("usr/lib/apt/methods/copy"),
                    install.join("usr/lib/apt/methods/file"),
                    install.join("usr/lib/apt/methods/http"),
                    install.join("usr/lib/apt/methods/https"),
                    install.join("usr/lib/apt/methods/store"),
                    install.join("usr/lib/x86_64-linux-gnu/libapt-private.so.0.0.0"),
                ],
                &[
                    install.join("usr/lib/x86_64-linux-gnu"),
                    systemd,
                    zlib,
                    bzip2,
                    lz4,
                    xz,
                    xxhash,
                    zstd,
                    openssl,
                ],
            )
        }
        "libgpg-error0" | "libgcrypt20" | "libassuan9" | "libksba8" | "libnpth0" => {
            let component = match spec.name {
                "libgpg-error0" => "libgpg-error",
                "libgcrypt20" => "libgcrypt",
                "libassuan9" => "libassuan",
                "libksba8" => "libksba",
                "libnpth0" => "npth",
                _ => unreachable!(),
            };
            let install = repo_root.join("out/build").join(component).join("install");
            let libdir = install.join("usr/lib/x86_64-linux-gnu");
            let mut search = vec![libdir.clone()];
            if component != "libgpg-error" && component != "npth" {
                search.push(
                    repo_root.join("out/build/libgpg-error/install/usr/lib/x86_64-linux-gnu"),
                );
            }
            ldd_sonames_many(
                &[libdir.join(match spec.name {
                    "libgpg-error0" => "libgpg-error.so.0",
                    "libgcrypt20" => "libgcrypt.so.20",
                    "libassuan9" => "libassuan.so.9",
                    "libksba8" => "libksba.so.8",
                    "libnpth0" => "libnpth.so.0",
                    _ => unreachable!(),
                })],
                &search,
            )
        }
        "gpgv" => {
            let install = repo_root.join("out/build/gpgv/install");
            let mut search = vec![install.join("usr/lib/x86_64-linux-gnu")];
            for component in [
                "libgpg-error",
                "libgcrypt",
                "libassuan",
                "libksba",
                "npth",
                "zlib",
            ] {
                search.push(
                    repo_root
                        .join("out/build")
                        .join(component)
                        .join("install/usr/lib/x86_64-linux-gnu"),
                );
            }
            ldd_sonames_many(&[install.join("usr/bin/gpgv")], &search)
        }
        "gnupg" => {
            let install = repo_root.join("out/build/gpgv/install");
            let mut search = vec![install.join("usr/lib/x86_64-linux-gnu")];
            for component in [
                "libgpg-error",
                "libgcrypt",
                "libassuan",
                "libksba",
                "npth",
                "zlib",
            ] {
                search.push(
                    repo_root
                        .join("out/build")
                        .join(component)
                        .join("install/usr/lib/x86_64-linux-gnu"),
                );
            }
            ldd_sonames_many(
                &[
                    install.join("usr/bin/gpg"),
                    install.join("usr/bin/gpg-agent"),
                    install.join("usr/bin/gpgconf"),
                ],
                &search,
            )
        }
        name if matches!(
            name,
            "mattos-libtinfow6"
                | "libncursesw6"
                | "ncurses-bin"
                | "libkmod2"
                | "kmod"
                | "mattos-libproc2"
                | "procps"
                | "libsystemd0"
                | "libudev1"
                | "libexpat1"
                | "libcap2"
                | "libattr1"
                | "libacl1"
                | "zlib1g"
                | "libbz2-1.0"
                | "liblz4-1"
                | "liblzma5"
                | "libxxhash0"
                | "libmd0"
                | "libbsd0"
                | "libzstd1"
                | "mattos-libcrypto3"
                | "libssl3t64"
                | "libelf1t64"
                | "libpcre2-8-0"
                | "libselinux1"
                | "libcrypt1"
                | "libblkid1"
                | "libmount1"
                | "libsmartcols1"
                | "libuuid1"
                | "libfdisk1"
                | "mount"
                | "util-linux"
                | "gzip"
                | "bzip2"
                | "xz-utils"
                | "zstd"
                | "patch"
                | "libmagic1"
                | "file"
                | "less"
                | "git"
                | "openssh-client"
                | "openssh-server"
                | "libffi8"
                | "libffi-dev"
                | "libwayland-client0"
                | "libwayland-server0"
                | "libwayland-egl1"
                | "libxkbcommon0"
                | "libvulkan1"
                | "libvulkan-dev"
                | "vulkan-tools"
                | "libxau6"
                | "libxdmcp6"
                | "libxcb1"
                | "libx11-6"
                | "libxext6"
                | "libglvnd0"
                | "libglx0"
                | "libgl1"
                | "libopengl0"
                | "libegl1"
                | "libgles1"
                | "libgles2"
                | "libegl-mesa0"
                | "libnvidia-gl-595"
                | "libnvidia-compute-595"
                | "libnvidia-encode-595"
                | "libnvidia-decode-595"
                | "nvidia-utils-595"
                | "libpython3.14"
                | "python3"
                | "python3-venv"
                | "python3-dev"
                | "libllvm22"
                | "llvm"
                | "llvm-dev"
                | "clang"
                | "lld"
                | "rustc"
                | "cargo"
                | "tar"
                | "dbus-broker"
                | "libpam0g"
                | "mattos-libpam-misc0"
                | "libpam-modules"
                | "libpam-runtime"
                | "passwd"
                | "mattos-sudo-rs"
                | "login"
                | "iproute2"
                | "iputils-ping"
                | "btrfs-progs"
                | "dosfstools"
                | "e2fsprogs"
                | "mattos-installer"
        ) =>
        {
            runtime_libraries_in_staging(repo_root, name)
        }
        _ => Ok(Vec::new()),
    }
}

fn runtime_libraries_in_staging(repo_root: &Path, package: &str) -> Result<Vec<String>> {
    let staging = repo_root.join("out/packages/staging").join(package);
    let mut binaries = Vec::new();
    walk_tree(&staging, &mut |path, metadata| {
        if metadata.is_file() && !path.starts_with(staging.join("DEBIAN")) {
            if let Some(facts) = crate::elf_cache::inspect(repo_root, path)?
                && matches!(facts.elf_type.as_str(), "DYN" | "EXEC")
            {
                binaries.push(path.to_path_buf());
            }
        }
        Ok(())
    })?;
    let library_dirs = [
        staging.join("usr/lib/x86_64-linux-gnu"),
        repo_root.join("out/sysroot/usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "apt").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "curl").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "libffi").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "wayland").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "mesa").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "vulkan-loader").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "xkbcommon").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "cpython").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "llvm").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "ncurses").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "kmod").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "procps-ng").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "linux-pam").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "systemd").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "lz4").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "xz").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "xxhash").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "zstd").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "openssl").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "elfutils").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "libmd").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "libbsd").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "pcre2").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "selinux").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "libxcrypt").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "util-linux").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "zlib").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "bzip2").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "file").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "git").join("usr/lib/x86_64-linux-gnu"),
        component_install(repo_root, "openssh").join("usr/lib/x86_64-linux-gnu"),
    ];
    ldd_sonames_many(&binaries, &library_dirs)
}

fn ldd_sonames_many(binaries: &[PathBuf], library_dirs: &[PathBuf]) -> Result<Vec<String>> {
    let library_path = if library_dirs.is_empty() {
        None
    } else {
        Some(std::env::join_paths(library_dirs)?)
    };
    let mut libraries = BTreeSet::new();
    for binary in binaries {
        let mut command = Command::new("ldd");
        command.arg(binary);
        if let Some(path) = &library_path {
            command.env("LD_LIBRARY_PATH", path);
        }
        let output = command
            .output()
            .with_context(|| format!("failed to inspect {} with ldd", binary.display()))?;
        let text = String::from_utf8(output.stdout)?;
        if !output.status.success() || text.contains("not found") {
            bail!(
                "unresolved ELF dependency for {}:\n{text}",
                binary.display()
            )
        }
        for line in text.lines() {
            let token = line.trim().split_whitespace().next().unwrap_or_default();
            if token.contains(".so") {
                libraries.insert(token.to_string());
            }
        }
    }
    Ok(libraries.into_iter().collect())
}

fn ldd_sonames(binary: &Path, library_path: Option<&Path>) -> Result<Vec<String>> {
    let mut command = Command::new("ldd");
    command.arg(binary);
    if let Some(library_path) = library_path {
        command.env("LD_LIBRARY_PATH", library_path);
    }
    let output = command.output().with_context(|| {
        format!(
            "failed to inspect runtime libraries for {}",
            binary.display()
        )
    })?;
    if !output.status.success() {
        bail!("ldd failed for {}", binary.display());
    }
    let mut libraries = BTreeSet::new();
    for line in String::from_utf8(output.stdout)?.lines() {
        let token = line.trim().split_whitespace().next().unwrap_or_default();
        if token.contains(".so") {
            libraries.insert(token.to_string());
        }
    }
    Ok(libraries.into_iter().collect())
}


pub(crate) fn detect_staging_collisions(staging_root: &Path, specs: &[PackageSpec]) -> Result<()> {
    let mut owners: BTreeMap<PathBuf, (&str, bool)> = BTreeMap::new();
    for spec in specs {
        let root = staging_root.join(spec.name);
        walk_tree(&root, &mut |path, meta| {
            if path.starts_with(root.join("DEBIAN")) {
                return Ok(());
            }
            let rel = path.strip_prefix(&root)?.to_path_buf();
            let is_dir = meta.is_dir();
            if let Some((owner, owner_is_dir)) = owners.get(&rel) {
                if !is_dir || !owner_is_dir {
                    bail!(
                        "package ownership collision at /{}: {} and {}",
                        rel.display(),
                        owner,
                        spec.name
                    )
                }
            } else {
                owners.insert(rel, (spec.name, is_dir));
            }
            Ok(())
        })?;
    }
    Ok(())
}

pub(crate) fn validate_staged_runtime_ownership(repo_root: &Path, specs: &[PackageSpec]) -> Result<()> {
    let staging_root = repo_root.join("out/packages/staging");
    let mut owners = BTreeMap::<String, &str>::new();
    let mut soname_owners = BTreeMap::<String, &str>::new();
    for spec in specs {
        let root = staging_root.join(spec.name);
        walk_tree(&root, &mut |path, metadata| {
            // `symlink_metadata` deliberately preserves package links, including
            // development links whose target is supplied by another package.
            // Such links can be dangling in an individual staging tree, so they
            // are valid ownership entries but are not ELF files to inspect.
            if metadata.is_file() && !path.starts_with(root.join("DEBIAN")) {
                if let Some(name) = path.file_name().and_then(OsStr::to_str) {
                    owners.entry(name.to_string()).or_insert(spec.name);
                }
                // Flatpak's bundled OSTree repository is an application/runtime
                // payload, not part of MattOS's host ELF namespace.  Its object
                // store legitimately contains copies of libraries such as
                // libm.so.6; inspecting those objects as package ELF outputs
                // creates false cross-package SONAME ownership collisions.
                if !path.starts_with(root.join("var/lib/flatpak"))
                    && let Some(facts) = crate::elf_cache::inspect(repo_root, path)?
                {
                    if let Some(soname) = facts.soname {
                        if let Some(owner) = soname_owners.get(&soname) {
                            if *owner != spec.name {
                                bail!(
                                    "SONAME {soname} has multiple package owners: {owner} and {}",
                                    spec.name
                                );
                            }
                        } else {
                            soname_owners.insert(soname, spec.name);
                        }
                    }
                }
            }
            Ok(())
        })?;
    }
    for spec in specs {
        for soname in runtime_libraries_for_spec(repo_root, spec)? {
            let name = Path::new(&soname)
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or(&soname);
            if name.starts_with("linux-vdso.so") {
                continue;
            }
            let owner = soname_owners
                .get(name)
                .or_else(|| owners.get(name))
                .ok_or_else(|| anyhow!("{} has unowned runtime dependency {name}", spec.name))?;
            if *owner != spec.name && !effective_dependencies(spec).contains(owner) {
                bail!(
                    "{} uses {name} from {owner} without declaring that dependency",
                    spec.name
                )
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn runtime_ownership_audit_ignores_dangling_package_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let spec = package_specs()
            .into_iter()
            .find(|spec| spec.name == "libexpat1")
            .unwrap();
        let root = temp.path().join("out/packages/staging").join(spec.name);
        fs::create_dir_all(root.join("usr/lib")).unwrap();
        symlink("libexpat.so.1", root.join("usr/lib/libexpat.so")).unwrap();

        validate_staged_runtime_ownership(temp.path(), &[spec]).unwrap();
    }
}
