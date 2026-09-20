// Source-owned Plasma session components.  This file intentionally keeps the
// four top-level products separate: KWin is the compositor, workspace owns
// the session shell, desktop owns plasmashell, and Breeze owns the default
// platform theme.  Their dependencies are declared in stage_graph.rs.

fn build_plasma_component(repo_root: &Path, stage: &str, source: &str, components: &[&str], options: &[&str], required_output: &str) -> Result<()> {
    if stage == "plasma-desktop" {
        // plasma-desktop's keyboard-layout backend queries xkb_base from the
        // output-owned xkeyboard-config pkg-config metadata at configure time.
        // This data component is not a standalone BuildStage, so materialize
        // its disposable install tree before constructing the KDE environment.
        build_xkeyboard_config(repo_root)?;
    }
    // polkit-qt's imported target retains GLib/polkit as private ELF
    // dependencies.  Keep those target-owned library directories available
    // to strict --no-undefined consumers without changing the upstream
    // package interface or searching the host.
    let mut build_components = components.to_vec();
    if stage == "plasma-workspace" {
        build_components.extend(["polkit", "glib"]);
    }
    if stage == "plasma-desktop" {
        build_components.extend(["kbookmarks", "kcompletion", "kitemviews", "kitemmodels", "kjobwidgets", "kservice", "kparts", "solid", "kirigami", "kded", "plasma-framework", "plasma-activities", "plasma-activities-stats", "plasma5support", "kwin", "ksysguard", "xorgproto", "libxcb", "libxau", "libxdmcp", "libx11", "x11-compat", "xkeyboard-config", "qtshadertools"]);
    }
    if stage == "kscreenlocker" {
        build_components.push("libkscreen");
    }
    let mut build_options = options.to_vec();
    if stage == "kscreenlocker" {
        build_options.push("-DWITH_X11=OFF");
        build_components.push("linux-pam");
        build_components.push("x11-compat");
        build_components.push("libxcb");
    }
    build_kde_cmake(repo_root, stage, source, &build_components, &build_options, required_output)
}

fn build_plasma_framework(repo_root: &Path) -> Result<()> {
    build_plasma_component(repo_root, "plasma-framework", "src/desktop/kde/plasma-framework", &["qtbase", "qtdeclarative", "qttools", "kconfig", "kcoreaddons", "ki18n", "kguiaddons", "kwidgetsaddons", "kiconthemes", "kirigami", "ksvg", "kpackage", "kglobalaccel", "kwindowsystem", "kwayland", "kio", "kbookmarks", "kcompletion", "solid", "kservice", "kcodecs", "kitemmodels", "kitemviews", "kjobwidgets", "karchive", "kauth", "kcrash", "kdbusaddons", "knotifications", "kcolorscheme", "plasma-wayland-protocols", "plasma-activities", "gzip", "x11-compat"], &["-DBUILD_TESTING=OFF", "-DWITHOUT_X11=ON"], "usr/lib/x86_64-linux-gnu/cmake/Plasma/PlasmaConfig.cmake")
}

fn build_qqc2_desktop_style(repo_root: &Path) -> Result<()> {
    build_kde_cmake(
        repo_root,
        "qqc2-desktop-style",
        "src/desktop/kde/qqc2-desktop-style",
        &["qtdeclarative", "kconfig", "kirigami", "kiconthemes", "kcolorscheme", "sonnet", "x11-compat"],
        &["-DBUILD_TESTING=OFF"],
        "usr/lib/x86_64-linux-gnu/cmake/KF6QQC2DesktopStyle/KF6QQC2DesktopStyleConfig.cmake",
    )
}

