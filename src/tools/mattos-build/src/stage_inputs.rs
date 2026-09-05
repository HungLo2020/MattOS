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
        BuildStage::Libepoxy => &["src/system/graphics/libepoxy"],
        BuildStage::Freetype => &["src/system/libraries/freetype"],
        BuildStage::Libfontenc => &["src/system/graphics/libfontenc"],
        BuildStage::Libxfont => &["src/system/graphics/libxfont"],
        BuildStage::Libxcvt => &["src/system/graphics/libxcvt"],
        BuildStage::Libxshmfence => &["src/system/graphics/libxshmfence"],
        BuildStage::Libxkbfile => &["src/system/graphics/libxkbfile"],
        BuildStage::Xkbcomp => &["src/system/graphics/xkbcomp"],
        BuildStage::Libglvnd => &["src/system/graphics/libglvnd"],
        BuildStage::Mesa => &["src/system/graphics/mesa"],
        BuildStage::Xwayland => &["src/system/graphics/xwayland"],
        BuildStage::NvidiaDriver => &[
            "src/system/graphics/nvidia-open-gpu-kernel-modules",
            "src/system/graphics/nvidia-driver",
            "upstream/patches/nvidia-open-gpu-kernel-modules",
        ],
        BuildStage::CosmicComp => &[
            "src/desktop/cosmic/cosmic-comp",
            "upstream/patches/cosmic-comp",
        ],
        BuildStage::CosmicSession => &["src/desktop/cosmic/cosmic-session"],
        BuildStage::CosmicGreeter => &["src/desktop/cosmic/cosmic-greeter"],
        BuildStage::CosmicPanel => &["src/desktop/cosmic/cosmic-panel"],
        BuildStage::CosmicApplets => &["src/desktop/cosmic/cosmic-applets"],
        BuildStage::CosmicAppLibrary => &["src/desktop/cosmic/cosmic-applibrary"],
        BuildStage::CosmicLauncher => &["src/desktop/cosmic/cosmic-launcher"],
        BuildStage::CosmicSettings => &["src/desktop/cosmic/cosmic-settings"],
        BuildStage::CosmicSettingsDaemon => &["src/desktop/cosmic/cosmic-settings-daemon"],
        BuildStage::CosmicNotifications => &["src/desktop/cosmic/cosmic-notifications"],
        BuildStage::CosmicOsd => &["src/desktop/cosmic/cosmic-osd"],
        BuildStage::CosmicBg => &["src/desktop/cosmic/cosmic-bg"],
        BuildStage::CosmicWorkspaces => &["src/desktop/cosmic/cosmic-workspaces"],
        BuildStage::CosmicFiles => &[
            "src/desktop/cosmic/cosmic-files",
            "src/desktop/cosmic/libcosmic",
            "src/desktop/cosmic/iced",
            "upstream/patches/cosmic-files",
        ],
        BuildStage::CosmicTerm => &["src/desktop/cosmic/cosmic-term"],
        BuildStage::CosmicEdit => &["src/desktop/cosmic/cosmic-edit"],
        BuildStage::CosmicInitialSetup => &[
            "src/desktop/cosmic/cosmic-initial-setup",
            "resources/COSMIC/layouts",
            "resources/COSMIC/themes",
        ],
        BuildStage::Polkit => &["src/system/security/polkit"],
        BuildStage::Duktape => &["src/system/security/duktape"],
        BuildStage::NetworkManager => &["src/system/network/NetworkManager"],
        BuildStage::Cozy => &["src/userland/cozy"],
        BuildStage::CosmicTweaks => &["src/desktop/cosmic/cosmic-tweaks"],
        BuildStage::CosmicUtilities => &[],
        BuildStage::CosmicRandr => & ["src/desktop/cosmic/cosmic-randr"],
        BuildStage::CosmicScreenshot => & ["src/desktop/cosmic/cosmic-screenshot"],
        BuildStage::PopLauncher => & ["src/desktop/cosmic/pop-launcher"],
        BuildStage::CosmicCalculator => & ["src/desktop/cosmic/cosmic-calculator"],
        BuildStage::CosmicStorage => & ["src/desktop/cosmic/cosmic-storage"],
        BuildStage::CosmicMonitor => & ["src/desktop/cosmic/cosmic-monitor"],
        BuildStage::CosmicStore => &["src/desktop/cosmic/cosmic-store"],
        BuildStage::Flatpak => &[
            "src/system/packages/flatpak",
            "src/system/installer/flatpak-target-install.c",
            "src/system/packages/flatpak/resources/mattos-flatpak-system-update.service",
            "src/system/packages/flatpak/resources/mattos-flatpak-system-update.timer",
            "src/system/packages/flatpak/resources/mattos-flatpak-user-update.service",
            "src/system/packages/flatpak/resources/mattos-flatpak-user-update.timer",
        ],
        BuildStage::Bubblewrap => &["src/system/security/bubblewrap"],
        BuildStage::XdgDbusProxy => &["src/system/packages/xdg-dbus-proxy"],
        // GStreamer ships core and plugins-base in one immutable upstream
        // superproject; both stages deliberately fingerprint that one source
        // identity rather than downloading Meson wrap fallbacks.
        BuildStage::Gstreamer | BuildStage::GstreamerBase => {
            &["src/system/multimedia/gstreamer"]
        }
        BuildStage::XdgDesktopPortal => &[
            "src/system/packages/xdg-desktop-portal",
            "src/system/packages/xdg-desktop-portal-gvdb",
            "src/system/packages/xdg-desktop-portal-libglnx",
        ],
        BuildStage::Libarchive => &["src/system/libraries/libarchive"],
        BuildStage::Libxml2 => &["src/system/libraries/libxml2"],
        BuildStage::Libpng => &["src/system/libraries/libpng"],
        BuildStage::Fuse3 => &["src/system/libraries/fuse3"],
        BuildStage::Libfyaml => &["src/system/libraries/libfyaml"],
        BuildStage::Libxmlb => &["src/system/libraries/libxmlb"],
        BuildStage::JsonGlib => &["src/system/libraries/json-glib"],
        BuildStage::Appstream => &["src/system/libraries/appstream"],
        BuildStage::GdkPixbuf => &["src/system/libraries/gdk-pixbuf"],
        BuildStage::Gpgme => &["src/system/security/gpgme"],
        BuildStage::Ostree => &[
            "src/system/packages/ostree",
            "src/system/packages/ostree/libglnx",
            "src/system/packages/ostree/bsdiff",
        ],
        BuildStage::CosmicPortal => &["src/desktop/cosmic/xdg-desktop-portal-cosmic"],
        BuildStage::CosmicAssets => &[
            "src/desktop/cosmic/cosmic-icons",
            "src/desktop/themes/pop-icon-theme",
            "src/desktop/fonts/open-sans",
            "src/desktop/fonts/noto-sans-mono",
            "src/desktop/fonts/pop-fonts",
            "resources/COSMIC/defaults",
        ],
        BuildStage::Greetd => &["src/system/session/greetd"],
        // The aggregate copies component outputs according to this narrowly
        // owned policy.  Do not fingerprint the global build dispatcher or
        // package subsystem: unrelated importer, reporting, or repository
        // changes cannot affect the aggregate's published install tree.
        BuildStage::CosmicDesktop => &[
            "src/tools/mattos-build/src/stages/desktop_aggregation.rs",
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
        BuildStage::Dav1d => &["src/system/multimedia/dav1d"],
        BuildStage::Glib => &["src/system/libraries/glib"],
        BuildStage::Pipewire => &["src/system/multimedia/pipewire"],
        BuildStage::Openssl => &["src/system/libraries/openssl"],
        BuildStage::Elfutils => &["src/system/libraries/elfutils"],
        BuildStage::Pcre2 => &["src/system/libraries/pcre2", "src/build-support/sljit"],
        BuildStage::Selinux => &["src/system/security/selinux"],
        BuildStage::Libxcrypt => &["src/system/libraries/libxcrypt"],
        BuildStage::Libmd => &["src/system/libraries/libmd"],
        BuildStage::Libbsd => &["src/system/libraries/libbsd"],
        BuildStage::Libndp => &["src/system/network/libndp"],
        BuildStage::Readline => &["src/system/userland/readline"],
        BuildStage::Pam => &["src/system/auth/linux-pam"],
        BuildStage::Shadow => &["src/system/auth/shadow"],
        BuildStage::SudoRs => &["src/system/auth/sudo-rs"],
        BuildStage::UtilLinux => &["src/userland/util-linux", "upstream/patches/util-linux"],
        BuildStage::Systemd => &["src/system/systemd"],
        BuildStage::Dbus => &["src/system/dbus/dbus"],
        BuildStage::DbusBroker => &[
            "src/system/dbus/dbus-broker",
            "upstream/patches/dbus-broker",
        ],
        BuildStage::Dpkg => &["src/system/packages/dpkg"],
        BuildStage::LibgpgError => &["src/system/security/libgpg-error"],
        BuildStage::Libgcrypt => &["src/system/security/libgcrypt"],
        BuildStage::Libassuan => &["src/system/security/libassuan"],
        BuildStage::Libksba => &["src/system/security/libksba"],
        BuildStage::Npth => &["src/system/security/npth"],
        BuildStage::Gpgv => &["src/system/security/gnupg"],
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
        // Rootfs/live-image assembly is implemented independently from the
        // dispatcher.  Its own policy is a semantic input to image stages;
        // unrelated command/import/package edits are not.
        BuildStage::Rootfs | BuildStage::LiveRoot => {
            &["src/tools/mattos-build/src/stages/image.rs"]
        }
        BuildStage::All => &[],
        BuildStage::Initramfs => &[
            "src/boot/live-init.c",
            "src/boot/module-loader.h",
            "src/system/data/linux-firmware",
            "src/tools/mattos-build/src/stages/image.rs",
        ],
        BuildStage::Iso => &[
            AUTHORITATIVE_GRUB_CFG,
            "src/tools/mattos-build/src/stages/image.rs",
        ],
    };
    let mut inputs = roots.iter().map(PathBuf::from).collect::<Vec<_>>();
    // These consumers declare one or more packages from libcosmic's Git
    // repository.  The build recipe replaces those declarations only in its
    // output-owned mirror, so the pinned libcosmic and iced trees are real
    // source inputs even when the consumer names `cosmic-config` rather than
    // the top-level `libcosmic` package.  Keep this list narrow: desktop
    // siblings which do not declare that Git source must not fan out when it
    // changes.
    if cosmic_stage_uses_owned_libcosmic(stage) {
        for input in [
            PathBuf::from("src/desktop/cosmic/libcosmic"),
            PathBuf::from("src/desktop/cosmic/iced"),
        ] {
            if !inputs.contains(&input) {
                inputs.push(input);
            }
        }
    }
    if matches!(
        stage,
        BuildStage::Brush
            | BuildStage::Coreutils
            | BuildStage::Grep
            | BuildStage::Sed
            | BuildStage::Findutils
            | BuildStage::Diffutils
            | BuildStage::Init
            | BuildStage::Pam
            | BuildStage::Shadow
            | BuildStage::SudoRs
            | BuildStage::UtilLinux
            | BuildStage::Kmod
            | BuildStage::Ncurses
            | BuildStage::Procps
    ) {
        inputs.push("src/tools/mattos-build/src/stages/base_userland.rs".into());
    }
    if matches!(
        stage,
        BuildStage::Kernel
            | BuildStage::Glibc
            | BuildStage::GccRuntime
            | BuildStage::GccToolchain
            | BuildStage::Binutils
            | BuildStage::Make
    ) {
        inputs.push("src/tools/mattos-build/src/stages/toolchain.rs".into());
    }
    if matches!(
        stage,
        BuildStage::Bzip2
            | BuildStage::Lz4
            | BuildStage::Xz
            | BuildStage::Xxhash
            | BuildStage::Zstd
            | BuildStage::Openssl
            | BuildStage::Elfutils
            | BuildStage::Pcre2
            | BuildStage::Selinux
            | BuildStage::Libxcrypt
            | BuildStage::Libmd
            | BuildStage::Libbsd
            | BuildStage::Libndp
            | BuildStage::Readline
            | BuildStage::LibgpgError
            | BuildStage::Libgcrypt
            | BuildStage::Libassuan
            | BuildStage::Libksba
            | BuildStage::Npth
            | BuildStage::Gpgv
    ) {
        inputs.push("src/tools/mattos-build/src/stages/libraries.rs".into());
    }
    if matches!(
        stage,
        BuildStage::CosmicComp
            | BuildStage::CosmicSession
            | BuildStage::CosmicGreeter
            | BuildStage::CosmicPanel
            | BuildStage::CosmicApplets
            | BuildStage::CosmicAppLibrary
            | BuildStage::CosmicLauncher
            | BuildStage::CosmicSettings
            | BuildStage::CosmicSettingsDaemon
            | BuildStage::CosmicNotifications
            | BuildStage::CosmicOsd
            | BuildStage::CosmicBg
            | BuildStage::CosmicWorkspaces
            | BuildStage::CosmicFiles
            | BuildStage::CosmicTerm
            | BuildStage::CosmicTweaks
            | BuildStage::CosmicUtilities
            | BuildStage::CosmicRandr
            | BuildStage::CosmicScreenshot
            | BuildStage::PopLauncher
            | BuildStage::CosmicCalculator
            | BuildStage::CosmicStorage
            | BuildStage::CosmicMonitor
            | BuildStage::CosmicStore
            | BuildStage::CosmicPortal
            | BuildStage::CosmicAssets
            | BuildStage::Greetd
            | BuildStage::CosmicEdit
            | BuildStage::CosmicInitialSetup
    ) {
        inputs.push("src/tools/mattos-build/src/stages/desktop.rs".into());
    }
    if matches!(
        stage,
        BuildStage::Libseat
            | BuildStage::X11Compat
            | BuildStage::Libepoxy
            | BuildStage::Freetype
            | BuildStage::Libfontenc
            | BuildStage::Libxfont
            | BuildStage::Libxcvt
            | BuildStage::Libxshmfence
            | BuildStage::Libxkbfile
            | BuildStage::Xkbcomp
            | BuildStage::Xwayland
            | BuildStage::Bubblewrap
            | BuildStage::XdgDbusProxy
            | BuildStage::Gstreamer
            | BuildStage::GstreamerBase
            | BuildStage::XdgDesktopPortal
            | BuildStage::Libglvnd
            | BuildStage::NvidiaDriver
            | BuildStage::LibdisplayInfo
            | BuildStage::Libevdev
            | BuildStage::Libinput
            | BuildStage::Pixman
            | BuildStage::Libdrm
            | BuildStage::VulkanHeaders
            | BuildStage::VulkanLoader
            | BuildStage::VulkanTools
            | BuildStage::Mesa
    ) {
        inputs.push("src/tools/mattos-build/src/stages/graphics.rs".into());
    }
    if stage == BuildStage::Glibc {
        inputs.extend(linux_x86_uapi_inputs().into_iter().map(PathBuf::from));
    }
    inputs
}

