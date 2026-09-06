fn build_expat(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/expat/expat");
    if !source.join("CMakeLists.txt").is_file() {
        bail!(
            "Expat source not found in {}; run upstream import expat first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/expat");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let options = [
        "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_INSTALL_PREFIX=/usr",
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
        "-DEXPAT_SHARED_LIBS=ON",
        "-DEXPAT_BUILD_TOOLS=OFF",
        "-DEXPAT_BUILD_EXAMPLES=OFF",
        "-DEXPAT_BUILD_TESTS=OFF",
        "-DEXPAT_BUILD_DOCS=OFF",
        "-DEXPAT_BUILD_FUZZERS=OFF",
        "-DEXPAT_BUILD_PKGCONFIG=ON",
    ];
    let state = fs::read_to_string(repo_root.join("upstream/state/expat.toml"))
        .context("failed to read Expat upstream state")?;
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    if !build_dir.join("CMakeCache.txt").is_file() {
        let mut args = vec![
            "-S",
            path_str(&source)?,
            "-B",
            path_str(&build_dir)?,
            "-G",
            "Ninja",
        ];
        args.extend(options);
        run_cmd(repo_root, "cmake", &args)?;
    }
    run_cmd(
        repo_root,
        "cmake",
        &["--build", path_str(&build_dir)?, "--parallel", "4"],
    )?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build_dir)?],
        &[("DESTDIR", install_dir.display().to_string())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libexpat.so.1");
    if !soname.exists() {
        bail!("Expat install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_libcap(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/libcap");
    if !source.join("libcap/Makefile").is_file() {
        bail!(
            "libcap source not found in {}; run upstream import libcap first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/libcap");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/libcap.toml"))
        .context("failed to read libcap upstream state")?;
    let make_options = [
        "prefix=/usr",
        "lib=lib/x86_64-linux-gnu",
        "PTHREADS=no",
        "PAM_CAP=no",
        "GOLANG=no",
        "SHARED=yes",
        "USE_GPERF=yes",
    ];
    let stamp = format!("{state}\n{}\n", make_options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    let libcap_dir = source_copy.join("libcap");
    // Upstream's cap_magic.o rule includes cap_names.h indirectly without listing
    // it as a prerequisite, so this focused library build must remain serial.
    let mut build_args = vec!["libcap.so"];
    build_args.extend(make_options);
    run_cmd(&libcap_dir, "make", &build_args)?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    let destdir = format!("DESTDIR={}", install_dir.display());
    let mut install_args = vec!["install-shared-cap", destdir.as_str()];
    install_args.extend(make_options);
    run_cmd(&libcap_dir, "make", &install_args)?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libcap.so.2");
    if !soname.exists() {
        bail!("libcap install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_attr(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/attr");
    if !source.join("configure.ac").is_file() {
        bail!(
            "attr source not found in {}; run upstream import attr first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/attr");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/attr.toml"))
        .context("failed to read attr upstream state")?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--disable-nls",
    ];
    let stamp = format!(
        "{state}\n{}\nattr-bootstrap={ATTR_UPSTREAM_COMMIT} {} {}\n",
        options.join("\n"),
        ATTR_RELEASE_ARCHIVE_URL,
        ATTR_RELEASE_ARCHIVE_SHA256,
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    let archive = ensure_attr_release_archive(&out_root)?;
    stage_attr_bootstrap_inputs(&source, &source_copy, &archive)?;
    // The imported Git files and generated release files have unrelated
    // timestamps.  Normalize the output mirror after staging so Automake does
    // not attempt a host-versioned regeneration merely because a macro was
    // copied a few milliseconds after aclocal.m4.
    run_cmd(
        &source_copy,
        "find",
        // Imported Git files and generated files copied from the verified
        // release archive otherwise retain unrelated timestamps.  Give every
        // source input one deterministic timestamp so Make cannot decide the
        // generated Makefile.in is stale and invoke host automake-1.16.
        &[".", "-type", "f", "-exec", "touch", "-c", "-d", "@0", "{}", "+"],
    )?;
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").is_file() {
        let configure = source_copy.join("configure");
        run_cmd(&build_dir, path_str(&configure)?, &options)?;
    }
    // The official distribution archive already supplies the generated
    // Autotools files.  Do not let timestamp differences from the imported
    // Git checkout trigger a host-versioned aclocal rebuild.
    run_cmd(&build_dir, "make", &["-j", "4", "MAKE_MAINTAINER_MODE="])?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        &build_dir,
        "make",
        &[
            "MAKE_MAINTAINER_MODE=",
            "install",
            &format!("DESTDIR={}", install_dir.display()),
        ],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libattr.so.1");
    let headers = install_dir.join("usr/include/attr");
    if !soname.exists() || !headers.join("error_context.h").is_file() {
        bail!(
            "attr install did not produce {} and its development headers",
            soname.display()
        );
    }
    copy_tree_contents(
        &install_dir.join("usr/include"),
        &repo_root.join("out/sysroot/usr/include"),
    )?;
    copy_tree_contents(
        &install_dir.join("usr/lib/x86_64-linux-gnu"),
        &repo_root.join("out/sysroot/usr/lib/x86_64-linux-gnu"),
    )?;
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

/// Obtains the official Attr v2.6.0 distribution archive in the Attr output
/// directory.  The archive is accepted only when its published SHA-256
/// matches, so an interrupted or substituted download cannot supply build
/// inputs.  `out/cache` is intentionally not used because this workspace
/// points it at the preserved reproduction baseline.
fn ensure_attr_release_archive(out_root: &Path) -> Result<PathBuf> {
    let bootstrap = out_root.join("bootstrap");
    let archive = bootstrap.join(format!("{ATTR_RELEASE_DIRECTORY}.tar.xz"));
    if archive.is_file() {
        verify_attr_release_archive(&archive)?;
        return Ok(archive);
    }

    fs::create_dir_all(&bootstrap)
        .with_context(|| format!("failed to create {}", bootstrap.display()))?;
    let temporary = bootstrap.join("attr-2.6.0.tar.xz.tmp");
    let temporary_arg = path_str(&temporary)?;
    run_cmd(
        out_root,
        "curl",
        &[
            "-fL",
            "--retry",
            "3",
            "--output",
            temporary_arg,
            ATTR_RELEASE_ARCHIVE_URL,
        ],
    )
    .context("failed to download the pinned official Attr v2.6.0 release archive")?;
    verify_attr_release_archive(&temporary)?;
    fs::rename(&temporary, &archive)
        .with_context(|| format!("failed to publish {}", archive.display()))?;
    Ok(archive)
}

fn verify_attr_release_archive(archive: &Path) -> Result<()> {
    let actual = performance::sha256_file(archive)?;
    if actual != ATTR_RELEASE_ARCHIVE_SHA256 {
        bail!(
            "Attr release archive checksum mismatch: expected {}, got {} at {}",
            ATTR_RELEASE_ARCHIVE_SHA256,
            actual,
            archive.display()
        );
    }
    Ok(())
}

/// Adds every distribution-only input from the verified release archive to an
/// output-owned Attr mirror.  Files present in the authoritative imported
/// checkout always win, including any intentional local source edits.  This
/// gives configure the complete generated release closure without modifying
/// the imported checkout or relying on host Autoconf macro packages.
fn stage_attr_bootstrap_inputs(
    authoritative_source: &Path,
    source_copy: &Path,
    archive: &Path,
) -> Result<()> {
    let release = archive
        .parent()
        .ok_or_else(|| anyhow!("Attr release archive has no parent directory"))?
        .join("release");
    remove_path_if_exists(&release)?;
    fs::create_dir_all(&release)
        .with_context(|| format!("failed to create {}", release.display()))?;
    let archive_arg = path_str(archive)?;
    let release_arg = path_str(&release)?;
    run_cmd(
        source_copy,
        "tar",
        &[
            "-xJf",
            archive_arg,
            "--strip-components=1",
            "-C",
            release_arg,
        ],
    )
    .context("failed to stage pinned Attr release bootstrap inputs")?;
    copy_attr_release_only_entries(&release, authoritative_source, source_copy)?;

    let visibility = source_copy.join("m4/visibility_hidden.m4");
    let contents = fs::read_to_string(&visibility)
        .with_context(|| format!("pinned Attr release omitted {}", visibility.display()))?;
    if !contents.contains("AC_DEFUN([AC_FUNC_GCC_VISIBILITY]") {
        bail!(
            "pinned Attr release bootstrap input {} does not define AC_FUNC_GCC_VISIBILITY",
            visibility.display()
        );
    }
    for required in [
        "configure",
        "aclocal.m4",
        "Makefile.in",
        "build-aux/config.rpath",
    ] {
        if !source_copy.join(required).is_file() {
            bail!("pinned Attr release bootstrap input is missing {required}");
        }
    }
    Ok(())
}

fn copy_attr_release_only_entries(
    release: &Path,
    authoritative: &Path,
    destination: &Path,
) -> Result<()> {
    let mut entries = fs::read_dir(release)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let source = entry.path();
        let original = authoritative.join(entry.file_name());
        let target = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source)?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            copy_attr_release_only_entries(&source, &original, &target)?;
            continue;
        }
        if fs::symlink_metadata(&original).is_ok_and(|_| true) {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        remove_path_if_exists(&target)?;
        if metadata.file_type().is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(&source)?, &target)?;
            #[cfg(not(unix))]
            fs::copy(&source, &target)?;
        } else {
            fs::copy(&source, &target).with_context(|| {
                format!(
                    "failed to stage {} to {}",
                    source.display(),
                    target.display()
                )
            })?;
            preserve_permissions(&metadata, &target)?;
        }
    }
    Ok(())
}

fn build_acl(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/acl");
    if !source.join("configure.ac").is_file() {
        bail!(
            "ACL source not found in {}; run upstream import acl first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/acl");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/acl.toml"))
        .context("failed to read ACL upstream state")?;
    let options = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
        "--disable-nls",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)
        .with_context(|| format!("failed to create {}", out_root.display()))?;
    sync_build_source(&source, &source_copy)?;
    let archive = ensure_acl_release_archive(&out_root)?;
    stage_acl_bootstrap_inputs(&source, &source_copy, &archive)?;
    run_cmd(
        &source_copy,
        "find",
        &[".", "-type", "f", "-exec", "touch", "-c", "-d", "@0", "{}", "+"],
    )?;
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").is_file() {
        let configure = source_copy.join("configure");
        let attr = repo_root.join("out/build/attr/install/usr");
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&configure)?,
            &options,
            &[
                ("CPPFLAGS", format!("-I{}", attr.join("include").display())),
                (
                    "LDFLAGS",
                    format!("-L{}", attr.join("lib/x86_64-linux-gnu").display()),
                ),
            ],
        )?;
    }
    // The pinned distribution archive already supplies Autotools-generated
    // files.  Keep maintainer regeneration disabled so timestamp differences
    // in this disposable mirror cannot require a host-versioned aclocal.
    run_cmd(&build_dir, "make", &["-j", "4", "MAKE_MAINTAINER_MODE="])?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        &build_dir,
        "make",
        &[
            "MAKE_MAINTAINER_MODE=",
            "install",
            &format!("DESTDIR={}", install_dir.display()),
        ],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libacl.so.1");
    if !soname.exists() {
        bail!("ACL install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn ensure_acl_release_archive(out_root: &Path) -> Result<PathBuf> {
    let bootstrap = out_root.join("bootstrap");
    let archive = bootstrap.join(format!("{ACL_RELEASE_DIRECTORY}.tar.xz"));
    fs::create_dir_all(&bootstrap)?;
    if !archive.is_file() {
        let temp = bootstrap.join("acl.tar.xz.tmp");
        run_cmd(
            out_root,
            "curl",
            &[
                "-fL",
                "--retry",
                "3",
                "--output",
                path_str(&temp)?,
                ACL_RELEASE_ARCHIVE_URL,
            ],
        )?;
        let actual = performance::sha256_file(&temp)?;
        if actual != ACL_RELEASE_ARCHIVE_SHA256 {
            bail!(
                "ACL release archive checksum mismatch: expected {ACL_RELEASE_ARCHIVE_SHA256}, got {actual}"
            );
        }
        fs::rename(temp, &archive)?;
    }
    let actual = performance::sha256_file(&archive)?;
    if actual != ACL_RELEASE_ARCHIVE_SHA256 {
        bail!(
            "ACL release archive checksum mismatch: expected {ACL_RELEASE_ARCHIVE_SHA256}, got {actual}"
        );
    }
    Ok(archive)
}

fn stage_acl_bootstrap_inputs(
    authoritative: &Path,
    destination: &Path,
    archive: &Path,
) -> Result<()> {
    let release = archive.parent().unwrap().join("release");
    remove_path_if_exists(&release)?;
    fs::create_dir_all(&release)?;
    run_cmd(
        destination,
        "tar",
        &[
            "-xJf",
            path_str(archive)?,
            "--strip-components=1",
            "-C",
            path_str(&release)?,
        ],
    )?;
    copy_attr_release_only_entries(&release, authoritative, destination)?;
    for required in [
        "configure",
        "aclocal.m4",
        "m4/visibility_hidden.m4",
        "m4/package_attrdev.m4",
    ] {
        if !destination.join(required).is_file() {
            bail!("pinned ACL release bootstrap input is missing {required}");
        }
    }
    Ok(())
}

fn build_zlib(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/zlib");
    if !source.join("configure").is_file() {
        bail!(
            "zlib source not found in {}; run upstream import zlib first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/zlib");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(repo_root.join("upstream/state/zlib.toml"))
        .context("failed to read zlib upstream state")?;
    let options = ["--prefix=/usr", "--libdir=/usr/lib/x86_64-linux-gnu"];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("failed to create {}", build_dir.display()))?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd(&build_dir, path_str(&source.join("configure"))?, &options)?;
    }
    run_cmd(&build_dir, "make", &["-j", "4"])?;
    remove_path_if_exists(&install_dir)?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    run_cmd(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libz.so.1");
    if !soname.exists() {
        bail!("zlib install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)
        .with_context(|| format!("failed to write {}", stamp_path.display()))?;
    Ok(())
}

fn build_gzip(repo_root: &Path) -> Result<()> {
    build_release_autotools_program(
        repo_root,
        "gzip",
        "gzip-1.14.tar.xz",
        GZIP_RELEASE_ARCHIVE_URL,
        GZIP_RELEASE_ARCHIVE_SHA256,
        &[],
        &["--prefix=/usr", "--disable-nls"],
        &["usr/bin/gzip"],
    )
}

fn build_patch(repo_root: &Path) -> Result<()> {
    build_release_autotools_program(
        repo_root,
        "patch",
        "patch-2.8.tar.xz",
        PATCH_RELEASE_ARCHIVE_URL,
        PATCH_RELEASE_ARCHIVE_SHA256,
        &[],
        &["--prefix=/usr", "--disable-nls"],
        &["usr/bin/patch"],
    )
}

