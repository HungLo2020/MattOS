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
            "src/system/libraries/libffi/libffi/LICENSE",
            "libffi8",
        )?,
        "libffi-dev" => stage_libffi_dev(repo_root, &staging)?,
        "libxkbcommon0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "xkbcommon",
            "libxkbcommon.so.0",
            "src/system/libraries/xkbcommon/LICENSE",
            "libxkbcommon0",
        )?,
        "libwayland-client0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "wayland",
            "libwayland-client.so.0",
            "src/system/libraries/wayland/COPYING",
            "libwayland-client0",
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
            "src/system/libraries/seatd/LICENSE",
            "libseat1",
        )?,
        "libdisplay-info3" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libdisplay-info",
            "libdisplay-info.so.3",
            "src/system/libraries/libdisplay-info/LICENSE",
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
        },
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
        "libxcb1" => stage_imported_soname_library(
            repo_root,
            &staging,
            "x11-compat",
            "libxcb.so.1",
            "src/system/graphics/libxcb/COPYING",
            "libxcb1",
        )?,
        "libx11-6" => {
            stage_imported_soname_library(
                repo_root,
                &staging,
                "x11-compat",
                "libX11.so.6",
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
        "libglvnd0" => stage_imported_soname_library(
            repo_root,
            &staging,
            "libglvnd",
            "libGLdispatch.so.0",
            "src/system/graphics/libglvnd/README.md",
            "libglvnd0",
        )?,
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
        "cosmic-comp" => {
            stage_runtime_paths(repo_root, &staging, "cosmic-comp", &["usr/bin/cosmic-comp"])?;
            for (source, destination) in [
                (
                    "src/desktop/cosmic/cosmic-comp/data/keybindings.ron",
                    "usr/share/cosmic/com.system76.CosmicSettings.Shortcuts/v1/defaults",
                ),
                (
                    "src/desktop/cosmic/cosmic-comp/data/tiling-exceptions.ron",
                    "usr/share/cosmic/com.system76.CosmicSettings.WindowRules/v1/tiling_exception_defaults",
                ),
            ] {
                copy_preserving(&repo_root.join(source), &staging.join(destination))?;
            }
        }
        "flatpak" => stage_flatpak(repo_root, &staging)?,
        "xwayland" => stage_xwayland(repo_root, &staging)?,
        "xdg-desktop-portal" => stage_xdg_desktop_portal(repo_root, &staging)?,
        "cosmic-desktop" => stage_cosmic_desktop(repo_root, &staging)?,
        "cosmic-edit" => stage_cosmic_edit(repo_root, &staging)?,
        "cosmic-initial-setup" => stage_cosmic_initial_setup(repo_root, &staging)?,
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
            // Flatpak's system AppStream transaction authenticates through
            // polkit-agent-helper-1. Preserve its upstream setuid-root
            // contract when copying the runtime subset into the package.
            set_mode(staging.join("usr/lib/polkit-1/polkit-agent-helper-1"), 0o4755)
        })?,
        "network-manager" => stage_network_manager(repo_root, &staging)?,
        "libnl-3-200" => stage_imported_soname_library(repo_root, &staging, "libnl", "libnl-3.so.200", "src/system/network/libnl/COPYING", "libnl-3-200")?,
        "libnl-genl-3-200" => stage_imported_soname_library(repo_root, &staging, "libnl", "libnl-genl-3.so.200", "src/system/network/libnl/COPYING", "libnl-genl-3-200")?,
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
            // COSMIC's upstream session launcher uses a private reference bus
            // when a login manager has not supplied a user bus (including the
            // live installer compositor). dbus-broker remains the sole
            // systemd-managed system/user daemon.
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
            for relative in ["usr/bin", "usr/sbin", "usr/libexec", "usr/share/man", "etc"] {
                copy_tree_preserving(&install.join(relative), &staging.join(relative))?;
            }
        }
        _ => bail!("no staging implementation for {}", spec.name),
    }

    // NVIDIA's redistribution grant requires its userspace binaries to remain
    // unmodified. Open modules are already compressed; preserve every file in
    // this separately versioned stack byte-for-byte after extraction.
    if !matches!(
        spec.name,
        "libc6"
            | "libgcc-s1"
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
        &repo_root.join("src/system/libraries/libffi/libffi/LICENSE"),
        &staging.join("usr/share/doc/libffi-dev/copyright"),
    )
}

