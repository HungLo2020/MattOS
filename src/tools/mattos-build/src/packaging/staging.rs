use super::*;
pub(crate) fn stage_package(repo_root: &Path, spec: &PackageSpec) -> Result<()> {
    let staging = repo_root.join("out/packages/staging").join(spec.name);
    remove_path_if_exists(&staging)?;
    fs::create_dir_all(staging.join("DEBIAN"))?;
    match spec.name {
        "mattos-filesystem" => stage_filesystem(&staging)?,
        "mattos-compat" => stage_mattos_compat(repo_root, &staging)?,
        "libc6" => stage_glibc_runtime(repo_root, &staging)?,
        "libgcc-s1" => {
            stage_gcc_runtime_library(repo_root, &staging, "libgcc_s.so.1", "libgcc-s1")?
        }
        "libgomp1" => stage_gcc_runtime_library(repo_root, &staging, "libgomp.so.1", "libgomp1")?,
        "libstdc++6" => {
            stage_gcc_runtime_library(repo_root, &staging, "libstdc++.so.6", "libstdc++6")?
        }
        "linux-libc-dev" => stage_linux_libc_dev(repo_root, &staging)?,
        "linux-modules-7.2.0-rc5-mattos" => stage_linux_modules(repo_root, &staging)?,
        "libc6-dev" => stage_glibc_development(repo_root, &staging)?,
        "mattos-libgcc-dev" => stage_gcc_development(repo_root, &staging, false)?,
        "mattos-libstdc++-dev" => stage_gcc_development(repo_root, &staging, true)?,
        "binutils" => stage_native_binutils(repo_root, &staging)?,
        "mattos-gcc-common" => stage_native_gcc_common(repo_root, &staging)?,
        "cpp" => stage_native_compiler_driver(repo_root, &staging, "cpp")?,
        "gcc" => stage_native_compiler_driver(repo_root, &staging, "gcc")?,
        "g++" => stage_native_compiler_driver(repo_root, &staging, "g++")?,
        "make" => stage_native_make(repo_root, &staging)?,
        "libc-bin" => stage_glibc_utilities(repo_root, &staging)?,
        "locales" => stage_glibc_locales(repo_root, &staging)?,
        "iso-codes" => stage_iso_codes(repo_root, &staging)?,
        "tzdata" => stage_tzdata(repo_root, &staging)?,
        "linux-firmware" => stage_linux_firmware(repo_root, &staging)?,
        "wireless-regdb" => stage_wireless_regdb(repo_root, &staging)?,
        "mattos-base-files" => stage_base_files(repo_root, &staging)?,
        "systemd" => stage_systemd_runtime(repo_root, &staging)?,
        "mattos-base-runtime" => stage_mattos_base_runtime(repo_root, &staging)?,
        "mattos-base" | "mattos-cli" | "mattos-plasma" => {
            stage_profile_package(repo_root, &staging, spec.name)?
        }
        "mattos-plasma-live" => stage_plasma_live_session_integration(repo_root, &staging)?,
        "mattos-plasma-theme" => stage_mattos_plasma_theme(repo_root, &staging)?,
        "ca-certificates" => stage_ca_certificates(repo_root, &staging)?,
        "mattos-brush" => stage_brush(repo_root, &staging)?,
        "coreutils" => stage_coreutils(repo_root, &staging)?,
        "curl" => {
            let source = repo_root.join("out/build/curl/install/usr/bin/curl");
            stage_executable(&source, &staging.join("usr/bin/curl"), 0o755)?;
            let source_libdir = repo_root.join("out/build/curl/install/usr/lib/x86_64-linux-gnu");
            let destination_libdir = staging.join("usr/lib/x86_64-linux-gnu");
            fs::create_dir_all(&destination_libdir)?;
            stage_executable(
                &source_libdir.join("libcurl.so.4.8.0"),
                &destination_libdir.join("libcurl.so.4.8.0"),
                0o644,
            )?;
            std::os::unix::fs::symlink(
                "libcurl.so.4.8.0",
                destination_libdir.join("libcurl.so.4"),
            )?;
        }
        "dpkg" => stage_dpkg(repo_root, &staging)?,
        "libgpg-error0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libgpg-error",
            "libgpg-error.so.0",
            "src/system/security/libgpg-error/COPYING.LIB",
            "libgpg-error0",
        )?,
        "libgcrypt20" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libgcrypt",
            "libgcrypt.so.20",
            "src/system/security/libgcrypt/COPYING.LIB",
            "libgcrypt20",
        )?,
        "libassuan9" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libassuan",
            "libassuan.so.9",
            "src/system/security/libassuan/COPYING.LIB",
            "libassuan9",
        )?,
        "libksba8" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libksba",
            "libksba.so.8",
            "src/system/security/libksba/COPYING.LGPLv3",
            "libksba8",
        )?,
        "libnpth0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "npth",
            "libnpth.so.0",
            "src/system/security/npth/COPYING.LIB",
            "libnpth0",
        )?,
        "gpgv" => {
            stage_runtime_paths(repo_root, &staging, "gpgv", &["usr/bin/gpgv"])?;
            copy_preserving(
                &repo_root.join("src/system/security/gnupg/COPYING"),
                &staging.join("usr/share/doc/gpgv/copyright"),
            )?;
        }
        "gnupg" => stage_gnupg(repo_root, &staging)?,
        "libapt-pkg7.0" => stage_libapt_pkg(repo_root, &staging)?,
        "apt" => stage_apt(repo_root, &staging)?,
        "mattos-libtinfow6" => stage_library_family(
            repo_root,
            &staging,
            "ncurses",
            &["libtinfow.so.6.6", "libtinfow.so.6"],
        )?,
        "libncursesw6" => stage_library_family(
            repo_root,
            &staging,
            "ncurses",
            &[
                "libncursesw.so.6.6",
                "libncursesw.so.6",
                "libpanelw.so.6.6",
                "libpanelw.so.6",
            ],
        )?,
        "libreadline8" => stage_library_family(
            repo_root,
            &staging,
            "readline",
            &["libreadline.so.8.2", "libreadline.so.8"],
        )?,
        "libndp0" => stage_library_family(
            repo_root,
            &staging,
            "libndp",
            &["libndp.so.0", "libndp.so.0.3.0"],
        )?,
        "ncurses-base" => stage_terminfo(repo_root, &staging)?,
        "ncurses-bin" => {
            stage_runtime_paths(repo_root, &staging, "ncurses", NCURSES_RUNTIME_PATHS)?
        }
        "libkmod2" => stage_library_family(
            repo_root,
            &staging,
            "kmod",
            &["libkmod.so.2.5.1", "libkmod.so.2"],
        )?,
        "kmod" => stage_runtime_paths(repo_root, &staging, "kmod", KMOD_RUNTIME_PATHS)?,
        "mattos-libproc2" => stage_library_family(
            repo_root,
            &staging,
            "procps-ng",
            &["libproc2.so.1.0.1", "libproc2.so.1"],
        )?,
        "procps" => stage_procps(repo_root, &staging)?,
        "libsystemd0" => stage_library_family(
            repo_root,
            &staging,
            "systemd",
            &["libsystemd.so.0.44.0", "libsystemd.so.0"],
        )?,
        "libudev1" => stage_library_family(
            repo_root,
            &staging,
            "systemd",
            &["libudev.so.1.7.14", "libudev.so.1"],
        )?,
        "udev" => stage_udev_hwdb(repo_root, &staging)?,
        "libexpat1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "expat",
            "libexpat.so.1",
            "src/system/libraries/expat/expat/COPYING",
            "libexpat1",
        )?,
        "libfreetype6" => stage_imported_soname_library(
            repo_root,
            &staging,
            "freetype",
            "libfreetype.so.6",
            "src/system/libraries/freetype/LICENSE.TXT",
            "libfreetype6",
        )?,
        "libfontconfig1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "fontconfig",
            "libfontconfig.so.1",
            "src/system/libraries/fontconfig/COPYING",
            "libfontconfig1",
        )?,
        "fontconfig" => stage_fontconfig(repo_root, &staging)?,
        "fonts-fira" => stage_pop_fonts(repo_root, &staging)?,
        "libcap2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libcap",
            "libcap.so.2",
            "src/system/libraries/libcap/License",
            "libcap2",
        )?,
        "libattr1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "attr",
            "libattr.so.1",
            "src/system/libraries/attr/doc/COPYING.LGPL",
            "libattr1",
        )?,
        "libacl1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "acl",
            "libacl.so.1",
            "src/system/libraries/acl/doc/COPYING.LGPL",
            "libacl1",
        )?,
        "zlib1g" => stage_imported_soname_library(
            repo_root,
            &staging,
            "zlib",
            "libz.so.1",
            "src/system/libraries/zlib/LICENSE",
            "zlib1g",
        )?,
        "libbz2-1.0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "bzip2",
            "libbz2.so.1.0",
            "src/system/libraries/bzip2/LICENSE",
            "libbz2-1.0",
        )?,
        "liblz4-1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "lz4",
            "liblz4.so.1",
            "src/system/libraries/lz4/LICENSE",
            "liblz4-1",
        )?,
        "liblzma5" => stage_imported_soname_library(
            repo_root,
            &staging,
            "xz",
            "liblzma.so.5",
            "src/system/libraries/xz/COPYING",
            "liblzma5",
        )?,
        "libxxhash0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "xxhash",
            "libxxhash.so.0",
            "src/system/libraries/xxhash/LICENSE",
            "libxxhash0",
        )?,
        "libmd0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libmd",
            "libmd.so.0",
            "src/system/libraries/libmd/COPYING",
            "libmd0",
        )?,
        "libbsd0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libbsd",
            "libbsd.so.0",
            "src/system/libraries/libbsd/COPYING",
            "libbsd0",
        )?,
        "libzstd1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "zstd",
            "libzstd.so.1",
            "src/system/libraries/zstd/LICENSE",
            "libzstd1",
        )?,
        "mattos-libcrypto3" => stage_imported_soname_library(
            repo_root,
            &staging,
            "openssl",
            "libcrypto.so.3",
            "src/system/libraries/openssl/LICENSE.txt",
            "mattos-libcrypto3",
        )?,
        "libssl3t64" => stage_imported_soname_library(
            repo_root,
            &staging,
            "openssl",
            "libssl.so.3",
            "src/system/libraries/openssl/LICENSE.txt",
            "libssl3t64",
        )?,
        "libelf1t64" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "elfutils",
                "libelf.so.1",
                "src/system/libraries/elfutils/COPYING-LGPLV3",
                "libelf1t64",
            )?;
            copy_preserving(
                &repo_root.join("src/system/libraries/elfutils/COPYING-GPLV2"),
                &staging.join("usr/share/doc/libelf1t64/copyright.GPL-2"),
            )?;
        }
        "libpcre2-8-0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "pcre2",
            "libpcre2-8.so.0",
            "src/system/libraries/pcre2/LICENCE.md",
            "libpcre2-8-0",
        )?,
        "libselinux1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "selinux",
            "libselinux.so.1",
            "src/system/security/selinux/libselinux/LICENSE",
            "libselinux1",
        )?,
        "libcrypt1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libxcrypt",
            "libcrypt.so.1",
            "src/system/libraries/libxcrypt/COPYING.LIB",
            "libcrypt1",
        )?,
        "libblkid1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libblkid.so.1",
            "src/userland/util-linux/COPYING",
            "libblkid1",
        )?,
        "libmount1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libmount.so.1",
            "src/userland/util-linux/COPYING",
            "libmount1",
        )?,
        "libsmartcols1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libsmartcols.so.1",
            "src/userland/util-linux/COPYING",
            "libsmartcols1",
        )?,
        "mount" => {
            stage_runtime_paths(
                repo_root,
                &staging,
                "util-linux",
                &["usr/bin/mount", "usr/bin/umount"],
            )?;
            copy_preserving(
                &repo_root.join("src/userland/util-linux/COPYING"),
                &staging.join("usr/share/doc/mount/copyright"),
            )?;
            for rel in ["usr/bin/mount", "usr/bin/umount"] {
                set_mode(staging.join(rel), 0o4755)?;
            }
        }
        "libuuid1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libuuid.so.1",
            "src/userland/util-linux/COPYING",
            "libuuid1",
        )?,
        "libfdisk1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "util-linux",
            "libfdisk.so.1",
            "src/userland/util-linux/COPYING",
            "libfdisk1",
        )?,
        "util-linux" => {
            stage_runtime_paths(repo_root, &staging, "util-linux", UTIL_LINUX_BASE_PATHS)?;
            copy_preserving(
                &repo_root.join("src/userland/util-linux/COPYING"),
                &staging.join("usr/share/doc/util-linux/copyright"),
            )?;
        }
        "gzip" => stage_runtime_paths(
            repo_root,
            &staging,
            "gzip",
            &["usr/bin/gzip", "usr/bin/gunzip", "usr/bin/zcat"],
        )?,
        "bzip2" => stage_runtime_paths(
            repo_root,
            &staging,
            "bzip2",
            &[
                "usr/bin/bzip2",
                "usr/bin/bunzip2",
                "usr/bin/bzcat",
                "usr/bin/bzip2recover",
            ],
        )?,
        "xz-utils" => stage_runtime_paths(
            repo_root,
            &staging,
            "xz",
            &[
                "usr/bin/xz",
                "usr/bin/unxz",
                "usr/bin/xzcat",
                "usr/bin/lzma",
                "usr/bin/unlzma",
                "usr/bin/lzcat",
            ],
        )?,
        "zstd" => stage_runtime_paths(
            repo_root,
            &staging,
            "zstd",
            &["usr/bin/zstd", "usr/bin/unzstd", "usr/bin/zstdcat"],
        )?,
        "patch" => stage_runtime_paths(repo_root, &staging, "patch", &["usr/bin/patch"])?,
        "libmagic1" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "file",
                "libmagic.so.1",
                "src/userland/file/COPYING",
                "libmagic1",
            )?;
            copy_preserving(
                &repo_root.join("out/build/file/install/usr/share/misc/magic.mgc"),
                &staging.join("usr/share/misc/magic.mgc"),
            )?;
        }
        "file" => stage_runtime_paths(repo_root, &staging, "file", &["usr/bin/file"])?,
        "less" => stage_runtime_paths(
            repo_root,
            &staging,
            "less",
            &["usr/bin/less", "usr/bin/lesskey", "usr/libexec/lessecho"],
        )?,
        "git" => copy_tree_preserving(
            &repo_root.join("out/build/git/install/usr"),
            &staging.join("usr"),
        )?,
        "libffi8" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libffi",
            "libffi.so.8",
            // LICENSE is globally ignored; libffi's retained README carries
            // the upstream licensing notice for the package boundary.
            "src/system/libraries/libffi/libffi/README.md",
            "libffi8",
        )?,
        "libffi-dev" => stage_libffi_dev(repo_root, &staging)?,
        "libicu78" => {
            stage_library_family(
                repo_root,
                &staging,
                "icu",
                &[
                    "libicudata.so.78.3",
                    "libicudata.so.78",
                    "libicuuc.so.78.3",
                    "libicuuc.so.78",
                    "libicui18n.so.78.3",
                    "libicui18n.so.78",
                    "libicuio.so.78.3",
                    "libicuio.so.78",
                ],
            )?;
            // ICU's shared data is intentionally an external archive for the
            // selected --with-data-packaging=archive build.  The data stub
            // libicudata.so cannot initialize locale/time-zone services by
            // itself, so it is part of this runtime package rather than an
            // optional development payload.
            let data = component_install(repo_root, "icu").join("usr/share/icu");
            if !data.join("78.3/icudt78l.dat").is_file() {
                bail!(
                    "ICU runtime data archive is missing from {}",
                    data.display()
                );
            }
            copy_tree_preserving(&data, &staging.join("usr/share/icu"))?;
        }
        "kf6-kdeclarative" => {
            stage_kde_module(repo_root, &staging, "kdeclarative", "kf6-kdeclarative")?
        }
        "greetd" => copy_tree_preserving(
            &repo_root.join("out/build/greetd/install/usr"),
            &staging.join("usr"),
        )?,
        "qt6-base" => stage_qt_module(repo_root, &staging, "qtbase", "qt6-base")?,
        "qt6-shadertools" => {
            stage_qt_module(repo_root, &staging, "qtshadertools", "qt6-shadertools")?
        }
        "qt6-declarative" => {
            stage_qt_module(repo_root, &staging, "qtdeclarative", "qt6-declarative")?
        }
        "qt6-svg" => stage_qt_module(repo_root, &staging, "qtsvg", "qt6-svg")?,
        "qt6-wayland" => stage_qt_module(repo_root, &staging, "qtwayland", "qt6-wayland")?,
        "qt6-tools" => stage_qt_module(repo_root, &staging, "qttools", "qt6-tools")?,
        "qt6-multimedia" => stage_qt_module(repo_root, &staging, "qtmultimedia", "qt6-multimedia")?,
        "qt6-speech" => stage_qt_module(repo_root, &staging, "qtspeech", "qt6-speech")?,
        "qt6-core5compat" => stage_qt_module(repo_root, &staging, "qt5compat", "qt6-core5compat")?,
        "qca-qt6" => stage_kde_module(repo_root, &staging, "qca", "qca-qt6")?,
        "qcoro-qt6" => stage_kde_module(repo_root, &staging, "qcoro", "qcoro-qt6")?,
        "kwin" => stage_kde_module(repo_root, &staging, "kwin", "kwin")?,
        "kwin-aurorae" => stage_kde_module(repo_root, &staging, "aurorae", "kwin-aurorae")?,
        "layer-shell-qt" => {
            stage_kde_module(repo_root, &staging, "layer-shell-qt", "layer-shell-qt")?
        }
        "plasma-workspace" => {
            stage_kde_module(repo_root, &staging, "plasma-workspace", "plasma-workspace")?
        }
        "kscreenlocker" => stage_kde_module(repo_root, &staging, "kscreenlocker", "kscreenlocker")?,
        "plasma-framework" => {
            stage_kde_module(repo_root, &staging, "plasma-framework", "plasma-framework")?
        }
        "plasma5support" => {
            stage_kde_module(repo_root, &staging, "plasma5support", "plasma5support")?
        }
        "krunner" => stage_kde_module(repo_root, &staging, "krunner", "krunner")?,
        "plasma-desktop" => {
            stage_kde_module(repo_root, &staging, "plasma-desktop", "plasma-desktop")?;
            stage_plasma_session_integration(repo_root, &staging)?;
        }
        "plasma-login-manager" => {
            stage_kde_module(
                repo_root,
                &staging,
                "plasma-login-manager",
                "plasma-login-manager",
            )?;
            stage_plasma_login_manager_integration(repo_root, &staging)?;
        }
        "breeze" => stage_kde_module(repo_root, &staging, "breeze", "breeze")?,
        "breeze-icons" => stage_kde_module(repo_root, &staging, "breeze-icons", "breeze-icons")?,
        "lm-sensors" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "lm-sensors",
            "lm-sensors",
            "src/system/libraries/lm-sensors/COPYING",
        )?,
        "libhwy1" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "highway",
            "libhwy1",
            "src/system/libraries/highway/LICENSE-BSD3",
        )?,
        "kf6-kfilemetadata" => {
            stage_kde_module(repo_root, &staging, "kfilemetadata", "kf6-kfilemetadata")?
        }
        "kf6-kpty" => stage_kde_module(repo_root, &staging, "kpty", "kf6-kpty")?,
        "kf6-networkmanager-qt" => stage_kde_module(
            repo_root,
            &staging,
            "networkmanager-qt",
            "kf6-networkmanager-qt",
        )?,
        "kf6-purpose" => stage_kde_module(repo_root, &staging, "purpose", "kf6-purpose")?,
        "milou" => stage_kde_module(repo_root, &staging, "milou", "milou")?,
        "systemsettings" => {
            stage_kde_module(repo_root, &staging, "systemsettings", "systemsettings")?
        }
        "ksystemstats" => stage_kde_module(repo_root, &staging, "ksystemstats", "ksystemstats")?,
        "plasma-systemmonitor" => stage_kde_module(
            repo_root,
            &staging,
            "plasma-systemmonitor",
            "plasma-systemmonitor",
        )?,
        "polkit-kde-agent-1" => stage_kde_module(
            repo_root,
            &staging,
            "polkit-kde-agent-1",
            "polkit-kde-agent-1",
        )?,
        "kquickimageeditor" => stage_kde_module(
            repo_root,
            &staging,
            "kquickimageeditor",
            "kquickimageeditor",
        )?,
        "ffmpeg-libs" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "ffmpeg",
            "ffmpeg-libs",
            "src/system/multimedia/ffmpeg/LICENSE.md",
        )?,
        "libva2" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "libva",
            "libva2",
            "src/system/graphics/libva/COPYING",
        )?,
        "libopencv4" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "opencv",
            "libopencv4",
            "src/system/multimedia/opencv/LICENSE",
        )?,
        "kpipewire" => stage_kde_module(repo_root, &staging, "kpipewire", "kpipewire")?,
        "spectacle" => stage_kde_module(repo_root, &staging, "spectacle", "spectacle")?,
        "pulseaudio-qt" => stage_kde_module(repo_root, &staging, "pulseaudio-qt", "pulseaudio-qt")?,
        "plasma-pa" => stage_kde_module(repo_root, &staging, "plasma-pa", "plasma-pa")?,
        "plasma-nm" => stage_kde_module(repo_root, &staging, "plasma-nm", "plasma-nm")?,
        "powerdevil" => stage_kde_module(repo_root, &staging, "powerdevil", "powerdevil")?,
        "xdg-desktop-portal-kde" => stage_kde_module(
            repo_root,
            &staging,
            "xdg-desktop-portal-kde",
            "xdg-desktop-portal-kde",
        )?,
        "dolphin" => stage_kde_module(repo_root, &staging, "dolphin", "dolphin")?,
        "konsole" => stage_kde_module(repo_root, &staging, "konsole", "konsole")?,
        "kate" => stage_kde_module(repo_root, &staging, "kate", "kate")?,
        "ark" => stage_kde_module(repo_root, &staging, "ark", "ark")?,
        "kf6-kparts" => stage_kde_module(repo_root, &staging, "kparts", "kf6-kparts")?,
        "kf6-ktextwidgets" => {
            stage_kde_module(repo_root, &staging, "ktextwidgets", "kf6-ktextwidgets")?
        }
        "kf6-ktexteditor" => {
            stage_kde_module(repo_root, &staging, "ktexteditor", "kf6-ktexteditor")?
        }
        "libqrencode4" => {
            stage_multimedia_sdk(
                repo_root,
                &staging,
                "qrencode",
                "libqrencode4",
                "src/system/libraries/qrencode/COPYING",
            )?;
            remove_path_if_exists(&staging.join("usr/lib/x86_64-linux-gnu/libqrencode.la"))?;
        }
        "libzxing4" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "zxing-cpp",
            "libzxing4",
            // The imported tree omits the ignored aggregate LICENSE;
            // retain the upstream project README as the package notice.
            "src/system/libraries/zxing-cpp/README.md",
        )?,
        "kf6-prison" => stage_kde_module(repo_root, &staging, "prison", "kf6-prison")?,
        "kf6-libkscreen" => stage_kde_module(repo_root, &staging, "libkscreen", "kf6-libkscreen")?,
        "modemmanager" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "modemmanager",
            "modemmanager",
            "src/system/services/modemmanager/COPYING",
        )?,
        "kf6-modemmanager-qt" => stage_kde_module(
            repo_root,
            &staging,
            "modemmanager-qt",
            "kf6-modemmanager-qt",
        )?,
        "libarchive13" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "libarchive",
            "libarchive13",
            "src/system/libraries/libarchive/COPYING",
        )?,
        "libsndfile1" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "libsndfile",
            "libsndfile1",
            "src/system/multimedia/libsndfile/COPYING",
        )?,
        "libpulse0" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "pulseaudio",
            "libpulse0",
            // The aggregate LICENSE path is globally ignored in the source
            // import; retain PulseAudio's tracked LGPL notice instead.
            "src/system/multimedia/pulseaudio/LGPL",
        )?,
        "libgudev-1.0-0" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "libgudev",
            "libgudev-1.0-0",
            "src/system/libraries/libgudev/COPYING",
        )?,
        "libgmp10" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "gmp",
            "libgmp10",
            "src/system/libraries/gmp/COPYING.LESSERv3",
        )?,
        "libmpfr6" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "mpfr",
            "libmpfr6",
            "src/system/libraries/mpfr/COPYING.LESSER",
        )?,
        "libbytesize1" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "libbytesize",
            "libbytesize1",
            "src/system/libraries/libbytesize/LICENSE",
        )?,
        "libkeyutils1" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "keyutils",
            "libkeyutils1",
            "src/system/security/keyutils/LICENCE.LGPL",
        )?,
        "libnvme1" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "libnvme",
            "libnvme1",
            "src/system/libraries/libnvme/COPYING",
        )?,
        "libpopt0" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "popt",
            "libpopt0",
            "src/system/libraries/popt/COPYING",
        )?,
        "libjson-c5" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "json-c",
            "libjson-c5",
            "src/system/libraries/json-c/COPYING",
        )?,
        "libdevmapper1.02.1" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "lvm2",
            "libdevmapper1.02.1",
            "src/system/storage/lvm2/COPYING",
        )?,
        "libcryptsetup12" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "cryptsetup",
            "libcryptsetup12",
            "src/system/storage/cryptsetup/COPYING",
        )?,
        "libblockdev3" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "libblockdev",
            "libblockdev3",
            "src/system/libraries/libblockdev/LICENSE",
        )?,
        "wireplumber" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "wireplumber",
            "wireplumber",
            // The imported tree omits the ignored aggregate LICENSE file;
            // its retained README identifies the upstream MIT licensing.
            "src/system/multimedia/wireplumber/README.rst",
        )?,
        "upower" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "upower",
            "upower",
            "src/system/services/upower/COPYING",
        )?,
        "udisks2" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "udisks2",
            "udisks2",
            "src/system/services/udisks2/COPYING",
        )?,
        "bluez" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "bluez",
            "bluez",
            "src/system/services/bluez/COPYING",
        )?,
        "power-profiles-daemon" => stage_multimedia_sdk(
            repo_root,
            &staging,
            "power-profiles-daemon",
            "power-profiles-daemon",
            "src/system/services/power-profiles-daemon/COPYING",
        )?,
        "kf6-kcoreaddons" => {
            stage_kde_module(repo_root, &staging, "kcoreaddons", "kf6-kcoreaddons")?
        }
        "kf6-ki18n" => stage_kde_module(repo_root, &staging, "ki18n", "kf6-ki18n")?,
        "kf6-kwidgetsaddons" => {
            stage_kde_module(repo_root, &staging, "kwidgetsaddons", "kf6-kwidgetsaddons")?
        }
        "kf6-kconfig" => stage_kde_module(repo_root, &staging, "kconfig", "kf6-kconfig")?,
        "kf6-kcmutils" => stage_kde_module(repo_root, &staging, "kcmutils", "kf6-kcmutils")?,
        "kf6-knewstuffcore" => {
            stage_kde_module(repo_root, &staging, "knewstuff", "kf6-knewstuffcore")?
        }
        "kf6-attica" => stage_kde_module(repo_root, &staging, "attica", "kf6-attica")?,
        "kf6-sonnet" => stage_kde_module(repo_root, &staging, "sonnet", "kf6-sonnet")?,
        "kf6-kdbusaddons" => {
            stage_kde_module(repo_root, &staging, "kdbusaddons", "kf6-kdbusaddons")?
        }
        "kf6-kcrash" => stage_kde_module(repo_root, &staging, "kcrash", "kf6-kcrash")?,
        "kf6-kwindowsystem" => {
            stage_kde_module(repo_root, &staging, "kwindowsystem", "kf6-kwindowsystem")?
        }
        "kf6-kpackage" => stage_kde_module(repo_root, &staging, "kpackage", "kf6-kpackage")?,
        "kf6-karchive" => stage_kde_module(repo_root, &staging, "karchive", "kf6-karchive")?,
        "kf6-kio" => stage_kde_module(repo_root, &staging, "kio", "kf6-kio")?,
        "kf6-kunitconversion" => stage_kde_module(
            repo_root,
            &staging,
            "kunitconversion",
            "kf6-kunitconversion",
        )?,
        "kf6-ksvg" => stage_kde_module(repo_root, &staging, "ksvg", "kf6-ksvg")?,
        "kf6-knotifications" => {
            stage_kde_module(repo_root, &staging, "knotifications", "kf6-knotifications")?
        }
        "kf6-knotifyconfig" => {
            stage_kde_module(repo_root, &staging, "knotifyconfig", "kf6-knotifyconfig")?
        }
        "kf6-kguiaddons" => stage_kde_module(repo_root, &staging, "kguiaddons", "kf6-kguiaddons")?,
        "kf6-kitemmodels" => {
            stage_kde_module(repo_root, &staging, "kitemmodels", "kf6-kitemmodels")?
        }
        "kf6-kglobalaccel" => {
            stage_kde_module(repo_root, &staging, "kglobalaccel", "kf6-kglobalaccel")?
        }
        "kf6-kiconthemes" => {
            stage_kde_module(repo_root, &staging, "kiconthemes", "kf6-kiconthemes")?
        }
        "kf6-kcolorscheme" => {
            stage_kde_module(repo_root, &staging, "kcolorscheme", "kf6-kcolorscheme")?
        }
        "kf6-ksyntaxhighlighting" => stage_kde_module(
            repo_root,
            &staging,
            "ksyntaxhighlighting",
            "kf6-ksyntaxhighlighting",
        )?,
        "kf6-kjobwidgets" => {
            stage_kde_module(repo_root, &staging, "kjobwidgets", "kf6-kjobwidgets")?
        }
        "kf6-kcompletion" => {
            stage_kde_module(repo_root, &staging, "kcompletion", "kf6-kcompletion")?
        }
        "kf6-kservice" => stage_kde_module(repo_root, &staging, "kservice", "kf6-kservice")?,
        "kf6-solid" => stage_kde_module(repo_root, &staging, "solid", "kf6-solid")?,
        "kf6-kcodecs" => stage_kde_module(repo_root, &staging, "kcodecs", "kf6-kcodecs")?,
        "kf6-kdecoration" => {
            stage_kde_module(repo_root, &staging, "kdecoration", "kf6-kdecoration")?
        }
        "kf6-kidletime" => stage_kde_module(repo_root, &staging, "kidletime", "kf6-kidletime")?,
        "liblcms2-2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "lcms2",
            "liblcms2.so.2",
            // The imported source omits the ignored aggregate LICENSE;
            // retain the upstream README as the package notice.
            "src/system/graphics/lcms2/README.md",
            "liblcms2-2",
        )?,
        "kf6-kwayland" => stage_kde_module(repo_root, &staging, "kwayland", "kf6-kwayland")?,
        "kf6-knighttime" => stage_kde_module(repo_root, &staging, "knighttime", "kf6-knighttime")?,
        "kf6-kwallet" => stage_kde_module(repo_root, &staging, "kwallet", "kf6-kwallet")?,
        "kf6-kholidays" => stage_kde_module(repo_root, &staging, "kholidays", "kf6-kholidays")?,
        "kf6-kstatusnotifieritem" => stage_kde_module(
            repo_root,
            &staging,
            "kstatusnotifieritem",
            "kf6-kstatusnotifieritem",
        )?,
        "kf6-kxmlgui" => stage_kde_module(repo_root, &staging, "kxmlgui", "kf6-kxmlgui")?,
        "kf6-kconfigwidgets" => {
            stage_kde_module(repo_root, &staging, "kconfigwidgets", "kf6-kconfigwidgets")?
        }
        "kf6-kitemviews" => stage_kde_module(repo_root, &staging, "kitemviews", "kf6-kitemviews")?,
        "kf6-kbookmarks" => stage_kde_module(repo_root, &staging, "kbookmarks", "kf6-kbookmarks")?,
        "qt6-positioning" => {
            stage_qt_module(repo_root, &staging, "qtpositioning", "qt6-positioning")?
        }
        "kf6-kirigami" => stage_kde_module(repo_root, &staging, "kirigami", "kf6-kirigami")?,
        "kf6-qqc2-desktop-style" => stage_kde_module(
            repo_root,
            &staging,
            "qqc2-desktop-style",
            "kf6-qqc2-desktop-style",
        )?,
        "kf6-kirigami-addons" => stage_kde_module(
            repo_root,
            &staging,
            "kirigami-addons",
            "kf6-kirigami-addons",
        )?,
        "kf6-kquickcharts" => {
            stage_kde_module(repo_root, &staging, "kquickcharts", "kf6-kquickcharts")?
        }
        "libcanberra0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libcanberra",
            "libcanberra.so.0",
            "src/system/libraries/libcanberra/LGPL",
            "libcanberra0",
        )?,
        "plasma-activities" => stage_kde_module(
            repo_root,
            &staging,
            "plasma-activities",
            "plasma-activities",
        )?,
        "kactivitymanagerd" => stage_kde_module(
            repo_root,
            &staging,
            "kactivitymanagerd",
            "kactivitymanagerd",
        )?,
        "kglobalacceld" => stage_kde_module(repo_root, &staging, "kglobalacceld", "kglobalacceld")?,
        "plasma-activities-stats" => stage_kde_module(
            repo_root,
            &staging,
            "plasma-activities-stats",
            "plasma-activities-stats",
        )?,
        "libprocesscore10" => {
            stage_kde_module(repo_root, &staging, "ksysguard", "libprocesscore10")?
        }
        "kf6-kauth" => stage_kde_module(repo_root, &staging, "kauth", "kf6-kauth")?,
        "polkit-qt6-1" => stage_kde_module(repo_root, &staging, "polkit-qt-1", "polkit-qt6-1")?,
        "libyaml-cpp0.8" => stage_kde_module(repo_root, &staging, "yaml-cpp", "libyaml-cpp0.8")?,
        "libkpmcore13" => stage_kde_module(repo_root, &staging, "kpmcore", "libkpmcore13")?,
        "calamares" => stage_calamares(repo_root, &staging)?,
        "libxkbcommon0" => {
            for soname in [
                "libxkbcommon.so.0",
                "libxkbcommon-x11.so.0",
                "libxkbregistry.so.0",
            ] {
                stage_imported_soname_library(
                    repo_root,
                    &staging,
                    "xkbcommon",
                    soname,
                    // The imported tree omits the ignored aggregate LICENSE;
                    // retain the upstream README as the package notice.
                    "src/system/libraries/xkbcommon/README.md",
                    "libxkbcommon0",
                )?;
            }
        }
        "libxml2-16" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libxml2",
            "libxml2.so.16",
            "src/system/libraries/libxml2/Copyright",
            "libxml2-16",
        )?,
        "libxkbfile1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libxkbfile",
            "libxkbfile.so.1",
            "src/system/graphics/libxkbfile/COPYING",
            "libxkbfile1",
        )?,
        "libwayland-client0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "wayland",
            "libwayland-client.so.0",
            "src/system/libraries/wayland/COPYING",
            "libwayland-client0",
        )?,
        "libwayland-cursor0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "wayland",
            "libwayland-cursor.so.0",
            "src/system/libraries/wayland/COPYING",
            "libwayland-cursor0",
        )?,
        "libwayland-server0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "wayland",
            "libwayland-server.so.0",
            "src/system/libraries/wayland/COPYING",
            "libwayland-server0",
        )?,
        "libwayland-egl1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "wayland",
            "libwayland-egl.so.1",
            "src/system/libraries/wayland/COPYING",
            "libwayland-egl1",
        )?,
        "xkb-data" => stage_xkeyboard_config_data(repo_root, &staging)?,
        "libseat1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "seatd",
            "libseat.so.1",
            // The imported tree omits the ignored aggregate LICENSE;
            // retain the upstream README as the package notice.
            "src/system/libraries/seatd/README.md",
            "libseat1",
        )?,
        "libdisplay-info3" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libdisplay-info",
            "libdisplay-info.so.3",
            // The imported tree omits the ignored aggregate LICENSE;
            // retain the upstream README as the package notice.
            "src/system/libraries/libdisplay-info/README.md",
            "libdisplay-info3",
        )?,
        "libevdev2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libevdev",
            "libevdev.so.2",
            "src/system/libraries/libevdev/COPYING",
            "libevdev2",
        )?,
        "libinput10" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "libinput",
                "libinput.so.10",
                "src/system/libraries/libinput/COPYING",
                "libinput10",
            )?;
            copy_tree_preserving(
                &repo_root.join("out/build/libinput/install/usr/share/libinput"),
                &staging.join("usr/share/libinput"),
            )?;
        }
        "libpixman-1-0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "pixman",
            "libpixman-1.so.0",
            "src/system/libraries/pixman/COPYING",
            "libpixman-1-0",
        )?,
        "libdrm2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libdrm",
            "libdrm.so.2",
            "src/system/libraries/libdrm/README.rst",
            "libdrm2",
        )?,
        "libdrm-amdgpu1" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "libdrm",
                "libdrm_amdgpu.so.1",
                "src/system/libraries/libdrm/README.rst",
                "libdrm-amdgpu1",
            )?;
            stage_amdgpu_ids(&component_install(repo_root, "libdrm"), &staging)?;
        }
        "libdrm-nouveau2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libdrm",
            "libdrm_nouveau.so.2",
            "src/system/libraries/libdrm/README.rst",
            "libdrm-nouveau2",
        )?,
        "libxau6" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXau.so.6",
            "src/system/graphics/libxau/COPYING",
            "libxau6",
        )?,
        "libxdmcp6" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXdmcp.so.6",
            "src/system/graphics/libxdmcp/COPYING",
            "libxdmcp6",
        )?,
        "libice6" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libICE.so.6",
            "src/system/graphics/libice/COPYING",
            "libice6",
        )?,
        "libsm6" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libSM.so.6",
            "src/system/graphics/libsm/COPYING",
            "libsm6",
        )?,
        "libxi6" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXi.so.6",
            "src/system/graphics/libxi/COPYING",
            "libxi6",
        )?,
        "libxrender1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXrender.so.1",
            "src/system/graphics/libxrender/COPYING",
            "libxrender1",
        )?,
        "libxtst6" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXtst.so.6",
            "src/system/graphics/libxtst/COPYING",
            "libxtst6",
        )?,
        "libxcursor1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXcursor.so.1",
            "src/system/graphics/libxcursor/COPYING",
            "libxcursor1",
        )?,
        "libxft2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXft.so.2",
            "src/system/graphics/libxft/COPYING",
            "libxft2",
        )?,
        "libxcb1" => {
            for (component, soname) in [
                ("x11-compat", "libxcb.so.1"),
                ("x11-compat", "libxcb-xkb.so.1"),
                ("x11-compat", "libxcb-composite.so.0"),
                ("x11-compat", "libxcb-dri3.so.0"),
                ("x11-compat", "libxcb-dpms.so.0"),
                ("x11-compat", "libxcb-glx.so.0"),
                ("x11-compat", "libxcb-present.so.0"),
                ("x11-compat", "libxcb-randr.so.0"),
                ("x11-compat", "libxcb-render.so.0"),
                ("x11-compat", "libxcb-res.so.0"),
                ("x11-compat", "libxcb-shape.so.0"),
                ("x11-compat", "libxcb-shm.so.0"),
                ("x11-compat", "libxcb-sync.so.1"),
                ("x11-compat", "libxcb-xinput.so.0"),
                ("x11-compat", "libxcb-xfixes.so.0"),
                ("xcb-util", "libxcb-util.so.1"),
                ("xcb-renderutil", "libxcb-render-util.so.0"),
                ("xcb-image", "libxcb-image.so.0"),
                ("xcb-cursor", "libxcb-cursor.so.0"),
                ("xcb-util-wm", "libxcb-icccm.so.4"),
                ("xcb-keysyms", "libxcb-keysyms.so.1"),
            ] {
                stage_imported_soname_library(
                    repo_root,
                    &staging,
                    component,
                    soname,
                    "src/system/graphics/libxcb/COPYING",
                    "libxcb1",
                )?;
            }
        }
        "libx11-6" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "x11-compat",
                "libX11.so.6",
                "src/system/graphics/libx11/COPYING",
                "libx11-6",
            )?;
            stage_imported_soname_library(
                repo_root,
                &staging,
                "x11-compat",
                "libX11-xcb.so.1",
                "src/system/graphics/libx11/COPYING",
                "libx11-6",
            )?;
            copy_tree_preserving(
                &component_install(repo_root, "x11-compat").join("usr/share/X11/locale"),
                &staging.join("usr/share/X11/locale"),
            )?;
        }
        "libxext6" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXext.so.6",
            "src/system/graphics/libxext/COPYING",
            "libxext6",
        )?,
        "libxfixes3" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libXfixes.so.3",
            "src/system/graphics/libxfixes/COPYING",
            "libxfixes3",
        )?,
        "libglvnd0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libGLdispatch.so.0",
            "src/system/graphics/libglvnd/README.md",
            "libglvnd0",
        )?,
        "libglvnd-dev" => stage_libglvnd_dev(repo_root, &staging)?,
        "libglx0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libGLX.so.0",
            "src/system/graphics/libglvnd/README.md",
            "libglx0",
        )?,
        "libgl1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libGL.so.1",
            "src/system/graphics/libglvnd/README.md",
            "libgl1",
        )?,
        "libopengl0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libOpenGL.so.0",
            "src/system/graphics/libglvnd/README.md",
            "libopengl0",
        )?,
        "libgbm1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "mesa",
            "libgbm.so.1",
            "src/system/graphics/mesa/docs/license.rst",
            "libgbm1",
        )?,
        "libegl1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libEGL.so.1",
            "src/system/graphics/libglvnd/README.md",
            "libegl1",
        )?,
        "libgles1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libGLESv1_CM.so.1",
            "src/system/graphics/libglvnd/README.md",
            "libgles1",
        )?,
        "libgles2" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libGLESv2.so.2",
            "src/system/graphics/libglvnd/README.md",
            "libgles2",
        )?,
        "libegl-mesa0" => stage_mesa_egl_vendor(repo_root, &staging)?,
        "libgl1-mesa-dri" => stage_mesa_dri_runtime(repo_root, &staging)?,
        "libvulkan1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "vulkan-loader",
            "libvulkan.so.1",
            "src/system/graphics/vulkan-loader/LICENSE.txt",
            "libvulkan1",
        )?,
        "libvulkan-dev" => stage_vulkan_development(repo_root, &staging)?,
        "mesa-vulkan-drivers" => stage_mesa_vulkan_runtime(repo_root, &staging)?,
        "vulkan-tools" => stage_vulkan_tools(repo_root, &staging)?,
        "linux-modules-nvidia-595-open-7.2.0-rc5-mattos"
        | "nvidia-firmware-595"
        | "libnvidia-gl-595"
        | "libnvidia-compute-595"
        | "libnvidia-encode-595"
        | "libnvidia-decode-595"
        | "nvidia-utils-595"
        | "nvidia-driver-595-open" => stage_nvidia_package(repo_root, &staging, spec.name)?,
        "flatpak" => stage_flatpak(repo_root, &staging)?,
        "xwayland" => stage_xwayland(repo_root, &staging)?,
        "xdg-desktop-portal" => stage_xdg_desktop_portal(repo_root, &staging)?,
        "libduktape207" => stage_runtime_paths(
            repo_root,
            &staging,
            "duktape",
            &[
                "usr/lib/x86_64-linux-gnu/libduktape.so.207.2.7.0",
                "usr/lib/x86_64-linux-gnu/libduktape.so.207",
                "usr/lib/x86_64-linux-gnu/libduktape.so",
            ],
        )?,
        "polkit" => stage_runtime_paths(
            repo_root,
            &staging,
            "polkit",
            &[
                "usr/bin/pkcheck",
                "usr/bin/pkexec",
                "usr/lib/polkit-1/polkitd",
                "usr/lib/polkit-1/polkit-agent-helper-1",
                "usr/lib/systemd/system/polkit.service",
                "usr/lib/systemd/system/polkit-agent-helper.socket",
                "usr/lib/systemd/system/polkit-agent-helper@.service",
                "usr/lib/sysusers.d/polkit.conf",
                "usr/lib/tmpfiles.d/polkit-tmpfiles.conf",
                "usr/lib/x86_64-linux-gnu/libpolkit-agent-1.so",
                "usr/lib/x86_64-linux-gnu/libpolkit-agent-1.so.0",
                "usr/lib/x86_64-linux-gnu/libpolkit-agent-1.so.0.0.0",
                "usr/lib/x86_64-linux-gnu/libpolkit-gobject-1.so",
                "usr/lib/x86_64-linux-gnu/libpolkit-gobject-1.so.0",
                "usr/lib/x86_64-linux-gnu/libpolkit-gobject-1.so.0.0.0",
                "usr/share/dbus-1/system-services/org.freedesktop.PolicyKit1.service",
                "usr/share/dbus-1/system.d/org.freedesktop.PolicyKit1.conf",
                "usr/share/polkit-1/actions/org.freedesktop.policykit.policy",
                "usr/share/polkit-1/polkitd.conf",
                "usr/share/polkit-1/rules.d/50-default.rules",
            ],
        )
        .and_then(|()| {
            // Preserve the upstream setuid-root contracts for the command
            // front-end and authentication helper when copying the runtime
            // subset into the package.
            set_mode(staging.join("usr/bin/pkexec"), 0o4755)?;
            set_mode(
                staging.join("usr/lib/polkit-1/polkit-agent-helper-1"),
                0o4755,
            )
        })?,
        "network-manager" => stage_network_manager(repo_root, &staging)?,
        "libnl-3-200" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libnl",
            "libnl-3.so.200",
            "src/system/network/libnl/COPYING",
            "libnl-3-200",
        )?,
        "libnl-genl-3-200" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libnl",
            "libnl-genl-3.so.200",
            "src/system/network/libnl/COPYING",
            "libnl-genl-3-200",
        )?,
        "libnl-route-3-200" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libnl",
            "libnl-route-3.so.200",
            "src/system/network/libnl/COPYING",
            "libnl-route-3-200",
        )?,
        "wpasupplicant" => stage_wpa_supplicant(repo_root, &staging)?,
        "grub-efi-amd64" => stage_grub_package(repo_root, &staging)?,
        "mattos-cozy" => stage_cozy(repo_root, &staging)?,
        "libdbus-1-3" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "dbus",
                "libdbus-1.so.3",
                "src/system/dbus/dbus/COPYING",
                "libdbus-1-3",
            )?;
            // Retain the reference bus utilities for standalone sessions and
            // diagnostics. dbus-broker remains the sole systemd-managed
            // system/user daemon.
            stage_runtime_paths(
                repo_root,
                &staging,
                "dbus",
                &[
                    "usr/bin/dbus-daemon",
                    "usr/bin/dbus-run-session",
                    "usr/bin/dbus-update-activation-environment",
                ],
            )?;
            let reference_config =
                component_install(repo_root, "dbus").join("usr/share/dbus-1/session.conf");
            let private_config = fs::read_to_string(&reference_config)?
                .lines()
                .map(|line| {
                    if line.contains("<listen>") {
                        "  <listen>unix:tmpdir=/tmp</listen>"
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            if !private_config.contains("<listen>unix:tmpdir=/tmp</listen>") {
                bail!("private D-Bus session config is missing its runtime listen address");
            }
            let private_config_path = staging.join("usr/share/dbus-1/mattos-private-session.conf");
            fs::create_dir_all(
                private_config_path
                    .parent()
                    .expect("private D-Bus config parent"),
            )?;
            fs::write(private_config_path, private_config)?;
        }
        "libdav1d7" => stage_imported_soname_library(
            repo_root,
            &staging,
            "dav1d",
            "libdav1d.so.7",
            "src/system/multimedia/dav1d/COPYING",
            "libdav1d7",
        )?,
        "libglib2.0-0t64" => {
            stage_library_family(
                repo_root,
                &staging,
                "glib",
                &[
                    "libglib-2.0.so.0",
                    "libgobject-2.0.so.0",
                    "libgio-2.0.so.0",
                    "libgmodule-2.0.so.0",
                    "libgthread-2.0.so.0",
                ],
            )?;
            stage_runtime_paths(
                repo_root,
                &staging,
                "glib",
                &["usr/bin/glib-compile-schemas", "usr/bin/gio-querymodules"],
            )?;
            copy_preserving(
                &repo_root.join("src/system/libraries/glib/COPYING"),
                &staging.join("usr/share/doc/libglib2.0-0t64/copyright"),
            )?;
        }
        "pipewire" => stage_pipewire(repo_root, &staging)?,
        "libpython3.14" => {
            stage_library_family(repo_root, &staging, "cpython", &["libpython3.14.so.1.0"])?;
            copy_preserving(
                &repo_root.join("src/development/python/cpython/LICENSE"),
                &staging.join("usr/share/doc/libpython3.14/copyright"),
            )?;
        }
        "python3" => stage_cpython_runtime(repo_root, &staging)?,
        "python3-venv" => stage_cpython_venv(repo_root, &staging)?,
        "python3-dev" => stage_cpython_dev(repo_root, &staging)?,
        "libllvm22" => stage_llvm_runtime(repo_root, &staging)?,
        "llvm" => stage_llvm_tools(repo_root, &staging)?,
        "llvm-dev" => stage_llvm_development(repo_root, &staging)?,
        "clang" => stage_clang(repo_root, &staging)?,
        "lld" => stage_lld(repo_root, &staging)?,
        "rustc" => stage_rustc(repo_root, &staging)?,
        "cargo" => stage_cargo(repo_root, &staging)?,
        "openssh-client" => {
            stage_runtime_paths(
                repo_root,
                &staging,
                "openssh",
                &[
                    "usr/bin/ssh",
                    "usr/bin/scp",
                    "usr/bin/sftp",
                    "usr/bin/ssh-add",
                    "usr/bin/ssh-agent",
                    "usr/bin/ssh-keygen",
                    "usr/bin/ssh-keyscan",
                ],
            )?;
            copy_preserving(
                &repo_root.join("src/system/network/openssh/ssh_config"),
                &staging.join("etc/ssh/ssh_config"),
            )?;
            fs::write(staging.join("DEBIAN/conffiles"), "/etc/ssh/ssh_config\n")?;
        }
        "openssh-server" => stage_openssh_server(repo_root, &staging)?,
        "tar" => {
            stage_executable(
                &repo_root.join("out/build/tar/install/usr/bin/tar"),
                &staging.join("usr/bin/tar"),
                0o755,
            )?;
            copy_preserving(
                &repo_root.join("src/userland/tar/COPYING"),
                &staging.join("usr/share/doc/tar/copyright"),
            )?;
        }
        "dbus-broker" => stage_dbus_broker(repo_root, &staging)?,
        "libpam0g" => stage_library_family(
            repo_root,
            &staging,
            "linux-pam",
            &["libpam.so.0.85.1", "libpam.so.0"],
        )?,
        "mattos-libpam-misc0" => stage_library_family(
            repo_root,
            &staging,
            "linux-pam",
            &["libpam_misc.so.0.82.1", "libpam_misc.so.0"],
        )?,
        "libpam-modules" => stage_pam_modules(repo_root, &staging)?,
        "libpam-runtime" => stage_pam_runtime(repo_root, &staging)?,
        "passwd" => stage_shadow(repo_root, &staging)?,
        "mattos-sudo-rs" => stage_sudo_rs(repo_root, &staging)?,
        "login" => stage_util_linux_auth(repo_root, &staging)?,
        "iproute2" => stage_iproute2(repo_root, &staging)?,
        "iputils-ping" => {
            stage_runtime_paths(repo_root, &staging, "iputils", IPUTILS_RUNTIME_PATHS)?
        }
        "mattos-installer" => stage_mattos_installer(repo_root, &staging)?,
        "btrfs-progs" => copy_tree_preserving(
            &repo_root.join("out/build/btrfs-progs/install/usr"),
            &staging.join("usr"),
        )?,
        "dosfstools" => copy_tree_preserving(
            &repo_root.join("out/build/dosfstools/install/usr"),
            &staging.join("usr"),
        )?,
        "e2fsprogs" => {
            let install = repo_root.join("out/build/e2fsprogs/install");
            for relative in [
                "usr/bin",
                "usr/sbin",
                "usr/libexec",
                "usr/lib/x86_64-linux-gnu",
                "usr/share/man",
                "etc",
            ] {
                copy_tree_preserving(&install.join(relative), &staging.join(relative))?;
            }
            let libdir = staging.join("usr/lib/x86_64-linux-gnu");
            for entry in fs::read_dir(&libdir)? {
                let path = entry?.path();
                let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
                if name.ends_with(".a") || name.ends_with(".so") {
                    fs::remove_file(path)?;
                }
            }
        }
        _ => bail!("no staging implementation for {}", spec.name),
    }

    // Build-stage pkg-config files intentionally point at the disposable
    // staged prefix so later source builds can consume them.  The Debian
    // package is a target `/usr` view, however, and must not publish that
    // host-side prefix.  Normalize only the copied package view here; never
    // mutate the cache-owned build output.
    normalize_staged_pkgconfig_paths(repo_root, &staging)?;
    normalize_staged_cmake_paths(repo_root, &staging)?;
    normalize_staged_rust_paths(repo_root, &staging)?;

    // NVIDIA's redistribution grant requires its userspace binaries to remain
    // unmodified. Open modules are already compressed; preserve every file in
    // this separately versioned stack byte-for-byte after extraction.
    if !matches!(
        spec.name,
        "libc6"
            | "libgcc-s1"
            | "libgomp1"
            | "libstdc++6"
            | "linux-modules-nvidia-595-open-7.2.0-rc5-mattos"
            | "nvidia-firmware-595"
            | "libnvidia-gl-595"
            | "libnvidia-compute-595"
            | "libnvidia-encode-595"
            | "libnvidia-decode-595"
            | "nvidia-utils-595"
            | "nvidia-driver-595-open"
    ) {
        strip_staged_debug(repo_root, &staging)?;
    }

    // Every ordinary target package must be free of checkout/build-host
    // paths, not only development packages. Runtime code can retain
    // __FILE__-style paths from host-only headers just as easily as a
    // toolchain package can retain paths in debug metadata.
    validate_no_embedded_build_root(repo_root, &staging)?;

    let version = package_version(repo_root, spec)?;
    validate_debian_version(&version)?;
    let runtime_libraries = runtime_libraries_for_spec(repo_root, spec)?;
    write_provenance(repo_root, &staging, spec, &version, &runtime_libraries)?;
    if matches!(
        spec.name,
        "linux-libc-dev"
            | "libc6-dev"
            | "mattos-libgcc-dev"
            | "mattos-libstdc++-dev"
            | "binutils"
            | "mattos-gcc-common"
            | "cpp"
            | "gcc"
            | "g++"
            | "make"
    ) {
        validate_no_embedded_build_root(repo_root, &staging)?;
    }
    let installed_size = installed_size_kib(&staging)?;
    let dependencies = package_dependencies(repo_root, spec)?;
    let control = render_control(
        spec,
        &version,
        installed_size,
        &dependencies,
        &runtime_libraries,
    )?;
    fs::write(staging.join("DEBIAN/control"), control)?;
    normalize_package_modes(&staging)?;
    Ok(())
}