fn cosmic_stage_uses_owned_libcosmic(stage: BuildStage) -> bool {
    matches!(
        stage,
        BuildStage::CosmicComp
            | BuildStage::CosmicGreeter
            | BuildStage::CosmicPanel
            | BuildStage::CosmicApplets
            | BuildStage::CosmicAppLibrary
            | BuildStage::CosmicLauncher
            | BuildStage::CosmicSettings
            | BuildStage::CosmicSettingsDaemon
            | BuildStage::CosmicNotifications
            | BuildStage::CosmicOsd
            | BuildStage::CosmicBg
            | BuildStage::CosmicWorkspaces
            | BuildStage::CosmicFiles
            | BuildStage::CosmicTerm
            | BuildStage::CosmicEdit
            | BuildStage::CosmicInitialSetup
            | BuildStage::CosmicTweaks
            | BuildStage::CosmicCalculator
            | BuildStage::CosmicStorage
            | BuildStage::CosmicMonitor
            | BuildStage::CosmicStore
            | BuildStage::CosmicPortal
    )
}

pub(crate) fn configuration_inputs(stage: BuildStage) -> Vec<PathBuf> {
    let mut inputs = Vec::new();
    if is_rust_stage(stage) {
        inputs.extend(local_cargo_manifest_inputs(stage));
    }
    inputs.extend(ownership_contract_inputs(stage));
    if stage == BuildStage::Rootfs {
        inputs.extend(rootfs_configuration_inputs());
        inputs.push("out/packages/inventory.toml".into());
    }
    match stage {
        BuildStage::CosmicEdit => {
            inputs.push("src/desktop/cosmic/cosmic-edit/Cargo.toml".into());
            inputs.push("src/desktop/cosmic/cosmic-edit/Cargo.lock".into());
        }
        BuildStage::CosmicInitialSetup => {
            inputs.push("src/desktop/cosmic/cosmic-initial-setup/Cargo.toml".into());
            inputs.push("src/desktop/cosmic/cosmic-initial-setup/Cargo.lock".into());
        }
        BuildStage::Cozy => {
            inputs.push("src/userland/cozy/Cargo.toml".into());
            inputs.push("src/userland/cozy/Cargo.lock".into());
        }
        _ => {}
    }
    inputs
}

