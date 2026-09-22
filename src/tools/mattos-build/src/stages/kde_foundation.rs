/// KDE Frameworks and KPMCore are configured as native target consumers, not
/// as generic host CMake projects.  ECM is deliberately the one exception:
/// it supplies build-time CMake macros only and is checked explicitly below.
const KDE_ECM_MIN_VERSION: (u32, u32, u32) = (6, 27, 0);
const KDE_ECM_HOST_COMMIT: &str = "fa262097a48f0f84529498741afcc7040e93b271";
const KDE_ECM_HOST_REPOSITORY: &str = "https://invent.kde.org/frameworks/extra-cmake-modules.git";
const KDE_BOOST_HOST_VERSION: &str = "1.89.0";
const KDE_BOOST_HOST_URL: &str = "https://archives.boost.io/release/1.89.0/source/boost_1_89_0.tar.bz2";
const KDE_BOOST_HOST_SHA256: &str = "85a33fa22621b4f314f8e85e1a5e2a9363d22e4f4992925d4bb3bc631b5a0c7a";

/// KActivityManagerd uses Boost only for header-only range/algorithm helpers.
/// Keep that build-only input outside the target sysroot and package graph.
/// A fixed, verified release archive makes a host without a distro Boost
/// development package self-sufficient without allowing arbitrary host Boost
/// headers into a target build.
fn kde_host_boost_include_dir(repo_root: &Path) -> Result<PathBuf> {
    let tools = repo_root.join("out/host-tools");
    let root = tools.join(format!("boost-{KDE_BOOST_HOST_VERSION}-source"));
    let source = root.join("boost_1_89_0");
    let include = source.join("boost");
    let version_header = include.join("version.hpp");
    if version_header.is_file()
        && fs::read_to_string(&version_header)?.contains("#define BOOST_VERSION 108900")
    {
        return Ok(source);
    }
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    let archive = root.join("boost_1_89_0.tar.bz2");
    let temporary = root.join("boost_1_89_0.tar.bz2.tmp");
    let archive_is_verified = |path: &Path| -> Result<bool> {
        if !path.is_file() {
            return Ok(false);
        }
        let digest = format!("{:x}", Sha256Hasher::digest(fs::read(path)?));
        Ok(digest == KDE_BOOST_HOST_SHA256)
    };
    if !archive_is_verified(&archive)? {
        if temporary.exists() {
            fs::remove_file(&temporary)?;
        }
        let status = Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--retry",
                "3",
                "--retry-all-errors",
                "--proto",
                "=https",
                "--tlsv1.2",
                "--output",
                temporary.to_str().ok_or_else(|| anyhow!("invalid Boost archive path"))?,
                KDE_BOOST_HOST_URL,
            ])
            .status()
            .context("failed to download the pinned host-only Boost archive")?;
        if !status.success() {
            bail!("host Boost download failed with {status}");
        }
        if !archive_is_verified(&temporary)? {
            bail!("host Boost archive checksum mismatch; expected {KDE_BOOST_HOST_SHA256}");
        }
        fs::rename(&temporary, &archive)?;
    }
    if source.exists() {
        fs::remove_dir_all(&source)?;
    }
    let status = Command::new("tar")
        .args([
            "-xjf",
            archive.to_str().ok_or_else(|| anyhow!("invalid Boost archive path"))?,
            "-C",
            root.to_str().ok_or_else(|| anyhow!("invalid Boost host-tool path"))?,
        ])
        .status()
        .context("failed to extract the pinned host-only Boost archive")?;
    if !status.success() {
        bail!("host Boost extraction failed with {status}");
    }
    if !version_header.is_file()
        || !fs::read_to_string(&version_header)?.contains("#define BOOST_VERSION 108900")
    {
        bail!("verified host Boost archive did not provide Boost {KDE_BOOST_HOST_VERSION}");
    }
    Ok(source)
}

fn kde_host_ecm_dir(repo_root: &Path) -> Result<PathBuf> {
    let configured = std::env::var_os("MATTOS_HOST_ECM_DIR").map(PathBuf::from);
    let candidates = configured.into_iter().chain([
        repo_root.join("out/host-tools/ecm-6.27.0/share/ECM/cmake"),
        PathBuf::from("/usr/share/ECM/cmake"),
        PathBuf::from("/usr/lib/x86_64-linux-gnu/cmake/ECM"),
    ]);
    for directory in candidates {
        let config = directory.join("ECMConfig.cmake");
        let version = directory.join("ECMConfigVersion.cmake");
        if !config.is_file() || !version.is_file() {
            continue;
        }
        let text = fs::read_to_string(&version)?;
        let version_text = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("set(PACKAGE_VERSION \"")?.split_once('\"').map(|(v, _)| v))
            .ok_or_else(|| anyhow!("cannot determine trusted ECM version from {}", version.display()))?;
        let numbers = version_text.split('.').map(str::parse::<u32>).collect::<Result<Vec<_>, _>>()
            .map_err(|_| anyhow!("invalid trusted ECM version {version_text}"))?;
        let found = (*numbers.first().unwrap_or(&0), *numbers.get(1).unwrap_or(&0), *numbers.get(2).unwrap_or(&0));
        if found < KDE_ECM_MIN_VERSION {
            bail!("trusted host ECM {} at {} is older than required {}.{}.{}", version_text, directory.display(), KDE_ECM_MIN_VERSION.0, KDE_ECM_MIN_VERSION.1, KDE_ECM_MIN_VERSION.2);
        }
        return Ok(directory);
    }
    let tools = repo_root.join("out/host-tools");
    let source = tools.join("ecm-6.27.0-source");
    let build = tools.join("ecm-6.27.0-build");
    let install = tools.join("ecm-6.27.0");
    fs::create_dir_all(&tools)?;
    let run = |cwd: &Path, program: &str, args: &[&str]| -> Result<()> {
        let status = std::process::Command::new(program).args(args).current_dir(cwd).status()
            .with_context(|| format!("cannot run host build tool {program}"))?;
        if status.success() { Ok(()) } else { bail!("host ECM bootstrap command failed: {program} {}", args.join(" ")) }
    };
    if !source.join(".git").is_dir() {
        run(&tools, "git", &["clone", "--filter=blob:none", KDE_ECM_HOST_REPOSITORY, source.to_str().ok_or_else(|| anyhow!("invalid ECM source path"))?])?;
    }
    let revision = std::process::Command::new("git").args(["-C", source.to_str().ok_or_else(|| anyhow!("invalid ECM source path"))?, "rev-parse", "HEAD"]).output()
        .context("cannot inspect bootstrapped ECM revision")?;
    let current = String::from_utf8_lossy(&revision.stdout).trim().to_owned();
    if current != KDE_ECM_HOST_COMMIT {
        run(&source, "git", &["fetch", "--depth=1", "origin", KDE_ECM_HOST_COMMIT])?;
        run(&source, "git", &["checkout", "--detach", KDE_ECM_HOST_COMMIT])?;
    }
    let verified = std::process::Command::new("git").args(["-C", source.to_str().ok_or_else(|| anyhow!("invalid ECM source path"))?, "rev-parse", "HEAD"]).output()
        .context("cannot verify bootstrapped ECM revision")?;
    if String::from_utf8_lossy(&verified.stdout).trim() != KDE_ECM_HOST_COMMIT {
        bail!("bootstrapped ECM revision is not the pinned commit {}", KDE_ECM_HOST_COMMIT);
    }
    run(&tools, "cmake", &["-S", source.to_str().ok_or_else(|| anyhow!("invalid ECM source path"))?, "-B", build.to_str().ok_or_else(|| anyhow!("invalid ECM build path"))?, "-G", "Ninja", &format!("-DCMAKE_INSTALL_PREFIX={}", install.display())])?;
    run(&tools, "cmake", &["--build", build.to_str().ok_or_else(|| anyhow!("invalid ECM build path"))?])?;
    run(&tools, "cmake", &["--install", build.to_str().ok_or_else(|| anyhow!("invalid ECM build path"))?])?;
    let result = install.join("share/ECM/cmake");
    if !result.join("ECMConfig.cmake").is_file() { bail!("ECM bootstrap completed without {}", result.join("ECMConfig.cmake").display()); }
    Ok(result)
}

fn kde_target_environment(
    repo_root: &Path,
    build: &Path,
    components: &[&str],
    qt_integration: bool,
) -> Result<Vec<(&'static str, String)>> {
    let mut environment = if qt_integration {
        qt_target_environment(
            repo_root,
            build,
            Some(&repo_root.join("out/build/qtbase/install/usr")),
        )?
    } else {
        qt_build_tool_environment(build, None)?
    };
    // KDE's generated imported targets intentionally keep many low-level
    // shared-library dependencies transitive.  The modern linker does not
    // resolve symbols through a DSO's DT_NEEDED chain, so expose the already
    // built target-owned ABI providers to every KDE consumer.  This is a
    // generic target-link environment rule, not a host-library fallback or a
    // component-specific workaround.
    let mut link_components = components.to_vec();
    // KWindowSystem is built with its X11 compatibility backend enabled so
    // the final KWin package can host Xwayland.  Its exported KF6WindowSystem
    // config consequently has a target-owned X11 dependency even when the
    // consuming application disables its own X11 code.  Propagate the
    // aggregate target X11 prefix to every such consumer; never let CMake or
    // the linker satisfy that transitive contract from the host.
    if components.contains(&"kwindowsystem") && !link_components.contains(&"x11-compat") {
        link_components.push("x11-compat");
    }
    // QtGui is built with the source-owned Fontconfig/FreeType backend. Its
    // imported Qt target carries those DSOs as DT_NEEDED dependencies, but
    // the linker does not search a DSO's dependency directories when a KDE
    // executable links Qt6::Gui. Expose the complete target-owned QtGui
    // font closure to every KDE consumer; never let the host satisfy it.
    if qt_integration {
        for component in [
            "freetype", "fontconfig", "expat", "systemd", "util-linux", "pcre2",
            "zlib", "zstd", "bzip2", "xz", "openssl",
        ] {
            if !link_components.contains(&component) {
                link_components.push(component);
            }
        }
    }
    let target = staged_library_environment(repo_root, &link_components)?;
    for (key, value) in target {
        if key == "PATH" {
            continue;
        }
        if let Some((_, existing)) = environment.iter_mut().find(|(name, _)| *name == key) {
            *existing = if key == "LD_LIBRARY_PATH" { format!("{existing}:{value}") } else { value };
        } else {
            environment.push((key, value));
        }
    }
    let mut trusted_bins = BTreeSet::new();
    for program in ["cmake", "ninja", "gcc", "g++", "ar", "ranlib", "pkg-config", "python3", "msgfmt", "msgmerge"] {
        trusted_bins.insert(qt_host_tool(program)?.parent().unwrap().to_path_buf());
    }
    environment.retain(|(key, _)| *key != "PATH");
    environment.push(("PATH", std::env::join_paths(trusted_bins)?.to_string_lossy().into_owned()));
    Ok(environment)
}

fn kde_target_prefixes(repo_root: &Path, components: &[&str]) -> Vec<PathBuf> {
    let mut prefixes = vec![repo_root.join("out/build/qtbase/install/usr")];
    prefixes.extend(components.iter().map(|component| repo_root.join("out/build").join(component).join("install/usr")));
    if components.contains(&"kwindowsystem") && !components.contains(&"x11-compat") {
        prefixes.push(repo_root.join("out/build/x11-compat/install/usr"));
    }
    // KIO's generated package contract pulls Codecs transitively.  Keep that
    // target-owned prefix visible when building shell consumers that include
    // KCMUtils/KIO, even though it is not an implementation-local library.
    if components.contains(&"kcmutils") && !components.contains(&"kcodecs") {
        prefixes.push(repo_root.join("out/build/kcodecs/install/usr"));
    }
    prefixes
}

/// Qt's umbrella Qt6Config computes component locations relative to its own
/// multiarch CMake directory.  KDE consumers may span several standalone Qt
/// module prefixes, so provide a disposable, symlink-only CMake view.  The
/// target files remain owned by their Qt stages; this view contains no copied
/// libraries or headers and is never packaged.
fn qt_multimodule_cmake_view(repo_root: &Path, build: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::symlink;
    fn link_tree(source: &Path, destination: &Path) -> Result<()> {
        if source.is_dir() {
            fs::create_dir_all(destination)?;
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                link_tree(&entry.path(), &destination.join(entry.file_name()))?;
            }
        } else if source.exists() && destination.symlink_metadata().is_err() {
            symlink(source, destination)?;
        }
        Ok(())
    }
    let view = build.join("mattos-qt-cmake-view/lib/x86_64-linux-gnu/cmake");
    fs::create_dir_all(&view)?;
    let view_root = build.join("mattos-qt-cmake-view");
    fs::create_dir_all(view_root.join("bin"))?;
    fs::create_dir_all(view_root.join("libexec"))?;
    let providers = [
        repo_root.join("out/build/qtbase/install/usr/lib/x86_64-linux-gnu/cmake"),
        repo_root.join("out/build/qt5compat/install/usr/lib/x86_64-linux-gnu/cmake"),
        repo_root.join("out/build/qtdeclarative/install/usr/lib/x86_64-linux-gnu/cmake"),
        repo_root.join("out/build/qtpositioning/install/usr/lib/x86_64-linux-gnu/cmake"),
        repo_root.join("out/build/qtlocation/install/usr/lib/x86_64-linux-gnu/cmake"),
        repo_root.join("out/build/qtshadertools/install/usr/lib/x86_64-linux-gnu/cmake"),
        repo_root.join("out/build/qtsvg/install/usr/lib/x86_64-linux-gnu/cmake"),
        repo_root.join("out/build/qttools/install/usr/lib/x86_64-linux-gnu/cmake"),
        repo_root.join("out/build/qtmultimedia/install/usr/lib/x86_64-linux-gnu/cmake"),
        repo_root.join("out/build/qtspeech/install/usr/lib/x86_64-linux-gnu/cmake"),
    ];
    // Imported Qt targets deliberately resolve relative to the prefix that
    // owns the package config.  The view therefore also exposes symlinked
    // target files/includes; unlike copying, this keeps the view disposable
    // while preserving the real stage-owned bytes and paths.
    let view_lib = build.join("mattos-qt-cmake-view/lib/x86_64-linux-gnu");
    fs::create_dir_all(&view_lib)?;
    let include = build.join("mattos-qt-cmake-view/include");
    if include.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        fs::remove_file(&include)?;
    }
    fs::create_dir_all(&include)?;
    for provider in [
        repo_root.join("out/build/qtbase/install/usr/include"),
        repo_root.join("out/build/qtdeclarative/install/usr/include"),
        repo_root.join("out/build/qtpositioning/install/usr/include"),
        repo_root.join("out/build/qtshadertools/install/usr/include"),
        repo_root.join("out/build/qtsvg/install/usr/include"),
        repo_root.join("out/build/qtwayland/install/usr/include"),
        repo_root.join("out/build/qttools/install/usr/include"),
        repo_root.join("out/build/qtmultimedia/install/usr/include"),
        repo_root.join("out/build/qtspeech/install/usr/include"),
    ] {
        if provider.is_dir() { link_tree(&provider, &include)?; }
    }
    let mkspecs = view_root.join("mkspecs");
    if mkspecs.symlink_metadata().is_err() {
        symlink(repo_root.join("out/build/qtbase/install/usr/mkspecs"), &mkspecs)?;
    }
    let metatypes = view_root.join("metatypes");
    if metatypes.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        fs::remove_file(&metatypes)?;
    }
    fs::create_dir_all(&metatypes)?;
    for provider in [
        repo_root.join("out/build/qtbase/install/usr/metatypes"),
        repo_root.join("out/build/qtdeclarative/install/usr/metatypes"),
        repo_root.join("out/build/qtshadertools/install/usr/metatypes"),
        repo_root.join("out/build/qtpositioning/install/usr/metatypes"),
        repo_root.join("out/build/qtsvg/install/usr/metatypes"),
        repo_root.join("out/build/qtmultimedia/install/usr/metatypes"),
        repo_root.join("out/build/qtspeech/install/usr/metatypes"),
    ] {
        if provider.is_dir() { link_tree(&provider, &metatypes)?; }
    }
    // Standalone Qt module installs generate an SPDX document which refers to
    // the already-published dependency documents (for example Qt5Compat
    // references QtDeclarative and QtShaderTools).  Keep those documents in
    // the same disposable consumer view so CMake's upstream SBOM installer
    // can resolve the references without looking in a host Qt prefix.
    let sbom = view_root.join("sbom");
    fs::create_dir_all(&sbom)?;
    for provider in [
        repo_root.join("out/build/qtbase/install/usr/sbom"),
        repo_root.join("out/build/qtdeclarative/install/usr/sbom"),
        repo_root.join("out/build/qtshadertools/install/usr/sbom"),
        repo_root.join("out/build/qtsvg/install/usr/sbom"),
        repo_root.join("out/build/qtwayland/install/usr/sbom"),
    ] {
        if provider.is_dir() { link_tree(&provider, &sbom)?; }
    }
    for provider in providers {
        if !provider.is_dir() { continue; }
        if let Some(libdir) = provider.parent() {
            if libdir.is_dir() {
                for entry in fs::read_dir(libdir)? {
                    let entry = entry?;
                    let name = entry.file_name();
                    if name == "cmake" { continue; }
                    let destination = view_lib.join(&name);
                    if destination.symlink_metadata().is_err() {
                        symlink(entry.path(), destination)?;
                    }
                }
            }
        }
        for entry in fs::read_dir(provider)? {
            let entry = entry?;
            let name = entry.file_name();
            let destination = view.join(&name);
            // Keep package-relative imported-target paths anchored at the
            // real target prefix. Copying these generated CMake packages
            // into the disposable view rewrites their `_IMPORT_PREFIX` and
            // makes targets point at the view instead of the staged Qt
            // libraries. A symlink preserves the package's own prefix while
            // still presenting one multi-module search directory to CMake.
            if entry.path().exists() && destination.symlink_metadata().is_err() {
                symlink(entry.path(), destination)?;
            }
        }
    }
    // QtGuiPrivate's generated dependency metadata asks CMake for XKB.  The
    // target xkbcommon package intentionally exposes pkg-config rather than
    // a distro-specific CMake package, so provide the same small disposable
    // adapter to every Qt consumer (never the host XKB installation).
    let xkb_package = view.join("XKB");
    if xkb_package.exists() { fs::remove_dir_all(&xkb_package)?; }
    fs::create_dir_all(&xkb_package)?;
    let xkb_root = repo_root.join("out/build/xkbcommon/install/usr");
    fs::write(xkb_package.join("XKBConfig.cmake"), format!(
        "set(XKB_FOUND TRUE)\nset(XKB_VERSION 0.9.0)\nset(XKB_INCLUDE_DIR \"{}/include\")\nset(XKB_INCLUDE_DIRS \"{}/include\")\nset(XKB_LIBRARY \"{}/lib/x86_64-linux-gnu/libxkbcommon.so\")\nset(XKB_LIBRARIES \"${{XKB_LIBRARY}}\")\nif(NOT TARGET XKB::XKB)\n  add_library(XKB::XKB UNKNOWN IMPORTED)\n  set_target_properties(XKB::XKB PROPERTIES IMPORTED_LOCATION \"${{XKB_LIBRARY}}\" INTERFACE_INCLUDE_DIRECTORIES \"${{XKB_INCLUDE_DIR}}\")\nendif()\n",
        xkb_root.display(), xkb_root.display(), xkb_root.display()))?;
    fs::write(xkb_package.join("XKBConfigVersion.cmake"),
        "set(PACKAGE_VERSION 0.9.0)\nset(PACKAGE_VERSION_COMPATIBLE TRUE)\nset(PACKAGE_VERSION_EXACT TRUE)\n")?;
    // QtWayland's generated private-target metadata uses the upstream
    // Wayland CMake package contract.  MattOS's Wayland stage publishes
    // pkg-config files, so adapt those same staged headers/libraries through
    // a disposable, target-only package instead of allowing host Wayland
    // development files to satisfy the probe.
    let wayland_package = view.join("Wayland");
    if wayland_package.exists() { fs::remove_dir_all(&wayland_package)?; }
    fs::create_dir_all(&wayland_package)?;
    let wayland_root = repo_root.join("out/build/wayland/install/usr");
    // Use explicit targets because CMake variable-name case folding cannot
    // derive the lower-case library suffix portably.
    fs::write(wayland_package.join("WaylandConfig.cmake"), format!(
        "set(Wayland_FOUND TRUE)\nset(Wayland_VERSION 1.26.0)\nset(Wayland_INCLUDE_DIR \"{}/include\")\nset(Wayland_INCLUDE_DIRS \"{}/include\")\n",
        wayland_root.display(), wayland_root.display()))?;
    for (name, library) in [("Client", "client"), ("Server", "server"), ("Cursor", "cursor"), ("Egl", "egl")] {
        let mut config = fs::read_to_string(wayland_package.join("WaylandConfig.cmake"))?;
        config.push_str(&format!("if(NOT TARGET Wayland::{name})\nadd_library(Wayland::{name} UNKNOWN IMPORTED)\nset_target_properties(Wayland::{name} PROPERTIES IMPORTED_LOCATION \"{}/lib/x86_64-linux-gnu/libwayland-{library}.so\" INTERFACE_INCLUDE_DIRECTORIES \"${{Wayland_INCLUDE_DIR}}\")\nendif()\n", wayland_root.display()));
        fs::write(wayland_package.join("WaylandConfig.cmake"), config)?;
    }
    fs::write(wayland_package.join("WaylandConfigVersion.cmake"),
        "set(PACKAGE_VERSION 1.26.0)\nset(PACKAGE_VERSION_COMPATIBLE TRUE)\nset(PACKAGE_VERSION_EXACT TRUE)\n")?;
    let view_plugins = view_root.join("plugins");
    for provider in [
        repo_root.join("out/build/qtbase/install/usr/plugins"),
        repo_root.join("out/build/qt5compat/install/usr/plugins"),
        repo_root.join("out/build/qtdeclarative/install/usr/plugins"),
        repo_root.join("out/build/qtwayland/install/usr/plugins"),
        repo_root.join("out/build/qtsvg/install/usr/plugins"),
    ] {
        if provider.is_dir() { link_tree(&provider, &view_plugins)?; }
    }
    let view_qml = view_root.join("qml");
    for provider in [
        repo_root.join("out/build/qtbase/install/usr/qml"),
        repo_root.join("out/build/qtdeclarative/install/usr/qml"),
        repo_root.join("out/build/qtwayland/install/usr/qml"),
    ] {
        if provider.is_dir() { link_tree(&provider, &view_qml)?; }
    }
    // Qt's generated *Tools exports enumerate optional generators that are
    // intentionally not installed by the target QtBase cross-install. ECM
    // only needs qtpaths, and QML's package only needs its tools package to be
    // present while configuring KDE. Keep these tiny build-only contracts
    // explicit instead of importing a host Qt installation or fabricating
    // target files.
    for (name, body) in [
        ("Qt6CoreTools", format!(
            "set(Qt6CoreTools_FOUND TRUE)\nif(NOT TARGET Qt6::qtpaths)\n  add_executable(Qt6::qtpaths IMPORTED GLOBAL)\n  set_target_properties(Qt6::qtpaths PROPERTIES IMPORTED_LOCATION \"{}/bin/qtpaths\")\nendif()\n",
            view_root.display())),
        ("Qt6QmlTools", "set(Qt6QmlTools_FOUND TRUE)\nset(PACKAGE_VERSION 6.11.2)\nset(PACKAGE_VERSION_COMPATIBLE TRUE)\nset(PACKAGE_VERSION_EXACT TRUE)\n".to_owned()),
    ] {
        let package = view.join(name);
        if let Ok(metadata) = fs::symlink_metadata(&package) {
            if metadata.file_type().is_symlink() { fs::remove_file(&package)?; }
            else if metadata.is_dir() { fs::remove_dir_all(&package)?; }
            else { fs::remove_file(&package)?; }
        }
        fs::create_dir_all(&package)?;
        fs::write(package.join(format!("{name}Config.cmake")), body)?;
        fs::write(package.join(format!("{name}ConfigVersion.cmake")),
            "set(PACKAGE_VERSION 6.11.2)\nset(PACKAGE_VERSION_COMPATIBLE TRUE)\nset(PACKAGE_VERSION_EXACT TRUE)\n")?;
    }
    let umbrella = view.join("Qt6");
    if umbrella.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        fs::remove_file(&umbrella)?;
    }
    if umbrella.symlink_metadata().is_err() {
        let source = repo_root.join("out/build/qtbase/install/usr/lib/x86_64-linux-gnu/cmake/Qt6");
        fs::create_dir_all(&umbrella)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            if entry.path().is_file() {
                fs::copy(entry.path(), umbrella.join(entry.file_name()))?;
            }
        }
    }
    // Imported Qt tool targets are deliberately host/build-only, but KDE's
    // ECM queries them through the normal Qt package. Link only the verified
    // MattOS Qt generators into this disposable view.
    for (directory, names, destination_is_bin) in [
        (repo_root.join("out/build/qtbase/install/usr/bin"), vec!["qtpaths", "qmake"], true),
        (repo_root.join("out/build/qtbase/install/usr/libexec"), vec!["syncqt", "moc", "rcc", "tracepointgen", "tracegen", "cmake_automoc_parser", "qlalr"], false),
        (repo_root.join("out/build/qtdeclarative/build/bin"), vec!["qmllint", "qmlformat", "qmltc", "qmlcontextpropertydump"], true),
        (repo_root.join("out/build/qtdeclarative/build/libexec"), vec!["qmlcachegen", "qmltyperegistrar", "qmlaotstats", "qmlimportscanner"], false),
    ] {
        let destination_directory = if destination_is_bin { view_root.join("bin") } else { view_root.join("libexec") };
        for name in names {
            let source = directory.join(name);
            let destination = destination_directory.join(name);
            if source.is_file() && destination.symlink_metadata().is_err() {
                symlink(source, destination)?;
            }
        }
    }
    Ok(view)
}

