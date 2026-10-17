// First-class desktop integration components; build only in output-owned mirrors.

fn build_plasma_integration(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "plasma-integration",
        "src/desktop/kde/plasma-integration",
        &[
            "qtbase",
            "qtdeclarative",
            "qtsvg",
            "kconfig",
            "kcoreaddons",
            "kcodecs",
            "ki18n",
            "kconfigwidgets",
            "kcolorscheme",
            "kiconthemes",
            "kio",
            "kservice",
            "kwidgetsaddons",
            "kwindowsystem",
            "knotifications",
            "kguiaddons",
            "kxmlgui",
            "kjobwidgets",
            "kcompletion",
            "kbookmarks",
            "kitemviews",
            "kcrash",
            "solid",
            "karchive",
            "kdbusaddons",
            "libffi",
            "kglobalaccel",
            "breeze-icons",
            "libcanberra",
            "qtwayland",
            "plasma-wayland-protocols",
            "wayland",
            "x11-compat",
            "kstatusnotifieritem",
            "fontconfig",
            "freetype",
        ],
        &[
            "-DBUILD_TESTING=OFF",
            "-DBUILD_QCH=OFF",
            "-DBUILD_QT5=OFF",
            "-DBUILD_QT6=ON",
            "-DCMAKE_DISABLE_FIND_PACKAGE_FontNotoSans=ON",
            "-DCMAKE_DISABLE_FIND_PACKAGE_FontHack=ON",
            "-DCMAKE_DISABLE_FIND_PACKAGE_FontNotoColorEmoji=ON",
            "-DCMAKE_DISABLE_FIND_PACKAGE_XDGDesktopPortalKDE=ON",
        ],
        "usr/lib/x86_64-linux-gnu/plugins/platformthemes/KDEPlasmaPlatformTheme6.so",
    )
}

fn build_bluez_qt(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "bluez-qt",
        "src/desktop/kde/bluez-qt",
        &["qtbase", "qtdeclarative"],
        &["-DBUILD_TESTING=OFF", "-DBUILD_QCH=OFF"],
        "usr/lib/x86_64-linux-gnu/libKF6BluezQt.so",
    )
}

fn build_bluedevil(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "bluedevil",
        "src/desktop/kde/bluedevil",
        &[
            "qtbase",
            "qtdeclarative",
            "qtsvg",
            "kconfig",
            "kcoreaddons",
            "kcodecs",
            "ki18n",
            "kconfigwidgets",
            "kcolorscheme",
            "kiconthemes",
            "kio",
            "kservice",
            "kwidgetsaddons",
            "kwindowsystem",
            "knotifications",
            "kguiaddons",
            "kxmlgui",
            "kjobwidgets",
            "kcompletion",
            "kbookmarks",
            "kitemviews",
            "kcrash",
            "solid",
            "karchive",
            "kdbusaddons",
            "libffi",
            "kglobalaccel",
            "breeze-icons",
            "libcanberra",
            "bluez-qt",
            "plasma-framework",
            "ksvg",
            "shared-mime-info",
            "kcmutils",
            "kdeclarative",
            "kirigami",
            "kpackage",
            "knewstuff",
            "attica",
        ],
        &["-DBUILD_TESTING=OFF", "-DBUILD_QCH=OFF"],
        "usr/bin/bluedevil-wizard",
    )
}

fn build_kwallet_pam(repo_root: &Path) -> Result<()> {
    let gpg_headers = format!(
        "-DCMAKE_C_FLAGS=-I{}",
        repo_root
            .join("out/build/libgpg-error/install/usr/include")
            .display()
    );
    build_kde_cmake(
        repo_root,
        "kwallet-pam",
        "src/desktop/kde/kwallet-pam",
        &[
            "qtbase",
            "linux-pam",
            "libgcrypt",
            "libgpg-error",
            "kwallet",
            "systemd",
        ],
        &[
            "-DBUILD_TESTING=OFF",
            "-DBUILD_QCH=OFF",
            "-DKWALLETD_BIN_PATH=/usr/bin/ksecretd",
            &gpg_headers,
        ],
        "usr/lib/x86_64-linux-gnu/security/pam_kwallet5.so",
    )
}

