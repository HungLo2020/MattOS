// Source-owned Calamares build used by the graphical MattOS installer.
//
// Calamares is built as a Qt6 Widgets application with its upstream storage,
// locale, keyboard, users, summary and execution modules. QML, Python job
// modules and optional package-manager modules are deliberately disabled: the
// target package/profile authority remains the Rust installer policy shared
// by both installation frontends.

fn build_calamares(repo_root: &Path) -> Result<()> {
    let kpmcore = repo_root.join("out/build/kpmcore/install/usr");
    let yaml = repo_root.join("out/build/yaml-cpp/install/usr");
    let mut options = vec![
        "-DWITH_QT6=ON".to_string(),
        "-DWITH_QML=OFF".to_string(),
        "-DWITH_PYTHON=ON".to_string(),
        "-DWITH_PYBIND11=ON".to_string(),
        "-DBUILD_SCHEMA_TESTING=OFF".to_string(),
        "-DBUILD_CRASH_REPORTING=OFF".to_string(),
        "-DINSTALL_POLKIT=ON".to_string(),
        "-DINSTALL_CONFIG=OFF".to_string(),
        "-DINSTALL_COMPLETION=OFF".to_string(),
        "-DCMAKE_INSTALL_PREFIX=/usr".to_string(),
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu".to_string(),
        "-DCMAKE_BUILD_TYPE=Release".to_string(),
        format!("-DKPMcore_DIR={}/lib/x86_64-linux-gnu/cmake/KPMcore", kpmcore.display()),
        format!("-Dyaml-cpp_DIR={}/lib/x86_64-linux-gnu/cmake/yaml-cpp", yaml.display()),
    ];
    // The upstream CMake project is intentionally configured from the
    // output-owned source mirror by the generic CMake helper.  Keep all
    // discovery prefixes and linker inputs target-owned.
    build_cmake_component(
        repo_root,
        "calamares",
        "src/system/installer/calamares/upstream",
        &[
            "qtbase",
            "qtsvg",
            "qtwayland",
            "qtdeclarative",
            "kcoreaddons",
            "ki18n",
            "kwidgetsaddons",
            "polkit-qt-1",
            "yaml-cpp",
            "kpmcore",
            "polkit",
            "glib",
            "dbus",
            "systemd",
            "pcre2",
            "zlib",
            "libffi",
            "libxcrypt",
            "util-linux",
            "btrfs-progs",
            "cpython",
        ],
        &options.iter().map(String::as_str).collect::<Vec<_>>(),
        "usr/bin/calamares",
        true,
    )?;
    options.clear();
    Ok(())
}

#[cfg(test)]
mod calamares_tests {
    #[test]
    fn production_build_disables_unowned_optional_apis() {
        let source = include_str!("calamares.rs");
        assert!(source.contains("-DWITH_QML=OFF"));
        assert!(source.contains("-DWITH_PYTHON=OFF"));
        assert!(source.contains("-DBUILD_SCHEMA_TESTING=OFF"));
        assert!(source.contains("-DKPMcore_DIR="));
    }
}
