fn sync_build_source(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let _lock = ConsumerMirrorLock::acquire(&source_lock_repo_root(source)?, destination)?;
    let source_arg = format!("{}/", source.display());
    let destination_arg = format!("{}/", destination.display());
    let mut args = SOURCE_MIRROR_RSYNC_FLAGS.to_vec();
    args.extend([source_arg.as_str(), destination_arg.as_str()]);
    run_cmd(Path::new("/"), "rsync", &args)
}

fn source_lock_repo_root(source: &Path) -> Result<PathBuf> {
    source
        .ancestors()
        .find(|candidate| {
            candidate.join("Cargo.toml").is_file()
                && candidate
                    .join("src/tools/mattos-build/Cargo.toml")
                    .is_file()
        })
        .map(Path::to_path_buf)
        .or_else(|| {
            std::env::current_dir().ok().and_then(|cwd| {
                cwd.ancestors()
                    .find(|candidate| {
                        candidate.join("Cargo.toml").is_file()
                            && candidate
                                .join("src/tools/mattos-build/Cargo.toml")
                                .is_file()
                    })
                    .map(Path::to_path_buf)
            })
        })
        .ok_or_else(|| {
            anyhow!(
                "unable to locate MattOS root for source mirror {}",
                source.display()
            )
        })
}

fn prune_derived_source_mirror_artifacts(repo_root: &Path) -> Result<()> {
    let root = repo_root.join("out/build/cosmic-desktop/sources");
    if !root.is_dir() {
        return Ok(());
    }
    fn visit(path: &Path) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let child = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                if entry.file_name() == "target" || entry.file_name() == "__pycache__" {
                    fs::remove_dir_all(&child).with_context(|| {
                        format!(
                            "failed to prune derived source mirror directory {}",
                            child.display()
                        )
                    })?;
                } else {
                    visit(&child)?;
                }
            } else if file_type.is_file()
                && child
                    .extension()
                    .is_some_and(|extension| extension == "pyc")
            {
                fs::remove_file(&child).with_context(|| {
                    format!(
                        "failed to prune derived source mirror file {}",
                        child.display()
                    )
                })?;
            }
        }
        Ok(())
    }
    visit(&root)
}

struct ConsumerMirrorLock {
    #[cfg(unix)]
    file: fs::File,
}

impl ConsumerMirrorLock {
    fn acquire(repo_root: &Path, mirror: &Path) -> Result<Self> {
        let locks = repo_root.join("out/source-ownership/locks");
        fs::create_dir_all(&locks)?;
        let resolved = mirror
            .canonicalize()
            .with_context(|| format!("unable to resolve consumer mirror {}", mirror.display()))?;
        let digest = Sha256Hasher::digest(resolved.to_string_lossy().as_bytes());
        let lock_id = digest[..12]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = locks.join(format!("consumer-{lock_id}.lock"));
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if result != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(Self { file })
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(Self {})
        }
    }
}

impl Drop for ConsumerMirrorLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            unsafe {
                libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
}

fn ensure_verified_release_archive(
    out_root: &Path,
    filename: &str,
    url: &str,
    expected_sha256: &str,
) -> Result<PathBuf> {
    let bootstrap = out_root.join("bootstrap");
    fs::create_dir_all(&bootstrap)?;
    let archive = bootstrap.join(filename);
    if archive.is_file() && performance::sha256_file(&archive)? == expected_sha256 {
        return Ok(archive);
    }
    let temporary = bootstrap.join(format!("{filename}.tmp"));
    remove_path_if_exists(&temporary)?;
    run_cmd(
        out_root,
        "curl",
        &[
            "-fL",
            "--retry",
            "3",
            "--output",
            path_str(&temporary)?,
            url,
        ],
    )?;
    let actual = performance::sha256_file(&temporary)?;
    if actual != expected_sha256 {
        bail!(
            "release archive checksum mismatch for {url}: expected {expected_sha256}, got {actual}"
        );
    }
    fs::rename(&temporary, &archive)?;
    Ok(archive)
}

fn stage_release_source(archive: &Path, source_copy: &Path) -> Result<()> {
    remove_path_if_exists(source_copy)?;
    fs::create_dir_all(source_copy)?;
    let extract_flag = if archive.extension().and_then(OsStr::to_str) == Some("gz") {
        "-xzf"
    } else {
        "-xJf"
    };
    run_cmd(
        source_copy,
        "tar",
        &[extract_flag, path_str(archive)?, "--strip-components=1"],
    )
}

fn isolate_standalone_cargo_manifest(manifest: &Path) -> Result<()> {
    let mut contents = fs::read_to_string(manifest)
        .with_context(|| format!("failed to read {}", manifest.display()))?;
    if !contents.lines().any(|line| line.trim() == "[workspace]") {
        // Cargo otherwise keeps walking above an output-owned release mirror
        // and can incorrectly adopt MattOS's outer workspace. Rust's bootstrap
        // crate is intentionally standalone upstream; make that boundary
        // explicit without changing the authoritative imported source tree.
        contents.push_str("\n# MattOS output-mirror workspace boundary.\n[workspace]\n");
        fs::write(manifest, contents)
            .with_context(|| format!("failed to isolate {}", manifest.display()))?;
    }
    Ok(())
}
fn build_release_autotools_program(
    repo_root: &Path,
    component: &str,
    archive_filename: &str,
    archive_url: &str,
    archive_sha256: &str,
    dependencies: &[&str],
    options: &[&str],
    required_outputs: &[&str],
) -> Result<()> {
    let out_root = repo_root.join("out/build").join(component);
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(
        repo_root
            .join("upstream/state")
            .join(format!("{component}.toml")),
    )?;
    let stamp = format!(
        "{state}\n{archive_url}\n{archive_sha256}\n{}\n{}\n",
        dependencies.join("\n"),
        options.join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    let archive =
        ensure_verified_release_archive(&out_root, archive_filename, archive_url, archive_sha256)?;
    if !source_copy.join("configure").is_file() {
        stage_release_source(&archive, &source_copy)?;
    }
    fs::create_dir_all(&build_dir)?;
    let env = staged_library_environment(repo_root, dependencies)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            options,
            &env,
        )?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env,
    )?;
    for relative in required_outputs {
        if !install_dir.join(relative).is_file() {
            bail!("{component} install did not produce {relative}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}