fn stage_mattos_compat(repo_root: &Path, staging: &Path) -> Result<()> {
    let status = Command::new("cargo")
        .current_dir(repo_root)
        .args(["build", "--release", "-p", "mattos-compat"])
        .status()
        .context("build mattos-compat")?;
    if !status.success() {
        bail!("cargo failed while building mattos-compat: {status}");
    }
    let binary = repo_root.join("target/release/mattos-compat");
    if !binary.is_file() {
        bail!("mattos-compat build did not produce {}", binary.display());
    }
    copy_preserving(&binary, &staging.join("usr/bin/mattos-compat"))?;
    let nspawn = repo_root.join("out/build/systemd/install/usr/bin/systemd-nspawn");
    if !nspawn.is_file() {
        bail!(
            "systemd-nspawn is missing from the systemd stage at {}; enable the nspawn component before building mattos-compat",
            nspawn.display()
        );
    }
    copy_preserving(&nspawn, &staging.join("usr/bin/systemd-nspawn"))?;
    Ok(())
}

fn stage_libffi_dev(repo_root: &Path, staging: &Path) -> Result<()> {
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

/// Stage only libglvnd's development interface. Runtime packages retain
/// ownership of every SONAME link and real shared object; downstream CMake,
/// qmake and pkg-config consumers need these unversioned linker symlinks.
fn stage_libglvnd_dev(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn normalize_staged_pkgconfig_paths(repo_root: &Path, staging: &Path) -> Result<()> {
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
        if normalized.contains(&build_root_text) {
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

fn normalize_staged_cmake_paths(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn normalize_staged_rust_paths(repo_root: &Path, staging: &Path) -> Result<()> {
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

/// Preserve one upstream Qt module's complete `/usr` prefix. Qt plugins,
/// tools and imported CMake targets reference sibling paths below that prefix,
/// so an arbitrary runtime/development split would make later KF6 consumers
/// fragile without reducing the required Qt foundation closure.
fn stage_qt_module(repo_root: &Path, staging: &Path, component: &str, package: &str) -> Result<()> {
    let install = component_install(repo_root, component).join("usr");
    if !install.is_dir() {
        bail!(
            "Qt package {package} requires staged {component} output at {}",
            install.display()
        );
    }
    // A Qt module's install prefix is normally self-contained.  The Qt
    // module build can, however, leave absolute links to sibling module
    // outputs in the shared plugin tree while standalone module configure
    // views are being assembled.  Those links are build-tree plumbing, not
    // payload owned by this package.  Copy the prefix while rejecting links
    // whose lexical target escapes the producing module's install prefix;
    // otherwise the first package would claim a sibling plugin and the
    // ownership audit would (correctly) reject the package set.
    copy_qt_prefix_without_foreign_links(&install, &staging.join("usr"))?;
    if component == "qtdeclarative" {
        // Qt Declarative installs QML test fixtures/plugins alongside the
        // runtime. They are not part of the target runtime package and some
        // embed the build host path; keep them available in the stage output
        // but exclude them from the shipped package.
        let tests = staging.join("usr/qml/Qt/test");
        if tests.exists() {
            fs::remove_dir_all(tests)?;
        }
        // QuickTestUtilsPrivate is a static test-support library, not part of
        // the QtQuick runtime or the development interface consumed by the
        // Plasma closure.  Qt builds it as an archive containing CMake's
        // disposable object-directory path; unlike ELF payloads it is not
        // processed by the debug-strip pass, so retaining it would publish a
        // host checkout path and fail the package boundary audit.  Exclude
        // the test-only archives and their qmake sidecars together with the
        // already-excluded QML fixtures.
        for name in [
            "libQt6QuickTestUtils.a",
            "libQt6QuickTestUtils.prl",
            "libQt6QuickControlsTestUtils.a",
            "libQt6QuickControlsTestUtils.prl",
        ] {
            let path = staging.join("usr/lib/x86_64-linux-gnu").join(name);
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
    }
    let source = repo_root.join("src/desktop/qt").join(component);
    let licenses = source.join("LICENSES");
    if !licenses.is_dir() {
        bail!(
            "Qt package {package} is missing retained upstream licenses at {}",
            licenses.display()
        );
    }
    copy_tree_preserving(
        &licenses,
        &staging.join("usr/share/doc").join(package).join("licenses"),
    )?;
    copy_preserving(
        &source.join("licenseRule.json"),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("licenseRule.json"),
    )
}

fn copy_qt_prefix_without_foreign_links(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(unix)]
    let mut hardlinks = BTreeMap::new();
    copy_qt_prefix_inner(source, destination, source, &mut hardlinks)
}

fn copy_qt_prefix_inner(
    source: &Path,
    destination: &Path,
    owner_prefix: &Path,
    hardlinks: &mut HardlinkMap,
) -> Result<()> {
    if !source.is_dir() {
        bail!(
            "required package input directory missing at {}",
            source.display()
        );
    }
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&from)?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&from)?;
            let resolved = if target.is_absolute() {
                target
            } else {
                from.parent().unwrap_or(owner_prefix).join(target)
            };
            if !resolved.starts_with(owner_prefix) {
                continue;
            }
        }
        if metadata.is_dir() {
            copy_qt_prefix_inner(&from, &to, owner_prefix, hardlinks)?;
        } else {
            copy_path_preserving_with_hardlinks(&from, &to, &metadata, hardlinks)?;
        }
    }
    Ok(())
}

fn stage_plasma_session_integration(repo_root: &Path, staging: &Path) -> Result<()> {
    let integration = repo_root.join("src/system/session/plasma");
    for (source, destination) in [
        (
            "plasma-desktop.conf",
            "usr/lib/environment.d/90-plasma-desktop.conf",
        ),
        (
            "mattos-graphics-watchdog.service",
            "usr/lib/systemd/system/mattos-graphics-watchdog.service",
        ),
        (
            "mattos-graphics-recovery.service",
            "usr/lib/systemd/system/mattos-graphics-recovery.service",
        ),
        (
            "mattos-graphics-capture.service",
            "usr/lib/systemd/system/mattos-graphics-capture.service",
        ),
    ] {
        copy_preserving(&integration.join(source), &staging.join(destination))?;
    }
    for name in ["mattos-graphics-report", "mattos-graphics-startup"] {
        copy_preserving(&integration.join(name), &staging.join("usr/bin").join(name))?;
        set_mode(staging.join("usr/bin").join(name), 0o755)?;
    }

    let wants = staging.join("etc/systemd/system/multi-user.target.wants");
    fs::create_dir_all(&wants)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        "/usr/lib/systemd/system/mattos-graphics-capture.service",
        wants.join("mattos-graphics-capture.service"),
    )?;

    for required in [
        "usr/bin/mattos-graphics-report",
        "usr/bin/mattos-graphics-startup",
        "usr/lib/systemd/system/mattos-graphics-watchdog.service",
        "usr/lib/environment.d/90-plasma-desktop.conf",
    ] {
        if fs::symlink_metadata(staging.join(required)).is_err() {
            bail!("plasma-desktop package is missing /{required}");
        }
    }
    Ok(())
}

fn stage_plasma_login_manager_integration(repo_root: &Path, staging: &Path) -> Result<()> {
    let policy = repo_root.join("src/system/session/plasma-login-manager");
    for (source, destination) in [
        ("plasmalogin.pam", "etc/pam.d/plasmalogin"),
        ("plasmalogin-greeter.pam", "etc/pam.d/plasmalogin-greeter"),
        (
            "plasmalogin-autologin.pam",
            "etc/pam.d/plasmalogin-autologin",
        ),
        (
            "mattos-plasma.desktop",
            "usr/share/wayland-sessions/mattos-plasma.desktop",
        ),
        ("../plasma/start-plasma", "usr/bin/start-plasma"),
    ] {
        copy_preserving(&policy.join(source), &staging.join(destination))?;
    }
    set_mode(staging.join("usr/bin/start-plasma"), 0o755)?;
    // Upstream PLM's KWin unit requests optional KWin features unconditionally.
    // MattOS deliberately builds KWin without its screen locker and Activities
    // integration, so those conditional CLI switches are not compiled into
    // our kwin_wayland binary. Remove only the switches for disabled features
    // from the package-owned unit; keep upstream source pristine and preserve
    // all supported options (notably the Wayland input method and locale1).
    let kwin_unit = staging.join("usr/lib/systemd/user/plasma-login-kwin_wayland.service");
    let unit_contents = fs::read_to_string(&kwin_unit).with_context(|| {
        format!(
            "Plasma Login Manager did not install its KWin user unit at {}",
            kwin_unit.display()
        )
    })?;
    let unit_contents = adapt_plasma_login_kwin_unit(&unit_contents)?;
    fs::write(&kwin_unit, unit_contents)?;
    // KWin opens the active seat's DRM card/render nodes as the dedicated
    // greeter account.  MattOS's installed users receive these groups from
    // the installer, but the system account generated by PLM otherwise has
    // only its private primary group and cannot open /dev/dri/card*.  Keep
    // the access scoped to the display-manager account and let systemd's
    // native sysusers mechanism apply it before the login-manager starts.
    let sysusers = staging.join("usr/lib/sysusers.d/plasmalogin.conf");
    let sysusers_contents = fs::read_to_string(&sysusers)?;
    fs::write(&sysusers, adapt_plasma_login_sysusers(&sysusers_contents)?)?;
    for required in [
        "usr/bin/plasmalogin",
        "usr/bin/start-plasma",
        "usr/lib/systemd/system/plasmalogin.service",
        "usr/lib/systemd/user/plasma-login-kwin_wayland.service",
        "usr/lib/sysusers.d/plasmalogin.conf",
        "usr/lib/tmpfiles.d/plasmalogin.conf",
        "etc/pam.d/plasmalogin",
        "etc/pam.d/plasmalogin-greeter",
        "etc/pam.d/plasmalogin-autologin",
        "usr/share/wayland-sessions/mattos-plasma.desktop",
    ] {
        if fs::symlink_metadata(staging.join(required)).is_err() {
            bail!("plasma-login-manager package is missing /{required}");
        }
    }
    let session =
        fs::read_to_string(staging.join("usr/share/wayland-sessions/mattos-plasma.desktop"))?;
    if !session.contains("Exec=/usr/bin/start-plasma") || session.contains("startplasma-x11") {
        bail!("MattOS Plasma Login Manager session must select start-plasma Wayland wrapper");
    }
    Ok(())
}

fn adapt_plasma_login_kwin_unit(contents: &str) -> Result<String> {
    const DISABLED_FEATURE_OPTIONS: [&str; 2] = ["--no-lockscreen", "--no-kactivities"];
    let mut exec_start_count = 0;
    let mut adjusted = String::with_capacity(contents.len());
    for line in contents.lines() {
        if let Some(command) = line.strip_prefix("ExecStart=") {
            exec_start_count += 1;
            if !command.contains("/kwin_wayland ") {
                bail!("Plasma Login Manager KWin unit does not launch kwin_wayland");
            }
            let mut adapted = command.to_owned();
            for option in DISABLED_FEATURE_OPTIONS {
                let occurrences = adapted
                    .split_whitespace()
                    .filter(|part| *part == option)
                    .count();
                if occurrences != 1 {
                    bail!(
                        "expected exactly one {option} in upstream Plasma Login Manager KWin unit; found {occurrences}"
                    );
                }
                adapted = adapted.replace(&format!(" {option}"), "");
            }
            adjusted.push_str("ExecStart=");
            adjusted.push_str(&adapted);
        } else {
            adjusted.push_str(line);
        }
        adjusted.push('\n');
    }
    if exec_start_count != 1 {
        bail!(
            "expected exactly one ExecStart in Plasma Login Manager KWin unit; found {exec_start_count}"
        );
    }
    if !adjusted.contains("--no-global-shortcuts")
        || !adjusted.contains("--inputmethod plasma-keyboard --locale1")
        || adjusted.contains("--no-lockscreen")
        || adjusted.contains("--no-kactivities")
    {
        bail!("adapted Plasma Login Manager KWin unit does not match MattOS KWin feature policy");
    }
    Ok(adjusted)
}

fn adapt_plasma_login_sysusers(contents: &str) -> Result<String> {
    let account_lines = contents
        .lines()
        .filter(|line| {
            let mut fields = line.split_whitespace();
            fields.next() == Some("u") && fields.next() == Some("plasmalogin")
        })
        .count();
    if account_lines != 1 {
        bail!("expected exactly one upstream plasmalogin sysusers account; found {account_lines}");
    }

    let mut output = contents.trim_end_matches('\n').to_owned();
    for group in ["video", "render"] {
        let expected = format!("m plasmalogin {group}");
        let memberships = contents
            .lines()
            .filter(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                fields.as_slice() == ["m", "plasmalogin", group]
            })
            .count();
        if memberships > 1 {
            bail!("duplicate plasmalogin membership for {group} in upstream sysusers file");
        }
        if memberships == 0 {
            output.push('\n');
            output.push_str(&expected);
        }
    }
    output.push('\n');
    Ok(output)
}

