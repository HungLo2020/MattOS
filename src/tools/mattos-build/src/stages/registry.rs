fn stage_resource_profile(stage: BuildStage) -> scheduler::StageResourceProfile {
    if stage == BuildStage::Libcap {
        return scheduler::StageResourceProfile::serial();
    }
    if matches!(
        stage,
        BuildStage::Llvm
            | BuildStage::Mesa
            | BuildStage::QtBase
            | BuildStage::QtSvg
            | BuildStage::QtWayland
            | BuildStage::QtDeclarative
            | BuildStage::QtPositioning
            | BuildStage::QtLocation
            | BuildStage::QtShaderTools
            | BuildStage::QtTools
            | BuildStage::QtMultimedia
            | BuildStage::QtSpeech
            | BuildStage::QtCore5Compat
            | BuildStage::Flatpak
            | BuildStage::Greetd
    ) {
        return scheduler::StageResourceProfile::high_memory_parallel();
    }
    match stage {
        BuildStage::Kernel
        | BuildStage::Glibc
        | BuildStage::GccRuntime
        | BuildStage::Binutils
        | BuildStage::GccToolchain
        | BuildStage::Brush
        | BuildStage::Coreutils
        | BuildStage::Grep
        | BuildStage::Sed
        | BuildStage::Findutils
        | BuildStage::Diffutils
        | BuildStage::Git
        | BuildStage::Libffi
        | BuildStage::NvidiaDriver
        | BuildStage::Python
        | BuildStage::Rust
        | BuildStage::SudoRs
        | BuildStage::Init
        // Zstd-backed squashfs compression scales cleanly to four workers but
        // needs the same bounded per-worker memory admission as compilers.
        | BuildStage::LiveRoot => scheduler::StageResourceProfile::memory_heavy(),
        _ => scheduler::StageResourceProfile::standard(),
    }
}

#[cfg(test)]
fn scheduler_child_job_policy(stage: BuildStage) -> scheduler::ChildJobPolicy {
    stage_resource_profile(stage).child_jobs
}

fn is_cacheable_stage(stage: BuildStage) -> bool {
    !matches!(
        stage,
        BuildStage::Rootfs
            | BuildStage::LiveRoot
            | BuildStage::Initramfs
            | BuildStage::Iso
            | BuildStage::All
    )
}

fn build_stage_id(stage: BuildStage) -> &'static str {
    stage_graph::stage_id(stage)
}