fn build_xdg_utils(repo_root: &Path) -> Result<()> {
    let root = repo_root.join("out/build/xdg-utils");
    let source = root.join("source");
    let install = root.join("install");
    sync_build_source(&repo_root.join("src/desktop/xdg-utils"), &source)?;
    // Upstream generates its shell programs, including help text, from the
    // retained .in and DocBook sources. Nothing is generated in the import.
    run_cmd(&source, "autoconf", &[])?;
    run_cmd(&source, "bash", &["./configure", "--prefix=/usr"])?;
    run_cmd(&source.join("scripts"), "make", &["scripts", "man"])?;
    remove_path_if_exists(&install)?;
    run_cmd(
        &source.join("scripts"),
        "make",
        &["install", &format!("DESTDIR={}", install.display())],
    )?;
    Ok(())
}

fn build_noto_color_emoji(repo_root: &Path) -> Result<()> {
    let install = repo_root.join("out/build/noto-color-emoji/install");
    remove_path_if_exists(&install)?;
    let source = repo_root.join("src/desktop/fonts/noto-color-emoji");
    stage_output_file(
        &source.join("2D/fonts/Noto-COLRv1.ttf"),
        &install.join("usr/share/fonts/truetype/noto/NotoColorEmoji-COLRv1.ttf"),
        0o644,
    )?;
    stage_output_file(
        &source.join("2D/fonts/LICENSE"),
        &install.join("usr/share/doc/fonts-noto-color-emoji/copyright"),
        0o644,
    )
}

fn build_noto_sans_cjk(repo_root: &Path) -> Result<()> {
    let install = repo_root.join("out/build/noto-sans-cjk/install");
    remove_path_if_exists(&install)?;
    for family in ["sc", "jp", "kr", "tc", "hk"] {
        let source = repo_root.join(format!(
            "src/desktop/fonts/noto-sans-cjk/ofl/notosans{family}"
        ));
        for entry in fs::read_dir(&source)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("ttf") {
                stage_output_file(
                    &path,
                    &install
                        .join("usr/share/fonts/truetype/noto")
                        .join(path.file_name().unwrap()),
                    0o644,
                )?;
            }
        }
        stage_output_file(
            &source.join("OFL.txt"),
            &install.join(format!("usr/share/doc/fonts-noto-cjk/OFL-{family}.txt")),
            0o644,
        )?;
    }
    Ok(())
}

fn build_libsecret(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libsecret",
        "src/system/security/libsecret",
        &[
            "glib",
            "libgcrypt",
            "libgpg-error",
            "libffi",
            "pcre2",
            "zlib",
        ],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Dgtk_doc=false",
            "-Dmanpage=false",
            "-Dintrospection=false",
            "-Dvapi=false",
            "-Dtest_setup=disabled",
            "-Dbash_completion=disabled",
            "-Dcrypto=libgcrypt",
        ],
        "usr/lib/x86_64-linux-gnu/libsecret-1.so",
        &[],
    )
}

fn build_kde_cli_tools(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "kde-cli-tools",
        "src/desktop/kde/kde-cli-tools",
        &[
            "qtbase",
            "qtdeclarative",
            "qtsvg",
            "kconfig",
            "kcoreaddons",
            "kcodecs",
            "ki18n",
            "kconfigwidgets",
            "kcolorscheme",
            "kiconthemes",
            "kio",
            "kservice",
            "kwidgetsaddons",
            "kwindowsystem",
            "knotifications",
            "kguiaddons",
            "kxmlgui",
            "kjobwidgets",
            "kcompletion",
            "kbookmarks",
            "kitemviews",
            "kcrash",
            "solid",
            "karchive",
            "kdbusaddons",
            "libffi",
            "kglobalaccel",
            "breeze-icons",
            "libcanberra",
            "kparts",
            "kcmutils",
        ],
        &[
            "-DBUILD_TESTING=OFF",
            "-DBUILD_DOC=OFF",
            "-DWITH_X11=OFF",
            "-DCMAKE_DISABLE_FIND_PACKAGE_KF6Su=ON",
        ],
        "usr/bin/kde-open",
    )
}