#[cfg(test)]
#[test]
fn plasma_login_kwin_unit_omits_disabled_kwin_feature_options_only() {
    let upstream = "[Service]\nExecStart=/usr/bin/kwin_wayland --no-lockscreen --no-global-shortcuts --no-kactivities --inputmethod plasma-keyboard --locale1\n";
    let adapted = adapt_plasma_login_kwin_unit(upstream).unwrap();
    assert_eq!(
        adapted,
        "[Service]\nExecStart=/usr/bin/kwin_wayland --no-global-shortcuts --inputmethod plasma-keyboard --locale1\n"
    );
    assert!(
        adapt_plasma_login_kwin_unit("[Service]\nExecStart=/usr/bin/kwin_wayland --locale1\n")
            .is_err()
    );
    assert!(
        adapt_plasma_login_kwin_unit(
            "[Service]\nExecStart=/usr/bin/kwin --no-lockscreen --no-kactivities\n"
        )
        .is_err()
    );
}

#[cfg(test)]
#[test]
fn plasma_login_greeter_receives_only_required_drm_device_groups() {
    let upstream =
        "# Generated by Plasma Login Manager\nu plasmalogin - \"Greeter\" /var/lib/plasmalogin -\n";
    let adapted = adapt_plasma_login_sysusers(upstream).unwrap();
    assert_eq!(
        adapted,
        "# Generated by Plasma Login Manager\nu plasmalogin - \"Greeter\" /var/lib/plasmalogin -\nm plasmalogin video\nm plasmalogin render\n"
    );
    assert_eq!(adapt_plasma_login_sysusers(&adapted).unwrap(), adapted);
    assert!(adapt_plasma_login_sysusers("u other - \"Greeter\" - -\n").is_err());
    assert!(
        adapt_plasma_login_sysusers(
            "u plasmalogin - \"Greeter\" - -\nm plasmalogin video\nm plasmalogin video\n"
        )
        .is_err()
    );
}

fn stage_plasma_live_session_integration(repo_root: &Path, staging: &Path) -> Result<()> {
    let integration = repo_root.join("src/system/session/plasma");
    for (source, destination) in [
        ("plasma-live.toml", "etc/greetd/plasma-live.toml"),
        ("plasma-greeter.pam", "etc/pam.d/plasma-greeter"),
        (
            "plasma-greeter.service",
            "usr/lib/systemd/system/plasma-greeter.service",
        ),
    ] {
        copy_preserving(&integration.join(source), &staging.join(destination))?;
    }
    for name in ["mattos-plasma-greeter"] {
        copy_preserving(&integration.join(name), &staging.join("usr/bin").join(name))?;
        set_mode(staging.join("usr/bin").join(name), 0o755)?;
    }
    let display_manager = staging.join("etc/systemd/system/display-manager.service");
    fs::create_dir_all(display_manager.parent().expect("display-manager parent"))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        "/usr/lib/systemd/system/plasma-greeter.service",
        &display_manager,
    )?;
    for required in [
        "etc/greetd/plasma-live.toml",
        "etc/pam.d/plasma-greeter",
        "etc/systemd/system/display-manager.service",
        "usr/bin/mattos-plasma-greeter",
        "usr/lib/systemd/system/plasma-greeter.service",
    ] {
        if fs::symlink_metadata(staging.join(required)).is_err() {
            bail!("mattos-plasma-live package is missing /{required}");
        }
    }
    Ok(())
}