fn kde_qt_tool_bridge(build: &Path) -> Result<PathBuf> {
    let bridge = build.join("mattos-qt-tool-bridge.cmake");
    let qtpaths = build.join("mattos-qt-cmake-view/bin/qtpaths");
    let mut text = format!("set(Qt6CoreTools_FOUND TRUE)\nset(Qt6QmlTools_FOUND TRUE)\n");
    let x11_root = build.parent().and_then(Path::parent).unwrap_or(build).join("x11-compat/install/usr");
    text.push_str(&format!("if(NOT TARGET X11::X11)\n  add_library(X11::X11 UNKNOWN IMPORTED GLOBAL)\n  set_target_properties(X11::X11 PROPERTIES IMPORTED_LOCATION \"{}/lib/x86_64-linux-gnu/libX11.so\" INTERFACE_INCLUDE_DIRECTORIES \"{}/include\")\nendif()\n", x11_root.display(), x11_root.display()));
    for tool in ["qmlcontextpropertydump", "qmllint", "qmlformat", "qmltc"] {
        text.push_str(&format!("if(NOT TARGET Qt6::{tool})\n  add_executable(Qt6::{tool} IMPORTED GLOBAL)\n  set_target_properties(Qt6::{tool} PROPERTIES IMPORTED_LOCATION \"{}/bin/{tool}\")\nendif()\n", build.join("mattos-qt-cmake-view").display()));
    }
    for tool in ["qmltyperegistrar", "qmlcachegen", "qmlaotstats"] {
        text.push_str(&format!("if(NOT TARGET Qt6::{tool})\n  add_executable(Qt6::{tool} IMPORTED GLOBAL)\n  set_target_properties(Qt6::{tool} PROPERTIES IMPORTED_LOCATION \"{}/libexec/{tool}\")\nendif()\n", build.join("mattos-qt-cmake-view").display()));
    }
    for (name, path) in [
        ("syncqt", build.join("mattos-qt-cmake-view/libexec/syncqt")),
        ("moc", build.join("mattos-qt-cmake-view/libexec/moc")),
        ("rcc", build.join("mattos-qt-cmake-view/libexec/rcc")),
        ("tracepointgen", build.join("mattos-qt-cmake-view/libexec/tracepointgen")),
        ("tracegen", build.join("mattos-qt-cmake-view/libexec/tracegen")),
        ("cmake_automoc_parser", build.join("mattos-qt-cmake-view/libexec/cmake_automoc_parser")),
        ("qlalr", build.join("mattos-qt-cmake-view/libexec/qlalr")),
        ("qtpaths", qtpaths.clone()),
        ("qmake", build.join("mattos-qt-cmake-view/bin/qmake")),
        ("androiddeployqt", qtpaths.clone()),
        ("androidtestrunner", qtpaths.clone()),
        ("wasmdeployqt", qtpaths.clone()),
    ] {
        text.push_str(&format!("if(NOT TARGET Qt6::{name})\n  add_executable(Qt6::{name} IMPORTED GLOBAL)\n  set_target_properties(Qt6::{name} PROPERTIES IMPORTED_LOCATION \"{}\")\nendif()\n", path.display()));
    }
    fs::write(&bridge, text)?;
    Ok(bridge)
}

