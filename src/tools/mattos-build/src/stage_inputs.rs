use crate::stage_graph::BuildStage;
use std::path::PathBuf;

pub(crate) const AUTHORITATIVE_GRUB_CFG: &str = "src/boot/grub/grub.cfg";

pub(crate) fn source_inputs(stage: BuildStage) -> Vec<PathBuf> {
    let roots: &[&str] = match stage {
        BuildStage::Kernel => &[
            "src/kernel/linux",
            "src/kernel/config/x86_64_mattos.config",
            "src/kernel/config/x86_64_mattos.policy.toml",
            "src/tools/mattos-build/src/stages/toolchain.rs",
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
            "src/tools/mattos-build/src/stages/foundation_libraries.rs",
        ],
        BuildStage::Patch => &[
            "src/userland/patch",
            "upstream/policies/release-archives.toml",
            "src/tools/mattos-build/src/stages/foundation_libraries.rs",
        ],
        BuildStage::File => &["src/userland/file"],
        BuildStage::Less => &[
            "src/userland/less",
            "upstream/policies/release-archives.toml",
        ],
        BuildStage::Git => &["src/userland/git"],
        BuildStage::Openssh => &["src/system/network/openssh-portable"],
        BuildStage::Libffi => &["src/system/libraries/libffi/libffi"],
        BuildStage::QtBase => &[
            "src/desktop/qt/qtbase",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        BuildStage::QtSvg => &[
            "src/desktop/qt/qtsvg",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        BuildStage::QtWayland => &[
            "src/desktop/qt/qtwayland",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        BuildStage::QtDeclarative => &[
            "src/desktop/qt/qtdeclarative",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        BuildStage::QtShaderTools => &[
            "src/desktop/qt/qtshadertools",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        BuildStage::QtPositioning => &[
            "src/desktop/qt/qtpositioning",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        BuildStage::QtLocation => &[
            "src/desktop/qt/qtlocation",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        BuildStage::QtTools => &[
            "src/desktop/qt/qttools",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        BuildStage::QtMultimedia => &[
            "src/desktop/qt/qtmultimedia",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        BuildStage::QtSpeech => &[
            "src/desktop/qt/qtspeech",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        BuildStage::QtCore5Compat => &[
            "src/desktop/qt/qt5compat",
            "src/tools/mattos-build/src/stages/qt.rs",
        ],
        BuildStage::Qca => &[
            "src/system/security/qca",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KCoreAddons => &[
            "src/desktop/kde/kcoreaddons",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KI18n => &[
            "src/desktop/kde/ki18n",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KWidgetsAddons => &[
            "src/desktop/kde/kwidgetsaddons",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KConfig => &[
            "src/desktop/kde/kconfig",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KConfigWidgets => &[
            "src/desktop/kde/kconfigwidgets",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KDbusAddons => &[
            "src/desktop/kde/kdbusaddons",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KAuth => &[
            "src/desktop/kde/kauth",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KArchive => &[
            "src/desktop/kde/karchive",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KDecoration => &[
            "src/desktop/kde/kdecoration",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KWayland => &[
            "src/desktop/kde/kwayland",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KNightTime => &[
            "src/desktop/kde/knighttime",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KHolidays => &[
            "src/desktop/kde/kholidays",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::Libcanberra => &[
            "src/system/libraries/libcanberra",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::Libqrencode => &[
            "src/system/libraries/qrencode",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KirigamiPlatform => &[
            "src/desktop/kde/kirigami",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::Qqc2DesktopStyle => &[
            "src/desktop/kde/qqc2-desktop-style",
            "src/tools/mattos-build/src/stages/plasma.rs",
        ],
        BuildStage::KirigamiAddons => &[
            "src/desktop/kde/kirigami-addons",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KQuickCharts => &[
            "src/desktop/kde/kquickcharts",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KColorScheme => &[
            "src/desktop/kde/kcolorscheme",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KCrash => &[
            "src/desktop/kde/kcrash",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KGlobalAccel => &[
            "src/desktop/kde/kglobalaccel",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KGuiAddons => &[
            "src/desktop/kde/kguiaddons",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KIdleTime => &[
            "src/desktop/kde/kidletime",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KPackage => &[
            "src/desktop/kde/kpackage",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KService => &[
            "src/desktop/kde/kservice",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::QCoro => &[
            "src/desktop/kde/qcoro",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KSvg => &[
            "src/desktop/kde/ksvg",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KDEDeclarative => &[
            "src/desktop/kde/kdeclarative",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KIconThemes => &[
            "src/desktop/kde/kiconthemes",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::BreezeIcons => &[
            "src/desktop/kde/breeze-icons",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KItemModels => &[
            "src/desktop/kde/kitemmodels",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KItemViews => &[
            "src/desktop/kde/kitemviews",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KJobWidgets => &[
            "src/desktop/kde/kjobwidgets",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KCMUtils => &[
            "src/desktop/kde/kcmutils",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KDED => &[
            "src/desktop/kde/kded",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KIO => &[
            "src/desktop/kde/kio",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KUnitConversion => &[
            "src/desktop/kde/kunitconversion",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KSolid => &[
            "src/desktop/kde/solid",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KDocTools => &[
            "src/desktop/kde/kdoctools",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KBookmarks => &[
            "src/desktop/kde/kbookmarks",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KCompletion => &[
            "src/desktop/kde/kcompletion",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KCodecs => &[
            "src/desktop/kde/kcodecs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KNewStuff => &[
            "src/desktop/kde/knewstuff",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KAttica => &[
            "src/desktop/kde/attica",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KNotifications => &[
            "src/desktop/kde/knotifications",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KNotifyConfig => &[
            "src/desktop/kde/knotifyconfig",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KParts => &[
            "src/desktop/kde/kparts",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KXmlGui => &[
            "src/desktop/kde/kxmlgui",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KPrison => &[
            "src/desktop/kde/prison",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KRunner => &[
            "src/desktop/kde/krunner",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KStatusNotifierItem => &[
            "src/desktop/kde/kstatusnotifieritem",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KTextEditor => &[
            "src/desktop/kde/ktexteditor",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KSyntaxHighlighting => &[
            "src/desktop/kde/ksyntaxhighlighting",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KTextWidgets => &[
            "src/desktop/kde/ktextwidgets",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KSonnet => &[
            "src/desktop/kde/sonnet",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KWallet => &[
            "src/desktop/kde/kwallet",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KWindowSystem => &[
            "src/desktop/kde/kwindowsystem",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::PlasmaWaylandProtocols => &[
            "src/desktop/kde/plasma-wayland-protocols",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::WaylandProtocols => &[
            "src/graphics/wayland-protocols",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::PolkitQt6 => &[
            "src/system/security/polkit-qt-1",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::YamlCpp => &[
            "src/system/libraries/yaml-cpp",
            "upstream/patches/yaml-cpp",
            "upstream/state/yaml-cpp.toml",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KPMCore => &[
            "src/system/storage/kpmcore",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::PlasmaKWin => &[
            "src/desktop/kde/kwin",
            "upstream/patches/kwin",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::PlasmaFramework => &[
            "src/desktop/kde/plasma-framework",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::PlasmaActivities => &[
            "src/desktop/kde/plasma-activities",
            "src/tools/mattos-build/src/stages/plasma.rs",
        ],
        BuildStage::KActivityManagerd => &[
            "src/desktop/kde/kactivitymanagerd",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KGlobalAccelD => &[
            "src/desktop/kde/kglobalacceld",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::PlasmaActivitiesStats => &[
            "src/desktop/kde/plasma-activities-stats",
            "src/tools/mattos-build/src/stages/plasma.rs",
        ],
        BuildStage::Plasma5Support => &[
            "src/desktop/kde/plasma5support",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::LibKScreen => &[
            "src/desktop/kde/libkscreen",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::LayerShellQt => &[
            "src/desktop/kde/layer-shell-qt",
            "src/tools/mattos-build/src/stages/plasma.rs",
        ],
        BuildStage::KScreenLocker => &[
            "src/desktop/kde/kscreenlocker",
            "src/tools/mattos-build/src/stages/plasma.rs",
        ],
        BuildStage::KSysGuard => &[
            "src/desktop/kde/ksysguard",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::Icu => &[
            "src/system/libraries/icu",
            "src/tools/mattos-build/src/stages/libraries.rs",
        ],
        BuildStage::LmSensors => &[
            "src/system/libraries/lm-sensors",
            "src/tools/mattos-build/src/stages/foundation_libraries.rs",
        ],
        BuildStage::Highway => &[
            "src/system/libraries/highway",
            "src/tools/mattos-build/src/stages/foundation_libraries.rs",
        ],
        BuildStage::PlasmaWorkspace => &[
            "src/desktop/kde/plasma-workspace",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::PlasmaDesktop => &[
            "src/desktop/kde/plasma-desktop",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::Breeze => &[
            "src/desktop/kde/breeze",
            "src/tools/mattos-build/src/stages/plasma.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KFileMetadata => &[
            "src/desktop/kde/kfilemetadata",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::KPty => &[
            "src/desktop/kde/kpty",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::NetworkManagerQt => &[
            "src/desktop/kde/networkmanager-qt",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::ModemManager => &[
            "src/system/services/modemmanager",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
            "src/tools/mattos-build/src/stages/helpers/meson.rs",
        ],
        BuildStage::ModemManagerQt => &[
            "src/desktop/kde/modemmanager-qt",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KPurpose => &[
            "src/desktop/kde/purpose",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::Milou => &[
            "src/desktop/kde/milou",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::SystemSettings => &[
            "src/desktop/kde/systemsettings",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::KSystemStats => &[
            "src/desktop/kde/ksystemstats",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::PlasmaSystemMonitor => &[
            "src/desktop/kde/plasma-systemmonitor",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::PolkitKdeAgent => &[
            "src/desktop/kde/polkit-kde-agent-1",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::KQuickImageEditor => &[
            "src/desktop/kde/kquickimageeditor",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
        BuildStage::KPipeWire => &[
            "src/desktop/kde/kpipewire",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::Spectacle => &[
            "src/desktop/kde/spectacle",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::PulseAudioQt => &[
            "src/desktop/kde/pulseaudio-qt",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::PlasmaPa => &[
            "src/desktop/kde/plasma-pa",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::PlasmaNm => &[
            "src/desktop/kde/plasma-nm",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::PowerDevil => &[
            "src/desktop/kde/powerdevil",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::XdgDesktopPortalKde => &[
            "src/desktop/kde/xdg-desktop-portal-kde",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::Dolphin => &[
            "src/desktop/kde/dolphin",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::Konsole => &[
            "src/desktop/kde/konsole",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::Kate => &[
            "src/desktop/kde/kate",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
        BuildStage::Ark => &[
            "src/desktop/kde/ark",
            "src/tools/mattos-build/src/stages/plasma_apps.rs",
        ],
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
            "src/system/graphics/libxfixes",
            "src/system/graphics/xcb-util",
            "src/system/graphics/xcb-renderutil",
            "src/system/graphics/xcb-image",
            "src/system/graphics/xcb-cursor",
            "src/system/graphics/xcb-util-wm",
            "src/system/graphics/xcb-keysyms",
            "src/system/graphics/xcb-util-m4",
        ],
        BuildStage::Libepoxy => &["src/system/graphics/libepoxy"],
        BuildStage::Freetype => &["src/system/libraries/freetype"],
        BuildStage::Fontconfig => &["src/system/libraries/fontconfig"],
        BuildStage::PopFonts => &[
            "src/desktop/fonts/pop-fonts",
            "src/tools/mattos-build/src/stages/desktop_support.rs",
        ],
        BuildStage::Libfontenc => &["src/system/graphics/libfontenc"],
        BuildStage::Libxfont => &["src/system/graphics/libxfont"],
        BuildStage::Libxcvt => &["src/system/graphics/libxcvt"],
        BuildStage::Lcms2 => &[
            "src/system/graphics/lcms2",
            "src/tools/mattos-build/src/stages/kde_foundation.rs",
        ],
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
        BuildStage::Polkit => &[
            "src/system/security/polkit",
            "src/tools/mattos-build/src/stages/system_services.rs",
        ],
        BuildStage::Duktape => &["src/system/security/duktape"],
        BuildStage::NetworkManager => &[
            "src/system/network/NetworkManager",
            "src/tools/mattos-build/src/stages/system_services.rs",
        ],
        BuildStage::Libnl => &[
            "src/system/network/libnl",
            "src/tools/mattos-build/src/stages/wifi.rs",
        ],
        BuildStage::WpaSupplicant => &[
            "src/system/network/hostap",
            "src/system/network/wpa-supplicant",
            "src/tools/mattos-build/src/stages/wifi.rs",
        ],
        BuildStage::Grub => &[
            "src/boot/grub/upstream",
            "src/build-support/grub-gnulib",
            "src/build-support/autoconf-archive",
            "src/desktop/fonts/open-sans/fonts/ttf/OpenSans-Regular.ttf",
            "src/desktop/fonts/open-sans/OFL.txt",
            "upstream/patches/grub",
            "src/tools/mattos-build/src/stages/grub.rs",
        ],
        BuildStage::Cozy => &[
            "src/userland/cozy",
            "src/tools/mattos-build/src/stages/desktop_support.rs",
        ],
        BuildStage::Flatpak => &[
            "src/system/packages/flatpak",
            "src/system/installer/flatpak-target-install.c",
        ],
        BuildStage::Bubblewrap => &["src/system/security/bubblewrap"],
        BuildStage::XdgDbusProxy => &["src/system/packages/xdg-dbus-proxy"],
        // GStreamer ships core and plugins-base in one immutable upstream
        // superproject; both stages deliberately fingerprint that one source
        // identity rather than downloading Meson wrap fallbacks.
        BuildStage::Gstreamer | BuildStage::GstreamerBase => &["src/system/multimedia/gstreamer"],
        BuildStage::XdgDesktopPortal => &[
            "src/system/packages/xdg-desktop-portal",
            "src/system/packages/xdg-desktop-portal-gvdb",
            "src/system/packages/xdg-desktop-portal-libglnx",
        ],
        BuildStage::Libarchive => &["src/system/libraries/libarchive"],
        BuildStage::Libxml2 => &["src/system/libraries/libxml2"],
        BuildStage::Libpng => &["src/system/libraries/libpng"],
        BuildStage::Fuse3 => &["src/system/libraries/fuse3"],
        BuildStage::Libfyaml => &[
            "src/system/libraries/libfyaml",
            "upstream/policies/release-archives.toml",
        ],
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
        BuildStage::Greetd => &["src/system/session/greetd"],
        BuildStage::Python => &[
            "src/development/python/cpython",
            "src/tools/mattos-build/src/stages/runtime_tooling.rs",
        ],
        BuildStage::Llvm => &[
            "src/toolchain/llvm-project",
            "src/tools/mattos-build/src/stages/runtime_tooling.rs",
        ],
        BuildStage::Rust => &[
            "src/toolchain/rust",
            "upstream/policies/release-archives.toml",
            "src/tools/mattos-build/src/stages/runtime_tooling.rs",
        ],
        BuildStage::Kmod => &["src/system/kmod"],
        BuildStage::Procps => &["src/userland/procps-ng"],
        BuildStage::Ncurses => &["src/system/terminal/ncurses"],
        BuildStage::Iproute2 => &[
            "src/userland/iproute2",
            "src/tools/mattos-build/src/stages/networking.rs",
        ],
        BuildStage::Iputils => &[
            "src/userland/iputils",
            "src/tools/mattos-build/src/stages/networking.rs",
        ],
        BuildStage::Curl => &[
            "src/userland/curl",
            "src/tools/mattos-build/src/stages/networking.rs",
        ],
        BuildStage::Expat => &[
            "src/system/libraries/expat/expat",
            "src/tools/mattos-build/src/stages/foundation_libraries.rs",
        ],
        BuildStage::Libcap => &[
            "src/system/libraries/libcap",
            "src/tools/mattos-build/src/stages/foundation_libraries.rs",
        ],
        BuildStage::Attr => &[
            "src/system/libraries/attr",
            "src/tools/mattos-build/src/stages/foundation_libraries.rs",
        ],
        BuildStage::Tar => &[
            "src/userland/tar",
            "src/build-support/paxutils",
            "src/build-support/gnulib",
            "src/tools/mattos-build/src/stages/archive_tools.rs",
        ],
        BuildStage::Acl => &[
            "src/system/libraries/acl",
            "src/tools/mattos-build/src/stages/foundation_libraries.rs",
        ],
        BuildStage::Zlib => &[
            "src/system/libraries/zlib",
            "src/tools/mattos-build/src/stages/foundation_libraries.rs",
        ],
        BuildStage::Bzip2 => &["src/system/libraries/bzip2"],
        BuildStage::Lz4 => &["src/system/libraries/lz4"],
        BuildStage::Xz => &["src/system/libraries/xz"],
        BuildStage::Xxhash => &["src/system/libraries/xxhash"],
        BuildStage::Zstd => &["src/system/libraries/zstd"],
        BuildStage::Dav1d => &[
            "src/system/multimedia/dav1d",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::Glib => &[
            "src/system/libraries/glib",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::Pipewire => &[
            "src/system/multimedia/pipewire",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::Ffmpeg => &[
            "src/system/multimedia/ffmpeg",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::Libva => &[
            "src/system/graphics/libva",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::OpenCv => &[
            "src/system/multimedia/opencv",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::ZxingCpp => &[
            "src/system/libraries/zxing-cpp",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::SndFile => &[
            "src/system/multimedia/libsndfile",
            "upstream/policies/release-archives.toml",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::PulseAudioClient => &[
            "src/system/multimedia/pulseaudio",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::LibGudev => &[
            "src/system/libraries/libgudev",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::Gmp => &[
            "src/system/libraries/gmp",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::Mpfr => &[
            "src/system/libraries/mpfr",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::LibBytesize => &[
            "src/system/libraries/libbytesize",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::Keyutils => &[
            "src/system/security/keyutils",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::LibNvme => &[
            "src/system/libraries/libnvme",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::Popt => &[
            "src/system/libraries/popt",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::JsonC => &[
            "src/system/libraries/json-c",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::E2fsprogs => &[
            "src/system/storage/e2fsprogs",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::DeviceMapper => &[
            "src/system/storage/lvm2",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::Cryptsetup => &[
            "src/system/storage/cryptsetup",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::LibBlockdev => &[
            "src/system/libraries/libblockdev",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::WirePlumber => &[
            "src/system/multimedia/wireplumber",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::UPower => &[
            "src/system/services/upower",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::UDisks2 => &[
            "src/system/services/udisks2",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::BlueZ => &[
            "src/system/services/bluez",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
        BuildStage::PowerProfilesDaemon => &[
            "src/system/services/power-profiles-daemon",
            "src/tools/mattos-build/src/stages/runtime_libraries.rs",
        ],
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
        BuildStage::Systemd => &[
            "src/system/systemd",
            "src/tools/mattos-build/src/stages/system_runtime.rs",
        ],
        BuildStage::Dbus => &[
            "src/system/dbus/dbus",
            "src/tools/mattos-build/src/stages/system_runtime.rs",
        ],
        BuildStage::DbusBroker => &[
            "src/system/dbus/dbus-broker",
            "upstream/patches/dbus-broker",
            "src/tools/mattos-build/src/stages/system_runtime.rs",
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
            "src/tools/mattos-build/src/stages/image.rs",
            "src/boot/module-loader.h",
            "src/system/storage/btrfs-progs",
            "src/system/storage/dosfstools",
            "src/system/storage/e2fsprogs",
            "src/system/data/linux-firmware",
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
        BuildStage::GccRuntime | BuildStage::GccToolchain | BuildStage::Binutils | BuildStage::Make
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
            | BuildStage::Libqrencode
    ) {
        inputs.push("src/tools/mattos-build/src/stages/libraries.rs".into());
    }
    if stage == BuildStage::Greetd {
        inputs.push("src/tools/mattos-build/src/stages/desktop.rs".into());
    }
    if matches!(
        stage,
        BuildStage::Libfyaml
            | BuildStage::Libxmlb
            | BuildStage::JsonGlib
            | BuildStage::Appstream
            | BuildStage::GdkPixbuf
            | BuildStage::Gpgme
            | BuildStage::Flatpak
            | BuildStage::Libarchive
            | BuildStage::Libxml2
            | BuildStage::Libpng
            | BuildStage::Fuse3
            | BuildStage::Ostree
            | BuildStage::Duktape
    ) {
        inputs.push("src/tools/mattos-build/src/stages/flatpak.rs".into());
    }
    if matches!(
        stage,
        BuildStage::Libseat
            | BuildStage::X11Compat
            | BuildStage::Libepoxy
            | BuildStage::Freetype
            | BuildStage::Fontconfig
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
            return vec!["src/system/installer/Cargo.toml".into()];
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
        | BuildStage::Ffmpeg
        | BuildStage::Libva
        | BuildStage::OpenCv
        | BuildStage::Dbus
        | BuildStage::Libseat
        | BuildStage::LibdisplayInfo
        | BuildStage::Libevdev
        | BuildStage::Libinput
        | BuildStage::Pixman
        | BuildStage::Libdrm
        | BuildStage::Libepoxy
        | BuildStage::Freetype
        | BuildStage::Fontconfig
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
        BuildStage::KCoreAddons
        | BuildStage::KI18n
        | BuildStage::KWidgetsAddons
        | BuildStage::PolkitQt6
        | BuildStage::YamlCpp
        | BuildStage::KPMCore => &[
            "gcc",
            "g++",
            "ld",
            "cmake",
            "ninja",
            "pkg-config",
            "python3",
            "msgfmt",
            "msgmerge",
        ],
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
        BuildStage::Flatpak | BuildStage::Greetd => &["cargo", "rustc", "gcc", "ld", "pkg-config"],
        BuildStage::Cozy => &["cargo", "rustc", "gcc", "ld"],
        BuildStage::Installer => &[
            "cargo",
            "rustc",
            "gcc",
            "ld",
            "autoreconf",
            "make",
            "cpio",
            "xz",
            "modinfo",
        ],
        BuildStage::Iso => &["xorriso"],
        BuildStage::Libnl => &[
            "autoreconf",
            "gcc",
            "ld",
            "make",
            "pkg-config",
            "flex",
            "bison",
        ],
        BuildStage::WpaSupplicant => &["gcc", "ld", "make", "pkg-config"],
        BuildStage::Grub => &[
            "autoreconf",
            "automake",
            "gettextize",
            "gcc",
            "ld",
            "make",
            "pkg-config",
            "flex",
            "bison",
            "python3",
            "patch",
        ],
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
        // Revision 3 preserves generated Duktape byte tables as Latin-1
        // bytes under the Python-3 generator adaptation.  This invalidates
        // previously published tables that were UTF-8 expanded and corrupt.
        BuildStage::Duktape => 3,
        BuildStage::Polkit => 2,
        BuildStage::LibdisplayInfo
        | BuildStage::Libevdev
        | BuildStage::Libinput
        | BuildStage::Pixman => 1,
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
        BuildStage::Iso => 3,
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
    fn image_stages_own_their_implementation_input() {
        let image_module = PathBuf::from("src/tools/mattos-build/src/stages/image.rs");
        for stage in [
            BuildStage::Installer,
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
    fn package_cache_implementation_is_not_a_stage_recipe_input() {
        let cache = PathBuf::from("src/tools/mattos-build/src/packaging/cache.rs");
        for stage in crate::stage_graph::build_plan(BuildStage::All) {
            assert!(
                !source_inputs(stage).contains(&cache),
                "package-cache mechanics must not become a blanket recipe input for {}",
                crate::stage_graph::stage_id(stage)
            );
        }
    }

    #[test]
    fn package_staging_implementation_is_not_a_blanket_stage_input() {
        let staging = PathBuf::from("src/tools/mattos-build/src/packaging/staging.rs");
        for stage in crate::stage_graph::build_plan(BuildStage::All) {
            assert!(
                !source_inputs(stage).contains(&staging),
                "package-payload mechanics must not become a blanket recipe input for {}",
                crate::stage_graph::stage_id(stage)
            );
        }
    }

    #[test]
    fn package_audit_implementation_is_not_a_blanket_stage_input() {
        let audit = PathBuf::from("src/tools/mattos-build/src/packaging/audit.rs");
        for stage in crate::stage_graph::build_plan(BuildStage::All) {
            assert!(
                !source_inputs(stage).contains(&audit),
                "package audit mechanics must not become a blanket recipe input for {}",
                crate::stage_graph::stage_id(stage)
            );
        }
    }

    #[test]
    fn runtime_tooling_recipe_change_does_not_fan_out_to_unrelated_stages() {
        let root = tempfile::tempdir().expect("temporary repository");
        let implementation = root
            .path()
            .join("src/tools/mattos-build/src/stages/runtime_tooling.rs");
        std::fs::create_dir_all(implementation.parent().expect("implementation parent"))
            .expect("create implementation parent");
        std::fs::write(&implementation, "original runtime tooling recipe\n")
            .expect("write implementation");
        let materialize = |input: &std::path::Path| {
            let path = root.path().join(input);
            if path.extension().is_some() {
                std::fs::create_dir_all(path.parent().expect("input parent"))
                    .expect("create input parent");
                std::fs::write(path, "source\n").expect("write input");
            } else {
                std::fs::create_dir_all(&path).expect("create input directory");
                std::fs::write(path.join("README"), "source\n").expect("write directory input");
            }
        };
        for input in source_inputs(BuildStage::Python) {
            if input
                != implementation
                    .strip_prefix(root.path())
                    .expect("relative implementation")
            {
                materialize(&input);
            }
        }
        for input in source_inputs(BuildStage::Bzip2) {
            materialize(&input);
        }
        let python_before = crate::performance::tracked_source_digest(
            root.path(),
            &source_inputs(BuildStage::Python),
            false,
        )
        .expect("python baseline");
        let bzip2_before = crate::performance::tracked_source_digest(
            root.path(),
            &source_inputs(BuildStage::Bzip2),
            false,
        )
        .expect("bzip2 baseline");
        std::fs::write(&implementation, "changed runtime tooling recipe\n")
            .expect("change implementation");
        assert_ne!(
            python_before,
            crate::performance::tracked_source_digest(
                root.path(),
                &source_inputs(BuildStage::Python),
                false,
            )
            .expect("changed python identity")
        );
        assert_eq!(
            bzip2_before,
            crate::performance::tracked_source_digest(
                root.path(),
                &source_inputs(BuildStage::Bzip2),
                false,
            )
            .expect("unrelated bzip2 identity")
        );
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
    fn release_archive_consumers_include_the_verified_policy() {
        for stage in [
            BuildStage::Gzip,
            BuildStage::Patch,
            BuildStage::Less,
            BuildStage::Rust,
            BuildStage::SndFile,
            BuildStage::Libfyaml,
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
    fn flatpak_recipe_identity_changes_only_for_its_owners() {
        let root = tempfile::tempdir().expect("temporary repository");
        let materialize = |path: &std::path::Path| {
            if path.extension().is_some() {
                std::fs::create_dir_all(path.parent().expect("input parent"))
                    .expect("create input parent");
                std::fs::write(path, "original\n").expect("write input");
            } else {
                std::fs::create_dir_all(path).expect("create input directory");
                std::fs::write(path.join("README"), "original\n").expect("write directory input");
            }
        };
        for input in source_inputs(BuildStage::Flatpak) {
            materialize(&root.path().join(input));
        }
        for input in source_inputs(BuildStage::Polkit) {
            materialize(&root.path().join(input));
        }
        let flatpak_inputs = source_inputs(BuildStage::Flatpak);
        let polkit_inputs = source_inputs(BuildStage::Polkit);
        let flatpak_before =
            crate::performance::tracked_source_digest(root.path(), &flatpak_inputs, false)
                .expect("flatpak baseline");
        let polkit_before =
            crate::performance::tracked_source_digest(root.path(), &polkit_inputs, false)
                .expect("polkit baseline");
        std::fs::write(
            root.path()
                .join("src/tools/mattos-build/src/stages/flatpak.rs"),
            "changed flatpak recipe\n",
        )
        .expect("change flatpak recipe");
        assert_ne!(
            flatpak_before,
            crate::performance::tracked_source_digest(root.path(), &flatpak_inputs, false)
                .expect("changed flatpak identity")
        );
        assert_eq!(
            polkit_before,
            crate::performance::tracked_source_digest(root.path(), &polkit_inputs, false)
                .expect("unrelated polkit identity")
        );
    }

    #[test]
    fn system_service_recipe_change_does_not_fan_out_to_unrelated_stages() {
        let root = tempfile::tempdir().expect("temporary repository");
        let implementation = root
            .path()
            .join("src/tools/mattos-build/src/stages/system_services.rs");
        std::fs::create_dir_all(implementation.parent().expect("implementation parent"))
            .expect("create implementation parent");
        std::fs::write(&implementation, "original system-service recipe\n")
            .expect("write implementation");
        let materialize = |input: &std::path::Path| {
            let path = root.path().join(input);
            if path.extension().is_some() {
                std::fs::create_dir_all(path.parent().expect("input parent"))
                    .expect("create input parent");
                std::fs::write(path, "source\n").expect("write input");
            } else {
                std::fs::create_dir_all(&path).expect("create input directory");
                std::fs::write(path.join("README"), "source\n").expect("write directory input");
            }
        };
        for input in source_inputs(BuildStage::Polkit) {
            if input
                != implementation
                    .strip_prefix(root.path())
                    .expect("relative implementation")
            {
                materialize(&input);
            }
        }
        for input in source_inputs(BuildStage::Flatpak) {
            materialize(&input);
        }
        let polkit_before = crate::performance::tracked_source_digest(
            root.path(),
            &source_inputs(BuildStage::Polkit),
            false,
        )
        .expect("polkit baseline");
        let flatpak_before = crate::performance::tracked_source_digest(
            root.path(),
            &source_inputs(BuildStage::Flatpak),
            false,
        )
        .expect("flatpak baseline");
        std::fs::write(&implementation, "changed system-service recipe\n")
            .expect("change implementation");
        assert_ne!(
            polkit_before,
            crate::performance::tracked_source_digest(
                root.path(),
                &source_inputs(BuildStage::Polkit),
                false,
            )
            .expect("changed polkit identity")
        );
        assert_eq!(
            flatpak_before,
            crate::performance::tracked_source_digest(
                root.path(),
                &source_inputs(BuildStage::Flatpak),
                false,
            )
            .expect("unrelated flatpak identity")
        );
    }

    #[test]
    fn system_runtime_recipe_change_does_not_fan_out_to_unrelated_stages() {
        let root = tempfile::tempdir().expect("temporary repository");
        let implementation = root
            .path()
            .join("src/tools/mattos-build/src/stages/system_runtime.rs");
        std::fs::create_dir_all(implementation.parent().expect("implementation parent"))
            .expect("create implementation parent");
        std::fs::write(&implementation, "original system-runtime recipe\n")
            .expect("write implementation");
        let materialize = |input: &std::path::Path| {
            let path = root.path().join(input);
            if path.extension().is_some() {
                std::fs::create_dir_all(path.parent().expect("input parent"))
                    .expect("create input parent");
                std::fs::write(path, "source\n").expect("write input");
            } else {
                std::fs::create_dir_all(&path).expect("create input directory");
                std::fs::write(path.join("README"), "source\n").expect("write directory input");
            }
        };
        for input in source_inputs(BuildStage::DbusBroker) {
            if input
                != implementation
                    .strip_prefix(root.path())
                    .expect("relative implementation")
            {
                materialize(&input);
            }
        }
        for input in source_inputs(BuildStage::Bzip2) {
            materialize(&input);
        }
        let runtime_inputs = source_inputs(BuildStage::DbusBroker);
        let bzip2_inputs = source_inputs(BuildStage::Bzip2);
        let runtime_before =
            crate::performance::tracked_source_digest(root.path(), &runtime_inputs, false)
                .expect("system-runtime baseline");
        let bzip2_before =
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("bzip2 baseline");
        std::fs::write(&implementation, "changed system-runtime recipe\n")
            .expect("change implementation");
        assert_ne!(
            runtime_before,
            crate::performance::tracked_source_digest(root.path(), &runtime_inputs, false)
                .expect("changed system-runtime identity")
        );
        assert_eq!(
            bzip2_before,
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("unrelated bzip2 identity")
        );
    }

    #[test]
    fn runtime_library_recipe_change_does_not_fan_out_to_unrelated_stages() {
        let root = tempfile::tempdir().expect("temporary repository");
        let implementation = root
            .path()
            .join("src/tools/mattos-build/src/stages/runtime_libraries.rs");
        std::fs::create_dir_all(implementation.parent().expect("implementation parent"))
            .expect("create implementation parent");
        std::fs::write(&implementation, "original runtime-library recipe\n")
            .expect("write implementation");
        let materialize = |input: &std::path::Path| {
            let path = root.path().join(input);
            if path.extension().is_some() {
                std::fs::create_dir_all(path.parent().expect("input parent"))
                    .expect("create input parent");
                std::fs::write(path, "source\n").expect("write input");
            } else {
                std::fs::create_dir_all(&path).expect("create input directory");
                std::fs::write(path.join("README"), "source\n").expect("write directory input");
            }
        };
        for input in source_inputs(BuildStage::Glib) {
            if input
                != implementation
                    .strip_prefix(root.path())
                    .expect("relative implementation")
            {
                materialize(&input);
            }
        }
        for input in source_inputs(BuildStage::Bzip2) {
            materialize(&input);
        }
        let glib_inputs = source_inputs(BuildStage::Glib);
        let bzip2_inputs = source_inputs(BuildStage::Bzip2);
        let glib_before =
            crate::performance::tracked_source_digest(root.path(), &glib_inputs, false)
                .expect("glib baseline");
        let bzip2_before =
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("bzip2 baseline");
        std::fs::write(&implementation, "changed runtime-library recipe\n")
            .expect("change implementation");
        assert_ne!(
            glib_before,
            crate::performance::tracked_source_digest(root.path(), &glib_inputs, false)
                .expect("changed glib identity")
        );
        assert_eq!(
            bzip2_before,
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("unrelated bzip2 identity")
        );
    }

    #[test]
    fn foundation_library_recipe_change_does_not_fan_out_to_unrelated_stages() {
        let root = tempfile::tempdir().expect("temporary repository");
        let implementation = root
            .path()
            .join("src/tools/mattos-build/src/stages/foundation_libraries.rs");
        std::fs::create_dir_all(implementation.parent().expect("implementation parent"))
            .expect("create implementation parent");
        std::fs::write(&implementation, "original foundation-library recipe\n")
            .expect("write implementation");
        let materialize = |input: &std::path::Path| {
            let path = root.path().join(input);
            if path.extension().is_some() {
                std::fs::create_dir_all(path.parent().expect("input parent"))
                    .expect("create input parent");
                std::fs::write(path, "source\n").expect("write input");
            } else {
                std::fs::create_dir_all(&path).expect("create input directory");
                std::fs::write(path.join("README"), "source\n").expect("write directory input");
            }
        };
        for input in source_inputs(BuildStage::Expat) {
            if input
                != implementation
                    .strip_prefix(root.path())
                    .expect("relative implementation")
            {
                materialize(&input);
            }
        }
        for input in source_inputs(BuildStage::Bzip2) {
            materialize(&input);
        }
        let expat_inputs = source_inputs(BuildStage::Expat);
        let bzip2_inputs = source_inputs(BuildStage::Bzip2);
        let expat_before =
            crate::performance::tracked_source_digest(root.path(), &expat_inputs, false)
                .expect("expat baseline");
        let bzip2_before =
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("bzip2 baseline");
        std::fs::write(&implementation, "changed foundation-library recipe\n")
            .expect("change implementation");
        assert_ne!(
            expat_before,
            crate::performance::tracked_source_digest(root.path(), &expat_inputs, false)
                .expect("changed expat identity")
        );
        assert_eq!(
            bzip2_before,
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("unrelated bzip2 identity")
        );
    }

    #[test]
    fn networking_recipe_change_does_not_fan_out_to_unrelated_stages() {
        let root = tempfile::tempdir().expect("temporary repository");
        let implementation = root
            .path()
            .join("src/tools/mattos-build/src/stages/networking.rs");
        std::fs::create_dir_all(implementation.parent().expect("implementation parent"))
            .expect("create implementation parent");
        std::fs::write(&implementation, "original networking recipe\n")
            .expect("write implementation");
        let materialize = |input: &std::path::Path| {
            let path = root.path().join(input);
            if path.extension().is_some() {
                std::fs::create_dir_all(path.parent().expect("input parent"))
                    .expect("create input parent");
                std::fs::write(path, "source\n").expect("write input");
            } else {
                std::fs::create_dir_all(&path).expect("create input directory");
                std::fs::write(path.join("README"), "source\n").expect("write directory input");
            }
        };
        for input in source_inputs(BuildStage::Curl) {
            if input
                != implementation
                    .strip_prefix(root.path())
                    .expect("relative implementation")
            {
                materialize(&input);
            }
        }
        for input in source_inputs(BuildStage::Bzip2) {
            materialize(&input);
        }
        let curl_inputs = source_inputs(BuildStage::Curl);
        let bzip2_inputs = source_inputs(BuildStage::Bzip2);
        let curl_before =
            crate::performance::tracked_source_digest(root.path(), &curl_inputs, false)
                .expect("curl baseline");
        let bzip2_before =
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("bzip2 baseline");
        std::fs::write(&implementation, "changed networking recipe\n")
            .expect("change implementation");
        assert_ne!(
            curl_before,
            crate::performance::tracked_source_digest(root.path(), &curl_inputs, false)
                .expect("changed curl identity")
        );
        assert_eq!(
            bzip2_before,
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("unrelated bzip2 identity")
        );
    }

    #[test]
    fn desktop_support_recipe_change_does_not_fan_out() {
        let root = tempfile::tempdir().expect("temporary repository");
        let implementation = root
            .path()
            .join("src/tools/mattos-build/src/stages/desktop_support.rs");
        std::fs::create_dir_all(implementation.parent().expect("implementation parent"))
            .expect("create implementation parent");
        std::fs::write(&implementation, "original desktop support recipe\n")
            .expect("write implementation");
        let materialize = |input: &std::path::Path| {
            let path = root.path().join(input);
            if path.extension().is_some() {
                std::fs::create_dir_all(path.parent().expect("input parent"))
                    .expect("create input parent");
                std::fs::write(path, "source\n").expect("write input");
            } else {
                std::fs::create_dir_all(&path).expect("create input directory");
                std::fs::write(path.join("README"), "source\n").expect("write directory input");
            }
        };
        for input in source_inputs(BuildStage::Cozy) {
            if input
                != implementation
                    .strip_prefix(root.path())
                    .expect("relative implementation")
            {
                materialize(&input);
            }
        }
        for input in source_inputs(BuildStage::Bzip2) {
            materialize(&input);
        }
        let cozy_inputs = source_inputs(BuildStage::Cozy);
        let bzip2_inputs = source_inputs(BuildStage::Bzip2);
        let cozy_before =
            crate::performance::tracked_source_digest(root.path(), &cozy_inputs, false)
                .expect("cozy baseline");
        let bzip2_before =
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("bzip2 baseline");
        std::fs::write(&implementation, "changed desktop support recipe\n")
            .expect("change implementation");
        assert_ne!(
            cozy_before,
            crate::performance::tracked_source_digest(root.path(), &cozy_inputs, false)
                .expect("changed cozy identity")
        );
        assert_eq!(
            bzip2_before,
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("unrelated bzip2 identity")
        );
    }

    #[test]
    fn archive_recipe_change_does_not_fan_out() {
        let root = tempfile::tempdir().expect("temporary repository");
        let implementation = root
            .path()
            .join("src/tools/mattos-build/src/stages/archive_tools.rs");
        std::fs::create_dir_all(implementation.parent().expect("implementation parent"))
            .expect("create implementation parent");
        std::fs::write(&implementation, "original archive recipe\n").expect("write implementation");
        let materialize = |input: &std::path::Path| {
            let path = root.path().join(input);
            if path.extension().is_some() {
                std::fs::create_dir_all(path.parent().expect("input parent"))
                    .expect("create input parent");
                std::fs::write(path, "source\n").expect("write input");
            } else {
                std::fs::create_dir_all(&path).expect("create input directory");
                std::fs::write(path.join("README"), "source\n").expect("write directory input");
            }
        };
        for input in source_inputs(BuildStage::Tar) {
            if input
                != implementation
                    .strip_prefix(root.path())
                    .expect("relative implementation")
            {
                materialize(&input);
            }
        }
        for input in source_inputs(BuildStage::Bzip2) {
            materialize(&input);
        }
        let tar_inputs = source_inputs(BuildStage::Tar);
        let bzip2_inputs = source_inputs(BuildStage::Bzip2);
        let tar_before = crate::performance::tracked_source_digest(root.path(), &tar_inputs, false)
            .expect("tar baseline");
        let bzip2_before =
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("bzip2 baseline");
        std::fs::write(&implementation, "changed archive recipe\n").expect("change implementation");
        assert_ne!(
            tar_before,
            crate::performance::tracked_source_digest(root.path(), &tar_inputs, false)
                .expect("changed tar identity")
        );
        assert_eq!(
            bzip2_before,
            crate::performance::tracked_source_digest(root.path(), &bzip2_inputs, false)
                .expect("unrelated bzip2 identity")
        );
    }
}