const MATTOS_THEME_CONFIG_DIR: &str = "src/system/desktop/branding/MattOS";
const MATTOS_DESKTOP_DIRECTORY_OVERRIDES: &[&str] = &[
    "kf5-development.directory",
    "kf5-education.directory",
    "kf5-games.directory",
    "kf5-graphics.directory",
    "kf5-internet.directory",
    "kf5-multimedia.directory",
    "kf5-office.directory",
    "kf5-science.directory",
    "kf5-system.directory",
    "kf5-utilities.directory",
];

fn is_mattos_desktop_directory_override(relative: &Path) -> bool {
    relative.starts_with("share/desktop-directories")
        && relative
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| MATTOS_DESKTOP_DIRECTORY_OVERRIDES.contains(&name))
}

fn stage_mattos_plasma_theme(repo_root: &Path, staging: &Path) -> Result<()> {
    let branding = repo_root.join(MATTOS_THEME_CONFIG_DIR);
    let laf_root = staging.join("usr/share/plasma/look-and-feel/org.mattos.desktop");
    copy_preserving(
        &branding.join("metadata.json"),
        &laf_root.join("metadata.json"),
    )?;
    copy_tree_preserving(&branding.join("contents"), &laf_root.join("contents"))?;
    let layout_path = laf_root.join("contents/layouts/org.kde.plasma.desktop-layout.js");
    let layout_template = fs::read_to_string(&layout_path)?;
    let panel_config = fs::read_to_string(branding.join("panel-defaults.conf"))?;
    fs::write(
        &layout_path,
        render_mattos_panel_layout(&layout_template, &panel_config)?,
    )?;

    let theme_root = staging.join("usr/share/plasma/desktoptheme/Nordic");
    let nordic = repo_root.join("src/desktop/themes/nordic-kde");
    for relative in ["widgets", "icons", "dialogs"] {
        copy_tree_preserving(&nordic.join(relative), &theme_root.join(relative))?;
    }
    copy_preserving(
        &nordic.join("metadata.desktop"),
        &theme_root.join("metadata.desktop"),
    )?;
    copy_preserving(
        &nordic.join("colors"),
        &staging.join("usr/share/color-schemes/Nordic.colors"),
    )?;

    copy_tree_preserving(
        &repo_root.join("src/desktop/themes/papirus-icon-theme/Papirus"),
        &staging.join("usr/share/icons/Papirus"),
    )?;
    copy_tree_preserving(
        &repo_root.join("src/desktop/themes/papirus-icon-theme/Papirus-Dark"),
        &staging.join("usr/share/icons/Papirus-Dark"),
    )?;
    let papirus_dark_index = staging.join("usr/share/icons/Papirus-Dark/index.theme");
    let index_contents = fs::read_to_string(&papirus_dark_index)?;
    let patched_index = add_papirus_base_fallback(&index_contents)?;
    fs::write(&papirus_dark_index, patched_index)?;
    copy_tree_preserving(
        &repo_root.join("src/desktop/themes/utterly-round-aurorae/Utterly-Round-Dark"),
        &staging.join("usr/share/aurorae/themes/Utterly-Round-Dark"),
    )?;
    copy_tree_preserving(
        &repo_root
            .join("out/build/material-cursors/install/usr/share/icons/material_light_cursors"),
        &staging.join("usr/share/icons/material_light_cursors"),
    )?;

    for config in ["kdeglobals", "kcminputrc", "kwinrc", "plasmarc"] {
        copy_preserving(
            &branding.join("xdg").join(config),
            &staging.join("etc/xdg").join(config),
        )?;
    }
    copy_tree_preserving(
        &branding.join("desktop-directories"),
        &staging.join("usr/share/desktop-directories"),
    )?;

    let docs = staging.join("usr/share/doc/mattos-plasma-theme/upstream-licenses");
    for (source, destination, is_directory) in [
        ("src/desktop/themes/nordic-kde/LICENSE", "nordic-kde", true),
        (
            "src/desktop/themes/papirus-icon-theme/LICENSE",
            "papirus-icon-theme.LICENSE",
            false,
        ),
        (
            "src/desktop/themes/material-cursors/LICENSE",
            "material-cursors.LICENSE",
            false,
        ),
        (
            "src/desktop/themes/utterly-round-aurorae/Utterly-Round-Dark/LICENSE.md",
            "utterly-round-aurorae.LICENSE.md",
            false,
        ),
    ] {
        if is_directory {
            copy_tree_preserving(&repo_root.join(source), &docs.join(destination))?;
        } else {
            copy_preserving(&repo_root.join(source), &docs.join(destination))?;
        }
    }
    let notice = "MattOS-owned desktop defaults. Upstream source revisions are recorded in mattos-build-info.toml. Wallpaper image files are intentionally not included; the configured slideshow paths are stored in the Plasma layout.\n";
    fs::write(
        staging.join("usr/share/doc/mattos-plasma-theme/README.MattOS"),
        notice,
    )?;

    for required in [
        "usr/share/plasma/look-and-feel/org.mattos.desktop/metadata.json",
        "usr/share/plasma/look-and-feel/org.mattos.desktop/contents/layouts/org.kde.plasma.desktop-layout.js",
        "usr/share/plasma/desktoptheme/Nordic/metadata.desktop",
        "usr/share/color-schemes/Nordic.colors",
        "usr/share/icons/Papirus/index.theme",
        "usr/share/icons/Papirus-Dark/index.theme",
        "usr/share/icons/Papirus/24x24/apps/org.kde.dolphin.svg",
        "usr/share/icons/Papirus/24x24/apps/kate.svg",
        "usr/share/icons/Papirus/24x24/apps/utilities-terminal.svg",
        "usr/share/icons/material_light_cursors/index.theme",
        "usr/share/icons/material_light_cursors/cursors/left_ptr",
        "usr/share/aurorae/themes/Utterly-Round-Dark/metadata.json",
        "etc/xdg/kdeglobals",
        "etc/xdg/kcminputrc",
        "etc/xdg/kwinrc",
        "etc/xdg/plasmarc",
    ] {
        if !staging.join(required).is_file() {
            bail!("mattos-plasma-theme package is missing /{required}");
        }
    }
    if staging.join("usr/share/wallpapers").exists() {
        bail!("mattos-plasma-theme must not package wallpaper images");
    }
    Ok(())
}

fn add_papirus_base_fallback(index: &str) -> Result<String> {
    let original = "Inherits=breeze-dark,hicolor";
    if index.matches(original).count() != 1 {
        bail!("Papirus-Dark index.theme must contain exactly one known inheritance declaration");
    }
    Ok(index.replacen(original, "Inherits=Papirus,breeze-dark,hicolor", 1))
}

fn render_mattos_panel_layout(template: &str, config: &str) -> Result<String> {
    let mut values = BTreeMap::new();
    let mut section = String::new();
    for (line_number, line) in config.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        if section != "Panel" {
            bail!(
                "panel-defaults.conf has unsupported section on line {}",
                line_number + 1
            );
        }
        let (key, value) = line
            .split_once('=')
            .with_context(|| format!("invalid panel-defaults.conf line {}", line_number + 1))?;
        let key = key.trim();
        if !matches!(
            key,
            "floating" | "lengthMode" | "opacityMode" | "visibilityMode" | "thickness"
        ) {
            bail!("unknown MattOS panel default {key}");
        }
        if values
            .insert(key.to_string(), value.trim().to_string())
            .is_some()
        {
            bail!("duplicate MattOS panel default {key}");
        }
    }
    let get = |key: &str| {
        values
            .get(key)
            .with_context(|| format!("panel-defaults.conf is missing {key}"))
    };
    let floating = match get("floating")?.as_str() {
        "true" => "true",
        "false" => "false",
        other => bail!("invalid panel floating value {other:?}"),
    };
    let length_mode = match get("lengthMode")?.as_str() {
        "0" => "fill",
        "1" => "fit",
        "2" => "custom",
        other => bail!("invalid panel lengthMode value {other:?}"),
    };
    let opacity = match get("opacityMode")?.as_str() {
        "0" => "adaptive",
        "1" => "opaque",
        "2" => "translucent",
        other => bail!("invalid panel opacityMode value {other:?}"),
    };
    let hiding = match get("visibilityMode")?.as_str() {
        "0" => "none",
        "1" => "autohide",
        "2" => "dodgewindows",
        "3" => "windowsgobelow",
        other => bail!("invalid panel visibilityMode value {other:?}"),
    };
    let thickness = get("thickness")?
        .parse::<u16>()
        .context("panel thickness must be a positive integer")?;
    if !(24..=256).contains(&thickness) {
        bail!("panel thickness {thickness} is outside the supported 24..=256 range");
    }
    let rendered = template
        .replace("@@MATTOS_PANEL_FLOATING@@", floating)
        .replace("@@MATTOS_PANEL_LENGTH_MODE@@", length_mode)
        .replace("@@MATTOS_PANEL_OPACITY@@", opacity)
        .replace("@@MATTOS_PANEL_HIDING@@", hiding)
        .replace("@@MATTOS_PANEL_THICKNESS@@", &thickness.to_string());
    if rendered.contains("@@MATTOS_PANEL_") {
        bail!("unexpanded MattOS panel-defaults template token");
    }
    Ok(rendered)
}

fn stage_kde_module(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    package: &str,
) -> Result<()> {
    let install_root = component_install(repo_root, component);
    let install = install_root.join("usr");
    if !install.is_dir() {
        bail!(
            "KDE package {package} requires staged {component} output at {}",
            install.display()
        );
    }
    if component == "plasma-workspace" {
        // MattOS's Plasma appearance package owns these customized menu
        // categories. The upstream copy would otherwise claim the same paths.
        copy_tree_filtered(&install, &staging.join("usr"), &|relative, _| {
            !is_mattos_desktop_directory_override(relative)
        })?;
    } else {
        copy_tree_preserving(&install, &staging.join("usr"))?;
    }
    // ModemManagerQt's disposable SDK prefix is hydrated with the underlying
    // ModemManager C headers so isolated downstream CMake probes can resolve
    // its exported private include contract.  Those headers remain owned by
    // the modemmanager package and must not be duplicated in the Qt wrapper.
    if component == "modemmanager-qt" {
        let dependency_headers = staging.join("usr/include/ModemManager");
        if dependency_headers.exists() {
            fs::remove_dir_all(&dependency_headers)?;
        }
    }
    // Plasma's shell autostart desktop file is installed by KDE's
    // KDE_INSTALL_AUTOSTARTDIR, which is /etc/xdg/autostart in this target.
    // Keep it with the workspace package; omitting it leaves the classic
    // plasma_session fallback with KWin but no plasmashell.  The systemd-user
    // path remains the primary path, so this is also the correct fallback
    // payload rather than a package-specific launcher.
    if component == "plasma-workspace" {
        let autostart = install_root.join("etc/xdg/autostart");
        if autostart.is_dir() {
            copy_tree_preserving(&autostart, &staging.join("etc/xdg/autostart"))?;
        }
        // KService resolves the desktop application catalog through the
        // freedesktop menu definition selected by XDG_MENU_PREFIX=plasma-.
        // Without this non-/usr payload kbuildsycoca6 creates an effectively
        // empty cache and Kicker cannot discover installed applications.
        let menus = install_root.join("etc/xdg/menus");
        if !menus.join("plasma-applications.menu").is_file() {
            bail!("Plasma workspace output lacks etc/xdg/menus/plasma-applications.menu");
        }
        copy_tree_preserving(&menus, &staging.join("etc/xdg/menus"))?;
        // ECM's QtBinariesDir query is evaluated against the build prefix.
        // Normalize generated user units to the installed target path before
        // publication; a runtime unit must never retain a developer checkout
        // path or depend on a build-tree qdbus executable.
        let host_qdbus = repo_root
            .join("out/build/qtbase/install/usr/bin/qdbus")
            .display()
            .to_string();
        let user_units = staging.join("usr/lib/systemd/user");
        if user_units.is_dir() {
            for entry in fs::read_dir(&user_units)? {
                let path = entry?.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("service") {
                    continue;
                }
                let contents = fs::read_to_string(&path)?;
                let normalized = contents.replace(&host_qdbus, "/usr/bin/qdbus");
                if normalized != contents {
                    fs::write(path, normalized)?;
                }
            }
        }
    }
    // The calendar-event plugin is optional Plasma integration. It requires
    // the separately maintained KCalendarCore/libical stack, which is not in
    // the current shell closure; omit the complete optional family rather
    // than shipping an unusable ELF with an absent dependency.
    if matches!(component, "plasma-workspace" | "plasma-desktop") {
        remove_path_if_exists(
            &staging.join("usr/lib/x86_64-linux-gnu/plugins/plasmacalendarplugins"),
        )?;
    }
    if component == "breeze" {
        // The optional Breeze settings helper is a KDE Control Module host;
        // it is not part of the Wayland shell and would introduce a runtime
        // dependency on the not-yet-packaged KCMUtils closure.  Keep the
        // style/window-decoration payload while leaving that unrelated
        // utility out of the first-class Breeze runtime package.
        let settings = staging.join("usr/bin/breeze-settings6");
        if settings.exists() {
            fs::remove_file(settings)?;
        }
    }
    let source = match component {
        "kcoreaddons"
        | "ki18n"
        | "kwidgetsaddons"
        | "kconfig"
        | "kdbusaddons"
        | "kauth"
        | "layer-shell-qt"
        | "plasma-framework"
        | "krunner"
        | "kcrash"
        | "kwindowsystem"
        | "kpackage"
        | "karchive"
        | "kio"
        | "ksvg"
        | "knotifications"
        | "kguiaddons"
        | "kitemmodels"
        | "kglobalaccel"
        | "kiconthemes"
        | "kcolorscheme"
        | "kcompletion"
        | "kjobwidgets"
        | "kservice"
        | "solid"
        | "kcodecs"
        | "kdecoration"
        | "kidletime"
        | "kwayland"
        | "knighttime"
        | "kholidays"
        | "kstatusnotifieritem"
        | "kxmlgui"
        | "kconfigwidgets"
        | "kitemviews"
        | "kbookmarks"
        | "kirigami"
        | "breeze-icons"
        | "plasma-activities"
        | "kwin"
        | "plasma-workspace"
        | "plasma-desktop"
        | "breeze"
        | "plasma-activities-stats"
        | "kcmutils"
        | "ksysguard"
        | "knewstuff"
        | "attica"
        | "sonnet"
        | "kglobalacceld"
        | "kirigami-addons"
        | "kquickcharts" => repo_root.join("src/desktop/kde").join(component),
        "polkit-qt-1" => repo_root.join("src/system/security/polkit-qt-1"),
        "qca" => repo_root.join("src/system/security/qca"),
        "yaml-cpp" => repo_root.join("src/system/libraries/yaml-cpp"),
        "kpmcore" => repo_root.join("src/system/storage/kpmcore"),
        component if repo_root.join("src/desktop/kde").join(component).is_dir() => {
            // Keep the KDE source-root ownership rule generic for every
            // first-class KDE package.  The package registry still controls
            // which components can reach this helper; this guard only avoids
            // a second, drift-prone list of source directories.
            repo_root.join("src/desktop/kde").join(component)
        }
        _ => bail!("unknown KDE module source component {component}"),
    };
    let license = [
        "LICENSES",
        "LICENSE",
        "COPYING",
        "COPYING-ICONS",
        "COPYING.LIB",
    ]
    .iter()
    .map(|name| source.join(name))
    .find(|path| path.exists())
    // yaml-cpp's imported tree retains README.md while its ignored aggregate
    // LICENSE is omitted; use that retained upstream notice for the package.
    .or_else(|| {
        (component == "yaml-cpp")
            .then(|| source.join("README.md"))
            .filter(|path| path.exists())
    })
    .ok_or_else(|| anyhow!("KDE package {package} lacks a retained upstream license"))?;
    let destination = staging.join("usr/share/doc").join(package);
    if license.is_dir() {
        copy_tree_preserving(&license, &destination.join("licenses"))?;
    } else {
        copy_preserving(&license, &destination.join("copyright"))?;
    }
    Ok(())
}

fn stage_calamares(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "calamares").join("usr");
    if !install.join("bin/calamares").is_file() {
        bail!("Calamares package requires a completed calamares stage");
    }
    copy_tree_preserving(&install, &staging.join("usr"))?;
    // Upstream ships a generic desktop entry which runs `pkexec calamares`
    // directly.  That bypasses the MattOS launcher environment and creates a
    // second, unreliable installer entry beside the MattOS one.  Calamares
    // remains source-owned here; only its generic launcher is replaced with
    // the single MattOS-owned entry below.
    remove_path_if_exists(&staging.join("usr/share/applications/calamares.desktop"))?;
    let integration = repo_root.join("src/system/installer/calamares/mattos");
    // Calamares' normal, upstream configuration contract is
    // /etc/calamares.  Keeping the MattOS configuration there means the
    // packaged launcher does not depend on the debug-only/optional XDG
    // search mode (-X), and avoids having two configuration authorities.
    let config_root = staging.join("etc/calamares");
    copy_tree_preserving(&integration.join("branding"), &config_root.join("branding"))?;
    copy_preserving(
        &integration.join("settings.conf"),
        &config_root.join("settings.conf"),
    )?;
    copy_preserving(
        &integration.join("modules/partition.conf"),
        &config_root.join("modules/partition.conf"),
    )?;
    for module in ["mount", "fstab", "users", "locale", "keyboard"] {
        copy_preserving(
            &repo_root
                .join("src/system/installer/calamares/upstream/src/modules")
                .join(module)
                .join(format!("{module}.conf")),
            &config_root.join("modules").join(format!("{module}.conf")),
        )?;
    }
    copy_preserving(
        &integration.join("modules/users.conf"),
        &config_root.join("modules/users.conf"),
    )?;
    copy_preserving(
        &integration.join("modules/mattos-profile.conf"),
        &config_root.join("modules/mattos-profile.conf"),
    )?;
    copy_preserving(
        &integration.join("modules/mattos-finalize.conf"),
        &config_root.join("modules/mattos-finalize.conf"),
    )?;
    // Use the upstream shellprocess plugin as the narrow MattOS adapter
    // boundary.  The adapter is deliberately a separate executable so the
    // graphical frontend cannot silently invent a package list or bypass the
    // Rust installer policy.
    fs::write(
        config_root.join("modules/mattos-executor.conf"),
        "dontChroot: true\ntimeout: 3600\nverbose: true\nscript: /usr/libexec/mattos/calamares-target-executor --phase compose --profile ${gs[packagechooser_profile]} --target ${ROOT}\n",
    )?;
    let executor = staging.join("usr/libexec/mattos/calamares-target-executor");
    fs::create_dir_all(executor.parent().expect("executor parent"))?;
    fs::write(
        &executor,
        r#"#!/bin/sh
set -eu
if [ $# -ne 6 ] || [ "$1" != "--phase" ] || [ "$3" != "--profile" ] || [ "$5" != "--target" ]; then
  echo 'MattOS Calamares adapter: missing validated phase/profile/target from Calamares' >&2
  exit 64
fi
exec /usr/bin/mattos-install calamares --phase "$2" --profile "$4" --target "$6" 2>&1
"#,
    )?;
    set_mode(executor, 0o755)?;
    let launcher = staging.join("usr/bin/mattos-install-gui");
    fs::create_dir_all(launcher.parent().expect("launcher parent"))?;
    // Generate one canonical launcher payload after the policy/environment setup.
    // Keep the generated shell script free of Rust escape/continuation
    // artifacts: the final write is the canonical launcher payload.
    fs::write(
        &launcher,
        r#"#!/bin/sh
set -eu
export XDG_CONFIG_DIRS=/etc/xdg${XDG_CONFIG_DIRS:+:$XDG_CONFIG_DIRS}
export XDG_DATA_DIRS=/usr/share${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}
export QT_PLUGIN_PATH=/usr/lib/x86_64-linux-gnu/plugins:/usr/plugins${QT_PLUGIN_PATH:+:$QT_PLUGIN_PATH}
export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"
export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-wayland}"
# The live profile grants the logged-in installer user passwordless sudo.
# Use it for the GUI itself so the desktop launcher cannot strand Calamares
# behind an invisible/terminal-only Polkit authentication request.  Calamares
# still performs its privileged work through KPMCore's normal helpers.
exec /usr/bin/sudo -n /usr/bin/env \
XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-}" \
WAYLAND_DISPLAY="$WAYLAND_DISPLAY" \
DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-}" \
XDG_CONFIG_DIRS="$XDG_CONFIG_DIRS" \
XDG_DATA_DIRS="$XDG_DATA_DIRS" \
QT_PLUGIN_PATH="$QT_PLUGIN_PATH" \
QT_QPA_PLATFORM="$QT_QPA_PLATFORM" \
/usr/bin/calamares "$@"
"#,
    )?;
    set_mode(launcher, 0o755)?;
    let desktop = staging.join("usr/share/applications/calamares.desktop");
    fs::create_dir_all(desktop.parent().expect("desktop parent"))?;
    fs::write(
        desktop,
        "[Desktop Entry]\nType=Application\nVersion=1.0\nName=Install MattOS (Calamares)\nGenericName=MattOS Graphical Installer\nKeywords=calamares;MattOS;system;installer;\nTryExec=/usr/bin/mattos-install-gui\nExec=/usr/bin/mattos-install-gui\nComment=Install MattOS with the graphical Calamares installer\nIcon=calamares\nTerminal=false\nStartupNotify=true\nCategories=Qt;System;Settings;\nX-AppStream-Ignore=true\n",
    )?;
    for required in [
        "usr/bin/calamares",
        "usr/bin/mattos-install-gui",
        "usr/libexec/mattos/calamares-target-executor",
        "etc/calamares/settings.conf",
        "etc/calamares/modules/partition.conf",
        "etc/calamares/modules/mount.conf",
        "etc/calamares/modules/fstab.conf",
        "etc/calamares/modules/users.conf",
        "etc/calamares/modules/locale.conf",
        "etc/calamares/modules/keyboard.conf",
        "etc/calamares/modules/mattos-executor.conf",
        "etc/calamares/modules/mattos-finalize.conf",
        "etc/calamares/modules/mattos-profile.conf",
        "etc/calamares/branding/mattos/branding.desc",
        "usr/share/applications/calamares.desktop",
    ] {
        if !staging.join(required).exists() {
            bail!("Calamares package is missing /{required}");
        }
    }
    Ok(())
}

