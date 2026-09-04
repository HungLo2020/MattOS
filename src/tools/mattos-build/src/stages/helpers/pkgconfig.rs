fn staged_library_environment(
    repo_root: &Path,
    components: &[&str],
) -> Result<Vec<(&'static str, String)>> {
    let mut include_dirs = Vec::new();
    let mut library_dirs = Vec::new();
    let mut pkgconfig_sources = Vec::new();
    let mut program_dirs = Vec::new();
    for component in components {
        let usr = repo_root
            .join("out/build")
            .join(component)
            .join("install/usr");
        let include = usr.join("include");
        let bin = usr.join("bin");
        let library = usr.join("lib/x86_64-linux-gnu");
        if include.is_dir() {
            include_dirs.push(include.clone());
        }
        if library.is_dir() {
            pkgconfig_sources.push((
                (*component).to_string(),
                "lib".to_string(),
                library.join("pkgconfig"),
            ));
            library_dirs.push(library);
        }
        let shared_pkgconfig = usr.join("share/pkgconfig");
        if shared_pkgconfig.is_dir() {
            pkgconfig_sources.push((
                (*component).to_string(),
                "share".to_string(),
                shared_pkgconfig,
            ));
        }
        if bin.is_dir() {
            program_dirs.push(bin);
        }
    }
    let cppflags = include_dirs
        .iter()
        .map(|p| format!("-I{}", p.display()))
        .collect::<Vec<_>>()
        .join(" ");
    let ldflags = library_dirs
        .iter()
        .map(|p| format!("-L{} -Wl,-rpath-link,{}", p.display(), p.display()))
        .collect::<Vec<_>>()
        .join(" ");
    let pkgconfig_dirs = staged_pkgconfig_overlay(repo_root, &pkgconfig_sources)?;
    Ok(vec![
        ("CPPFLAGS", cppflags),
        ("LDFLAGS", ldflags),
        (
            "LIBRARY_PATH",
            std::env::join_paths(&library_dirs)?
                .to_string_lossy()
                .to_string(),
        ),
        (
            "LD_LIBRARY_PATH",
            std::env::join_paths(&library_dirs)?
                .to_string_lossy()
                .to_string(),
        ),
        (
            "PKG_CONFIG_PATH",
            std::env::join_paths(&pkgconfig_dirs)?
                .to_string_lossy()
                .to_string(),
        ),
        // Do not fall back to host .pc files. Native runtime stages are built
        // only against previously produced MattOS development metadata.
        (
            "PKG_CONFIG_LIBDIR",
            std::env::join_paths(&pkgconfig_dirs)?
                .to_string_lossy()
                .to_string(),
        ),
        (
            "PATH",
            std::env::join_paths(
                &program_dirs
                    .iter()
                    .cloned()
                    .chain(std::env::split_paths(
                        &std::env::var_os("PATH").unwrap_or_default(),
                    ))
                    .collect::<Vec<_>>(),
            )?
            .to_string_lossy()
            .to_string(),
        ),
    ])
}

/// Build a disposable, content-addressed pkg-config view for a consumer.
///
/// A target package's installed `.pc` files correctly describe `/usr`.
/// Build consumers need those same descriptors to resolve the producer's
/// output-owned headers and libraries instead.  Historically Meson consumers
/// solved that by rewriting their dependencies' published install trees in
/// place.  That made a later Flatpak build mutate cached xkbcommon/libbsd
/// outputs after their manifests had been recorded.  The overlay keeps that
/// build-only relocation private to the consumer environment.
fn staged_pkgconfig_overlay(
    repo_root: &Path,
    sources: &[(String, String, PathBuf)],
) -> Result<Vec<PathBuf>> {
    let sources = sources
        .iter()
        .filter(|(_, _, directory)| directory.is_dir())
        .collect::<Vec<_>>();
    if sources.is_empty() {
        return Ok(Vec::new());
    }

    let identity = sources
        .iter()
        .map(|(component, kind, directory)| {
            Ok::<_, anyhow::Error>((
                component.clone(),
                kind.clone(),
                directory.to_string_lossy().to_string(),
                match stage_cache::read_stage_manifest(repo_root, component) {
                    Ok(manifest) => manifest.output_content_digest,
                    // Focused native-environment tests intentionally provide
                    // just a minimal staged tree.  Real stage execution has
                    // a producer manifest; fall back to the actual metadata
                    // bytes only for this pre-manifest fixture/bootstrap case.
                    Err(_) => performance::digest_paths(
                        repo_root,
                        std::slice::from_ref(directory),
                        false,
                        "pkgconfig-overlay-pre-manifest-v1",
                    )?,
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let digest = performance::digest_value(&identity)?;
    let root = repo_root
        .join("out/build/.pkgconfig-overlays")
        .join(&digest);

    if !root.is_dir() {
        let parent = root.parent().expect("pkg-config overlay parent");
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(".{digest}.building-{}", std::process::id()));
        remove_path_if_exists(&temporary)?;
        fs::create_dir_all(&temporary)?;
        for (component, kind, source) in &sources {
            let destination = temporary.join(component).join(kind);
            fs::create_dir_all(&destination)?;
            let producer_usr = repo_root
                .join("out/build")
                .join(component)
                .join("install/usr");
            let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let path = entry.path();
                if !path.is_file() || path.extension().and_then(OsStr::to_str) != Some("pc") {
                    continue;
                }
                let contents = fs::read_to_string(&path)?;
                fs::write(
                    destination.join(entry.file_name()),
                    rewrite_pkgconfig_for_staged_consumer(&contents, &producer_usr),
                )?;
            }
        }
        match fs::rename(&temporary, &root) {
            Ok(()) => {}
            Err(_error) if root.is_dir() => remove_path_if_exists(&temporary)?,
            Err(error) => {
                return Err(error).with_context(|| format!("failed to publish {}", root.display()))
            }
        }
    }

    Ok(sources
        .iter()
        .map(|(component, kind, _)| root.join(component).join(kind))
        .collect())
}