fn build_stage_spec(stage: BuildStage) -> performance::StageSpec {
    let id = build_stage_id(stage);
    let sources = stage_inputs::source_inputs(stage);
    let outputs: Vec<PathBuf> = match stage {
        BuildStage::Kernel => vec![
            "out/build/linux/build/arch/x86/boot/bzImage".into(),
            "out/build/linux/modules/usr/lib/modules".into(),
            "out/build/linux/kernel-release".into(),
        ],
        BuildStage::Glibc => vec![
            "out/build/glibc/install".into(),
            "out/build/glibc/linux-headers".into(),
            "out/build/glibc/linux-headers-inventory.txt".into(),
            "out/sysroot/usr/include/stdio.h".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libc.so.6".into(),
            "out/sysroot/lib64/ld-linux-x86-64.so.2".into(),
        ],
        BuildStage::GccRuntime => vec![
            "out/build/gcc-runtime/install".into(),
            "out/build/gcc-runtime/runtime".into(),
            "out/build/gcc-runtime/runtime-abi.tsv".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libgcc_s.so.1".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libstdc++.so.6.0.34".into(),
        ],
        BuildStage::Binutils => vec![
            "out/build/binutils/cross-install".into(),
            "out/build/binutils/install".into(),
            "out/build/binutils/configure-invocation.txt".into(),
        ],
        BuildStage::GccToolchain => vec![
            "out/build/gcc-toolchain/install".into(),
            "out/build/gcc-toolchain/configure-invocation.txt".into(),
        ],
        BuildStage::Make => vec!["out/build/make/install".into()],
        BuildStage::Brush => vec!["out/build/brush/cargo-target/release/brush".into()],
        BuildStage::Coreutils => {
            vec!["out/build/coreutils/cargo-target/release/coreutils".into()]
        }
        BuildStage::Grep => vec!["out/build/grep/cargo-target/release/grep".into()],
        BuildStage::Sed => vec!["out/build/sed/cargo-target/release/sed".into()],
        BuildStage::Findutils => vec!["out/build/findutils/cargo-target/release/find".into()],
        BuildStage::Diffutils => {
            vec!["out/build/diffutils/cargo-target/release/diffutils".into()]
        }
        BuildStage::Gzip => vec!["out/build/gzip/install".into()],
        BuildStage::Patch => vec!["out/build/patch/install".into()],
        BuildStage::File => vec!["out/build/file/install".into()],
        BuildStage::Less => vec!["out/build/less/install".into()],
        BuildStage::Git => vec!["out/build/git/install".into()],
        BuildStage::Openssh => vec!["out/build/openssh/install".into()],
        BuildStage::Libffi => vec!["out/build/libffi/install".into()],
        BuildStage::QtBase => vec!["out/build/qtbase/install".into()],
        BuildStage::QtSvg => vec!["out/build/qtsvg/install".into()],
        BuildStage::QtWayland => vec!["out/build/qtwayland/install".into()],
        BuildStage::QtDeclarative => vec!["out/build/qtdeclarative/install".into()],
        BuildStage::QtPositioning => vec!["out/build/qtpositioning/install".into()],
        BuildStage::QtLocation => vec!["out/build/qtlocation/install".into()],
        BuildStage::QtShaderTools => vec!["out/build/qtshadertools/install".into()],
        BuildStage::QtTools => vec!["out/build/qttools/install".into()],
        BuildStage::QtMultimedia => vec!["out/build/qtmultimedia/install".into()],
        BuildStage::QtSpeech => vec!["out/build/qtspeech/install".into()],
        BuildStage::QtCore5Compat => vec!["out/build/qt5compat/install".into()],
        BuildStage::Qca => vec!["out/build/qca/install".into()],
        BuildStage::KCoreAddons => vec!["out/build/kcoreaddons/install".into()],
        BuildStage::KI18n => vec!["out/build/ki18n/install".into()],
        BuildStage::KWidgetsAddons => vec!["out/build/kwidgetsaddons/install".into()],
        BuildStage::KConfig => vec!["out/build/kconfig/install".into()],
        BuildStage::KConfigWidgets => vec!["out/build/kconfigwidgets/install/usr/lib/x86_64-linux-gnu/libKF6ConfigWidgets.so".into()],
        BuildStage::KDbusAddons => vec!["out/build/kdbusaddons/install".into()],
        BuildStage::KAuth => vec!["out/build/kauth/install".into()],
        BuildStage::KArchive => vec!["out/build/karchive/install".into()],
        BuildStage::KDecoration => vec!["out/build/kdecoration/install".into()],
        BuildStage::KWayland => vec!["out/build/kwayland/install".into()],
        BuildStage::KNightTime => vec!["out/build/knighttime/install".into()],
        BuildStage::KHolidays => vec!["out/build/kholidays/install".into()],
        BuildStage::Libcanberra => vec!["out/build/libcanberra/install".into()],
        BuildStage::Libqrencode => vec!["out/build/qrencode/install".into()],
        BuildStage::KirigamiPlatform => vec!["out/build/kirigami/install".into()],
        BuildStage::Qqc2DesktopStyle => vec!["out/build/qqc2-desktop-style/install".into()],
        BuildStage::KirigamiAddons => vec!["out/build/kirigami-addons/install".into()],
        BuildStage::KQuickCharts => vec!["out/build/kquickcharts/install".into()],
        BuildStage::PolkitQt6 => vec!["out/build/polkit-qt-1/install".into()],
        BuildStage::YamlCpp => vec!["out/build/yaml-cpp/install".into()],
        BuildStage::KPMCore => vec!["out/build/kpmcore/install".into()],
        BuildStage::Calamares => vec!["out/build/calamares/install".into()],
        BuildStage::Aurorae => vec!["out/build/aurorae/install".into()],
        BuildStage::KColorScheme => vec!["out/build/kcolorscheme/install".into()],
        BuildStage::KCrash => vec!["out/build/kcrash/install".into()],
        BuildStage::KGlobalAccel => vec!["out/build/kglobalaccel/install".into()],
        BuildStage::KGuiAddons => vec!["out/build/kguiaddons/install".into()],
        BuildStage::KIdleTime => vec!["out/build/kidletime/install".into()],
        BuildStage::KPackage => vec!["out/build/kpackage/install".into()],
        BuildStage::KService => vec!["out/build/kservice/install".into()],
        BuildStage::QCoro => vec!["out/build/qcoro/install".into()],
        BuildStage::KSvg => vec!["out/build/ksvg/install".into()],
        BuildStage::KDEDeclarative => vec!["out/build/kdeclarative/install/usr/lib/x86_64-linux-gnu/cmake/KF6Declarative/KF6DeclarativeConfig.cmake".into()],
        BuildStage::KIconThemes => vec!["out/build/kiconthemes/install".into()],
        BuildStage::BreezeIcons => vec!["out/build/breeze-icons/install/usr/share/icons/breeze/index.theme".into()],
        BuildStage::KItemModels => vec!["out/build/kitemmodels/install".into()],
        BuildStage::KItemViews => vec!["out/build/kitemviews/install".into()],
        BuildStage::KJobWidgets => vec!["out/build/kjobwidgets/install".into()],
        BuildStage::KCMUtils => vec!["out/build/kcmutils/install".into()],
        BuildStage::KDED => vec!["out/build/kded/install".into()],
        BuildStage::KIO => vec!["out/build/kio/install".into()],
        BuildStage::KUnitConversion => vec!["out/build/kunitconversion/install".into()],
        BuildStage::KSolid => vec!["out/build/solid/install".into()],
        BuildStage::KDocTools => vec!["out/build/kdoctools/install".into()],
        BuildStage::KBookmarks => vec!["out/build/kbookmarks/install".into()],
        BuildStage::KCompletion => vec!["out/build/kcompletion/install".into()],
        BuildStage::KCodecs => vec!["out/build/kcodecs/install".into()],
        BuildStage::KNewStuff => vec!["out/build/knewstuff/install".into()],
        BuildStage::KAttica => vec!["out/build/attica/install/usr/lib/x86_64-linux-gnu/libKF6Attica.so".into()],
        BuildStage::KNotifications => vec!["out/build/knotifications/install".into()],
        BuildStage::KNotifyConfig => vec!["out/build/knotifyconfig/install".into()],
        BuildStage::KParts => vec!["out/build/kparts/install".into()],
        BuildStage::KXmlGui => vec!["out/build/kxmlgui/install/usr/lib/x86_64-linux-gnu/libKF6XmlGui.so".into()],
        BuildStage::KPrison => vec!["out/build/prison/install".into()],
        BuildStage::KRunner => vec!["out/build/krunner/install".into()],
        BuildStage::KStatusNotifierItem => vec!["out/build/kstatusnotifieritem/install".into()],
        BuildStage::KTextEditor => vec!["out/build/ktexteditor/install".into()],
        BuildStage::KSyntaxHighlighting => vec!["out/build/ksyntaxhighlighting/install".into()],
        BuildStage::KTextWidgets => vec!["out/build/ktextwidgets/install".into()],
        BuildStage::KSonnet => vec!["out/build/sonnet/install/usr/lib/x86_64-linux-gnu/libKF6SonnetUi.so".into()],
        BuildStage::KWallet => vec!["out/build/kwallet/install".into()],
        BuildStage::KWindowSystem => vec!["out/build/kwindowsystem/install".into()],
        BuildStage::PlasmaWaylandProtocols => vec!["out/build/plasma-wayland-protocols/install".into()],
        BuildStage::WaylandProtocols => vec!["out/build/wayland-protocols/install".into()],
        BuildStage::PlasmaKWin | BuildStage::PlasmaFramework | BuildStage::PlasmaActivities | BuildStage::KActivityManagerd | BuildStage::KGlobalAccelD | BuildStage::PlasmaActivitiesStats | BuildStage::Plasma5Support | BuildStage::LibKScreen | BuildStage::LayerShellQt | BuildStage::KScreenLocker | BuildStage::KSysGuard | BuildStage::PlasmaWorkspace | BuildStage::PlasmaDesktop | BuildStage::Breeze | BuildStage::Icu | BuildStage::LmSensors | BuildStage::Highway
        | BuildStage::KFileMetadata | BuildStage::KPty | BuildStage::NetworkManagerQt | BuildStage::ModemManager | BuildStage::ModemManagerQt | BuildStage::KPurpose | BuildStage::Milou | BuildStage::SystemSettings | BuildStage::KSystemStats | BuildStage::PlasmaSystemMonitor | BuildStage::PolkitKdeAgent | BuildStage::KQuickImageEditor | BuildStage::Ffmpeg | BuildStage::Libva | BuildStage::OpenCv | BuildStage::ZxingCpp | BuildStage::SndFile | BuildStage::PulseAudioClient | BuildStage::LibGudev | BuildStage::Gmp | BuildStage::Mpfr | BuildStage::LibBytesize | BuildStage::Keyutils | BuildStage::LibNvme | BuildStage::Popt | BuildStage::JsonC | BuildStage::E2fsprogs | BuildStage::DeviceMapper | BuildStage::Cryptsetup | BuildStage::LibBlockdev | BuildStage::WirePlumber | BuildStage::UPower | BuildStage::UDisks2 | BuildStage::BlueZ | BuildStage::PowerProfilesDaemon | BuildStage::KPipeWire | BuildStage::Spectacle | BuildStage::PulseAudioQt | BuildStage::PlasmaPa | BuildStage::PlasmaNm | BuildStage::PowerDevil | BuildStage::XdgDesktopPortalKde | BuildStage::Dolphin | BuildStage::Konsole | BuildStage::Kate | BuildStage::Ark => vec![format!("out/build/{}/install", build_stage_id(stage)).into()],
        BuildStage::Wayland => vec!["out/build/wayland/install".into()],
        BuildStage::Xkbcommon => vec!["out/build/xkbcommon/install".into()],
        BuildStage::Libseat => vec!["out/build/seatd/install".into()],
        BuildStage::LibdisplayInfo => vec!["out/build/libdisplay-info/install".into()],
        BuildStage::Libevdev => vec!["out/build/libevdev/install".into()],
        BuildStage::Libinput => vec!["out/build/libinput/install".into()],
        BuildStage::Pixman => vec!["out/build/pixman/install".into()],
        BuildStage::Libdrm => vec!["out/build/libdrm/install".into()],
        BuildStage::VulkanHeaders => vec!["out/build/vulkan-headers/install".into()],
        BuildStage::VulkanLoader => vec!["out/build/vulkan-loader/install".into()],
        BuildStage::VulkanTools => vec!["out/build/vulkan-tools/install".into()],
        BuildStage::X11Compat => vec!["out/build/x11-compat/install".into()],
        BuildStage::Libepoxy => vec!["out/build/libepoxy/install/usr/lib/x86_64-linux-gnu/libepoxy.so.0".into()],
        BuildStage::Freetype => vec!["out/build/freetype/install/usr/lib/x86_64-linux-gnu/libfreetype.so.6".into()],
        BuildStage::Fontconfig => vec!["out/build/fontconfig/install/usr/lib/x86_64-linux-gnu/libfontconfig.so.1".into()],
        BuildStage::PopFonts => vec!["out/build/pop-fonts/install/usr/share/fonts/opentype/fira/FiraSans-Regular.otf".into()],
        BuildStage::MaterialCursors => vec![
            "out/build/material-cursors/install/usr/share/icons/material_light_cursors".into(),
            "out/host-tools/xcursorgen-1.0.9/bin/xcursorgen".into(),
        ],
        BuildStage::Libfontenc => vec!["out/build/libfontenc/install/usr/lib/x86_64-linux-gnu/libfontenc.so.1".into()],
        BuildStage::Libxfont => vec!["out/build/libxfont/install/usr/lib/x86_64-linux-gnu/libXfont2.so.2".into()],
        BuildStage::Libxcvt => vec!["out/build/libxcvt/install/usr/lib/x86_64-linux-gnu/libxcvt.so.0".into()],
        BuildStage::Lcms2 => vec!["out/build/lcms2/install/usr/lib/x86_64-linux-gnu/liblcms2.so.2".into()],
        BuildStage::Libxshmfence => vec!["out/build/libxshmfence/install/usr/lib/x86_64-linux-gnu/libxshmfence.so.1".into()],
        BuildStage::Libxkbfile => vec!["out/build/libxkbfile/install/usr/lib/x86_64-linux-gnu/libxkbfile.so.1".into()],
        BuildStage::Xkbcomp => vec!["out/build/xkbcomp/install/usr/bin/xkbcomp".into()],
        BuildStage::Libglvnd => vec!["out/build/libglvnd/install".into()],
        BuildStage::Mesa => vec!["out/build/mesa/install".into()],
        BuildStage::Xwayland => vec!["out/build/xwayland/install/usr/bin/Xwayland".into()],
        BuildStage::NvidiaDriver => vec![
            "out/build/nvidia-driver/install".into(),
            "out/build/nvidia-driver/source/LICENSE".into(),
            "out/build/nvidia-driver/runfile.sha256".into(),
        ],
        // Publish the complete target Duktape install, not only its SONAME
        // symlink.  The generated shared object and headers are the actual
        // ABI consumed by Polkit; omitting them from the inventory let a
        // corrupted library retain the old stage output digest and prevented
        // dependency-output propagation into Polkit.
        BuildStage::Duktape => vec!["out/build/duktape/install".into()],
        BuildStage::Flatpak => vec![
            "out/build/flatpak/install/usr/bin/flatpak".into(),
            "out/build/flatpak/install/usr/lib/x86_64-linux-gnu/libflatpak.so.0".into(),
            "out/build/flatpak/install/usr/libexec/mattos-flatpak-target-install".into(),
        ],
        BuildStage::Bubblewrap => vec!["out/build/bubblewrap/install/usr/bin/bwrap".into()],
        BuildStage::XdgDbusProxy => {
            vec!["out/build/xdg-dbus-proxy/install/usr/bin/xdg-dbus-proxy".into()]
        }
        // The portal package consumes the complete GStreamer installs,
        // including plugins such as libgstgio.so.  Publishing only one
        // library here allowed a changed plugin to leave the stage output
        // digest unchanged and a stale portal package cache to be reused.
        BuildStage::Gstreamer => vec!["out/build/gstreamer/install".into()],
        BuildStage::GstreamerBase => vec!["out/build/gstreamer-base/install".into()],
        // The portal package publishes the broker, document services,
        // validators, D-Bus activation files, and GStreamer plugins.  Its
        // cache contract must therefore cover the complete install tree, not
        // only the broker binary, or a changed helper can leave a stale .deb.
        BuildStage::XdgDesktopPortal => {
            vec!["out/build/xdg-desktop-portal/install".into()]
        }
        BuildStage::Libarchive => vec!["out/build/libarchive/install/usr/lib/x86_64-linux-gnu/libarchive.so.13".into()],
        BuildStage::Libxml2 => vec!["out/build/libxml2/install/usr/lib/x86_64-linux-gnu/libxml2.so.16".into()],
        BuildStage::Libpng => vec!["out/build/libpng/install/usr/lib/x86_64-linux-gnu/libpng16.so.16".into()],
        BuildStage::Fuse3 => vec!["out/build/fuse3/install/usr/lib/x86_64-linux-gnu/libfuse3.so.4".into()],
        BuildStage::Libfyaml => vec!["out/build/libfyaml/install/usr/lib/x86_64-linux-gnu/libfyaml.so.0".into()],
        BuildStage::Libxmlb => vec!["out/build/libxmlb/install/usr/lib/x86_64-linux-gnu/libxmlb.so.2".into()],
        BuildStage::JsonGlib => vec!["out/build/json-glib/install/usr/lib/x86_64-linux-gnu/libjson-glib-1.0.so.0".into()],
        BuildStage::Appstream => vec!["out/build/appstream/install/usr/lib/x86_64-linux-gnu/libappstream.so.5".into()],
        BuildStage::GdkPixbuf => vec!["out/build/gdk-pixbuf/install/usr/lib/x86_64-linux-gnu/libgdk_pixbuf-2.0.so.0".into()],
        BuildStage::Gpgme => vec!["out/build/gpgme/install/usr/lib/x86_64-linux-gnu/libgpgme.so.45".into()],
        BuildStage::Ostree => vec!["out/build/ostree/install/usr/lib/x86_64-linux-gnu/libostree-1.so.1".into()],
        BuildStage::Greetd => vec!["out/build/greetd/install/usr/bin/greetd".into()],
        BuildStage::PlasmaLoginManager => vec!["out/build/plasma-login-manager/install/usr/bin/plasmalogin".into()],
        BuildStage::Cozy => vec!["out/build/cozy/install/usr/bin/cozy".into()],
        BuildStage::Python => vec!["out/build/cpython/install".into()],
        BuildStage::Llvm => vec!["out/build/llvm/install".into()],
        BuildStage::Rust => vec!["out/build/rust/install".into()],
        BuildStage::SudoRs => vec!["out/build/sudo-rs/cargo-target/release/sudo".into()],
        BuildStage::Init => vec!["target/release/mattos-init".into()],
        BuildStage::Installer => vec![
            "out/build/installer/cargo-target/release/mattos-install".into(),
            "out/build/btrfs-progs/install/usr/bin/btrfs".into(),
            "out/build/btrfs-progs/install/usr/include/btrfsutil.h".into(),
            "out/build/btrfs-progs/install/usr/lib/x86_64-linux-gnu/libbtrfsutil.so".into(),
            "out/build/btrfs-progs/install/usr/lib/x86_64-linux-gnu/pkgconfig/libbtrfsutil.pc".into(),
            "out/build/dosfstools/install/usr/sbin/mkfs.fat".into(),
            "out/build/e2fsprogs/install/usr/sbin/mkfs.ext4".into(),
            "out/build/installed-initramfs.cpio.xz".into(),
            "out/build/installer/BOOTX64.EFI".into(),
        ],
        BuildStage::LiveRoot => vec![
            LIVE_ROOT_IMAGE_PATH.into(),
            "out/reports/live-root-inventory.tsv".into(),
        ],
        BuildStage::Initramfs => vec![INITRAMFS_ARCHIVE_PATH.into()],
        BuildStage::Iso => vec![
            "out/build/iso".into(),
            "out/images/mattos-x86_64.iso".into(),
            "out/reports/live-image-inventory.tsv".into(),
            "out/reports/artifacts.tsv".into(),
        ],
        BuildStage::Rootfs => vec!["out/build/rootfs".into()],
        _ => vec![format!("out/build/{}/install", stage_output_directory(stage)).into()],
    };
    performance::StageSpec {
        id: id.to_string(),
        source_inputs: sources,
        configuration_inputs: stage_inputs::configuration_inputs(stage),
        tools: stage_inputs::tool_names(stage),
        dependencies: build_stage_dependencies(stage)
            .iter()
            .map(|value| value.to_string())
            .collect(),
        outputs,
        recipe: format!(
            "mattos-build-stage:{id}:recipe={}:schema={}",
            stage_inputs::recipe_revision(stage),
            performance::STAGE_MANIFEST_SCHEMA_VERSION
        ),
    }
}