fn stage_multimedia_sdk(
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

fn stage_mattos_installer(repo_root: &Path, staging: &Path) -> Result<()> {
    let installer = repo_root.join("out/build/installer");
    stage_executable(
        &installer.join("cargo-target/release/mattos-install"),
        &staging.join("usr/bin/mattos-install"),
        0o755,
    )?;
    let assets = staging.join("usr/lib/mattos/installer");
    fs::create_dir_all(&assets)?;
    for (source, name) in [
        (
            repo_root.join("out/build/linux/build/arch/x86/boot/bzImage"),
            "vmlinuz",
        ),
        (
            repo_root.join("out/build/installed-initramfs.cpio.xz"),
            "installed-initramfs.cpio.xz",
        ),
        (installer.join("BOOTX64.EFI"), "BOOTX64.EFI"),
        (
            repo_root.join("out/build/linux/kernel-release"),
            "kernel-release",
        ),
    ] {
        copy_preserving(&source, &assets.join(name))?;
    }
    copy_preserving(
        &repo_root.join("src/system/installer/policy/example-plan.toml"),
        &staging.join("usr/share/doc/mattos-installer/example-plan.toml"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/installer/PROVENANCE.md"),
        &staging.join("usr/share/doc/mattos-installer/PROVENANCE.md"),
    )?;
    for name in ["mattos-install-cli.service", "mattos-install-cli.target"] {
        copy_preserving(
            &repo_root.join("src/system/units").join(name),
            &staging.join("usr/lib/systemd/system").join(name),
        )?;
    }
    Ok(())
}

fn stage_cpython_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn stage_cpython_venv(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn stage_cpython_dev(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn normalize_cpython_package_metadata(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn llvm_install(repo_root: &Path) -> PathBuf {
    component_install(repo_root, "llvm").join("usr")
}

fn stage_llvm_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn stage_llvm_tools(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn stage_llvm_development(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn stage_clang(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn stage_lld(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn rust_install(repo_root: &Path) -> PathBuf {
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

fn stage_filesystem(staging: &Path) -> Result<()> {
    for rel in [
        "usr/bin",
        "usr/sbin",
        "usr/lib",
        "usr/lib64",
        "usr/share",
        "usr/share/doc",
        "etc",
        "var",
        "var/lib",
        "home",
        "root",
        "run",
        "tmp",
    ] {
        fs::create_dir_all(staging.join(rel))?;
    }
    set_mode(staging.join("root"), 0o700)?;
    set_mode(staging.join("tmp"), 0o1777)?;
    #[cfg(unix)]
    for (link, target) in [
        ("bin", "usr/bin"),
        ("sbin", "usr/sbin"),
        ("lib", "usr/lib"),
        ("lib64", "usr/lib64"),
    ] {
        std::os::unix::fs::symlink(target, staging.join(link))?;
    }
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

fn stage_glibc_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn stage_glibc_utilities(repo_root: &Path, staging: &Path) -> Result<()> {
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
fn stage_glibc_locales(repo_root: &Path, staging: &Path) -> Result<()> {
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

/// Ship the pinned ISO-codes JSON contract required by locales-rs.  This is
/// source data, not a host locale database and is intentionally limited to
/// the three registries consumed by MattOS locale selection.
pub(crate) fn stage_iso_codes(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/iso-codes");
    let destination = staging.join("usr/share/iso-codes/json");
    fs::create_dir_all(&destination)?;
    for name in ["iso_3166-1.json", "iso_639-2.json", "iso_639-3.json"] {
        let path = source.join(name);
        if !path.is_file() {
            bail!("ISO-codes source is missing {name}");
        }
        copy_preserving(&path, &destination.join(name))?;
    }
    copy_preserving(
        &source.join("PROVENANCE.md"),
        &staging.join("usr/share/doc/iso-codes/PROVENANCE.md"),
    )?;
    Ok(())
}

/// Compile the pinned IANA database in an output-owned mirror and package
/// only the runtime zoneinfo tree; no host timezone files are consulted.
fn stage_tzdata(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/tzdata");
    let output = repo_root.join("out/build/tzdata");
    let build_source = output.join("source");
    let zoneinfo = output.join("zoneinfo");
    remove_path_if_exists(&output)?;
    sync_build_source(&source, &build_source)?;
    // LICENSE files are globally ignored in the MattOS source tree, while
    // tzcode's Makefile still lists one as a version-generation dependency.
    // Keep the authoritative import untouched and provide the source README
    // as the disposable build-tree notice instead; it records the upstream
    // public-domain/BSD licensing terms and is also suitable package notice
    // material when the ignored LICENSE file is unavailable.
    let license = build_source.join("LICENSE");
    if !license.is_file() {
        fs::copy(source.join("README"), &license)?;
    }
    fs::create_dir_all(&zoneinfo)?;
    run_cmd(&build_source, "make", &["zic"])?;
    let zic = build_source.join("zic");
    let destination = format!("-d{}", zoneinfo.display());
    run_cmd(
        &build_source,
        path_str(&zic)?,
        &[
            destination.as_str(),
            "africa",
            "antarctica",
            "asia",
            "australasia",
            "backward",
            "etcetera",
            "europe",
            "northamerica",
            "southamerica",
        ],
    )?;
    for file in ["zone.tab", "zone1970.tab", "iso3166.tab"] {
        copy_preserving(&source.join(file), &zoneinfo.join(file))?;
    }
    if !zoneinfo.join("Etc/UTC").is_file() || !zoneinfo.join("America/Los_Angeles").is_file() {
        bail!("pinned tzdata build did not produce canonical zoneinfo files")
    }
    copy_tree_preserving(&zoneinfo, &staging.join("usr/share/zoneinfo"))?;
    copy_preserving(&license, &staging.join("usr/share/doc/tzdata/copyright"))?;
    Ok(())
}

/// Stage the complete WHENCE-described firmware closure. Firmware is the
/// documented source-closure exception: the authoritative, pinned upstream
/// tree and its redistribution metadata are retained even though most payload
/// files are device bytecode rather than preferred-form source.
fn stage_linux_firmware(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/linux-firmware");
    let firmware = staging.join("usr/lib/firmware");
    if !source.join("WHENCE").is_file() || !source.join("copy-firmware.sh").is_file() {
        bail!("pinned linux-firmware source is missing WHENCE or its installer")
    }
    fs::create_dir_all(&firmware)?;
    run_cmd(
        &source,
        "sh",
        &["./copy-firmware.sh", "--zstd", path_str(&firmware)?],
    )?;
    if !firmware.join("intel").is_dir()
        || !firmware.join("amdgpu").is_dir()
        || !firmware
            .join("intel/iwlwifi/iwlwifi-so-a0-gf-a0-83.ucode.zst")
            .is_file()
    {
        bail!("linux-firmware staging lacks broad Intel/AMD firmware coverage")
    }
    let documentation = staging.join("usr/share/doc/linux-firmware");
    for name in ["WHENCE", "README.md"] {
        copy_preserving(&source.join(name), &documentation.join(name))?;
    }
    // LICENSE is globally ignored by the MattOS source tree. Preserve the
    // upstream licensing notice at the package boundary using README.md when
    // the ignored aggregate file is unavailable; individual LICENSES files
    // below remain authoritative for firmware-specific terms.
    let license = source.join("LICENSE");
    if license.is_file() {
        copy_preserving(&license, &documentation.join("LICENSE"))?;
    } else {
        copy_preserving(&source.join("README.md"), &documentation.join("LICENSE"))?;
    }
    copy_tree_preserving(&source.join("LICENSES"), &documentation.join("LICENSES"))?;
    for entry in fs::read_dir(&source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.path().is_file() && (name.starts_with("LICENCE.") || name.starts_with("LICENSE."))
        {
            copy_preserving(&entry.path(), &documentation.join(name.as_ref()))?;
        }
    }
    Ok(())
}

/// Regenerate the canonical database in an output-owned directory and require
/// it to match the signed upstream artifact before pairing it with that
/// artifact's detached signature and public redistribution metadata.
pub(crate) fn stage_wireless_regdb(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/wireless-regdb");
    let output = repo_root.join("out/build/wireless-regdb");
    remove_path_if_exists(&output)?;
    fs::create_dir_all(&output)?;
    run_cmd(
        &source,
        "python3",
        &[
            "db2fw.py",
            path_str(&output.join("regulatory.db"))?,
            "db.txt",
        ],
    )?;
    let generated = fs::read(output.join("regulatory.db"))?;
    let signed_payload = fs::read(source.join("regulatory.db"))?;
    if generated != signed_payload {
        bail!("generated wireless regulatory database does not match the pinned signed artifact")
    }
    let firmware = staging.join("usr/lib/firmware");
    copy_preserving(
        &output.join("regulatory.db"),
        &firmware.join("regulatory.db"),
    )?;
    copy_preserving(
        &source.join("regulatory.db.p7s"),
        &firmware.join("regulatory.db.p7s"),
    )?;
    let documentation = staging.join("usr/share/doc/wireless-regdb");
    let license = source.join("LICENSE");
    if license.is_file() {
        copy_preserving(&license, &documentation.join("copyright"))?;
    } else {
        // LICENSE files are globally ignored by the MattOS source tree; the
        // pinned upstream README is the retained licensing notice.
        copy_preserving(&source.join("README"), &documentation.join("copyright"))?;
    }
    copy_preserving(&source.join("db.txt"), &documentation.join("db.txt"))?;
    copy_preserving(
        &source.join("wens.key.pub.pem"),
        &documentation.join("wens.key.pub.pem"),
    )?;
    Ok(())
}

pub(crate) fn stage_brush(repo_root: &Path, staging: &Path) -> Result<()> {
    let bin_dir = staging.join("usr/bin");
    stage_executable(
        &repo_root.join("out/build/brush/cargo-target/release/brush"),
        &bin_dir.join("brush"),
        0o755,
    )?;
    #[cfg(unix)]
    for alias in ["sh", "bash"] {
        std::os::unix::fs::symlink("brush", bin_dir.join(alias))?;
    }
    Ok(())
}

fn stage_gcc_runtime_library(
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

fn stage_base_files(repo_root: &Path, staging: &Path) -> Result<()> {
    let skeleton = repo_root.join("src/rootfs/skeleton/etc");
    for name in ["os-release", "hostname", "profile", "shells"] {
        copy_preserving(&skeleton.join(name), &staging.join("etc").join(name))?;
    }
    let user_skeleton = skeleton.join("skel");
    if user_skeleton.is_dir() {
        // /etc/skel is the single packaged source for defaults inherited by
        // installed users and explicitly mirrored into the pre-created live
        // account. Do not leave its contents as unowned rootfs overlay data.
        copy_tree_preserving(&user_skeleton, &staging.join("etc/skel"))?;
    }
    copy_preserving(
        &repo_root.join("src/system/packages/config/base-files/environment"),
        &staging.join("etc/environment"),
    )?;
    let config = repo_root.join("src/system/packages/config/base-files");
    copy_preserving(&config.join("issue"), &staging.join("etc/issue"))?;
    let conffiles = [
        "/etc/hostname",
        "/etc/profile",
        "/etc/shells",
        "/etc/issue",
        "/etc/environment",
    ];
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", conffiles.join("\n")),
    )?;
    Ok(())
}

fn stage_systemd_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "systemd");
    copy_tree_filtered(&install, staging, &|relative, _| {
        let path = relative.to_string_lossy();
        !path.starts_with("usr/include/")
            && !path.starts_with("usr/lib/x86_64-linux-gnu/pkgconfig/")
            && !path.starts_with("usr/share/pkgconfig/")
            && !path.starts_with("usr/lib/udev/hwdb.d/")
            && path != UDEV_HWDB_UNIT_REL
            && path != UDEV_HWDB_WANTS_REL
            && path != "usr/lib/udev/hwdb.bin"
            // mattos-compat deliberately owns the nspawn executable that it
            // wraps.  Keep the base systemd package disjoint so installing
            // both packages never creates a duplicate path owner.
            && path != "usr/bin/systemd-nspawn"
            // MattOS's libpam-runtime package owns the effective systemd-user
            // stack in /etc.  Do not also ship systemd's optional vendor
            // fallback, whose module closure is intentionally broader.
            && path != "usr/lib/pam.d/systemd-user"
            && !matches!(
                path.as_ref(),
                "usr/lib/x86_64-linux-gnu/libsystemd.so"
                    | "usr/lib/x86_64-linux-gnu/libsystemd.so.0"
                    | "usr/lib/x86_64-linux-gnu/libsystemd.so.0.44.0"
                    | "usr/lib/x86_64-linux-gnu/libudev.so"
                    | "usr/lib/x86_64-linux-gnu/libudev.so.1"
                    | "usr/lib/x86_64-linux-gnu/libudev.so.1.7.14"
            )
    })?;
    copy_preserving(
        &repo_root.join("src/system/systemd/LICENSE.LGPL2.1"),
        &staging.join("usr/share/doc/systemd/copyright"),
    )
}

fn stage_mattos_base_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    for (source, destination) in [
        ("out/build/grep/cargo-target/release/grep", "usr/bin/grep"),
        ("out/build/sed/cargo-target/release/sed", "usr/bin/sed"),
        (
            "out/build/findutils/cargo-target/release/find",
            "usr/bin/find",
        ),
        (
            "out/build/findutils/cargo-target/release/xargs",
            "usr/bin/xargs",
        ),
        (
            "out/build/findutils/cargo-target/release/locate",
            "usr/bin/locate",
        ),
        (
            "out/build/findutils/cargo-target/release/updatedb",
            "usr/bin/updatedb",
        ),
        (
            "out/build/diffutils/cargo-target/release/diffutils",
            "usr/bin/diffutils",
        ),
        (
            "target/release/mattos-init",
            "usr/libexec/mattos/rescue-init",
        ),
    ] {
        stage_executable(&repo_root.join(source), &staging.join(destination), 0o755)?;
    }
    for alias in ["diff", "cmp"] {
        std::os::unix::fs::symlink("diffutils", staging.join("usr/bin").join(alias))?;
    }
    for relative in [
        "usr/libexec/mattos/brush-login",
        "usr/libexec/mattos/validate-shell-env",
    ] {
        copy_preserving(
            &repo_root.join("src/rootfs/skeleton").join(relative),
            &staging.join(relative),
        )?;
        set_mode(staging.join(relative), 0o755)?;
    }
    for unit in [
        "mattos.target",
        "mattos-shell.service",
        "mattos-smoke.service",
    ] {
        copy_preserving(
            &repo_root.join("src/system/units").join(unit),
            &staging.join("usr/lib/systemd/system").join(unit),
        )?;
    }
    for (source, destination) in [
        (
            "src/system/network/resolved.conf",
            "etc/systemd/resolved.conf.d/10-mattos.conf",
        ),
        (
            "src/system/network/timesyncd.conf",
            "etc/systemd/timesyncd.conf.d/10-mattos.conf",
        ),
        ("src/system/network/nsswitch.conf", "etc/nsswitch.conf"),
        ("src/system/network/hosts", "etc/hosts"),
        ("src/system/network/networks", "etc/networks"),
        (
            "src/system/network/99-mattos-network.conf",
            "etc/sysctl.d/99-mattos-network.conf",
        ),
    ] {
        copy_preserving(&repo_root.join(source), &staging.join(destination))?;
    }
    Ok(())
}

const PLASMA_PROFILE_POSTINST: &str = "#!/bin/sh\nset -e\n[ -n \"${DPKG_ROOT:-}\" ] && exit 0\nif command -v systemctl >/dev/null 2>&1; then\n    if [ \"$(readlink /etc/systemd/system/display-manager.service 2>/dev/null || true)\" = \"/usr/lib/systemd/system/plasma-greeter.service\" ]; then rm /etc/systemd/system/display-manager.service; fi\n    systemctl enable plasmalogin.service >/dev/null\n    systemctl set-default graphical.target >/dev/null\nfi\n";

fn stage_profile_package(repo_root: &Path, staging: &Path, package: &str) -> Result<()> {
    let profile = package.strip_prefix("mattos-").unwrap_or(package);
    if matches!(profile, "base" | "cli" | "plasma") {
        copy_preserving(
            &repo_root
                .join("src/system/packages/profiles")
                .join(format!("{profile}.toml")),
            &staging
                .join("usr/share/mattos/install-profiles")
                .join(format!("{profile}.toml")),
        )?;
    }
    if package == "mattos-plasma" {
        // Installing the desktop meta-package onto an existing CLI system is
        // a supported profile transition.  Offline installer composition
        // sets DPKG_ROOT and performs target activation itself; a normal APT
        // transaction should make the newly installed graphical profile the
        // next boot's default and enable its display-session service.
        let postinst = staging.join("DEBIAN/postinst");
        fs::write(&postinst, PLASMA_PROFILE_POSTINST)?;
        set_mode(postinst, 0o755)?;
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn plasma_profile_package_activates_graphical_target_only_on_real_systems() {
    let source = include_str!("../../../../system/packages/profiles/plasma.toml");
    assert!(source.contains("meta_package = \"mattos-plasma\""));
    assert!(PLASMA_PROFILE_POSTINST.contains("DPKG_ROOT"));
    assert!(PLASMA_PROFILE_POSTINST.contains("systemctl enable plasmalogin.service"));
    assert!(PLASMA_PROFILE_POSTINST.contains("display-manager.service"));
    assert!(PLASMA_PROFILE_POSTINST.contains("systemctl set-default graphical.target"));
}

pub(crate) fn stage_ca_certificates(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/network");
    let bundle = source.join("ca-certificates.crt");
    let metadata = source.join("ca-bundle.toml");
    let parsed: toml::Value = toml::from_str(&fs::read_to_string(&metadata)?)?;
    let expected_sha = parsed
        .get("sha256")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| anyhow!("CA bundle metadata lacks sha256"))?;
    if sha256_file(&bundle)? != expected_sha {
        bail!("pinned CA bundle checksum does not match ca-bundle.toml")
    }
    let expected_count = parsed
        .get("certificate_count")
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| anyhow!("CA bundle metadata lacks certificate_count"))?;
    let actual_count = fs::read_to_string(&bundle)?
        .matches("-----BEGIN CERTIFICATE-----")
        .count() as i64;
    if actual_count != expected_count {
        bail!("pinned CA bundle contains {actual_count} certificates; expected {expected_count}")
    }
    copy_preserving(&bundle, &staging.join("etc/ssl/certs/ca-certificates.crt"))?;
    let openssl_default = staging.join("etc/ssl/cert.pem");
    if let Some(parent) = openssl_default.parent() {
        fs::create_dir_all(parent)?;
    }
    std::os::unix::fs::symlink("certs/ca-certificates.crt", &openssl_default)?;
    copy_preserving(
        &metadata,
        &staging.join("usr/share/doc/ca-certificates/ca-bundle.toml"),
    )?;
    fs::write(
        staging.join("usr/share/doc/ca-certificates/UPDATE.md"),
        "Update only by replacing the pinned curl CA Extract input, then update the source URL, Mozilla data timestamp, SHA-256, certificate count, and MPL-2.0 license metadata in ca-bundle.toml. Ordinary builds never download a mutable CA bundle.\n",
    )?;
    Ok(())
}

fn stage_dpkg(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/dpkg/install");
    for rel in DPKG_RUNTIME_PATHS {
        stage_executable(&install.join(rel), &staging.join(rel), 0o755)?;
    }
    copy_tree_preserving(
        &install.join("usr/share/dpkg"),
        &staging.join("usr/share/dpkg"),
    )?;
    fs::create_dir_all(staging.join("etc/dpkg/dpkg.cfg.d"))?;
    fs::create_dir_all(staging.join("etc/alternatives"))?;
    fs::create_dir_all(staging.join("var/lib/dpkg/alternatives"))?;
    copy_preserving(
        &repo_root.join("src/system/packages/config/dpkg/dpkg.cfg"),
        &staging.join("etc/dpkg/dpkg.cfg"),
    )?;
    fs::write(staging.join("DEBIAN/conffiles"), "/etc/dpkg/dpkg.cfg\n")?;
    validate_no_mutable_package_state(staging)
}

fn stage_libapt_pkg(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("out/build/apt/install/usr/lib/x86_64-linux-gnu");
    let destination = staging.join("usr/lib/x86_64-linux-gnu");
    for name in ["libapt-pkg.so.7.0.0", "libapt-pkg.so.7.0", "libapt-pkg.so"] {
        copy_path_preserving(&source.join(name), &destination.join(name))?;
    }
    Ok(())
}

fn stage_apt(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/apt/install");
    for rel in APT_RUNTIME_PATHS {
        stage_executable(&install.join(rel), &staging.join(rel), 0o755)?;
    }
    for rel in ["usr/lib/apt/planners", "usr/lib/apt/solvers"] {
        copy_tree_preserving(&install.join(rel), &staging.join(rel))?;
    }
    let libdir = "usr/lib/x86_64-linux-gnu";
    for name in ["libapt-private.so.0.0.0", "libapt-private.so.0.0"] {
        copy_path_preserving(
            &install.join(libdir).join(name),
            &staging.join(libdir).join(name),
        )?;
    }
    let config = repo_root.join("src/system/packages/config/apt");
    copy_preserving(
        &config.join("mattos.sources"),
        &staging.join("etc/apt/sources.list.d/mattos.sources"),
    )?;
    copy_preserving(
        &config.join("01mattos"),
        &staging.join("etc/apt/apt.conf.d/01mattos"),
    )?;
    for name in ["mattos-hosted.sources", "debian-trixie.sources"] {
        copy_preserving(
            &config.join(name),
            &staging.join("etc/apt/sources.list.d").join(name),
        )?;
    }
    copy_preserving(
        &config.join("00mattos-priority"),
        &staging.join("etc/apt/preferences.d/00mattos-priority"),
    )?;
    let installed_config = config.join("installed");
    for name in [
        "01mattos",
        "00mattos-priority",
        "00-mattos-local.sources",
        "mattos-hosted.sources",
        "debian-trixie.sources",
    ] {
        copy_preserving(
            &installed_config.join(name),
            &staging.join("usr/share/mattos/apt/installed").join(name),
        )?;
    }
    for name in ["mattos-archive-keyring.asc", "debian-archive-keyring.asc"] {
        copy_preserving(
            &config.join("keys").join(name),
            &staging.join("usr/share/keyrings").join(name),
        )?;
    }
    let resources = repo_root.join("src/system/packages/config/apt/units");
    for unit in [
        "mattos-apt-daily.service",
        "mattos-apt-daily.timer",
        "mattos-apt-bootstrap.service",
        "mattos-apt-bootstrap.timer",
    ] {
        copy_preserving(
            &resources.join(unit),
            &staging.join("usr/lib/systemd/system").join(unit),
        )?;
    }
    for rel in [
        "etc/apt/auth.conf.d",
        "etc/apt/preferences.d",
        "etc/apt/trusted.gpg.d",
        "var/lib/apt/lists/partial",
        "var/cache/apt/archives/partial",
        "var/log/apt",
    ] {
        fs::create_dir_all(staging.join(rel))?;
    }
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", APT_CONFFILES.join("\n")),
    )?;
    validate_no_mutable_package_state(staging)
}

/// Apply the deliberately offline/live APT policy after package installation.
/// The apt package itself carries the installed policy templates; this overlay
/// makes the ISO root deterministic without relying on network availability.
pub(crate) fn apply_live_apt_policy(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let config = repo_root.join("src/system/packages/config/apt");
    copy_preserving(
        &config.join("01mattos"),
        &rootfs.join("etc/apt/apt.conf.d/01mattos"),
    )?;
    copy_preserving(
        &config.join("mattos.sources"),
        &rootfs.join("etc/apt/sources.list.d/mattos.sources"),
    )?;
    for name in ["mattos-hosted.sources", "debian-trixie.sources"] {
        copy_preserving(
            &config.join(name),
            &rootfs.join("etc/apt/sources.list.d").join(name),
        )?;
    }
    copy_preserving(
        &config.join("00mattos-priority"),
        &rootfs.join("etc/apt/preferences.d/00mattos-priority"),
    )?;
    validate_live_apt_policy(rootfs)
}

pub(crate) fn validate_live_apt_policy(rootfs: &Path) -> Result<()> {
    let local = fs::read_to_string(rootfs.join("etc/apt/sources.list.d/mattos.sources"))?;
    let hosted = fs::read_to_string(rootfs.join("etc/apt/sources.list.d/mattos-hosted.sources"))?;
    let debian = fs::read_to_string(rootfs.join("etc/apt/sources.list.d/debian-trixie.sources"))?;
    let preferences = fs::read_to_string(rootfs.join("etc/apt/preferences.d/00mattos-priority"))?;
    let keyrings = rootfs.join("usr/share/keyrings");
    if !local.contains("URIs: file:/usr/share/mattos/repository")
        || local.contains("Enabled: no")
        || !local.contains("Trusted: yes")
        || !hosted.contains("Enabled: yes")
        || !debian.contains("Enabled: no")
        || !hosted.contains("Signed-By: /usr/share/keyrings/mattos-archive-keyring.asc")
        || !debian.contains("Signed-By: /usr/share/keyrings/debian-archive-keyring.asc")
        || !keyrings.join("mattos-archive-keyring.asc").is_file()
        || !keyrings.join("debian-archive-keyring.asc").is_file()
        || !rootfs.join("usr/bin/gpgv").is_file()
        || fs::symlink_metadata(
            rootfs.join("etc/systemd/system/timers.target.wants/mattos-apt-daily.timer"),
        )
        .is_ok()
        || !preferences.contains("Pin-Priority: 990")
    {
        bail!("live APT policy does not enable both local and hosted MattOS repositories")
    }
    Ok(())
}

pub(crate) fn component_install(repo_root: &Path, component: &str) -> PathBuf {
    repo_root.join("out/build").join(component).join("install")
}

pub(crate) fn stage_udev_hwdb(repo_root: &Path, staging: &Path) -> Result<()> {
    let systemd_install = component_install(repo_root, "systemd");

    // Meson converts the authoritative imported systemd hwdb inputs into the
    // exact vendor-source closure selected by this pinned systemd revision.
    // Stage that closure into the package-owned output tree, then compile the
    // binary database there. The imported systemd tree is never writable.
    copy_tree_preserving(
        &systemd_install.join(UDEV_HWDB_SOURCE_REL),
        &staging.join(UDEV_HWDB_SOURCE_REL),
    )?;
    for rel in [UDEV_HWDB_UNIT_REL, UDEV_HWDB_WANTS_REL] {
        copy_path_preserving(&systemd_install.join(rel), &staging.join(rel))?;
    }
    generate_udev_hwdb(repo_root, staging)?;
    copy_preserving(
        &repo_root.join("src/system/systemd/LICENSE.LGPL2.1"),
        &staging.join("usr/share/doc/udev/copyright"),
    )?;
    validate_udev_hwdb_payload(repo_root, staging)
}

fn generate_udev_hwdb(repo_root: &Path, root: &Path) -> Result<()> {
    let glibc_install = component_install(repo_root, "glibc");
    let systemd_install = component_install(repo_root, "systemd");
    let loader = glibc_install.join("lib64/ld-linux-x86-64.so.2");
    let generator = systemd_install.join("usr/bin/systemd-hwdb");
    let library_path = std::env::join_paths([
        glibc_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu/systemd"),
    ])?;
    let root_arg = format!("--root={}", root.display());
    let loader_text = path_str(&loader)?;
    let generator_text = path_str(&generator)?;
    let library_path_text = library_path
        .to_str()
        .ok_or_else(|| anyhow!("systemd-hwdb library path is not valid UTF-8"))?;
    run_cmd(
        repo_root,
        loader_text,
        &[
            "--library-path",
            library_path_text,
            generator_text,
            &root_arg,
            "--usr",
            "--strict",
            "update",
        ],
    )
}

pub(crate) fn validate_udev_hwdb_payload(repo_root: &Path, root: &Path) -> Result<()> {
    let database = root.join(UDEV_HWDB_BINARY_REL);
    let bytes = fs::read(&database)
        .with_context(|| format!("prebuilt udev hwdb is missing at {}", database.display()))?;
    if bytes.len() < 1_000_000 || !bytes.starts_with(b"KSLPHHRH") {
        bail!("prebuilt udev hwdb has an invalid header or implausible size")
    }
    if path_entry_exists(&root.join("etc/udev/hwdb.bin")) {
        bail!("mutable /etc/udev/hwdb.bin must not be baked into the udev package")
    }

    let systemd_install = component_install(repo_root, "systemd");
    let glibc_install = component_install(repo_root, "glibc");
    let loader = glibc_install.join("lib64/ld-linux-x86-64.so.2");
    let generator = systemd_install.join("usr/bin/systemd-hwdb");
    let library_path = std::env::join_paths([
        glibc_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu/systemd"),
    ])?;
    let output = Command::new(loader)
        .args(["--library-path"])
        .arg(library_path)
        .arg(generator)
        .arg(format!("--root={}", root.display()))
        .args(["query", UDEV_HWDB_TEST_MODALIAS])
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("SOURCE_DATE_EPOCH", SOURCE_DATE_EPOCH.to_string())
        .output()
        .context("failed to query the prebuilt udev hwdb")?;
    if !output.status.success() {
        bail!("source-built systemd-hwdb rejected the prebuilt database")
    }
    let stdout = String::from_utf8(output.stdout)?;
    for required in ["Intel Corporation", "82540EM Gigabit Ethernet Controller"] {
        if !stdout.contains(required) {
            bail!("prebuilt udev hwdb query is missing {required}")
        }
    }
    Ok(())
}

fn stage_runtime_paths(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    paths: &[&str],
) -> Result<()> {
    let install = component_install(repo_root, component);
    for rel in paths {
        copy_path_preserving(&install.join(rel), &staging.join(rel))?;
    }
    Ok(())
}

fn stage_gnupg(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "gpgv");
    // Keep gpgv's dedicated package ownership intact while publishing the
    // OpenPGP engine and helpers GPGME needs for verified Flatpak remotes.
    copy_tree_filtered(
        &install.join("usr/bin"),
        &staging.join("usr/bin"),
        &|relative, metadata| {
            metadata.is_dir()
                || relative
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name != "gpgv")
        },
    )?;
    for relative in ["usr/libexec", "usr/sbin", "usr/share/gnupg"] {
        copy_tree_preserving(&install.join(relative), &staging.join(relative))?;
    }
    copy_preserving(
        &repo_root.join("src/system/security/gnupg/COPYING"),
        &staging.join("usr/share/doc/gnupg/copyright"),
    )
}

pub(crate) fn stage_flatpak(repo_root: &Path, staging: &Path) -> Result<()> {
    // Flatpak is shipped as one first-class MattOS package. Until the support
    // libraries below receive their own package boundaries, this package owns
    // their target-built runtime files as an explicit, closed payload. This
    // is deliberately not a host fallback: every copied tree comes from a
    // declared MattOS build stage and is covered by the package cache input.
    // Seed only the configured remote. Optional applications are installed
    // into the target later by the installer and are never package payload.
    stage_flatpak_system_remote(
        &repo_root.join("src/system/packages/config/flatpak/flathub.flatpakrepo"),
        staging,
    )?;
    for component in [
        "ostree",
        "gpgme",
        "gdk-pixbuf",
        "appstream",
        "json-glib",
        "libxmlb",
        "libfyaml",
        "fuse3",
        "libpng",
        "bubblewrap",
        "xdg-dbus-proxy",
        "flatpak",
    ] {
        let install = component_install(repo_root, component);
        for top_level in ["usr", "etc", "var"] {
            let source = install.join(top_level);
            if source.is_dir() {
                copy_tree_preserving(&source, &staging.join(top_level))?;
            }
        }
    }
    // Libtool archives are build-time link metadata, not part of Flatpak's
    // runtime ABI.  The source stages retain them only where a later target
    // build needs them; do not copy their disposable absolute build paths
    // into the runtime Flatpak package.
    remove_staged_libtool_archives_from_package(&staging)?;
    copy_preserving(
        &repo_root.join("out/build/flatpak/install/usr/libexec/mattos-flatpak-target-install"),
        &staging.join("usr/libexec/mattos-flatpak-target-install"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/packages/flatpak/COPYING"),
        &staging.join("usr/share/doc/flatpak/copyright"),
    )?;
    // MattOS ships Flathub as a distro-owned, signed system remote. Flatpak
    // imports this descriptor for both system and user installations without
    // any first-run shell setup or application-specific override policy.
    copy_preserving(
        &repo_root.join("src/system/packages/config/flatpak/flathub.flatpakrepo"),
        &staging.join("usr/share/flatpak/remotes.d/flathub.flatpakrepo"),
    )?;
    let resources = repo_root.join("src/system/packages/config/flatpak");
    for unit in [
        "mattos-flatpak-system-update.service",
        "mattos-flatpak-system-update.timer",
        "mattos-flatpak-user-update.service",
        "mattos-flatpak-user-update.timer",
    ] {
        let destination = if unit.contains("user") {
            staging.join("usr/lib/systemd/user").join(unit)
        } else {
            staging.join("usr/lib/systemd/system").join(unit)
        };
        copy_preserving(&resources.join(unit), &destination)?;
    }
    for (target, link) in [
        (
            "usr/lib/systemd/system/mattos-flatpak-system-update.timer",
            "etc/systemd/system/timers.target.wants/mattos-flatpak-system-update.timer",
        ),
        (
            "usr/lib/systemd/user/mattos-flatpak-user-update.timer",
            "etc/systemd/user/timers.target.wants/mattos-flatpak-user-update.timer",
        ),
    ] {
        let link = staging.join(link);
        if let Some(parent) = link.parent() {
            fs::create_dir_all(parent)?;
        }
        if link.exists() || link.symlink_metadata().is_ok() {
            fs::remove_file(&link)?;
        }
        std::os::unix::fs::symlink(format!("/{target}"), link)?;
    }
    // The document portal mounts its per-user document filesystem through
    // libfuse's privileged helper. The source build deliberately avoids
    // setting ownership bits (it runs unprivileged), so establish the target
    // package's documented root-owned setuid contract at package staging.
    set_mode(staging.join("usr/bin/fusermount3"), 0o4755)?;
    Ok(())
}

fn remove_staged_libtool_archives_from_package(staging: &Path) -> Result<()> {
    let mut archives = Vec::new();
    walk_tree(staging, &mut |path, metadata| {
        if metadata.is_file() && path.extension().and_then(OsStr::to_str) == Some("la") {
            archives.push(path.to_owned());
        }
        Ok(())
    })?;
    for archive in archives {
        fs::remove_file(archive)?;
    }
    Ok(())
}

/// Seed Flatpak's default system installation from MattOS's signed static
/// policy. Flatpak normally imports descriptors in `remotes.d` lazily when a
/// writable OSTree repository is first opened. A fresh live system has no such
/// repository yet, so a non-root `flatpak remotes` would otherwise show
/// nothing even though the descriptor is present. This is the same normal
/// system-remote configuration Flatpak writes when it imports a descriptor,
/// materialized deterministically while composing the MattOS package.
///
/// The repository contains no objects or refs: only the required OSTree
/// layout, remote configuration, and public verification key derived from the
/// packaged descriptor. Application and runtime content remains user data.
pub(crate) fn stage_flatpak_system_remote(descriptor: &Path, staging: &Path) -> Result<()> {
    let policy = fs::read_to_string(descriptor).with_context(|| {
        format!(
            "failed to read Flatpak remote policy {}",
            descriptor.display()
        )
    })?;
    let value = |key: &str| -> Result<String> {
        policy
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .map(str::to_owned)
            .with_context(|| format!("Flatpak remote policy is missing {key}"))
    };
    let url = value("Url")?;
    let title = value("Title")?;
    let homepage = value("Homepage")?;
    let comment = value("Comment")?;
    let description = value("Description")?;
    let icon = value("Icon")?;
    let gpg_key = decode_flatpak_base64(&value("GPGKey")?)?;
    if gpg_key.len() < 10 {
        bail!("Flatpak remote policy contains an invalid short GPG key");
    }

    let repo = staging.join("var/lib/flatpak/repo");
    // libostree requires the remote-ref namespace to exist even before the
    // first pull.  `flatpak remote-add` creates it as part of repository
    // initialization; omit it and a later install fails opening
    // `refs/remotes` after downloading content.
    for directory in [
        "objects",
        "refs",
        "refs/heads",
        "refs/remotes",
        "state",
        "tmp",
        "extensions",
    ] {
        fs::create_dir_all(repo.join(directory))?;
    }
    fs::write(
        repo.join("config"),
        format!(
            "[core]\nrepo_version=1\nmode=bare-user-only\nmin-free-space-size=500MB\nxa.applied-remotes=flathub;\n\n[remote \"flathub\"]\nurl={url}\nxa.title={title}\ngpg-verify=true\ngpg-verify-summary=true\nxa.comment={comment}\nxa.description={description}\nxa.icon={icon}\nxa.homepage={homepage}\n"
        ),
    )?;
    fs::write(repo.join("flathub.trustedkeys.gpg"), gpg_key)?;
    Ok(())
}

/// Decode the standard base64 GPG payload in a `.flatpakrepo` without adding a
/// host tool or an unrelated runtime dependency to package composition.
fn decode_flatpak_base64(input: &str) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut chunk = [0u8; 4];
    let mut chunk_len = 0;
    for byte in input.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        chunk[chunk_len] = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => 64,
            _ => bail!("invalid base64 byte {byte:?} in Flatpak remote policy"),
        };
        chunk_len += 1;
        if chunk_len != 4 {
            continue;
        }
        if chunk[0] == 64 || chunk[1] == 64 || (chunk[2] == 64 && chunk[3] != 64) {
            bail!("invalid base64 padding in Flatpak remote policy");
        }
        output.push((chunk[0] << 2) | (chunk[1] >> 4));
        if chunk[2] != 64 {
            output.push((chunk[1] << 4) | (chunk[2] >> 2));
            if chunk[3] != 64 {
                output.push((chunk[2] << 6) | chunk[3]);
            }
        }
        chunk_len = 0;
    }
    if chunk_len != 0 {
        bail!("incomplete base64 payload in Flatpak remote policy");
    }
    Ok(output)
}

fn copy_component_usr_and_etc(repo_root: &Path, staging: &Path, component: &str) -> Result<()> {
    let install = component_install(repo_root, component);
    for top_level in ["usr", "etc"] {
        let source = install.join(top_level);
        if source.is_dir() {
            copy_tree_preserving(&source, &staging.join(top_level))?;
        }
    }
    Ok(())
}

fn stage_xwayland(repo_root: &Path, staging: &Path) -> Result<()> {
    // The Xwayland package owns every auxiliary library built solely for the
    // server. Each tree is a declared MattOS stage, never a host fallback.
    for component in [
        "libepoxy",
        "libfontenc",
        "libxfont",
        "libxcvt",
        "libxshmfence",
        "xkbcomp",
        "xwayland",
    ] {
        copy_component_usr_and_etc(repo_root, staging, component)?;
    }
    Ok(())
}

fn stage_fontconfig(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "fontconfig");
    stage_runtime_paths(
        repo_root,
        staging,
        "fontconfig",
        &[
            "usr/bin/fc-cache",
            "usr/bin/fc-cat",
            "usr/bin/fc-conflist",
            "usr/bin/fc-list",
            "usr/bin/fc-match",
            "usr/bin/fc-pattern",
            "usr/bin/fc-query",
            "usr/bin/fc-scan",
            "usr/bin/fc-validate",
        ],
    )?;
    for relative in [
        "etc/fonts",
        "usr/share/fontconfig",
        "usr/share/xml/fontconfig",
    ] {
        copy_tree_preserving(&install.join(relative), &staging.join(relative))?;
    }
    copy_preserving(
        &repo_root.join("src/system/libraries/fontconfig/COPYING"),
        &staging.join("usr/share/doc/fontconfig/copyright"),
    )?;
    for required in [
        "usr/bin/fc-match",
        "etc/fonts/fonts.conf",
        "usr/share/fontconfig/conf.avail",
    ] {
        if !staging.join(required).exists() {
            bail!("fontconfig package missing /{required}");
        }
    }
    Ok(())
}

fn stage_pop_fonts(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "pop-fonts");
    copy_tree_preserving(&install.join("usr"), &staging.join("usr"))?;
    for font in POP_FIRA_RUNTIME_FONTS {
        let path = staging.join("usr/share/fonts/opentype/fira").join(font);
        if !path.is_file() {
            bail!("fonts-fira package missing /usr/share/fonts/opentype/fira/{font}");
        }
    }
    if !staging.join("usr/share/doc/fonts-fira/copyright").is_file() {
        bail!("fonts-fira package missing its source-owned license");
    }
    Ok(())
}

pub(crate) fn stage_xdg_desktop_portal(repo_root: &Path, staging: &Path) -> Result<()> {
    // The generic broker and its GStreamer pbutils closure ship together. The
    // portal executes Bubblewrap at /usr/bin/bwrap, but Flatpak owns that
    // target-built executable and is the portal package's declared runtime
    // dependency. Copying it here would create two Debian package owners for
    // the same path. Desktop-specific portal backends are packaged by their
    // own first-class stages.
    for component in ["gstreamer", "gstreamer-base", "xdg-desktop-portal"] {
        copy_component_usr_and_etc(repo_root, staging, component)?;
    }
    Ok(())
}

fn stage_amdgpu_ids(install: &Path, staging: &Path) -> Result<()> {
    let relative = "usr/share/libdrm/amdgpu.ids";
    let source = install.join(relative);
    if fs::metadata(&source)?.len() == 0 {
        bail!("libdrm AMD device database is empty: {}", source.display());
    }
    copy_preserving(&source, &staging.join(relative))
}

#[cfg(test)]
mod desktop_data_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn fontconfig_staging_preserves_font_discovery_payload() {
        let repo = tempfile::tempdir().unwrap();
        let install = repo.path().join("out/build/fontconfig/install");
        for relative in [
            "usr/bin/fc-cache",
            "usr/bin/fc-cat",
            "usr/bin/fc-conflist",
            "usr/bin/fc-list",
            "usr/bin/fc-match",
            "usr/bin/fc-pattern",
            "usr/bin/fc-query",
            "usr/bin/fc-scan",
            "usr/bin/fc-validate",
            "etc/fonts/fonts.conf",
            "usr/share/fontconfig/conf.avail/45-generic.conf",
            "usr/share/xml/fontconfig/fonts.dtd",
        ] {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, relative.as_bytes()).unwrap();
        }
        fs::create_dir_all(install.join("etc/fonts/conf.d")).unwrap();
        symlink(
            "../../../usr/share/fontconfig/conf.avail/45-generic.conf",
            install.join("etc/fonts/conf.d/45-generic.conf"),
        )
        .unwrap();
        let license = repo.path().join("src/system/libraries/fontconfig/COPYING");
        fs::create_dir_all(license.parent().unwrap()).unwrap();
        fs::write(&license, b"fontconfig license").unwrap();

        let staging = repo.path().join("staged");
        fs::create_dir_all(&staging).unwrap();
        stage_fontconfig(repo.path(), &staging).unwrap();

        assert_eq!(
            fs::read(staging.join("etc/fonts/fonts.conf")).unwrap(),
            b"etc/fonts/fonts.conf"
        );
        assert!(
            staging
                .join("usr/share/fontconfig/conf.avail/45-generic.conf")
                .is_file()
        );
        assert!(staging.join("usr/share/xml/fontconfig/fonts.dtd").is_file());
        assert!(
            staging
                .join("etc/fonts/conf.d/45-generic.conf")
                .is_symlink()
        );
        assert_eq!(
            fs::read(staging.join("usr/share/doc/fontconfig/copyright")).unwrap(),
            b"fontconfig license"
        );
    }

    #[test]
    fn amd_database_is_required_and_staged_byte_for_byte() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        assert!(stage_amdgpu_ids(source.path(), target.path()).is_err());
        let relative = "usr/share/libdrm/amdgpu.ids";
        let file = source.path().join(relative);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "").unwrap();
        assert!(stage_amdgpu_ids(source.path(), target.path()).is_err());
        fs::write(&file, "# AMD device names\n164E, C1, AMD Radeon Graphics\n").unwrap();
        stage_amdgpu_ids(source.path(), target.path()).unwrap();
        assert_eq!(
            fs::read(file).unwrap(),
            fs::read(target.path().join(relative)).unwrap()
        );
    }

    #[test]
    fn breeze_staging_preserves_standard_theme_and_icon_lookup_paths() {
        let fixture = tempfile::tempdir().unwrap();
        let install = fixture.path().join("out/build/breeze-icons/install/usr");
        for relative in [
            "share/icons/breeze/index.theme",
            "share/icons/breeze-dark/index.theme",
            "share/icons/breeze/actions/22/go-next.svg",
        ] {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, relative.as_bytes()).unwrap();
        }
        let license = fixture.path().join("src/desktop/kde/breeze-icons/COPYING");
        fs::create_dir_all(license.parent().unwrap()).unwrap();
        fs::write(&license, b"breeze license").unwrap();

        let staging = fixture.path().join("staged");
        stage_kde_module(fixture.path(), &staging, "breeze-icons", "breeze-icons").unwrap();

        for relative in [
            "usr/share/icons/breeze/index.theme",
            "usr/share/icons/breeze-dark/index.theme",
            "usr/share/icons/breeze/actions/22/go-next.svg",
        ] {
            assert!(
                staging.join(relative).is_file(),
                "missing staged /{relative}"
            );
        }
        assert_eq!(
            fs::read(staging.join("usr/share/doc/breeze-icons/copyright")).unwrap(),
            b"breeze license"
        );
    }

    #[test]
    fn plasma_workspace_staging_preserves_application_menu_contract() {
        let fixture = tempfile::tempdir().unwrap();
        let install_root = fixture.path().join("out/build/plasma-workspace/install");
        let executable = install_root.join("usr/bin/plasmashell");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"plasmashell").unwrap();
        let menu = install_root.join("etc/xdg/menus/plasma-applications.menu");
        fs::create_dir_all(menu.parent().unwrap()).unwrap();
        fs::write(&menu, b"<Menu><Name>Applications</Name></Menu>\n").unwrap();
        let license = fixture
            .path()
            .join("src/desktop/kde/plasma-workspace/LICENSES/GPL-2.0-only.txt");
        fs::create_dir_all(license.parent().unwrap()).unwrap();
        fs::write(&license, b"license").unwrap();

        let staging = fixture.path().join("staged");
        stage_kde_module(
            fixture.path(),
            &staging,
            "plasma-workspace",
            "plasma-workspace",
        )
        .unwrap();

        assert_eq!(
            fs::read(staging.join("etc/xdg/menus/plasma-applications.menu")).unwrap(),
            b"<Menu><Name>Applications</Name></Menu>\n"
        );
    }

    #[test]
    fn qt_declarative_staging_preserves_qml_module_and_plugin_paths() {
        let fixture = tempfile::tempdir().unwrap();
        let install = fixture.path().join("out/build/qtdeclarative/install/usr");
        for relative in [
            "lib/x86_64-linux-gnu/qml/QtQuick/qmldir",
            "lib/x86_64-linux-gnu/qml/QtQuick/libqtquick2plugin.so",
        ] {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, relative.as_bytes()).unwrap();
        }
        let source = fixture.path().join("src/desktop/qt/qtdeclarative");
        fs::create_dir_all(source.join("LICENSES")).unwrap();
        fs::write(source.join("LICENSES/Qt-LICENSE"), b"qt license").unwrap();
        fs::write(source.join("licenseRule.json"), b"{}\n").unwrap();

        let staging = fixture.path().join("staged");
        stage_qt_module(fixture.path(), &staging, "qtdeclarative", "qt6-declarative").unwrap();

        assert!(
            staging
                .join("usr/lib/x86_64-linux-gnu/qml/QtQuick/qmldir")
                .is_file()
        );
        assert!(
            staging
                .join("usr/lib/x86_64-linux-gnu/qml/QtQuick/libqtquick2plugin.so")
                .is_file()
        );
        assert!(
            staging
                .join("usr/share/doc/qt6-declarative/licenses/Qt-LICENSE")
                .is_file()
        );
    }

    #[test]
    fn fira_staging_requires_complete_source_owned_runtime_set() {
        let repo = tempfile::tempdir().unwrap();
        let install = repo.path().join("out/build/pop-fonts/install");
        for font in POP_FIRA_RUNTIME_FONTS {
            let path = install.join("usr/share/fonts/opentype/fira").join(font);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, font.as_bytes()).unwrap();
        }
        let license = install.join("usr/share/doc/fonts-fira/copyright");
        fs::create_dir_all(license.parent().unwrap()).unwrap();
        fs::write(&license, b"OFL").unwrap();

        let staging = repo.path().join("staged");
        fs::create_dir_all(&staging).unwrap();
        stage_pop_fonts(repo.path(), &staging).unwrap();
        assert!(
            staging
                .join("usr/share/fonts/opentype/fira/FiraSans-Regular.otf")
                .is_file()
        );
        assert_eq!(
            fs::read(staging.join("usr/share/doc/fonts-fira/copyright")).unwrap(),
            b"OFL"
        );

        fs::remove_file(install.join("usr/share/fonts/opentype/fira/FiraMono-Regular.otf"))
            .unwrap();
        let incomplete = repo.path().join("incomplete");
        fs::create_dir_all(&incomplete).unwrap();
        assert!(stage_pop_fonts(repo.path(), &incomplete).is_err());
    }
}