fn local_cargo_manifest_inputs(stage: BuildStage) -> Vec<PathBuf> {
    let root = match stage {
        BuildStage::Brush => "src/userland/brush",
        BuildStage::Coreutils => "src/userland/coreutils",
        BuildStage::Grep => "src/userland/grep",
        BuildStage::Sed => "src/userland/sed",
        BuildStage::Findutils => "src/userland/findutils",
        BuildStage::Diffutils => "src/userland/diffutils",
        BuildStage::SudoRs => "src/system/auth/sudo-rs",
        BuildStage::Init => return vec!["src/userland/init/Cargo.toml".into()],
        BuildStage::Installer => {
            return vec![
                "src/system/installer/Cargo.toml".into(),
                "src/system/installer/gui/cosmic/Cargo.lock".into(),
            ];
        }
        _ => return Vec::new(),
    };
    vec![
        format!("{root}/Cargo.toml").into(),
        format!("{root}/Cargo.lock").into(),
    ]
}

pub(crate) fn ownership_contract_inputs(stage: BuildStage) -> Vec<PathBuf> {
    let components: &[&str] = match stage {
        BuildStage::Brush => &["brush"],
        BuildStage::Coreutils => &["coreutils"],
        BuildStage::Grep => &["grep"],
        BuildStage::Sed => &["sed"],
        BuildStage::Findutils => &["findutils"],
        BuildStage::Diffutils => &["diffutils"],
        BuildStage::SudoRs => &["sudo-rs"],
        BuildStage::Installer => &["btrfs-progs", "dosfstools", "e2fsprogs"],
        BuildStage::CosmicComp => &["cosmic-comp"],
        BuildStage::CosmicSession => &["cosmic-session"],
        BuildStage::CosmicGreeter => &["cosmic-greeter"],
        BuildStage::CosmicPanel => &["cosmic-panel"],
        BuildStage::CosmicApplets => &["cosmic-applets"],
        BuildStage::CosmicAppLibrary => &["cosmic-applibrary"],
        BuildStage::CosmicLauncher => &["cosmic-launcher"],
        BuildStage::CosmicSettings => &["cosmic-settings"],
        BuildStage::CosmicSettingsDaemon => &["cosmic-settings-daemon"],
        BuildStage::CosmicNotifications => &["cosmic-notifications"],
        BuildStage::CosmicOsd => &["cosmic-osd"],
        BuildStage::CosmicBg => &["cosmic-bg"],
        BuildStage::CosmicWorkspaces => &["cosmic-workspaces"],
        BuildStage::CosmicFiles => &["cosmic-files"],
        BuildStage::CosmicEdit => &["cosmic-edit"],
        BuildStage::CosmicInitialSetup => &["cosmic-initial-setup"],
        BuildStage::CosmicTerm => &["cosmic-term"],
        BuildStage::CosmicTweaks => &["cosmic-tweaks"],
        BuildStage::CosmicUtilities => &[],
        BuildStage::CosmicRandr => &["cosmic-randr"],
        BuildStage::CosmicScreenshot => &["cosmic-screenshot"],
        BuildStage::PopLauncher => &["pop-launcher"],
        BuildStage::CosmicCalculator => &["cosmic-calculator"],
        BuildStage::CosmicStorage => &["cosmic-storage"],
        BuildStage::CosmicMonitor => &["cosmic-monitor"],
        BuildStage::CosmicStore => &["cosmic-store"],
        BuildStage::CosmicPortal => &["xdg-desktop-portal-cosmic"],
        BuildStage::Cozy => &["cozy"],
        _ => &[],
    };
    components
        .iter()
        .map(|component| {
            PathBuf::from(format!(
                "out/source-ownership/cargo/contracts/{component}.json"
            ))
        })
        .collect()
}

