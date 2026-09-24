/// Qt is configured with trusted host executables and a target-only package
/// search domain. The generated bridge deliberately names every target ABI
/// file it exposes; it is not a copy of the host's OpenGL CMake metadata.
fn qt_opengl_bridge(repo_root: &Path) -> Result<PathBuf> {
    let glvnd = repo_root.join("out/build/libglvnd/install/usr");
    let mesa = repo_root.join("out/build/mesa/install/usr");
    let include = glvnd.join("include");
    let gl = glvnd.join("lib/x86_64-linux-gnu/libGL.so");
    let open_gl = glvnd.join("lib/x86_64-linux-gnu/libOpenGL.so");
    let egl = glvnd.join("lib/x86_64-linux-gnu/libEGL.so");
    let gles2 = glvnd.join("lib/x86_64-linux-gnu/libGLESv2.so");
    let gbm = mesa.join("lib/x86_64-linux-gnu/libgbm.so");
    for path in [&include, &include.join("GL/gl.h"), &gl, &open_gl, &egl, &gles2, &gbm] {
        if !path.exists() {
            bail!("Qt OpenGL bridge requires verified MattOS output {}", path.display());
        }
        if !path.starts_with(repo_root.join("out/build")) {
            bail!("Qt OpenGL bridge path escapes MattOS build outputs: {}", path.display());
        }
    }
    let root = repo_root.join("out/build/.qt-opengl-bridge");
    fs::create_dir_all(&root)?;
    fs::write(root.join("OpenGLConfig.cmake"), format!(
        concat!(
            "add_library(OpenGL::OpenGL SHARED IMPORTED)\n",
            "set_target_properties(OpenGL::OpenGL PROPERTIES IMPORTED_LOCATION \"{}\" INTERFACE_INCLUDE_DIRECTORIES \"{}\")\n",
            "add_library(OpenGL::GL SHARED IMPORTED)\n",
            "set_target_properties(OpenGL::GL PROPERTIES IMPORTED_LOCATION \"{}\" INTERFACE_LINK_LIBRARIES OpenGL::OpenGL INTERFACE_INCLUDE_DIRECTORIES \"{}\")\n",
            "add_library(OpenGL::EGL SHARED IMPORTED)\n",
            "set_target_properties(OpenGL::EGL PROPERTIES IMPORTED_LOCATION \"{}\" INTERFACE_INCLUDE_DIRECTORIES \"{}\")\n",
            "add_library(OpenGL::GLES2 SHARED IMPORTED)\n",
            "set_target_properties(OpenGL::GLES2 PROPERTIES IMPORTED_LOCATION \"{}\" INTERFACE_INCLUDE_DIRECTORIES \"{}\")\n",
            "add_library(MattOS::GBM SHARED IMPORTED)\n",
            "set_target_properties(MattOS::GBM PROPERTIES IMPORTED_LOCATION \"{}\")\n"
        ),
        open_gl.display(), include.display(), gl.display(), include.display(), egl.display(), include.display(),
        gles2.display(), include.display(), gbm.display(),
    ))?;
    Ok(root)
}