fn stage_mattos_installer(repo_root: &Path, staging: &Path) -> Result<()> {
    let installer = repo_root.join("out/build/installer");
    stage_executable(
        &installer.join("cargo-target/release/mattos-install"),
        &staging.join("usr/bin/mattos-install"),
        0o755,
    )?;
    stage_executable(
        &installer.join("cosmic-target/release/mattos-install-cosmic"),
        &staging.join("usr/bin/mattos-install-cosmic"),
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
        (repo_root.join("out/build/linux/kernel-release"), "kernel-release"),
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
    for name in [
        "mattos-install-cli.service",
        "mattos-install-cli.target",
        "mattos-install-graphical.service",
        "mattos-install-graphical.target",
        "mattos-cosmic-installer-session.service",
    ] {
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
        },
    )?;
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
    for relative in [
        "lib/python3.14/ensurepip",
        "lib/python3.14/venv",
        "lib/python3.14/site-packages",
    ] {
        copy_tree_preserving(&install.join(relative), &staging.join("usr").join(relative))?;
    }
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
    copy_preserving(
        &repo_root.join("src/development/python/cpython/LICENSE"),
        &staging.join("usr/share/doc/python3-dev/copyright"),
    )
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
    copy_tree_preserving(&install.join("lib"), &staging.join("usr/lib"))?;
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
/// the three registries consumed by COSMIC Initial Setup.
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
    copy_preserving(
        &source.join("LICENSE"),
        &staging.join("usr/share/doc/tzdata/copyright"),
    )?;
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
    for name in ["WHENCE", "README.md", "LICENSE"] {
        copy_preserving(&source.join(name), &documentation.join(name))?;
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
    copy_preserving(&source.join("LICENSE"), &documentation.join("copyright"))?;
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
        "mattos.sources",
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
    for unit in ["mattos-apt-daily.service", "mattos-apt-daily.timer", "mattos-apt-bootstrap.service", "mattos-apt-bootstrap.timer"] {
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
        "libxml2",
        "libarchive",
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
    let policy = fs::read_to_string(descriptor)
        .with_context(|| format!("failed to read Flatpak remote policy {}", descriptor.display()))?;
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
        "freetype",
        "libfontenc",
        "libxfont",
        "libxcvt",
        "libxshmfence",
        "libxkbfile",
        "xkbcomp",
        "xwayland",
    ] {
        copy_component_usr_and_etc(repo_root, staging, component)?;
    }
    Ok(())
}

pub(crate) fn stage_xdg_desktop_portal(repo_root: &Path, staging: &Path) -> Result<()> {
    // The generic broker and its GStreamer pbutils closure ship together. The
    // portal executes Bubblewrap at /usr/bin/bwrap, but Flatpak owns that
    // target-built executable and is the portal package's declared runtime
    // dependency. Copying it here would create two Debian package owners for
    // the same path. The COSMIC backend remains in cosmic-desktop because it
    // is a separate first-class upstream source.
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
        assert_eq!(fs::read(file).unwrap(), fs::read(target.path().join(relative)).unwrap());
    }
}

