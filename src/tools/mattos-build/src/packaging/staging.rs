use super::*;

mod desktop;
mod development;
mod system;
mod toolchain;
pub(crate) use desktop::*;
use development::*;
pub(crate) use system::*;
pub(crate) use toolchain::*;

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
        "mattos-base" | "mattos-cli" | "mattos-plasma" | "mattos-toolchain"
        | "mattos-build-essential" => {
            stage_profile_package(repo_root, &staging, spec.name)?
        }
        "mattos-plasma-live" => stage_plasma_live_session_integration(repo_root, &staging)?,
        "mattos-plasma-theme" => stage_mattos_plasma_theme(repo_root, &staging)?,
        "ca-certificates" => stage_ca_certificates(repo_root, &staging)?,
        "mattos-brush" => stage_brush(repo_root, &staging)?,
        "sed" => stage_base_command(repo_root, &staging, "sed", "sed", &[], "src/userland/sed/COPYING")?,
        "dash" => stage_base_command(repo_root, &staging, "dash", "dash", &["sh"], "src/userland/dash/COPYING")?,
        "mawk" => stage_base_command(repo_root, &staging, "mawk", "mawk", &["awk"], "src/userland/mawk/COPYING")?,
        "rsync" => stage_base_command(repo_root, &staging, "rsync", "rsync", &[], "src/userland/rsync/COPYING")?,
        "pkgconf" => stage_pkgconf(repo_root, &staging)?,
        "perl" => stage_install_tree(repo_root, &staging, "perl", "perl", "src/development/perl", &["Copying", "Artistic"])?,
        "m4" => stage_install_tree(repo_root, &staging, "m4", "m4", "src/build-tools/m4", &["COPYING"])?,
        "autoconf" => stage_install_tree(
            repo_root,
            &staging,
            "autoconf",
            "autoconf",
            "src/build-tools/autoconf",
            &["COPYING", "COPYING.EXCEPTION"],
        )?,
        "automake" => stage_install_tree(repo_root, &staging, "automake", "automake", "src/build-tools/automake", &["COPYING"])?,
        // Libtool's Git tree has no license file; its verified release does.
        "libtool" => stage_install_tree(repo_root, &staging, "libtool", "libtool", "out/build/libtool/source", &["COPYING"])?,
        "ninja-build" => stage_install_tree(repo_root, &staging, "ninja", "ninja-build", "src/build-tools/ninja", &["COPYING"])?,
        "meson" => stage_meson(repo_root, &staging)?,
        "cmake" => stage_cmake(repo_root, &staging)?,
        name @ ("libncurses-dev" | "libcap-dev" | "libnl-3-dev" | "libnl-genl-3-dev"
        | "libsystemd-dev" | "libacl1-dev" | "libattr1-dev") => {
            stage_development_package(repo_root, &staging, name)?
        }
        "coreutils" => stage_coreutils(repo_root, &staging)?,
        "grep" => stage_uutils_commands(
            repo_root,
            &staging,
            "grep",
            &[("grep", "grep")],
            &[],
            &["LICENSE"],
        )?,
        "findutils" => stage_uutils_commands(
            repo_root,
            &staging,
            "findutils",
            &[("find", "find"), ("xargs", "xargs"), ("locate", "locate"), ("updatedb", "updatedb")],
            &[],
            &["LICENSE"],
        )?,
        "diffutils" => stage_uutils_commands(
            repo_root,
            &staging,
            "diffutils",
            &[("diffutils", "diffutils")],
            &[("diff", "diffutils"), ("cmp", "diffutils")],
            &["LICENSE-MIT", "LICENSE-APACHE"],
        )?,
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
        "ncurses-base" => stage_terminfo(repo_root, &staging)?,
        "procps" => stage_procps(repo_root, &staging)?,
        "udev" => stage_udev_hwdb(repo_root, &staging)?,
        "fontconfig" => stage_fontconfig(repo_root, &staging)?,
        "fonts-fira" => stage_pop_fonts(repo_root, &staging)?,
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
        "util-linux" => {
            stage_runtime_paths(repo_root, &staging, "util-linux", UTIL_LINUX_BASE_PATHS)?;
            copy_preserving(
                &repo_root.join("src/userland/util-linux/COPYING"),
                &staging.join("usr/share/doc/util-linux/copyright"),
            )?;
        }
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
        "libglvnd-dev" => stage_libglvnd_dev(repo_root, &staging)?,
        "libegl-mesa0" => stage_mesa_egl_vendor(repo_root, &staging)?,
        "libgl1-mesa-dri" => stage_mesa_dri_runtime(repo_root, &staging)?,
        "libvulkan-dev" => stage_vulkan_development(repo_root, &staging)?,
        "mesa-vulkan-drivers" => stage_mesa_vulkan_runtime(repo_root, &staging)?,
        "vulkan-tools" => stage_vulkan_tools(repo_root, &staging)?,
        "nvidia-firmware-595"
        | "libnvidia-gl-595"
        | "libnvidia-compute-595"
        | "libnvidia-encode-595"
        | "libnvidia-decode-595"
        | "nvidia-utils-595"
        | "nvidia-driver-595-open" => stage_nvidia_package(repo_root, &staging, spec.name)?,
        "flatpak" => stage_flatpak(repo_root, &staging)?,
        "xwayland" => stage_xwayland(repo_root, &staging)?,
        "xdg-desktop-portal" => stage_xdg_desktop_portal(repo_root, &staging)?,
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
        "libpam-modules" => stage_pam_modules(repo_root, &staging)?,
        "libpam-runtime" => stage_pam_runtime(repo_root, &staging)?,
        "passwd" => stage_shadow(repo_root, &staging)?,
        "mattos-sudo-rs" => stage_sudo_rs(repo_root, &staging)?,
        "login" => stage_util_linux_auth(repo_root, &staging)?,
        "iproute2" => stage_iproute2(repo_root, &staging)?,
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
        name => stage_table_package(repo_root, &staging, name)?,
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
            | NVIDIA_OPEN_MODULES_PACKAGE
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

    // install-info maintains the aggregate Info index on the installed system
    // (Debian Policy 12.2). A package that ships it collides with every other
    // package carrying manuals and publishes a partial, build-order-dependent
    // index; bundled component installs (flatpak's gpgme) otherwise leak one.
    remove_path_if_exists(&staging.join("usr/share/info/dir"))?;

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
            | "sed"
            | "dash"
            | "mawk"
            | "rsync"
            | "pkgconf"
            | "cmake"
            | "perl"
            | "m4"
            | "autoconf"
            | "automake"
            | "libtool"
            | "ninja-build"
            | "meson"
            | "libncurses-dev"
            | "libcap-dev"
            | "libnl-3-dev"
            | "libnl-genl-3-dev"
            | "libsystemd-dev"
            | "libacl1-dev"
            | "libattr1-dev"
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