pub(crate) fn tool_names(stage: BuildStage) -> Vec<String> {
    let tools: &[&str] = match stage {
        BuildStage::LiveRoot => &["mksquashfs", "unsquashfs"],
        BuildStage::Duktape => &["gcc", "python3"],
        BuildStage::Initramfs => &["gcc", "cpio", "xz", "modinfo"],
        BuildStage::Xkbcommon => &["gcc", "ld", "meson", "ninja"],
        BuildStage::Dav1d
        | BuildStage::Glib
        | BuildStage::Pipewire
        | BuildStage::Dbus
        | BuildStage::Libseat
        | BuildStage::LibdisplayInfo
        | BuildStage::Libevdev
        | BuildStage::Libinput
        | BuildStage::Pixman
        | BuildStage::Libdrm
        | BuildStage::Libepoxy
        | BuildStage::Freetype
        | BuildStage::Libxcvt
        | BuildStage::Libxkbfile
        | BuildStage::Xwayland
        | BuildStage::Bubblewrap
        | BuildStage::XdgDbusProxy
        | BuildStage::Gstreamer
        | BuildStage::GstreamerBase
        | BuildStage::XdgDesktopPortal => &["gcc", "ld", "meson", "ninja", "pkg-config"],
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
        BuildStage::LibgpgError
        | BuildStage::Libgcrypt
        | BuildStage::Libassuan
        | BuildStage::Libksba
        | BuildStage::Npth
        | BuildStage::Gpgv
        | BuildStage::Libfontenc
        | BuildStage::Libxfont
        | BuildStage::Libxshmfence => &["autoreconf", "gcc", "ld", "make", "pkg-config"],
        BuildStage::CosmicComp
        | BuildStage::CosmicSession
        | BuildStage::CosmicGreeter
        | BuildStage::CosmicPanel
        | BuildStage::CosmicApplets
        | BuildStage::CosmicAppLibrary
        | BuildStage::CosmicLauncher
        | BuildStage::CosmicSettings
        | BuildStage::CosmicSettingsDaemon
        | BuildStage::CosmicNotifications
        | BuildStage::CosmicOsd
        | BuildStage::CosmicBg
        | BuildStage::CosmicWorkspaces
        | BuildStage::CosmicFiles
        | BuildStage::CosmicTerm
        | BuildStage::CosmicEdit
        | BuildStage::CosmicInitialSetup
        | BuildStage::CosmicTweaks
        | BuildStage::CosmicUtilities
        | BuildStage::CosmicRandr
        | BuildStage::CosmicScreenshot
        | BuildStage::PopLauncher
        | BuildStage::CosmicCalculator
        | BuildStage::CosmicStorage
        | BuildStage::CosmicMonitor
        | BuildStage::CosmicStore
        | BuildStage::Flatpak
        | BuildStage::CosmicPortal
        | BuildStage::Greetd => &["cargo", "rustc", "gcc", "ld", "pkg-config"],
        BuildStage::CosmicAssets | BuildStage::CosmicDesktop => &[],
        BuildStage::Cozy => &["cargo", "rustc", "gcc", "ld"],
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
        // Revision 2 disables host libseccomp discovery for the target APT
        // build; only target-owned native interfaces may be selected.
        BuildStage::Apt => 2,
        BuildStage::Python => 4,
        BuildStage::Llvm => 6,
        // Revision 2 selects the measured canonical Zstd level-12 SquashFS
        // configuration and its corresponding published report.
        BuildStage::LiveRoot => 2,
        // Revision 5 establishes the pre-created live user's Flatpak data
        // hierarchy through tmpfiles, including correct UID/GID ownership.
        // Revision 4 preserves fuse3's setuid fusermount3 contract after
        // package extraction so xdg-document-portal can mount per-user
        // document filesystems. Revision 3 generated an individual
        // en_US.utf8 locale beside the package-provided C/POSIX archive.
        BuildStage::Rootfs => 5,
        BuildStage::Initramfs => 7,
        BuildStage::Installer => 7,
        BuildStage::Xkbcommon => 4,
        // Revision 2 publishes the complete target-owned X.Org development
        // contract in the aggregate so later Xwayland stages can resolve
        // X11/X.h, xtrans and pkg-config data without host leakage.
        BuildStage::X11Compat => 2,
        BuildStage::CosmicWorkspaces => 2,
        BuildStage::CosmicDesktop => 2,
        // Revision 3 preserves generated Duktape byte tables as Latin-1
        // bytes under the Python-3 generator adaptation.  This invalidates
        // previously published tables that were UTF-8 expanded and corrupt.
        BuildStage::Duktape => 3,
        BuildStage::Polkit => 2,
        BuildStage::LibdisplayInfo
        | BuildStage::Libevdev
        | BuildStage::Libinput
        | BuildStage::Pixman
        | BuildStage::CosmicComp => 1,
        BuildStage::CosmicFiles => 2,
        // Revision 2 excludes Curl's build-private libtool archive from the
        // published install so target consumers cannot inherit a host path.
        BuildStage::Curl => 2,
        // Revision 3 uses target-owned bwrap and xdg-dbus-proxy and disables
        // Meson wrap/network fallback for the normal build.
        // Revision 7 selects MattOS's `sudo` administrative group for
        // Flatpak system-helper authorization instead of upstream `wheel`.
        // Revision 6 seeds Flatpak's minimal system OSTree remote metadata
        // from MattOS's signed descriptor, so a fresh system exposes Flathub
        // without a first-run configuration command. Revision 5 moves
        // dependency pkg-config relocation into a private
        // consumer overlay. Flatpak must never rewrite a cached producer's
        // published install tree while it prepares its native environment.
        // Revision 4 packages MattOS's signed Flathub descriptor as Flatpak
        // policy, so only Flatpak/package/image composition is invalidated
        // when that policy changes.
        // Revision 7 covers the Flathub descriptor and system/user update
        // timers; Firefox is maintained outside the core build DAG.
        BuildStage::Flatpak => 7,
        // Revision 4 enables the target-owned libcurl fetcher as well as
        // GPGME: Flatpak needs OSTree to verify and download HTTPS remote
        // metadata and commits without host libraries.
        BuildStage::Ostree => 4,
        // Revision 2 removes gpgme's build-private libtool archive so target
        // consumers cannot record the host staging path as an ELF RUNPATH.
        BuildStage::Gpgme => 2,
        // Revision 2 prevents GStreamer's gio plugin from embedding GLib's
        // output-staging paths as installed runtime search directories.
        BuildStage::GstreamerBase => 2,
        // Revision 2 makes portal icon/sound validators embed the packaged
        // Bubblewrap runtime path rather than the staged build path.
        BuildStage::XdgDesktopPortal => 2,
        // Revision 2 places COSMIC Edit's hicolor-sized application icons at
        // /usr/share/icons/hicolor rather than nesting a second hicolor tree.
        BuildStage::CosmicEdit => 3,
        BuildStage::CosmicInitialSetup => 1,
        BuildStage::Cozy => 1,
        BuildStage::Libseat => 2,
        BuildStage::Dbus => 3,
        // Revision 7 enables and publishes systemd-nspawn for mattos-compat;
        // the prior revision deliberately configured nspawn out.
        BuildStage::Systemd => 7,
        BuildStage::Pipewire => 2,
        BuildStage::Glib => 2,
        BuildStage::Libdrm => 2,
        // Revision 2 enables target-owned libGL/GLX dispatch for Xwayland;
        // the earlier EGL-only output cannot satisfy gl.pc consumers.
        BuildStage::Libglvnd => 2,
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
            path.starts_with("src/userland/brush")
                || path.starts_with("upstream/patches/brush")
                || path == "src/tools/mattos-build/src/stages/base_userland.rs"
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
    fn cosmic_component_inputs_are_leaf_precise() {
        assert_eq!(
            source_inputs(BuildStage::CosmicPanel),
            vec![
                PathBuf::from("src/desktop/cosmic/cosmic-panel"),
                PathBuf::from("src/desktop/cosmic/libcosmic"),
                PathBuf::from("src/desktop/cosmic/iced"),
                PathBuf::from("src/tools/mattos-build/src/stages/desktop.rs"),
            ]
        );
        assert_eq!(
            source_inputs(BuildStage::CosmicSession),
            vec![
                PathBuf::from("src/desktop/cosmic/cosmic-session"),
                PathBuf::from("src/tools/mattos-build/src/stages/desktop.rs"),
            ]
        );
        assert_eq!(
            source_inputs(BuildStage::CosmicInitialSetup),
            vec![
                PathBuf::from("src/desktop/cosmic/cosmic-initial-setup"),
                PathBuf::from("resources/COSMIC/layouts"),
                PathBuf::from("resources/COSMIC/themes"),
                PathBuf::from("src/desktop/cosmic/libcosmic"),
                PathBuf::from("src/desktop/cosmic/iced"),
                PathBuf::from("src/tools/mattos-build/src/stages/desktop.rs"),
            ]
        );
        assert_eq!(
            source_inputs(BuildStage::CosmicFiles),
            vec![
                PathBuf::from("src/desktop/cosmic/cosmic-files"),
                PathBuf::from("src/desktop/cosmic/libcosmic"),
                PathBuf::from("src/desktop/cosmic/iced"),
                PathBuf::from("upstream/patches/cosmic-files"),
                PathBuf::from("src/tools/mattos-build/src/stages/desktop.rs"),
            ]
        );
        assert_eq!(
            source_inputs(BuildStage::CosmicStore),
            vec![
                PathBuf::from("src/desktop/cosmic/cosmic-store"),
                PathBuf::from("src/desktop/cosmic/libcosmic"),
                PathBuf::from("src/desktop/cosmic/iced"),
                PathBuf::from("src/tools/mattos-build/src/stages/desktop.rs"),
            ]
        );
        assert_eq!(
            source_inputs(BuildStage::XdgDbusProxy),
            vec![
                PathBuf::from("src/system/packages/xdg-dbus-proxy"),
                PathBuf::from("src/tools/mattos-build/src/stages/graphics.rs"),
            ]
        );
        assert!(
            source_inputs(BuildStage::CosmicAssets)
                .contains(&PathBuf::from("resources/COSMIC/defaults"))
        );
        assert_eq!(
            source_inputs(BuildStage::CosmicDesktop),
            vec![PathBuf::from(
                "src/tools/mattos-build/src/stages/desktop_aggregation.rs"
            )]
        );
        for stage in [
            BuildStage::CosmicSession,
            BuildStage::CosmicGreeter,
            BuildStage::CosmicPanel,
            BuildStage::CosmicApplets,
            BuildStage::CosmicLauncher,
            BuildStage::CosmicSettings,
            BuildStage::CosmicTweaks,
        ] {
            assert!(
                !source_inputs(stage)
                    .iter()
                    .any(|path| path.starts_with("src/system/session/cosmic")),
                "package-only session metadata leaked into native stage {}",
                crate::stage_graph::stage_id(stage)
            );
        }
    }

    #[test]
    fn cosmic_desktop_identity_is_scoped_to_its_aggregation_policy() {
        let root = tempfile::tempdir().expect("temporary repository");
        let policy = root
            .path()
            .join("src/tools/mattos-build/src/stages/desktop_aggregation.rs");
        std::fs::create_dir_all(policy.parent().expect("policy parent"))
            .expect("create policy parent");
        std::fs::write(&policy, "aggregate = [\"cosmic-panel\"]\n")
            .expect("write aggregation policy");

        let unrelated = [
            "src/tools/mattos-build/src/main.rs",
            "src/tools/mattos-build/src/packaging.rs",
            "src/tools/mattos-build/src/commands/doctor.rs",
            "src/tools/mattos-build/src/commands/report.rs",
            "src/tools/mattos-build/src/source/import.rs",
        ];
        for path in unrelated {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().expect("unrelated parent"))
                .expect("create unrelated parent");
            std::fs::write(path, "unrelated = true\n").expect("write unrelated input");
        }

        let roots = source_inputs(BuildStage::CosmicDesktop);
        let baseline = crate::performance::tracked_source_digest(root.path(), &roots, false)
            .expect("baseline cosmic desktop identity");

        for path in unrelated {
            std::fs::write(root.path().join(path), "unrelated = false\n")
                .expect("change unrelated input");
            assert_eq!(
                baseline,
                crate::performance::tracked_source_digest(root.path(), &roots, false)
                    .expect("scoped cosmic desktop identity"),
                "{path} must not invalidate CosmicDesktop"
            );
        }

        std::fs::write(&policy, "aggregate = [\"cosmic-files\"]\n")
            .expect("change aggregation policy");
        assert_ne!(
            baseline,
            crate::performance::tracked_source_digest(root.path(), &roots, false)
                .expect("changed cosmic desktop identity"),
            "aggregation policy must invalidate CosmicDesktop"
        );
    }

    #[test]
    fn image_stages_own_their_implementation_input() {
        let image_module = PathBuf::from("src/tools/mattos-build/src/stages/image.rs");
        for stage in [
            BuildStage::Rootfs,
            BuildStage::LiveRoot,
            BuildStage::Initramfs,
            BuildStage::Iso,
        ] {
            assert!(
                source_inputs(stage).contains(&image_module),
                "{} must track its image implementation",
                crate::stage_graph::stage_id(stage)
            );
        }
    }

    #[test]
    fn shared_native_helpers_do_not_become_blanket_stage_inputs() {
        let helper_roots = [
            "src/tools/mattos-build/src/stages/helpers/native.rs",
            "src/tools/mattos-build/src/stages/helpers/pkgconfig.rs",
            "src/tools/mattos-build/src/stages/helpers/autotools.rs",
            "src/tools/mattos-build/src/stages/helpers/meson.rs",
            "src/tools/mattos-build/src/stages/helpers/cargo.rs",
        ];
        for stage in [
            BuildStage::Kernel,
            BuildStage::Glibc,
            BuildStage::CosmicFiles,
            BuildStage::Flatpak,
            BuildStage::NetworkManager,
        ] {
            let roots = source_inputs(stage);
            assert!(
                helper_roots.iter().all(|helper| !roots.contains(&PathBuf::from(helper))),
                "shared helper implementation must not become a blanket source input for {}",
                crate::stage_graph::stage_id(stage)
            );
        }
    }

    #[test]
    fn registry_implementation_is_not_a_global_stage_source_input() {
        let registry = PathBuf::from("src/tools/mattos-build/src/stages/registry.rs");
        for stage in crate::stage_graph::build_plan(BuildStage::All) {
            assert!(
                !source_inputs(stage).contains(&registry),
                "one registry metadata edit must not globally invalidate {}",
                crate::stage_graph::stage_id(stage)
            );
        }
    }

    #[test]
    fn base_userland_recipe_implementation_is_owned_only_by_its_stages() {
        let implementation = PathBuf::from("src/tools/mattos-build/src/stages/base_userland.rs");
        for stage in [
            BuildStage::Brush,
            BuildStage::Coreutils,
            BuildStage::Grep,
            BuildStage::Sed,
            BuildStage::Findutils,
            BuildStage::Diffutils,
            BuildStage::Init,
            BuildStage::Pam,
            BuildStage::Shadow,
            BuildStage::SudoRs,
            BuildStage::UtilLinux,
            BuildStage::Kmod,
            BuildStage::Ncurses,
            BuildStage::Procps,
        ] {
            assert!(source_inputs(stage).contains(&implementation));
        }
        for stage in [BuildStage::Kernel, BuildStage::Glibc, BuildStage::Mesa, BuildStage::CosmicFiles, BuildStage::Flatpak] {
            assert!(!source_inputs(stage).contains(&implementation));
        }
    }

    #[test]
    fn toolchain_recipe_implementation_is_owned_only_by_foundational_toolchain_stages() {
        let implementation = PathBuf::from("src/tools/mattos-build/src/stages/toolchain.rs");
        for stage in [
            BuildStage::Kernel,
            BuildStage::Glibc,
            BuildStage::GccRuntime,
            BuildStage::GccToolchain,
            BuildStage::Binutils,
            BuildStage::Make,
        ] {
            assert!(source_inputs(stage).contains(&implementation));
        }
        for stage in [
            BuildStage::Mesa,
            BuildStage::CosmicFiles,
            BuildStage::Flatpak,
            BuildStage::NetworkManager,
        ] {
            assert!(!source_inputs(stage).contains(&implementation));
        }
    }

    #[test]
    fn libraries_recipe_implementation_is_owned_only_by_its_library_stages() {
        let implementation = PathBuf::from("src/tools/mattos-build/src/stages/libraries.rs");
        for stage in [
            BuildStage::Bzip2,
            BuildStage::Lz4,
            BuildStage::Xz,
            BuildStage::Xxhash,
            BuildStage::Zstd,
            BuildStage::Openssl,
            BuildStage::Elfutils,
            BuildStage::Pcre2,
            BuildStage::Selinux,
            BuildStage::Libxcrypt,
            BuildStage::Libmd,
            BuildStage::Libbsd,
            BuildStage::Libndp,
            BuildStage::Readline,
            BuildStage::LibgpgError,
            BuildStage::Libgcrypt,
            BuildStage::Libassuan,
            BuildStage::Libksba,
            BuildStage::Npth,
            BuildStage::Gpgv,
        ] {
            assert!(source_inputs(stage).contains(&implementation));
        }
        for stage in [
            BuildStage::Kernel,
            BuildStage::Glibc,
            BuildStage::Mesa,
            BuildStage::CosmicFiles,
            BuildStage::Flatpak,
            BuildStage::NetworkManager,
        ] {
            assert!(!source_inputs(stage).contains(&implementation));
        }
    }

    #[test]
    fn libraries_recipe_identity_changes_only_for_its_owners() {
        let root = tempfile::tempdir().expect("temporary repository");
        for path in [
            "src/system/libraries/lz4/README",
            "src/tools/mattos-build/src/stages/libraries.rs",
        ] {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().expect("input parent"))
                .expect("create input parent");
            std::fs::write(path, "original\n").expect("write input");
        }

        let roots = source_inputs(BuildStage::Lz4);
        let baseline = crate::performance::tracked_source_digest(root.path(), &roots, false)
            .expect("baseline libraries identity");
        for path in [
            "src/tools/mattos-build/src/main.rs",
            "src/tools/mattos-build/src/stages/base_userland.rs",
            "src/tools/mattos-build/src/stages/desktop.rs",
            "src/tools/mattos-build/src/stages/image.rs",
        ] {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().expect("unrelated parent"))
                .expect("create unrelated parent");
            std::fs::write(&path, "unrelated\n").expect("write unrelated input");
            assert_eq!(
                baseline,
                crate::performance::tracked_source_digest(root.path(), &roots, false)
                    .expect("narrow libraries identity"),
                "{path:?} must not invalidate LZ4"
            );
        }

        let implementation = root
            .path()
            .join("src/tools/mattos-build/src/stages/libraries.rs");
        std::fs::write(&implementation, "changed recipe\n").expect("change recipe");
        assert_ne!(
            baseline,
            crate::performance::tracked_source_digest(root.path(), &roots, false)
                .expect("changed libraries identity"),
            "an owned recipe change must invalidate LZ4"
        );
    }

    #[test]
    fn base_userland_recipe_identity_changes_only_for_its_owners() {
        let root = tempfile::tempdir().expect("temporary repository");
        for path in [
            "src/userland/brush/README",
            "upstream/patches/brush/0001.patch",
            "src/tools/mattos-build/src/stages/base_userland.rs",
        ] {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().expect("input parent"))
                .expect("create input parent");
            std::fs::write(path, "original\n").expect("write input");
        }

        let roots = source_inputs(BuildStage::Brush);
        let baseline = crate::performance::tracked_source_digest(root.path(), &roots, false)
            .expect("baseline base-userland identity");

        for path in [
            "src/tools/mattos-build/src/main.rs",
            "src/tools/mattos-build/src/commands/cache.rs",
            "src/tools/mattos-build/src/stages/image.rs",
            "src/tools/mattos-build/src/stages/desktop_aggregation.rs",
        ] {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().expect("unrelated parent"))
                .expect("create unrelated parent");
            std::fs::write(&path, "unrelated\n").expect("write unrelated input");
            assert_eq!(
                baseline,
                crate::performance::tracked_source_digest(root.path(), &roots, false)
                    .expect("narrow base-userland identity"),
                "{path:?} must not invalidate Brush"
            );
        }

        let implementation = root
            .path()
            .join("src/tools/mattos-build/src/stages/base_userland.rs");
        std::fs::write(&implementation, "changed recipe\n").expect("change recipe");
        assert_ne!(
            baseline,
            crate::performance::tracked_source_digest(root.path(), &roots, false)
                .expect("changed base-userland identity"),
            "an owned recipe change must invalidate Brush"
        );
    }

    #[test]
    fn graphics_recipe_implementation_is_owned_only_by_graphics_platform_stages() {
        let implementation = PathBuf::from("src/tools/mattos-build/src/stages/graphics.rs");
        for stage in [
            BuildStage::Mesa,
            BuildStage::Xwayland,
            BuildStage::VulkanLoader,
            BuildStage::Libdrm,
            BuildStage::Libinput,
            BuildStage::NvidiaDriver,
            BuildStage::XdgDesktopPortal,
        ] {
            assert!(source_inputs(stage).contains(&implementation));
        }
        for stage in [
            BuildStage::Kernel,
            BuildStage::Glibc,
            BuildStage::CosmicFiles,
            BuildStage::Flatpak,
            BuildStage::NetworkManager,
        ] {
            assert!(!source_inputs(stage).contains(&implementation));
        }
    }

    #[test]
    fn desktop_recipe_implementation_is_owned_only_by_desktop_stages() {
        let implementation = PathBuf::from("src/tools/mattos-build/src/stages/desktop.rs");
        for stage in [
            BuildStage::CosmicComp,
            BuildStage::CosmicSession,
            BuildStage::CosmicGreeter,
            BuildStage::CosmicPanel,
            BuildStage::CosmicApplets,
            BuildStage::CosmicAppLibrary,
            BuildStage::CosmicLauncher,
            BuildStage::CosmicSettings,
            BuildStage::CosmicSettingsDaemon,
            BuildStage::CosmicNotifications,
            BuildStage::CosmicOsd,
            BuildStage::CosmicBg,
            BuildStage::CosmicWorkspaces,
            BuildStage::CosmicFiles,
            BuildStage::CosmicTerm,
            BuildStage::CosmicTweaks,
            BuildStage::CosmicUtilities,
            BuildStage::CosmicRandr,
            BuildStage::CosmicScreenshot,
            BuildStage::PopLauncher,
            BuildStage::CosmicCalculator,
            BuildStage::CosmicStorage,
            BuildStage::CosmicMonitor,
            BuildStage::CosmicStore,
            BuildStage::CosmicPortal,
            BuildStage::CosmicAssets,
            BuildStage::Greetd,
            BuildStage::CosmicEdit,
            BuildStage::CosmicInitialSetup,
        ] {
            assert!(source_inputs(stage).contains(&implementation));
        }
        for stage in [
            BuildStage::Kernel,
            BuildStage::Glibc,
            BuildStage::Mesa,
            BuildStage::Flatpak,
            BuildStage::Rootfs,
            BuildStage::LiveRoot,
        ] {
            assert!(!source_inputs(stage).contains(&implementation));
        }
    }

    #[test]
    fn desktop_recipe_identity_changes_only_for_its_owners() {
        let root = tempfile::tempdir().expect("temporary repository");
        for path in [
            "src/desktop/cosmic/cosmic-store/README",
            "src/desktop/cosmic/libcosmic/README",
            "src/desktop/cosmic/iced/README",
            "src/tools/mattos-build/src/stages/desktop.rs",
        ] {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().expect("input parent"))
                .expect("create input parent");
            std::fs::write(path, "original\n").expect("write input");
        }

        let roots = source_inputs(BuildStage::CosmicStore);
        let baseline = crate::performance::tracked_source_digest(root.path(), &roots, false)
            .expect("baseline desktop identity");
        for path in [
            "src/tools/mattos-build/src/main.rs",
            "src/tools/mattos-build/src/stages/base_userland.rs",
            "src/tools/mattos-build/src/stages/image.rs",
            "src/tools/mattos-build/src/stages/desktop_aggregation.rs",
        ] {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().expect("unrelated parent"))
                .expect("create unrelated parent");
            std::fs::write(&path, "unrelated\n").expect("write unrelated input");
            assert_eq!(
                baseline,
                crate::performance::tracked_source_digest(root.path(), &roots, false)
                    .expect("narrow desktop identity"),
                "{path:?} must not invalidate CosmicStore"
            );
        }

        let implementation = root
            .path()
            .join("src/tools/mattos-build/src/stages/desktop.rs");
        std::fs::write(&implementation, "changed recipe\n").expect("change recipe");
        assert_ne!(
            baseline,
            crate::performance::tracked_source_digest(root.path(), &roots, false)
                .expect("changed desktop identity"),
            "an owned recipe change must invalidate CosmicStore"
        );
    }

    #[test]
    fn independent_cargo_stages_use_local_manifests_and_scoped_contracts() {
        let brush = configuration_inputs(BuildStage::Brush);
        assert!(
            !brush
                .iter()
                .any(|path| path == "Cargo.toml" || path == "Cargo.lock")
        );
        assert!(
            brush
                .iter()
                .any(|path| path == "out/source-ownership/cargo/contracts/brush.json")
        );
        assert_eq!(recipe_revision(BuildStage::Duktape), 3);
        assert_eq!(recipe_revision(BuildStage::Polkit), 2);
        assert_eq!(recipe_revision(BuildStage::CosmicWorkspaces), 2);
        assert_eq!(recipe_revision(BuildStage::Flatpak), 7);
        assert_eq!(recipe_revision(BuildStage::Ostree), 4);
        assert_eq!(recipe_revision(BuildStage::Curl), 2);
        assert_eq!(recipe_revision(BuildStage::GstreamerBase), 2);
        assert_eq!(recipe_revision(BuildStage::Rootfs), 5);
        assert_eq!(recipe_revision(BuildStage::LiveRoot), 2);
    }

    #[test]
    fn cosmic_files_text_editor_compatibility_is_pinned_to_output_patch() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let authoritative =
            std::fs::read_to_string(root.join("src/desktop/cosmic/cosmic-files/src/tab.rs"))
                .expect("authoritative cosmic-files source");
        let owned_libcosmic = std::fs::read_to_string(
            root.join("src/desktop/cosmic/libcosmic/src/widget/text_editor.rs"),
        )
        .expect("authoritative libcosmic text editor module");
        let manifest =
            std::fs::read_to_string(root.join("upstream/patches/cosmic-files/manifest.toml"))
                .expect("cosmic-files patch manifest");
        let patch =
            std::fs::read_to_string(root.join(
                "upstream/patches/cosmic-files/0001-adapt-text-editor-to-owned-libcosmic.patch",
            ))
            .expect("cosmic-files compatibility patch");

        // The pinned upstream tree intentionally remains unmodified and still
        // uses the pre-migration API. The owned libcosmic tree exposes the
        // migrated constructor, so the checked patch must remain an input to
        // the output mirror rather than being removed as obsolete.
        assert_eq!(
            authoritative
                .matches("widget::text_editor(content)")
                .count(),
            1
        );
        assert_eq!(
            authoritative.matches("widget::text_editor(text)").count(),
            1
        );
        assert!(owned_libcosmic.contains("pub fn text_editor"));
        assert!(manifest.contains("application = \"output-mirror-only\""));
        assert!(
            manifest.contains("upstream_commit = \"24e34eaa0f0acf4e24ea1338ad4bbde3a138e1f3\"")
        );
        assert!(manifest.contains(
            "sha256 = \"e35c8bce1c0787a54b227da4731447a362563d883208bf3ca30dccc0d10c51f4\""
        ));
        assert!(patch.contains("widget::text_editor::text_editor(content)"));
        assert!(patch.contains("widget::text_editor::text_editor(text)"));
        assert!(patch.contains(".style(text_editor_class)"));
        assert_eq!(
            source_inputs(BuildStage::CosmicFiles),
            vec![
                PathBuf::from("src/desktop/cosmic/cosmic-files"),
                PathBuf::from("src/desktop/cosmic/libcosmic"),
                PathBuf::from("src/desktop/cosmic/iced"),
                PathBuf::from("upstream/patches/cosmic-files"),
                PathBuf::from("src/tools/mattos-build/src/stages/desktop.rs"),
            ]
        );
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

    #[test]
    fn only_actual_libcosmic_git_consumers_own_the_libcosmic_source_identity() {
        let owned = [
            BuildStage::CosmicAppLibrary,
            BuildStage::CosmicBg,
            BuildStage::CosmicStore,
            BuildStage::CosmicPortal,
        ];
        for stage in owned {
            let inputs = source_inputs(stage);
            assert!(
                inputs.contains(&PathBuf::from("src/desktop/cosmic/libcosmic")),
                "{} must own its patched libcosmic source identity",
                crate::stage_graph::stage_id(stage)
            );
            assert!(
                inputs.contains(&PathBuf::from("src/desktop/cosmic/iced")),
                "{} must own its patched iced source identity",
                crate::stage_graph::stage_id(stage)
            );
        }
        for stage in [
            BuildStage::CosmicSession,
            BuildStage::CosmicAssets,
            BuildStage::CosmicUtilities,
            BuildStage::Flatpak,
        ] {
            let inputs = source_inputs(stage);
            assert!(
                !inputs.contains(&PathBuf::from("src/desktop/cosmic/libcosmic")),
                "{} must not inherit unrelated libcosmic invalidation",
                crate::stage_graph::stage_id(stage)
            );
        }
    }

    #[test]
    fn libcosmic_source_change_invalidates_its_consumers_but_not_desktop_siblings() {
        let root = tempfile::tempdir().expect("temporary repository");
        for path in [
            "src/desktop/cosmic/cosmic-store/README",
            "src/desktop/cosmic/cosmic-session/README",
            "src/desktop/cosmic/libcosmic/README",
            "src/desktop/cosmic/iced/README",
            "src/tools/mattos-build/src/stages/desktop.rs",
        ] {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().expect("input parent"))
                .expect("create input parent");
            std::fs::write(path, "original\n").expect("write input");
        }
        let store_inputs = source_inputs(BuildStage::CosmicStore);
        let session_inputs = source_inputs(BuildStage::CosmicSession);
        let store_before = crate::performance::tracked_source_digest(
            root.path(),
            &store_inputs,
            false,
        )
        .expect("store baseline");
        let session_before = crate::performance::tracked_source_digest(
            root.path(),
            &session_inputs,
            false,
        )
        .expect("session baseline");

        std::fs::write(
            root.path().join("src/desktop/cosmic/libcosmic/README"),
            "changed pinned libcosmic\n",
        )
        .expect("change libcosmic");

        assert_ne!(
            store_before,
            crate::performance::tracked_source_digest(root.path(), &store_inputs, false)
                .expect("changed store identity")
        );
        assert_eq!(
            session_before,
            crate::performance::tracked_source_digest(root.path(), &session_inputs, false)
                .expect("unrelated session identity")
        );
    }
}