fn qt_private_targets_bridge(repo_root: &Path, build: &Path, qt: &Path) -> Result<PathBuf> {
    // This is a deterministic, consumer-private CMake include file.  Keep it
    // in QtSvg's disposable build tree rather than publishing an auxiliary
    // mutable tree beside stage outputs.
    let cmake = build.join("MattOSQtPrivateTargets.cmake");
    let xkb = repo_root.join("out/build/xkbcommon/install/usr");
    let view = build.join("mattos-qt-cmake-view");
    let text = format!(r#"list(PREPEND CMAKE_PREFIX_PATH "{qt}")
set(XKB_INCLUDE_DIR "{xkb}/include")
set(XKB_LIBRARY "{xkb}/lib/x86_64-linux-gnu/libxkbcommon.so")
find_package(Qt6 6.11.2 CONFIG REQUIRED COMPONENTS BuildInternals Core Gui Widgets)
find_package(Qt6CorePrivate 6.11.2 CONFIG REQUIRED)
find_package(Qt6GuiPrivate 6.11.2 CONFIG REQUIRED)
find_package(Qt6WidgetsPrivate 6.11.2 CONFIG REQUIRED)
set(Qt6QmlTools_FOUND TRUE)
foreach(tool IN ITEMS qmllint qmlcontextpropertydump qmlformat qmltc)
  if(NOT TARGET Qt6::${{tool}})
    add_executable(Qt6::${{tool}} IMPORTED GLOBAL)
    set_target_properties(Qt6::${{tool}} PROPERTIES IMPORTED_LOCATION "{view}/bin/${{tool}}")
  endif()
endforeach()
foreach(tool IN ITEMS qmltyperegistrar qmlcachegen qmlaotstats qmlimportscanner)
  if(NOT TARGET Qt6::${{tool}})
    add_executable(Qt6::${{tool}} IMPORTED GLOBAL)
    set_target_properties(Qt6::${{tool}} PROPERTIES IMPORTED_LOCATION "{view}/libexec/${{tool}}")
  endif()
endforeach()
"#, qt = qt.display(), xkb = xkb.display(), view = view.display());
    fs::write(&cmake, text)?;
    Ok(cmake)
}

fn qt_shader_tools_private_bridge(repo_root: &Path, build: &Path, qt: &Path) -> Result<PathBuf> {
    // QtShaderTools is the producer of qsb and must remain below
    // QtDeclarative in the graph.  Its private dependency is only Gui; using
    // the full QML/Quick bridge here creates a false circular requirement.
    let cmake = build.join("MattOSQtShaderToolsPrivateTargets.cmake");
    let xkb = repo_root.join("out/build/xkbcommon/install/usr");
    fs::write(&cmake, format!(
        "list(PREPEND CMAKE_PREFIX_PATH \"{qt}\")\nset(XKB_INCLUDE_DIR \"{xkb}/include\")\nset(XKB_LIBRARY \"{xkb}/lib/x86_64-linux-gnu/libxkbcommon.so\")\nfind_package(Qt6 6.11.2 CONFIG REQUIRED COMPONENTS BuildInternals Core Gui)\nfind_package(Qt6CorePrivate 6.11.2 CONFIG REQUIRED)\nfind_package(Qt6GuiPrivate 6.11.2 CONFIG REQUIRED)\n",
        qt = qt.display(), xkb = xkb.display(),
    ))?;
    Ok(cmake)
}

fn qt_declarative_private_bridge(repo_root: &Path, build: &Path, qt: &Path) -> Result<PathBuf> {
    // The QML generators are built by this very stage.  Do not predeclare
    // imported targets for them (which would point at nonexistent files in a
    // disposable view); only expose QtBase's private ABI to the project.
    let cmake = build.join("MattOSQtDeclarativePrivateTargets.cmake");
    let xkb = repo_root.join("out/build/xkbcommon/install/usr");
    fs::write(&cmake, format!(
        "list(PREPEND CMAKE_PREFIX_PATH \"{qt}\")\nset(XKB_INCLUDE_DIR \"{xkb}/include\")\nset(XKB_LIBRARY \"{xkb}/lib/x86_64-linux-gnu/libxkbcommon.so\")\nfind_package(Qt6 6.11.2 CONFIG REQUIRED COMPONENTS BuildInternals Core Gui Widgets)\nfind_package(Qt6CorePrivate 6.11.2 CONFIG REQUIRED)\nfind_package(Qt6GuiPrivate 6.11.2 CONFIG REQUIRED)\nfind_package(Qt6WidgetsPrivate 6.11.2 CONFIG REQUIRED)\n",
        qt = qt.display(), xkb = xkb.display(),
    ))?;
    Ok(cmake)
}

/// Resolve only build-only tools from the host. Returned paths are supplied
/// explicitly to CMake, so its target package search never needs host paths.
fn qt_host_tool(name: &str) -> Result<PathBuf> {
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("Qt build requires trusted host executable `{name}` in PATH")
}

fn qt_host_tool_environment() -> Result<Vec<(&'static str, String)>> {
    let paths = ["cmake", "ninja", "gcc", "g++", "ar", "ranlib", "pkg-config"]
        .iter()
        .map(|tool| qt_host_tool(tool))
        .collect::<Result<Vec<_>>>()?;
    let path_dirs = paths.iter().filter_map(|path| path.parent()).collect::<BTreeSet<_>>();
    Ok(vec![
        ("PATH", std::env::join_paths(path_dirs)?.to_string_lossy().into_owned()),
        ("CMAKE_MAKE_PROGRAM", qt_host_tool("ninja")?.display().to_string()),
        ("CC", qt_host_tool("gcc")?.display().to_string()),
        ("CXX", qt_host_tool("g++")?.display().to_string()),
        ("AR", qt_host_tool("ar")?.display().to_string()),
        ("RANLIB", qt_host_tool("ranlib")?.display().to_string()),
        ("PKG_CONFIG", qt_host_tool("pkg-config")?.display().to_string()),
        // Clear inherited target-discovery state before explicitly supplying
        // the MattOS prefixes below. These variables are otherwise accepted
        // by CMake/Qt even when its command-line find-root policy is strict.
        ("CMAKE_PREFIX_PATH", String::new()),
        ("CMAKE_FIND_ROOT_PATH", String::new()),
        ("Qt6_DIR", String::new()),
        ("Qt6Core_DIR", String::new()),
        ("Qt6Gui_DIR", String::new()),
        ("Qt6Widgets_DIR", String::new()),
        ("QTDIR", String::new()),
        ("QT_PLUGIN_PATH", String::new()),
    ])
}

/// Qt generates and executes target-ABI host helpers (moc/rcc/qmake) during
/// its own build.  They must load the just-built Qt libraries, never a host
/// Qt library with an unrelated symbol-version namespace.  This path is
/// private to the disposable build tree and is not a target search prefix.
fn qt_build_tool_environment(build: &Path, qtbase: Option<&Path>) -> Result<Vec<(&'static str, String)>> {
    let mut env = qt_host_tool_environment()?;
    let mut libraries = vec![build.join("lib/x86_64-linux-gnu")];
    if let Some(qtbase) = qtbase {
        libraries.push(qtbase.join("lib/x86_64-linux-gnu"));
    }
    env.push((
        "LD_LIBRARY_PATH",
        std::env::join_paths(libraries)?.to_string_lossy().into_owned(),
    ));
    Ok(env)
}

/// Combine the explicitly trusted host executables with *declared* target
/// development metadata. QtWayland uses pkg-config for Wayland discovery and
/// executes wayland-scanner while generating protocol sources; both inputs
/// must be supplied rather than silently falling back to host metadata.
fn qt_target_environment(
    repo_root: &Path,
    build: &Path,
    qtbase: Option<&Path>,
) -> Result<Vec<(&'static str, String)>> {
    let mut env = qt_build_tool_environment(build, qtbase)?;
    // QtGui's target-owned font backend links against these providers. Keep
    // their library/pkg-config directories in every Qt module environment so
    // downstream consumers can resolve QtGui's DT_NEEDED closure without
    // consulting the host linker search path.
    let target = staged_library_environment(
        repo_root,
        &[
            "wayland", "xkbcommon", "libglvnd", "mesa", "openssl", "x11-compat",
            "freetype", "fontconfig", "expat", "zlib",
        ],
    )?;
    for (key, value) in target {
        if key == "PATH" {
            let host_path = env.iter().find(|(existing, _)| *existing == "PATH")
                .map(|(_, value)| value.clone()).unwrap_or_default();
            let value = format!("{}:{host_path}", repo_root.join("out/build/wayland/install/usr/bin").display());
            if let Some((_, existing)) = env.iter_mut().find(|(existing, _)| *existing == "PATH") {
                *existing = value;
            }
        } else if key == "LD_LIBRARY_PATH" {
            // Module configure/build runs generated QtBase tools as well as
            // target probes.  Retain the private Qt build-tree and installed
            // QtBase library directories before adding declared target ABI
            // paths; replacing them makes moc/rcc load an arbitrary host Qt.
            if let Some((_, existing)) = env.iter_mut().find(|(existing, _)| *existing == key) {
                *existing = format!("{existing}:{value}");
            } else {
                env.push((key, value));
            }
        } else if let Some((_, existing)) = env.iter_mut().find(|(existing, _)| *existing == key) {
            *existing = value;
        } else {
            env.push((key, value));
        }
    }
    Ok(env)
}

fn isolated_target_cmake_args(prefixes: &[PathBuf]) -> Result<Vec<String>> {
    // CMake lists are semicolon-delimited; OS PATH delimiters are not valid.
    let prefix = prefixes
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(";");
    Ok(vec![
        format!("-DCMAKE_MAKE_PROGRAM={}", qt_host_tool("ninja")?.display()),
        format!("-DCMAKE_C_COMPILER={}", qt_host_tool("gcc")?.display()),
        format!("-DCMAKE_CXX_COMPILER={}", qt_host_tool("g++")?.display()),
        format!("-DCMAKE_AR={}", qt_host_tool("ar")?.display()),
        format!("-DCMAKE_RANLIB={}", qt_host_tool("ranlib")?.display()),
        format!("-DPKG_CONFIG_EXECUTABLE={}", qt_host_tool("pkg-config")?.display()),
        format!("-DCMAKE_PREFIX_PATH={prefix}"),
        format!("-DCMAKE_FIND_ROOT_PATH={prefix}"),
        "-DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER".into(),
        "-DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY".into(),
        "-DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY".into(),
        "-DCMAKE_FIND_ROOT_PATH_MODE_PACKAGE=ONLY".into(),
        "-DCMAKE_FIND_USE_CMAKE_SYSTEM_PATH=OFF".into(),
        "-DCMAKE_FIND_USE_SYSTEM_ENVIRONMENT_PATH=OFF".into(),
        "-DCMAKE_INSTALL_RPATH=".into(),
        "-DCMAKE_BUILD_RPATH=".into(),
        "-DCMAKE_SKIP_RPATH=ON".into(),
    ])
}

fn qt_target_cmake_args(repo_root: &Path, extra_prefix: &[PathBuf]) -> Result<Vec<String>> {
    let bridge = qt_opengl_bridge(repo_root)?;
    let mut prefixes = vec![bridge, repo_root.join("out/build/libglvnd/install/usr"), repo_root.join("out/build/mesa/install/usr")];
    prefixes.extend_from_slice(extra_prefix);
    isolated_target_cmake_args(&prefixes)
}

fn qt_install(repo_root: &Path, build: &Path, install: &Path) -> Result<()> {
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", &build.display().to_string()],
        &[("DESTDIR", install.display().to_string())],
    )
}

