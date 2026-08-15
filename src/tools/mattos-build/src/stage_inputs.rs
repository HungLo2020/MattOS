use crate::stage_graph::BuildStage;
use std::path::PathBuf;

pub(crate) const AUTHORITATIVE_GRUB_CFG: &str = "src/boot/grub/grub.cfg";

pub(crate) fn source_inputs(stage: BuildStage) -> Vec<PathBuf> {
    let roots: &[&str] = match stage {
        BuildStage::Kernel => &[
            "src/kernel/linux",
            "src/kernel/config/x86_64_mattos.config",
            "src/kernel/config/x86_64_mattos.policy.toml",
        ],
        BuildStage::Glibc => &["src/system/libc/glibc"],
        BuildStage::GccRuntime | BuildStage::GccToolchain => &["src/toolchain/gcc"],
        BuildStage::Binutils => &["src/toolchain/binutils"],
        BuildStage::Make => &["src/build-tools/make", "src/build-support/gnulib"],
        BuildStage::Brush => &["src/userland/brush", "upstream/patches/brush"],
        BuildStage::Coreutils => &["src/userland/coreutils"],
        BuildStage::Grep => &["src/userland/grep"],
        BuildStage::Sed => &["src/userland/sed"],
        BuildStage::Findutils => &["src/userland/findutils"],
        BuildStage::Diffutils => &["src/userland/diffutils"],
        BuildStage::Gzip => &[
            "src/userland/gzip",
            "upstream/policies/release-archives.toml",
        ],
        BuildStage::Patch => &[
            "src/userland/patch",
            "upstream/policies/release-archives.toml",
        ],
        BuildStage::File => &["src/userland/file"],
        BuildStage::Less => &[
            "src/userland/less",
            "upstream/policies/release-archives.toml",
        ],
        BuildStage::Git => &["src/userland/git"],
        BuildStage::Openssh => &["src/system/network/openssh-portable"],
        BuildStage::Libffi => &["src/system/libraries/libffi/libffi"],
        BuildStage::Wayland => &["src/system/libraries/wayland"],
        BuildStage::Xkbcommon => &["src/system/libraries/xkbcommon"],
        BuildStage::Libseat => &["src/system/libraries/seatd"],
        BuildStage::LibdisplayInfo => &[
            "src/system/libraries/libdisplay-info",
            "src/system/data/hwdata/pnp.ids",
        ],
        BuildStage::Libevdev => &["src/system/libraries/libevdev"],
        BuildStage::Libinput => &["src/system/libraries/libinput"],
        BuildStage::Pixman => &["src/system/libraries/pixman"],
        BuildStage::Libdrm => &["src/system/libraries/libdrm"],
        BuildStage::VulkanHeaders => &["src/system/graphics/vulkan-headers"],
        BuildStage::VulkanLoader => &["src/system/graphics/vulkan-loader"],
        BuildStage::VulkanTools => &["src/system/graphics/vulkan-tools"],
        BuildStage::X11Compat => &[
            "src/system/graphics/xorgproto",
            "src/system/graphics/xorg-util-macros",
            "src/system/graphics/xtrans",
            "src/system/graphics/libxau",
            "src/system/graphics/libxdmcp",
            "src/system/graphics/xcb-proto",
            "src/system/graphics/libxcb",
            "src/system/graphics/libx11",
            "src/system/graphics/libxext",
        ],
        BuildStage::Libglvnd => &["src/system/graphics/libglvnd"],
        BuildStage::Mesa => &["src/system/graphics/mesa"],
        BuildStage::NvidiaDriver => &[
            "src/system/graphics/nvidia-open-gpu-kernel-modules",
            "src/system/graphics/nvidia-driver",
            "upstream/patches/nvidia-open-gpu-kernel-modules",
        ],
        BuildStage::CosmicComp => &[
            "src/desktop/cosmic/cosmic-comp",
            "upstream/patches/cosmic-comp",
        ],
        BuildStage::Python => &["src/development/python/cpython"],
        BuildStage::Llvm => &["src/toolchain/llvm-project"],
        BuildStage::Rust => &[
            "src/toolchain/rust",
            "upstream/policies/release-archives.toml",
        ],
        BuildStage::Kmod => &["src/system/kmod"],
        BuildStage::Procps => &["src/userland/procps-ng"],
        BuildStage::Ncurses => &["src/system/terminal/ncurses"],
        BuildStage::Iproute2 => &["src/userland/iproute2"],
        BuildStage::Iputils => &["src/userland/iputils"],
        BuildStage::Curl => &["src/userland/curl"],
        BuildStage::Expat => &["src/system/libraries/expat/expat"],
        BuildStage::Libcap => &["src/system/libraries/libcap"],
        BuildStage::Attr => &["src/system/libraries/attr"],
        BuildStage::Tar => &[
            "src/userland/tar",
            "src/build-support/paxutils",
            "src/build-support/gnulib",
        ],
        BuildStage::Acl => &["src/system/libraries/acl"],
        BuildStage::Zlib => &["src/system/libraries/zlib"],
        BuildStage::Bzip2 => &["src/system/libraries/bzip2"],
        BuildStage::Lz4 => &["src/system/libraries/lz4"],
        BuildStage::Xz => &["src/system/libraries/xz"],
        BuildStage::Xxhash => &["src/system/libraries/xxhash"],
        BuildStage::Zstd => &["src/system/libraries/zstd"],
        BuildStage::Openssl => &["src/system/libraries/openssl"],
        BuildStage::Elfutils => &["src/system/libraries/elfutils"],
        BuildStage::Pcre2 => &["src/system/libraries/pcre2", "src/build-support/sljit"],
        BuildStage::Selinux => &["src/system/security/selinux"],
        BuildStage::Libxcrypt => &["src/system/libraries/libxcrypt"],
        BuildStage::Libmd => &["src/system/libraries/libmd"],
        BuildStage::Libbsd => &["src/system/libraries/libbsd"],
        BuildStage::Pam => &["src/system/auth/linux-pam"],
        BuildStage::Shadow => &["src/system/auth/shadow"],
        BuildStage::SudoRs => &["src/system/auth/sudo-rs"],
        BuildStage::UtilLinux => &["src/userland/util-linux", "upstream/patches/util-linux"],
        BuildStage::Systemd => &["src/system/systemd"],
        BuildStage::DbusBroker => &[
            "src/system/dbus/dbus-broker",
            "upstream/patches/dbus-broker",
        ],
        BuildStage::Dpkg => &["src/system/packages/dpkg"],
        BuildStage::Apt => &["src/system/packages/apt", "upstream/patches/apt"],
        BuildStage::Init => &["src/userland/init"],
        BuildStage::Installer => &[
            "src/system/installer",
            "src/boot/module-loader.h",
            "src/system/storage/btrfs-progs",
            "src/system/storage/dosfstools",
            "src/system/storage/e2fsprogs",
            "src/desktop/cosmic/libcosmic",
            "src/desktop/cosmic/iced",
            "src/desktop/cosmic/cosmic-protocols",
            "src/system/libraries/xkbcommon",
            "src/system/data/linux-firmware",
            "upstream/policies/gitlinks.toml",
        ],
        BuildStage::Rootfs | BuildStage::LiveRoot | BuildStage::All => &[],
        BuildStage::Initramfs => &[
            "src/boot/live-init.c",
            "src/boot/module-loader.h",
            "src/system/data/linux-firmware",
        ],
        BuildStage::Iso => &[AUTHORITATIVE_GRUB_CFG],
    };
    let mut inputs = roots.iter().map(PathBuf::from).collect::<Vec<_>>();
    if stage == BuildStage::Glibc {
        inputs.extend(linux_x86_uapi_inputs().into_iter().map(PathBuf::from));
    }
    inputs
}