/// Packages that ship one imported SONAME library plus its license notice:
/// (package, producing component, SONAME, license file).
const IMPORTED_SONAME_LIBRARIES: &[(&str, &str, &str, &str)] = &[
    (
        "libgpg-error0",
        "libgpg-error",
        "libgpg-error.so.0",
        "src/system/security/libgpg-error/COPYING.LIB",
    ),
    (
        "libgcrypt20",
        "libgcrypt",
        "libgcrypt.so.20",
        "src/system/security/libgcrypt/COPYING.LIB",
    ),
    (
        "libassuan9",
        "libassuan",
        "libassuan.so.9",
        "src/system/security/libassuan/COPYING.LIB",
    ),
    (
        "libksba8",
        "libksba",
        "libksba.so.8",
        "src/system/security/libksba/COPYING.LGPLv3",
    ),
    (
        "libnpth0",
        "npth",
        "libnpth.so.0",
        "src/system/security/npth/COPYING.LIB",
    ),
    (
        "libexpat1",
        "expat",
        "libexpat.so.1",
        "src/system/libraries/expat/expat/COPYING",
    ),
    (
        "libfreetype6",
        "freetype",
        "libfreetype.so.6",
        "src/system/libraries/freetype/LICENSE.TXT",
    ),
    (
        "libfontconfig1",
        "fontconfig",
        "libfontconfig.so.1",
        "src/system/libraries/fontconfig/COPYING",
    ),
    (
        "libcap2",
        "libcap",
        "libcap.so.2",
        "src/system/libraries/libcap/License",
    ),
    (
        "libattr1",
        "attr",
        "libattr.so.1",
        "src/system/libraries/attr/doc/COPYING.LGPL",
    ),
    (
        "libacl1",
        "acl",
        "libacl.so.1",
        "src/system/libraries/acl/doc/COPYING.LGPL",
    ),
    (
        "zlib1g",
        "zlib",
        "libz.so.1",
        "src/system/libraries/zlib/LICENSE",
    ),
    (
        "libbz2-1.0",
        "bzip2",
        "libbz2.so.1.0",
        "src/system/libraries/bzip2/LICENSE",
    ),
    (
        "liblz4-1",
        "lz4",
        "liblz4.so.1",
        "src/system/libraries/lz4/LICENSE",
    ),
    (
        "liblzma5",
        "xz",
        "liblzma.so.5",
        "src/system/libraries/xz/COPYING",
    ),
    (
        "libxxhash0",
        "xxhash",
        "libxxhash.so.0",
        "src/system/libraries/xxhash/LICENSE",
    ),
    (
        "libmd0",
        "libmd",
        "libmd.so.0",
        "src/system/libraries/libmd/COPYING",
    ),
    (
        "libbsd0",
        "libbsd",
        "libbsd.so.0",
        "src/system/libraries/libbsd/COPYING",
    ),
    (
        "libzstd1",
        "zstd",
        "libzstd.so.1",
        "src/system/libraries/zstd/LICENSE",
    ),
    (
        "mattos-libcrypto3",
        "openssl",
        "libcrypto.so.3",
        "src/system/libraries/openssl/LICENSE.txt",
    ),
    (
        "libssl3t64",
        "openssl",
        "libssl.so.3",
        "src/system/libraries/openssl/LICENSE.txt",
    ),
    (
        "libpcre2-8-0",
        "pcre2",
        "libpcre2-8.so.0",
        "src/system/libraries/pcre2/LICENCE.md",
    ),
    (
        "libselinux1",
        "selinux",
        "libselinux.so.1",
        "src/system/security/selinux/libselinux/LICENSE",
    ),
    (
        "libcrypt1",
        "libxcrypt",
        "libcrypt.so.1",
        "src/system/libraries/libxcrypt/COPYING.LIB",
    ),
    (
        "libblkid1",
        "util-linux",
        "libblkid.so.1",
        "src/userland/util-linux/COPYING",
    ),
    (
        "libmount1",
        "util-linux",
        "libmount.so.1",
        "src/userland/util-linux/COPYING",
    ),
    (
        "libsmartcols1",
        "util-linux",
        "libsmartcols.so.1",
        "src/userland/util-linux/COPYING",
    ),
    (
        "libuuid1",
        "util-linux",
        "libuuid.so.1",
        "src/userland/util-linux/COPYING",
    ),
    (
        "libfdisk1",
        "util-linux",
        "libfdisk.so.1",
        "src/userland/util-linux/COPYING",
    ),
    (
        "libcanberra0",
        "libcanberra",
        "libcanberra.so.0",
        "src/system/libraries/libcanberra/LGPL",
    ),
    (
        "libxml2-16",
        "libxml2",
        "libxml2.so.16",
        "src/system/libraries/libxml2/Copyright",
    ),
    (
        "libxkbfile1",
        "libxkbfile",
        "libxkbfile.so.1",
        "src/system/graphics/libxkbfile/COPYING",
    ),
    (
        "libwayland-client0",
        "wayland",
        "libwayland-client.so.0",
        "src/system/libraries/wayland/COPYING",
    ),
    (
        "libwayland-cursor0",
        "wayland",
        "libwayland-cursor.so.0",
        "src/system/libraries/wayland/COPYING",
    ),
    (
        "libwayland-server0",
        "wayland",
        "libwayland-server.so.0",
        "src/system/libraries/wayland/COPYING",
    ),
    (
        "libwayland-egl1",
        "wayland",
        "libwayland-egl.so.1",
        "src/system/libraries/wayland/COPYING",
    ),
    (
        "libevdev2",
        "libevdev",
        "libevdev.so.2",
        "src/system/libraries/libevdev/COPYING",
    ),
    (
        "libpixman-1-0",
        "pixman",
        "libpixman-1.so.0",
        "src/system/libraries/pixman/COPYING",
    ),
    (
        "libdrm2",
        "libdrm",
        "libdrm.so.2",
        "src/system/libraries/libdrm/README.rst",
    ),
    (
        "libdrm-nouveau2",
        "libdrm",
        "libdrm_nouveau.so.2",
        "src/system/libraries/libdrm/README.rst",
    ),
    (
        "libxau6",
        "x11-compat",
        "libXau.so.6",
        "src/system/graphics/libxau/COPYING",
    ),
    (
        "libxdmcp6",
        "x11-compat",
        "libXdmcp.so.6",
        "src/system/graphics/libxdmcp/COPYING",
    ),
    (
        "libice6",
        "x11-compat",
        "libICE.so.6",
        "src/system/graphics/libice/COPYING",
    ),
    (
        "libsm6",
        "x11-compat",
        "libSM.so.6",
        "src/system/graphics/libsm/COPYING",
    ),
    (
        "libxi6",
        "x11-compat",
        "libXi.so.6",
        "src/system/graphics/libxi/COPYING",
    ),
    (
        "libxrender1",
        "x11-compat",
        "libXrender.so.1",
        "src/system/graphics/libxrender/COPYING",
    ),
    (
        "libxtst6",
        "x11-compat",
        "libXtst.so.6",
        "src/system/graphics/libxtst/COPYING",
    ),
    (
        "libxcursor1",
        "x11-compat",
        "libXcursor.so.1",
        "src/system/graphics/libxcursor/COPYING",
    ),
    (
        "libxft2",
        "x11-compat",
        "libXft.so.2",
        "src/system/graphics/libxft/COPYING",
    ),
    (
        "libxext6",
        "x11-compat",
        "libXext.so.6",
        "src/system/graphics/libxext/COPYING",
    ),
    (
        "libxfixes3",
        "x11-compat",
        "libXfixes.so.3",
        "src/system/graphics/libxfixes/COPYING",
    ),
    (
        "libglvnd0",
        "libglvnd",
        "libGLdispatch.so.0",
        "src/system/graphics/libglvnd/README.md",
    ),
    (
        "libglx0",
        "libglvnd",
        "libGLX.so.0",
        "src/system/graphics/libglvnd/README.md",
    ),
    (
        "libgl1",
        "libglvnd",
        "libGL.so.1",
        "src/system/graphics/libglvnd/README.md",
    ),
    (
        "libopengl0",
        "libglvnd",
        "libOpenGL.so.0",
        "src/system/graphics/libglvnd/README.md",
    ),
    (
        "libgbm1",
        "mesa",
        "libgbm.so.1",
        "src/system/graphics/mesa/docs/license.rst",
    ),
    (
        "libegl1",
        "libglvnd",
        "libEGL.so.1",
        "src/system/graphics/libglvnd/README.md",
    ),
    (
        "libgles1",
        "libglvnd",
        "libGLESv1_CM.so.1",
        "src/system/graphics/libglvnd/README.md",
    ),
    (
        "libgles2",
        "libglvnd",
        "libGLESv2.so.2",
        "src/system/graphics/libglvnd/README.md",
    ),
    (
        "libvulkan1",
        "vulkan-loader",
        "libvulkan.so.1",
        "src/system/graphics/vulkan-loader/LICENSE.txt",
    ),
    (
        "libnl-3-200",
        "libnl",
        "libnl-3.so.200",
        "src/system/network/libnl/COPYING",
    ),
    (
        "libnl-genl-3-200",
        "libnl",
        "libnl-genl-3.so.200",
        "src/system/network/libnl/COPYING",
    ),
    (
        "libnl-route-3-200",
        "libnl",
        "libnl-route-3.so.200",
        "src/system/network/libnl/COPYING",
    ),
    (
        "libdav1d7",
        "dav1d",
        "libdav1d.so.7",
        "src/system/multimedia/dav1d/COPYING",
    ),
];

