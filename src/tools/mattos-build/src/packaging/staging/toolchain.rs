//! Payloads of the C/C++/Rust/Python toolchain and C library packages.

use super::*;

pub(super) fn stage_libffi_dev(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "libffi").join("usr");
    copy_tree_preserving(&install.join("include"), &staging.join("usr/include"))?;
    for relative in [
        "lib/x86_64-linux-gnu/libffi.so",
        "lib/x86_64-linux-gnu/pkgconfig/libffi.pc",
    ] {
        copy_path_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    copy_tree_preserving(
        &install.join("share/man/man3"),
        &staging.join("usr/share/man/man3"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/libraries/libffi/libffi/README.md"),
        &staging.join("usr/share/doc/libffi-dev/copyright"),
    )
}

pub(super) fn stage_cpython_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cpython").join("usr");
    for relative in [
        "bin/python3",
        "bin/python3.14",
        "bin/pydoc3",
        "bin/pydoc3.14",
    ] {
        copy_path_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    let stdlib = install.join("lib/python3.14");
    copy_tree_filtered(
        &stdlib,
        &staging.join("usr/lib/python3.14"),
        &|relative, _| {
            let first = relative.components().next().map(|part| part.as_os_str());
            !matches!(
                first.and_then(|part| part.to_str()),
                Some("ensurepip" | "venv" | "site-packages")
            ) && !relative
                .components()
                .next()
                .and_then(|part| part.as_os_str().to_str())
                .is_some_and(|name| name.starts_with("config-"))
                && !relative.components().any(|part| {
                    part.as_os_str() == OsStr::new("__pycache__")
                        || part.as_os_str().to_string_lossy().ends_with(".pyc")
                })
        },
    )?;
    normalize_cpython_package_metadata(repo_root, staging)?;
    copy_preserving(
        &repo_root.join("src/development/python/cpython/LICENSE"),
        &staging.join("usr/share/doc/python3/copyright"),
    )
}

pub(super) fn stage_cpython_venv(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cpython").join("usr");
    for relative in ["bin/pip3", "bin/pip3.14"] {
        copy_path_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    for relative in ["lib/python3.14/ensurepip", "lib/python3.14/venv"] {
        copy_tree_filtered(
            &install.join(relative),
            &staging.join("usr").join(relative),
            &|relative, metadata| {
                metadata.is_dir()
                    || !relative.components().any(|part| {
                        part.as_os_str() == OsStr::new("__pycache__")
                            || part.as_os_str().to_string_lossy().ends_with(".pyc")
                    })
            },
        )?;
    }
    // ensurepip's wheel is the supported offline bootstrap path.  The
    // installed pip package is useful for venv creation, but its generated
    // bytecode embeds the disposable build root and must never enter the
    // target package.  Recompile-on-first-use is deterministic and is also
    // what Python does when the cache is absent.
    copy_tree_filtered(
        &install.join("lib/python3.14/site-packages"),
        &staging.join("usr/lib/python3.14/site-packages"),
        &|relative, metadata| {
            metadata.is_dir()
                || !relative.components().any(|part| {
                    part.as_os_str() == OsStr::new("__pycache__")
                        || part.as_os_str().to_string_lossy().ends_with(".pyc")
                })
        },
    )?;
    copy_preserving(
        &repo_root.join("src/development/python/cpython/LICENSE"),
        &staging.join("usr/share/doc/python3-venv/copyright"),
    )
}

pub(super) fn stage_cpython_dev(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cpython").join("usr");
    for relative in ["bin/python3-config", "bin/python3.14-config"] {
        copy_path_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    copy_tree_preserving(&install.join("include"), &staging.join("usr/include"))?;
    for relative in [
        "lib/x86_64-linux-gnu/libpython3.14.so",
        "lib/x86_64-linux-gnu/libpython3.so",
    ] {
        copy_path_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    copy_tree_preserving(
        &install.join("lib/x86_64-linux-gnu/pkgconfig"),
        &staging.join("usr/lib/x86_64-linux-gnu/pkgconfig"),
    )?;
    let stdlib = install.join("lib/python3.14");
    copy_tree_filtered(
        &stdlib,
        &staging.join("usr/lib/python3.14"),
        &|relative, _| {
            relative
                .components()
                .next()
                .and_then(|part| part.as_os_str().to_str())
                .is_some_and(|name| name.starts_with("config-"))
        },
    )?;
    normalize_cpython_package_metadata(repo_root, staging)?;
    copy_preserving(
        &repo_root.join("src/development/python/cpython/LICENSE"),
        &staging.join("usr/share/doc/python3-dev/copyright"),
    )
}

pub(super) fn normalize_cpython_package_metadata(repo_root: &Path, staging: &Path) -> Result<()> {
    let build_root = repo_root.join("out/build");
    let build_root_text = build_root.to_string_lossy().into_owned();
    let repo_root_text = repo_root.to_string_lossy().into_owned();
    let sysroot = repo_root.join("out/sysroot").to_string_lossy().into_owned();
    let cpython = repo_root.join("out/build/cpython");
    let cpython_source = cpython.join("source").to_string_lossy().into_owned();
    let cpython_build = cpython.join("build").to_string_lossy().into_owned();
    let mut install_prefixes = Vec::new();
    if let Ok(entries) = fs::read_dir(&build_root) {
        for entry in entries {
            let install_prefix = entry?.path().join("install/usr");
            if install_prefix.is_dir() {
                install_prefixes.push(install_prefix.to_string_lossy().into_owned());
            }
        }
    }
    let python_stdlib = staging.join("usr/lib/python3.14");
    let python_config_bin = staging.join("usr/bin");
    walk_tree(staging, &mut |path, metadata| {
        if !metadata.is_file() {
            return Ok(());
        }
        // CPython emits more than Python source/JSON here.  Its generated
        // config scripts and Makefile contain absolute source, build, and
        // dependency-prefix paths too, and package validation must never
        // publish those host paths.  Keep the selection explicit so binary
        // runtime files are not treated as text metadata.
        let is_python_metadata = path.starts_with(&python_stdlib)
            && matches!(
                path.extension().and_then(OsStr::to_str),
                Some("json" | "py")
            );
        let is_python_config_file = path
            .strip_prefix(&python_stdlib)
            .ok()
            .and_then(|relative| relative.components().next())
            .and_then(|component| component.as_os_str().to_str())
            .is_some_and(|name| name.starts_with("config-"));
        let is_python_config_script = path.starts_with(&python_config_bin)
            && path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.ends_with("-config"));
        if !(is_python_metadata || is_python_config_file || is_python_config_script) {
            return Ok(());
        }
        let original = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let mut normalized = original.replace(&sysroot, "/");
        normalized = normalized.replace(&cpython_source, "/usr/src/mattos/cpython");
        normalized = normalized.replace(&cpython_build, "/usr/src/mattos/cpython/build");
        for prefix in &install_prefixes {
            normalized = normalized.replace(prefix, "/usr");
        }
        normalized = normalized.replace(&repo_root_text, "/usr/src/mattos");
        if normalized.contains(&build_root_text) || normalized.contains(&repo_root_text) {
            bail!(
                "CPython metadata /{} retains the host build root",
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
mod cpython_package_metadata_tests {
    use super::*;

    #[test]
    fn normalizes_config_scripts_and_generated_development_files() {
        let fixture = tempfile::tempdir().unwrap();
        let repo_root = fixture.path();
        let staging = repo_root.join("staging");
        let config_dir = staging.join("usr/lib/python3.14/config-3.14-x86_64-linux-gnu");
        let script = staging.join("usr/bin/python3.14-config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::create_dir_all(repo_root.join("out/build/openssl/install/usr/include")).unwrap();

        let root = repo_root.to_string_lossy();
        fs::write(
            &script,
            format!(
                "CFLAGS=--sysroot={root}/out/sysroot -I{root}/out/build/openssl/install/usr/include\n"
            ),
        )
        .unwrap();
        fs::write(
            config_dir.join("Makefile"),
            format!("srcdir={root}/out/build/cpython/source\nabs_builddir={root}/out/build/cpython/build\n"),
        )
        .unwrap();

        normalize_cpython_package_metadata(repo_root, &staging).unwrap();

        let script_contents = fs::read_to_string(script).unwrap();
        let makefile_contents = fs::read_to_string(config_dir.join("Makefile")).unwrap();
        assert!(!script_contents.contains(root.as_ref()));
        assert!(!makefile_contents.contains(root.as_ref()));
        assert!(script_contents.contains("--sysroot=/"));
        assert!(script_contents.contains("-I/usr/include"));
        assert!(makefile_contents.contains("/usr/src/mattos/cpython"));
        assert!(makefile_contents.contains("/usr/src/mattos/cpython/build"));
    }
}

pub(super) fn llvm_install(repo_root: &Path) -> PathBuf {
    component_install(repo_root, "llvm").join("usr")
}

pub(super) fn stage_llvm_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = llvm_install(repo_root);
    for name in [
        "libLLVM.so.22.1",
        "libLLVM-22.so",
        "libclang-cpp.so.22.1",
        "libclang.so.22.1.8",
        "libclang.so.22.1",
        "libLTO.so.22.1",
        "libRemarks.so.22.1",
    ] {
        let relative = Path::new("lib/x86_64-linux-gnu").join(name);
        copy_path_preserving(
            &install.join(&relative),
            &staging.join("usr").join(relative),
        )?;
    }
    copy_preserving(
        &repo_root.join("src/toolchain/llvm-project/llvm/LICENSE.TXT"),
        &staging.join("usr/share/doc/libllvm22/copyright"),
    )
}

pub(super) fn stage_llvm_tools(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = llvm_install(repo_root);
    copy_tree_filtered(
        &install.join("bin"),
        &staging.join("usr/bin"),
        &|relative, metadata| {
            if metadata.is_dir() {
                return true;
            }
            let name = relative
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default();
            name.starts_with("llvm-")
                || matches!(name, "FileCheck" | "llc" | "lli" | "opt" | "bugpoint")
        },
    )?;
    copy_tree_preserving(
        &install.join("share/opt-viewer"),
        &staging.join("usr/share/opt-viewer"),
    )?;
    copy_preserving(
        &repo_root.join("src/toolchain/llvm-project/llvm/LICENSE.TXT"),
        &staging.join("usr/share/doc/llvm/copyright"),
    )
}

pub(super) fn stage_llvm_development(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = llvm_install(repo_root);
    copy_tree_preserving(&install.join("include"), &staging.join("usr/include"))?;
    copy_tree_preserving(
        &install.join("lib/x86_64-linux-gnu/cmake"),
        &staging.join("usr/lib/x86_64-linux-gnu/cmake"),
    )?;
    copy_tree_filtered(
        &install.join("lib/x86_64-linux-gnu"),
        &staging.join("usr/lib/x86_64-linux-gnu"),
        &|relative, metadata| {
            if metadata.is_dir() {
                return true;
            }
            relative
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| {
                    name.ends_with(".a") || (name.ends_with(".so") && name != "libLLVM-22.so")
                })
                && !relative.starts_with("cmake")
        },
    )?;
    copy_preserving(
        &repo_root.join("src/toolchain/llvm-project/llvm/LICENSE.TXT"),
        &staging.join("usr/share/doc/llvm-dev/copyright"),
    )
}

pub(super) fn stage_clang(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = llvm_install(repo_root);
    copy_tree_filtered(
        &install.join("bin"),
        &staging.join("usr/bin"),
        &|relative, metadata| {
            if metadata.is_dir() {
                return true;
            }
            let name = relative
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default();
            name.starts_with("clang")
                || matches!(
                    name,
                    "analyze-build"
                        | "diagtool"
                        | "git-clang-format"
                        | "hmaptool"
                        | "intercept-build"
                        | "reduce-chunk-list"
                        | "sancov"
                        | "sanstats"
                        | "scan-build"
                        | "scan-build-py"
                        | "scan-view"
                        | "verify-uselistorder"
                )
        },
    )?;
    copy_tree_preserving(
        &install.join("lib/x86_64-linux-gnu/clang"),
        &staging.join("usr/lib/x86_64-linux-gnu/clang"),
    )?;
    for relative in ["share/clang", "share/scan-build", "share/scan-view"] {
        copy_tree_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
    copy_tree_preserving(
        &component_install(repo_root, "llvm").join("etc/clang"),
        &staging.join("etc/clang"),
    )?;
    copy_preserving(
        &repo_root.join("src/toolchain/llvm-project/clang/LICENSE.TXT"),
        &staging.join("usr/share/doc/clang/copyright"),
    )
}

pub(super) fn stage_lld(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = llvm_install(repo_root);
    for name in ["lld", "ld.lld", "ld64.lld", "lld-link", "wasm-ld"] {
        copy_path_preserving(
            &install.join("bin").join(name),
            &staging.join("usr/bin").join(name),
        )?;
    }
    copy_preserving(
        &repo_root.join("src/toolchain/llvm-project/lld/LICENSE.TXT"),
        &staging.join("usr/share/doc/lld/copyright"),
    )
}

pub(super) fn rust_install(repo_root: &Path) -> PathBuf {
    component_install(repo_root, "rust").join("usr")
}

pub(crate) fn stage_rustc(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = rust_install(repo_root);
    copy_tree_filtered(
        &install.join("bin"),
        &staging.join("usr/bin"),
        &|relative, metadata| {
            metadata.is_dir()
                || relative
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name != "cargo")
        },
    )?;
    copy_tree_filtered(
        &install.join("lib"),
        &staging.join("usr/lib"),
        &|relative, metadata| metadata.is_dir() || relative != Path::new("rustlib/install.log"),
    )?;
    copy_tree_preserving(
        &install.join("share/doc/rustc"),
        &staging.join("usr/share/doc/rustc"),
    )?;
    for name in ["rustc.1", "rustdoc.1"] {
        copy_preserving(
            &install.join("share/man/man1").join(name),
            &staging.join("usr/share/man/man1").join(name),
        )?;
    }
    Ok(())
}

pub(crate) fn stage_cargo(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = rust_install(repo_root);
    copy_preserving(&install.join("bin/cargo"), &staging.join("usr/bin/cargo"))?;
    copy_tree_preserving(
        &install.join("share/doc/cargo"),
        &staging.join("usr/share/doc/cargo"),
    )?;
    copy_tree_filtered(
        &install.join("share/man/man1"),
        &staging.join("usr/share/man/man1"),
        &|relative, metadata| {
            metadata.is_dir()
                || relative
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name == "cargo.1" || name.starts_with("cargo-"))
        },
    )?;
    copy_preserving(
        &install.join("share/zsh/site-functions/_cargo"),
        &staging.join("usr/share/zsh/site-functions/_cargo"),
    )?;
    Ok(())
}

pub(crate) const GLIBC_RUNTIME_LIBRARIES: &[&str] = &[
    "libBrokenLocale.so.1",
    "libanl.so.1",
    "libc.so.6",
    "libdl.so.2",
    "libm.so.6",
    "libmvec.so.1",
    "libnsl.so.1",
    "libnss_compat.so.2",
    "libnss_db.so.2",
    "libnss_dns.so.2",
    "libnss_files.so.2",
    "libnss_hesiod.so.2",
    "libpthread.so.0",
    "libresolv.so.2",
    "librt.so.1",
    "libthread_db.so.1",
    "libutil.so.1",
];

pub(super) fn stage_glibc_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/glibc/install");
    let source_libdir = install.join("usr/lib/x86_64-linux-gnu");
    let destination_libdir = staging.join("usr/lib/x86_64-linux-gnu");
    fs::create_dir_all(&destination_libdir)?;
    let mut manifest = Vec::new();
    for name in GLIBC_RUNTIME_LIBRARIES {
        let source = source_libdir.join(name);
        let destination = destination_libdir.join(name);
        copy_path_preserving(&source, &destination)?;
        manifest.push(format!(
            "/usr/lib/x86_64-linux-gnu/{name}\t{}",
            sha256_file(&destination)?
        ));
    }
    let loader = staging.join("usr/lib64/ld-linux-x86-64.so.2");
    copy_path_preserving(&install.join("lib64/ld-linux-x86-64.so.2"), &loader)?;
    manifest.push(format!(
        "/usr/lib64/ld-linux-x86-64.so.2\t{}",
        sha256_file(&loader)?
    ));
    manifest.sort();
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/COPYING.LIB"),
        &staging.join("usr/share/doc/libc6/copyright"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/LICENSES"),
        &staging.join("usr/share/doc/libc6/LICENSES"),
    )?;
    fs::write(
        staging.join("usr/share/doc/libc6/runtime-files.tsv"),
        format!("path\tsha256\n{}\n", manifest.join("\n")),
    )?;
    Ok(())
}

pub(super) fn stage_glibc_utilities(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/glibc/install");
    for name in ["getent", "locale"] {
        stage_executable(
            &install.join("usr/bin").join(name),
            &staging.join("usr/bin").join(name),
            0o755,
        )?;
    }
    copy_path_preserving(&install.join("usr/bin/ldd"), &staging.join("usr/bin/ldd"))?;
    stage_executable(
        &install.join("sbin/ldconfig"),
        &staging.join("usr/sbin/ldconfig"),
        0o755,
    )?;
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/COPYING.LIB"),
        &staging.join("usr/share/doc/libc-bin/copyright"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/LICENSES"),
        &staging.join("usr/share/doc/libc-bin/LICENSES"),
    )?;
    Ok(())
}

/// Ship glibc's own locale definitions and compiler.  The installer uses
/// these source-owned inputs to generate exactly the selected locale in the
/// target instead of claiming host-generated locales are available.
pub(super) fn stage_glibc_locales(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/glibc/install/usr");
    stage_executable(
        &install.join("bin/localedef"),
        &staging.join("usr/bin/localedef"),
        0o755,
    )?;
    copy_tree_preserving(&install.join("share/i18n"), &staging.join("usr/share/i18n"))?;
    if !staging.join("usr/share/i18n/locales/en_US").is_file()
        || !staging.join("usr/share/i18n/charmaps/UTF-8.gz").is_file()
    {
        bail!("glibc locale package is missing en_US or UTF-8 source data")
    }
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/COPYING.LIB"),
        &staging.join("usr/share/doc/locales/copyright"),
    )?;
    Ok(())
}

pub(super) fn stage_gcc_runtime_library(
    repo_root: &Path,
    staging: &Path,
    soname: &str,
    package: &str,
) -> Result<()> {
    let source_libdir = repo_root.join("out/build/gcc-runtime/runtime/usr/lib/x86_64-linux-gnu");
    let destination_libdir = staging.join("usr/lib/x86_64-linux-gnu");
    fs::create_dir_all(&destination_libdir)?;
    let soname_source = source_libdir.join(soname);
    if !path_entry_exists(&soname_source) {
        bail!(
            "GCC runtime build is missing {soname} at {}; build the gcc-runtime stage first",
            soname_source.display()
        )
    }
    let mut installed = Vec::new();
    if fs::symlink_metadata(&soname_source)?
        .file_type()
        .is_symlink()
    {
        let target = fs::read_link(&soname_source)?;
        let target_name = target
            .file_name()
            .ok_or_else(|| anyhow!("invalid GCC runtime symlink {}", soname_source.display()))?;
        copy_path_preserving(
            &source_libdir.join(target_name),
            &destination_libdir.join(target_name),
        )?;
        installed.push(target_name.to_string_lossy().to_string());
    }
    copy_path_preserving(&soname_source, &destination_libdir.join(soname))?;
    installed.push(soname.to_string());
    installed.sort();

    for license in ["COPYING3", "COPYING.RUNTIME"] {
        copy_preserving(
            &repo_root.join("src/toolchain/gcc").join(license),
            &staging.join("usr/share/doc").join(package).join(license),
        )?;
    }
    copy_preserving(
        &repo_root.join("out/build/gcc-runtime/runtime-abi.tsv"),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("runtime-abi.tsv"),
    )?;
    fs::write(
        staging
            .join("usr/share/doc")
            .join(package)
            .join("runtime-files.txt"),
        format!("{}\n", installed.join("\n")),
    )?;
    Ok(())
}

pub(super) fn stage_linux_libc_dev(repo_root: &Path, staging: &Path) -> Result<()> {
    let glibc_headers = repo_root.join("out/build/glibc/install/usr/include");
    copy_tree_filtered(
        &repo_root.join("out/build/glibc/linux-headers/usr/include"),
        &staging.join("usr/include"),
        &|relative, _| !path_entry_exists(&glibc_headers.join(relative)),
    )?;
    copy_preserving(
        &repo_root.join("src/kernel/linux-uapi/COPYING"),
        &staging.join("usr/share/doc/linux-libc-dev/copyright"),
    )?;
    copy_preserving(
        &repo_root.join("out/build/glibc/linux-headers-inventory.txt"),
        &staging.join("usr/share/doc/linux-libc-dev/generated-files.txt"),
    )
}

pub(super) fn stage_glibc_development(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/glibc/install/usr");
    copy_tree_preserving(&install.join("include"), &staging.join("usr/include"))?;
    copy_tree_filtered(
        &install.join("lib/x86_64-linux-gnu"),
        &staging.join("usr/lib/x86_64-linux-gnu"),
        &|relative, _| {
            relative
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| {
                    name.ends_with(".a") || name.ends_with(".o") || name.ends_with(".so")
                })
        },
    )?;
    copy_preserving(
        &repo_root.join("src/system/libc/glibc/COPYING.LIB"),
        &staging.join("usr/share/doc/libc6-dev/copyright"),
    )
}

pub(crate) fn stage_gcc_development(repo_root: &Path, staging: &Path, cxx: bool) -> Result<()> {
    let install = repo_root.join("out/build/gcc-runtime/install/usr");
    if cxx {
        copy_tree_preserving(
            &install.join("include/c++"),
            &staging.join("usr/include/c++"),
        )?;
        let source = install.join("lib/lib64");
        let destination = staging.join("usr/lib/x86_64-linux-gnu");
        for name in ["libstdc++.a", "libsupc++.a"] {
            copy_preserving(&source.join(name), &destination.join(name))?;
        }
        fs::create_dir_all(&destination)?;
        std::os::unix::fs::symlink("libstdc++.so.6", destination.join("libstdc++.so"))?;
        copy_preserving(
            &repo_root.join("src/toolchain/gcc/COPYING3"),
            &staging.join("usr/share/doc/mattos-libstdc++-dev/copyright"),
        )?;
        copy_preserving(
            &repo_root.join("src/toolchain/gcc/COPYING.RUNTIME"),
            &staging.join("usr/share/doc/mattos-libstdc++-dev/copyright.RUNTIME"),
        )?;
    } else {
        copy_tree_preserving(
            &install.join("lib/x86_64-linux-gnu/gcc"),
            &staging.join("usr/lib/x86_64-linux-gnu/gcc"),
        )?;
        let destination = staging.join("usr/lib/x86_64-linux-gnu");
        fs::create_dir_all(&destination)?;
        std::os::unix::fs::symlink("libgcc_s.so.1", destination.join("libgcc_s.so"))?;
        copy_preserving(
            &repo_root.join("src/toolchain/gcc/COPYING.RUNTIME"),
            &staging.join("usr/share/doc/mattos-libgcc-dev/copyright"),
        )?;
    }
    Ok(())
}

pub(super) fn stage_native_binutils(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("out/build/binutils/install/usr/bin");
    for name in [
        "addr2line",
        "ar",
        "as",
        "c++filt",
        "elfedit",
        "ld",
        "nm",
        "objcopy",
        "objdump",
        "ranlib",
        "readelf",
        "size",
        "strings",
        "strip",
    ] {
        copy_preserving(&source.join(name), &staging.join("usr/bin").join(name))?;
    }
    copy_preserving(
        &repo_root.join("src/toolchain/binutils/COPYING3"),
        &staging.join("usr/share/doc/binutils/copyright"),
    )
}

pub(super) fn stage_native_gcc_common(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/gcc-toolchain/install/usr");
    copy_tree_preserving(
        &install.join("libexec/gcc"),
        &staging.join("usr/libexec/gcc"),
    )?;
    if install.join("lib/gcc").is_dir() {
        copy_tree_preserving(&install.join("lib/gcc"), &staging.join("usr/lib/gcc"))?;
    }
    let multiarch_gcc = install.join("lib/x86_64-linux-gnu/gcc");
    if multiarch_gcc.is_dir() {
        let runtime_development =
            repo_root.join("out/build/gcc-runtime/install/usr/lib/x86_64-linux-gnu/gcc");
        copy_tree_filtered(
            &multiarch_gcc,
            &staging.join("usr/lib/x86_64-linux-gnu/gcc"),
            &|relative, _| !path_entry_exists(&runtime_development.join(relative)),
        )?;
    }
    copy_preserving(
        &repo_root.join("src/toolchain/gcc/COPYING3"),
        &staging.join("usr/share/doc/mattos-gcc-common/copyright"),
    )
}

pub(super) fn stage_native_compiler_driver(
    repo_root: &Path,
    staging: &Path,
    driver: &str,
) -> Result<()> {
    let source = repo_root.join("out/build/gcc-toolchain/install/usr/bin");
    copy_preserving(&source.join(driver), &staging.join("usr/bin").join(driver))?;
    match driver {
        "gcc" => std::os::unix::fs::symlink("gcc", staging.join("usr/bin/cc"))?,
        "g++" => std::os::unix::fs::symlink("g++", staging.join("usr/bin/c++"))?,
        _ => {}
    }
    copy_preserving(
        &repo_root.join("src/toolchain/gcc/COPYING3"),
        &staging.join("usr/share/doc").join(driver).join("copyright"),
    )
}

pub(super) fn stage_native_make(repo_root: &Path, staging: &Path) -> Result<()> {
    copy_preserving(
        &repo_root.join("out/build/make/install/usr/bin/make"),
        &staging.join("usr/bin/make"),
    )?;
    copy_preserving(
        &repo_root.join("src/build-tools/make/COPYING"),
        &staging.join("usr/share/doc/make/copyright"),
    )
}