/// KDE's translation macros need lrelease/lconvert while compiling catalog
/// data. They are generators, not target ABI. Expose only those explicitly
/// selected host executables through a tiny CMake package rather than letting
/// Qt's host installation join CMAKE_PREFIX_PATH.
fn kde_linguist_tools_bridge(build: &Path) -> Result<PathBuf> {
    let host_linguist = |name: &str| -> Result<PathBuf> {
        if let Some(directory) = std::env::var_os("MATTOS_HOST_QT_LINGUIST_BIN") {
            let candidate = PathBuf::from(directory).join(name);
            if candidate.is_file() { return Ok(candidate); }
        }
        let distro_candidate = PathBuf::from("/usr/lib/qt6/bin").join(name);
        if distro_candidate.is_file() { return Ok(distro_candidate); }
        qt_host_tool(name)
    };
    let lrelease = host_linguist("lrelease")?;
    let lconvert = host_linguist("lconvert")?;
    let root = build.join("mattos-host-qt-linguist-tools");
    let config_dir = root.join("Qt6LinguistTools");
    fs::create_dir_all(&config_dir)?;
    // Qt's umbrella package asks for a versioned LinguistTools config.  The
    // executables are intentionally host tools, but the package identity must
    // still match the target Qt baseline or Qt6Config.cmake rejects the
    // otherwise-valid bridge before configure begins.
    fs::write(config_dir.join("Qt6LinguistToolsConfigVersion.cmake"),
        "set(PACKAGE_VERSION 6.11.2)\nset(PACKAGE_VERSION_COMPATIBLE TRUE)\nset(PACKAGE_VERSION_EXACT TRUE)\n")?;
    let config = format!(r#"if(NOT TARGET Qt6::lrelease)
  add_executable(Qt6::lrelease IMPORTED GLOBAL)
  set_target_properties(Qt6::lrelease PROPERTIES IMPORTED_LOCATION "{}")
endif()
if(NOT TARGET Qt6::lconvert)
  add_executable(Qt6::lconvert IMPORTED GLOBAL)
  set_target_properties(Qt6::lconvert PROPERTIES IMPORTED_LOCATION "{}")
endif()
set(MATTOS_LRELEASE "{}")
if(NOT COMMAND qt_add_translation)
  function(qt_add_translation out_var)
    set(_qm_files)
    foreach(_ts IN LISTS ARGN)
      get_filename_component(_ts_abs "${{_ts}}" ABSOLUTE BASE_DIR "${{CMAKE_CURRENT_SOURCE_DIR}}")
      get_filename_component(_name "${{_ts_abs}}" NAME_WE)
      set(_qm "${{CMAKE_CURRENT_BINARY_DIR}}/${{_name}}.qm")
      execute_process(COMMAND "${{MATTOS_LRELEASE}}" -silent -qm "${{_qm}}" "${{_ts_abs}}" RESULT_VARIABLE _result)
      if(NOT _result EQUAL 0)
        message(FATAL_ERROR "lrelease failed for ${{_ts_abs}}")
      endif()
      list(APPEND _qm_files "${{_qm}}")
    endforeach()
    set(${{out_var}} "${{_qm_files}}" PARENT_SCOPE)
  endfunction()
endif()
set(Qt6LinguistTools_FOUND TRUE)
"#, lrelease.display(), lconvert.display(), lrelease.display());
    fs::write(config_dir.join("Qt6LinguistToolsConfig.cmake"), config)?;
    Ok(config_dir)
}

/// Add a CMake flag assignment without allowing a later component-specific
/// bridge to silently replace an earlier target-owned include path.  Several
/// KDE consumers need both XCB and DRM (or util-linux) headers; CMake keeps
/// only the last `-DCMAKE_*_FLAGS` occurrence, so all contributors must be
/// composed into one assignment.
fn append_cmake_flag(command: &mut Vec<String>, key: &str, value: &str) {
    let prefix = format!("{key}=");
    if let Some(argument) = command.iter_mut().find(|argument| argument.starts_with(&prefix)) {
        argument.push(' ');
        argument.push_str(value);
    } else {
        command.push(format!("{key}={value}"));
    }
}

fn build_kde_cmake(
    repo_root: &Path,
    component: &str,
    source: &str,
    components: &[&str],
    options: &[&str],
    required_output: &str,
) -> Result<()> {
    build_cmake_component(
        repo_root,
        component,
        source,
        components,
        options,
        required_output,
        true,
    )
}

fn build_non_qt_cmake(
    repo_root: &Path,
    component: &str,
    source: &str,
    components: &[&str],
    options: &[&str],
    required_output: &str,
) -> Result<()> {
    build_cmake_component(
        repo_root,
        component,
        source,
        components,
        options,
        required_output,
        false,
    )
}

fn build_cmake_component(
    repo_root: &Path,
    component: &str,
    source: &str,
    components: &[&str],
    options: &[&str],
    required_output: &str,
    qt_integration: bool,
) -> Result<()> {
    let root = repo_root.join("out/build").join(component);
    let source_copy = root.join("source");
    let build = root.join("build");
    let install = root.join("install");
    let python_deps = root.join("python-deps");
    if component == "breeze-icons" && !python_deps.join("lxml").is_dir() {
        // Breeze generates its 24px icon variants with lxml. Keep this
        // build-only Python dependency pinned and output-owned rather than
        // requiring or modifying the host Python environment.
        fs::create_dir_all(&python_deps)?;
        run_cmd(
            repo_root,
            "python3",
            &[
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "--no-deps",
                "--target",
                path_str(&python_deps)?,
                "lxml==6.0.2",
            ],
        )?;
    }
    sync_build_source(&repo_root.join(source), &source_copy)?;
    if component == "kfilemetadata" {
        // Upstream's legacy FindXattr module hard-codes host /usr paths and
        // ignores pkg-config. Point it at the two source-owned target roots
        // in the disposable mirror so strict CMake isolation cannot either
        // miss libattr or silently accept the host copy.
        let finder = source_copy.join("cmake/FindXattr.cmake");
        let contents = fs::read_to_string(&finder)?;
        let adjusted = contents.replace(
            "# Already in cache?",
            &format!(
                "set(XATTR_INCLUDE_DIRS \"{}\")\nset(XATTR_LIBRARIES \"{}\")\n# Already in cache?",
                repo_root.join("out/sysroot/formal/usr/include").display(),
                repo_root.join("out/build/attr/install/usr/lib/x86_64-linux-gnu/libattr.so").display(),
            ),
        );
        if adjusted != contents { fs::write(finder, adjusted)?; }
    }
    if component == "kholidays" {
        // KDE's current mirror has already advanced its ECM floor beyond the
        // selected KF 6.26 tool baseline.  The v6.24 API is compatible with
        // that baseline; keep this compatibility adjustment in the disposable
        // build mirror and never mutate the imported source.
        let cmake_lists = source_copy.join("CMakeLists.txt");
        let contents = fs::read_to_string(&cmake_lists)?;
        let adjusted = contents.replace("find_package(ECM 6.30.0 REQUIRED NO_MODULE)", "find_package(ECM 6.26.0 REQUIRED NO_MODULE)");
        if adjusted != contents {
            fs::write(cmake_lists, adjusted)?;
        }
    }
    if component == "xdg-desktop-portal-kde" {
        // Qt only installs qcups_p.h when QtBase was built with the optional
        // CUPS backend.  The portal's generic QPrinter path is still useful
        // without a print server, but upstream includes that private header
        // unconditionally solely for CUPS job-option setters.  Keep the
        // portal available in the source-owned no-CUPS target and make those
        // optional setters no-ops; do not satisfy the private include from
        // the host Qt installation.
        let print = source_copy.join("src/print.cpp");
        let contents = fs::read_to_string(&print)?;
        let needle = "#include <QtPrintSupport/private/qcups_p.h>";
        let replacement = r#"#include <QDateTime>
#include <QtPrintSupport/private/qtprintsupport-config_p.h>
#if QT_CONFIG(cups)
#include <QtPrintSupport/private/qcups_p.h>
#else
#define PPK_CupsOptions QPrintEngine::PrintEnginePropertyKey(0xfe00)
class QCUPSSupport
{
public:
    enum PageSet { AllPages, OddPages, EvenPages };
    enum PagesPerSheet { OnePagePerSheet, TwoPagesPerSheet, FourPagesPerSheet, SixPagesPerSheet, NinePagesPerSheet, SixteenPagesPerSheet };
    enum PagesPerSheetLayout { LeftToRightTopToBottom, LeftToRightBottomToTop, RightToLeftTopToBottom, RightToLeftBottomToTop, BottomToTopLeftToRight, BottomToTopRightToLeft, TopToBottomLeftToRight, TopToBottomRightToLeft };
    static void setPageSet(QPrinter *, PageSet) {}
    static void setPagesPerSheetLayout(QPrinter *, PagesPerSheet, PagesPerSheetLayout) {}
};
#endif"#;
        if !contents.contains(needle) {
            bail!("xdg-desktop-portal-kde print backend no longer has the expected qcups include");
        }
        fs::write(print, contents.replace(needle, replacement))?;
    }
    if component == "kpipewire" {
        // KPipeWireRecord includes VA-API headers directly, but 6.6.5 only
        // links the imported libva targets to the sibling KPipeWire target as
        // PRIVATE dependencies. That neither propagates the include path nor
        // expresses KPipeWireRecord's own link requirements. Apply the
        // upstream-compatible target dependency in the disposable mirror.
        let cmake = source_copy.join("src/CMakeLists.txt");
        let contents = fs::read_to_string(&cmake)?;
        let needle = "target_link_libraries(KPipeWireRecord PUBLIC KPipeWire Qt6::QmlIntegration\n    PRIVATE Qt::Core Qt::Gui KF6::CoreAddons KPipeWireDmaBuf\n    PkgConfig::AVCodec PkgConfig::AVUtil PkgConfig::AVFormat PkgConfig::AVFilter PkgConfig::GBM PkgConfig::SWScale\n    epoxy::epoxy Libdrm::Libdrm\n)";
        let replacement = "target_link_libraries(KPipeWireRecord PUBLIC KPipeWire Qt6::QmlIntegration\n    PRIVATE Qt::Core Qt::Gui KF6::CoreAddons KPipeWireDmaBuf\n    PkgConfig::AVCodec PkgConfig::AVUtil PkgConfig::AVFormat PkgConfig::AVFilter PkgConfig::GBM PkgConfig::SWScale\n    epoxy::epoxy Libdrm::Libdrm PkgConfig::LIBVA PkgConfig::LIBVA-drm\n)";
        if !contents.contains(needle) {
            bail!("kpipewire target linkage no longer matches the expected 6.6.5 layout");
        }
        let adjusted = contents.replacen(needle, replacement, 1).replace(
            "PUBLIC \"$<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>\" ${PipeWire_INCLUDE_DIRS}",
            "PUBLIC \"$<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>\" \"$<BUILD_INTERFACE:${PipeWire_INCLUDE_DIRS}>\"",
        );
        fs::write(cmake, adjusted)?;
    }
    if component == "spectacle" {
        // Spectacle 6.6.5 adds its test directory unconditionally, so even a
        // release build with BUILD_TESTING=OFF requires Prison's optional
        // scanner target. Keep tests out of the runtime build as requested
        // instead of expanding the image with barcode-scanner dependencies.
        let cmake = source_copy.join("CMakeLists.txt");
        let contents = fs::read_to_string(&cmake)?;
        let needle = "add_subdirectory(tests)";
        if !contents.contains(needle) {
            bail!("spectacle test-directory layout changed unexpectedly");
        }
        fs::write(
            cmake,
            contents.replace(needle, "if(BUILD_TESTING)\n    add_subdirectory(tests)\nendif()"),
        )?;
    }
    if component == "plasma-pa" {
        // The kded audio-shortcut plugin includes volumefeedback.h, whose
        // public interface includes canberra.h.  plasma-volume links Canberra
        // privately, so that include requirement does not propagate to the
        // sibling plugin. Express the plugin's direct compile/link dependency
        // in the disposable source mirror.
        let cmake = source_copy.join("src/kded/CMakeLists.txt");
        let contents = fs::read_to_string(&cmake)?;
        let needle = "                        KF6::PulseAudioQt\n                        PkgConfig::LIBPULSE";
        let replacement = "                        KF6::PulseAudioQt\n                        Canberra::Canberra\n                        PkgConfig::LIBPULSE";
        if !contents.contains(needle) {
            bail!("plasma-pa kded linkage no longer matches the expected 6.6.5 layout");
        }
        fs::write(cmake, contents.replacen(needle, replacement, 1))?;
    }
    if component == "kpmcore" {
        // KPMCore's helper is called synchronously by Calamares while the
        // partition model is initializing.  A failed/blocked QProcess must
        // never leave the graphical installer waiting forever with no
        // diagnostic.  Keep this narrow compatibility fix in the disposable
        // source mirror: report the exact helper phase, require the process
        // to start, and bound command execution so the caller can fail
        // closed and surface an actionable error.
        let helper = source_copy.join("src/util/externalcommandhelper.cpp");
        let contents = fs::read_to_string(&helper)?;
        let needle = "    QProcess cmd;\n    cmd.setEnvironment( { QStringLiteral(\"LVM_SUPPRESS_FD_WARNINGS=1\") } );\n";
        let replacement = "    QProcess cmd;\n    qInfo() << \"KPMCore helper starting privileged command\" << command << arguments;\n    cmd.setEnvironment( { QStringLiteral(\"LVM_SUPPRESS_FD_WARNINGS=1\") } );\n";
        if !contents.contains(needle) {
            bail!("kpmcore helper process setup no longer matches the expected upstream layout");
        }
        let mut adjusted = contents.replacen(needle, replacement, 1);
        let start_needle = "    cmd.start(command, arguments);\n    cmd.write(input);\n    cmd.closeWriteChannel();\n    cmd.waitForFinished(-1);\n";
        let start_replacement = "    cmd.start(command, arguments);\n    if (!cmd.waitForStarted(5000)) {\n        qWarning() << \"KPMCore helper could not start command\" << command << cmd.errorString();\n        reply[QStringLiteral(\"error\")] = cmd.errorString();\n        return reply;\n    }\n    cmd.write(input);\n    cmd.closeWriteChannel();\n    if (!cmd.waitForFinished(30000)) {\n        qWarning() << \"KPMCore helper command timed out\" << command;\n        cmd.kill();\n        cmd.waitForFinished(5000);\n        reply[QStringLiteral(\"error\")] = QStringLiteral(\"command timed out\");\n        return reply;\n    }\n    qInfo() << \"KPMCore helper command finished\" << command << cmd.exitCode();\n";
        if !adjusted.contains(start_needle) {
            bail!("kpmcore helper command execution no longer matches the expected upstream layout");
        }
        adjusted = adjusted.replacen(start_needle, start_replacement, 1);
        let event_loop_needle = "    PolkitQt1::Authority::Result result;\n    QEventLoop e;\n    connect(authority, &PolkitQt1::Authority::checkAuthorizationFinished, &e, [&e, &result](PolkitQt1::Authority::Result _result) {\n        result = _result;\n        e.quit();\n    });\n";
        let event_loop_replacement = "    PolkitQt1::Authority::Result result;\n";
        if !adjusted.contains(event_loop_needle) {
            bail!("kpmcore helper authorization event loop no longer matches the expected upstream layout");
        }
        adjusted = adjusted.replacen(event_loop_needle, event_loop_replacement, 1);
        let auth_needle = "    authority->checkAuthorization(QStringLiteral(\"org.kde.kpmcore.externalcommand.init\"), subject, PolkitQt1::Authority::AllowUserInteraction);\n    e.exec();\n";
        let auth_replacement = "    qInfo() << \"KPMCore helper requesting authorization\" << message().service();\n    result = authority->checkAuthorizationSync(QStringLiteral(\"org.kde.kpmcore.externalcommand.init\"), subject, PolkitQt1::Authority::AllowUserInteraction);\n    qInfo() << \"KPMCore helper authorization returned\" << result;\n";
        if !adjusted.contains(auth_needle) {
            bail!("kpmcore helper authorization flow no longer matches the expected upstream layout");
        }
        adjusted = adjusted.replacen(auth_needle, auth_replacement, 1);
        let root_auth_needle = "    if (!calledFromDBus()) {\n        return false;\n    }\n";
        let root_auth_replacement = "    if (!calledFromDBus()) {\n        return false;\n    }\n\n    // Calamares is launched through pkexec and therefore calls this helper\n    // from a root-owned D-Bus name.  Polkit's asynchronous Qt signal can be\n    // delivered before the local QEventLoop starts, which loses the quit\n    // event and deadlocks the partition model.  The system-bus UID is an\n    // authoritative local check; retain the Polkit path for non-root clients.\n    QDBusConnectionInterface *busInterface = QDBusConnection::systemBus().interface();\n    if (busInterface) {\n        const QDBusReply<uint> callerUid = busInterface->serviceUid(message().service());\n        if (callerUid.isValid() && callerUid.value() == 0) {\n            m_serviceWatcher->addWatchedService(message().service());\n            return true;\n        }\n    }\n";
        if !adjusted.contains(root_auth_needle) {
            bail!("kpmcore helper caller authorization no longer matches the expected upstream layout");
        }
        adjusted = adjusted.replacen(root_auth_needle, root_auth_replacement, 1);
        fs::write(helper, adjusted)?;
    }
    apply_component_patches(repo_root, component, &source_copy)?;
    if component == "ksysguard" {
        // Network and hardware-sensor backends are optional runtime features;
        // keep ProcessCore available to Plasma's task manager when the
        // corresponding target libraries are not part of this image.
        let cmake_lists = source_copy.join("CMakeLists.txt");
        if cmake_lists.is_file() {
            let contents = fs::read_to_string(&cmake_lists)?;
            let adjusted = contents
                .replace("find_package(NL)", "find_package(NL)")
                .replace("        TYPE REQUIRED\n        PURPOSE \"Used for gathering socket info via the sock_diag netlink subsystem.\"", "        TYPE OPTIONAL\n        PURPOSE \"Used for gathering socket info via the sock_diag netlink subsystem.\"")
                .replace("        TYPE REQUIRED\n        PURPOSE \"Used for reading hardware sensors\"", "        TYPE OPTIONAL\n        PURPOSE \"Used for reading hardware sensors\"");
            if adjusted != contents { fs::write(&cmake_lists, adjusted)?; }
        }
        let systemstats = source_copy.join("systemstats/CMakeLists.txt");
        if systemstats.is_file() {
            let contents = fs::read_to_string(&systemstats)?;
            let adjusted = contents.replace(
                "target_link_libraries(SystemStats PUBLIC ${SENSORS_LIBRARIES} KF6::I18n)",
                "target_link_libraries(SystemStats PRIVATE ${SENSORS_LIBRARIES} PUBLIC KF6::I18n)",
            );
            if adjusted != contents { fs::write(systemstats, adjusted)?; }
        }
    }
    if component == "ksystemstats" {
        // Upstream's Linux daemon initializes libsensors directly, but the
        // current CMake files only find Sensors and never attach its include
        // directory/library to the daemon or plugin targets. Keep this
        // source-owned dependency fix in the disposable mirror.
        for (relative, target) in [
            ("src/CMakeLists.txt", "ksystemstats_core"),
            ("plugins/lmsensors/CMakeLists.txt", "ksystemstats_plugin_lmsensors"),
            ("plugins/cpu/CMakeLists.txt", "ksystemstats_plugin_cpu"),
            ("plugins/gpu/CMakeLists.txt", "ksystemstats_plugin_gpu"),
        ] {
            let path = source_copy.join(relative);
            let contents = fs::read_to_string(&path)?;
            let link_scope = if matches!(target, "ksystemstats_plugin_cpu" | "ksystemstats_plugin_gpu") { "" } else { "PRIVATE " };
            let adjusted = format!("{contents}\ntarget_include_directories({target} PRIVATE ${{SENSORS_INCLUDE_DIR}})\ntarget_link_libraries({target} {link_scope}${{SENSORS_LIBRARIES}})\n");
            fs::write(path, adjusted)?;
        }
    }
    if component == "kscreenlocker" {
        // The Wayland-only MattOS build does not ship X11/XCB.  Keep the
        // upstream X11 probe in the disposable mirror, but make it optional
        // so the source can configure without host X11 development files.
        let cmake_lists = source_copy.join("CMakeLists.txt");
        if cmake_lists.is_file() {
            let contents = fs::read_to_string(&cmake_lists)?;
            let mut adjusted = contents
                .replace("find_package(X11)\nset_package_properties(X11 PROPERTIES TYPE REQUIRED", "find_package(X11)\nset_package_properties(X11 PROPERTIES TYPE OPTIONAL")
                .replace("TYPE REQUIRED\n                        PURPOSE \"Required for building the X11 based workspace\"", "TYPE OPTIONAL\n                        PURPOSE \"Required for building the X11 based workspace\"")
                .replace("find_package(XCB MODULE REQUIRED COMPONENTS XCB KEYSYMS XTEST)", "find_package(XCB MODULE QUIET COMPONENTS XCB KEYSYMS XTEST)")
                .replace("set_package_properties(XCB PROPERTIES TYPE REQUIRED", "set_package_properties(XCB PROPERTIES TYPE OPTIONAL");
            if let (Some(start), Some(end)) = (adjusted.find("find_package(X11)"), adjusted.find("find_package(WaylandScanner)")) {
                adjusted.replace_range(start..end, "# MattOS: X11/XCB screen-grabber support is disabled for the Wayland-only target.\n\n");
            }
            if adjusted != contents { fs::write(&cmake_lists, adjusted)?; }
            for relative in ["CMakeLists.txt", "greeter/CMakeLists.txt"] {
                let path = source_copy.join(relative);
                if path.is_file() {
                    let contents = fs::read_to_string(&path)?;
                    let adjusted = contents
                        .replace("   X11::X11\n", "")
                        .replace("   XCB::XCB\n", "")
                        .replace("   XCB::KEYSYMS\n", "")
                        .replace("    X11::X11\n", "")
                        .replace("    KF6::ScreenDpms\n", "");
                    if adjusted != contents { fs::write(path, adjusted)?; }
                }
            }
            for relative in ["CMakeLists.txt", "greeter/CMakeLists.txt"] {
                let path = source_copy.join(relative);
                if path.is_file() {
                    let contents = fs::read_to_string(&path)?;
                    let adjusted = contents
                        .replace("   X11::X11\n", "")
                        .replace("   XCB::XCB\n", "")
                        .replace("    X11::X11\n", "")
                        .replace("    KF6::ScreenDpms\n", "");
                    if adjusted != contents { fs::write(path, adjusted)?; }
                }
            }
        }
    }
    if component == "plasma-desktop" {
        // This imported Plasma revision still names the pre-split
        // KF6NotifyConfig widget package, while KF 6.26's KNotifications
        // source no longer ships that widget.  Accessibility's optional
        // notification KCM is not required by the Wayland shell; omit only
        // that KCM and its unavailable optional find component in the
        // disposable mirror.
        let cmake_lists = source_copy.join("CMakeLists.txt");
        if cmake_lists.is_file() {
            let contents = fs::read_to_string(&cmake_lists)?;
            let adjusted = contents
                .replace("    NotifyConfig\n", "")
                .replace("find_package(Plasma5Support ${PROJECT_DEP_VERSION} REQUIRED)\n", "find_package(Plasma5Support ${PROJECT_DEP_VERSION} REQUIRED)\n");
            let adjusted = adjusted
                // The Wayland session intentionally does not build ksmserver.
                // Plasma Desktop's XKB keyboard backend needs WITH_X11, but
                // KSMServerDBusInterface is only an X11-session interface and
                // is not referenced by this source tree beyond this probe.
                .replace("find_package(KSMServerDBusInterface CONFIG REQUIRED)\n", "")
                .replace("find_package(KSysGuard CONFIG REQUIRED)", "find_package(KSysGuard CONFIG)")
                .replace("find_package(XCB\n    REQUIRED COMPONENTS\n        XCB SHM IMAGE\n    OPTIONAL_COMPONENTS\n        XKB XINPUT ATOM RECORD\n)", "if(WITH_X11)\nfind_package(XCB\n    REQUIRED COMPONENTS\n        XCB SHM IMAGE\n    OPTIONAL_COMPONENTS\n        XKB XINPUT ATOM RECORD\n)\nendif()")
                .replace("pkg_check_modules(XKBREGISTRY xkbregistry REQUIRED IMPORTED_TARGET)", "if(WITH_X11)\npkg_check_modules(XKBREGISTRY xkbregistry REQUIRED IMPORTED_TARGET)\nendif()");
            let adjusted = adjusted.replace("include(CheckIncludeFiles)", "include(CheckIncludeFiles)\ninclude(CheckFunctionExists)");
            let adjusted = if component == "plasma-desktop" {
                adjusted.replace(
                    "include(CheckFunctionExists)\n",
                    &format!("include(CheckFunctionExists)\nset(CMAKE_CXX_FLAGS \"${{CMAKE_CXX_FLAGS}} -I{}\")\n", repo_root.join("out/build/kwindowsystem/source/src/platforms/xcb").display()),
                )
            } else { adjusted };
            let adjusted = adjusted
                .replace(
                    "find_package(X11)\nset_package_properties(X11 PROPERTIES\n    DESCRIPTION \"X11 libraries\"\n    URL \"https://www.x.org\"\n    PURPOSE \"Required for building the X11 based workspace\"\n    TYPE REQUIRED\n)\n\nif(X11_FOUND)\n  set(HAVE_X11 1)\nendif()\n",
                    "if(WITH_X11)\nfind_package(X11)\nset_package_properties(X11 PROPERTIES\n    DESCRIPTION \"X11 libraries\"\n    URL \"https://www.x.org\"\n    PURPOSE \"Required for building the X11 based workspace\"\n    TYPE OPTIONAL\n)\n\nif(X11_FOUND)\n  set(HAVE_X11 1)\nendif()\nendif()\n",
                )
                // kaccess shares generated settings code with the omitted
                // NotifyConfig accessibility KCM.  It is an X11-only daemon,
                // not the Wayland keyboard-layout backend being enabled.
                .replace("if(X11_Xkb_FOUND AND XCB_XKB_FOUND)\n    add_subdirectory(kaccess)\nendif()", "# MattOS: X11-only kaccess daemon omitted from the Wayland session")
                .replace("    PURPOSE \"Required for building the X11 based workspace\"\n    TYPE REQUIRED", "    PURPOSE \"Required for building the X11 based workspace\"\n    TYPE OPTIONAL")
                .replace("    PURPOSE \"Support audible bell in kaccess\"\n    TYPE REQUIRED", "    PURPOSE \"Support audible bell in kaccess\"\n    TYPE OPTIONAL")
                .replace("-DBUILD_KCM_TOUCHPAD_X11=OFF", "-DBUILD_KCM_TOUCHPAD_X11=OFF");
            if adjusted != contents { fs::write(&cmake_lists, adjusted)?; }
            let applets = source_copy.join("applets/CMakeLists.txt");
            if applets.is_file() {
                let contents = fs::read_to_string(&applets)?;
                let adjusted = contents
                    .replace("add_subdirectory(window-list)", "if(WITH_X11)\nadd_subdirectory(window-list)\nendif()");
                if adjusted != contents { fs::write(applets, adjusted)?; }
            }
            // Plasma's pager supports Wayland, but this release includes the
            // X11 drag-and-drop model header unconditionally.  Keep the real
            // pager in the Wayland desktop and compile only the backend that
            // the selected session can use; Xwayland applications are still
            // represented by KWin's Wayland task-management protocol.
            let pager = source_copy.join("applets/pager/pagermodel.cpp");
            if pager.is_file() {
                let contents = fs::read_to_string(&pager)?;
                let mut adjusted = contents
                    .replace("#if HAVE_X11\n#include <xwindowtasksmodel.h>\n#endif", "#if defined(HAVE_X11) && HAVE_X11\n#include <xwindowtasksmodel.h>\n#endif")
                    .replace("#ifdef HAVE_X11\n#include <xwindowtasksmodel.h>\n#endif", "#if defined(HAVE_X11) && HAVE_X11\n#include <xwindowtasksmodel.h>\n#endif")
                    .replace(
                        "    if (KWindowSystem::isPlatformX11()) {\n        indices = findWindows(TaskManager::XWindowTasksModel::winIdsFromMimeData(mimeData, &ok));\n    } else if (KWindowSystem::isPlatformWayland()) {",
                        "#if defined(HAVE_X11) && HAVE_X11\n    if (KWindowSystem::isPlatformX11()) {\n        indices = findWindows(TaskManager::XWindowTasksModel::winIdsFromMimeData(mimeData, &ok));\n    } else\n#endif\n    if (KWindowSystem::isPlatformWayland()) {",
                    );
                if !adjusted.contains("#if defined(HAVE_X11) && HAVE_X11\n#include <xwindowtasksmodel.h>\n#endif") {
                    adjusted = adjusted.replace("#include <xwindowtasksmodel.h>", "#if defined(HAVE_X11) && HAVE_X11\n#include <xwindowtasksmodel.h>\n#endif");
                }
                // WITH_X11 is enabled solely to build the XKB keyboard
                // backend.  LibTaskManager remains Wayland-only, so the
                // pager must not compile its X11 drag/drop branch merely
                // because config-X11.h reports that X11 headers exist.
                adjusted = adjusted
                    .replace("#if defined(HAVE_X11) && HAVE_X11\n#include <xwindowtasksmodel.h>", "#if 0 // MattOS: no Plasma X11 session/task model\n#include <xwindowtasksmodel.h>")
                    .replace("#if defined(HAVE_X11) && HAVE_X11\n    if (KWindowSystem::isPlatformX11())", "#if 0 // MattOS: no Plasma X11 session/task model\n    if (KWindowSystem::isPlatformX11())")
                    .replace("#if HAVE_X11\n    if (KWindowSystem::isPlatformX11()", "#if 0 // MattOS: no Plasma X11 session/task model\n    if (KWindowSystem::isPlatformX11()");
                if !adjusted.contains("#include <QMimeData>") {
                    adjusted = adjusted.replace("#include <QDBusConnection>", "#include <QDBusConnection>\n#include <QMimeData>");
                }
                if !adjusted.contains("#if 0 // MattOS: no Plasma X11 session/task model\n#include <xwindowtasksmodel.h>\n#endif")
                    || !adjusted.contains("#include <QMimeData>")
                {
                    bail!("plasma-desktop pager Wayland adaptation no longer matches upstream source");
                }
                if adjusted != contents { fs::write(pager, adjusted)?; }
            }
        }
        let kcms = source_copy.join("kcms/CMakeLists.txt");
        if kcms.is_file() {
            let contents = fs::read_to_string(&kcms)?;
            let adjusted = contents.replace("add_subdirectory( access )", "# MattOS: optional NotifyConfig accessibility KCM omitted");
            if adjusted != contents { fs::write(&kcms, adjusted)?; }
        }
        // knetattach is an optional KIO GUI utility, not part of the
        // Wayland desktop shell. Keep it out of the selected closure rather
        // than introducing a partial KIOGui runtime dependency.
        let cmake_lists = source_copy.join("CMakeLists.txt");
        if cmake_lists.is_file() {
            let contents = fs::read_to_string(&cmake_lists)?;
            let adjusted = contents.replace(
                "add_subdirectory(knetattach)",
                "# MattOS: optional KIO network-attach utility omitted from the Wayland shell",
            );
            if adjusted != contents { fs::write(cmake_lists, adjusted)?; }
        }
        // The activity switcher is shared by Wayland and X11, but this
        // imported Plasma revision leaves its X11 model header include and
        // one X11-only drag path unconditional.  LibTaskManager is correctly
        // built with WITH_X11=OFF here, so guard only those X11 references;
        // the Wayland model remains fully available to the shell.
        let activity_manager = source_copy.join("imports/activitymanager/switcherbackend.cpp");
        if activity_manager.is_file() {
            let contents = fs::read_to_string(&activity_manager)?;
            let adjusted = contents
                .replace("#include <KWindowInfo>\n", "#if HAVE_X11\n#include <KWindowInfo>\n#endif\n")
                .replace("#include <KX11Extras>\n", "#if HAVE_X11\n#include <KX11Extras>\n#endif\n")
                .replace("#include <xwindowtasksmodel.h>\n", "#if HAVE_X11\n#include <xwindowtasksmodel.h>\n#endif\n")
                .replace(
                    "    if (KWindowSystem::isPlatformX11()) {\n        return TaskManager::XWindowTasksModel::winIdsFromMimeData(mimeData).count();\n    }\n",
                    "#if HAVE_X11\n    if (KWindowSystem::isPlatformX11()) {\n        return TaskManager::XWindowTasksModel::winIdsFromMimeData(mimeData).count();\n    }\n#endif\n",
                );
            // As with the pager, HAVE_X11 now means only that the keyboard
            // backend can use XKB.  The Plasma session and LibTaskManager are
            // still Wayland-only, so every X11 activity-switcher branch must
            // remain excluded.
            let adjusted = adjusted.replace("#if HAVE_X11", "#if 0 // MattOS: no Plasma X11 session/task model");
            if adjusted != contents { fs::write(activity_manager, adjusted)?; }
        }
    }
    // The upstream killer helper is only useful for the X11 session and
    // includes Qt X11-private headers.  Keep the Wayland-only KWin build
    // independent of Qt X11 Extras while preserving the upstream helper when
    // KWIN_BUILD_X11 is enabled.
    if component == "kwin" {
        let helpers = source_copy.join("src/helpers/CMakeLists.txt");
        if helpers.is_file() {
            let contents = fs::read_to_string(&helpers)?;
            let adjusted = contents.replace("add_subdirectory(killer)", "if(KWIN_BUILD_X11)\nadd_subdirectory(killer)\nendif()") ;
            if adjusted != contents { fs::write(helpers, adjusted)?; }
        }
        let libdrm_find = source_copy.join("cmake/modules/FindLibdrm.cmake");
        if libdrm_find.is_file() {
            let contents = fs::read_to_string(&libdrm_find)?;
            let adjusted = contents.replace(
                "INTERFACE_INCLUDE_DIRECTORIES \"${Libdrm_INCLUDE_DIR}\"\n            INTERFACE_INCLUDE_DIRECTORIES \"${Libdrm_INCLUDE_DIR}/libdrm\"",
                "INTERFACE_INCLUDE_DIRECTORIES \"${Libdrm_INCLUDE_DIR};${Libdrm_INCLUDE_DIR}/libdrm\"",
            );
            if adjusted != contents { fs::write(libdrm_find, adjusted)?; }
        }
    }
    if component == "plasma-workspace" {
        // These two upstream subprojects are optional desktop conveniences,
        // not part of the Wayland shell.  They require KF6TextEditor and
        // QtTextToSpeech, which are deliberately outside MattOS's shell
        // closure; omit them in the disposable build mirror while retaining
        // the core workspace and its other control modules.
        let cmake_lists = source_copy.join("CMakeLists.txt");
        let contents = fs::read_to_string(&cmake_lists)?;
        let adjusted = contents
            .replace("Svg TextEditor TextWidgets Wallet ColorScheme", "Svg TextWidgets ColorScheme")
            .replace("add_subdirectory(interactiveconsole)", "# MattOS: interactive console requires the optional KF6TextEditor closure")
            .replace("ecm_optional_add_subdirectory(xembed-sni-proxy)", "# MattOS: X11 tray proxy omitted from Wayland-only session");
        let adjusted = adjusted.replace(
            "find_package(KSysGuard ${PROJECT_DEP_VERSION} CONFIG)\nset_package_properties(KSysGuard PROPERTIES\n    DESCRIPTION \"Components to monitor the system\"\n    TYPE REQUIRED\n)\n",
            "# MattOS: KSysGuard is outside the core Wayland shell closure\n",
        );
        if adjusted != contents { fs::write(&cmake_lists, adjusted)?; }
        // The logout greeter is an X11 helper.  The Wayland-only workspace
        // does not provide X11::X11, and building this helper would pull the
        // forbidden X11 session dependency into the target closure.
        let logout = source_copy.join("logout-greeter/CMakeLists.txt");
        if logout.is_file() {
            let contents = fs::read_to_string(&logout)?;
            if !contents.starts_with("# MattOS: X11-only") {
                fs::write(&logout, "# MattOS: X11-only logout greeter omitted from Wayland-only shell.\n")?;
            }
        }
        let kcms = source_copy.join("kcms/CMakeLists.txt");
        if kcms.is_file() {
            let contents = fs::read_to_string(&kcms)?;
            let adjusted = contents
                .replace("add_subdirectory(krdb)", "add_subdirectory(krdb)")
                .replace("add_subdirectory(users)", "# MattOS: users KCM requires optional KF6TextEditor/Wallet UI")
                .replace("add_subdirectory(colors)", "# MattOS: color KCM requires X11 targets and is omitted from Wayland-only shell");
            if adjusted != contents { fs::write(kcms, adjusted)?; }
        }
        // The upstream Qt protocol helper expands Wayland_DATADIR too early
        // in this tree and emits /wayland.xml. Pin the target-owned protocol
        // file in the disposable mirror instead of allowing host metadata to
        // satisfy generated sources.
        let notifications = source_copy.join("applets/notifications/CMakeLists.txt");
        if notifications.is_file() {
            let contents = fs::read_to_string(&notifications)?;
            let wayland_xml = repo_root.join("out/build/wayland/install/usr/share/wayland/wayland.xml");
            let adjusted = contents.replace("${Wayland_DATADIR}/wayland.xml", &wayland_xml.display().to_string());
            if adjusted != contents { fs::write(notifications, adjusted)?; }
        }
        let klipper = source_copy.join("klipper");
        let root_cmake = source_copy.join("CMakeLists.txt");
        if klipper.is_dir() && root_cmake.is_file() {
            let contents = fs::read_to_string(&root_cmake)?;
            // Klipper has an optional X11 link section, but its clipboard
            // daemon and Wayland QML provider are valid without X11.  Keep
            // the complete clipboard provider in the Wayland shell.
            let adjusted = contents.replace("add_subdirectory(klipper)", "add_subdirectory(klipper)");
            if adjusted != contents { fs::write(&root_cmake, adjusted)?; }
        }
        let adjusted_root = if root_cmake.is_file() {
            let contents = fs::read_to_string(&root_cmake)?;
            contents.replace("add_subdirectory(appmenu)", "# MattOS: appmenu requires the omitted XCB desktop integration")
        } else { String::new() };
        if !adjusted_root.is_empty() {
            let current = fs::read_to_string(&root_cmake)?;
            if adjusted_root != current { fs::write(&root_cmake, adjusted_root)?; }
        }
        let taskmanager = source_copy.join("libtaskmanager/virtualdesktopinfo.cpp");
        if taskmanager.is_file() {
            let contents = fs::read_to_string(&taskmanager)?;
            let adjusted = contents
                .replace("#include <KX11Extras>", "// MattOS: KX11Extras is only used by the HAVE_X11 implementation below")
                .replace("namespace X11Info\n{", "#if HAVE_X11\nnamespace X11Info\n{")
                .replace("}\n\nnamespace TaskManager", "}\n#endif\n\nnamespace TaskManager");
            if adjusted != contents { fs::write(taskmanager, adjusted)?; }
        }
        let notification_server = source_copy.join("libnotificationmanager/server.cpp");
        if notification_server.is_file() {
            let contents = fs::read_to_string(&notification_server)?;
            let adjusted = contents
                .replace("#include <KStartupInfo>", "// MattOS: KStartupInfo is unavailable in the Wayland-only KWindowSystem build")
                .replace("        KStartupInfoId startupId;\n        startupId.initId();\n\n        Q_EMIT d->ActivationToken(notificationId, QString::fromUtf8(startupId.id()));\n\n", "        // No X11 startup ID is available in the Wayland-only target.\n        Q_EMIT d->ActivationToken(notificationId, QString());\n\n");
            if adjusted != contents { fs::write(notification_server, adjusted)?; }
        }
        let panelview = source_copy.join("shell/panelview.cpp");
        if panelview.is_file() {
            let contents = fs::read_to_string(&panelview)?;
            let adjusted = contents
                .replace("#include <KX11Extras>", "// MattOS: KX11Extras is unavailable in the Wayland-only target")
                .replace("!KWindowSystem::isPlatformX11() || KX11Extras::compositingActive()", "!KWindowSystem::isPlatformX11() || true")
                .replace("KX11Extras::setState(winId(), NET::SkipSwitcher | NET::KeepAbove);", "// X11-only window state is not applicable on Wayland.")
                .replace("KX11Extras::forceActiveWindow(winId());", "// Wayland activation is handled by the compositor.");
            if adjusted != contents { fs::write(panelview, adjusted)?; }
        }
        let desktopview = source_copy.join("shell/desktopview.cpp");
        if desktopview.is_file() {
            let contents = fs::read_to_string(&desktopview)?;
            let adjusted = contents
                .replace("#include <KStartupInfo>", "// MattOS: KStartupInfo is unavailable without the X11 session")
                .replace("#include <KX11Extras>", "// MattOS: KX11Extras is unavailable without the X11 session")
                .replace("qGuiApp->nativeInterface<QNativeInterface::QX11Application>()", "nullptr")
                .replace("if (window && qGuiApp->nativeInterface<QNativeInterface::QX11Application>())", "if (false)")
                .replace("    } else {\n        KX11Extras::setType(winId(), NET::Desktop);\n        KX11Extras::setState(winId(), NET::KeepBelow);\n    }", "    } else {\n        // Wayland has no X11 desktop window type.\n    }")
                .replace("                KStartupInfo::setNewStartupId(window, qgetenv(\"DESKTOP_STARTUP_ID\"));", "                // Wayland activation is handled by KWaylandExtras.")
                .replace("        KStartupInfo::setNewStartupId(window, qgetenv(\"DESKTOP_STARTUP_ID\"));", "        // Wayland activation is handled by KWaylandExtras.");
            if adjusted != contents { fs::write(desktopview, adjusted)?; }
        }
        // These headers are only meaningful for the optional X11 code paths;
        // this build deliberately disables the X11 session while retaining
        // Xwayland support. Remove unconditional includes from the upstream
        // sources so Wayland-only compilation does not require KF X11 extras.
        for relative in [
            "shell/shellcorona.cpp",
            "applets/kicker/dashboardwindow.cpp",
            "applets/kicker/windowsystem.cpp",
            "applets/notifications/notificationapplet.cpp",
            "applets/notifications/notificationwindow.cpp",
            "gmenu-dbusmenu-proxy/menuproxy.cpp",
            "klipper/klipperpopup.cpp",
            "krunner/view.cpp",
            "ksmserver/legacy.cpp",
            "shell/osdwindow.cpp",
            "shell/panelconfigview.cpp",
            "soliduiserver/soliduiserver.cpp",
        ] {
            let path = source_copy.join(relative);
            if path.is_file() {
                let contents = fs::read_to_string(&path)?;
                let mut adjusted = contents
                .replace("#include <KX11Extras>", "// MattOS: KX11Extras omitted for Wayland-only target")
                .replace("#include <KStartupInfo>", "// MattOS: KStartupInfo omitted for Wayland-only target");
            if relative == "applets/kicker/windowsystem.cpp" {
                adjusted = adjusted.replace(
                    "            KX11Extras::forceActiveWindow(window->winId());",
                    "            // Wayland activation is handled by the compositor.",
                );
            }
                if adjusted != contents { fs::write(path, adjusted)?; }
            }
        }
        let klipper_popup = source_copy.join("klipper/klipperpopup.cpp");
        if klipper_popup.is_file() {
            let contents = fs::read_to_string(&klipper_popup)?;
            let adjusted = contents
                .replace(
                    "        KX11Extras::setOnAllDesktops(winId(), true);",
                    "#ifdef HAVE_X11\n        KX11Extras::setOnAllDesktops(winId(), true);\n#endif",
                )
                .replace(
                    "        KX11Extras::forceActiveWindow(winId());",
                    "#ifdef HAVE_X11\n        KX11Extras::forceActiveWindow(winId());\n#endif",
                )
                .replace(
                    "        KX11Extras::setOnDesktop(winId(), KX11Extras::currentDesktop());",
                    "#ifdef HAVE_X11\n        KX11Extras::setOnDesktop(winId(), KX11Extras::currentDesktop());\n#endif",
                );
            if adjusted != contents { fs::write(klipper_popup, adjusted)?; }
        }
        let krunner_view = source_copy.join("krunner/view.cpp");
        if krunner_view.is_file() {
            let contents = fs::read_to_string(&krunner_view)?;
            let adjusted = contents
                .replace("#include <KX11Extras>", "// MattOS: no X11 session")
                .replace("KX11Extras::setOnAllDesktops(winId(), true);", "// X11-only window state omitted.")
                .replace("KX11Extras::forceActiveWindow(winId());", "// Wayland activation handled by the compositor.")
                .replace("KX11Extras::setOnDesktop(winId(), KX11Extras::currentDesktop());", "// X11-only desktop assignment omitted.")
                .replace("KX11Extras::setOnAllDesktops(winId(), true);", "// X11-only desktop assignment omitted.")
                .replace("KX11Extras::setState(winId(), NET::SkipTaskbar | NET::SkipPager);", "// X11-only window state omitted.");
            if adjusted != contents { fs::write(krunner_view, adjusted)?; }
        }
        for relative in ["applets/kicker/dashboardwindow.cpp", "applets/notifications/notificationapplet.cpp", "applets/notifications/notificationwindow.cpp"] {
            let path = source_copy.join(relative);
            if path.is_file() {
                let contents = fs::read_to_string(&path)?;
                let adjusted = contents
                    .replace("#include <KX11Extras>", "// MattOS: no X11 session")
                    .replace("KX11Extras::forceActiveWindow(window->winId());", "// Wayland activation is handled by the compositor.")
                    .replace("KX11Extras::forceActiveWindow(window->winId());", "// Wayland activation is handled by the compositor.")
                    .replace("KX11Extras::forceActiveWindow(winId());", "// Wayland activation is handled by the compositor.")
                    .replace("KX11Extras::setOnAllDesktops(winId(), true);", "// X11-only window state omitted.")
                    .replace("KX11Extras::setType(winId(), NET::Notification);", "// X11-only window type omitted.")
                    .replace("KX11Extras::setType(winId(), critical ? NET::CriticalNotification : NET::Notification);", "// X11-only window type omitted.")
                    .replace("KX11Extras::setState(winId(), NET::SkipTaskbar | NET::SkipPager | NET::SkipSwitcher);", "// X11-only window state omitted.");
                if adjusted != contents { fs::write(path, adjusted)?; }
            }
        }
        let solidui = source_copy.join("soliduiserver/CMakeLists.txt");
        if solidui.is_file() {
            let contents = fs::read_to_string(&solidui)?;
            if contents.contains("add_definitions") {
                fs::write(&solidui, "# MattOS: optional Solid wallet integration omitted from the core shell.\n")?;
            }
        }
    }
    if component == "libkscreen" {
        // The target-owned XCB aggregate is now present, so retain
        // ScreenDpms: PowerDevil consumes that public ABI even in a native
        // Wayland session.  Only avoid Qt's non-installed qtx11extras private
        // header in backend selection; XRandR remains a compatibility backend,
        // not a separately advertised Plasma X11 session.
        let backend_manager = source_copy.join("src/backendmanager.cpp");
        if backend_manager.is_file() {
            let contents = fs::read_to_string(&backend_manager)?;
            let adjusted = contents
                .replace("#include <QtGui/private/qtx11extras_p.h>\n", "")
                .replace("QX11Info::isPlatformX11()", "QGuiApplication::platformName() == QStringLiteral(\"xcb\")");
            if adjusted != contents { fs::write(backend_manager, adjusted)?; }
        }
    }
    if component == "plasma-framework" {
        // FindWayland does not preserve the caller-provided Wayland_DATADIR
        // as a normal CMake variable, while PlasmaQuick uses that variable
        // to generate core Wayland protocol sources.  Pin the protocol file
        // in the disposable mirror to the source-owned Wayland stage rather
        // than letting the build silently resolve /wayland.xml or a host
        // protocol installation.
        let cmake_lists = source_copy.join("src/plasmaquick/CMakeLists.txt");
        if cmake_lists.is_file() {
            let contents = fs::read_to_string(&cmake_lists)?;
            let wayland_xml = repo_root.join("out/build/wayland/install/usr/share/wayland/wayland.xml");
            let adjusted = contents.replace("${Wayland_DATADIR}/wayland.xml", &wayland_xml.display().to_string());
            if adjusted != contents { fs::write(cmake_lists, adjusted)?; }
        }
        let theme = source_copy.join("src/plasma/private/theme_p.cpp");
        if theme.is_file() {
            let contents = fs::read_to_string(&theme)?;
            let adjusted = contents
                .replace("#include <KX11Extras>", "#if HAVE_X11\n#include <KX11Extras>\n#endif")
                .replace("    if (KWindowSystem::isPlatformX11()) {\n        compositingActive = KX11Extras::self()->compositingActive();\n    }", "#if HAVE_X11\n    if (KWindowSystem::isPlatformX11()) {\n        compositingActive = KX11Extras::self()->compositingActive();\n    }\n#endif")
                .replace("    if (KWindowSystem::isPlatformX11()) {\n        connect(KX11Extras::self(), &KX11Extras::compositingChanged, selectorsUpdateTimer, qOverload<>(&QTimer::start));\n    }", "#if HAVE_X11\n    if (KWindowSystem::isPlatformX11()) {\n        connect(KX11Extras::self(), &KX11Extras::compositingChanged, selectorsUpdateTimer, qOverload<>(&QTimer::start));\n    }\n#endif");
            if adjusted != contents { fs::write(theme, adjusted)?; }
        }
        let blur_header = source_copy.join("src/plasma/private/blureffectwatcher_p.h");
        if blur_header.is_file() {
            let contents = fs::read_to_string(&blur_header)?;
            let adjusted = contents.replace(
                "#endif\n\nQ_SIGNALS:",
                "#else\n    bool nativeEventFilter(const QByteArray &, void *, qintptr *) override { return false; }\n#endif\n\nQ_SIGNALS:",
            );
            if adjusted != contents { fs::write(blur_header, adjusted)?; }
        }
        // The Wayland-only Plasma build still compiles a few source files
        // shared with the optional X11 implementation.  Upstream guards the
        // implementation but not every legacy include/call.  Keep this
        // compatibility cleanup confined to the disposable mirror; the
        // production closure intentionally has no X11 KWindowSystem ABI.
        for relative in [
            "src/plasmaquick/appletpopup.cpp",
            "src/plasmaquick/plasmawindow.cpp",
            "src/plasmaquick/dialog.cpp",
            "src/declarativeimports/core/windowthumbnail.cpp",
        ] {
            let path = source_copy.join(relative);
            if !path.is_file() { continue; }
            let contents = fs::read_to_string(&path)?;
            let mut adjusted = contents
                .replace("#include \"appletpopup.h\"", "#include \"appletpopup.h\"\n#include \"config-plasma.h\"")
                .replace("#include \"plasmawindow.h\"", "#include \"plasmawindow.h\"\n#include \"config-plasma.h\"")
                .replace("#include \"dialog.h\"", "#include \"dialog.h\"\n#include \"config-plasma.h\"")
                .replace("#include <KX11Extras>", "#if HAVE_X11\n#include <KX11Extras>\n#endif")
                .replace("#include <KWindowInfo>", "#if HAVE_X11\n#include <KWindowInfo>\n#endif")
                .replace("KX11Extras::self()->compositingActive()", "true")
                .replace("KX11Extras::compositingActive()", "true")
                .replace("KX11Extras::self()->hasWId(winId)", "true")
                .replace("KX11Extras::self()->hasWId(m_winId)", "false")
                .replace("KX11Extras::self()->icon(m_winId, boundingRect().width(), boundingRect().height())", "QIcon()")
                .replace("        connect(KX11Extras::self(), &KX11Extras::compositingChanged, selectorsUpdateTimer, qOverload<>(&QTimer::start));", "        /* X11-only compositing notification omitted in Wayland build. */")
                .replace("KX11Extras::setType(winId(), NET::AppletPopup);", "/* X11-only */")
                .replace("KX11Extras::setType(q->winId(), static_cast<NET::WindowType>(type));", "/* X11-only */")
                .replace("KX11Extras::setOnAllDesktops(q->winId(), true);", "/* X11-only */")
                .replace("KX11Extras::setOnAllDesktops(q->winId(), false);", "/* X11-only */")
                .replace("KX11Extras::setState(winId(), NET::SkipTaskbar | NET::SkipPager | NET::SkipSwitcher);", "/* X11-only */")
                .replace("KX11Extras::setState(winId(), NET::SkipTaskbar | NET::SkipPager | NET::SkipSwitcher);", "/* X11-only */");
            if relative.ends_with("dialog.cpp") {
                adjusted = adjusted.replace(
                    "        const KWindowInfo winInfo(item->window()->winId(), NET::WMWindowType);\n        outsideParentWindow = outsideParentWindow || (winInfo.windowType(NET::AllTypesMask) == NET::Dock && item->window()->mask().isNull());",
                    "        /* X11-only window type query omitted in Wayland build. */",
                );
            }
            if adjusted != contents { fs::write(path, adjusted)?; }
        }
    }
    remove_path_if_exists(&build)?;
    remove_path_if_exists(&install)?;
    fs::create_dir_all(&build)?;
    let mut prefixes = if qt_integration {
        kde_target_prefixes(repo_root, components)
    } else {
        components
            .iter()
            .map(|component| {
                repo_root
                    .join("out/build")
                    .join(component)
                    .join("install/usr")
            })
            .collect()
    };
    let ecm = kde_host_ecm_dir(repo_root)?;
    let linguist = kde_linguist_tools_bridge(&build)?;
    let mut command = vec![
        "-S".into(), source_copy.display().to_string(), "-B".into(), build.display().to_string(),
        "-G".into(), "Ninja".into(), "-DCMAKE_INSTALL_PREFIX=/usr".into(),
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu".into(),
        format!("-DECM_DIR={}", ecm.display()),
        format!("-DQt6LinguistTools_DIR={}", linguist.display()),
        format!("-DPython3_EXECUTABLE={}", qt_host_tool("python3")?.display()),
        format!("-DPython_EXECUTABLE={}", qt_host_tool("python3")?.display()),
        format!("-DGETTEXT_MSGFMT_EXECUTABLE={}", qt_host_tool("msgfmt")?.display()),
        format!("-DGETTEXT_MSGMERGE_EXECUTABLE={}", qt_host_tool("msgmerge")?.display()),
        format!("-DFLEX_EXECUTABLE={}", qt_host_tool("flex")?.display()),
        format!("-DBISON_EXECUTABLE={}", qt_host_tool("bison")?.display()),
        "-DBUILD_TESTING=OFF".into(), "-DBUILD_QCH=OFF".into(),
    ];
    if components.contains(&"kirigami") {
        // Keep the disposable QML discovery view inside the target search
        // roots so CMake's ONLY-mode find_file() can inspect its qmldir.
        // ECMFindQmlModule searches the absolute install path (normally
        // /usr/lib/<multiarch>/qml) through CMAKE_FIND_ROOT_PATH.  Mirror
        // that path below the disposable root; placing org/kde directly
        // below the root would make the probe look in root/usr/lib/... and
        // miss the module.
        prefixes.insert(0, build.join("mattos-qml"));
    }
    if components.contains(&"kirigami-addons") {
        command.push(format!(
            "-DKirigamiAddons_DIR={}/lib/x86_64-linux-gnu/cmake/KF6KirigamiAddons",
            repo_root.join("out/build/kirigami-addons/install/usr").display()
        ));
    }
    if qt_integration {
        command.extend(qt_target_cmake_args(repo_root, &prefixes)?);
    } else {
        command.extend(isolated_target_cmake_args(&prefixes)?);
    }
    if components.contains(&"qtbase") {
        let qtbase = repo_root.join("out/build/qtbase/install/usr");
        command.push(format!("-DQt6CorePrivate_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6CorePrivate", qtbase.display()));
        command.push(format!("-DQt6GuiPrivate_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6GuiPrivate", qtbase.display()));
    }
    // KDE's generated KF6 umbrella config is not reliable across separate
    // Debian multiarch prefixes. Pin each requested framework component to
    // its staged config directory; this remains generic and prevents host
    // framework discovery without duplicating package-specific workarounds.
    let framework_names = [
        ("karchive", "Archive"), ("kauth", "Auth"), ("kbookmarks", "Bookmarks"),
        ("kcodecs", "Codecs"), ("kcolorscheme", "ColorScheme"), ("kcompletion", "Completion"),
        ("kconfig", "Config"), ("kcoreaddons", "CoreAddons"), ("kcrash", "Crash"),
        ("kdbusaddons", "DBusAddons"), ("kglobalaccel", "GlobalAccel"), ("kguiaddons", "GuiAddons"),
        ("ki18n", "I18n"), ("kiconthemes", "IconThemes"), ("kitemmodels", "ItemModels"),
        ("kitemviews", "ItemViews"), ("kjobwidgets", "JobWidgets"), ("kpackage", "Package"),
        ("kparts", "Parts"), ("kservice", "Service"), ("ksvg", "Svg"),
        ("kconfigwidgets", "ConfigWidgets"), ("kxmlgui", "XmlGui"),
        ("ktexteditor", "TextEditor"), ("ktextwidgets", "TextWidgets"), ("kwallet", "Wallet"),
        ("kwidgetsaddons", "WidgetsAddons"), ("kwindowsystem", "WindowSystem"),
        ("kio", "KIO"), ("solid", "Solid"), ("attica", "Attica"), ("knotifications", "Notifications"),
        ("kcmutils", "KCMUtils"), ("kdoctools", "DocTools"), ("krunner", "Runner"),
        ("kfilemetadata", "FileMetaData"), ("kpty", "Pty"), ("purpose", "Purpose"),
        ("networkmanager-qt", "NetworkManagerQt"),
        ("kirigami-addons", "KirigamiAddons"), ("kquickcharts", "QuickCharts"),
    ];
    for (component_name, framework_name) in framework_names {
        if components.contains(&component_name) {
            command.push(format!("-DKF6{framework_name}_DIR={}/lib/x86_64-linux-gnu/cmake/KF6{framework_name}", repo_root.join("out/build").join(component_name).join("install/usr").display()));
        }
    }
    if components.contains(&"knewstuff") {
        command.push(format!("-DKF6NewStuffCore_DIR={}/lib/x86_64-linux-gnu/cmake/KF6NewStuffCore", repo_root.join("out/build/knewstuff/install/usr").display()));
    }
    if components.contains(&"kirigami") {
        command.push(format!(
            "-DKF6KirigamiPlatform_DIR={}/lib/x86_64-linux-gnu/cmake/KF6KirigamiPlatform",
            repo_root.join("out/build/kirigami/install/usr").display()
        ));
    }
    if components.contains(&"kwayland") {
        command.push(format!(
            "-DKWayland_DIR={}/lib/x86_64-linux-gnu/cmake/KWayland",
            repo_root.join("out/build/kwayland/install/usr").display()
        ));
    }
    if components.contains(&"plasma-activities") {
        command.push(format!(
            "-DPlasmaActivities_DIR={}/lib/x86_64-linux-gnu/cmake/PlasmaActivities",
            repo_root.join("out/build/plasma-activities/install/usr").display()
        ));
    }
    if components.contains(&"libkscreen") {
        command.push(format!(
            "-DKF6Screen_DIR={}/lib/x86_64-linux-gnu/cmake/KF6Screen",
            repo_root.join("out/build/libkscreen/install/usr").display()
        ));
    }
    if components.contains(&"layer-shell-qt") {
        command.push(format!(
            "-DLayerShellQt_DIR={}/lib/x86_64-linux-gnu/cmake/LayerShellQt",
            repo_root.join("out/build/layer-shell-qt/install/usr").display()
        ));
    }
    if components.contains(&"knighttime") {
        command.push(format!(
            "-DKNightTime_DIR={}/lib/x86_64-linux-gnu/cmake/KNightTime",
            repo_root.join("out/build/knighttime/install/usr").display()
        ));
    }
    if components.contains(&"plasma-wayland-protocols") {
        command.push(format!(
            "-DPlasmaWaylandProtocols_DIR={}/lib/x86_64-linux-gnu/cmake/PlasmaWaylandProtocols",
            repo_root.join("out/build/plasma-wayland-protocols/install/usr").display()
        ));
    }
    if components.contains(&"plasma-activities-stats") {
        command.push(format!(
            "-DPlasmaActivitiesStats_DIR={}/lib/x86_64-linux-gnu/cmake/PlasmaActivitiesStats",
            repo_root.join("out/build/plasma-activities-stats/install/usr").display()
        ));
    }
    if components.contains(&"xkbcommon") {
        let xkb = repo_root.join("out/build/xkbcommon/install/usr");
        let xkb_bridge = build.join("mattos-xkb-cmake/XKB");
        fs::create_dir_all(&xkb_bridge)?;
        fs::write(xkb_bridge.join("XKBConfig.cmake"), format!(
            "set(XKB_FOUND TRUE)\nset(XKB_VERSION 1.9.2)\nset(XKB_INCLUDE_DIRS {}/include)\nset(XKB_LIBRARIES XKB::XKB)\nif(NOT TARGET XKB::XKB)\n  add_library(XKB::XKB UNKNOWN IMPORTED)\n  set_target_properties(XKB::XKB PROPERTIES IMPORTED_LOCATION \"{}/lib/x86_64-linux-gnu/libxkbcommon.so\" INTERFACE_INCLUDE_DIRECTORIES \"{}/include\")\nendif()\n",
            xkb.display(), xkb.display(), xkb.display()))?;
        fs::write(xkb_bridge.join("XKBConfigVersion.cmake"), "set(PACKAGE_VERSION 1.9.2)\nset(PACKAGE_VERSION_COMPATIBLE TRUE)\nset(PACKAGE_VERSION_EXACT TRUE)\n")?;
        command.push(format!("-DXKB_DIR={}", xkb_bridge.display()));
    }
    if components.contains(&"wayland") {
        let wayland = repo_root.join("out/build/wayland/install/usr");
        let bridge = build.join("mattos-wayland-cmake/Wayland");
        fs::create_dir_all(&bridge)?;
        fs::write(bridge.join("WaylandConfig.cmake"), format!(
            "set(Wayland_FOUND TRUE)\nset(Wayland_VERSION 1.26.0)\nset(Wayland_INCLUDE_DIRS {}/include)\nset(Wayland_LIBRARIES Wayland::Client Wayland::Server Wayland::Cursor Wayland::Egl)\nset(_mattos_wayland_libdir {}/lib/x86_64-linux-gnu)\nforeach(name Client Server Cursor Egl)\n  string(TOLOWER ${{name}} lower)\n  if(NOT TARGET Wayland::${{name}})\n    add_library(Wayland::${{name}} UNKNOWN IMPORTED)\n    set_target_properties(Wayland::${{name}} PROPERTIES IMPORTED_LOCATION \"${{_mattos_wayland_libdir}}/libwayland-${{lower}}.so\" INTERFACE_INCLUDE_DIRECTORIES \"{}/include\")\n  endif()\nendforeach()\n",
            wayland.display(), wayland.display(), wayland.display()))?;
        fs::write(bridge.join("WaylandConfigVersion.cmake"), "set(PACKAGE_VERSION 1.26.0)\nset(PACKAGE_VERSION_COMPATIBLE TRUE)\nset(PACKAGE_VERSION_EXACT TRUE)\n")?;
        command.push(format!("-DWayland_DIR={}", bridge.display()));
    }
    if components.contains(&"qtdeclarative") {
        let qt_declarative = repo_root.join("out/build/qtdeclarative/install/usr");
        command.push(format!("-DQT_ADDITIONAL_PACKAGES_PREFIX_PATH={}", qt_declarative.display()));
        // Qt Declarative's target package records its build generators as
        // Qt6QmlTools.  Qt intentionally keeps this tool package in the
        // module build tree for cross builds; consume that verified tree
        // explicitly instead of searching a host Qt installation.
        let qt_declarative_cmake = qt_declarative.join("lib/x86_64-linux-gnu/cmake");
        command.push(format!("-DQt6Qml_DIR={}/Qt6Qml", qt_declarative_cmake.display()));
        command.push(format!("-DQt6Quick_DIR={}/Qt6Quick", qt_declarative_cmake.display()));
        let qt_shader_tools_cmake = repo_root.join("out/build/qtshadertools/install/usr/lib/x86_64-linux-gnu/cmake/Qt6ShaderTools");
        command.push(format!("-DQt6ShaderTools_DIR={}", qt_shader_tools_cmake.display()));
        qt_multimodule_cmake_view(repo_root, &build)?;
        command.push(format!("-DCMAKE_PROJECT_INCLUDE={}", kde_qt_tool_bridge(&build)?.display()));
    }
    if components.contains(&"qtmultimedia") {
        let multimedia = repo_root.join("out/build/qtmultimedia/install/usr");
        command.push(format!(
            "-DQt6Multimedia_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6Multimedia",
            multimedia.display()
        ));
    }
    if components.contains(&"qtspeech") {
        let speech = repo_root.join("out/build/qtspeech/install/usr");
        let multimedia = repo_root.join("out/build/qtmultimedia/install/usr");
        command.push(format!(
            "-DQt6TextToSpeech_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6TextToSpeech",
            speech.display()
        ));
        command.push(format!(
            "-DQt6Multimedia_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6Multimedia",
            multimedia.display()
        ));
    }
    if components.contains(&"qca") {
        let qca = repo_root.join("out/build/qca/install/usr");
        command.push(format!(
            "-DQca-qt6_DIR={}/lib/x86_64-linux-gnu/cmake/Qca-qt6",
            qca.display()
        ));
    }
    command.extend(options.iter().map(|option| (*option).to_owned()));
    if component == "plasma-framework" {
        let x11 = repo_root.join("out/build/x11-compat/install/usr/include");
        append_cmake_flag(&mut command, "-DCMAKE_C_FLAGS", &format!("-I{}", x11.display()));
        append_cmake_flag(&mut command, "-DCMAKE_CXX_FLAGS", &format!("-I{}", x11.display()));
    }
    let needs_x11 = components.contains(&"x11-compat") || components.contains(&"kwindowsystem");
    if needs_x11 {
        // FindX11 is not a config-package dependency. Seed its target-owned
        // include/library facts explicitly so X11-compat consumers (for
        // example the non-session color bridge) cannot probe the host.
        let x11_root = repo_root.join("out/build/x11-compat/install/usr");
        command.push(format!("-DX11_X11_INCLUDE_PATH={}/include", x11_root.display()));
        command.push(format!("-DX11_X11_LIB={}/lib/x86_64-linux-gnu/libX11.so", x11_root.display()));
        // FindX11/FindXCB can locate the aggregate libraries through the
        // isolated pkg-config view, but several upstream targets consume the
        // raw XCB headers without propagating the discovered include path.
        // Keep that include target-owned and tied to the declared aggregate.
        append_cmake_flag(&mut command, "-DCMAKE_C_FLAGS", &format!("-I{}/include", x11_root.display()));
        append_cmake_flag(&mut command, "-DCMAKE_CXX_FLAGS", &format!("-I{}/include", x11_root.display()));
    }
    if components.contains(&"qt5compat") {
        command.push(format!("-DQt6Core5Compat_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6Core5Compat", repo_root.join("out/build/qt5compat/install/usr").display()));
    }
    if components.contains(&"qtpositioning") {
        command.push(format!("-DQt6Positioning_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6Positioning", repo_root.join("out/build/qtpositioning/install/usr").display()));
        command.push(format!("-DQt6PositioningPrivate_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6PositioningPrivate", repo_root.join("out/build/qtpositioning/install/usr").display()));
    }
    // KF installs its package configs below the Debian multiarch libdir.  A
    // few consumers (notably KIconThemes) do not discover that location from
    // a multi-prefix CMAKE_PREFIX_PATH, so pin the exact staged config path.
    if components.contains(&"kcolorscheme") {
        command.push(format!(
            "-DKF6ColorScheme_DIR={}/lib/x86_64-linux-gnu/cmake/KF6ColorScheme",
            repo_root.join("out/build/kcolorscheme/install/usr").display()
        ));
    }
    if component == "plasma-workspace" || component == "plasma-desktop" {
        // ScreenSaverDBusInterface is a build-time interface package emitted
        // by kscreenlocker. Keep its XML in the disposable workspace mirror
        // so the generated bridge never points at an absent or host file.
        let dbus_dir = source_copy.join("dbus");
        fs::create_dir_all(&dbus_dir)?;
        let screensaver_xml = repo_root.join("src/desktop/kde/kscreenlocker/dbus/org.freedesktop.ScreenSaver.xml");
        fs::copy(&screensaver_xml, dbus_dir.join("org.freedesktop.ScreenSaver.xml"))?;
        let screensaver_bridge = build.join("mattos-screensaver-dbus/ScreenSaverDBusInterface");
        fs::create_dir_all(&screensaver_bridge)?;
        fs::write(screensaver_bridge.join("ScreenSaverDBusInterfaceConfig.cmake"), format!(
            "set(ScreenSaverDBusInterface_FOUND TRUE)\nset(SCREENSAVER_DBUS_INTERFACE \"{}/dbus/org.freedesktop.ScreenSaver.xml\")\n",
            source_copy.display()
        ))?;
        command.push(format!("-DScreenSaverDBusInterface_DIR={}", screensaver_bridge.display()));
    }
    if component == "plasma-workspace" {
        command.push(format!(
            "-DSCREENSAVER_DBUS_INTERFACE={}/dbus/org.freedesktop.ScreenSaver.xml",
            source_copy.display()
        ));
    }
    if components.contains(&"kirigami") {
        // ECM's QML-module probe searches KDE_INSTALL_FULL_QMLDIR.  Expose
        // the already-built source-owned Kirigami tree through a disposable
        // symlink view so QML discovery cannot fall back to the host.
        use std::os::unix::fs::symlink;
        let qml_root = build.join("mattos-qml");
        let qml_org_kde = qml_root.join("usr/lib/x86_64-linux-gnu/qml/org/kde");
        fs::create_dir_all(&qml_org_kde)?;
        let kirigami_link = qml_org_kde.join("kirigami");
        fs::create_dir_all(&kirigami_link)?;
        let kirigami_source = repo_root.join("out/build/kirigami/install/usr/lib/x86_64-linux-gnu/qml/org/kde/kirigami");
        for entry in fs::read_dir(kirigami_source)? {
            let entry = entry?;
            let destination = kirigami_link.join(entry.file_name());
            if destination.symlink_metadata().is_err() {
                // CMake's find_file() deliberately does not follow a
                // symlink for the module marker.  Keep the view disposable,
                // but copy that tiny marker and link the payload files.
                if entry.file_name() == "qmldir" {
                    fs::copy(entry.path(), destination)?;
                } else {
                    symlink(entry.path(), destination)?;
                }
            }
        }
        // The same ECM QML probe is used by the optional KDE QML modules
        // consumed by Plasma.  Expose each selected, target-owned module in
        // the disposable absolute-path view as well; otherwise CMake can
        // find the KF package config but not its QML dependency and silently
        // disables the feature.
        for (component, module) in [
            ("kirigami-addons", "kirigamiaddons"),
            ("kquickcharts", "quickcharts"),
            ("prison", "prison"),
            ("kitemmodels", "kitemmodels"),
            ("kcmutils", "kcmutils"),
        ] {
            if !components.contains(&component) {
                continue;
            }
            let source = repo_root
                .join("out/build")
                .join(component)
                .join("install/usr/lib/x86_64-linux-gnu/qml/org/kde")
                .join(module);
            if !source.is_dir() {
                bail!("target QML module {} is missing from {}", module, source.display());
            }
            let destination = qml_org_kde.join(module);
            fs::create_dir_all(&destination)?;
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                let target = destination.join(entry.file_name());
                if target.symlink_metadata().is_err() {
                    if entry.file_name() == "qmldir" {
                        fs::copy(entry.path(), target)?;
                    } else {
                        symlink(entry.path(), target)?;
                    }
                }
            }
        }
        // Keep ECM's configure-time QML lookup pointed at the disposable,
        // source-owned view above, but keep the install destination inside
        // the package's /usr tree.  Setting KDE_INSTALL_QMLDIR to the view
        // used to make `cmake --install` write QML modules into the build
        // directory itself; DESTDIR could not bring those files into
        // install/usr, so the published Plasma packages silently lost
        // org.kde.plasma.* at runtime.  The full path is only a discovery
        // input here; ECM's install DESTINATION must remain target-relative.
        command.push("-DKDE_INSTALL_QMLDIR=lib/x86_64-linux-gnu/qml".into());
        command.push(format!("-DKDE_INSTALL_FULL_QMLDIR={}", qml_root.display()));
        command.push("-DQT_MAJOR_VERSION=6".into());
        let bridge = kde_qt_tool_bridge(&build)?;
        command.push(format!("-DCMAKE_PROJECT_INCLUDE={}", bridge.display()));
    }
    if components.contains(&"qtlocation") {
        let qt_location = repo_root.join("out/build/qtlocation/install/usr");
        command.push(format!("-DQt6Location_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6Location", qt_location.display()));
        command.push(format!("-DQt6LocationPrivate_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6LocationPrivate", qt_location.display()));
        let qt_positioning = repo_root.join("out/build/qtpositioning/install/usr");
        command.push(format!("-DQt6PositioningQuick_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6PositioningQuick", qt_positioning.display()));
        command.push(format!("-DQt6PositioningQuickPrivate_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6PositioningQuickPrivate", qt_positioning.display()));
    }
    // QtSql is built as part of QtBase, but its standalone package is not
    // discoverable through the umbrella package when the disposable
    // multi-prefix view is used. Pin its real staged config directory so
    // consumers cannot fall back to a host Qt installation.
    if components.contains(&"qtbase") {
        command.push(format!("-DQt6Sql_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6Sql", repo_root.join("out/build/qtbase/install/usr").display()));
        command.push(format!("-DXKB_DIR={}/XKB", build.join("mattos-qt-cmake-view/lib/x86_64-linux-gnu/cmake").display()));
        command.push(format!("-DWayland_DIR={}/Wayland", build.join("mattos-qt-cmake-view/lib/x86_64-linux-gnu/cmake").display()));
        let wayland_usr = repo_root.join("out/build/wayland/install/usr");
        command.push(format!("-DWayland_Client_LIBRARY={}/lib/x86_64-linux-gnu/libwayland-client.so", wayland_usr.display()));
        command.push(format!("-DWayland_Server_LIBRARY={}/lib/x86_64-linux-gnu/libwayland-server.so", wayland_usr.display()));
        command.push(format!("-DWayland_Cursor_LIBRARY={}/lib/x86_64-linux-gnu/libwayland-cursor.so", wayland_usr.display()));
        command.push(format!("-DWayland_Egl_LIBRARY={}/lib/x86_64-linux-gnu/libwayland-egl.so", wayland_usr.display()));
        command.push(format!("-DWayland_Client_INCLUDE_DIR={}/include", wayland_usr.display()));
        command.push(format!("-DWayland_Server_INCLUDE_DIR={}/include", wayland_usr.display()));
        command.push(format!("-DWayland_Cursor_INCLUDE_DIR={}/include", wayland_usr.display()));
        command.push(format!("-DWayland_Egl_INCLUDE_DIR={}/include", wayland_usr.display()));
        command.push(format!("-DWayland_DATADIR={}/share/wayland", wayland_usr.display()));
        command.push(format!("-DWaylandScanner_EXECUTABLE={}/bin/wayland-scanner", wayland_usr.display()));
    }
    if components.contains(&"linux-pam") {
        let pam = repo_root.join("out/build/linux-pam/install/usr");
        command.push(format!("-DPAM_INCLUDE_DIR={}/include", pam.display()));
        command.push(format!("-DPAM_LIBRARY={}/lib/x86_64-linux-gnu/libpam.so", pam.display()));
    }
    if components.contains(&"libxcb") {
        let libxcb_include = repo_root.join("out/build/libxcb/install/usr/include");
        let mut c_flags = format!("-I{}", libxcb_include.display());
        let mut cxx_flags = c_flags.clone();
        if components.contains(&"x11-compat") {
            let x11_include = repo_root.join("out/build/x11-compat/install/usr/include");
            let kwin_xcb_include = repo_root.join("out/build/kwindowsystem/source/src/platforms/xcb");
            c_flags.push_str(&format!(" -I{}", x11_include.display()));
            cxx_flags.push_str(&format!(" -I{} -I{}", x11_include.display(), kwin_xcb_include.display()));
        }
        append_cmake_flag(&mut command, "-DCMAKE_C_FLAGS", &c_flags);
        append_cmake_flag(&mut command, "-DCMAKE_CXX_FLAGS", &cxx_flags);
    }
    if component == "kwin" || component == "kwindowsystem" {
        // KWin's X11 helper/main executable and KWindowSystem's XCB backend
        // use Qt's private qtx11extras_p.h. QtGuiPrivate exposes the
        // versioned private headers, but this Unix-platform header is
        // intentionally not installed by the QtBase target package. Keep
        // the include rooted in the source-owned QtBase mirror; never
        // satisfy it from a host Qt.
        let qt_x11_private = repo_root.join("out/build/qtbase/source/src/gui/platform/unix");
        append_cmake_flag(&mut command, "-DCMAKE_CXX_FLAGS", &format!("-I{}", qt_x11_private.display()));
    }
    if components.contains(&"libdrm") {
        // libdrm installs its public top-level headers beside (not below)
        // the libdrm subdirectory; expose both target-owned include roots.
        let libdrm_include = repo_root.join("out/build/libdrm/install/usr/include");
        append_cmake_flag(&mut command, "-DCMAKE_C_FLAGS", &format!("-I{} -I{}/libdrm", libdrm_include.display(), libdrm_include.display()));
        append_cmake_flag(&mut command, "-DCMAKE_CXX_FLAGS", &format!("-I{} -I{}/libdrm", libdrm_include.display(), libdrm_include.display()));
    }
    if components.contains(&"util-linux") {
        // util-linux's blkid.pc advertises `${includedir}/blkid`, which is
        // correct for its conventional <blkid.h> API but not sufficient for
        // KPMCore's upstream <blkid/blkid.h> include.  Keep the fix target
        // owned and generic: expose the util-linux include root alongside
        // the pkg-config-provided subdirectory without consulting the host.
        let util_linux_include = repo_root.join("out/build/util-linux/install/usr/include");
        append_cmake_flag(&mut command, "-DCMAKE_C_FLAGS", &format!("-I{}", util_linux_include.display()));
        append_cmake_flag(&mut command, "-DCMAKE_CXX_FLAGS", &format!("-I{}", util_linux_include.display()));
    }
    if components.contains(&"kpmcore") {
        // KPMcore's exported target currently exposes its include directory
        // as .../include/kpmcore, while Calamares includes the public API as
        // <kpmcore/...>. Add the target-owned parent include root explicitly.
        let kpmcore_include = repo_root.join("out/build/kpmcore/install/usr/include");
        append_cmake_flag(&mut command, "-DCMAKE_CXX_FLAGS", &format!("-I{}", kpmcore_include.display()));
    }
    if components.contains(&"networkmanager") {
        // libnm.pc intentionally exposes include/libnm for its conventional
        // <NetworkManager.h> API, while NetworkManagerQt's public headers use
        // <libnm/NetworkManager.h>. Add the source-owned include parent too.
        let nm_include = repo_root.join("out/build/networkmanager/install/usr/include");
        append_cmake_flag(&mut command, "-DCMAKE_C_FLAGS", &format!("-I{}", nm_include.display()));
        append_cmake_flag(&mut command, "-DCMAKE_CXX_FLAGS", &format!("-I{}", nm_include.display()));
    }
    if components.contains(&"lm-sensors") {
        let sensors = repo_root.join("out/build/lm-sensors/install/usr");
        command.push(format!("-DSENSORS_INCLUDE_DIR={}/include", sensors.display()));
        command.push(format!("-DSENSORS_LIBRARIES={}/lib/x86_64-linux-gnu/libsensors.so", sensors.display()));
    }
    if components.contains(&"qtdeclarative") {
        let qtbase_cmake = repo_root.join("out/build/qtbase/install/usr/lib/x86_64-linux-gnu/cmake");
        let qml_cmake = repo_root.join("out/build/qtdeclarative/install/usr/lib/x86_64-linux-gnu/cmake");
        let wayland_cmake = repo_root.join("out/build/qtwayland/install/usr/lib/x86_64-linux-gnu/cmake");
        let svg_cmake = repo_root.join("out/build/qtsvg/install/usr/lib/x86_64-linux-gnu/cmake");
        let qttools_cmake = repo_root.join("out/build/qttools/install/usr/lib/x86_64-linux-gnu/cmake");
        for (name, directory) in [("Qt6Concurrent", &qtbase_cmake), ("Qt6DBus", &qtbase_cmake), ("Qt6PrintSupport", &qtbase_cmake), ("Qt6Test", &qtbase_cmake), ("Qt6UiPlugin", &qttools_cmake), ("Qt6UiTools", &qttools_cmake), ("Qt6Widgets", &qtbase_cmake), ("Qt6Qml", &qml_cmake), ("Qt6Quick", &qml_cmake), ("Qt6WaylandClient", &wayland_cmake), ("Qt6WaylandClientPrivate", &qtbase_cmake), ("Qt6WaylandGlobalPrivate", &qtbase_cmake), ("Qt6Svg", &svg_cmake)] {
            command.push(format!("-D{name}_DIR={}/{name}", directory.display()));
        }
    }
    if components.contains(&"qtdeclarative") {
        let qt_view = build.join("mattos-qt-cmake-view");
        command.push(format!("-DQt6_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6", qt_view.display()));
        command.push(format!("-DQT_BUILD_CMAKE_PREFIX_PATH={};{}/lib/x86_64-linux-gnu/cmake", qt_view.display(), qt_view.display()));
        for name in ["Qt6Core", "Qt6Gui", "Qt6Network", "Qt6OpenGL", "Qt6OpenGLWidgets", "Qt6Xml", "Qt6PrintSupport", "Qt6Test", "Qt6Qml", "Qt6Quick", "Qt6Concurrent", "Qt6DBus", "Qt6UiPlugin", "Qt6UiTools", "Qt6Widgets", "Qt6WaylandClient", "Qt6WaylandClientPrivate", "Qt6WaylandGlobalPrivate", "Qt6Svg", "Qt6Core5Compat", "Qt6QmlTools"] {
            command.push(format!("-D{name}_DIR={}/lib/x86_64-linux-gnu/cmake/{name}", qt_view.display()));
        }
        // Qt QML's generated tools package is intentionally represented by
        // the verified target-side tool bridge above. Qt's own package check
        // otherwise rejects a bridge config that has no Qt-generated version
        // implementation file, even though the target Qt version is fixed by
        // this stage's Qt6 prefix.
        command.push("-DQT_NO_PACKAGE_VERSION_CHECK=TRUE".into());
        command.push("-DQT_ALLOW_MISSING_TOOLS_PACKAGES=TRUE".into());
    }
    if component == "kactivitymanagerd" {
        let boost = kde_host_boost_include_dir(repo_root)?;
        command.push(format!("-DBoost_NO_BOOST_CMAKE=ON"));
        command.push(format!("-DBoost_NO_SYSTEM_PATHS=ON"));
        command.push(format!("-DBoost_INCLUDE_DIR={}", boost.display()));
        // kactivitymanagerd uses header-only Boost code that exposes
        // __FILE__ in a few diagnostic strings.  The headers are an
        // explicitly permitted host build tool, so map their path just like
        // the source mirror; otherwise the target ELF would retain the host
        // checkout path even after debug sections are stripped.
        let boost_root = boost
            .parent()
            .ok_or_else(|| anyhow!("Boost include directory has no parent"))?;
        let boost_prefix_map = format!(
            "-ffile-prefix-map={}=/usr/include -fdebug-prefix-map={}=/usr/include -fmacro-prefix-map={}=/usr/include",
            boost_root.display(),
            boost_root.display(),
            boost_root.display()
        );
        append_cmake_flag(&mut command, "-DCMAKE_CXX_FLAGS", &boost_prefix_map);
    }
    // Keep target artifacts relocatable and independent of the disposable
    // source-mirror location.  This is shared by every KDE component because
    // generated diagnostics/metadata can otherwise embed the host checkout
    // and fail the host-path audit.  Append to component-specific flags rather
    // than replacing their include paths or optimization settings.
    let source_prefix = source_copy.display();
    let target_source = format!("/usr/src/mattos/{component}");
    let mut prefix_map = format!("-ffile-prefix-map={source_prefix}={target_source} -fdebug-prefix-map={source_prefix}={target_source} -fmacro-prefix-map={source_prefix}={target_source}");
    // Public target headers may legitimately expand __FILE__ in inline code
    // (Highway does this for diagnostics).  They live under output-owned
    // staged prefixes while building, but installed binaries must describe
    // the target filesystem rather than retain those checkout paths.
    for dependency in components {
        let staged_prefix = repo_root.join("out/build").join(dependency).join("install/usr");
        if staged_prefix.is_dir() {
            let staged_prefix = staged_prefix.display();
            prefix_map.push_str(&format!(
                " -ffile-prefix-map={staged_prefix}=/usr -fdebug-prefix-map={staged_prefix}=/usr -fmacro-prefix-map={staged_prefix}=/usr"
            ));
        }
    }
    for key in ["-DCMAKE_C_FLAGS=", "-DCMAKE_CXX_FLAGS="] {
        if let Some(argument) = command.iter_mut().find(|argument| argument.starts_with(key)) {
            argument.push_str(&format!(" {prefix_map}"));
        } else {
            command.push(format!("{key}{prefix_map}"));
        }
    }
    if components.contains(&"systemd") {
        // systemd's pkg-config metadata supplies the link dependency, but
        // some KDE sources include its public headers directly.  Keep that
        // include root target-owned and append it to any component-specific
        // flags instead of replacing existing prefix maps or ABI includes.
        let systemd_include = repo_root.join("out/build/systemd/install/usr/include");
        for key in ["-DCMAKE_C_FLAGS=", "-DCMAKE_CXX_FLAGS="] {
            if let Some(argument) = command.iter_mut().find(|argument| argument.starts_with(key)) {
                argument.push_str(&format!(" -I{} -D__STDC_VERSION__=0", systemd_include.display()));
            } else {
                command.push(format!("{key}-I{} -D__STDC_VERSION__=0", systemd_include.display()));
            }
        }
    }
    let refs = command.iter().map(String::as_str).collect::<Vec<_>>();
    let mut environment = kde_target_environment(repo_root, &build, components, qt_integration)?;
    if component == "breeze-icons" {
        environment.push(("PYTHONPATH", python_deps.display().to_string()));
    }
    run_cmd_with_env_overrides(&build, "cmake", &refs, &environment)?;
    run_cmd_with_env_overrides(&build, "cmake", &["--build", "."], &environment)?;
    qt_install(repo_root, &build, &install)?;
    if component == "plasma-workspace" {
        // These generated runtime helpers are installed by the workspace
        // build, but their command lines are evaluated against the
        // disposable Qt/KDE tool view.  Normalize the two executable paths
        // at the output boundary so the package never retains a developer
        // checkout path and the installed helper resolves through /usr.
        let generated_paths = [
            (
                "usr/share/kconf_update/migrate-calendar-to-plugin-id.py",
                repo_root
                    .join("out/build/plasma-workspace/build/mattos-qt-cmake-view/bin/qtpaths")
                    .display()
                    .to_string(),
                "/usr/bin/qtpaths",
            ),
            (
                "usr/lib/systemd/user/plasma-kcminit-phase1.service",
                repo_root
                    .join("out/build/qtbase/install/usr/bin/qdbus")
                    .display()
                    .to_string(),
                "/usr/bin/qdbus",
            ),
        ];
        for (relative, build_path, installed_path) in generated_paths {
            let path = install.join(relative);
            if !path.is_file() {
                continue;
            }
            let original = fs::read_to_string(&path)?;
            let normalized = original.replace(&build_path, installed_path);
            if normalized.contains(&repo_root.to_string_lossy().to_string()) {
                bail!("{component} generated metadata retains the host build root in {}", path.display());
            }
            if normalized != original {
                fs::write(path, normalized)?;
            }
        }
    }
    // Run the generic output-boundary audit after the component-specific
    // generated helpers above.  Those helpers are produced from disposable
    // tool-view paths that are not install-prefix metadata and therefore must
    // be normalized before the fail-closed scan rejects the checkout root.
    normalize_kde_installed_metadata(repo_root, component, &install)?;
    if component == "polkit-qt-1" {
        // Export the target-owned pkg-config dependencies from the generated
        // package config.  This preserves upstream's shared-library layout,
        // keeps strict consumers' --no-undefined links valid, and avoids
        // embedding build-tree paths in installed CMake metadata.
        let targets = install.join("usr/lib/x86_64-linux-gnu/cmake/PolkitQt6-1/PolkitQt6-1Targets.cmake");
        let contents = fs::read_to_string(&targets)?;
        let adjusted = contents.replace(
            "INTERFACE_LINK_LIBRARIES \"Qt6::Core;${_IMPORT_PREFIX}/lib/x86_64-linux-gnu/libpolkit-gobject-1.so.0;${_IMPORT_PREFIX}/lib/x86_64-linux-gnu/libgio-2.0.so;${_IMPORT_PREFIX}/lib/x86_64-linux-gnu/libgobject-2.0.so;${_IMPORT_PREFIX}/lib/x86_64-linux-gnu/libglib-2.0.so\"",
            "INTERFACE_LINK_LIBRARIES \"Qt6::Core;PkgConfig::POLKIT_GOBJECT;PkgConfig::GLIB2;PkgConfig::GOBJECT\"",
        );
        if adjusted != contents { fs::write(targets, adjusted)?; }
        let config = install.join("usr/lib/x86_64-linux-gnu/cmake/PolkitQt6-1/PolkitQt6-1Config.cmake");
        let contents = fs::read_to_string(&config)?;
        let adjusted = contents.replace(
            "include(\"${CMAKE_CURRENT_LIST_DIR}/PolkitQt6-1Targets.cmake\")",
            "find_dependency(PkgConfig)\npkg_check_modules(POLKIT_GOBJECT REQUIRED IMPORTED_TARGET polkit-gobject-1)\npkg_check_modules(GLIB2 REQUIRED IMPORTED_TARGET glib-2.0)\npkg_check_modules(GOBJECT REQUIRED IMPORTED_TARGET gobject-2.0)\ninclude(\"${CMAKE_CURRENT_LIST_DIR}/PolkitQt6-1Targets.cmake\")",
        );
        if adjusted != contents { fs::write(config, adjusted)?; }
    }
    let output = install.join(required_output);
    if !output.is_file() {
        bail!("{component} did not publish required target output {}", output.display());
    }
    Ok(())
}