fn linux_x86_uapi_inputs() -> Vec<&'static str> {
    stage_inputs::linux_x86_uapi_inputs()
}

fn stage_output_directory(stage: BuildStage) -> &'static str {
    match stage {
        BuildStage::GccToolchain => "gcc-toolchain",
        BuildStage::Procps => "procps-ng",
        BuildStage::Iputils => "iputils",
        BuildStage::Pam => "linux-pam",
        _ => build_stage_id(stage),
    }
}

fn build_stage_dependencies(stage: BuildStage) -> &'static [&'static str] {
    stage_graph::direct_dependencies(stage)
}

fn linux_headers_stage_spec() -> performance::StageSpec {
    performance::StageSpec {
        id: "linux-headers".to_string(),
        source_inputs: linux_x86_uapi_inputs()
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        configuration_inputs: Vec::new(),
        tools: vec!["make".to_string(), "gcc".to_string()],
        dependencies: vec!["glibc".to_string()],
        outputs: vec![
            "out/build/glibc/linux-headers".into(),
            "out/build/glibc/linux-headers-inventory.txt".into(),
        ],
        recipe: "make ARCH=x86 headers_install".to_string(),
    }
}

fn formal_sysroot_stage_spec() -> performance::StageSpec {
    performance::StageSpec {
        id: "formal-sysroot".to_string(),
        source_inputs: Vec::new(),
        configuration_inputs: Vec::new(),
        tools: vec!["gcc".to_string(), "ld".to_string()],
        dependencies: vec![
            "linux-headers".to_string(),
            "glibc".to_string(),
            "gcc-runtime".to_string(),
        ],
        outputs: vec![
            "out/sysroot/usr/include/stdio.h".into(),
            "out/sysroot/usr/include/linux/version.h".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libc.so.6".into(),
            "out/sysroot/lib64/ld-linux-x86-64.so.2".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libgcc_s.so.1".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libstdc++.so.6.0.34".into(),
        ],
        recipe: "formal MattOS sysroot inventory".to_string(),
    }
}