fn build_plasma_activities(repo_root: &Path) -> Result<()> {
    build_plasma_component(repo_root, "plasma-activities", "src/desktop/kde/plasma-activities", &["qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "kdbusaddons", "dbus"], &["-DBUILD_TESTING=OFF", "-DPLASMA_ACTIVITIES_LIBRARY_ONLY=OFF"], "usr/lib/x86_64-linux-gnu/cmake/PlasmaActivities/PlasmaActivitiesConfig.cmake")
}

fn build_kactivitymanagerd(repo_root: &Path) -> Result<()> {
    // kactivitymanagerd is a separate Plasma release component.  Its Boost
    // use is header-only; the runtime closure is the Qt/KF/D-Bus service
    // recorded here, while the normal host compiler supplies only the
    // build-time Boost headers.
    build_plasma_component(repo_root, "kactivitymanagerd", "src/desktop/kde/kactivitymanagerd", &["qtbase", "qtdeclarative", "qttools", "kconfig", "kconfigwidgets", "kcodecs", "kcoreaddons", "kdbusaddons", "ki18n", "kcrash", "kglobalaccel", "kxmlgui", "kio", "kwindowsystem", "kbookmarks", "kcompletion", "kitemviews", "kjobwidgets", "solid", "kwidgetsaddons", "kservice", "kguiaddons", "kiconthemes", "kcolorscheme", "util-linux"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libexec/kactivitymanagerd")
}

fn build_kglobalacceld(repo_root: &Path) -> Result<()> {
    build_plasma_component(repo_root, "kglobalacceld", "src/desktop/kde/kglobalacceld", &["qtbase", "qtdeclarative", "qttools", "kconfig", "kcoreaddons", "kcrash", "kdbusaddons", "kwindowsystem", "kglobalaccel", "kservice", "kio", "karchive", "kauth", "kbookmarks", "kcolorscheme", "kcompletion", "kguiaddons", "ki18n", "kiconthemes", "kitemmodels", "kitemviews", "kjobwidgets", "knotifications", "kwidgetsaddons", "solid", "util-linux", "xkbcommon", "libcanberra"], &["-DBUILD_TESTING=OFF", "-DWITH_X11=OFF"], "usr/lib/x86_64-linux-gnu/libexec/kglobalacceld")
}

fn build_plasma_activities_stats(repo_root: &Path) -> Result<()> {
    build_plasma_component(repo_root, "plasma-activities-stats", "src/desktop/kde/plasma-activities-stats", &["qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "plasma-activities"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/cmake/PlasmaActivitiesStats/PlasmaActivitiesStatsConfig.cmake")
}

fn build_plasma5support(repo_root: &Path) -> Result<()> {
    build_plasma_component(repo_root, "plasma5support", "src/desktop/kde/plasma5support", &["qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "kguiaddons", "ki18n", "knotifications", "solid", "kservice", "kidletime", "kio", "kunitconversion", "kwindowsystem", "kbookmarks", "kcompletion", "kitemviews", "kjobwidgets", "kwidgetsaddons", "kholidays", "ksysguard", "plasma-activities", "xkbcommon"], &["-DBUILD_TESTING=OFF", "-DBUILD_QCH=OFF", "-DWITH_X11=OFF"], "usr/lib/x86_64-linux-gnu/libPlasma5Support.so")
}

fn build_kscreen(repo_root: &Path) -> Result<()> {
    // ScreenDpms includes both Wayland and XCB backends in this upstream
    // release.  Building the XCB helper does not add an X11 Plasma session;
    // it supplies the complete library ABI consumed by PowerDevil.
    build_plasma_component(repo_root, "libkscreen", "src/desktop/kde/libkscreen", &["qtbase", "qtdeclarative", "qtwayland", "kconfig", "kcoreaddons", "ki18n", "kwidgetsaddons", "kwayland", "plasma-wayland-protocols", "wayland", "libffi", "libdrm", "x11-compat"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/cmake/KF6Screen/KF6ScreenConfig.cmake")
}

fn build_layer_shell_qt(repo_root: &Path) -> Result<()> {
    build_plasma_component(repo_root, "layer-shell-qt", "src/desktop/kde/layer-shell-qt", &["qtbase", "qtwayland", "qtdeclarative", "kwayland", "plasma-wayland-protocols", "wayland", "xkbcommon", "libffi"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/cmake/LayerShellQt/LayerShellQtConfig.cmake")
}

fn build_kscreen_locker(repo_root: &Path) -> Result<()> {
    build_plasma_component(repo_root, "kscreenlocker", "src/desktop/kde/kscreenlocker", &["qtbase", "qtdeclarative", "kconfig", "kconfigwidgets", "kcolorscheme", "kcoreaddons", "kcrash", "kdbusaddons", "ki18n", "kpackage", "kcmutils", "kio", "kbookmarks", "kwidgetsaddons", "kwindowsystem", "kglobalaccel", "kidletime", "knotifications", "solid", "kxmlgui", "ksvg", "kguiaddons", "kcodecs", "karchive", "kcompletion", "kitemviews", "kitemmodels", "kjobwidgets", "kservice", "plasma-framework", "kirigami", "layer-shell-qt"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/cmake/ScreenSaverDBusInterface/ScreenSaverDBusInterfaceConfig.cmake")
}

fn build_ksysguard(repo_root: &Path) -> Result<()> {
    build_plasma_component(repo_root, "ksysguard", "src/desktop/kde/ksysguard", &["qtbase", "qtdeclarative", "kcoreaddons", "kconfig", "ki18n", "kdbusaddons", "kio", "kpackage", "kconfigwidgets", "kglobalaccel", "kiconthemes", "kwidgetsaddons", "kxmlgui", "kservice", "kitemmodels", "kitemviews", "knotifications", "kjobwidgets", "kauth", "knewstuff", "solid", "attica", "zlib", "libdrm", "libcap", "procps", "libnl", "lm-sensors"], &["-DBUILD_TESTING=OFF", "-DWITH_X11=OFF"], "usr/lib/x86_64-linux-gnu/cmake/KSysGuard/KSysGuardConfig.cmake")
}

fn build_kwin(repo_root: &Path) -> Result<()> {
    build_plasma_component(
        repo_root,
        "kwin",
        "src/desktop/kde/kwin",
        &["qtbase", "qt5compat", "qtwayland", "qtdeclarative", "qttools", "kconfig", "kcoreaddons", "kdbusaddons", "kauth", "karchive", "kcolorscheme", "kcrash", "kglobalaccel", "kguiaddons", "ki18n", "kidletime", "kpackage", "kservice", "ksvg", "kwidgetsaddons", "kwindowsystem", "kdecoration", "kwayland", "knighttime", "kholidays", "qtpositioning", "lcms2", "x11-compat", "libxcb", "libffi", "libevdev", "plasma-wayland-protocols", "wayland-protocols", "wayland", "xkbcommon", "libdrm", "mesa", "libinput", "libdisplay-info", "seatd", "libepoxy", "libxcvt", "libcanberra", "systemd", "dbus", "xwayland"],
        // The installed Wayland wrapper and Plasma session deliberately pass
        // --xwayland to KWin.  Keep the X11 common code/Xwayland server
        // enabled so that legacy X11 clients work inside the Wayland session;
        // disabling this option leaves an installed wrapper that KWin rejects
        // immediately with "Unknown option 'xwayland'".
        &["-DBUILD_TESTING=OFF", "-DKWIN_BUILD_X11=ON", "-DKWIN_BUILD_KCMS=OFF", "-DKWIN_BUILD_NOTIFICATIONS=OFF", "-DKWIN_BUILD_SCREENLOCKER=OFF", "-DKWIN_BUILD_TABBOX=OFF", "-DKWIN_BUILD_GLOBALSHORTCUTS=OFF", "-DKWIN_BUILD_RUNNERS=OFF", "-DKWIN_BUILD_DECORATIONS=OFF", "-DWITH_PIPEWIRE=ON"],
        "usr/bin/kwin_wayland",
    )
}

fn build_plasma_workspace(repo_root: &Path) -> Result<()> {
    build_plasma_component(
        repo_root,
        "plasma-workspace",
        "src/desktop/kde/plasma-workspace",
        &["qtbase", "qtdeclarative", "qtshadertools", "qtpositioning", "qtlocation", "qcoro", "kconfig", "kcoreaddons", "kdbusaddons", "kauth", "karchive", "kcrash", "kglobalaccel", "kguiaddons", "ki18n", "kholidays", "kidletime", "kpackage", "ksvg", "kcolorscheme", "kwidgetsaddons", "kdeclarative", "kirigami", "kirigami-addons", "kquickcharts", "kiconthemes", "kitemmodels", "kitemviews", "kcmutils", "kded", "kio", "kwindowsystem", "kwayland", "libkscreen", "layer-shell-qt", "knighttime", "plasma-wayland-protocols", "xkbcommon", "kbookmarks", "kcompletion", "solid", "kservice", "kcodecs", "knewstuff", "attica", "knotifications", "kparts", "kjobwidgets", "prison", "krunner", "kstatusnotifieritem", "ktextwidgets", "sonnet", "ktexteditor", "kwallet", "kconfigwidgets", "kxmlgui", "breeze-icons", "libffi", "libcanberra", "zlib", "icu", "polkit-qt-1", "plasma-framework", "plasma-activities", "plasma-activities-stats", "kwin", "systemd", "dbus", "networkmanager", "pipewire", "x11-compat"],
        &["-DBUILD_TESTING=OFF", "-DWITH_X11=OFF", "-DWITH_X11_SESSION=OFF", "-DWITH_WAYLAND=ON", "-DBUILD_CAMERAINDICATOR=OFF"],
        "usr/bin/startplasma-wayland",
    )
}

fn build_plasma_desktop(repo_root: &Path) -> Result<()> {
    build_plasma_component(
        repo_root,
        "plasma-desktop",
        "src/desktop/kde/plasma-desktop",
        &["qtbase", "qtdeclarative", "kconfig", "kconfigwidgets", "kcolorscheme", "kwindowsystem", "kcoreaddons", "ki18n", "kwidgetsaddons", "kauth", "kcrash", "kcmutils", "knewstuff", "kio", "knotifications", "attica", "krunner", "kglobalaccel", "kguiaddons", "kdbusaddons", "kcodecs", "sonnet", "kpackage", "kiconthemes", "kxmlgui", "ksvg", "kbookmarks", "kcompletion", "kjobwidgets", "kservice", "kparts", "solid", "qqc2-desktop-style", "kirigami-addons", "plasma-workspace", "xkbcommon", "libxml2", "libxkbfile", "xorgproto", "libxcb", "libxau", "libxdmcp", "libx11", "x11-compat", "xkeyboard-config", "qtshadertools"],
        // Plasma is a Wayland-only session, but plasma-desktop still builds its
        // keyboard-layout applet's private QML backend behind WITH_X11.  Keep
        // that XKB/XCB integration enabled for Xwayland/keyboard management;
        // the individual X11 mouse/touchpad backends and any X11 session stay
        // disabled explicitly.
        &["-DBUILD_TESTING=OFF", "-DBUILD_DOC=OFF", "-DWITH_X11=ON", "-DBUILD_KCM_TABLET=OFF", "-DBUILD_KCM_MOUSE_X11=OFF", "-DBUILD_KCM_TOUCHPAD_X11=OFF"],
        // plasmashell is owned by plasma-workspace; this stage's own
        // executable provides a stable output proof without claiming it.
        "usr/bin/plasma-emojier",
    )?;
    let keyboard_qml = repo_root.join(
        "out/build/plasma-desktop/install/usr/lib/x86_64-linux-gnu/qml/org/kde/plasma/private/kcm_keyboard/qmldir",
    );
    if !keyboard_qml.is_file() {
        bail!(
            "plasma-desktop completed without the keyboard applet's required QML module {}",
            keyboard_qml.display()
        );
    }
    Ok(())
}

fn build_breeze(repo_root: &Path) -> Result<()> {
    build_plasma_component(
        repo_root,
        "breeze",
        "src/desktop/kde/breeze",
        &["qtbase", "qtdeclarative", "kconfig", "kconfigwidgets", "kwidgetsaddons", "kitemviews", "kitemmodels", "kcompletion", "kcoreaddons", "ki18n", "kcolorscheme", "kguiaddons", "kiconthemes", "kwindowsystem", "kirigami", "kdecoration", "kcmutils", "kxmlgui", "kglobalaccel", "karchive", "breeze-icons", "libffi", "plasma-desktop"],
        &["-DBUILD_TESTING=OFF", "-DBUILD_QT5=OFF", "-DBUILD_QT6=ON", "-DWITH_X11=OFF"],
        "usr/lib/x86_64-linux-gnu/plugins/styles/breeze6.so",
    )
}
