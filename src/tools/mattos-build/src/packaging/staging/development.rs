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