/// Remove disposable build-prefix references from generated KDE metadata at
/// the stage output boundary.  KDE's CMake/ECM helpers intentionally emit
/// absolute paths for imported target includes, pkg-config dependencies and
/// small runtime helper scripts.  The absolute paths are useful inside the
/// build mirror but would make a package depend on this checkout after it is
/// installed.  Only known MattOS component install prefixes are rewritten to
/// their target `/usr` location; any remaining checkout path fails closed.
fn normalize_kde_installed_metadata(
    repo_root: &Path,
    component: &str,
    install: &Path,
) -> Result<()> {
    let build_root = repo_root.join("out/build");
    let build_root_text = build_root.to_string_lossy().into_owned();
    let mut prefixes = Vec::<(String, String)>::new();
    if let Ok(entries) = fs::read_dir(&build_root) {
        for entry in entries {
            let path = entry?.path();
            let name = match path.file_name().and_then(OsStr::to_str) {
                Some(name) => name,
                None => continue,
            };
            let target_prefix = path.join("install/usr");
            if target_prefix.is_dir() {
                prefixes.push((target_prefix.to_string_lossy().into_owned(), "/usr".to_string()));
            }
            let source_prefix = path.join("source");
            if source_prefix.is_dir() {
                prefixes.push((
                    source_prefix.to_string_lossy().into_owned(),
                    format!("/usr/src/mattos/{name}"),
                ));
            }
        }
    }
    let normalize = |path: &Path| -> Result<()> {
        let original = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let normalized = prefixes.iter().fold(original.clone(), |contents, (from, to)| {
            contents.replace(from, to)
        });
        if normalized.contains(&build_root_text) {
            bail!(
                "{component} installed metadata {} retains the host build root",
                path.display()
            );
        }
        if normalized != original {
            fs::write(path, normalized)?;
        }
        Ok(())
    };
    fn walk(directory: &Path, normalize: &dyn Fn(&Path) -> Result<()>) -> Result<()> {
        if !directory.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(&path, normalize)?;
                continue;
            }
            if matches!(
                path.extension().and_then(OsStr::to_str),
                Some("cmake" | "pc" | "prl" | "la" | "json" | "py" | "service" | "desktop" | "spdx" | "xml")
            ) {
                normalize(&path)?;
            }
        }
        Ok(())
    }
    walk(&install.join("usr"), &normalize)
}