/// Publish the already-built qsb executable into the build-only host-tools
/// area.  Qt Declarative invokes qsb while compiling its target shaders; qsb
/// is a generator, not a MattOS runtime component, so this deliberately does
/// not add it to a package or to any target prefix.  The symlink keeps the
/// host-tool view tied to the exact QtShaderTools stage output and avoids a
/// second mutable copy of the executable.
fn qt_host_qsb(repo_root: &Path) -> Result<PathBuf> {
    let target = repo_root.join("out/build/qtshadertools/install/usr/bin/qsb");
    if !target.is_file() || !target.starts_with(repo_root.join("out/build")) {
        bail!("Qt Declarative requires the verified QtShaderTools qsb host tool at {}", target.display());
    }
    let host_root = repo_root.join("out/host-tools/qt-6.11.2");
    let host_bin = host_root.join("bin");
    fs::create_dir_all(&host_bin)?;
    let host_qsb = host_bin.join("qsb");
    if host_qsb.exists() || fs::symlink_metadata(&host_qsb).is_ok() {
        remove_path_if_exists(&host_qsb)?;
    }
    std::os::unix::fs::symlink(&target, &host_qsb)
        .with_context(|| format!("linking verified qsb host tool at {}", host_qsb.display()))?;
    if fs::canonicalize(&host_qsb)? != fs::canonicalize(&target)? {
        bail!("qsb host-tool link does not resolve to the verified QtShaderTools output");
    }
    // Qt Declarative asks CMake for Qt6ShaderToolsTools and then consumes its
    // normal Qt6::qsb imported target.  The generated target file in the
    // target install points back into the target prefix, so provide the
    // minimal upstream-compatible host-tool package here with the same
    // target name and the host-tools executable path.  The shader macros are
    // copied as build metadata only; no target library or header is exposed.
    let host_cmake = host_root.join("lib/cmake/Qt6ShaderToolsTools");
    fs::create_dir_all(&host_cmake)?;
    let target_macros = repo_root.join(
        "out/build/qtshadertools/install/usr/lib/x86_64-linux-gnu/cmake/Qt6ShaderToolsTools/Qt6ShaderToolsMacros.cmake",
    );
    if !target_macros.is_file() {
        bail!("QtShaderTools host package is missing its upstream shader macros");
    }
    fs::copy(&target_macros, host_cmake.join("Qt6ShaderToolsMacros.cmake"))?;
    fs::write(host_cmake.join("Qt6ShaderToolsToolsConfig.cmake"), format!(
        "set(Qt6ShaderToolsTools_FOUND TRUE)\nif(NOT TARGET Qt6::qsb)\n  add_executable(Qt6::qsb IMPORTED GLOBAL)\n  set_target_properties(Qt6::qsb PROPERTIES IMPORTED_LOCATION \"{}\")\nendif()\ninclude(\"${{CMAKE_CURRENT_LIST_DIR}}/Qt6ShaderToolsMacros.cmake\")\nset(Qt6ShaderToolsTools_TARGETS Qt6::qsb)\n",
        host_qsb.display(),
    ))?;
    // Qt's module configs also probe the host tool packages for the ordinary
    // generators. They are build-only contracts; advertise only the tools
    // actually present in this isolated view, never the host Qt installation.
    let qtpaths = repo_root.join("out/build/qtbase/install/usr/bin/qtpaths");
    if qtpaths.is_file() {
        let host_qtpaths = host_bin.join("qtpaths");
        if host_qtpaths.exists() || fs::symlink_metadata(&host_qtpaths).is_ok() {
            remove_path_if_exists(&host_qtpaths)?;
        }
        std::os::unix::fs::symlink(&qtpaths, &host_qtpaths)?;
    }
    for package in ["Qt6GuiTools", "Qt6WidgetsTools", "Qt6QmlTools", "Qt6NetworkTools", "Qt6DBusTools"] {
        let directory = host_root.join("lib/cmake").join(package);
        fs::create_dir_all(&directory)?;
        fs::write(directory.join(format!("{package}Config.cmake")),
            format!("set({package}_FOUND TRUE)\n"))?;
    }
    let core_tools = host_root.join("lib/cmake/Qt6CoreTools");
    fs::create_dir_all(&core_tools)?;
    fs::write(core_tools.join("Qt6CoreToolsConfig.cmake"), format!(
        "set(Qt6CoreTools_FOUND TRUE)\nif(NOT TARGET Qt6::qtpaths AND EXISTS \"{}\")\n  add_executable(Qt6::qtpaths IMPORTED GLOBAL)\n  set_target_properties(Qt6::qtpaths PROPERTIES IMPORTED_LOCATION \"{}\")\nendif()\n",
        host_bin.join("qtpaths").display(), host_bin.join("qtpaths").display(),
    ))?;
    Ok(host_root)
}

/// Qt records target feature-discovery paths in installed qmake and CMake
/// metadata. CMake must see isolated stage prefixes while configuring Qt, but
/// those build-machine paths cannot be published. Every declared provider
/// below is packaged at `/usr`; rewrite only those exact, verified roots after
/// install. Refuse an unrecognised path rather than publishing metadata that
/// can silently point a downstream build back into `out/build`.
fn normalize_qt_target_metadata(repo_root: &Path, install: &Path) -> Result<()> {
    let modules = install.join("usr/mkspecs/modules");
    // Keep this in exact lockstep with QtBase's direct target-stage closure.
    // Each provider is installed by its own MattOS package at `/usr`.
    let mut published_prefixes = [
        "libglvnd", "mesa", "wayland", "xkbcommon", "x11-compat", "freetype", "fontconfig", "expat", "libpng", "openssl", "zlib",
    ]
        .into_iter()
        .map(|component| {
            (
                repo_root
                    .join("out/build")
                    .join(component)
                    .join("install/usr")
                    .to_string_lossy()
                    .into_owned(),
                "/usr",
            )
        })
        .collect::<Vec<_>>();
    // The X11 aggregate is assembled from independently staged X.Org
    // providers.  pkg-config expands those providers into qmake/CMake
    // metadata, so normalize their private build prefixes too.  They remain
    // one declared QtBase input (`x11-compat`); these names only describe
    // the aggregate's internal, non-published construction paths.
    for component in [
        "xorg-util-macros", "xorgproto", "xtrans", "libxau", "libxdmcp", "xcb-proto",
        "libxcb", "libx11", "libxext", "libxfixes", "libxrender", "xcb-util", "xcb-renderutil",
        "xcb-image", "xcb-cursor", "xcb-util-wm", "xcb-keysyms",
    ] {
        published_prefixes.push((
            repo_root.join("out/build").join(component).join("install/usr").to_string_lossy().into_owned(),
            "/usr",
        ));
    }
    // Qt's bundled libjpeg is not an externally consumable target dependency.
    // Its generated qmake feature record nevertheless carries the private
    // source-tree include directory.  The corresponding static archive is
    // private to Qt, so publishing an empty include list is the only valid
    // installed representation; downstream applications must use Qt's image
    // APIs rather than an unshipped Qt source directory.
    for bundled_source in [
        "src/3rdparty/libjpeg/src",
        "src/3rdparty/freetype/include",
        "src/3rdparty/libpng",
    ] {
        published_prefixes.push((
            repo_root.join("out/build/qtbase/source").join(bundled_source).to_string_lossy().into_owned(),
            "",
        ));
    }
    // QtBuildInternals records this source root for building Qt modules, but
    // MattOS deliberately does not ship an unpacked Qt source tree. This must
    // follow the more-specific bundled paths above: otherwise it would turn
    // them into relative paths before they can be removed.
    published_prefixes.push((
        repo_root.join("out/build/qtbase/source").to_string_lossy().into_owned(),
        "",
    ));
    // Qt's SPDX generator also records the absolute install prefixes of
    // sibling Qt modules (QtWayland, for example, attributes its bundled
    // protocol files to QtBase).  Those paths are valid while building but
    // are not valid in a published package.  Normalize the complete
    // source-owned Qt namespace in generated metadata: installed providers
    // become their target /usr location, while source references remain
    // stable provenance paths rather than leaking the developer checkout.
    for component in [
        "qtbase", "qtdeclarative", "qtsvg", "qtwayland", "qtshadertools",
        "qttools", "qtmultimedia", "qtspeech", "qt5compat", "qtpositioning",
        "qtlocation",
    ] {
        published_prefixes.push((
            repo_root
                .join("out/build")
                .join(component)
                .join("install/usr")
                .to_string_lossy()
                .into_owned(),
            "/usr",
        ));
    }
    let build_root = repo_root.join("out/build").to_string_lossy().into_owned();
    let entries = fs::read_dir(&modules)
        .with_context(|| format!("Qt install is missing qmake module metadata at {}", modules.display()))?;
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(OsStr::to_str) != Some("pri") {
            continue;
        }
        let original = fs::read_to_string(&path)?;
        let normalized = published_prefixes.iter().fold(original.clone(), |contents, (from, to)| {
            contents.replace(from, to)
        });
        if normalized.contains(&build_root) {
            bail!(
                "Qt qmake metadata {} retains an unpublished build path below {}",
                path.display(),
                build_root,
            );
        }
        if normalized != original {
            fs::write(&path, normalized)?;
        }
    }
    let cmake_root = install.join("usr/lib/x86_64-linux-gnu/cmake");
    fn normalize_cmake_tree(
        directory: &Path,
        published_prefixes: &[(String, &str)],
        build_root: &str,
    ) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                normalize_cmake_tree(&path, published_prefixes, build_root)?;
                continue;
            }
            if path.extension().and_then(OsStr::to_str) != Some("cmake") {
                continue;
            }
            let original = fs::read_to_string(&path)?;
            let normalized = published_prefixes.iter().fold(original.clone(), |contents, (from, to)| {
                contents.replace(from, to)
            });
            if normalized.contains(build_root) {
                bail!(
                    "Qt CMake metadata {} retains an unpublished build path below {}",
                    path.display(),
                    build_root,
                );
            }
            if normalized != original {
                fs::write(&path, normalized)?;
            }
        }
        Ok(())
    }
    normalize_cmake_tree(&cmake_root, &published_prefixes, &build_root)?;
    fn normalize_text_metadata_tree(
        directory: &Path,
        published_prefixes: &[(String, &str)],
        sbom_prefixes: &[(String, String)],
        build_root: &str,
    ) -> Result<()> {
        if !directory.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                normalize_text_metadata_tree(path.as_path(), published_prefixes, sbom_prefixes, build_root)?;
                continue;
            }
            let extension = path.extension().and_then(OsStr::to_str);
            if !matches!(extension, Some("prl" | "pc" | "la" | "json")) {
                continue;
            }
            let original = fs::read_to_string(&path)?;
            let normalized = published_prefixes.iter().fold(original.clone(), |contents, (from, to)| {
                contents.replace(from, to)
            });
            let normalized = sbom_prefixes.iter().fold(normalized, |contents, (from, to)| {
                contents.replace(from, to)
            });
            if normalized.contains(build_root) {
                bail!(
                    "Qt text metadata {} retains an unpublished build path below {}",
                    path.display(),
                    build_root,
                );
            }
            if normalized != original {
                fs::write(&path, normalized)?;
            }
        }
        Ok(())
    }
    let mut sbom_prefixes = published_prefixes
        .iter()
        .map(|(from, to)| (from.clone(), (*to).to_string()))
        .collect::<Vec<_>>();
    for component in [
        "qtbase", "qtdeclarative", "qtsvg", "qtwayland", "qtshadertools",
        "qttools", "qtmultimedia", "qtspeech", "qt5compat", "qtpositioning",
        "qtlocation",
    ] {
        sbom_prefixes.push((
            repo_root
                .join("out/build")
                .join(component)
                .join("source")
                .to_string_lossy()
                .into_owned(),
            format!("/usr/src/mattos/qt/{component}"),
        ));
    }
    normalize_text_metadata_tree(
        &install.join("usr"),
        &published_prefixes,
        &sbom_prefixes,
        &build_root,
    )?;
    let sbom_root = install.join("usr/sbom");
    if sbom_root.is_dir() {
        for entry in fs::read_dir(&sbom_root)? {
            let path = entry?.path();
            if path.extension().and_then(OsStr::to_str) != Some("spdx") {
                continue;
            }
            let original = fs::read_to_string(&path)?;
            let normalized = sbom_prefixes.iter().fold(original.clone(), |contents, (from, to)| {
                contents.replace(from, to)
            });
            if normalized.contains(&build_root) {
                bail!(
                    "Qt SPDX metadata {} retains an unpublished build path below {}",
                    path.display(),
                    build_root,
                );
            }
            if normalized != original {
                fs::write(&path, normalized)?;
            }
        }
    }
    Ok(())
}

