//! Development-package payloads and the path normalization that keeps
//! pkg-config, CMake and Rust metadata free of build-machine paths.

use super::*;

/// Stage only libglvnd's development interface. Runtime packages retain
/// ownership of every SONAME link and real shared object; downstream CMake,
/// qmake and pkg-config consumers need these unversioned linker symlinks.
pub(super) fn stage_libglvnd_dev(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "libglvnd").join("usr");
    copy_tree_preserving(&install.join("include"), &staging.join("usr/include"))?;
    let libdir = "lib/x86_64-linux-gnu";
    for relative in [
        "libGL.so",
        "libGLX.so",
        "libOpenGL.so",
        "libEGL.so",
        "libGLESv1_CM.so",
        "libGLESv2.so",
        "libGLdispatch.so",
    ] {
        copy_path_preserving(
            &install.join(libdir).join(relative),
            &staging.join("usr").join(libdir).join(relative),
        )?;
    }
    copy_tree_preserving(
        &install.join(libdir).join("pkgconfig"),
        &staging.join("usr").join(libdir).join("pkgconfig"),
    )?;
    let cmake = install.join(libdir).join("cmake");
    if cmake.is_dir() {
        copy_tree_preserving(&cmake, &staging.join("usr").join(libdir).join("cmake"))?;
    }
    copy_preserving(
        &repo_root.join("src/system/graphics/libglvnd/README.md"),
        &staging.join("usr/share/doc/libglvnd-dev/copyright"),
    )
}