pub(crate) fn configuration_inputs(stage: BuildStage) -> Vec<PathBuf> {
    let mut inputs = Vec::new();
    if is_rust_stage(stage) {
        inputs.push("Cargo.toml".into());
        inputs.push("Cargo.lock".into());
    }
    if stage == BuildStage::Rootfs {
        inputs.extend(rootfs_configuration_inputs());
        inputs.push("out/packages/inventory.toml".into());
    }
    inputs
}

pub(crate) fn tool_names(stage: BuildStage) -> Vec<String> {
    let tools: &[&str] = match stage {
        BuildStage::LiveRoot => &["mksquashfs", "unsquashfs"],
        BuildStage::Initramfs => &["gcc", "cpio", "xz", "modinfo"],
        BuildStage::Xkbcommon => &["gcc", "ld", "meson", "ninja"],
        BuildStage::Libseat
        | BuildStage::LibdisplayInfo
        | BuildStage::Libevdev
        | BuildStage::Libinput
        | BuildStage::Pixman
        | BuildStage::Libdrm => &["gcc", "ld", "meson", "ninja", "pkg-config"],
        BuildStage::Mesa | BuildStage::X11Compat | BuildStage::Libglvnd => &[
            "gcc",
            "ld",
            "meson",
            "ninja",
            "pkg-config",
            "cmake",
            "git",
            "cargo",
            "rustc",
        ],
        BuildStage::VulkanHeaders | BuildStage::VulkanLoader | BuildStage::VulkanTools => {
            &["gcc", "g++", "ld", "cmake", "ninja", "pkg-config"]
        }
        BuildStage::NvidiaDriver => &["gcc", "ld", "make", "depmod", "zstd", "curl"],
        BuildStage::CosmicComp => &["cargo", "rustc", "gcc", "ld", "pkg-config"],
        BuildStage::Installer => &[
            "cargo",
            "rustc",
            "gcc",
            "ld",
            "autoreconf",
            "make",
            "grub-mkimage",
            "cpio",
            "xz",
            "modinfo",
        ],
        BuildStage::Iso => &["grub-mkrescue", "xorriso"],
        stage if is_rust_stage(stage) => &["cargo", "rustc", "gcc", "ld"],
        _ => &["gcc", "g++", "as", "ld", "make"],
    };
    tools.iter().map(|tool| (*tool).to_string()).collect()
}