fn stage_cosmic_desktop(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cosmic-desktop");
    copy_tree_preserving(&install.join("usr"), &staging.join("usr"))?;
    // The pinned settings schema carries the complete v1 component styling,
    // while the matching libcosmic v2 model still requires list_button. Keep
    // this MattOS integration in package staging so an integration-only change
    // does not invalidate and rebuild every upstream COSMIC workspace.
    for theme in ["Dark", "Light"] {
        let theme_root = staging.join(format!("usr/share/cosmic/com.system76.CosmicTheme.{theme}"));
        let source = theme_root.join("v1/list_button");
        let destination = theme_root.join("v2/list_button");
        if !source.is_file() || !destination.parent().is_some_and(Path::is_dir) {
            bail!("COSMIC {theme} theme lacks the expected v1/v2 schemas");
        }
        fs::copy(source, destination)?;
    }
    fs::write(
        staging.join("usr/share/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom"),
        "{}\n",
    )?;
    let start_cosmic = staging.join("usr/bin/start-cosmic");
    let wayland_session = fs::read_to_string(&start_cosmic)?
        .replace("GDK_BACKEND=wayland,x11", "GDK_BACKEND=wayland")
        .replace("QT_QPA_PLATFORM=\"wayland;xcb\"", "QT_QPA_PLATFORM=wayland")
        .replace(
            "XDG_SESSION_TYPE XDG_CURRENT_DESKTOP DCONF_PROFILE SSH_AUTH_SOCK",
            "XDG_SESSION_TYPE XDG_CURRENT_DESKTOP DCONF_PROFILE XDG_DATA_DIRS SSH_AUTH_SOCK",
        )
        .replace(
            "exec /usr/bin/dbus-run-session -- /usr/bin/cosmic-session",
            "exec /usr/bin/dbus-run-session --config-file=/usr/share/dbus-1/mattos-private-session.conf -- /usr/bin/cosmic-session",
        );
    let flatpak_environment = "# Flatpak exports desktop entries and themed icons outside the standard XDG\n# locations. Make them visible to COSMIC, its launcher, and session helpers.\nflatpak_user_exports=\"${XDG_DATA_HOME:-$HOME/.local/share}/flatpak/exports/share\"\nflatpak_system_exports=\"/var/lib/flatpak/exports/share\"\nexport XDG_DATA_DIRS=\"${flatpak_user_exports}:${flatpak_system_exports}:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}\"";
    let dconf_anchor = [
        "export DCONF_PROFILE=/usr/share/dconf/profile/cosmic",
        "export DCONF_PROFILE=cosmic",
    ]
    .into_iter()
    .find(|anchor| wayland_session.contains(anchor))
    .ok_or_else(|| anyhow::anyhow!("pinned start-cosmic is missing its DCONF_PROFILE export"))?;
    let wayland_session = wayland_session.replace(
        dconf_anchor,
        &format!("{dconf_anchor}\n\n{flatpak_environment}"),
    );
    let rewritten = &wayland_session;
    for required in [
        "export XDG_DATA_DIRS=",
        "flatpak_user_exports=",
        "flatpak_system_exports=",
        "systemctl --user import-environment XDG_SESSION_TYPE XDG_CURRENT_DESKTOP DCONF_PROFILE XDG_DATA_DIRS SSH_AUTH_SOCK",
    ] {
        if !rewritten.contains(required) {
            bail!("packaged start-cosmic is missing Flatpak desktop/icon environment: {required}");
        }
    }
    fs::write(&start_cosmic, wayland_session)?;

    let integration = repo_root.join("src/system/session/cosmic");
    copy_preserving(
        &integration.join("mattos-graphics-report"),
        &staging.join("usr/bin/mattos-graphics-report"),
    )?;
    set_mode(staging.join("usr/bin/mattos-graphics-report"), 0o755)?;
    copy_preserving(
        &integration.join("mattos-graphics-startup"),
        &staging.join("usr/bin/mattos-graphics-startup"),
    )?;
    set_mode(staging.join("usr/bin/mattos-graphics-startup"), 0o755)?;
    copy_preserving(
        &integration.join("cosmic-greeter.toml"),
        &staging.join("etc/greetd/cosmic-greeter.toml"),
    )?;
    copy_preserving(
        &integration.join("cosmic-greeter.pam"),
        &staging.join("etc/pam.d/cosmic-greeter"),
    )?;
    copy_preserving(
        &integration.join("cosmic-greeter-start"),
        &staging.join("usr/bin/cosmic-greeter-start"),
    )?;
    set_mode(staging.join("usr/bin/cosmic-greeter-start"), 0o755)?;
    for unit in [
        "cosmic-greeter.service",
        "cosmic-greeter-daemon.service",
        "mattos-graphics-watchdog.service",
        "mattos-graphics-recovery.service",
        "mattos-graphics-capture.service",
    ] {
        copy_preserving(
            &integration.join(unit),
            &staging.join("usr/lib/systemd/system").join(unit),
        )?;
    }
    let wants = staging.join("etc/systemd/system/multi-user.target.wants");
    fs::create_dir_all(&wants)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        "/usr/lib/systemd/system/mattos-graphics-capture.service",
        wants.join("mattos-graphics-capture.service"),
    )?;
    copy_preserving(
        &integration.join("cosmic-desktop.conf"),
        &staging.join("usr/lib/environment.d/90-cosmic-desktop.conf"),
    )?;
    copy_preserving(
        &integration.join("README.md"),
        &staging.join("usr/share/doc/cosmic-desktop/README.md"),
    )?;
    copy_preserving(
        &integration.join("hicolor-index.theme"),
        &staging.join("usr/share/icons/hicolor/index.theme"),
    )?;

    let display_manager = staging.join("etc/systemd/system/display-manager.service");
    fs::create_dir_all(display_manager.parent().expect("display-manager parent"))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        "/usr/lib/systemd/system/cosmic-greeter.service",
        &display_manager,
    )?;
    for required in [
        "usr/bin/cosmic-session",
        "usr/bin/cosmic-panel",
        "usr/bin/cosmic-launcher",
        "usr/bin/cosmic-term",
        "usr/bin/cosmic-ext-tweaks",
        "usr/bin/cosmic-ext-calculator",
        "usr/bin/cosmic-ext-storage",
        "usr/bin/cosmic-monitor",
        "usr/bin/cosmic-store",
        "usr/bin/greetd",
        "usr/bin/cosmic-greeter-start",
        "usr/share/wayland-sessions/cosmic.desktop",
        "usr/share/icons/Pop/cursors/default",
        "usr/share/icons/hicolor/index.theme",
        "usr/share/fonts/truetype/open-sans/OpenSans-Regular.ttf",
        "usr/share/fonts/truetype/noto/NotoSansMono[wdth,wght].ttf",
        "etc/pam.d/cosmic-greeter",
        "etc/systemd/system/display-manager.service",
    ] {
        if fs::symlink_metadata(staging.join(required)).is_err() {
            bail!("cosmic-desktop package is missing /{required}");
        }
    }
    let launcher = fs::read_to_string(staging.join("usr/bin/cosmic-greeter-start"))?;
    for contract in [
        "LIBSEAT_BACKEND=logind",
        "XDG_SESSION_TYPE=wayland",
        "cosmic-comp --no-xwayland /usr/bin/cosmic-greeter",
    ] {
        if !launcher.contains(contract) {
            bail!("COSMIC greeter launcher is missing runtime contract: {contract}");
        }
    }
    let greeter_unit =
        fs::read_to_string(staging.join("usr/lib/systemd/system/cosmic-greeter.service"))?;
    if !greeter_unit.contains("Restart=always")
        || !greeter_unit.contains("After=systemd-user-sessions.service systemd-logind.service")
        || !greeter_unit.contains("TimeoutStopSec=10s")
    {
        bail!(
            "COSMIC display manager lacks logind ordering, bounded shutdown, or restart recovery"
        );
    }
    Ok(())
}