fn stage_grub_package(repo_root: &Path, staging: &Path) -> Result<()> {
    for directory in ["usr", "etc"] {
        copy_tree_preserving(
            &component_install(repo_root, "grub").join(directory),
            &staging.join(directory),
        )?;
    }
    // GRUB's installed modinfo.sh is used by grub-install/mkimage, but its
    // generated build flags otherwise retain the disposable sysroot and
    // component install prefixes.  Normalize this metadata in the package
    // view only; the cache-owned GRUB build output remains untouched.
    normalize_staged_grub_modinfo(
        repo_root,
        &staging.join("usr/lib/grub/x86_64-efi/modinfo.sh"),
    )?;
    // These belong respectively to a global index and hybrid media, not the
    // installed x86_64 UEFI package. Do not exempt BIOS objects from ELF audit.
    remove_path_if_exists(&staging.join("usr/share/info/dir"))?;
    remove_path_if_exists(&staging.join("usr/lib/grub/i386-pc"))?;
    copy_preserving(
        &repo_root.join("src/boot/grub/upstream/COPYING"),
        &staging.join("usr/share/doc/grub-efi-amd64/copyright"),
    )?;
    let policy = repo_root.join("src/boot/grub/config");
    stage_executable(
        &policy.join("update-grub"),
        &staging.join("usr/sbin/update-grub"),
        0o755,
    )?;
    let mut conffiles = Vec::new();
    for entry in fs::read_dir(staging.join("etc/grub.d"))? {
        let path = entry?.path();
        if path.is_file() && path.file_name().is_some_and(|name| name != "README") {
            conffiles.push(format!("/{}", path.strip_prefix(staging)?.display()));
        }
    }
    for hook in ["postinst.d", "postrm.d"] {
        let relative = format!("etc/kernel/{hook}/zz-update-grub");
        stage_executable(&policy.join("kernel-hook"), &staging.join(&relative), 0o755)?;
        conffiles.push(format!("/{relative}"));
    }
    // Preserve administrator edits (especially 40_custom) across upgrades.
    // /etc/default/grub and generated /boot/grub/grub.cfg are installer/user
    // state and intentionally are not shipped or overwritten by this package.
    conffiles.sort();
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", conffiles.join("\n")),
    )?;
    Ok(())
}