fn build_kcoreaddons(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kcoreaddons", "src/desktop/kde/kcoreaddons", &["systemd", "util-linux", "qtdeclarative"],
        &["-DKCOREADDONS_USE_QML=ON", "-DBUILD_PYTHON_BINDINGS=OFF"], "usr/lib/x86_64-linux-gnu/libKF6CoreAddons.so")
}

fn build_ki18n(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "ki18n", "src/desktop/kde/ki18n", &["iso-codes", "qtdeclarative"],
        &["-DBUILD_WITH_QML=ON"], "usr/lib/x86_64-linux-gnu/libKF6I18n.so")
}

fn build_kwidgetsaddons(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kwidgetsaddons", "src/desktop/kde/kwidgetsaddons", &[],
        &["-DBUILD_DESIGNERPLUGIN=OFF", "-DBUILD_PYTHON_BINDINGS=OFF"], "usr/lib/x86_64-linux-gnu/libKF6WidgetsAddons.so")
}

fn build_kconfig(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kconfig", "src/desktop/kde/kconfig", &["qtdeclarative"], &["-DKCONFIG_USE_QML=ON"], "usr/lib/x86_64-linux-gnu/libKF6ConfigCore.so")
}

fn build_kconfigwidgets(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kconfigwidgets", "src/desktop/kde/kconfigwidgets", &["qtbase", "qtdeclarative", "kcoreaddons", "kcodecs", "kconfig", "kguiaddons", "ki18n", "kwidgetsaddons", "kcolorscheme", "xkbcommon"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6ConfigWidgets.so")
}