fn stage_cosmic_edit(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cosmic-edit");
    copy_tree_preserving(&install.join("usr"), &staging.join("usr"))?;
    copy_preserving(
        &repo_root.join("src/desktop/cosmic/cosmic-edit/LICENSE"),
        &staging.join("usr/share/doc/cosmic-edit/copyright"),
    )?;
    for required in [
        "usr/bin/cosmic-edit",
        "usr/share/applications/com.system76.CosmicEdit.desktop",
        "usr/share/metainfo/com.system76.CosmicEdit.metainfo.xml",
    ] {
        if !staging.join(required).is_file() {
            bail!("cosmic-edit package is missing /{required}");
        }
    }
    let desktop =
        fs::read_to_string(staging.join("usr/share/applications/com.system76.CosmicEdit.desktop"))?;
    if !desktop.contains("Exec=cosmic-edit %F") || !desktop.contains("MimeType=text/plain;") {
        bail!("cosmic-edit desktop entry does not advertise the expected editor contract");
    }
    Ok(())
}

fn stage_cosmic_initial_setup(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "cosmic-initial-setup");
    copy_tree_preserving(&install.join("usr"), &staging.join("usr"))?;
    copy_tree_preserving(&install.join("etc"), &staging.join("etc"))?;
    let launcher = staging.join("usr/libexec/mattos/cosmic-initial-setup-autostart");
    if let Some(parent) = launcher.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &launcher,
        "#!/bin/sh\n# Live media starts COSMIC without running the installed-user wizard.\n[ ! -e /run/mattos-live ] || exit 0\nexec /usr/bin/cosmic-initial-setup\n",
    )?;
    set_mode(launcher, 0o755)?;
    let desktop =
        staging.join("etc/xdg/autostart/com.system76.CosmicInitialSetup.Autostart.desktop");
    let body = fs::read_to_string(&desktop)?.replace(
        "Exec=cosmic-initial-setup",
        "Exec=/usr/libexec/mattos/cosmic-initial-setup-autostart",
    );
    fs::write(desktop, body)?;
    copy_preserving(
        &repo_root.join("src/desktop/cosmic/cosmic-initial-setup/LICENSE"),
        &staging.join("usr/share/doc/cosmic-initial-setup/copyright"),
    )?;
    for rel in [
        "usr/bin/cosmic-initial-setup",
        "usr/share/applications/com.system76.CosmicInitialSetup.desktop",
        "etc/xdg/autostart/com.system76.CosmicInitialSetup.Autostart.desktop",
        "usr/share/icons/hicolor/scalable/apps/com.system76.CosmicInitialSetup.svg",
        "usr/share/polkit-1/rules.d/20-cosmic-initial-setup.rules",
        "usr/share/cosmic-layouts/top-panel-and-bottom-dock/layout.kdl",
        "usr/share/cosmic-layouts/top-panel-and-bottom-dock/icon.png",
        "usr/share/cosmic-themes/nebula-dark.ron",
    ] {
        if !staging.join(rel).is_file() {
            bail!("cosmic-initial-setup package is missing /{rel}");
        }
    }
    Ok(())
}