fn normalize_staged_grub_modinfo(repo_root: &Path, path: &Path) -> Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    let build_root = repo_root.join("out/build");
    let sysroot = repo_root.join("out/sysroot").to_string_lossy().into_owned();
    let repo_root_text = repo_root.to_string_lossy().into_owned();
    let mut contents = fs::read_to_string(path)?;
    contents = contents.replace(&sysroot, "/");
    if let Ok(entries) = fs::read_dir(&build_root) {
        for entry in entries {
            let install_prefix = entry?.path().join("install/usr");
            if install_prefix.is_dir() {
                let install_prefix_text = install_prefix.to_string_lossy().into_owned();
                contents = contents.replace(&install_prefix_text, "/usr");
            }
        }
    }
    contents = contents.replace(&repo_root_text, "/usr/src/mattos");
    if contents.contains(&repo_root_text) {
        bail!(
            "GRUB metadata {} retains the host build root",
            path.display()
        );
    }
    fs::write(path, contents)?;
    Ok(())
}

fn stage_wpa_supplicant(repo_root: &Path, staging: &Path) -> Result<()> {
    copy_tree_preserving(
        &component_install(repo_root, "wpa-supplicant").join("usr"),
        &staging.join("usr"),
    )?;
    // Upstream COPYING refers to README for the current BSD license terms.
    copy_preserving(
        &repo_root.join("src/system/network/hostap/wpa_supplicant/README"),
        &staging.join("usr/share/doc/wpasupplicant/copyright"),
    )?;
    for relative in WPA_RUNTIME_FILES {
        if !staging.join(relative).is_file() {
            bail!("wpasupplicant package is missing /{relative}");
        }
    }
    Ok(())
}

const WPA_RUNTIME_FILES: &[&str] = &[
    "usr/sbin/wpa_supplicant",
    "usr/sbin/wpa_cli",
    "usr/sbin/wpa_passphrase",
    "usr/share/dbus-1/system-services/fi.w1.wpa_supplicant1.service",
    "usr/share/dbus-1/system.d/wpa_supplicant.conf",
    "usr/lib/systemd/system/wpa_supplicant.service",
];

#[cfg(test)]
mod wifi_payload_tests {
    use super::*;
    #[test]
    fn grub_preserves_custom_configuration_and_keeps_bios_objects_out_of_efi_package() {
        let fixture = tempfile::tempdir().unwrap();
        let install = fixture.path().join("out/build/grub/install");
        for relative in [
            "usr/lib/grub/i386-pc/linux.mod",
            "usr/lib/grub/x86_64-efi/linux.mod",
            "usr/share/info/dir",
            "etc/grub.d/00_header",
            "etc/grub.d/40_custom",
            "etc/grub.d/README",
        ] {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, relative).unwrap();
        }
        for relative in [
            "src/boot/grub/upstream/COPYING",
            "src/boot/grub/config/update-grub",
            "src/boot/grub/config/kernel-hook",
        ] {
            let path = fixture.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "fixture").unwrap();
        }
        let staging = fixture.path().join("payload");
        fs::create_dir_all(staging.join("DEBIAN")).unwrap();
        stage_grub_package(fixture.path(), &staging).unwrap();
        assert!(!staging.join("usr/lib/grub/i386-pc").exists());
        assert!(!staging.join("usr/share/info/dir").exists());
        assert!(staging.join("usr/lib/grub/x86_64-efi/linux.mod").is_file());
        let files = fs::read_to_string(staging.join("DEBIAN/conffiles")).unwrap();
        assert_eq!(
            files,
            "/etc/grub.d/00_header\n/etc/grub.d/40_custom\n/etc/kernel/postinst.d/zz-update-grub\n/etc/kernel/postrm.d/zz-update-grub\n"
        );
        assert!(!staging.join("etc/default/grub").exists());
        assert!(!staging.join("boot/grub/grub.cfg").exists());
        assert!(
            staging
                .join("usr/share/doc/grub-efi-amd64/copyright")
                .is_file()
        );
    }
    #[test]
    fn stages_the_complete_directory_tree_and_rejects_missing_activation_files() {
        let fixture = tempfile::tempdir().unwrap();
        let install = fixture.path().join("out/build/wpa-supplicant/install");
        let license = fixture
            .path()
            .join("src/system/network/hostap/wpa_supplicant/README");
        fs::create_dir_all(license.parent().unwrap()).unwrap();
        fs::write(&license, "upstream license").unwrap();
        for relative in WPA_RUNTIME_FILES {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, relative).unwrap();
        }
        let staging = fixture.path().join("payload");
        stage_wpa_supplicant(fixture.path(), &staging).unwrap();
        assert_eq!(
            fs::read_to_string(staging.join("usr/share/doc/wpasupplicant/copyright")).unwrap(),
            "upstream license"
        );
        for relative in WPA_RUNTIME_FILES {
            assert_eq!(
                fs::read_to_string(staging.join(relative)).unwrap(),
                *relative
            );
        }
        fs::remove_file(install.join(WPA_RUNTIME_FILES[3])).unwrap();
        let fresh = fixture.path().join("missing-activation");
        assert!(stage_wpa_supplicant(fixture.path(), &fresh).is_err());
    }
}

fn stage_network_manager(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "networkmanager");
    copy_tree_preserving(&install.join("usr"), &staging.join("usr"))?;
    copy_tree_preserving(&install.join("etc"), &staging.join("etc"))?;
    for rel in [
        "usr/sbin/NetworkManager",
        "usr/bin/nmcli",
        "usr/lib/systemd/system/NetworkManager.service",
        "usr/lib/systemd/system/NetworkManager-wait-online.service",
    ] {
        if !staging.join(rel).exists() {
            bail!("network-manager package is missing /{rel}");
        }
    }
    Ok(())
}

fn stage_cozy(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_executable(
        &component_install(repo_root, "cozy").join("usr/bin/cozy"),
        &staging.join("usr/bin/cozy"),
        0o755,
    )?;
    for license in ["LICENSE-MIT", "LICENSE-APACHE"] {
        copy_preserving(
            &repo_root.join("src/userland/cozy").join(license),
            &staging.join("usr/share/doc/mattos-cozy").join(license),
        )?;
    }
    Ok(())
}

fn stage_pipewire(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "pipewire");
    for relative in [
        "usr/bin",
        "usr/lib/x86_64-linux-gnu",
        "usr/lib/systemd/user",
        "usr/share/pipewire",
    ] {
        copy_tree_preserving(&install.join(relative), &staging.join(relative))?;
    }
    copy_preserving(
        &repo_root.join("src/system/multimedia/pipewire/COPYING"),
        &staging.join("usr/share/doc/pipewire/copyright"),
    )?;
    let socket_wants = staging.join("usr/lib/systemd/user/sockets.target.wants");
    fs::create_dir_all(&socket_wants)?;
    #[cfg(unix)]
    for unit in ["pipewire.socket", "pipewire-pulse.socket"] {
        std::os::unix::fs::symlink(format!("../{unit}"), socket_wants.join(unit))?;
    }
    for required in [
        "usr/bin/pipewire",
        "usr/bin/pipewire-pulse",
        "usr/lib/x86_64-linux-gnu/libpipewire-0.3.so.0",
        "usr/lib/systemd/user/pipewire.service",
        "usr/lib/systemd/user/pipewire.socket",
        "usr/lib/systemd/user/sockets.target.wants/pipewire.socket",
    ] {
        if fs::symlink_metadata(staging.join(required)).is_err() {
            bail!("pipewire package is missing /{required}");
        }
    }
    Ok(())
}

fn stage_mesa_dri_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "mesa");
    let library_dir = install.join("usr/lib/x86_64-linux-gnu");
    let gallium = fs::read_dir(&library_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .find(|name| {
            let name = name.to_string_lossy();
            name.starts_with("libgallium-") && name.ends_with(".so")
        })
        .ok_or_else(|| anyhow!("Mesa did not install its versioned Gallium DRI runtime"))?;
    let gallium_rel = format!("usr/lib/x86_64-linux-gnu/{}", gallium.to_string_lossy());
    stage_runtime_paths(
        repo_root,
        staging,
        "mesa",
        &[&gallium_rel, "usr/lib/x86_64-linux-gnu/gbm/dri_gbm.so"],
    )?;
    copy_tree_preserving(
        &install.join("usr/share/drirc.d"),
        &staging.join("usr/share/drirc.d"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/mesa/docs/license.rst"),
        &staging.join("usr/share/doc/libgl1-mesa-dri/copyright"),
    )?;
    Ok(())
}

fn stage_mesa_egl_vendor(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "mesa",
        &[
            "usr/lib/x86_64-linux-gnu/libEGL_mesa.so.0.0.0",
            "usr/lib/x86_64-linux-gnu/libEGL_mesa.so.0",
            "usr/share/glvnd/egl_vendor.d/50_mesa.json",
        ],
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/mesa/docs/license.rst"),
        &staging.join("usr/share/doc/libegl-mesa0/copyright"),
    )
}

fn stage_nvidia_package(repo_root: &Path, staging: &Path, package: &str) -> Result<()> {
    let install = component_install(repo_root, "nvidia-driver");
    let lib = "usr/lib/x86_64-linux-gnu";
    let copy_libraries = |names: &[&str]| -> Result<()> {
        for name in names {
            copy_path_preserving(&install.join(lib).join(name), &staging.join(lib).join(name))?;
        }
        Ok(())
    };
    match package {
        "linux-modules-nvidia-595-open-7.2.0-rc5-mattos" => {
            copy_tree_preserving(
                &install.join("usr/lib/modules/7.2.0-rc5-mattos/updates/nvidia"),
                &staging.join("usr/lib/modules/7.2.0-rc5-mattos/updates/nvidia"),
            )?;
            copy_preserving(
                &repo_root.join("src/system/graphics/nvidia-driver/nvidia-modprobe.conf"),
                &staging.join("etc/modprobe.d/nvidia.conf"),
            )?;
            copy_preserving(
                &install.join("usr/lib/modprobe.d/nvidia-supported-gpus.conf"),
                &staging.join("usr/lib/modprobe.d/nvidia-supported-gpus.conf"),
            )?;
            copy_preserving(
                &install.join("usr/libexec/mattos-nvidia-select"),
                &staging.join("usr/libexec/mattos-nvidia-select"),
            )?;
            fs::write(
                staging.join("DEBIAN/conffiles"),
                "/etc/modprobe.d/nvidia.conf\n",
            )?;
            fs::write(
                staging.join("DEBIAN/postinst"),
                "#!/bin/sh\nset -e\n# Offline image assembly runs depmod after all module packages are unpacked.\n[ -n \"${DPKG_ROOT:-}\" ] && exit 0\nif command -v depmod >/dev/null 2>&1; then depmod 7.2.0-rc5-mattos; fi\n",
            )?;
            set_mode(staging.join("DEBIAN/postinst"), 0o755)?;
            fs::write(
                staging.join("DEBIAN/postrm"),
                "#!/bin/sh\nset -e\n# Do not modify the build host while assembling an offline root.\n[ -n \"${DPKG_ROOT:-}\" ] && exit 0\nif command -v depmod >/dev/null 2>&1; then depmod 7.2.0-rc5-mattos; fi\n",
            )?;
            set_mode(staging.join("DEBIAN/postrm"), 0o755)?;
        }
        "nvidia-firmware-595" => copy_tree_preserving(
            &install.join("usr/lib/firmware/nvidia/595.84"),
            &staging.join("usr/lib/firmware/nvidia/595.84"),
        )?,
        "libnvidia-gl-595" => {
            copy_libraries(&[
                "libEGL_nvidia.so.595.84",
                "libEGL_nvidia.so.0",
                "libGLESv1_CM_nvidia.so.595.84",
                "libGLESv1_CM_nvidia.so.1",
                "libGLESv2_nvidia.so.595.84",
                "libGLESv2_nvidia.so.2",
                "libGLX_nvidia.so.595.84",
                "libGLX_nvidia.so.0",
                "libnvidia-allocator.so.595.84",
                "libnvidia-allocator.so.1",
                "libnvidia-egl-gbm.so.1.1.3",
                "libnvidia-egl-gbm.so.1",
                "libnvidia-egl-wayland.so.1.1.20",
                "libnvidia-egl-wayland.so.1",
                "libnvidia-egl-wayland2.so.1.0.1",
                "libnvidia-egl-wayland2.so.1",
                "libnvidia-eglcore.so.595.84",
                "libnvidia-glcore.so.595.84",
                "libnvidia-glsi.so.595.84",
                "libnvidia-glvkspirv.so.595.84",
                "libnvidia-gpucomp.so.595.84",
                "libnvidia-present.so.595.84",
                "libnvidia-tls.so.595.84",
            ])?;
            for relative in [
                "usr/share/glvnd/egl_vendor.d/10_nvidia.json",
                "usr/share/vulkan/icd.d/nvidia_icd.json",
                "usr/share/vulkan/implicit_layer.d/nvidia_layers.json",
                "usr/share/egl/egl_external_platform.d/09_nvidia_wayland2.json",
                "usr/share/egl/egl_external_platform.d/10_nvidia_wayland.json",
                "usr/share/egl/egl_external_platform.d/15_nvidia_gbm.json",
            ] {
                copy_path_preserving(&install.join(relative), &staging.join(relative))?;
            }
            let backend = staging.join("usr/lib/x86_64-linux-gnu/gbm/nvidia-drm_gbm.so");
            fs::create_dir_all(backend.parent().expect("NVIDIA GBM backend parent"))?;
            std::os::unix::fs::symlink("../libnvidia-allocator.so.1", backend)?;
            validate_nvidia_graphics_metadata(staging)?;
        }
        "libnvidia-compute-595" => copy_libraries(&[
            "libcuda.so.595.84",
            "libcuda.so.1",
            "libnvidia-ml.so.595.84",
            "libnvidia-ml.so.1",
            "libnvidia-ptxjitcompiler.so.595.84",
            "libnvidia-ptxjitcompiler.so.1",
        ])?,
        "libnvidia-encode-595" => {
            copy_libraries(&["libnvidia-encode.so.595.84", "libnvidia-encode.so.1"])?
        }
        "libnvidia-decode-595" => copy_libraries(&["libnvcuvid.so.595.84", "libnvcuvid.so.1"])?,
        "nvidia-utils-595" => {
            for name in ["nvidia-smi", "nvidia-modprobe", "nvidia-persistenced"] {
                copy_path_preserving(
                    &install.join("usr/bin").join(name),
                    &staging.join("usr/bin").join(name),
                )?;
            }
        }
        "nvidia-driver-595-open" => {}
        _ => bail!("unknown NVIDIA package {package}"),
    }
    copy_preserving(
        &install.join("usr/share/doc/nvidia-driver-595/LICENSE"),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("copyright"),
    )?;
    copy_preserving(
        &install.join("usr/share/doc/nvidia-driver-595/manifest.toml"),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("manifest.toml"),
    )?;
    for name in [
        "README.md",
        "runfile.sha256",
        "supported-gpus.json",
        "supported-gpus.LICENSE",
    ] {
        copy_preserving(
            &install.join("usr/share/doc/nvidia-driver-595").join(name),
            &staging.join("usr/share/doc").join(package).join(name),
        )?;
    }
    Ok(())
}

fn validate_nvidia_graphics_metadata(root: &Path) -> Result<()> {
    let icd_path = root.join("usr/share/vulkan/icd.d/nvidia_icd.json");
    let icd: serde_json::Value = serde_json::from_slice(&fs::read(&icd_path)?)?;
    let library = icd
        .pointer("/ICD/library_path")
        .and_then(|value| value.as_str());
    if library != Some("libGLX_nvidia.so.0")
        || !root
            .join("usr/lib/x86_64-linux-gnu/libGLX_nvidia.so.0")
            .is_symlink()
    {
        bail!("NVIDIA Vulkan ICD does not resolve through its canonical system SONAME");
    }
    for relative in [
        "usr/share/glvnd/egl_vendor.d/10_nvidia.json",
        "usr/share/egl/egl_external_platform.d/09_nvidia_wayland2.json",
        "usr/share/egl/egl_external_platform.d/10_nvidia_wayland.json",
        "usr/share/egl/egl_external_platform.d/15_nvidia_gbm.json",
    ] {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(root.join(relative))?)?;
        if value
            .get("file_format_version")
            .and_then(|version| version.as_str())
            .is_none()
        {
            bail!("NVIDIA metadata /{relative} is not a valid vendor manifest");
        }
    }
    Ok(())
}

fn stage_mesa_vulkan_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "mesa",
        &[
            "usr/lib/x86_64-linux-gnu/libVkLayer_MESA_device_select.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_radeon.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_intel.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_nouveau.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_virtio.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_lvp.so",
            "usr/share/vulkan/icd.d/radeon_icd.x86_64.json",
            "usr/share/vulkan/icd.d/intel_icd.x86_64.json",
            "usr/share/vulkan/icd.d/nouveau_icd.x86_64.json",
            "usr/share/vulkan/icd.d/virtio_icd.x86_64.json",
            "usr/share/vulkan/icd.d/lvp_icd.x86_64.json",
            "usr/share/vulkan/implicit_layer.d/VkLayer_MESA_device_select.json",
        ],
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/mesa/docs/license.rst"),
        &staging.join("usr/share/doc/mesa-vulkan-drivers/copyright"),
    )?;
    validate_vulkan_icd_manifests(staging)?;
    Ok(())
}

pub(crate) fn validate_vulkan_icd_manifests(root: &Path) -> Result<()> {
    let manifests = [
        ("radeon_icd.x86_64.json", "libvulkan_radeon.so"),
        ("intel_icd.x86_64.json", "libvulkan_intel.so"),
        ("nouveau_icd.x86_64.json", "libvulkan_nouveau.so"),
        ("virtio_icd.x86_64.json", "libvulkan_virtio.so"),
        ("lvp_icd.x86_64.json", "libvulkan_lvp.so"),
    ];
    for (manifest, library) in manifests {
        let path = root.join("usr/share/vulkan/icd.d").join(manifest);
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("invalid Vulkan ICD manifest {}", path.display()))?;
        let expected = format!("/usr/lib/x86_64-linux-gnu/{library}");
        if value.pointer("/ICD/library_path").and_then(|v| v.as_str()) != Some(&expected)
            || value
                .pointer("/ICD/api_version")
                .and_then(|v| v.as_str())
                .is_none()
            || value
                .get("file_format_version")
                .and_then(|v| v.as_str())
                .is_none()
        {
            bail!("Vulkan ICD manifest {} is not canonical", path.display())
        }
        if !root.join(expected.trim_start_matches('/')).is_file() {
            bail!("Vulkan ICD {manifest} references missing {expected}")
        }
    }
    Ok(())
}

fn stage_vulkan_development(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn stage_vulkan_tools(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "vulkan-tools",
        &["usr/bin/vulkaninfo", "usr/bin/vkcube"],
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/vulkan-tools/LICENSE.txt"),
        &staging.join("usr/share/doc/vulkan-tools/copyright"),
    )?;
    Ok(())
}

fn stage_library_family(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    names: &[&str],
) -> Result<()> {
    let source = component_install(repo_root, component).join("usr/lib/x86_64-linux-gnu");
    let destination = staging.join("usr/lib/x86_64-linux-gnu");
    for name in names {
        let source_soname = source.join(name);
        let metadata = fs::symlink_metadata(&source_soname)
            .with_context(|| format!("{component} did not install {name}"))?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&source_soname)?;
            if target.is_absolute() || target.components().count() != 1 {
                bail!(
                    "{component} installed unsafe SONAME target {} -> {}",
                    source_soname.display(),
                    target.display()
                );
            }
            copy_preserving(&source.join(&target), &destination.join(&target))?;
            copy_path_preserving(&source_soname, &destination.join(name))?;
        } else {
            copy_preserving(&source_soname, &destination.join(name))?;
        }
    }
    Ok(())
}

/// Install generated XKB rules from the output-owned xkeyboard-config mirror.
/// The Git import contains rules fragments; Meson produces `rules/evdev`.
fn stage_xkeyboard_config_data(repo_root: &Path, staging: &Path) -> Result<()> {
    build_xkeyboard_config(repo_root)?;
    let source = repo_root.join("out/build/xkeyboard-config/install/usr/share");
    copy_tree_preserving(
        &source.join("xkeyboard-config-2"),
        &staging.join("usr/share/xkeyboard-config-2"),
    )?;
    // Meson's installed legacy link is absolute (`/usr/share/...`).  Preserve
    // its in-image meaning rather than copying an absolute host-root link
    // into package staging.
    let legacy_root = staging.join("usr/share/X11/xkb");
    if let Some(parent) = legacy_root.parent() {
        fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("../xkeyboard-config-2", &legacy_root)?;
    #[cfg(not(unix))]
    bail!("xkeyboard-config package staging requires Unix symlinks");
    let rules = staging.join("usr/share/xkeyboard-config-2/rules/evdev");
    if !rules.is_file() {
        bail!(
            "xkeyboard-config staging did not contain generated {}",
            rules.display()
        );
    }
    copy_preserving(
        &repo_root.join("src/system/data/xkeyboard-config/COPYING"),
        &staging.join("usr/share/doc/xkb-data/copyright"),
    )?;
    Ok(())
}

fn stage_imported_soname_library(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    soname: &str,
    license_rel: &str,
    package: &str,
) -> Result<()> {
    let source_dir = component_install(repo_root, component).join("usr/lib/x86_64-linux-gnu");
    let source_soname = source_dir.join(soname);
    let destination_dir = staging.join("usr/lib/x86_64-linux-gnu");
    fs::create_dir_all(&destination_dir)?;
    let metadata = fs::symlink_metadata(&source_soname)
        .with_context(|| format!("{component} did not install {soname}"))?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(&source_soname)?;
        if target.is_absolute() || target.components().count() != 1 {
            bail!(
                "{component} installed unsafe SONAME target {} -> {}",
                source_soname.display(),
                target.display()
            );
        }
        copy_preserving(&source_dir.join(&target), &destination_dir.join(&target))?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, destination_dir.join(soname))?;
        #[cfg(not(unix))]
        copy_preserving(&source_dir.join(&target), &destination_dir.join(soname))?;
    } else {
        copy_preserving(&source_soname, &destination_dir.join(soname))?;
    }
    copy_preserving(
        &repo_root.join(license_rel),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("copyright"),
    )?;
    Ok(())
}