/// Keep CMake/Ninja's dependency graph and object files across invalidated Qt
/// stage runs.  The stamp is an explicit recipe-layout contract: changing the
/// generator/toolchain layout bumps it and deliberately starts clean, while
/// ordinary source edits are handled incrementally by CMake and Ninja.
fn prepare_incremental_qt_build(build: &Path, layout_identity: &str) -> Result<()> {
    let stamp = build.join(".mattos-qt-build-layout");
    let reusable = fs::read_to_string(&stamp)
        .is_ok_and(|saved| saved.trim_end() == layout_identity)
        && build.join("build.ninja").is_file();
    if !reusable {
        remove_path_if_exists(build)?;
        fs::create_dir_all(build)?;
        fs::write(&stamp, format!("{layout_identity}\n"))?;
    }
    Ok(())
}

fn build_qtbase(repo_root: &Path) -> Result<()> {
    let root = repo_root.join("out/build/qtbase");
    let source = root.join("source");
    let build = root.join("build");
    let install = root.join("install");
    sync_build_source(&repo_root.join("src/desktop/qt/qtbase"), &source)?;
    remove_path_if_exists(&install)?;
    prepare_incremental_qt_build(&build, "qtbase-ninja-v1")?;
    let mut command = vec![
        "-S".into(), source.display().to_string(), "-B".into(), build.display().to_string(),
        "-G".into(), "Ninja".into(), "-DCMAKE_INSTALL_PREFIX=/usr".into(),
        "-DINSTALL_LIBDIR=lib/x86_64-linux-gnu".into(),
        "-DQT_BUILD_EXAMPLES_BY_DEFAULT=OFF".into(), "-DQT_BUILD_TESTS_BY_DEFAULT=OFF".into(),
        "-DFEATURE_icu=OFF".into(), "-DFEATURE_fontconfig=ON".into(), "-DINPUT_freetype=system".into(), "-DINPUT_harfbuzz=qt".into(),
        // KWin's documented Qt closure uses the UI compiler package. Keep it
        // in the source-owned QtBase target rather than accepting a host
        // Qt6UiTools configuration during downstream CMake discovery.
        "-DFEATURE_uitools=ON".into(),
        // KWindowSystem/KWin build their X11 compatibility code against
        // QtGuiPrivate's generated qtx11extras headers. Those headers and
        // the Qt XCB platform backend are only produced when QtBase's XCB
        // feature is enabled; keep this in the source-owned Qt foundation.
        "-DFEATURE_xcb=ON".into(),
        // Brotli is not yet a MattOS-owned target ABI. Do not let Qt's
        // optional network compression probe consume an undeclared host header.
        "-DFEATURE_brotli=OFF".into(),
    ];
    let target_prefixes = vec![
        repo_root.join("out/build/wayland/install/usr"),
        repo_root.join("out/build/xkbcommon/install/usr"),
        repo_root.join("out/build/openssl/install/usr"),
        repo_root.join("out/build/x11-compat/install/usr"),
        repo_root.join("out/build/freetype/install/usr"),
        repo_root.join("out/build/fontconfig/install/usr"),
        repo_root.join("out/build/expat/install/usr"),
    ];
    command.extend(qt_target_cmake_args(repo_root, &target_prefixes)?);
    let refs = command.iter().map(String::as_str).collect::<Vec<_>>();
    let mut env = qt_target_environment(repo_root, &build, None)?;
    // CMake's imported X11 targets correctly locate the headers, but Qt's
    // Wayland EGL integration also includes EGL's platform header directly.
    // That header is compiled by a different Qt target and therefore does
    // not inherit the XCB target's include interface.  Add the same
    // source-owned X11 include root to the compiler environment for this
    // QtBase build only; it is not a host include and does not affect the
    // standalone module stages.
    let x11_include = repo_root.join("out/build/x11-compat/install/usr/include");
    for key in ["CFLAGS", "CXXFLAGS"] {
        env.push((key, format!("-isystem {}", x11_include.display())));
    }
    run_cmd_with_env_overrides(&build, "cmake", &refs, &env)?;
    run_cmd_with_env_overrides(&build, "cmake", &["--build", "."], &env)?;
    qt_install(repo_root, &build, &install)?;
    // Qt records the disposable staging prefix in QLibraryInfo.  The
    // upstream-supported qt.conf relocation file makes the installed
    // /usr/bin tools and every Qt application resolve /usr/plugins after the
    // stage is copied into the target root, rather than the build host's
    // out/build/qtbase/install/usr/plugins path.
    fs::write(install.join("usr/bin/qt.conf"), "[Paths]\nPrefix=..\n")?;
    normalize_qt_target_metadata(repo_root, &install)?;
    if !install.join("usr/lib/x86_64-linux-gnu/libQt6Core.so").is_file() {
        bail!("QtBase did not publish libQt6Core");
    }
    Ok(())
}