/// Packages that ship a family of shared-library files and SONAME links:
/// (package, producing component, files in usr/lib/x86_64-linux-gnu).
const LIBRARY_FAMILIES: &[(&str, &str, &[&str])] = &[
    (
        "mattos-libtinfow6",
        "ncurses",
        &["libtinfow.so.6.6", "libtinfow.so.6"],
    ),
    (
        "libncursesw6",
        "ncurses",
        &[
            "libncursesw.so.6.6",
            "libncursesw.so.6",
            "libpanelw.so.6.6",
            "libpanelw.so.6",
        ],
    ),
    (
        "libreadline8",
        "readline",
        &["libreadline.so.8.2", "libreadline.so.8"],
    ),
    ("libndp0", "libndp", &["libndp.so.0", "libndp.so.0.3.0"]),
    ("libkmod2", "kmod", &["libkmod.so.2.5.1", "libkmod.so.2"]),
    (
        "mattos-libproc2",
        "procps-ng",
        &["libproc2.so.1.0.1", "libproc2.so.1"],
    ),
    (
        "libsystemd0",
        "systemd",
        &["libsystemd.so.0.44.0", "libsystemd.so.0"],
    ),
    (
        "libudev1",
        "systemd",
        &["libudev.so.1.7.14", "libudev.so.1"],
    ),
    (
        "libpam0g",
        "linux-pam",
        &["libpam.so.0.85.1", "libpam.so.0"],
    ),
    (
        "mattos-libpam-misc0",
        "linux-pam",
        &["libpam_misc.so.0.82.1", "libpam_misc.so.0"],
    ),
];