fn stage_terminfo(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = component_install(repo_root, "ncurses").join("usr/share/terminfo");
    for terminal in TERMINFO_ENTRIES {
        let first = terminal.as_bytes()[0];
        let candidates = [
            source.join(char::from(first).to_string()).join(terminal),
            source.join(format!("{first:x}")).join(terminal),
        ];
        let entry = candidates
            .iter()
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| {
                anyhow!(
                    "terminfo entry {terminal} missing from {}",
                    source.display()
                )
            })?;
        let relative = entry.strip_prefix(&source)?;
        copy_preserving(entry, &staging.join("usr/share/terminfo").join(relative))?;
    }
    Ok(())
}

fn stage_procps(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "procps-ng", PROCPS_RUNTIME_PATHS)?;
    copy_preserving(
        &repo_root.join("src/userland/procps-ng/sysctl.conf"),
        &staging.join("etc/sysctl.conf"),
    )?;
    fs::write(staging.join("DEBIAN/conffiles"), "/etc/sysctl.conf\n")?;
    Ok(())
}

fn stage_dbus_broker(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "dbus-broker");
    for rel in ["usr/bin/dbus-broker", "usr/bin/dbus-broker-launch"] {
        copy_path_preserving(&install.join(rel), &staging.join(rel))?;
    }
    let dbus = repo_root.join("src/system/dbus");
    copy_preserving(
        &dbus.join("config/system.conf"),
        &staging.join("etc/dbus-1/system.conf"),
    )?;
    copy_preserving(
        &dbus.join("config/dbus.conf"),
        &staging.join("usr/lib/sysusers.d/dbus.conf"),
    )?;
    fs::create_dir_all(staging.join("usr/lib/tmpfiles.d"))?;
    fs::write(
        staging.join("usr/lib/tmpfiles.d/dbus.conf"),
        "d /run/dbus 0755 root root -\n",
    )?;
    copy_tree_preserving(&dbus.join("units"), &staging.join("usr/lib/systemd/system"))?;
    let session = repo_root.join("src/system/session");
    copy_preserving(
        &session.join("dbus/session.conf"),
        &staging.join("usr/share/dbus-1/session.conf"),
    )?;
    copy_tree_preserving(
        &session.join("user-units"),
        &staging.join("usr/lib/systemd/user"),
    )?;
    stage_dbus_service_links(staging)?;
    for rel in [
        "etc/dbus-1/system.d",
        "etc/dbus-1/session.d",
        "usr/share/dbus-1/system-services",
        "usr/share/dbus-1/system.d",
        "usr/share/dbus-1/session.d",
        "usr/share/dbus-1/services",
    ] {
        fs::create_dir_all(staging.join(rel))?;
    }
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/dbus-1/system.conf\n",
    )?;
    Ok(())
}

#[cfg(unix)]
fn stage_dbus_service_links(staging: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    // These aliases and enablement links are part of the D-Bus provider, not
    // an image-overlay concern. Package-composed targets never run the
    // live-root overlay helper that previously supplied them.
    symlink(
        "dbus-broker.service",
        staging.join("usr/lib/systemd/system/dbus.service"),
    )?;
    symlink(
        "dbus-broker.service",
        staging.join("usr/lib/systemd/user/dbus.service"),
    )?;
    let system_wants = staging.join("usr/lib/systemd/system/sockets.target.wants");
    fs::create_dir_all(&system_wants)?;
    symlink("../dbus.socket", system_wants.join("dbus.socket"))?;
    let user_wants = staging.join("usr/lib/systemd/user/sockets.target.wants");
    fs::create_dir_all(&user_wants)?;
    symlink("../dbus.socket", user_wants.join("dbus.socket"))?;
    Ok(())
}

#[cfg(not(unix))]
fn stage_dbus_service_links(_staging: &Path) -> Result<()> {
    bail!("D-Bus package service links require a Unix build host")
}

#[cfg(all(test, unix))]
#[test]
fn dbus_package_owns_provider_aliases_and_socket_enablement() {
    let staging = tempfile::tempdir().expect("staging");
    fs::create_dir_all(staging.path().join("usr/lib/systemd/system"))
        .expect("system unit directory");
    fs::create_dir_all(staging.path().join("usr/lib/systemd/user")).expect("user unit directory");
    stage_dbus_service_links(staging.path()).expect("D-Bus service links");

    for (path, target) in [
        ("usr/lib/systemd/system/dbus.service", "dbus-broker.service"),
        ("usr/lib/systemd/user/dbus.service", "dbus-broker.service"),
        (
            "usr/lib/systemd/system/sockets.target.wants/dbus.socket",
            "../dbus.socket",
        ),
        (
            "usr/lib/systemd/user/sockets.target.wants/dbus.socket",
            "../dbus.socket",
        ),
    ] {
        assert_eq!(
            fs::read_link(staging.path().join(path)).expect("service link"),
            Path::new(target)
        );
    }
}

fn stage_pam_modules(repo_root: &Path, staging: &Path) -> Result<()> {
    let source =
        component_install(repo_root, "linux-pam").join("usr/lib/x86_64-linux-gnu/security");
    let destination = staging.join("usr/lib/x86_64-linux-gnu/security");
    for module in PAM_MODULES {
        copy_preserving(&source.join(module), &destination.join(module))?;
    }
    Ok(())
}

#[test]
fn pam_module_package_contains_modules_referenced_by_plasma_login_policy() {
    let plasma_login_pam =
        include_str!("../../../../system/session/plasma-login-manager/plasmalogin.pam");
    for module in [
        "pam_unix.so",
        "pam_limits.so",
        "pam_env.so",
        "pam_nologin.so",
    ] {
        assert!(
            PAM_MODULES.contains(&module),
            "PAM package must stage {module}, referenced by Plasma Login Manager policy"
        );
    }
    assert!(plasma_login_pam.contains("session    required     pam_limits.so"));
}

fn stage_pam_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    copy_path_preserving(
        &component_install(repo_root, "linux-pam").join("usr/sbin/unix_chkpwd"),
        &staging.join("usr/sbin/unix_chkpwd"),
    )?;
    let pam_policy = repo_root.join("src/system/auth/config/pam.d");
    copy_tree_preserving(&pam_policy, &staging.join("etc/pam.d"))?;
    // Linux-PAM is built with its upstream vendor configuration directory at
    // /usr/share/pam.  Ship the source-built pam_env defaults there so PAM
    // does not warn on every login while still allowing /etc/security to
    // override them locally.
    copy_preserving(
        &component_install(repo_root, "linux-pam").join("usr/share/pam/security/pam_env.conf"),
        &staging.join("usr/share/pam/security/pam_env.conf"),
    )?;
    let mut conffiles = fs::read_dir(&pam_policy)?
        .map(|entry| {
            Ok(format!(
                "/etc/pam.d/{}",
                entry?.file_name().to_string_lossy()
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    conffiles.sort();
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", conffiles.join("\n")),
    )?;
    Ok(())
}

fn stage_shadow(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "shadow", SHADOW_RUNTIME_PATHS)?;
    let config = repo_root.join("src/system/auth/config");
    copy_preserving(&config.join("login.defs"), &staging.join("etc/login.defs"))?;
    copy_preserving(
        &config.join("default/useradd"),
        &staging.join("etc/default/useradd"),
    )?;
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/login.defs\n/etc/default/useradd\n",
    )?;
    for rel in ["usr/bin/passwd", "usr/bin/newgrp"] {
        set_mode(staging.join(rel), 0o4755)?;
    }
    validate_no_mutable_system_state(staging)
}

fn stage_sudo_rs(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "sudo-rs",
        &["usr/bin/sudo", "usr/bin/visudo"],
    )?;
    let config = repo_root.join("src/system/auth/config");
    copy_preserving(&config.join("sudoers"), &staging.join("etc/sudoers"))?;
    copy_preserving(
        &config.join("sudoers.d/README"),
        &staging.join("etc/sudoers.d/README"),
    )?;
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/sudoers\n/etc/sudoers.d/README\n",
    )?;
    set_mode(staging.join("usr/bin/sudo"), 0o4755)?;
    set_mode(staging.join("etc/sudoers"), 0o440)?;
    set_mode(staging.join("etc/sudoers.d"), 0o750)?;
    set_mode(staging.join("etc/sudoers.d/README"), 0o440)?;
    Ok(())
}

fn stage_util_linux_auth(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "util-linux", UTIL_LINUX_AUTH_PATHS)?;
    for rel in ["usr/bin/login", "usr/bin/su"] {
        set_mode(staging.join(rel), 0o4755)?;
    }
    Ok(())
}

fn stage_openssh_server(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "openssh", OPENSSH_SERVER_RUNTIME_PATHS)?;
    let config = repo_root.join("src/system/network/openssh");
    copy_preserving(
        &config.join("sshd_config"),
        &staging.join("etc/ssh/sshd_config"),
    )?;
    copy_preserving(&config.join("ssh-pam"), &staging.join("etc/pam.d/sshd"))?;
    copy_preserving(
        &config.join("ssh.service"),
        &staging.join("usr/lib/systemd/system/ssh.service"),
    )?;
    copy_preserving(
        &config.join("openssh-sysusers.conf"),
        &staging.join("usr/lib/sysusers.d/openssh.conf"),
    )?;
    fs::create_dir_all(staging.join("etc/ssh/sshd_config.d"))?;
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/ssh/sshd_config\n/etc/pam.d/sshd\n",
    )?;
    Ok(())
}

fn stage_iproute2(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "iproute2", IPROUTE2_RUNTIME_PATHS)?;
    copy_tree_preserving(
        &component_install(repo_root, "iproute2").join("usr/share/iproute2"),
        &staging.join("usr/share/iproute2"),
    )
}

fn stage_linux_libc_dev(repo_root: &Path, staging: &Path) -> Result<()> {
    let glibc_headers = repo_root.join("out/build/glibc/install/usr/include");
    copy_tree_filtered(
        &repo_root.join("out/build/glibc/linux-headers/usr/include"),
        &staging.join("usr/include"),
        &|relative, _| !path_entry_exists(&glibc_headers.join(relative)),
    )?;
    copy_preserving(
        &repo_root.join("src/kernel/linux/COPYING"),
        &staging.join("usr/share/doc/linux-libc-dev/copyright"),
    )?;
    copy_preserving(
        &repo_root.join("out/build/glibc/linux-headers-inventory.txt"),
        &staging.join("usr/share/doc/linux-libc-dev/generated-files.txt"),
    )
}

fn stage_linux_modules(repo_root: &Path, staging: &Path) -> Result<()> {
    let release = fs::read_to_string(repo_root.join("out/build/linux/kernel-release"))?;
    let release = release.trim();
    if release != "7.2.0-rc5-mattos" {
        bail!("kernel module package name does not match built release {release}");
    }
    let source = repo_root
        .join("out/build/linux/modules/usr/lib/modules")
        .join(release);
    copy_tree_preserving(&source, &staging.join("usr/lib/modules").join(release))?;
    copy_preserving(
        &repo_root.join("src/kernel/linux/COPYING"),
        &staging.join(format!("usr/share/doc/linux-modules-{release}/copyright")),
    )
}

fn stage_glibc_development(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn stage_native_binutils(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn stage_native_gcc_common(repo_root: &Path, staging: &Path) -> Result<()> {
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

fn stage_native_compiler_driver(repo_root: &Path, staging: &Path, driver: &str) -> Result<()> {
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

fn stage_native_make(repo_root: &Path, staging: &Path) -> Result<()> {
    copy_preserving(
        &repo_root.join("out/build/make/install/usr/bin/make"),
        &staging.join("usr/bin/make"),
    )?;
    copy_preserving(
        &repo_root.join("src/build-tools/make/COPYING"),
        &staging.join("usr/share/doc/make/copyright"),
    )
}

fn copy_tree_filtered(
    source: &Path,
    destination: &Path,
    include: &dyn Fn(&Path, &fs::Metadata) -> bool,
) -> Result<()> {
    fn recurse(
        root: &Path,
        current: &Path,
        destination: &Path,
        include: &dyn Fn(&Path, &fs::Metadata) -> bool,
    ) -> Result<()> {
        let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let source = entry.path();
            let relative = source.strip_prefix(root)?;
            let metadata = fs::symlink_metadata(&source)?;
            if metadata.is_dir() {
                recurse(root, &source, destination, include)?;
            } else if include(relative, &metadata) {
                copy_path_preserving(&source, &destination.join(relative))?;
            }
        }
        Ok(())
    }
    if !source.is_dir() {
        bail!(
            "required package input directory missing at {}",
            source.display()
        )
    }
    recurse(source, source, destination, include)
}

pub(crate) fn validate_no_mutable_system_state(staging: &Path) -> Result<()> {
    for forbidden in [
        "etc/passwd",
        "etc/group",
        "etc/shadow",
        "etc/gshadow",
        "etc/machine-id",
        "run",
        "var/log",
        "var/lib/systemd/random-seed",
        "var/lib/dhcp",
    ] {
        if path_entry_exists(&staging.join(forbidden)) {
            bail!("mutable system state must not be packaged: /{forbidden}")
        }
    }
    Ok(())
}

pub(crate) fn validate_no_embedded_build_root(repo_root: &Path, staging: &Path) -> Result<()> {
    let needle = repo_root.to_string_lossy();
    walk_tree(staging, &mut |path, metadata| {
        if metadata.is_file() && !path.starts_with(staging.join("DEBIAN")) {
            let bytes = fs::read(path)?;
            if bytes
                .windows(needle.len())
                .any(|window| window == needle.as_bytes())
            {
                bail!(
                    "package payload /{} embeds the host build root",
                    path.strip_prefix(staging)?.display()
                )
            }
        }
        Ok(())
    })
}

fn strip_staged_debug(repo_root: &Path, staging: &Path) -> Result<()> {
    let strip = repo_root.join("out/build/binutils/cross-install/usr/bin/strip");
    if !strip.is_file() {
        bail!(
            "source-built Binutils strip is required before package staging at {}",
            strip.display()
        )
    }
    let mut objects = Vec::new();
    #[cfg(unix)]
    let mut object_inodes = BTreeSet::new();
    walk_tree(staging, &mut |path, metadata| {
        if metadata.is_file() && !path.starts_with(staging.join("DEBIAN")) {
            let header = Command::new("readelf").args(["-h"]).arg(path).output()?;
            if header.status.success() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if !object_inodes.insert((metadata.dev(), metadata.ino())) {
                        return Ok(());
                    }
                }
                objects.push(path.to_path_buf());
            }
        }
        Ok(())
    })?;
    for object in objects {
        #[cfg(unix)]
        let original_mode = {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&object)?.permissions().mode();
            if mode & 0o200 == 0 {
                fs::set_permissions(&object, fs::Permissions::from_mode(mode | 0o200))?;
            }
            mode
        };
        let status = Command::new(&strip)
            .arg("--strip-debug")
            .arg(&object)
            .status()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&object, fs::Permissions::from_mode(original_mode))?;
        }
        if !status.success() {
            bail!("source-built strip failed for {}", object.display())
        }
    }
    Ok(())
}

pub(crate) fn copy_tree_preserving(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(unix)]
    let mut hardlinks = BTreeMap::new();
    copy_tree_preserving_inner(source, destination, &mut hardlinks)
}

#[cfg(unix)]
type HardlinkMap = BTreeMap<(u64, u64), PathBuf>;

#[cfg(not(unix))]
type HardlinkMap = ();

fn copy_tree_preserving_inner(
    source: &Path,
    destination: &Path,
    hardlinks: &mut HardlinkMap,
) -> Result<()> {
    if !source.is_dir() {
        bail!(
            "required package input directory missing at {}",
            source.display()
        )
    }
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&from)?;
        if metadata.is_dir() {
            copy_tree_preserving_inner(&from, &to, hardlinks)?;
        } else {
            copy_path_preserving_with_hardlinks(&from, &to, &metadata, hardlinks)?;
        }
    }
    Ok(())
}

fn copy_path_preserving_with_hardlinks(
    source: &Path,
    destination: &Path,
    metadata: &fs::Metadata,
    hardlinks: &mut HardlinkMap,
) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.is_file() && metadata.nlink() > 1 {
            let identity = (metadata.dev(), metadata.ino());
            if let Some(first_destination) = hardlinks.get(&identity) {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::hard_link(first_destination, destination).with_context(|| {
                    format!(
                        "failed to preserve hardlink {} -> {}",
                        destination.display(),
                        first_destination.display()
                    )
                })?;
                return Ok(());
            }
            copy_path_preserving(source, destination)?;
            hardlinks.insert(identity, destination.to_path_buf());
            return Ok(());
        }
    }
    copy_path_preserving(source, destination)
}

pub(crate) fn copy_path_preserving(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("required package input missing at {}", source.display()))?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    if metadata.file_type().is_symlink() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(fs::read_link(source)?, destination)?;
        #[cfg(not(unix))]
        bail!("package symlink staging requires Unix")
    } else {
        copy_preserving(source, destination)?;
    }
    Ok(())
}

pub(crate) fn stage_coreutils(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = resolve_coreutils_multicall(repo_root)?;
    stage_executable(&source, &staging.join("usr/bin/coreutils"), 0o755)?;
    let applets = package_coreutils_applets(&source)?;
    #[cfg(unix)]
    for applet in applets {
        let path = staging.join("usr/bin").join(&applet);
        if path_entry_exists(&path) {
            bail!("duplicate coreutils command alias {applet}")
        }
        std::os::unix::fs::symlink("coreutils", path)?;
    }
    Ok(())
}

pub(crate) fn stage_executable(source: &Path, destination: &Path, mode: u32) -> Result<()> {
    if !source.is_file() {
        bail!("required package input missing at {}", source.display())
    }
    copy_preserving(source, destination)?;
    set_mode(destination.to_path_buf(), mode)
}

pub(crate) fn copy_preserving(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(source)?.permissions().mode();
        fs::set_permissions(destination, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

#[cfg(test)]
mod mattos_plasma_theme_tests {
    use super::*;

    const TEMPLATE: &str = "floating=@@MATTOS_PANEL_FLOATING@@;length=@@MATTOS_PANEL_LENGTH_MODE@@;opacity=@@MATTOS_PANEL_OPACITY@@;hiding=@@MATTOS_PANEL_HIDING@@;height=@@MATTOS_PANEL_THICKNESS@@";

    #[test]
    fn system_defaults_select_the_bundled_nordic_color_scheme() {
        let kdeglobals = include_str!("../../../../system/desktop/branding/MattOS/xdg/kdeglobals");
        let look_and_feel_defaults = include_str!("../../../../system/desktop/branding/MattOS/contents/defaults");
        let nordic = include_str!("../../../../desktop/themes/nordic-kde/colors");

        assert!(kdeglobals.contains("ColorScheme=Nordic"));
        assert!(look_and_feel_defaults.contains("[kdeglobals][General]\nColorScheme=Nordic"));
        assert!(nordic.contains("Name=Nordic"));
    }

    #[test]
    fn panel_policy_is_consumed_and_mapped_to_supported_plasma_values() {
        let policy =
            "[Panel]\nfloating=true\nlengthMode=0\nopacityMode=0\nvisibilityMode=0\nthickness=60\n";
        assert_eq!(
            render_mattos_panel_layout(TEMPLATE, policy).unwrap(),
            "floating=true;length=fill;opacity=adaptive;hiding=none;height=60"
        );
    }

    #[test]
    fn panel_policy_fails_closed_on_unknown_or_out_of_range_values() {
        assert!(render_mattos_panel_layout(TEMPLATE, "[Panel]\nfloating=true\nlengthMode=8\nopacityMode=0\nvisibilityMode=0\nthickness=60\n").is_err());
        assert!(render_mattos_panel_layout(TEMPLATE, "[Panel]\nfloating=true\nlengthMode=0\nopacityMode=0\nvisibilityMode=0\nthickness=999\n").is_err());
        assert!(
            render_mattos_panel_layout(
                TEMPLATE,
                "[Panel]\nfloating=true\nlengthMode=0\nopacityMode=0\nvisibilityMode=0\n"
            )
            .is_err()
        );
    }

    #[test]
    fn wallpaper_mount_is_preserved_without_packaging_wallpaper_files() {
        let layout = include_str!(
            "../../../../system/desktop/branding/MattOS/contents/layouts/org.kde.plasma.desktop-layout.js"
        );
        assert!(layout.contains("/mnt/storage/OneDrive/Media/Wallpapers/Wide/"));
        assert!(layout.contains("/usr/share/wallpapers/"));
        let rendered = render_mattos_panel_layout(
            layout,
            "[Panel]\nfloating=true\nlengthMode=0\nopacityMode=0\nvisibilityMode=0\nthickness=60\n",
        )
        .unwrap();
        assert!(!rendered.contains("@@MATTOS_PANEL_"));
        assert!(rendered.contains("/mnt/storage/OneDrive/Media/Wallpapers/Wide/"));
    }

    #[test]
    fn plasma_workspace_filter_transfers_only_the_ten_custom_categories() {
        for name in MATTOS_DESKTOP_DIRECTORY_OVERRIDES {
            assert!(is_mattos_desktop_directory_override(
                Path::new("share/desktop-directories").join(name).as_path()
            ));
        }
        assert!(!is_mattos_desktop_directory_override(Path::new(
            "share/desktop-directories/kf5-audio.directory"
        )));
        assert!(!is_mattos_desktop_directory_override(Path::new(
            "share/applications/kf5-games.directory"
        )));
    }

    #[test]
    fn papirus_dark_inherits_the_full_app_icon_theme_without_changing_upstream() {
        let upstream = include_str!("../../../../desktop/themes/papirus-icon-theme/Papirus-Dark/index.theme");
        let base = include_str!("../../../../desktop/themes/papirus-icon-theme/Papirus/index.theme");
        let staged = add_papirus_base_fallback(upstream).unwrap();
        assert!(staged.contains("Inherits=Papirus,breeze-dark,hicolor"));
        assert_eq!(upstream.matches("Inherits=breeze-dark,hicolor").count(), 1);
        assert!(base.contains("[Icon Theme]"));
        let apps = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../desktop/themes/papirus-icon-theme/Papirus/24x24/apps");
        for app_icon in ["org.kde.dolphin.svg", "kate.svg", "utilities-terminal.svg"] {
            assert!(apps.join(app_icon).is_file(), "Papirus app fallback lacks {app_icon}");
        }
    }

    #[test]
    fn papirus_theme_patch_rejects_unknown_or_ambiguous_upstream_metadata() {
        assert!(add_papirus_base_fallback("[Icon Theme]\nInherits=breeze-dark,hicolor\n").is_ok());
        assert!(add_papirus_base_fallback("[Icon Theme]\nInherits=hicolor\n").is_err());
        assert!(add_papirus_base_fallback("Inherits=breeze-dark,hicolor\nInherits=breeze-dark,hicolor\n").is_err());
    }
}