fn build_qt_module(repo_root: &Path, component: &str) -> Result<()> {
    let root = repo_root.join("out/build").join(component);
    let source = root.join("source");
    let build = root.join("build");
    let install = root.join("install");
    sync_build_source(&repo_root.join("src/desktop/qt").join(component), &source)?;
    remove_path_if_exists(&install)?;
    prepare_incremental_qt_build(&build, &format!("{component}-ninja-v1"))?;
    let qt = repo_root.join("out/build/qtbase/install/usr");
    let mut prefixes = vec![qt.clone()];
    let mut command = vec![
        "-S".into(), source.display().to_string(), "-B".into(), build.display().to_string(),
        "-G".into(), "Ninja".into(),
        "-DCMAKE_INSTALL_PREFIX=/usr".into(),
        "-DINSTALL_LIBDIR=lib/x86_64-linux-gnu".into(),
        format!("-DQt6_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6", qt.display()),
        // Qt Declarative's feature probe uses FindPython (not FindPython3).
        // This is an explicitly trusted generator, never a target runtime
        // dependency; pin its executable rather than opening host package
        // discovery for target libraries.
        format!("-DPython_EXECUTABLE={}", qt_host_tool("python3")?.display()),
    ];
    if matches!(component, "qtpositioning" | "qtlocation" | "qt5compat") {
        // Qt's umbrella config resolves optional module packages relative to
        // its own multiarch directory.  Present a disposable symlink-only
        // view so standalone Qt modules can discover the source-owned QML,
        // Quick, and Positioning packages without mutating QtBase output.
        let view = build.join("mattos-qt-cmake-view/lib/x86_64-linux-gnu/cmake");
        fs::create_dir_all(&view)?;
        let qt_umbrella = qt.join("lib/x86_64-linux-gnu/cmake/Qt6");
        let view_umbrella = view.join("Qt6");
        fs::create_dir_all(&view_umbrella)?;
        for entry in fs::read_dir(&qt_umbrella)? {
            let entry = entry?;
            if entry.path().is_file() { fs::copy(entry.path(), view_umbrella.join(entry.file_name()))?; }
        }
        let view_root = build.join("mattos-qt-cmake-view");
        // Imported Qt targets also refer to plugin and QML paths relative to
        // the package prefix.  Merge only the top-level provider directories
        // into the disposable view; never expose host paths or mutate a
        // published Qt installation.
        for (prefix, directories) in [
            (qt.clone(), vec!["plugins", "mkspecs"]),
            // Qt5Compat's GraphicalEffects module is generated with Qt QML's
            // host-side tools.  They are published by the QtDeclarative stage
            // under bin/libexec, but are not target runtime directories. Keep
            // them in this disposable consumer view so the private-target
            // bridge can expose the exact staged tools without consulting a
            // host Qt installation.
            (repo_root.join("out/build/qtdeclarative/install/usr"), vec!["qml", "plugins", "bin", "libexec"]),
            (repo_root.join("out/build/qtpositioning/install/usr"), vec!["qml", "plugins"]),
            (repo_root.join("out/build/qtshadertools/install/usr"), vec!["bin"]),
        ] {
            for directory in directories {
                let source_directory = prefix.join(directory);
                if !source_directory.is_dir() { continue; }
                let destination_directory = view_root.join(directory);
                fs::create_dir_all(&destination_directory)?;
                for entry in fs::read_dir(source_directory)? {
                    let entry = entry?;
                    let destination = destination_directory.join(entry.file_name());
                    if destination.symlink_metadata().is_err() {
                        if entry.path().is_file() {
                            fs::copy(entry.path(), destination)?;
                        } else {
                            std::os::unix::fs::symlink(entry.path(), destination)?;
                        }
                    }
                }
            }
        }
        let view_include = view_root.join("include");
        fs::create_dir_all(&view_include)?;
        for (prefix, dirs) in [
            (qt.clone(), vec!["QtCore", "QtGui", "QtNetwork", "QtOpenGL", "QtOpenGLWidgets", "QtXml"]),
            (repo_root.join("out/build/qtdeclarative/install/usr"), vec!["QtQml", "QtQuick", "QtQuickControls2"]),
            (repo_root.join("out/build/qtpositioning/install/usr"), vec!["QtPositioning"]),
        ] {
            for directory in dirs {
                let source_directory = prefix.join("include").join(directory);
                let destination = view_include.join(directory);
                if source_directory.is_dir() && destination.symlink_metadata().is_err() {
                    std::os::unix::fs::symlink(source_directory, destination)?;
                }
            }
        }
        for prefix in [
            qt.clone(),
            repo_root.join("out/build/qtdeclarative/install/usr"),
            repo_root.join("out/build/qtshadertools/install/usr"),
            repo_root.join("out/build/qtpositioning/install/usr"),
        ] {
            let include_root = prefix.join("include");
            if !include_root.is_dir() { continue; }
            for entry in fs::read_dir(include_root)? {
                let entry = entry?;
                let destination = view_include.join(entry.file_name());
                if destination.symlink_metadata().is_err() {
                    std::os::unix::fs::symlink(entry.path(), destination)?;
                }
            }
        }
        let view_lib = view_root.join("lib/x86_64-linux-gnu");
        fs::create_dir_all(&view_lib)?;
        for prefix in [qt.clone(), repo_root.join("out/build/qtdeclarative/install/usr"), repo_root.join("out/build/qtshadertools/install/usr"), repo_root.join("out/build/qtpositioning/install/usr")] {
            let library_directory = prefix.join("lib/x86_64-linux-gnu");
            if library_directory.is_dir() {
                for entry in fs::read_dir(library_directory)? {
                    let entry = entry?;
                    let destination = view_lib.join(entry.file_name());
                    if destination.symlink_metadata().is_err() {
                        // CMake may clean files it considers foreign in the
                        // build tree.  Keep the view deterministic by
                        // materializing regular staged libraries and
                        // preserving any source symlink target explicitly.
                        if entry.path().is_file() {
                            fs::copy(entry.path(), destination)?;
                        } else if let Ok(target) = fs::read_link(entry.path()) {
                            std::os::unix::fs::symlink(target, destination)?;
                        }
                    }
                }
            }
        }
        for (prefix, packages) in [
            (qt.clone(), vec![
                "Qt6Core", "Qt6CorePrivate", "Qt6Gui", "Qt6GuiPrivate",
                "Qt6Network", "Qt6NetworkPrivate", "Qt6OpenGL", "Qt6OpenGLPrivate",
                "Qt6OpenGLWidgets", "Qt6Xml", "Qt6BuildInternals",
            ]),
            (repo_root.join("out/build/qtdeclarative/install/usr"), vec!["Qt6Qml", "Qt6Quick", "Qt6QuickControls2", "Qt6QuickTemplates2", "Qt6QuickShapes"]),
            (repo_root.join("out/build/qtshadertools/install/usr"), vec!["Qt6ShaderTools"]),
            (repo_root.join("out/build/qtpositioning/install/usr"), vec!["Qt6Positioning", "Qt6PositioningPrivate", "Qt6PositioningQuick", "Qt6PositioningQuickPrivate"]),
        ] {
            for package in packages {
                let source_package = prefix.join("lib/x86_64-linux-gnu/cmake").join(package);
                let destination = view.join(package);
                if source_package.is_dir() && destination.symlink_metadata().is_err() {
                    std::os::unix::fs::symlink(source_package, destination)?;
                }
            }
        }
        // Preserve the complete source-owned package-config namespace.  Qt
        // module configs pull optional/private dependencies transitively (for
        // example Gui -> DBus and Quick -> Qml); selectively listing only the
        // top-level modules makes those imports appear broken in a standalone
        // build even though their staged libraries are present.
        for prefix in [
            qt.clone(),
            repo_root.join("out/build/qtdeclarative/install/usr"),
            repo_root.join("out/build/qtshadertools/install/usr"),
            repo_root.join("out/build/qtpositioning/install/usr"),
        ] {
            let cmake_root = prefix.join("lib/x86_64-linux-gnu/cmake");
            if !cmake_root.is_dir() { continue; }
            for entry in fs::read_dir(cmake_root)? {
                let entry = entry?;
                let destination = view.join(entry.file_name());
                if destination.symlink_metadata().is_err() {
                    std::os::unix::fs::symlink(entry.path(), destination)?;
                }
            }
        }
        // Reuse the canonical Qt/KDE consumer view so private target
        // dependencies, recursively merged headers, plugins, QML modules,
        // and host-only tool adapters have one implementation.
        qt_multimodule_cmake_view(repo_root, &build)?;
        let xkb = repo_root.join("out/build/xkbcommon/install/usr");
        let xkb_package = build.join("mattos-qt-cmake-view/lib/x86_64-linux-gnu/cmake/XKB");
        fs::create_dir_all(&xkb_package)?;
        fs::write(xkb_package.join("XKBConfig.cmake"), format!(
            "set(XKB_FOUND TRUE)\nif(NOT TARGET XKB::XKB)\n  add_library(XKB::XKB UNKNOWN IMPORTED)\n  set_target_properties(XKB::XKB PROPERTIES IMPORTED_LOCATION \"{}/lib/x86_64-linux-gnu/libxkbcommon.so\" INTERFACE_INCLUDE_DIRECTORIES \"{}/include\")\nendif()\n",
            xkb.display(), xkb.display()
        ))?;
        fs::write(xkb_package.join("XKBConfigVersion.cmake"), "set(PACKAGE_VERSION 0.9.0)\nset(PACKAGE_VERSION_COMPATIBLE TRUE)\nset(PACKAGE_VERSION_EXACT TRUE)\n")?;
        prefixes.push(build.join("mattos-qt-cmake-view"));
        command.push(format!("-DQt6_DIR={}/Qt6", view.display()));
        // Standalone Qt repositories do not receive QtBase's disposable
        // config-test build tree.  The installed target is already stripped
        // by MattOS's package pipeline, so avoid asking the relocated view to
        // regenerate QtBase's strip wrapper from an absent source tree.
        command.push("-DQT_NO_STRIP_WRAPPER=ON".into());
        command.push("-DQT_NO_PACKAGE_VERSION_CHECK=TRUE".into());
        if matches!(component, "qtpositioning" | "qtlocation") {
            // The canonical disposable view provides the versioned adapter;
            // the host installation remains untouched and target CMake never
            // searches it directly.
            let qml_tools_view = build.join("mattos-qt-cmake-view/lib/x86_64-linux-gnu/cmake/Qt6QmlTools");
            command.push(format!("-DQt6QmlTools_DIR={}", qml_tools_view.display()));
        }
    }
    // QtSvg's SvgWidgets add-on reaches private Widgets targets when it is
    // built standalone.  Supply the staged QtBase public/private target
    // packages after project() rather than mutating QtSvg source or allowing
    // CMake to discover a host Qt installation.
    if matches!(component, "qtsvg" | "qtdeclarative" | "qt5compat" | "qttools" | "qtpositioning" | "qtlocation" | "qtmultimedia" | "qtspeech") {
        let private_targets = if component == "qtdeclarative" {
            qt_declarative_private_bridge(repo_root, &build, &qt)?
        } else {
            qt_private_targets_bridge(repo_root, &build, &qt)?
        };
        command.push(format!("-DCMAKE_PROJECT_INCLUDE={}", private_targets.display()));
    }
    if component == "qtshadertools" {
        let private_targets = qt_shader_tools_private_bridge(repo_root, &build, &qt)?;
        command.push(format!("-DCMAKE_PROJECT_INCLUDE={}", private_targets.display()));
    }
    if component == "qtwayland" {
        let wayland = repo_root.join("out/build/wayland/install/usr");
        let scanner = wayland.join("bin/wayland-scanner");
        if !scanner.is_file() {
            bail!("QtWayland requires MattOS-owned wayland-scanner at {}", scanner.display());
        }
        prefixes.push(wayland);
        prefixes.push(repo_root.join("out/build/xkbcommon/install/usr"));
        command.push(format!("-DWaylandScanner_EXECUTABLE={}", scanner.display()));
    }
    if component == "qtdeclarative" {
        let host_qt = qt_host_qsb(repo_root)?;
        let shader_tools = repo_root.join("out/build/qtshadertools/install/usr");
        command.push(format!("-DQt6HostInfo_DIR={}/out/build/qtbase/build/lib/x86_64-linux-gnu/cmake/Qt6HostInfo", repo_root.display()));
        command.push(format!("-DQt6ShaderToolsTools_DIR={}/lib/cmake/Qt6ShaderToolsTools", host_qt.display()));
        command.push(format!("-DQT_HOST_PATH={}", host_qt.display()));
        command.push(format!("-DQT_HOST_PATH_CMAKE_DIR={}/lib/cmake", host_qt.display()));
        command.push(format!("-DQT_QSB_EXECUTABLE={}/bin/qsb", host_qt.display()));
        // Qt6Config keeps the target umbrella in QtBase, while this
        // additional target prefix supplies the separately staged
        // QtShaderTools component beside it.
        command.push(format!("-DQT_ADDITIONAL_PACKAGES_PREFIX_PATH={}", shader_tools.display()));
        prefixes.push(shader_tools);
    }
    if component == "qtpositioning" {
        command.push("-DQT_NO_PACKAGE_VERSION_CHECK=TRUE".into());
        prefixes.push(repo_root.join("out/build/qtdeclarative/install/usr"));
        let view_cmake = build.join("mattos-qt-cmake-view/lib/x86_64-linux-gnu/cmake");
        command.push(format!("-DQt6Qml_DIR={}/Qt6Qml", view_cmake.display()));
        command.push(format!("-DQt6Quick_DIR={}/Qt6Quick", view_cmake.display()));
    }
    if component == "qtlocation" {
        prefixes.push(repo_root.join("out/build/qtdeclarative/install/usr"));
        prefixes.push(repo_root.join("out/build/qtpositioning/install/usr"));
        let view_cmake = build.join("mattos-qt-cmake-view/lib/x86_64-linux-gnu/cmake");
        command.push(format!("-DQt6Qml_DIR={}/Qt6Qml", view_cmake.display()));
        command.push(format!("-DQt6Quick_DIR={}/Qt6Quick", view_cmake.display()));
        command.push(format!("-DQt6Positioning_DIR={}/Qt6Positioning", view_cmake.display()));
        command.push(format!("-DQt6PositioningQuick_DIR={}/Qt6PositioningQuick", view_cmake.display()));
    }
    if component == "qttools" {
        // KWin consumes UiTools; the documentation, designer, assistant and
        // translation applications are separate products and would pull in
        // undeclared tool/runtime closures (including qlitehtml).
        for feature in ["assistant", "qdoc", "designer", "distancefieldgenerator", "kmap2qmap", "linguist", "pixeltool", "qev", "qtattributionsscanner", "qtdiag", "qtplugininfo"] {
            command.push(format!("-DFEATURE_{feature}=OFF"));
        }
    }
    if component == "qtmultimedia" {
        // TextToSpeech consumes QtMultimedia's core ABI. Keep optional media
        // backends disabled here so the foundation does not acquire an
        // undeclared GStreamer/ALSA/PulseAudio/FFmpeg target closure.
        for (feature, value) in [
            ("alsa", "OFF"),
            ("gstreamer", "no"),
            ("pulseaudio", "OFF"),
            ("pipewire", "OFF"),
            ("ffmpeg", "OFF"),
        ] {
            command.push(format!("-DFEATURE_{feature}={value}"));
        }
        let shader_tools = repo_root.join("out/build/qtshadertools/install/usr");
        prefixes.push(shader_tools.clone());
        command.push(format!(
            "-DQt6ShaderTools_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6ShaderTools",
            shader_tools.display()
        ));
        command.push(format!("-DQT_QSB_EXECUTABLE={}/bin/qsb", shader_tools.display()));
    }
    if component == "qtspeech" {
        // QtSpeech's required Multimedia package is a separate standalone
        // Qt repository. Point CMake at the staged target package directly.
        let multimedia = repo_root.join("out/build/qtmultimedia/install/usr");
        prefixes.push(multimedia.clone());
        command.push(format!(
            "-DQt6Multimedia_DIR={}/lib/x86_64-linux-gnu/cmake/Qt6Multimedia",
            multimedia.display()
        ));
    }
    command.extend(qt_target_cmake_args(repo_root, &prefixes)?);
    let refs = command.iter().map(String::as_str).collect::<Vec<_>>();
    let mut env = qt_target_environment(repo_root, &build, Some(&qt))?;
    if component == "qtwayland" {
        // QtWayland's EGL compositor integration includes GLVND's
        // eglplatform.h directly.  GLVND exposes the X11 branch of that
        // header when XCB support is enabled, so make the same source-owned
        // X11 include root available to every QtWayland compile target.  The
        // target environment above deliberately keeps host include paths out;
        // this is the corresponding staged target path, not a host fallback.
        let x11_include = repo_root.join("out/build/x11-compat/install/usr/include");
        for key in ["CFLAGS", "CXXFLAGS"] {
            env.push((key, format!("-isystem {}", x11_include.display())));
        }
    }
    if matches!(component, "qtdeclarative" | "qt5compat") {
        // Qt5Compat's GraphicalEffects QML module executes the QtDeclarative
        // generators during the build.  Those generators are host
        // executables built by MattOS, but they link against the staged Qt
        // Declarative/ShaderTools target libraries.  Keep that runtime
        // closure in the disposable build environment; it is never a host
        // search path and is not published as part of the host-tool view.
        let shader_lib = repo_root.join("out/build/qtshadertools/install/usr/lib/x86_64-linux-gnu");
        let declarative_lib = repo_root.join("out/build/qtdeclarative/install/usr/lib/x86_64-linux-gnu");
        if let Some((_, value)) = env.iter_mut().find(|(key, _)| *key == "LD_LIBRARY_PATH") {
            *value = format!("{}:{}:{value}", declarative_lib.display(), shader_lib.display());
        }
    }
    if matches!(component, "qtpositioning" | "qtlocation") {
        let runtime_libs = [
            repo_root.join("out/build/qtdeclarative/install/usr/lib/x86_64-linux-gnu"),
            repo_root.join("out/build/qtshadertools/install/usr/lib/x86_64-linux-gnu"),
            repo_root.join("out/build/qtpositioning/install/usr/lib/x86_64-linux-gnu"),
        ];
        if let Some((_, value)) = env.iter_mut().find(|(key, _)| *key == "LD_LIBRARY_PATH") {
            for directory in runtime_libs.iter().rev() {
                *value = format!("{}:{value}", directory.display());
            }
        }
    }
    run_cmd_with_env_overrides(&build, "cmake", &refs, &env)?;
    run_cmd_with_env_overrides(&build, "cmake", &["--build", "."], &env)?;
    qt_install(repo_root, &build, &install)?;
    // Standalone Qt modules emit the same qmake/CMake/SPDX metadata as
    // QtBase. Normalize it at the output boundary before any package stage
    // can publish the install tree; in particular QtWayland's SPDX document
    // records absolute QtBase install paths.
    normalize_qt_target_metadata(repo_root, &install)?;
    let libdir = install.join("usr/lib/x86_64-linux-gnu");
    match component {
        "qtsvg" if libdir.join("libQt6Svg.so").is_file() => Ok(()),
        // In Qt 6.11 the client platform plugin is deliberately built by
        // QtBase's QtGui Wayland feature.  This module contributes the
        // compositor ABI and its plugin family, so validate those actual
        // upstream-owned outputs instead of demanding a duplicate client
        // library from QtWayland.
        "qtwayland"
            if libdir.join("libQt6WaylandCompositor.so").is_file()
                && install.join("usr/plugins/wayland-graphics-integration-server").is_dir() => Ok(()),
        "qtdeclarative" if libdir.join("libQt6Qml.so").is_file() => Ok(()),
        "qtshadertools" if install.join("usr/bin/qsb").is_file() => Ok(()),
        "qt5compat" if libdir.join("libQt6Core5Compat.so").is_file() => Ok(()),
        "qttools" if libdir.join("libQt6UiTools.so").is_file() => Ok(()),
        "qtpositioning" if libdir.join("libQt6Positioning.so").is_file() => Ok(()),
        "qtlocation" if libdir.join("libQt6Location.so").is_file() => Ok(()),
        "qtmultimedia" if libdir.join("libQt6Multimedia.so").is_file() => Ok(()),
        "qtspeech" if libdir.join("libQt6TextToSpeech.so").is_file() => Ok(()),
        "qtsvg" => bail!("QtSvg configure completed without publishing libQt6Svg"),
        "qtwayland" => bail!("QtWayland configure completed without publishing the compositor ABI/plugins"),
        "qtdeclarative" => bail!("Qt Declarative configure completed without publishing libQt6Qml"),
        "qtshadertools" => bail!("Qt Shader Tools configure completed without publishing qsb"),
        "qt5compat" => bail!("Qt Core5Compat configure completed without publishing libQt6Core5Compat"),
        "qttools" => bail!("Qt Tools configure completed without publishing libQt6UiTools"),
        "qtpositioning" => bail!("Qt Positioning configure completed without publishing libQt6Positioning"),
        "qtlocation" => bail!("Qt Location configure completed without publishing libQt6Location"),
        "qtmultimedia" => bail!("Qt Multimedia configure completed without publishing libQt6Multimedia"),
        "qtspeech" => bail!("Qt Speech configure completed without publishing libQt6TextToSpeech"),
        _ => bail!("unknown Qt module build contract: {component}"),
    }
}