/// Packages that ship listed paths of their component's install tree:
/// (package, producing component, install-relative paths).
const RUNTIME_PATH_PACKAGES: &[(&str, &str, &[&str])] = &[
    ("ncurses-bin", "ncurses", NCURSES_RUNTIME_PATHS),
    ("kmod", "kmod", KMOD_RUNTIME_PATHS),
    (
        "gzip",
        "gzip",
        &["usr/bin/gzip", "usr/bin/gunzip", "usr/bin/zcat"],
    ),
    (
        "bzip2",
        "bzip2",
        &[
            "usr/bin/bzip2",
            "usr/bin/bunzip2",
            "usr/bin/bzcat",
            "usr/bin/bzip2recover",
        ],
    ),
    (
        "xz-utils",
        "xz",
        &[
            "usr/bin/xz",
            "usr/bin/unxz",
            "usr/bin/xzcat",
            "usr/bin/lzma",
            "usr/bin/unlzma",
            "usr/bin/lzcat",
        ],
    ),
    (
        "zstd",
        "zstd",
        &["usr/bin/zstd", "usr/bin/unzstd", "usr/bin/zstdcat"],
    ),
    ("patch", "patch", &["usr/bin/patch"]),
    ("file", "file", &["usr/bin/file"]),
    (
        "less",
        "less",
        &["usr/bin/less", "usr/bin/lesskey", "usr/libexec/lessecho"],
    ),
    (
        "libduktape207",
        "duktape",
        &[
            "usr/lib/x86_64-linux-gnu/libduktape.so.207.2.7.0",
            "usr/lib/x86_64-linux-gnu/libduktape.so.207",
            "usr/lib/x86_64-linux-gnu/libduktape.so",
        ],
    ),
    ("iputils-ping", "iputils", IPUTILS_RUNTIME_PATHS),
];