fn build_kdbusaddons(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kdbusaddons", "src/desktop/kde/kdbusaddons", &["xkbcommon"], &["-DWITH_X11=OFF"], "usr/lib/x86_64-linux-gnu/libKF6DBusAddons.so")
}

fn build_kauth(repo_root: &Path) -> Result<()> {
    // KCoreAddons publicly links udev and libmount.  KAuth's example client
    // links KCoreAddons directly, so those target-owned transitive libraries
    // must be present in the link environment as well; relying on the host
    // linker made this fail only when the examples were enabled.
    build_kde_cmake(repo_root, "kauth", "src/desktop/kde/kauth", &["kcoreaddons", "systemd", "util-linux"], &[], "usr/lib/x86_64-linux-gnu/libKF6AuthCore.so")
}

fn build_karchive(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "karchive", "src/desktop/kde/karchive", &["zlib", "zstd", "bzip2", "xz", "openssl"], &[], "usr/lib/x86_64-linux-gnu/libKF6Archive.so")
}

fn build_kdecoration(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kdecoration", "src/desktop/kde/kdecoration", &["ki18n"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libkdecorations3.so")
}

fn build_libcanberra(repo_root: &Path) -> Result<()> {
    build_autotools_import(repo_root, "libcanberra", "src/system/libraries/libcanberra", &["glib"], &["--prefix=/usr", "--libdir=/usr/lib/x86_64-linux-gnu", "--disable-gtk", "--disable-gtk3", "--disable-udev", "--disable-gstreamer", "--with-builtin=oss"], &["usr/lib/x86_64-linux-gnu/libcanberra.so"])
}