fn build_qtsvg(repo_root: &Path) -> Result<()> { build_qt_module(repo_root, "qtsvg") }
fn build_qtwayland(repo_root: &Path) -> Result<()> { build_qt_module(repo_root, "qtwayland") }
fn build_qtdeclarative(repo_root: &Path) -> Result<()> { build_qt_module(repo_root, "qtdeclarative") }
fn build_qtpositioning(repo_root: &Path) -> Result<()> { build_qt_module(repo_root, "qtpositioning") }
fn build_qtlocation(repo_root: &Path) -> Result<()> { build_qt_module(repo_root, "qtlocation") }
fn build_qtshadertools(repo_root: &Path) -> Result<()> { build_qt_module(repo_root, "qtshadertools") }
fn build_qt5compat(repo_root: &Path) -> Result<()> { build_qt_module(repo_root, "qt5compat") }
fn build_qttools(repo_root: &Path) -> Result<()> { build_qt_module(repo_root, "qttools") }
fn build_qtmultimedia(repo_root: &Path) -> Result<()> { build_qt_module(repo_root, "qtmultimedia") }
fn build_qtspeech(repo_root: &Path) -> Result<()> { build_qt_module(repo_root, "qtspeech") }

#[cfg(test)]
mod qt_tests {
    use super::*;
    use crate::stage_graph::direct_dependencies;

