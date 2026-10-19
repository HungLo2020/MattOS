// Required native dependencies introduced by the Plasma 6.8 stack.
// Corrosion is build-only CMake glue; no host library enters the target.
fn kde_host_corrosion_dir(repo_root: &Path) -> Result<PathBuf> {
    const COMMIT: &str = "1499b14e4906a2890f5cee1547c8848db261753d";
    let tools = repo_root.join("out/host-tools");
    let source = tools.join("corrosion-0.6.1-source");
    let build = tools.join("corrosion-0.6.1-build");
    let install = tools.join("corrosion-0.6.1");
    fs::create_dir_all(&tools)?;
    if !source.join(".git").is_dir() {
        run_cmd(&tools, "git", &["clone", "--no-checkout", "--depth=1", "--branch", "v0.6.1",
            "https://github.com/corrosion-rs/corrosion.git", path_str(&source)?])?;
    }
    let current = run_cmd_capture(&source, "git", &["rev-parse", "HEAD"])?;
    if current.trim() != COMMIT {
        run_cmd(&source, "git", &["fetch", "--depth=1", "origin", COMMIT])?;
    }
    // A fresh --no-checkout clone can already have the pinned HEAD while
    // its working tree is empty. Materialize it in both cases.
    run_cmd(&source, "git", &["checkout", "--detach", COMMIT])?;
    if run_cmd_capture(&source, "git", &["rev-parse", "HEAD"])?.trim() != COMMIT {
        bail!("Corrosion build-only source does not match pinned commit {COMMIT}");
    }
    let config = install.join("lib/cmake/Corrosion");
    if !config.join("CorrosionConfig.cmake").is_file() {
        run_cmd(&tools, "cmake", &["-S", path_str(&source)?, "-B", path_str(&build)?, "-G", "Ninja",
            "-DCORROSION_INSTALL_ONLY=ON", "-DCORROSION_BUILD_TESTS=OFF", "-DCMAKE_INSTALL_LIBDIR=lib",
            &format!("-DCMAKE_INSTALL_PREFIX={}", install.display())])?;
        run_cmd(&tools, "cmake", &["--install", path_str(&build)?])?;
    }
    if !config.join("CorrosionConfig.cmake").is_file() {
        bail!("Corrosion bootstrap did not install its build-only CMake package");
    }
    Ok(config)
}

fn build_cxx_rust_cssparser(repo_root: &Path) -> Result<()> {
    let corrosion = kde_host_corrosion_dir(repo_root)?;
    let rust = repo_root.join(MATTOS_RUST_TOOLS);
    build_non_qt_cmake(repo_root, "cxx-rust-cssparser", "src/system/libraries/cxx-rust-cssparser",
        &[], &[
            "-DBUILD_SHARED_LIBS=ON", "-DBUILD_DOCS=OFF", "-DBUILD_TESTING=OFF",
            &format!("-DCorrosion_DIR={}", corrosion.display()),
            &format!("-DRust_COMPILER={}", rust.join("rustc").display()),
            &format!("-DRust_CARGO={}", rust.join("cargo").display()),
        ], "usr/lib/x86_64-linux-gnu/libcxx-rust-cssparser.so")
}

fn build_union(repo_root: &Path) -> Result<()> {
    build_kde_cmake(repo_root, "union", "src/desktop/kde/union",
        &["qtbase", "qtdeclarative", "qtshadertools", "kcoreaddons", "kguiaddons", "kconfig",
          "kiconthemes", "kcolorscheme", "kirigami", "cxx-rust-cssparser"],
        &["-DBUILD_SHARED_LIBS=ON", "-DBUILD_TESTING=OFF", "-DBUILD_TOOLS=OFF", "-DBUILD_EXAMPLES=OFF"],
        "usr/lib/x86_64-linux-gnu/libUnion.so")
}

fn build_qtkeychain(repo_root: &Path) -> Result<()> {
    // The existing KWallet service provides secure D-Bus storage. No plaintext
    // fallback is enabled; the optional GNOME/libsecret backend is not built.
    build_kde_cmake(repo_root, "qtkeychain", "src/system/libraries/qtkeychain",
        &["qtbase"], &["-DBUILD_WITH_QT5=OFF", "-DBUILD_SHARED_LIBS=ON", "-DBUILD_TESTING=OFF",
          "-DBUILD_TEST_APPLICATION=OFF", "-DBUILD_QTQUICK_DEMO=OFF", "-DLIBSECRET_SUPPORT=OFF"],
        "usr/lib/x86_64-linux-gnu/libqt6keychain.so")
}

fn build_leptonica(repo_root: &Path) -> Result<()> {
    // Spectacle supplies pixels directly. PNG/JPEG and BMP/PNM are sufficient
    // for the native OCR command; unsupported codecs cannot fall back to host.
    build_non_qt_cmake(repo_root, "leptonica", "src/system/libraries/leptonica",
        &["zlib", "libpng", "libjpeg-turbo"],
        &["-DBUILD_SHARED_LIBS=ON", "-DBUILD_PROG=OFF", "-DSW_BUILD=OFF", "-DENABLE_ZLIB=ON",
          "-DENABLE_PNG=ON", "-DENABLE_JPEG=ON", "-DENABLE_GIF=OFF", "-DENABLE_TIFF=OFF",
          "-DENABLE_WEBP=OFF", "-DENABLE_OPENJPEG=OFF"],
        "usr/lib/x86_64-linux-gnu/libleptonica.so")
}

fn build_tesseract(repo_root: &Path) -> Result<()> {
    build_non_qt_cmake(repo_root, "tesseract", "src/system/libraries/tesseract",
        &["leptonica", "zlib", "libpng", "libjpeg-turbo"],
        &["-DBUILD_SHARED_LIBS=ON", "-DBUILD_TRAINING_TOOLS=OFF", "-DBUILD_TESTS=OFF",
          "-DOPENMP_BUILD=OFF", "-DENABLE_NATIVE=OFF", "-DGRAPHICS_DISABLED=ON",
          "-DDISABLE_TIFF=ON", "-DDISABLE_ARCHIVE=ON", "-DDISABLE_CURL=ON",
          "-DENABLE_NINJA_POOL=OFF", "-DCMAKE_INSTALL_DATAROOTDIR=share", "-DTESSDATA_PREFIX=/usr/share"],
        "usr/lib/x86_64-linux-gnu/libtesseract.so")
}

fn build_tessdata_fast(repo_root: &Path) -> Result<()> {
    let install = repo_root.join("out/build/tessdata-fast/install");
    remove_path_if_exists(&install)?;
    for file in ["eng.traineddata", "osd.traineddata"] {
        stage_output_file(&repo_root.join("src/system/libraries/tessdata-fast").join(file),
            &install.join("usr/share/tessdata").join(file), 0o644)?;
    }
    Ok(())
}