pub(crate) fn recipe_revision(stage: BuildStage) -> u32 {
    match stage {
        BuildStage::All => 0,
        BuildStage::Bzip2 | BuildStage::Xz | BuildStage::Zstd => 2,
        BuildStage::Python => 4,
        BuildStage::Llvm => 6,
        BuildStage::LiveRoot => 1,
        BuildStage::Initramfs => 7,
        BuildStage::Installer => 7,
        BuildStage::Xkbcommon => 4,
        BuildStage::Libseat
        | BuildStage::LibdisplayInfo
        | BuildStage::Libevdev
        | BuildStage::Libinput
        | BuildStage::Pixman
        | BuildStage::CosmicComp => 1,
        BuildStage::Libdrm => 2,
        // Revision 4 moves EGL/GLES dispatch to source-built GLVND while Mesa
        // remains a coinstallable vendor implementation.
        BuildStage::Mesa => 4,
        BuildStage::Iso => 2,
        BuildStage::UtilLinux => 5,
        _ => 1,
    }
}

pub(crate) fn is_rust_stage(stage: BuildStage) -> bool {
    matches!(
        stage,
        BuildStage::Brush
            | BuildStage::Coreutils
            | BuildStage::Grep
            | BuildStage::Sed
            | BuildStage::Findutils
            | BuildStage::Diffutils
            | BuildStage::SudoRs
            | BuildStage::Init
            | BuildStage::Installer
    )
}

pub(crate) fn linux_x86_uapi_inputs() -> Vec<&'static str> {
    vec![
        "src/kernel/linux/Makefile",
        "src/kernel/linux/Kbuild",
        "src/kernel/linux/scripts",
        "src/kernel/linux/include/uapi",
        "src/kernel/linux/include/asm-generic",
        "src/kernel/linux/arch/x86/Makefile",
        "src/kernel/linux/arch/x86/include/uapi",
        "src/kernel/linux/arch/x86/entry/syscalls",
    ]
}

pub(crate) fn rootfs_configuration_inputs() -> Vec<PathBuf> {
    [
        "src/rootfs/skeleton",
        "src/system/profiles/live",
        "src/system/units",
        "src/system/network/network",
        "src/system/network/resolved.conf",
        "src/system/network/timesyncd.conf",
        "src/system/network/nsswitch.conf",
        "src/system/network/hosts",
        "src/system/network/networks",
        "src/system/network/99-mattos-network.conf",
        "src/system/session/dbus/session.conf",
        "src/system/session/user-units",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_input_families_remain_narrow() {
        assert!(source_inputs(BuildStage::Brush).iter().all(|path| {
            path.starts_with("src/userland/brush") || path.starts_with("upstream/patches/brush")
        }));
        assert!(
            !source_inputs(BuildStage::Brush)
                .iter()
                .any(|path| path.starts_with("src/kernel"))
        );
        assert_eq!(
            configuration_inputs(BuildStage::Initramfs),
            Vec::<PathBuf>::new()
        );
        assert_eq!(configuration_inputs(BuildStage::Iso), Vec::<PathBuf>::new());
    }

    #[test]
    fn release_archive_consumers_include_the_verified_policy() {
        for stage in [
            BuildStage::Gzip,
            BuildStage::Patch,
            BuildStage::Less,
            BuildStage::Rust,
        ] {
            assert!(
                source_inputs(stage)
                    .contains(&PathBuf::from("upstream/policies/release-archives.toml")),
                "{} must invalidate when its pinned release archive policy changes",
                crate::stage_graph::stage_id(stage)
            );
        }
    }

    #[test]
    fn installer_inputs_keep_only_first_class_cosmic_sources() {
        let inputs = source_inputs(BuildStage::Installer);
        for retained in ["libcosmic", "iced", "cosmic-protocols"] {
            assert!(
                inputs.contains(&PathBuf::from(format!("src/desktop/cosmic/{retained}"))),
                "installer inputs omit first-class COSMIC source {retained}"
            );
        }
        for cargo_dependency in [
            "dbus-settings-bindings",
            "freedesktop-icons",
            "winit",
            "window-clipboard",
            "softbuffer",
            "smithay-clipboard",
            "accesskit",
            "cryoglyph",
            "rust-atomicwrites",
        ] {
            assert!(
                !inputs.contains(&PathBuf::from(format!(
                    "src/desktop/cosmic/{cargo_dependency}"
                ))),
                "normal Cargo dependency {cargo_dependency} was promoted into stage source ownership"
            );
        }
    }
}