    #[test]
    fn incremental_build_preserves_matching_ninja_state() {
        let temporary = tempfile::tempdir().unwrap();
        let build = temporary.path().join("build");
        fs::create_dir_all(&build).unwrap();
        fs::write(build.join("build.ninja"), "fixture").unwrap();
        fs::write(build.join(".mattos-qt-build-layout"), "layout-v1\n").unwrap();
        fs::write(build.join("compiled-object"), "keep").unwrap();

        prepare_incremental_qt_build(&build, "layout-v1").unwrap();

        assert_eq!(fs::read_to_string(build.join("compiled-object")).unwrap(), "keep");
    }

    #[test]
    fn incremental_build_fails_closed_to_clean_state_for_changed_layout() {
        let temporary = tempfile::tempdir().unwrap();
        let build = temporary.path().join("build");
        fs::create_dir_all(&build).unwrap();
        fs::write(build.join("build.ninja"), "fixture").unwrap();
        fs::write(build.join(".mattos-qt-build-layout"), "old-layout\n").unwrap();
        fs::write(build.join("compiled-object"), "discard").unwrap();

        prepare_incremental_qt_build(&build, "new-layout").unwrap();

        assert!(!build.join("compiled-object").exists());
        assert_eq!(
            fs::read_to_string(build.join(".mattos-qt-build-layout")).unwrap(),
            "new-layout\n"
        );
    }