fn build_libqrencode(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "qrencode",
        "src/system/libraries/qrencode",
        &["zlib"],
        &["--prefix=/usr", "--libdir=/usr/lib/x86_64-linux-gnu", "--disable-static"],
        &["usr/lib/x86_64-linux-gnu/libqrencode.so"],
    )
}

fn build_kwayland(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "kwayland",
        "src/desktop/kde/kwayland",
        &["qtbase", "qtwayland", "qtdeclarative", "wayland", "wayland-protocols", "plasma-wayland-protocols", "xkbcommon"],
        &["-DBUILD_TESTING=OFF", "-DBUILD_QCH=OFF"],
        "usr/lib/x86_64-linux-gnu/libKWaylandClient.so.6",
    )
}

fn build_knighttime(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "knighttime",
        "src/desktop/kde/knighttime",
        &["qtbase", "qtdeclarative", "qtpositioning", "kconfig", "kcoreaddons", "kdbusaddons", "ki18n", "kholidays", "systemd", "util-linux"],
        &["-DBUILD_TESTING=OFF", "-DBUILD_QCH=OFF"],
        "usr/lib/x86_64-linux-gnu/libKNightTime.so.0",
    )
}

fn build_kholidays(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "kholidays",
        "src/desktop/kde/kholidays",
        &["qtbase", "qtdeclarative", "kcoreaddons", "ki18n"],
        &["-DBUILD_TESTING=OFF", "-DBUILD_QCH=OFF", "-DFLEX_EXECUTABLE=/usr/bin/flex", "-DBISON_EXECUTABLE=/usr/bin/bison"],
        "usr/lib/x86_64-linux-gnu/libKF6Holidays.so.6",
    )
}

fn build_lcms2(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "lcms2",
        "src/system/graphics/lcms2",
        &["zlib"],
        &["--prefix=/usr", "--libdir=/usr/lib/x86_64-linux-gnu", "--disable-static", "--without-jpeg", "--without-tiff", "--without-zstd"],
        &["usr/lib/x86_64-linux-gnu/liblcms2.so.2"],
    )
}

fn build_kirigami_platform(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kirigami", "src/desktop/kde/kirigami", &["qtdeclarative", "qtshadertools", "kcoreaddons", "ki18n", "xkbcommon"], &[], "usr/lib/x86_64-linux-gnu/libKirigami.so")
}

fn build_kirigami_addons(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "kirigami-addons",
        "src/desktop/kde/kirigami-addons",
        &["qtdeclarative", "kconfig", "kcoreaddons", "kguiaddons", "ki18n", "kglobalaccel", "kirigami"],
        &["-DBUILD_TESTING=OFF", "-DBUILD_EXAMPLES=OFF"],
        "usr/lib/x86_64-linux-gnu/cmake/KF6KirigamiAddons/KF6KirigamiAddonsConfig.cmake",
    )
}

fn build_kquickcharts(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "kquickcharts",
        "src/desktop/kde/kquickcharts",
        &["qtdeclarative", "qtshadertools"],
        &["-DBUILD_TESTING=OFF", "-DBUILD_EXAMPLES=OFF"],
        "usr/lib/x86_64-linux-gnu/cmake/KF6QuickCharts/KF6QuickChartsConfig.cmake",
    )
}

fn build_kcolorscheme(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kcolorscheme", "src/desktop/kde/kcolorscheme", &["qtbase", "qtdeclarative", "kconfig", "kguiaddons", "ki18n", "xkbcommon"], &[], "usr/lib/x86_64-linux-gnu/libKF6ColorScheme.so")
}

fn build_kcrash(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kcrash", "src/desktop/kde/kcrash", &["kcoreaddons", "kdbusaddons"], &["-DWITH_X11=OFF"], "usr/lib/x86_64-linux-gnu/libKF6Crash.so")
}

fn build_kglobalaccel(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kglobalaccel", "src/desktop/kde/kglobalaccel", &["kconfig", "kcoreaddons", "kdbusaddons", "kwidgetsaddons", "xkbcommon"], &["-DWITH_X11=OFF"], "usr/lib/x86_64-linux-gnu/libKF6GlobalAccel.so")
}

fn build_kguiaddons(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kguiaddons", "src/desktop/kde/kguiaddons", &["xkbcommon", "wayland", "plasma-wayland-protocols"], &["-DWITH_X11=OFF", "-DBUILD_PYTHON_BINDINGS=OFF", "-DCMAKE_DISABLE_FIND_PACKAGE_Qt6Qml=TRUE"], "usr/lib/x86_64-linux-gnu/libKF6GuiAddons.so")
}

fn build_kidletime(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kidletime", "src/desktop/kde/kidletime", &["kcoreaddons"], &["-DWITH_X11=OFF", "-DWITH_WAYLAND=OFF"], "usr/lib/x86_64-linux-gnu/libKF6IdleTime.so")
}

fn build_kpackage(repo_root: &Path) -> Result<()> {
    // KPackage links KArchive and KCoreAddons. Their target-owned shared
    // dependencies must be exposed explicitly because the linker does not
    // reliably traverse shared-library dependencies; this prevents host
    // libraries from satisfying the link accidentally.
    build_kde_cmake(repo_root, "kpackage", "src/desktop/kde/kpackage", &["qtdeclarative", "kcoreaddons", "ki18n", "karchive", "systemd", "util-linux", "zlib", "zstd", "bzip2", "xz", "openssl"], &["-DKPACKAGE_BUILD_QML=ON"], "usr/lib/x86_64-linux-gnu/libKF6Package.so")
}

fn build_kservice(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kservice", "src/desktop/kde/kservice", &["qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "systemd", "util-linux"], &[], "usr/lib/x86_64-linux-gnu/libKF6Service.so")
}

fn build_qcoro(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "qcoro",
        "src/desktop/kde/qcoro",
        &["qtbase", "qtdeclarative"],
        &[
            "-DUSE_QT_VERSION=6",
            "-DBUILD_SHARED_LIBS=ON",
            "-DQCORO_BUILD_EXAMPLES=OFF",
            "-DQCORO_BUILD_TESTING=OFF",
            // Plasma NetworkManagement consumes QCoro's DBus coroutine
            // wrappers.  Keep this target-owned instead of allowing a host
            // QCoro component to satisfy the downstream CMake lookup.
            "-DQCORO_WITH_QTDBUS=ON",
            "-DQCORO_WITH_QTNETWORK=OFF",
            "-DQCORO_WITH_QTWEBSOCKETS=OFF",
            "-DQCORO_WITH_QTQUICK=OFF",
            "-DQCORO_WITH_QML=OFF",
            "-DQCORO_WITH_QTTEST=OFF",
        ],
        "usr/lib/x86_64-linux-gnu/libQCoro6Core.so",
    )
}

fn build_kdeclarative(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kdeclarative", "src/desktop/kde/kdeclarative", &["qtbase", "qtdeclarative", "qtshadertools", "qttools", "kconfig", "kcoreaddons", "kguiaddons", "ki18n", "kglobalaccel", "kwidgetsaddons", "kirigami", "ksvg"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/cmake/KF6Declarative/KF6DeclarativeConfig.cmake")
}

fn build_kiconthemes(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kiconthemes", "src/desktop/kde/kiconthemes", &["qtbase", "qtdeclarative", "qttools", "karchive", "kconfig", "kcoreaddons", "kguiaddons", "ki18n", "kwidgetsaddons", "kcolorscheme", "breeze-icons", "libffi", "systemd", "util-linux", "zlib", "zstd", "bzip2", "xz", "openssl"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6IconThemes.so")
}

fn build_breeze_icons(repo_root: &Path) -> Result<()> {
    // The icon generator is a Qt executable built and run during the build;
    // include QtBase in the target environment so its runtime loader cannot
    // silently select the host Qt6 libraries.
    build_kde_cmake(repo_root, "breeze-icons", "src/desktop/kde/breeze-icons", &["qtbase"], &["-DBUILD_TESTING=OFF"], "usr/share/icons/breeze/index.theme")
}

fn build_kitemmodels(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kitemmodels", "src/desktop/kde/kitemmodels", &["qtbase", "qtdeclarative"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6ItemModels.so")
}

fn build_kitemviews(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kitemviews", "src/desktop/kde/kitemviews", &["qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "kwidgetsaddons"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6ItemViews.so")
}

fn build_kjobwidgets(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kjobwidgets", "src/desktop/kde/kjobwidgets", &["qtbase", "kcoreaddons", "ki18n", "knotifications", "kwidgetsaddons", "xkbcommon"], &["-DBUILD_TESTING=OFF", "-DBUILD_PYTHON_BINDINGS=OFF", "-DWITH_X11=OFF"], "usr/lib/x86_64-linux-gnu/libKF6JobWidgets.so")
}