/// Every package staged from one of the tables below, in table order.
#[cfg(test)]
pub(crate) fn table_packages() -> Vec<&'static str> {
    IMPORTED_SONAME_LIBRARIES
        .iter()
        .map(|row| row.0)
        .chain(LIBRARY_FAMILIES.iter().map(|row| row.0))
        .chain(RUNTIME_PATH_PACKAGES.iter().map(|row| row.0))
        .chain(KERNEL_RELEASE_PACKAGES.iter().map(|row| row.0))
        .collect()
}

/// The license notice of every package in `IMPORTED_SONAME_LIBRARIES`.
#[cfg(test)]
pub(crate) fn imported_soname_library_licenses() -> Vec<&'static str> {
    IMPORTED_SONAME_LIBRARIES.iter().map(|row| row.3).collect()
}

/// Packages named after the kernel release (`mattos_kernel_release!`): their
/// names change with every kernel update, so no dispatcher arm names them.
const KERNEL_RELEASE_PACKAGES: &[(&str, fn(&Path, &Path) -> Result<()>)] = &[
    (LINUX_MODULES_PACKAGE, stage_linux_modules),
    (NVIDIA_OPEN_MODULES_PACKAGE, |repo_root, staging| {
        stage_nvidia_package(repo_root, staging, NVIDIA_OPEN_MODULES_PACKAGE)
    }),
];

/// Stages a package described by one of the tables above.
fn stage_table_package(repo_root: &Path, staging: &Path, package: &str) -> Result<()> {
    if let Some((_, stage)) = KERNEL_RELEASE_PACKAGES.iter().find(|row| row.0 == package) {
        return stage(repo_root, staging);
    }
    if let Some((_, component, soname, license)) = IMPORTED_SONAME_LIBRARIES
        .iter()
        .find(|row| row.0 == package)
    {
        return stage_imported_soname_library(
            repo_root, staging, component, soname, license, package,
        );
    }
    if let Some((_, component, files)) = LIBRARY_FAMILIES.iter().find(|row| row.0 == package) {
        return stage_library_family(repo_root, staging, component, files);
    }
    if let Some((_, component, paths)) = RUNTIME_PATH_PACKAGES.iter().find(|row| row.0 == package) {
        return stage_runtime_paths(repo_root, staging, component, paths);
    }
    bail!("no staging implementation for {package}")
}

pub(crate) fn component_install(repo_root: &Path, component: &str) -> PathBuf {
    repo_root.join("out/build").join(component).join("install")
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
            if memchr::memmem::find(&bytes, needle.as_bytes()).is_some() {
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
    let strip = cross_binutil(repo_root, "strip");
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