    #[test]
    fn incremental_build_fails_closed_when_ninja_graph_is_missing() {
        let temporary = tempfile::tempdir().unwrap();
        let build = temporary.path().join("build");
        fs::create_dir_all(&build).unwrap();
        fs::write(build.join(".mattos-qt-build-layout"), "layout-v1\n").unwrap();
        fs::write(build.join("compiled-object"), "discard").unwrap();

        prepare_incremental_qt_build(&build, "layout-v1").unwrap();

        assert!(!build.join("compiled-object").exists());
    }

    #[test]
    fn qtsvg_declares_the_private_xkbcommon_input_it_resolves() {
        assert!(direct_dependencies(BuildStage::QtSvg).contains(&"qtbase"));
        assert!(direct_dependencies(BuildStage::QtSvg).contains(&"xkbcommon"));
    }

    #[test]
    fn qt_target_environment_exposes_the_source_owned_qtgui_font_closure() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        for component in ["wayland", "xkbcommon", "libglvnd", "mesa", "openssl", "x11-compat", "freetype", "fontconfig", "expat"] {
            fs::create_dir_all(repo.join("out/build").join(component).join("install/usr/lib/x86_64-linux-gnu")).unwrap();
        }
        let environment = qt_target_environment(repo, &repo.join("out/build/qtbase/build"), None).unwrap();
        let ldflags = environment.iter().find(|(key, _)| *key == "LD_LIBRARY_PATH").unwrap().1.clone();
        for component in ["freetype", "fontconfig", "expat"] {
            assert!(ldflags.contains(&repo.join("out/build").join(component).join("install/usr/lib/x86_64-linux-gnu").display().to_string()));
        }
    }

    #[test]
    fn private_target_bridge_uses_only_staged_multiarch_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        let qt = repo.join("out/build/qtbase/install/usr");
        let build = repo.join("out/build/qtsvg/build");
        fs::create_dir_all(repo.join("out/build/xkbcommon/install/usr/include")).unwrap();
        fs::create_dir_all(repo.join("out/build/xkbcommon/install/usr/lib/x86_64-linux-gnu")).unwrap();
        fs::create_dir_all(&build).unwrap();
        let bridge = qt_private_targets_bridge(repo, &build, &qt).unwrap();
        assert!(bridge.starts_with(&build));
        let contents = fs::read_to_string(bridge).unwrap();
        assert!(!contents.contains("set(Qt6_DIR"));
        assert!(contents.contains("out/build/xkbcommon/install/usr"));
        assert!(!contents.contains("/usr/lib/cmake/Qt6"));
    }

    #[test]
    fn qt_host_environment_clears_inherited_qt_target_discovery() {
        let environment = qt_host_tool_environment().unwrap();
        for variable in [
            "CMAKE_PREFIX_PATH", "CMAKE_FIND_ROOT_PATH", "Qt6_DIR",
            "Qt6Core_DIR", "Qt6Gui_DIR", "Qt6Widgets_DIR", "QTDIR", "QT_PLUGIN_PATH",
        ] {
            assert_eq!(
                environment.iter().find(|(key, _)| *key == variable).unwrap().1,
                "",
                "{variable} must not leak from the host",
            );
        }
    }

    #[test]
    fn qsb_is_exposed_only_through_the_build_only_host_tool_view() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        let target = repo.join("out/build/qtshadertools/install/usr/bin/qsb");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"verified qsb").unwrap();
        let macros = repo.join(
            "out/build/qtshadertools/install/usr/lib/x86_64-linux-gnu/cmake/Qt6ShaderToolsTools/Qt6ShaderToolsMacros.cmake",
        );
        fs::create_dir_all(macros.parent().unwrap()).unwrap();
        fs::write(macros, "# fixture shader-tools macros\n").unwrap();

        let host_root = qt_host_qsb(repo).unwrap();
        let host_qsb = host_root.join("bin/qsb");
        assert!(host_qsb.is_symlink());
        assert_eq!(fs::canonicalize(host_qsb).unwrap(), fs::canonicalize(target).unwrap());
        assert!(host_root.starts_with(repo.join("out/host-tools")));
        assert!(!host_root.starts_with(repo.join("out/build")));
    }

    #[test]
    fn qmake_metadata_publishes_declared_target_prefixes_not_build_paths() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        let modules = repo.join("out/build/qtbase/install/usr/mkspecs/modules");
        fs::create_dir_all(&modules).unwrap();
        fs::create_dir_all(repo.join("out/build/qtbase/install/usr/lib/x86_64-linux-gnu/cmake/Qt6")).unwrap();
        let glvnd = repo.join("out/build/libglvnd/install/usr/include");
        let wayland = repo.join("out/build/wayland/install/usr/include");
        let xrender = repo.join("out/build/libxrender/install/usr/include");
        fs::write(
            modules.join("qt_lib_gui_private.pri"),
            format!(
                "QMAKE_INCDIR_OPENGL = {}\nQMAKE_INCDIR_WAYLAND_CLIENT = {}\nQMAKE_INCDIR_XRENDER = {}\n",
                glvnd.display(),
                wayland.display(),
                xrender.display(),
            ),
        )
        .unwrap();
        normalize_qt_target_metadata(repo, &repo.join("out/build/qtbase/install")).unwrap();
        let metadata = fs::read_to_string(modules.join("qt_lib_gui_private.pri")).unwrap();
        assert!(metadata.contains("QMAKE_INCDIR_OPENGL = /usr/include"));
        assert!(metadata.contains("QMAKE_INCDIR_WAYLAND_CLIENT = /usr/include"));
        assert!(metadata.contains("QMAKE_INCDIR_XRENDER = /usr/include"));
        assert!(!metadata.contains("out/build"));
    }

    #[test]
    fn qmake_metadata_rejects_undeclared_build_prefixes() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        let modules = repo.join("out/build/qtbase/install/usr/mkspecs/modules");
        fs::create_dir_all(&modules).unwrap();
        fs::create_dir_all(repo.join("out/build/qtbase/install/usr/lib/x86_64-linux-gnu/cmake/Qt6")).unwrap();
        fs::write(
            modules.join("qt_lib_gui_private.pri"),
            format!("QMAKE_INCDIR_UNKNOWN = {}/out/build/undeclared/install/usr/include\n", repo.display()),
        )
        .unwrap();
        let error = normalize_qt_target_metadata(repo, &repo.join("out/build/qtbase/install"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("retains an unpublished build path"));
    }

    #[test]
    fn qmake_metadata_does_not_publish_qt_bundled_libjpeg_source() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        let modules = repo.join("out/build/qtbase/install/usr/mkspecs/modules");
        fs::create_dir_all(&modules).unwrap();
        fs::create_dir_all(repo.join("out/build/qtbase/install/usr/lib/x86_64-linux-gnu/cmake/Qt6")).unwrap();
        fs::write(
            modules.join("qt_ext_libjpeg.pri"),
            format!(
                "QMAKE_INCDIR_LIBJPEG = {}/out/build/qtbase/source/src/3rdparty/libjpeg/src\n",
                repo.display(),
            ),
        )
        .unwrap();
        normalize_qt_target_metadata(repo, &repo.join("out/build/qtbase/install")).unwrap();
        assert_eq!(
            fs::read_to_string(modules.join("qt_ext_libjpeg.pri")).unwrap(),
            "QMAKE_INCDIR_LIBJPEG = \n",
        );
    }

    #[test]
    fn cmake_build_internals_does_not_publish_qt_source_tree() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        let install = repo.join("out/build/qtbase/install");
        fs::create_dir_all(install.join("usr/mkspecs/modules")).unwrap();
        let cmake = install.join("usr/lib/x86_64-linux-gnu/cmake/Qt6BuildInternals");
        fs::create_dir_all(&cmake).unwrap();
        fs::write(
            cmake.join("QtBuildInternalsExtra.cmake"),
            format!(
                "set(QT_SOURCE_TREE \"{}/out/build/qtbase/source\" CACHE PATH \"source\")\n",
                repo.display(),
            ),
        )
        .unwrap();
        normalize_qt_target_metadata(repo, &install).unwrap();
        let metadata = fs::read_to_string(cmake.join("QtBuildInternalsExtra.cmake")).unwrap();
        assert!(metadata.contains("QT_SOURCE_TREE \"\""));
        assert!(!metadata.contains("out/build"));
    }
}