fn stage_grub_package(repo_root: &Path, staging: &Path) -> Result<()> {
    for directory in ["usr", "etc"] {
        copy_tree_preserving(&component_install(repo_root, "grub").join(directory), &staging.join(directory))?;
    }
    // These belong respectively to a global index and hybrid media, not the
    // installed x86_64 UEFI package. Do not exempt BIOS objects from ELF audit.
    remove_path_if_exists(&staging.join("usr/share/info/dir"))?;
    remove_path_if_exists(&staging.join("usr/lib/grub/i386-pc"))?;
    copy_preserving(&repo_root.join("src/boot/grub/upstream/COPYING"),
        &staging.join("usr/share/doc/grub-efi-amd64/copyright"))?;
    let policy = repo_root.join("src/boot/grub/config");
    stage_executable(&policy.join("update-grub"), &staging.join("usr/sbin/update-grub"), 0o755)?;
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
    fs::write(staging.join("DEBIAN/conffiles"), format!("{}\n", conffiles.join("\n")))?;
    Ok(())
}

fn stage_wpa_supplicant(repo_root: &Path, staging: &Path) -> Result<()> {
    copy_tree_preserving(&component_install(repo_root, "wpa-supplicant").join("usr"), &staging.join("usr"))?;
    // Upstream COPYING refers to README for the current BSD license terms.
    copy_preserving(&repo_root.join("src/system/network/hostap/wpa_supplicant/README"),
        &staging.join("usr/share/doc/wpasupplicant/copyright"))?;
    for relative in WPA_RUNTIME_FILES {
        if !staging.join(relative).is_file() {
            bail!("wpasupplicant package is missing /{relative}");
        }
    }
    Ok(())
}

const WPA_RUNTIME_FILES: &[&str] = &[
    "usr/sbin/wpa_supplicant", "usr/sbin/wpa_cli", "usr/sbin/wpa_passphrase",
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
        for relative in ["usr/lib/grub/i386-pc/linux.mod", "usr/lib/grub/x86_64-efi/linux.mod",
            "usr/share/info/dir", "etc/grub.d/00_header", "etc/grub.d/40_custom", "etc/grub.d/README"] {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, relative).unwrap();
        }
        for relative in ["src/boot/grub/upstream/COPYING", "src/boot/grub/config/update-grub", "src/boot/grub/config/kernel-hook"] {
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
        assert_eq!(files, "/etc/grub.d/00_header\n/etc/grub.d/40_custom\n/etc/kernel/postinst.d/zz-update-grub\n/etc/kernel/postrm.d/zz-update-grub\n");
        assert!(!staging.join("etc/default/grub").exists());
        assert!(!staging.join("boot/grub/grub.cfg").exists());
        assert!(staging.join("usr/share/doc/grub-efi-amd64/copyright").is_file());
    }
    #[test]
    fn stages_the_complete_directory_tree_and_rejects_missing_activation_files() {
        let fixture = tempfile::tempdir().unwrap();
        let install = fixture.path().join("out/build/wpa-supplicant/install");
        let license = fixture.path().join("src/system/network/hostap/wpa_supplicant/README");
        fs::create_dir_all(license.parent().unwrap()).unwrap();
        fs::write(&license, "upstream license").unwrap();
        for relative in WPA_RUNTIME_FILES {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, relative).unwrap();
        }
        let staging = fixture.path().join("payload");
        stage_wpa_supplicant(fixture.path(), &staging).unwrap();
        assert_eq!(fs::read_to_string(staging.join("usr/share/doc/wpasupplicant/copyright")).unwrap(), "upstream license");
        for relative in WPA_RUNTIME_FILES {
            assert_eq!(fs::read_to_string(staging.join(relative)).unwrap(), *relative);
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

fn stage_pam_modules(repo_root: &Path, staging: &Path) -> Result<()> {
    let source =
        component_install(repo_root, "linux-pam").join("usr/lib/x86_64-linux-gnu/security");
    let destination = staging.join("usr/lib/x86_64-linux-gnu/security");
    for module in PAM_MODULES {
        copy_preserving(&source.join(module), &destination.join(module))?;
    }
    Ok(())
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

fn validate_no_embedded_build_root(repo_root: &Path, staging: &Path) -> Result<()> {
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
        let status = Command::new(&strip)
            .arg("--strip-debug")
            .arg(&object)
            .status()?;
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