/// Build-time locations whose installed equivalents are fixed: the MattOS
/// sysroot is the target root, and the MattOS compiler wrappers correspond to
/// the native compiler in /usr/bin.  CMake/pkg-config discovery records these
/// absolute paths (e.g. `find_library(m)` resolving libm in the sysroot).
pub(super) fn target_equivalent_build_paths(repo_root: &Path) -> [(String, &'static str); 3] {
    [
        // A build-time `--sysroot` recorded into Libs/Cflags is meaningless
        // on the target, where the sysroot is `/`.
        (
            format!("--sysroot={}", repo_root.join("out/sysroot").display()),
            "",
        ),
        (format!("{}/", repo_root.join("out/sysroot").display()), "/"),
        (
            repo_root
                .join(TARGET_TOOL_WRAPPERS)
                .to_string_lossy()
                .into_owned(),
            "/usr/bin",
        ),
    ]
}

pub(super) fn normalize_staged_pkgconfig_paths(repo_root: &Path, staging: &Path) -> Result<()> {
    let build_root = repo_root.join("out/build");
    let build_root_text = build_root.to_string_lossy().into_owned();
    let mut install_prefixes = Vec::new();
    if let Ok(entries) = fs::read_dir(&build_root) {
        for entry in entries {
            let path = entry?.path().join("install/usr");
            if path.is_dir() {
                install_prefixes.push(path.to_string_lossy().into_owned());
            }
        }
    }
    walk_tree(staging, &mut |path, metadata| {
        if !metadata.is_file() || path.extension().and_then(OsStr::to_str) != Some("pc") {
            return Ok(());
        }
        let original = fs::read_to_string(path)?;
        let normalized = install_prefixes
            .iter()
            .fold(original.clone(), |contents, prefix| {
                contents.replace(prefix, "/usr")
            });
        let normalized = target_equivalent_build_paths(repo_root)
            .iter()
            .fold(normalized, |contents, (from, to)| {
                contents.replace(from, to)
            });
        if normalized.contains(&build_root_text)
            || normalized.contains(repo_root.to_string_lossy().as_ref())
        {
            bail!(
                "package pkg-config metadata /{} embeds the host build root",
                path.strip_prefix(staging)?.display()
            );
        }
        if normalized != original {
            fs::write(path, normalized)?;
        }
        Ok(())
    })
}

pub(super) fn normalize_staged_cmake_paths(repo_root: &Path, staging: &Path) -> Result<()> {
    let build_root = repo_root.join("out/build");
    let build_root_text = build_root.to_string_lossy().into_owned();
    let repo_root_text = repo_root.to_string_lossy().into_owned();
    let llvm_build = build_root.join("llvm/build").to_string_lossy().into_owned();
    let cmake_root = staging.join("usr/lib/x86_64-linux-gnu/cmake");
    if !cmake_root.is_dir() {
        return Ok(());
    }

    let mut install_prefixes = Vec::new();
    if let Ok(entries) = fs::read_dir(&build_root) {
        for entry in entries {
            let path = entry?.path().join("install/usr");
            if path.is_dir() {
                install_prefixes.push(path.to_string_lossy().into_owned());
            }
        }
    }
    walk_tree(staging, &mut |path, metadata| {
        if !metadata.is_file() || !path.starts_with(&cmake_root) {
            return Ok(());
        }
        let original = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let mut normalized = install_prefixes
            .iter()
            .fold(original.clone(), |contents, prefix| {
                contents.replace(prefix, "/usr")
            });
        normalized = target_equivalent_build_paths(repo_root)
            .iter()
            .fold(normalized, |contents, (from, to)| {
                contents.replace(from, to)
            });
        // LLVM exposes this build-tree helper in LLVMConfig.cmake. It is a
        // development-time convenience, but publishing the disposable host
        // path would make the target package non-reproducible and unusable.
        for suffix in ["/./bin/llvm-lit", "/bin/llvm-lit"] {
            normalized = normalized.replace(&format!("{llvm_build}{suffix}"), "/usr/bin/llvm-lit");
        }
        if normalized.contains(&build_root_text) || normalized.contains(&repo_root_text) {
            bail!(
                "package CMake metadata /{} embeds the host build root",
                path.strip_prefix(staging)?.display()
            );
        }
        if normalized != original {
            fs::write(path, normalized)?;
        }
        Ok(())
    })
}

pub(super) fn normalize_staged_rust_paths(repo_root: &Path, staging: &Path) -> Result<()> {
    let build_root = repo_root.join("out/build");
    let build_root_text = build_root.to_string_lossy().into_owned();
    let repo_root_text = repo_root.to_string_lossy().into_owned();
    let install_prefix = component_install(repo_root, "rust")
        .to_string_lossy()
        .into_owned();
    let rustlib_root = staging.join("usr/lib/rustlib");
    if !rustlib_root.is_dir() {
        return Ok(());
    }
    walk_tree(staging, &mut |path, metadata| {
        if !metadata.is_file() || !path.starts_with(&rustlib_root) {
            return Ok(());
        }
        let original = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        // Rust's installer manifests use `file:<install-prefix>/usr/...`.
        // Replace the install directory itself, rather than only relying on
        // the `/usr`-suffixed spelling, so both manifest and installer
        // generated forms normalize to the target filesystem view.
        let normalized = original.replace(&install_prefix, "");
        if normalized.contains(&build_root_text) || normalized.contains(&repo_root_text) {
            bail!(
                "package Rust metadata /{} embeds the host build root",
                path.strip_prefix(staging)?.display()
            );
        }
        if normalized != original {
            fs::write(path, normalized)?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod cmake_package_metadata_tests {
    use super::*;

    #[test]
    fn normalizes_llvm_build_tree_helper_without_mutating_install_output() {
        let fixture = tempfile::tempdir().unwrap();
        let repo_root = fixture.path();
        let staging = repo_root.join("staging");
        let cmake = staging.join("usr/lib/x86_64-linux-gnu/cmake/llvm");
        fs::create_dir_all(&cmake).unwrap();
        fs::create_dir_all(repo_root.join("out/build/llvm/install/usr")).unwrap();
        let config = cmake.join("LLVMConfig.cmake");
        let root = repo_root.to_string_lossy();
        fs::write(
            &config,
            format!(
                "set(LLVM_DEFAULT_EXTERNAL_LIT \"{root}/out/build/llvm/build/./bin/llvm-lit\")\n"
            ),
        )
        .unwrap();

        normalize_staged_cmake_paths(repo_root, &staging).unwrap();

        let contents = fs::read_to_string(config).unwrap();
        assert!(contents.contains("/usr/bin/llvm-lit"));
        assert!(!contents.contains(root.as_ref()));
    }
}

#[cfg(test)]
mod rust_package_metadata_tests {
    use super::*;

    #[test]
    fn normalizes_rust_manifests_and_excludes_install_log() {
        let fixture = tempfile::tempdir().unwrap();
        let repo_root = fixture.path();
        let staging = repo_root.join("staging");
        let rustlib = staging.join("usr/lib/rustlib");
        fs::create_dir_all(&rustlib).unwrap();
        fs::create_dir_all(repo_root.join("out/build/rust/install/usr")).unwrap();
        let root = repo_root.to_string_lossy();
        fs::write(
            rustlib.join("manifest-rustc"),
            format!("file:{root}/out/build/rust/install/usr/bin/rustc\n"),
        )
        .unwrap();
        fs::write(
            rustlib.join("install.log"),
            format!("install: creating {root}/out/build/rust/install/usr/bin/rustc\n"),
        )
        .unwrap();

        normalize_staged_rust_paths(repo_root, &staging).unwrap();

        let manifest = fs::read_to_string(rustlib.join("manifest-rustc")).unwrap();
        assert_eq!(manifest, "file:/usr/bin/rustc\n");
        assert!(rustlib.join("install.log").is_file());
        assert!(!manifest.contains(root.as_ref()));
    }
}

pub(super) fn stage_multimedia_sdk(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    package: &str,
    license: &str,
) -> Result<()> {
    let install = component_install(repo_root, component).join("usr");
    if !install.is_dir() {
        bail!("package {package} requires staged {component} output");
    }
    copy_tree_preserving(&install, &staging.join("usr"))?;
    // GNU install-info maintains this aggregate index on the installed
    // system.  It is not owned by any individual binary package and would
    // otherwise collide whenever two source builds install Info manuals.
    remove_path_if_exists(&staging.join("usr/share/info/dir"))?;
    copy_preserving(
        &repo_root.join(license),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("copyright"),
    )?;
    #[cfg(unix)]
    if component == "wireplumber" {
        let wants = staging.join("usr/lib/systemd/user/pipewire.service.wants");
        fs::create_dir_all(&wants)?;
        std::os::unix::fs::symlink("../wireplumber.service", wants.join("wireplumber.service"))?;
    }
    #[cfg(unix)]
    if component == "bluez" {
        let wants = staging.join("usr/lib/systemd/system/multi-user.target.wants");
        fs::create_dir_all(&wants)?;
        std::os::unix::fs::symlink("../bluetooth.service", wants.join("bluetooth.service"))?;
    }
    Ok(())
}

pub(super) fn stage_vulkan_development(repo_root: &Path, staging: &Path) -> Result<()> {
    let headers = component_install(repo_root, "vulkan-headers");
    for relative in [
        "usr/include/vulkan",
        "usr/include/vk_video",
        "usr/share/vulkan/registry",
        "usr/share/cmake/VulkanHeaders",
    ] {
        copy_tree_preserving(&headers.join(relative), &staging.join(relative))?;
    }
    let loader = component_install(repo_root, "vulkan-loader");
    for relative in [
        "usr/lib/x86_64-linux-gnu/libvulkan.so",
        "usr/lib/x86_64-linux-gnu/pkgconfig/vulkan.pc",
    ] {
        copy_path_preserving(&loader.join(relative), &staging.join(relative))?;
    }
    let cmake = "usr/lib/x86_64-linux-gnu/cmake/VulkanLoader";
    copy_tree_preserving(&loader.join(cmake), &staging.join(cmake))?;
    copy_preserving(
        &repo_root.join("src/system/graphics/vulkan-loader/LICENSE.txt"),
        &staging.join("usr/share/doc/libvulkan-dev/copyright"),
    )?;
    Ok(())
}

/// One `-dev` package built from a library stage's install tree: headers,
/// linker names pointing at the runtime package's SONAME (as Debian does),
/// pkg-config files and the license notice.
struct DevelopmentPackage {
    package: &'static str,
    component: &'static str,
    /// Paths under `usr/include` in the install tree.
    headers: &'static [&'static str],
    /// (linker name, SONAME it points at).
    linker_names: &'static [(&'static str, &'static str)],
    pkgconfig: &'static [&'static str],
    license: &'static str,
}

const DEVELOPMENT_PACKAGES: &[DevelopmentPackage] = &[
    DevelopmentPackage {
        package: "libncurses-dev",
        component: "ncurses",
        headers: &[
            "curses.h", "eti.h", "form.h", "menu.h", "ncurses.h", "ncurses_dll.h", "panel.h",
            "term.h", "term_entry.h", "termcap.h", "unctrl.h",
        ],
        linker_names: &[("libncursesw.so", "libncursesw.so.6"), ("libtinfow.so", "libtinfow.so.6")],
        pkgconfig: &["ncursesw.pc", "tinfow.pc"],
        license: "src/system/terminal/ncurses/COPYING",
    },
    DevelopmentPackage {
        package: "libcap-dev",
        component: "libcap",
        headers: &["sys/capability.h"],
        linker_names: &[("libcap.so", "libcap.so.2")],
        pkgconfig: &["libcap.pc"],
        license: "src/system/libraries/libcap/License",
    },
    DevelopmentPackage {
        package: "libnl-3-dev",
        component: "libnl",
        headers: &["libnl3"],
        linker_names: &[("libnl-3.so", "libnl-3.so.200")],
        pkgconfig: &["libnl-3.0.pc"],
        license: "src/system/network/libnl/COPYING",
    },
    DevelopmentPackage {
        package: "libnl-genl-3-dev",
        component: "libnl",
        headers: &[],
        linker_names: &[("libnl-genl-3.so", "libnl-genl-3.so.200")],
        pkgconfig: &["libnl-genl-3.0.pc"],
        license: "src/system/network/libnl/COPYING",
    },
    DevelopmentPackage {
        package: "libsystemd-dev",
        component: "systemd",
        headers: &["systemd"],
        linker_names: &[("libsystemd.so", "libsystemd.so.0")],
        pkgconfig: &["libsystemd.pc"],
        license: "src/system/systemd/LICENSE.LGPL2.1",
    },
    DevelopmentPackage {
        package: "libacl1-dev",
        component: "acl",
        headers: &["acl", "sys/acl.h"],
        linker_names: &[("libacl.so", "libacl.so.1")],
        pkgconfig: &["libacl.pc"],
        license: "src/system/libraries/acl/doc/COPYING.LGPL",
    },
    DevelopmentPackage {
        package: "libattr1-dev",
        component: "attr",
        headers: &["attr"],
        linker_names: &[("libattr.so", "libattr.so.1")],
        pkgconfig: &["libattr.pc"],
        license: "src/system/libraries/attr/doc/COPYING.LGPL",
    },
];

pub(super) fn stage_development_package(repo_root: &Path, staging: &Path, name: &str) -> Result<()> {
    let package = DEVELOPMENT_PACKAGES
        .iter()
        .find(|package| package.package == name)
        .ok_or_else(|| anyhow!("{name} is not a table development package"))?;
    let install = component_install(repo_root, package.component).join("usr");
    for header in package.headers {
        let source = install.join("include").join(header);
        let destination = staging.join("usr/include").join(header);
        if source.is_dir() {
            copy_tree_preserving(&source, &destination)?;
        } else {
            copy_preserving(&source, &destination)?;
        }
    }
    // Generated headers can name the checkout they were generated in (for
    // example a comment in ncurses' curses.h); publish the stable source
    // prefix instead, as the kernel's module paths do.
    normalize_checkout_paths(repo_root, &staging.join("usr/include"))?;
    let libdir = staging.join("usr/lib/x86_64-linux-gnu");
    fs::create_dir_all(&libdir)?;
    for (linker_name, soname) in package.linker_names {
        std::os::unix::fs::symlink(soname, libdir.join(linker_name))?;
    }
    // Some producers (systemd's Meson install) write their DESTDIR prefix
    // into pkg-config metadata; the installed prefix is always /usr.
    let staged_prefix = format!("{}", install.display());
    for pc in package.pkgconfig {
        let source = install.join("lib/x86_64-linux-gnu/pkgconfig").join(pc);
        let body = fs::read_to_string(&source)
            .with_context(|| format!("failed to read {}", source.display()))?
            .replace(&staged_prefix, "/usr");
        let destination = libdir.join("pkgconfig").join(pc);
        fs::create_dir_all(destination.parent().expect("pkgconfig has a parent"))?;
        fs::write(destination, body)?;
    }
    copy_preserving(
        &repo_root.join(package.license),
        &staging.join("usr/share/doc").join(name).join("copyright"),
    )
}

/// pkgconf with its compatibility `pkg-config` name.  libpkgconf is static,
/// so its headers and archive stay in the build tree.
pub(super) fn stage_pkgconf(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "pkgconf").join("usr");
    stage_executable(&install.join("bin/pkgconf"), &staging.join("usr/bin/pkgconf"), 0o755)?;
    std::os::unix::fs::symlink("pkgconf", staging.join("usr/bin/pkg-config"))?;
    std::os::unix::fs::symlink("pkgconf", staging.join("usr/bin/x86_64-linux-gnu-pkg-config"))?;
    copy_preserving(&install.join("share/aclocal/pkg.m4"), &staging.join("usr/share/aclocal/pkg.m4"))?;
    copy_preserving(
        &repo_root.join("src/build-tools/pkgconf/COPYING"),
        &staging.join("usr/share/doc/pkgconf/copyright"),
    )
}

/// CMake with its module tree (Debian's cmake plus cmake-data).
pub(super) fn stage_cmake(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cmake").join("usr");
    for tool in ["cmake", "ctest", "cpack"] {
        stage_executable(&install.join("bin").join(tool), &staging.join("usr/bin").join(tool), 0o755)?;
    }
    let share = install.join("share");
    let modules = fs::read_dir(&share)?
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.file_name().to_string_lossy().starts_with("cmake-"))
        .ok_or_else(|| anyhow!("cmake install has no share/cmake-<version> module tree"))?;
    copy_tree_preserving(&modules.path(), &staging.join("usr/share").join(modules.file_name()))?;
    if share.join("aclocal").is_dir() {
        copy_tree_preserving(&share.join("aclocal"), &staging.join("usr/share/aclocal"))?;
    }
    copy_preserving(
        &repo_root.join("src/build-tools/cmake/LICENSE.rst"),
        &staging.join("usr/share/doc/cmake/copyright"),
    )
}

fn normalize_checkout_paths(repo_root: &Path, directory: &Path) -> Result<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    let checkout = repo_root.to_string_lossy().into_owned();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            normalize_checkout_paths(repo_root, &path)?;
        } else if metadata.is_file()
            && let Ok(text) = fs::read_to_string(&path)
            && text.contains(&checkout)
        {
            fs::write(&path, text.replace(&checkout, "/usr/src/mattos"))?;
        }
    }
    Ok(())
}