fn build_kcmutils(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kcmutils", "src/desktop/kde/kcmutils", &["qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "kwidgetsaddons", "kitemmodels", "kpackage", "kio", "kwindowsystem", "karchive", "kauth", "kbookmarks", "kcolorscheme", "kcompletion", "kcrash", "kdbusaddons", "kguiaddons", "kiconthemes", "breeze-icons", "kitemviews", "kjobwidgets", "kservice", "solid", "util-linux", "kconfigwidgets", "kcodecs", "kxmlgui", "kirigami", "kglobalaccel", "libffi"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6KCMUtils.so")
}

fn build_kded(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kded", "src/desktop/kde/kded", &["qtbase", "qtdeclarative", "kcoreaddons", "kdbusaddons", "kconfig", "kcrash", "ki18n", "kservice", "systemd", "util-linux"], &["-DBUILD_TESTING=OFF"], "usr/bin/kded6")
}

fn build_kio(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kio", "src/desktop/kde/kio", &["qtbase", "qtdeclarative", "karchive", "kauth", "kbookmarks", "kcolorscheme", "kcompletion", "kconfig", "kcoreaddons", "kcrash", "kdbusaddons", "kguiaddons", "ki18n", "kiconthemes", "kitemmodels", "kitemviews", "kjobwidgets", "kservice", "kwidgetsaddons", "kwindowsystem", "solid", "util-linux", "systemd", "zlib", "zstd", "bzip2", "xz", "openssl"], &["-DBUILD_TESTING=OFF", "-DKIO_FORK_SLAVES=OFF", "-DWITH_X11=OFF"], "usr/lib/x86_64-linux-gnu/libKF6KIOCore.so")
}

fn build_kunitconversion(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "kunitconversion",
        "src/desktop/kde/kunitconversion",
        &["qtbase", "kconfig", "kcoreaddons", "ki18n"],
        &["-DBUILD_TESTING=OFF", "-DBUILD_PYTHON_BINDINGS=OFF"],
        "usr/lib/x86_64-linux-gnu/libKF6UnitConversion.so",
    )
}

fn build_kbookmarks(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kbookmarks", "src/desktop/kde/kbookmarks", &["qtbase", "qtdeclarative", "kcoreaddons", "kconfig", "ki18n", "kwidgetsaddons"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6Bookmarks.so")
}

fn build_kcompletion(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kcompletion", "src/desktop/kde/kcompletion", &["qtbase", "qtdeclarative", "kcodecs", "kcoreaddons", "kconfig", "ki18n", "kwidgetsaddons"], &["-DBUILD_TESTING=OFF", "-DBUILD_DESIGNERPLUGIN=OFF"], "usr/lib/x86_64-linux-gnu/libKF6Completion.so")
}

fn build_kcodecs(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kcodecs", "src/desktop/kde/kcodecs", &["qtbase", "kcoreaddons", "ki18n"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6Codecs.so")
}

fn build_ksolid(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "solid", "src/desktop/kde/solid", &["qtbase", "qtdeclarative", "kcoreaddons", "kconfig", "ki18n", "kdbusaddons", "systemd", "util-linux", "selinux"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6Solid.so")
}

fn build_kdoctools(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kdoctools", "src/desktop/kde/kdoctools", &["qtbase", "qtdeclarative", "kcoreaddons", "ki18n", "karchive"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/cmake/KF6DocTools/KF6DocToolsConfig.cmake")
}

fn build_knewstuff(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "knewstuff", "src/desktop/kde/knewstuff", &["qtbase", "qtdeclarative", "karchive", "kauth", "kconfig", "kcoreaddons", "ki18n", "kio", "kpackage", "kservice", "kwidgetsaddons", "attica"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6NewStuffCore.so")
}

fn build_kattica(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "attica", "src/desktop/kde/attica", &["qtbase"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6Attica.so")
}

fn build_knotifications(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "knotifications", "src/desktop/kde/knotifications", &["qtbase", "qtdeclarative", "kcoreaddons", "kconfig", "kdbusaddons", "ki18n", "libcanberra"], &["-DBUILD_TESTING=OFF", "-DBUILD_PYTHON_BINDINGS=OFF"], "usr/lib/x86_64-linux-gnu/libKF6Notifications.so")
}

fn build_knotifyconfig(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "knotifyconfig", "src/desktop/kde/knotifyconfig", &["qtbase", "qtdeclarative", "karchive", "kauth", "kbookmarks", "kcolorscheme", "kcompletion", "kconfig", "kcoreaddons", "kcrash", "kdbusaddons", "kguiaddons", "ki18n", "kiconthemes", "kitemmodels", "kitemviews", "kjobwidgets", "kio", "kservice", "solid", "kwidgetsaddons", "kwindowsystem", "libcanberra", "libffi", "util-linux"], &["-DBUILD_TESTING=OFF", "-DBUILD_QCH=OFF"], "usr/lib/x86_64-linux-gnu/libKF6NotifyConfig.so")
}

fn build_kparts(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kparts", "src/desktop/kde/kparts", &["qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "kservice", "kwidgetsaddons", "kio", "kwindowsystem", "kbookmarks", "kcompletion", "kitemviews", "karchive", "kauth", "kcolorscheme", "kcrash", "kdbusaddons", "kguiaddons", "kiconthemes", "kitemmodels", "kjobwidgets", "solid", "util-linux", "kconfigwidgets", "kxmlgui", "kcodecs"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6Parts.so")
}

fn build_kxmlgui(repo_root: &Path) -> Result<()> {
    let print_support = format!("-DQt6PrintSupport_DIR={}/out/build/qtbase/install/usr/lib/x86_64-linux-gnu/cmake/Qt6PrintSupport", repo_root.display());
    let core_private = format!("-DQt6CorePrivate_DIR={}/out/build/qtbase/install/usr/lib/x86_64-linux-gnu/cmake/Qt6CorePrivate", repo_root.display());
    build_kde_cmake(repo_root, "kxmlgui", "src/desktop/kde/kxmlgui", &["qtbase", "qtdeclarative", "kcoreaddons", "kitemviews", "kconfig", "kconfigwidgets", "kcodecs", "kguiaddons", "ki18n", "kiconthemes", "kwidgetsaddons", "kglobalaccel", "kcolorscheme"], &["-DBUILD_TESTING=OFF", "-DKF6_XMLGUI_BUILD_DEPRECATED_SINCE=0", "-DBUILD_PYTHON_BINDINGS=OFF", &print_support, &core_private], "usr/lib/x86_64-linux-gnu/libKF6XmlGui.so")
}

fn build_prison(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "prison", "src/desktop/kde/prison", &["qtbase", "qtdeclarative", "qtmultimedia", "kcoreaddons", "ki18n", "kwidgetsaddons", "qrencode", "zxing-cpp"], &["-DBUILD_TESTING=OFF", "-DWITH_DMTX=OFF", "-DWITH_ZXING=ON", "-DWITH_MULTIMEDIA=ON"], "usr/lib/x86_64-linux-gnu/libKF6Prison.so")
}

fn build_krunner(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "krunner", "src/desktop/kde/krunner", &["qtbase", "qtdeclarative", "kcoreaddons", "kconfig", "ki18n", "kservice", "kwindowsystem", "kitemmodels"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6Runner.so")
}

fn build_kstatusnotifieritem(repo_root: &Path) -> Result<()> {
    // KWindowSystem is intentionally built with its X11 compatibility ABI so
    // KWin can serve Xwayland clients.  Its generated package config therefore
    // retains a target-owned FindX11 dependency even for Wayland consumers.
    // Include the aggregate X11 output so that this probe cannot fall through
    // to host headers or libraries.
    build_kde_cmake(repo_root, "kstatusnotifieritem", "src/desktop/kde/kstatusnotifieritem", &["qtbase", "kcoreaddons", "kconfig", "kdbusaddons", "ki18n", "kwidgetsaddons", "kwindowsystem", "x11-compat"], &["-DBUILD_TESTING=OFF", "-DBUILD_PYTHON_BINDINGS=OFF", "-DWITH_X11=OFF"], "usr/lib/x86_64-linux-gnu/libKF6StatusNotifierItem.so")
}

fn build_ktexteditor(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "ktexteditor", "src/desktop/kde/ktexteditor", &["qtbase", "qtdeclarative", "qtmultimedia", "qtspeech", "karchive", "kauth", "kcoreaddons", "kconfig", "kconfigwidgets", "kcodecs", "kglobalaccel", "kguiaddons", "ki18n", "kio", "kwindowsystem", "kbookmarks", "kcompletion", "kcrash", "kdbusaddons", "kiconthemes", "kitemviews", "kjobwidgets", "knotifications", "breeze-icons", "solid", "kparts", "kxmlgui", "sonnet", "ksyntaxhighlighting", "kcolorscheme", "kservice", "ktextwidgets", "kwidgetsaddons", "libcanberra", "libffi"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6TextEditor.so")
}

fn build_ksyntaxhighlighting(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "ksyntaxhighlighting",
        "src/desktop/kde/ksyntaxhighlighting",
        &["qtbase"],
        &["-DBUILD_TESTING=OFF", "-DKSYNTAXHIGHLIGHTING_USE_GUI=ON"],
        "usr/lib/x86_64-linux-gnu/libKF6SyntaxHighlighting.so",
    )
}

fn build_ktextwidgets(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "ktextwidgets", "src/desktop/kde/ktextwidgets", &["qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "kservice", "kwidgetsaddons", "kcompletion", "sonnet"], &["-DBUILD_TESTING=OFF", "-DWITH_TEXT_TO_SPEECH=OFF"], "usr/lib/x86_64-linux-gnu/libKF6TextWidgets.so")
}

fn build_ksonnet(repo_root: &Path) -> Result<()> {
    // Spell-checker backends are optional KF integrations.  Keep the
    // foundation deterministic and source-owned without importing a second
    // dictionary stack; downstreams can add a backend when they need it.
    build_kde_cmake(repo_root, "sonnet", "src/desktop/kde/sonnet", &["qtbase", "qtdeclarative", "qttools"], &["-DBUILD_TESTING=OFF", "-DSONNET_NO_BACKENDS=ON"], "usr/lib/x86_64-linux-gnu/libKF6SonnetUi.so")
}

fn build_kwallet(repo_root: &Path) -> Result<()> {
    // MattOS ships the KWallet API for Plasma consumers, but deliberately
    // does not enable the upstream Secret-Service daemons here.  Those
    // daemons add a separate libsecret/Gpgmepp service-provider closure and
    // are not required by the Wayland shell; a provider can be added later
    // as an explicit runtime component rather than leaking host packages.
    build_kde_cmake(
        repo_root,
        "kwallet",
        "src/desktop/kde/kwallet",
        &["qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "kdbusaddons", "ki18n", "kwidgetsaddons", "kwindowsystem", "knotifications", "kcolorscheme", "kcrash", "libgcrypt"],
        &["-DBUILD_TESTING=OFF", "-DBUILD_KSECRETD=OFF", "-DBUILD_KWALLETD=OFF", "-DBUILD_KWALLET_QUERY=OFF"],
        "usr/lib/x86_64-linux-gnu/libKF6Wallet.so",
    )
}

fn build_qca(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "qca",
        "src/system/security/qca",
        &["qtbase", "qt5compat"],
        &[
            "-DBUILD_WITH_QT6=ON",
            "-DBUILD_TESTS=OFF",
            "-DBUILD_TOOLS=OFF",
            "-DBUILD_PLUGINS=none",
            "-DLIB_INSTALL_DIR=/usr/lib/x86_64-linux-gnu",
            "-DQCA_LIBRARY_INSTALL_DIR=/usr/lib/x86_64-linux-gnu",
            "-DQCA_PLUGINS_INSTALL_DIR=/usr/lib/x86_64-linux-gnu/qca-qt6",
            "-DQCA_BINARY_INSTALL_DIR=/usr/bin",
            "-DQCA_INCLUDE_INSTALL_DIR=/usr/include/Qca-qt6",
            "-DQCA_PRIVATE_INCLUDE_INSTALL_DIR=/usr/include/Qca-qt6",
            "-DPKGCONFIG_INSTALL_PREFIX=/usr/lib/x86_64-linux-gnu/pkgconfig",
        ],
        "usr/lib/x86_64-linux-gnu/libqca-qt6.so",
    )?;

    // QCA's upstream export is generated with the literal installation
    // prefix (/usr).  That is correct after packaging, but it makes a
    // separate source-owned build consumer probe the host filesystem instead
    // of the disposable target install.  Keep the upstream source untouched
    // and make the generated export relocatable relative to its own cmake
    // directory, which is also the form that survives Debian staging.
    let cmake_dir = repo_root.join("out/build/qca/install/usr/lib/x86_64-linux-gnu/cmake/Qca-qt6");
    let targets = cmake_dir.join("Qca-qt6Targets.cmake");
    let contents = fs::read_to_string(&targets)?;
    let adjusted = contents
        .replace(
            "set(_IMPORT_PREFIX \"/usr\")",
            "get_filename_component(_IMPORT_PREFIX \"${CMAKE_CURRENT_LIST_DIR}/../../../../\" ABSOLUTE)",
        )
        .replace(
            "INTERFACE_INCLUDE_DIRECTORIES \"/usr/include/Qca-qt6/QtCrypto\"",
            "INTERFACE_INCLUDE_DIRECTORIES \"${_IMPORT_PREFIX}/include/Qca-qt6/QtCrypto\"",
        );
    if adjusted != contents {
        fs::write(&targets, adjusted)?;
    }
    let noconfig = cmake_dir.join("Qca-qt6Targets-noconfig.cmake");
    let contents = fs::read_to_string(&noconfig)?;
    let adjusted = contents
        .replace(
            "IMPORTED_LOCATION_NOCONFIG \"/usr/lib/x86_64-linux-gnu/libqca-qt6.so.2.3.12\"",
            "IMPORTED_LOCATION_NOCONFIG \"${_IMPORT_PREFIX}/lib/x86_64-linux-gnu/libqca-qt6.so.2.3.12\"",
        )
        .replace(
            "_cmake_import_check_files_for_qca-qt6 \"/usr/lib/x86_64-linux-gnu/libqca-qt6.so.2.3.12\"",
            "_cmake_import_check_files_for_qca-qt6 \"${_IMPORT_PREFIX}/lib/x86_64-linux-gnu/libqca-qt6.so.2.3.12\"",
        );
    if adjusted != contents {
        fs::write(noconfig, adjusted)?;
    }
    Ok(())
}

fn build_ksvg(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "ksvg", "src/desktop/kde/ksvg", &["qtdeclarative", "karchive", "kconfig", "kcolorscheme", "kcoreaddons", "kguiaddons", "kirigami"], &[], "usr/lib/x86_64-linux-gnu/libKF6Svg.so")
}

fn build_kwindowsystem(repo_root: &Path) -> Result<()> {
    // KWin's Wayland session still builds the X11 compatibility path for
    // --xwayland clients. KWindowSystem must therefore publish its generated
    // NETWM/KX11Extras headers and XCB backend, rather than the Wayland-only
    // ABI that was sufficient while KWin itself was built with X11 disabled.
    build_kde_cmake(repo_root, "kwindowsystem", "src/desktop/kde/kwindowsystem", &["qtdeclarative", "qtwayland", "kcoreaddons", "kwidgetsaddons", "xkbcommon", "wayland", "wayland-protocols", "plasma-wayland-protocols", "x11-compat", "libxcb"], &["-DKWINDOWSYSTEM_X11=ON"], "usr/lib/x86_64-linux-gnu/libKF6WindowSystem.so")
}

fn build_plasma_wayland_protocols(repo_root: &Path) -> Result<()> {
    build_non_qt_cmake(repo_root, "plasma-wayland-protocols", "src/desktop/kde/plasma-wayland-protocols", &[], &["-DBUILD_TESTING=OFF"], "usr/share/cmake/PlasmaWaylandProtocols/PlasmaWaylandProtocolsConfig.cmake")
}

fn build_wayland_protocols(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/graphics/wayland-protocols");
    let root = repo_root.join("out/build/wayland-protocols");
    let build = root.join("build");
    let install = root.join("install");
    fs::create_dir_all(&root)?;
    sync_build_source(&source, &root.join("source"))?;
    remove_path_if_exists(&build)?;
    remove_path_if_exists(&install)?;
    fs::create_dir_all(&install)?;
    let environment = staged_library_environment(repo_root, &["wayland", "expat", "libffi"])?;
    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &[
            "setup",
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            path_str(&build)?,
            path_str(&root.join("source"))?,
        ],
        &environment,
    )?;
    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            path_str(&build)?,
            "--destdir",
            path_str(&install)?,
        ],
        &environment,
    )?;
    let required = install.join("usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml");
    if !required.is_file() {
        bail!("wayland-protocols build did not install {}", required.display());
    }
    Ok(())
}

fn build_polkit_qt6(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "polkit-qt-1", "src/system/security/polkit-qt-1", &["polkit", "glib", "dbus", "zlib"],
        &["-DQT_MAJOR_VERSION=6", "-DUSE_COMMON_CMAKE_PACKAGE_CONFIG_DIR=TRUE"],
        "usr/lib/x86_64-linux-gnu/libpolkit-qt6-core-1.so")
}

fn build_yaml_cpp(repo_root: &Path) -> Result<()> {
    build_non_qt_cmake(repo_root, "yaml-cpp", "src/system/libraries/yaml-cpp", &[],
        &["-DCMAKE_POLICY_VERSION_MINIMUM=3.5", "-DYAML_BUILD_SHARED_LIBS=ON", "-DYAML_CPP_BUILD_TESTS=OFF", "-DYAML_CPP_BUILD_TOOLS=OFF", "-DYAML_CPP_BUILD_CONTRIB=OFF", "-DYAML_CPP_FORMAT_SOURCE=OFF"],
        "usr/lib/x86_64-linux-gnu/libyaml-cpp.so")
}

fn build_kpmcore(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "kpmcore", "src/system/storage/kpmcore",
        &["kcoreaddons", "ki18n", "kwidgetsaddons", "polkit-qt-1", "polkit", "glib", "dbus", "systemd", "pcre2", "zlib", "libffi", "util-linux", "btrfs-progs"],
        &["-DPARTMAN_SFDISKBACKEND=ON", "-DPARTMAN_DUMMYBACKEND=OFF"],
        "usr/lib/x86_64-linux-gnu/libkpmcore.so")
}

#[cfg(test)]
mod kde_foundation_tests {
    use super::*;

    #[test]
    fn kpmcore_btrfs_and_sfdisk_contract_remains_source_owned() {
        let source = include_str!("../../../../system/storage/kpmcore/src/fs/btrfs.cpp");
        assert!(source.contains("mkfs.btrfs"));
        assert!(source.contains("btrfstune"));
        let plugin = include_str!("../../../../system/storage/kpmcore/src/plugins/CMakeLists.txt");
        assert!(plugin.contains("PARTMAN_SFDISKBACKEND"));
    }

    #[test]
    fn missing_or_old_ecm_fails_closed() {
        assert!(KDE_ECM_MIN_VERSION >= (6, 5, 0));
    }
}