fn validate_cached_build_stage(repo_root: &Path, stage: BuildStage) -> Result<()> {
    match stage {
        BuildStage::Kernel => {
            if !repo_root
                .join("out/build/linux/build/arch/x86/boot/bzImage")
                .is_file()
            {
                bail!("cached Linux image is missing")
            }
        }
        BuildStage::Glibc => {
            for path in [
                "out/sysroot/usr/include/stdio.h",
                "out/sysroot/usr/lib/x86_64-linux-gnu/libc.so.6",
                "out/sysroot/lib64/ld-linux-x86-64.so.2",
            ] {
                if !repo_root.join(path).exists() {
                    bail!("cached glibc/sysroot output is missing: {path}")
                }
            }
        }
        BuildStage::GccRuntime => {
            if !repo_root
                .join("out/sysroot/usr/lib/x86_64-linux-gnu/libgcc_s.so.1")
                .is_file()
            {
                bail!("cached GCC runtime is missing")
            }
        }
        BuildStage::Rust => validate_cached_rust_install(repo_root)?,
        BuildStage::Binutils => {
            for tool in ["as", "ld", "readelf", "strip"] {
                if !repo_root
                    .join("out/build/binutils/install/usr/bin")
                    .join(tool)
                    .is_file()
                {
                    bail!("cached native Binutils tool is missing: {tool}")
                }
            }
        }
        BuildStage::GccToolchain => {
            for tool in ["gcc", "g++"] {
                if !repo_root
                    .join("out/build/gcc-toolchain/install/usr/bin")
                    .join(tool)
                    .is_file()
                {
                    bail!("cached native compiler is missing: {tool}")
                }
            }
        }
        BuildStage::Make => {
            if !repo_root
                .join("out/build/make/install/usr/bin/make")
                .is_file()
            {
                bail!("cached native GNU Make is missing")
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_cached_rust_install(repo_root: &Path) -> Result<()> {
    let install = repo_root.join("out/build/rust/install/usr");
    let rustc = install.join("bin/rustc");
    let cargo = install.join("bin/cargo");
    if !rustc.is_file() || !cargo.is_file() {
        bail!("cached Rust installation is missing rustc or Cargo")
    }
    let rustc_path = path_str(&rustc)?;
    let sysroot = run_cmd_capture(&install, rustc_path, &["--print", "sysroot"])?;
    let reported_sysroot = PathBuf::from(sysroot.trim());
    let expected_sysroot = install.clone();
    let canonical_reported = reported_sysroot.canonicalize().with_context(|| {
        format!(
            "published rustc reported missing sysroot {}",
            reported_sysroot.display()
        )
    })?;
    let canonical_expected = expected_sysroot.canonicalize()?;
    if canonical_reported != canonical_expected {
        bail!(
            "published rustc/sysroot mismatch: rustc reports {}, expected {}",
            canonical_reported.display(),
            canonical_expected.display()
        )
    }
    let target_libdir = run_cmd_capture(&install, rustc_path, &["--print", "target-libdir"])?;
    let target_libdir = PathBuf::from(target_libdir.trim());
    if !target_libdir.is_dir() || !target_libdir.starts_with(&install) {
        bail!(
            "published rustc target library directory is outside its install: {}",
            target_libdir.display()
        )
    }
    if fs::read_dir(&target_libdir)?
        .filter_map(Result::ok)
        .all(|entry| {
            !entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "rlib" || extension == "rmeta")
        })
    {
        bail!("published Rust target library directory has no compiler sysroot artifacts")
    }
    Ok(())
}

fn build_plan(stage: BuildStage) -> Vec<BuildStage> {
    stage_graph::build_plan(stage)
}

fn cacheable_stage_specs(repo_root: &Path) -> Result<Vec<performance::StageSpec>> {
    let mut specs = build_plan(BuildStage::All)
        .into_iter()
        .filter(|stage| {
            is_cacheable_stage(*stage)
                || matches!(
                    stage,
                    BuildStage::Rootfs
                        | BuildStage::LiveRoot
                        | BuildStage::Initramfs
                        | BuildStage::Iso
                )
        })
        .map(build_stage_spec)
        .collect::<Vec<_>>();
    specs.push(linux_headers_stage_spec());
    specs.push(formal_sysroot_stage_spec());
    if let Ok(repository) = packaging::repository_stage_spec(repo_root) {
        specs.push(repository);
    }
    specs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(specs)
}

fn build_stage(repo_root: &Path, stage: BuildStage) -> Result<()> {
    performance::trace_log_context("build_stage-entry");
    match stage {
        BuildStage::Kernel => build_kernel(repo_root),
        BuildStage::Glibc => build_glibc(repo_root),
        BuildStage::GccRuntime => build_gcc_runtime(repo_root),
        BuildStage::Binutils => build_binutils(repo_root),
        BuildStage::GccToolchain => {
            performance::trace_log_context("build_stage-before-gcc-toolchain-dispatch");
            build_gcc_toolchain(repo_root)
        }
        BuildStage::Make => build_make(repo_root),
        BuildStage::Brush => build_brush(repo_root),
        BuildStage::Coreutils => build_coreutils(repo_root),
        BuildStage::Grep => build_grep(repo_root),
        BuildStage::Sed => build_sed(repo_root),
        BuildStage::Findutils => build_findutils(repo_root),
        BuildStage::Diffutils => build_diffutils(repo_root),
        BuildStage::Gzip => build_gzip(repo_root),
        BuildStage::Patch => build_patch(repo_root),
        BuildStage::File => build_file(repo_root),
        BuildStage::Less => build_less(repo_root),
        BuildStage::Git => build_git(repo_root),
        BuildStage::Openssh => build_openssh(repo_root),
        BuildStage::Libffi => build_libffi(repo_root),
        BuildStage::QtBase => build_qtbase(repo_root),
        BuildStage::QtSvg => build_qtsvg(repo_root),
        BuildStage::QtWayland => build_qtwayland(repo_root),
        BuildStage::QtDeclarative => build_qtdeclarative(repo_root),
        BuildStage::QtPositioning => build_qtpositioning(repo_root),
        BuildStage::QtLocation => build_qtlocation(repo_root),
        BuildStage::QtShaderTools => build_qtshadertools(repo_root),
        BuildStage::QtTools => build_qttools(repo_root),
        BuildStage::QtMultimedia => build_qtmultimedia(repo_root),
        BuildStage::QtSpeech => build_qtspeech(repo_root),
        BuildStage::QtCore5Compat => build_qt5compat(repo_root),
        BuildStage::Qca => build_qca(repo_root),
        BuildStage::KCoreAddons => build_kcoreaddons(repo_root),
        BuildStage::KI18n => build_ki18n(repo_root),
        BuildStage::KWidgetsAddons => build_kwidgetsaddons(repo_root),
        BuildStage::KConfig => build_kconfig(repo_root),
        BuildStage::KConfigWidgets => build_kconfigwidgets(repo_root),
        BuildStage::KDbusAddons => build_kdbusaddons(repo_root),
        BuildStage::KAuth => build_kauth(repo_root),
        BuildStage::KArchive => build_karchive(repo_root),
        BuildStage::KDecoration => build_kdecoration(repo_root),
        BuildStage::KWayland => build_kwayland(repo_root),
        BuildStage::KNightTime => build_knighttime(repo_root),
        BuildStage::KHolidays => build_kholidays(repo_root),
        BuildStage::Libcanberra => build_libcanberra(repo_root),
        BuildStage::Libqrencode => build_libqrencode(repo_root),
        BuildStage::KirigamiPlatform => build_kirigami_platform(repo_root),
        BuildStage::Qqc2DesktopStyle => build_qqc2_desktop_style(repo_root),
        BuildStage::KirigamiAddons => build_kirigami_addons(repo_root),
        BuildStage::KQuickCharts => build_kquickcharts(repo_root),
        BuildStage::PolkitQt6 => build_polkit_qt6(repo_root),
        BuildStage::YamlCpp => build_yaml_cpp(repo_root),
        BuildStage::KPMCore => build_kpmcore(repo_root),
        BuildStage::Calamares => build_calamares(repo_root),
        BuildStage::KColorScheme => build_kcolorscheme(repo_root),
        BuildStage::KCrash => build_kcrash(repo_root),
        BuildStage::KGlobalAccel => build_kglobalaccel(repo_root),
        BuildStage::KGuiAddons => build_kguiaddons(repo_root),
        BuildStage::KIdleTime => build_kidletime(repo_root),
        BuildStage::KPackage => build_kpackage(repo_root),
        BuildStage::KService => build_kservice(repo_root),
        BuildStage::QCoro => build_qcoro(repo_root),
        BuildStage::KSvg => build_ksvg(repo_root),
        BuildStage::KDEDeclarative => build_kdeclarative(repo_root),
        BuildStage::KIconThemes => build_kiconthemes(repo_root),
        BuildStage::BreezeIcons => build_breeze_icons(repo_root),
        BuildStage::KItemModels => build_kitemmodels(repo_root),
        BuildStage::KItemViews => build_kitemviews(repo_root),
        BuildStage::KJobWidgets => build_kjobwidgets(repo_root),
        BuildStage::KCMUtils => build_kcmutils(repo_root),
        BuildStage::KDED => build_kded(repo_root),
        BuildStage::KIO => build_kio(repo_root),
        BuildStage::KUnitConversion => build_kunitconversion(repo_root),
        BuildStage::KSolid => build_ksolid(repo_root),
        BuildStage::KDocTools => build_kdoctools(repo_root),
        BuildStage::KBookmarks => build_kbookmarks(repo_root),
        BuildStage::KCompletion => build_kcompletion(repo_root),
        BuildStage::KCodecs => build_kcodecs(repo_root),
        BuildStage::KNewStuff => build_knewstuff(repo_root),
        BuildStage::KAttica => build_kattica(repo_root),
        BuildStage::KNotifications => build_knotifications(repo_root),
        BuildStage::KNotifyConfig => build_knotifyconfig(repo_root),
        BuildStage::KParts => build_kparts(repo_root),
        BuildStage::KXmlGui => build_kxmlgui(repo_root),
        BuildStage::KPrison => build_prison(repo_root),
        BuildStage::KRunner => build_krunner(repo_root),
        BuildStage::KStatusNotifierItem => build_kstatusnotifieritem(repo_root),
        BuildStage::KTextEditor => build_ktexteditor(repo_root),
        BuildStage::KSyntaxHighlighting => build_ksyntaxhighlighting(repo_root),
        BuildStage::KTextWidgets => build_ktextwidgets(repo_root),
        BuildStage::KSonnet => build_ksonnet(repo_root),
        BuildStage::KWallet => build_kwallet(repo_root),
        BuildStage::KWindowSystem => build_kwindowsystem(repo_root),
        BuildStage::PlasmaWaylandProtocols => build_plasma_wayland_protocols(repo_root),
        BuildStage::WaylandProtocols => build_wayland_protocols(repo_root),
        BuildStage::Aurorae => build_aurorae(repo_root),
        BuildStage::PlasmaKWin => build_kwin(repo_root),
        BuildStage::PlasmaFramework => build_plasma_framework(repo_root),
        BuildStage::PlasmaActivities => build_plasma_activities(repo_root),
        BuildStage::KActivityManagerd => build_kactivitymanagerd(repo_root),
        BuildStage::KGlobalAccelD => build_kglobalacceld(repo_root),
        BuildStage::PlasmaActivitiesStats => build_plasma_activities_stats(repo_root),
        BuildStage::Plasma5Support => build_plasma5support(repo_root),
        BuildStage::LibKScreen => build_kscreen(repo_root),
        BuildStage::LayerShellQt => build_layer_shell_qt(repo_root),
        BuildStage::KScreenLocker => build_kscreen_locker(repo_root),
        BuildStage::KSysGuard => build_ksysguard(repo_root),
        BuildStage::Icu => build_icu(repo_root),
        BuildStage::LmSensors => build_lm_sensors(repo_root),
        BuildStage::Highway => build_highway(repo_root),
        BuildStage::PlasmaWorkspace => build_plasma_workspace(repo_root),
        BuildStage::PlasmaDesktop => build_plasma_desktop(repo_root),
        BuildStage::PlasmaLoginManager => build_plasma_login_manager(repo_root),
        BuildStage::Breeze => build_breeze(repo_root),
        BuildStage::KFileMetadata => build_kfilemetadata(repo_root),
        BuildStage::KPty => build_kpty(repo_root),
        BuildStage::NetworkManagerQt => build_networkmanager_qt(repo_root),
        BuildStage::ModemManager => build_modemmanager(repo_root),
        BuildStage::ModemManagerQt => build_modemmanager_qt(repo_root),
        BuildStage::KPurpose => build_purpose(repo_root),
        BuildStage::Milou => build_milou(repo_root),
        BuildStage::SystemSettings => build_systemsettings(repo_root),
        BuildStage::KSystemStats => build_ksystemstats(repo_root),
        BuildStage::PlasmaSystemMonitor => build_plasma_systemmonitor(repo_root),
        BuildStage::PolkitKdeAgent => build_polkit_kde_agent(repo_root),
        BuildStage::KQuickImageEditor => build_kquickimageeditor(repo_root),
        BuildStage::Ffmpeg => build_ffmpeg(repo_root),
        BuildStage::Libva => build_libva(repo_root),
        BuildStage::OpenCv => build_opencv(repo_root),
        BuildStage::ZxingCpp => build_zxing_cpp(repo_root),
        BuildStage::SndFile => build_libsndfile(repo_root),
        BuildStage::PulseAudioClient => build_pulseaudio_client(repo_root),
        BuildStage::LibGudev => build_libgudev(repo_root),
        BuildStage::Gmp => build_gmp(repo_root),
        BuildStage::Mpfr => build_mpfr(repo_root),
        BuildStage::LibBytesize => build_libbytesize(repo_root),
        BuildStage::Keyutils => build_keyutils(repo_root),
        BuildStage::LibNvme => build_libnvme(repo_root),
        BuildStage::Popt => build_popt(repo_root),
        BuildStage::JsonC => build_json_c(repo_root),
        BuildStage::E2fsprogs => build_e2fsprogs(repo_root),
        BuildStage::DeviceMapper => build_device_mapper(repo_root),
        BuildStage::Cryptsetup => build_cryptsetup(repo_root),
        BuildStage::LibBlockdev => build_libblockdev(repo_root),
        BuildStage::WirePlumber => build_wireplumber(repo_root),
        BuildStage::UPower => build_upower(repo_root),
        BuildStage::UDisks2 => build_udisks2(repo_root),
        BuildStage::BlueZ => build_bluez(repo_root),
        BuildStage::PowerProfilesDaemon => build_power_profiles_daemon(repo_root),
        BuildStage::KPipeWire => build_kpipewire(repo_root),
        BuildStage::Spectacle => build_spectacle(repo_root),
        BuildStage::PulseAudioQt => build_pulseaudio_qt(repo_root),
        BuildStage::PlasmaPa => build_plasma_pa(repo_root),
        BuildStage::PlasmaNm => build_plasma_nm(repo_root),
        BuildStage::PowerDevil => build_powerdevil(repo_root),
        BuildStage::XdgDesktopPortalKde => build_xdg_desktop_portal_kde(repo_root),
        BuildStage::Dolphin => build_dolphin(repo_root),
        BuildStage::Konsole => build_konsole(repo_root),
        BuildStage::Kate => build_kate(repo_root),
        BuildStage::Ark => build_ark(repo_root),
        BuildStage::Wayland => build_wayland(repo_root),
        BuildStage::Xkbcommon => build_xkbcommon(repo_root),
        BuildStage::Libseat => build_libseat(repo_root),
        BuildStage::LibdisplayInfo => build_libdisplay_info(repo_root),
        BuildStage::Libevdev => build_libevdev(repo_root),
        BuildStage::Libinput => build_libinput(repo_root),
        BuildStage::Pixman => build_pixman(repo_root),
        BuildStage::Libdrm => build_libdrm(repo_root),
        BuildStage::VulkanHeaders => build_vulkan_headers(repo_root),
        BuildStage::VulkanLoader => build_vulkan_loader(repo_root),
        BuildStage::VulkanTools => build_vulkan_tools(repo_root),
        BuildStage::X11Compat => build_x11_compat(repo_root),
        BuildStage::Libepoxy => build_libepoxy(repo_root),
        BuildStage::Freetype => build_freetype(repo_root),
        BuildStage::Fontconfig => build_fontconfig(repo_root),
        BuildStage::PopFonts => build_pop_fonts(repo_root),
        BuildStage::MaterialCursors => build_material_cursors(repo_root),
        BuildStage::Libfontenc => build_libfontenc(repo_root),
        BuildStage::Libxfont => build_libxfont(repo_root),
        BuildStage::Libxcvt => build_libxcvt(repo_root),
        BuildStage::Lcms2 => build_lcms2(repo_root),
        BuildStage::Libxshmfence => build_libxshmfence(repo_root),
        BuildStage::Libxkbfile => build_libxkbfile(repo_root),
        BuildStage::Xkbcomp => build_xkbcomp(repo_root),
        BuildStage::Libglvnd => build_libglvnd(repo_root),
        BuildStage::Mesa => build_mesa(repo_root),
        BuildStage::Xwayland => build_xwayland(repo_root),
        BuildStage::NvidiaDriver => build_nvidia_driver(repo_root),
        BuildStage::Flatpak => build_flatpak(repo_root),
        BuildStage::Bubblewrap => build_bubblewrap(repo_root),
        BuildStage::XdgDbusProxy => build_xdg_dbus_proxy(repo_root),
        BuildStage::Gstreamer => build_gstreamer(repo_root),
        BuildStage::GstreamerBase => build_gstreamer_base(repo_root),
        BuildStage::XdgDesktopPortal => build_xdg_desktop_portal(repo_root),
        BuildStage::Libarchive => build_libarchive(repo_root),
        BuildStage::Libxml2 => build_libxml2(repo_root),
        BuildStage::Libpng => build_libpng(repo_root),
        BuildStage::Fuse3 => build_fuse3(repo_root),
        BuildStage::Libfyaml => build_libfyaml(repo_root),
        BuildStage::Libxmlb => build_libxmlb(repo_root),
        BuildStage::JsonGlib => build_json_glib(repo_root),
        BuildStage::Appstream => build_appstream(repo_root),
        BuildStage::GdkPixbuf => build_gdk_pixbuf(repo_root),
        BuildStage::Gpgme => build_gpgme(repo_root),
        BuildStage::Ostree => build_ostree(repo_root),
        BuildStage::Greetd => build_greetd(repo_root),
        BuildStage::Cozy => build_cozy(repo_root),
        BuildStage::Python => build_cpython(repo_root),
        BuildStage::Llvm => build_llvm(repo_root),
        BuildStage::Rust => build_rust(repo_root),
        BuildStage::Kmod => build_kmod(repo_root),
        BuildStage::Procps => build_procps(repo_root),
        BuildStage::Ncurses => build_ncurses(repo_root),
        BuildStage::Iproute2 => build_iproute2(repo_root),
        BuildStage::Iputils => build_iputils(repo_root),
        BuildStage::Curl => build_curl(repo_root),
        BuildStage::Expat => build_expat(repo_root),
        BuildStage::Libcap => build_libcap(repo_root),
        BuildStage::Attr => build_attr(repo_root),
        BuildStage::Tar => build_tar(repo_root),
        BuildStage::Acl => build_acl(repo_root),
        BuildStage::Zlib => build_zlib(repo_root),
        BuildStage::Bzip2 => build_bzip2(repo_root),
        BuildStage::Lz4 => build_lz4(repo_root),
        BuildStage::Xz => build_xz(repo_root),
        BuildStage::Xxhash => build_xxhash(repo_root),
        BuildStage::Zstd => build_zstd(repo_root),
        BuildStage::Dav1d => build_dav1d(repo_root),
        BuildStage::Glib => build_glib(repo_root),
        BuildStage::Pipewire => build_pipewire(repo_root),
        BuildStage::Openssl => build_openssl(repo_root),
        BuildStage::Elfutils => build_elfutils(repo_root),
        BuildStage::Pcre2 => build_pcre2(repo_root),
        BuildStage::Selinux => build_selinux(repo_root),
        BuildStage::Libxcrypt => build_libxcrypt(repo_root),
        BuildStage::Libmd => build_libmd(repo_root),
        BuildStage::Libbsd => build_libbsd(repo_root),
        BuildStage::Libndp => build_libndp(repo_root),
        BuildStage::Readline => build_readline(repo_root),
        BuildStage::Pam => build_linux_pam(repo_root),
        BuildStage::Shadow => build_shadow(repo_root),
        BuildStage::SudoRs => build_sudo_rs(repo_root),
        BuildStage::UtilLinux => build_util_linux(repo_root),
        BuildStage::Systemd => build_systemd(repo_root),
        BuildStage::Dbus => build_dbus(repo_root),
        BuildStage::DbusBroker => build_dbus_broker(repo_root),
        BuildStage::Dpkg => packaging::build_dpkg(repo_root),
        BuildStage::LibgpgError => {
            build_gpg_autotools_library(repo_root, "libgpg-error", &[], "libgpg-error.so.0")
        }
        BuildStage::Libgcrypt => build_gpg_autotools_library(
            repo_root,
            "libgcrypt",
            &["libgpg-error"],
            "libgcrypt.so.20",
        ),
        BuildStage::Libassuan => {
            build_gpg_autotools_library(repo_root, "libassuan", &["libgpg-error"], "libassuan.so.9")
        }
        BuildStage::Libksba => {
            build_gpg_autotools_library(repo_root, "libksba", &["libgpg-error"], "libksba.so.8")
        }
        BuildStage::Npth => build_gpg_autotools_library(repo_root, "npth", &[], "libnpth.so.0"),
        BuildStage::Gpgv => build_gpgv(repo_root),
        BuildStage::Polkit => build_polkit(repo_root),
        BuildStage::Duktape => build_duktape(repo_root),
        BuildStage::NetworkManager => build_networkmanager(repo_root),
        BuildStage::Libnl => build_libnl(repo_root),
        BuildStage::WpaSupplicant => build_wpa_supplicant(repo_root),
        BuildStage::Grub => build_grub(repo_root),
        BuildStage::Apt => packaging::build_apt(repo_root),
        BuildStage::Init => build_init(repo_root),
        BuildStage::Installer => build_installer(repo_root),
        BuildStage::Rootfs => build_rootfs(repo_root),
        BuildStage::LiveRoot => build_live_root(repo_root),
        BuildStage::Initramfs => build_initramfs(repo_root),
        BuildStage::Iso => build_iso(repo_root),
        BuildStage::All => {
            bail!("internal error: BuildStage::All should be expanded by build_plan")
        }
    }
}

#[derive(Debug, Deserialize)]
struct KernelConfigPolicy {
    minimum_module_symbols: usize,
    builtin: Vec<String>,
    module: Vec<String>,
    unsupported: Vec<String>,
    unsupported_prefixes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KernelConfigState {
    Builtin,
    Module,
    Unsupported,
}