/// A package holding a component's whole `make install` tree, with the
/// named upstream license files as its copyright.  The shared GNU Info
/// directory index is left out: every GNU package would claim it.
pub(super) fn stage_install_tree(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    package: &str,
    source: &str,
    licenses: &[&str],
) -> Result<()> {
    let install = component_install(repo_root, component);
    copy_tree_filtered(&install, staging, &|relative, _| {
        relative != Path::new("usr/share/info/dir")
    })?;
    let doc = staging.join("usr/share/doc").join(package);
    fs::create_dir_all(&doc)?;
    let mut copyright = String::new();
    for license in licenses {
        copyright.push_str(&fs::read_to_string(repo_root.join(source).join(license))?);
    }
    fs::write(doc.join("copyright"), copyright)
        .with_context(|| format!("write {package} copyright"))
}

pub(crate) const MESON_LAUNCHER: &str =
    "#!/usr/bin/python3\nimport sys\nfrom mesonbuild.mesonmain import main\nsys.exit(main())\n";

/// Meson is pure Python: its `mesonbuild` package goes into the MattOS
/// Python's site-packages, with the launcher its entry point describes.
pub(super) fn stage_meson(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/build-tools/meson");
    copy_tree_filtered(
        &source.join("mesonbuild"),
        &staging.join("usr/lib/python3.14/site-packages/mesonbuild"),
        &|relative, _| {
            !relative.components().any(|part| {
                part.as_os_str() == OsStr::new("__pycache__")
                    || part.as_os_str().to_string_lossy().ends_with(".pyc")
            })
        },
    )?;
    let launcher = staging.join("usr/bin/meson");
    fs::create_dir_all(launcher.parent().expect("launcher has a parent"))?;
    fs::write(&launcher, MESON_LAUNCHER)?;
    set_mode(launcher.clone(), 0o755)?;
    copy_preserving(&source.join("man/meson.1"), &staging.join("usr/share/man/man1/meson.1"))?;
    copy_preserving(&source.join("COPYING"), &staging.join("usr/share/doc/meson/copyright"))
}
