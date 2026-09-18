use clap::ValueEnum;
#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, ValueEnum)]
pub(crate) enum BuildStage {
    Kernel,
    Glibc,
    GccRuntime,
    Binutils,
    GccToolchain,
    Make,
    Brush,
    Coreutils,
    Grep,
    Sed,
    Findutils,
    Diffutils,
    Gzip,
    Patch,
    File,
    Less,
    Git,
    Openssh,
    Libffi,
    QtBase,
    QtSvg,
    QtWayland,
    QtDeclarative,
    QtPositioning,
    QtLocation,
    QtShaderTools,
    QtTools,
    QtMultimedia,
    QtSpeech,
    QtCore5Compat,
    Qca,
    KCoreAddons,
    KI18n,
    KWidgetsAddons,
    KConfig,
    KConfigWidgets,
    KDbusAddons,
    KAuth,
    KArchive,
    KDecoration,
    KWayland,
    KNightTime,
    KHolidays,
    Libcanberra,
    Libqrencode,
    KirigamiPlatform,
    KirigamiAddons,
    KQuickCharts,
    KColorScheme,
    KCrash,
    KGlobalAccel,
    KGuiAddons,
    KIdleTime,
    KPackage,
    KService,
    QCoro,
    KSvg,
    KDEDeclarative,
    KIconThemes,
    BreezeIcons,
    KItemModels,
    KItemViews,
    KJobWidgets,
    KCMUtils,
    KDED,
    KIO,
    KUnitConversion,
    KSolid,
    KDocTools,
    KBookmarks,
    KCompletion,
    KCodecs,
    KNewStuff,
    KAttica,
    KNotifications,
    KParts,
    KXmlGui,
    KPrison,
    KRunner,
    KStatusNotifierItem,
    KTextEditor,
    KSyntaxHighlighting,
    KTextWidgets,
    KSonnet,
    KWallet,
    KWindowSystem,
    PlasmaWaylandProtocols,
    WaylandProtocols,
    PolkitQt6,
    YamlCpp,
    KPMCore,
    PlasmaKWin,
    PlasmaFramework,
    PlasmaActivities,
    KActivityManagerd,
    KGlobalAccelD,
    PlasmaActivitiesStats,
    Plasma5Support,
    LibKScreen,
    LayerShellQt,
    KScreenLocker,
    KSysGuard,
    Icu,
    PlasmaWorkspace,
    PlasmaDesktop,
    Breeze,
    Wayland,
    Xkbcommon,
    Libseat,
    LibdisplayInfo,
    Libevdev,
    Libinput,
    Pixman,
    Libdrm,
    VulkanHeaders,
    VulkanLoader,
    VulkanTools,
    X11Compat,
    Libepoxy,
    Freetype,
    Fontconfig,
    Libfontenc,
    Libxfont,
    Libxcvt,
    Lcms2,
    Libxshmfence,
    Libxkbfile,
    Xkbcomp,
    Libglvnd,
    Mesa,
    Xwayland,
    NvidiaDriver,
    CosmicComp,
    CosmicSession,
    CosmicGreeter,
    CosmicPanel,
    CosmicApplets,
    CosmicAppLibrary,
    CosmicLauncher,
    CosmicSettings,
    CosmicSettingsDaemon,
    CosmicNotifications,
    CosmicOsd,
    CosmicBg,
    CosmicIdle,
    CosmicWorkspaces,
    CosmicFiles,
    CosmicEdit,
    CosmicInitialSetup,
    CosmicTerm,
    CosmicTweaks,
    CosmicUtilities,
    CosmicRandr,
    CosmicScreenshot,
    PopLauncher,
    CosmicCalculator,
    CosmicStorage,
    CosmicMonitor,
    CosmicStore,
    Flatpak,
    Bubblewrap,
    XdgDbusProxy,
    Gstreamer,
    GstreamerBase,
    XdgDesktopPortal,
    Libarchive,
    Libxml2,
    Libpng,
    Fuse3,
    Libfyaml,
    Libxmlb,
    JsonGlib,
    Appstream,
    GdkPixbuf,
    Gpgme,
    Ostree,
    CosmicPortal,
    CosmicAssets,
    Greetd,
    CosmicDesktop,
    Cozy,
    Python,
    Llvm,
    Rust,
    Kmod,
    Procps,
    Ncurses,
    Iproute2,
    Iputils,
    Curl,
    Expat,
    Libcap,
    Attr,
    Tar,
    Acl,
    Zlib,
    Bzip2,
    Lz4,
    Xz,
    Xxhash,
    Zstd,
    Dav1d,
    Glib,
    Pipewire,
    Openssl,
    Elfutils,
    Pcre2,
    Selinux,
    Libxcrypt,
    Libmd,
    Libbsd,
    Libndp,
    Readline,
    Pam,
    Shadow,
    SudoRs,
    UtilLinux,
    Systemd,
    Dbus,
    DbusBroker,
    Dpkg,
    LibgpgError,
    Libgcrypt,
    Libassuan,
    Libksba,
    Npth,
    Gpgv,
    Polkit,
    Duktape,
    NetworkManager,
    Libnl,
    WpaSupplicant,
    Grub,
    Apt,
    Init,
    Installer,
    Rootfs,
    LiveRoot,
    Initramfs,
    Iso,
    All,
}

pub(crate) fn stage_id(stage: BuildStage) -> &'static str {
    match stage {
        BuildStage::Kernel => "linux",
        BuildStage::Glibc => "glibc",
        BuildStage::GccRuntime => "gcc-runtime",
        BuildStage::Binutils => "binutils",
        BuildStage::GccToolchain => "gcc-compiler",
        BuildStage::Make => "make",
        BuildStage::Brush => "brush",
        BuildStage::Coreutils => "coreutils",
        BuildStage::Grep => "grep",
        BuildStage::Sed => "sed",
        BuildStage::Findutils => "findutils",
        BuildStage::Diffutils => "diffutils",
        BuildStage::Gzip => "gzip",
        BuildStage::Patch => "patch",
        BuildStage::File => "file",
        BuildStage::Less => "less",
        BuildStage::Git => "git",
        BuildStage::Openssh => "openssh",
        BuildStage::Libffi => "libffi",
        BuildStage::QtBase => "qtbase",
        BuildStage::QtSvg => "qtsvg",
        BuildStage::QtWayland => "qtwayland",
        BuildStage::QtDeclarative => "qtdeclarative",
        BuildStage::QtPositioning => "qtpositioning",
        BuildStage::QtLocation => "qtlocation",
        BuildStage::QtShaderTools => "qtshadertools",
        BuildStage::QtTools => "qttools",
        BuildStage::QtMultimedia => "qtmultimedia",
        BuildStage::QtSpeech => "qtspeech",
        BuildStage::QtCore5Compat => "qt5compat",
        BuildStage::Qca => "qca",
        BuildStage::KCoreAddons => "kcoreaddons",
        BuildStage::KI18n => "ki18n",
        BuildStage::KWidgetsAddons => "kwidgetsaddons",
        BuildStage::KConfig => "kconfig",
        BuildStage::KConfigWidgets => "kconfigwidgets",
        BuildStage::KDbusAddons => "kdbusaddons",
        BuildStage::KAuth => "kauth",
        BuildStage::KArchive => "karchive",
        BuildStage::KDecoration => "kdecoration",
        BuildStage::KWayland => "kwayland",
        BuildStage::KNightTime => "knighttime",
        BuildStage::KHolidays => "kholidays",
        BuildStage::Libcanberra => "libcanberra",
        BuildStage::Libqrencode => "qrencode",
        BuildStage::KirigamiPlatform => "kirigami",
        BuildStage::KirigamiAddons => "kirigami-addons",
        BuildStage::KQuickCharts => "kquickcharts",
        BuildStage::KColorScheme => "kcolorscheme",
        BuildStage::KCrash => "kcrash",
        BuildStage::KGlobalAccel => "kglobalaccel",
        BuildStage::KGuiAddons => "kguiaddons",
        BuildStage::KIdleTime => "kidletime",
        BuildStage::KPackage => "kpackage",
        BuildStage::KService => "kservice",
        BuildStage::QCoro => "qcoro",
        BuildStage::KSvg => "ksvg",
        BuildStage::KDEDeclarative => "kdeclarative",
        BuildStage::KIconThemes => "kiconthemes",
        BuildStage::BreezeIcons => "breeze-icons",
        BuildStage::KItemModels => "kitemmodels",
        BuildStage::KItemViews => "kitemviews",
        BuildStage::KJobWidgets => "kjobwidgets",
        BuildStage::KCMUtils => "kcmutils",
        BuildStage::KDED => "kded",
        BuildStage::KIO => "kio",
        BuildStage::KUnitConversion => "kunitconversion",
        BuildStage::KSolid => "solid",
        BuildStage::KDocTools => "kdoctools",
        BuildStage::KBookmarks => "kbookmarks",
        BuildStage::KCompletion => "kcompletion",
        BuildStage::KCodecs => "kcodecs",
        BuildStage::KNewStuff => "knewstuff",
        BuildStage::KAttica => "attica",
        BuildStage::KNotifications => "knotifications",
        BuildStage::KParts => "kparts",
        BuildStage::KXmlGui => "kxmlgui",
        BuildStage::KPrison => "prison",
        BuildStage::KRunner => "krunner",
        BuildStage::KStatusNotifierItem => "kstatusnotifieritem",
        BuildStage::KTextEditor => "ktexteditor",
        BuildStage::KSyntaxHighlighting => "ksyntaxhighlighting",
        BuildStage::KTextWidgets => "ktextwidgets",
        BuildStage::KSonnet => "sonnet",
        BuildStage::KWallet => "kwallet",
        BuildStage::KWindowSystem => "kwindowsystem",
        BuildStage::PlasmaWaylandProtocols => "plasma-wayland-protocols",
        BuildStage::WaylandProtocols => "wayland-protocols",
        BuildStage::PolkitQt6 => "polkit-qt-1",
        BuildStage::YamlCpp => "yaml-cpp",
        BuildStage::KPMCore => "kpmcore",
        BuildStage::PlasmaKWin => "kwin",
        BuildStage::PlasmaFramework => "plasma-framework",
        BuildStage::PlasmaActivities => "plasma-activities",
        BuildStage::KActivityManagerd => "kactivitymanagerd",
        BuildStage::KGlobalAccelD => "kglobalacceld",
        BuildStage::PlasmaActivitiesStats => "plasma-activities-stats",
        BuildStage::Plasma5Support => "plasma5support",
        BuildStage::LibKScreen => "libkscreen",
        BuildStage::LayerShellQt => "layer-shell-qt",
        BuildStage::KScreenLocker => "kscreenlocker",
        BuildStage::KSysGuard => "ksysguard",
        BuildStage::Icu => "icu",
        BuildStage::PlasmaWorkspace => "plasma-workspace",
        BuildStage::PlasmaDesktop => "plasma-desktop",
        BuildStage::Breeze => "breeze",
        BuildStage::Wayland => "wayland",
        BuildStage::Xkbcommon => "xkbcommon",
        BuildStage::Libseat => "seatd",
        BuildStage::LibdisplayInfo => "libdisplay-info",
        BuildStage::Libevdev => "libevdev",
        BuildStage::Libinput => "libinput",
        BuildStage::Pixman => "pixman",
        BuildStage::Libdrm => "libdrm",
        BuildStage::VulkanHeaders => "vulkan-headers",
        BuildStage::VulkanLoader => "vulkan-loader",
        BuildStage::VulkanTools => "vulkan-tools",
        BuildStage::X11Compat => "x11-compat",
        BuildStage::Libepoxy => "libepoxy",
        BuildStage::Freetype => "freetype",
        BuildStage::Fontconfig => "fontconfig",
        BuildStage::Libfontenc => "libfontenc",
        BuildStage::Libxfont => "libxfont",
        BuildStage::Libxcvt => "libxcvt",
        BuildStage::Lcms2 => "lcms2",
        BuildStage::Libxshmfence => "libxshmfence",
        BuildStage::Libxkbfile => "libxkbfile",
        BuildStage::Xkbcomp => "xkbcomp",
        BuildStage::Libglvnd => "libglvnd",
        BuildStage::Mesa => "mesa",
        BuildStage::Xwayland => "xwayland",
        BuildStage::NvidiaDriver => "nvidia-driver",
        BuildStage::CosmicComp => "cosmic-comp",
        BuildStage::CosmicSession => "cosmic-session",
        BuildStage::CosmicGreeter => "cosmic-greeter",
        BuildStage::CosmicPanel => "cosmic-panel",
        BuildStage::CosmicApplets => "cosmic-applets",
        BuildStage::CosmicAppLibrary => "cosmic-applibrary",
        BuildStage::CosmicLauncher => "cosmic-launcher",
        BuildStage::CosmicSettings => "cosmic-settings",
        BuildStage::CosmicSettingsDaemon => "cosmic-settings-daemon",
        BuildStage::CosmicNotifications => "cosmic-notifications",
        BuildStage::CosmicOsd => "cosmic-osd",
        BuildStage::CosmicBg => "cosmic-bg",
        BuildStage::CosmicIdle => "cosmic-idle",
        BuildStage::CosmicWorkspaces => "cosmic-workspaces",
        BuildStage::CosmicFiles => "cosmic-files",
        BuildStage::CosmicEdit => "cosmic-edit",
        BuildStage::CosmicInitialSetup => "cosmic-initial-setup",
        BuildStage::CosmicTerm => "cosmic-term",
        BuildStage::CosmicTweaks => "cosmic-tweaks",
        BuildStage::CosmicUtilities => "cosmic-utilities",
        BuildStage::CosmicRandr => "cosmic-randr",
        BuildStage::CosmicScreenshot => "cosmic-screenshot",
        BuildStage::PopLauncher => "pop-launcher",
        BuildStage::CosmicCalculator => "cosmic-calculator",
        BuildStage::CosmicStorage => "cosmic-storage",
        BuildStage::CosmicMonitor => "cosmic-monitor",
        BuildStage::CosmicStore => "cosmic-store",
        BuildStage::Flatpak => "flatpak",
        BuildStage::Bubblewrap => "bubblewrap",
        BuildStage::XdgDbusProxy => "xdg-dbus-proxy",
        BuildStage::Gstreamer => "gstreamer",
        BuildStage::GstreamerBase => "gstreamer-base",
        BuildStage::XdgDesktopPortal => "xdg-desktop-portal",
        BuildStage::Libarchive => "libarchive",
        BuildStage::Libxml2 => "libxml2",
        BuildStage::Libpng => "libpng",
        BuildStage::Fuse3 => "fuse3",
        BuildStage::Libfyaml => "libfyaml",
        BuildStage::Libxmlb => "libxmlb",
        BuildStage::JsonGlib => "json-glib",
        BuildStage::Appstream => "appstream",
        BuildStage::GdkPixbuf => "gdk-pixbuf",
        BuildStage::Gpgme => "gpgme",
        BuildStage::Ostree => "ostree",
        BuildStage::CosmicPortal => "cosmic-portal",
        BuildStage::CosmicAssets => "cosmic-assets",
        BuildStage::Greetd => "greetd",
        BuildStage::CosmicDesktop => "cosmic-desktop",
        BuildStage::Cozy => "cozy",
        BuildStage::Python => "cpython",
        BuildStage::Llvm => "llvm",
        BuildStage::Rust => "rust",
        BuildStage::Kmod => "kmod",
        BuildStage::Procps => "procps-ng",
        BuildStage::Ncurses => "ncurses",
        BuildStage::Iproute2 => "iproute2",
        BuildStage::Iputils => "iputils",
        BuildStage::Curl => "curl",
        BuildStage::Expat => "expat",
        BuildStage::Libcap => "libcap",
        BuildStage::Attr => "attr",
        BuildStage::Tar => "tar",
        BuildStage::Acl => "acl",
        BuildStage::Zlib => "zlib",
        BuildStage::Bzip2 => "bzip2",
        BuildStage::Lz4 => "lz4",
        BuildStage::Xz => "xz",
        BuildStage::Xxhash => "xxhash",
        BuildStage::Zstd => "zstd",
        BuildStage::Dav1d => "dav1d",
        BuildStage::Glib => "glib",
        BuildStage::Pipewire => "pipewire",
        BuildStage::Openssl => "openssl",
        BuildStage::Elfutils => "elfutils",
        BuildStage::Pcre2 => "pcre2",
        BuildStage::Selinux => "selinux",
        BuildStage::Libxcrypt => "libxcrypt",
        BuildStage::Libmd => "libmd",
        BuildStage::Libbsd => "libbsd",
        BuildStage::Libndp => "libndp",
        BuildStage::Readline => "readline",
        BuildStage::Pam => "linux-pam",
        BuildStage::Shadow => "shadow",
        BuildStage::SudoRs => "sudo-rs",
        BuildStage::UtilLinux => "util-linux",
        BuildStage::Systemd => "systemd",
        BuildStage::Dbus => "dbus",
        BuildStage::DbusBroker => "dbus-broker",
        BuildStage::Dpkg => "dpkg",
        BuildStage::LibgpgError => "libgpg-error",
        BuildStage::Libgcrypt => "libgcrypt",
        BuildStage::Libassuan => "libassuan",
        BuildStage::Libksba => "libksba",
        BuildStage::Npth => "npth",
        BuildStage::Gpgv => "gpgv",
        BuildStage::Polkit => "polkit",
        BuildStage::Duktape => "duktape",
        BuildStage::NetworkManager => "networkmanager",
        BuildStage::Libnl => "libnl",
        BuildStage::WpaSupplicant => "wpa-supplicant",
        BuildStage::Grub => "grub",
        BuildStage::Apt => "apt",
        BuildStage::Init => "init",
        BuildStage::Installer => "installer",
        BuildStage::Rootfs => "rootfs",
        BuildStage::LiveRoot => "live-root",
        BuildStage::Initramfs => "initramfs",
        BuildStage::Iso => "iso",
        BuildStage::All => "all",
    }
}

pub(crate) fn direct_dependencies(stage: BuildStage) -> &'static [&'static str] {
    match stage {
        BuildStage::Kernel | BuildStage::Glibc | BuildStage::All => &[],
        BuildStage::GccRuntime => &["glibc", "linux-headers"],
        BuildStage::Binutils => &["gcc-runtime"],
        BuildStage::GccToolchain => &["binutils", "gcc-runtime"],
        BuildStage::Make => &["gcc-compiler", "binutils", "gcc-runtime"],
        BuildStage::Acl => &["formal-sysroot", "attr"],
        BuildStage::Openssl | BuildStage::Elfutils => &["formal-sysroot", "zlib", "zstd"],
        BuildStage::Selinux => &["formal-sysroot", "pcre2"],
        BuildStage::Libbsd => &["formal-sysroot", "libmd"],
        BuildStage::Libndp => &["formal-sysroot"],
        BuildStage::Readline => &["formal-sysroot", "ncurses"],
        BuildStage::Tar => &["formal-sysroot", "acl", "attr"],
        BuildStage::File => &["formal-sysroot", "zlib"],
        BuildStage::Less => &["formal-sysroot", "ncurses", "pcre2"],
        BuildStage::Git => &[
            "formal-sysroot",
            "curl",
            "expat",
            "openssl",
            "zlib",
            "zstd",
            "pcre2",
        ],
        BuildStage::Openssh => &[
            "formal-sysroot",
            "openssl",
            "zlib",
            "zstd",
            "linux-pam",
            "libxcrypt",
        ],
        BuildStage::Libffi => &["formal-sysroot"],
        // QtBase consumes only target-owned graphics and text runtime ABI.
        // QtGui's public Wayland and wayland-scanner features are generated
        // by QtBase itself.  QtWayland cannot retroactively enable them, so
        // both owned Wayland ABI outputs are direct QtBase inputs.
        BuildStage::QtBase => &["formal-sysroot", "libglvnd", "mesa", "wayland", "xkbcommon", "x11-compat", "freetype", "fontconfig", "expat", "libpng", "openssl", "zlib"],
        // QtSvg's standalone private-target bridge explicitly resolves the
        // staged xkbcommon headers/library referenced by QtGuiPrivate.
        BuildStage::QtSvg => &["formal-sysroot", "qtbase", "xkbcommon"],
        BuildStage::QtWayland => &["formal-sysroot", "qtbase", "wayland", "xkbcommon", "mesa", "libglvnd"],
        BuildStage::QtDeclarative => &["formal-sysroot", "qtbase", "qtshadertools", "wayland", "xkbcommon", "mesa", "libglvnd"],
        // QtPositioningQuick includes QtQuick private headers and therefore
        // must not race QtDeclarative or consume a stale prior deployment.
        BuildStage::QtPositioning => &["formal-sysroot", "qtbase", "qtdeclarative"],
        BuildStage::QtLocation => &["formal-sysroot", "qtbase", "qtdeclarative", "qtpositioning"],
        BuildStage::QtShaderTools => &["formal-sysroot", "qtbase"],
        BuildStage::QtTools => &["formal-sysroot", "qtbase", "qtdeclarative", "qtsvg"],
        BuildStage::QtMultimedia => &["formal-sysroot", "qtbase", "qtshadertools"],
        BuildStage::QtSpeech => &["formal-sysroot", "qtbase", "qtmultimedia"],
        BuildStage::QtCore5Compat => &["formal-sysroot", "qtbase", "qtdeclarative", "qtshadertools"],
        BuildStage::Qca => &["formal-sysroot", "qtbase", "qt5compat"],
        BuildStage::KCoreAddons => &["formal-sysroot", "qtbase", "systemd", "util-linux"],
        // iso-codes is runtime translation data; KI18n's compiled ABI does
        // not consume it at build time.
        BuildStage::KI18n => &["formal-sysroot", "qtbase", "qtdeclarative"],
        BuildStage::KWidgetsAddons => &["formal-sysroot", "qtbase"],
        BuildStage::KConfig => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons"],
        BuildStage::KConfigWidgets => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons", "kcodecs", "kconfig", "kguiaddons", "ki18n", "kwidgetsaddons", "kcolorscheme", "xkbcommon"],
        BuildStage::KDbusAddons => &["formal-sysroot", "qtbase", "xkbcommon"],
        BuildStage::KAuth => &["formal-sysroot", "qtbase", "kcoreaddons"],
        BuildStage::KArchive => &["formal-sysroot", "qtbase", "kcoreaddons", "zlib", "zstd", "bzip2", "xz", "openssl"],
        BuildStage::KDecoration => &["formal-sysroot", "qtbase", "ki18n"],
        BuildStage::KWayland => &["formal-sysroot", "qtbase", "qtwayland", "qtdeclarative", "wayland", "wayland-protocols", "plasma-wayland-protocols", "xkbcommon"],
        BuildStage::KNightTime => &["formal-sysroot", "qtbase", "qtdeclarative", "qtpositioning", "kconfig", "kcoreaddons", "kdbusaddons", "ki18n", "kholidays"],
        BuildStage::KHolidays => &["formal-sysroot", "qtbase", "kcoreaddons", "ki18n"],
        BuildStage::Libcanberra => &["formal-sysroot", "glib"],
        BuildStage::Libqrencode => &["formal-sysroot", "zlib"],
        BuildStage::KirigamiPlatform => &["formal-sysroot", "qtbase", "qtdeclarative", "qtshadertools", "kcoreaddons", "ki18n", "xkbcommon"],
        BuildStage::KirigamiAddons => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "kguiaddons", "ki18n", "kglobalaccel", "kirigami"],
        BuildStage::KQuickCharts => &["formal-sysroot", "qtbase", "qtdeclarative", "qtshadertools"],
        BuildStage::KColorScheme => &["formal-sysroot", "qtbase", "kconfig", "kguiaddons", "ki18n"],
        BuildStage::KCrash => &["formal-sysroot", "qtbase", "kcoreaddons", "kdbusaddons"],
        BuildStage::KGlobalAccel => &["formal-sysroot", "qtbase", "kconfig", "kcoreaddons", "kdbusaddons", "kwidgetsaddons", "xkbcommon"],
        BuildStage::KGuiAddons => &["formal-sysroot", "qtbase", "xkbcommon", "wayland", "plasma-wayland-protocols"],
        BuildStage::KIdleTime => &["formal-sysroot", "qtbase", "kcoreaddons"],
        BuildStage::KPackage => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons", "ki18n", "karchive"],
        BuildStage::KService => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n"],
        BuildStage::QCoro => &["formal-sysroot", "qtbase", "qtdeclarative"],
        BuildStage::KSvg => &["formal-sysroot", "qtbase", "qtdeclarative", "karchive", "kconfig", "kcolorscheme", "kcoreaddons", "kguiaddons", "kirigami"],
        BuildStage::KDEDeclarative => &["formal-sysroot", "qtbase", "qtdeclarative", "qtshadertools", "qttools", "kconfig", "kcoreaddons", "kguiaddons", "ki18n", "kglobalaccel", "kwidgetsaddons", "kirigami", "ksvg"],
        BuildStage::KIconThemes => &["formal-sysroot", "qtbase", "qtdeclarative", "karchive", "kconfig", "kcoreaddons", "ki18n", "kwidgetsaddons", "kcolorscheme", "breeze-icons", "libffi"],
        BuildStage::BreezeIcons => &["formal-sysroot", "qtbase"],
        BuildStage::KItemModels => &["formal-sysroot", "qtbase", "qtdeclarative"],
        BuildStage::KItemViews => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "kwidgetsaddons"],
        BuildStage::KJobWidgets => &["formal-sysroot", "qtbase", "kcoreaddons", "ki18n", "knotifications", "kwidgetsaddons", "xkbcommon"],
        BuildStage::KCMUtils => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "kwidgetsaddons", "kitemmodels", "kpackage", "kio", "kwindowsystem", "karchive", "kauth", "kbookmarks", "kcolorscheme", "kcompletion", "kcrash", "kdbusaddons", "kguiaddons", "kiconthemes", "breeze-icons", "kitemviews", "kjobwidgets", "kservice", "solid", "util-linux", "kconfigwidgets", "kcodecs", "kxmlgui", "kirigami", "kglobalaccel", "libffi"],
        BuildStage::KDED => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons", "kdbusaddons", "kconfig", "kcrash", "ki18n", "kservice"],
        BuildStage::KIO => &["formal-sysroot", "qtbase", "qtdeclarative", "karchive", "kauth", "kbookmarks", "kcolorscheme", "kcompletion", "kconfig", "kcoreaddons", "kcrash", "kdbusaddons", "kguiaddons", "ki18n", "kiconthemes", "kitemmodels", "kitemviews", "kjobwidgets", "kservice", "kwidgetsaddons", "kwindowsystem", "solid", "util-linux"],
        BuildStage::KUnitConversion => &["formal-sysroot", "qtbase", "kconfig", "kcoreaddons", "ki18n"],
        BuildStage::KSolid => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons", "kconfig", "ki18n", "kdbusaddons", "systemd", "util-linux", "selinux"],
        BuildStage::KDocTools => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons", "ki18n", "karchive"],
        BuildStage::KBookmarks => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons", "kconfig", "ki18n", "kwidgetsaddons"],
        BuildStage::KCompletion => &["formal-sysroot", "qtbase", "kcodecs", "kcoreaddons", "kconfig", "ki18n", "kwidgetsaddons"],
        BuildStage::KCodecs => &["formal-sysroot", "qtbase", "kcoreaddons", "ki18n"],
        BuildStage::KNewStuff => &["formal-sysroot", "qtbase", "qtdeclarative", "karchive", "kauth", "kconfig", "kcoreaddons", "ki18n", "kio", "kpackage", "kservice", "kwidgetsaddons", "attica"],
        BuildStage::KAttica => &["formal-sysroot", "qtbase"],
        BuildStage::KNotifications => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons", "kconfig", "kdbusaddons", "ki18n", "libcanberra"],
        BuildStage::KParts => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "kservice", "kwidgetsaddons", "kio", "kwindowsystem", "kbookmarks", "kcompletion", "kitemviews", "karchive", "kauth", "kcolorscheme", "kcrash", "kdbusaddons", "kguiaddons", "kiconthemes", "kitemmodels", "kjobwidgets", "solid", "util-linux", "kconfigwidgets", "kxmlgui", "kcodecs"],
        BuildStage::KXmlGui => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons", "kitemviews", "kconfig", "kconfigwidgets", "kcodecs", "kguiaddons", "ki18n", "kiconthemes", "kwidgetsaddons", "kglobalaccel", "kcolorscheme"],
        BuildStage::KPrison => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons", "ki18n", "kwidgetsaddons", "qrencode"],
        BuildStage::KRunner => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons", "kconfig", "kservice", "ki18n", "kwindowsystem", "kitemmodels"],
        BuildStage::KStatusNotifierItem => &["formal-sysroot", "qtbase", "kcoreaddons", "kconfig", "kdbusaddons", "ki18n", "kwidgetsaddons", "kwindowsystem", "x11-compat"],
        BuildStage::KTextEditor => &["formal-sysroot", "qtbase", "qtdeclarative", "qtmultimedia", "qtspeech", "karchive", "kauth", "kcoreaddons", "kconfig", "kconfigwidgets", "kcodecs", "kglobalaccel", "kguiaddons", "ki18n", "kio", "kwindowsystem", "kbookmarks", "kcompletion", "kcrash", "kdbusaddons", "kiconthemes", "kitemviews", "kjobwidgets", "knotifications", "breeze-icons", "solid", "kparts", "kxmlgui", "sonnet", "ksyntaxhighlighting", "kcolorscheme", "kservice", "ktextwidgets", "kwidgetsaddons", "libcanberra", "libffi"],
        BuildStage::KSyntaxHighlighting => &["formal-sysroot", "qtbase"],
        BuildStage::KTextWidgets => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "kservice", "kwidgetsaddons", "kcompletion", "sonnet"],
        // Sonnet's UI build unconditionally generates its Qt Designer
        // collection plugin.  Qt6UiPlugin is provided by Qt Tools, so keep
        // that target-side build output ordered before Sonnet rather than
        // allowing a parallel build to observe an incomplete Qt view.
        BuildStage::KSonnet => &["formal-sysroot", "qtbase", "qtdeclarative", "qttools"],
        BuildStage::KWallet => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "kdbusaddons", "ki18n", "kwidgetsaddons", "kwindowsystem", "knotifications", "kcolorscheme", "kcrash", "libgcrypt"],
        BuildStage::KWindowSystem => &["formal-sysroot", "qtbase", "qtdeclarative", "qtwayland", "kcoreaddons", "kwidgetsaddons", "xkbcommon", "wayland", "wayland-protocols", "plasma-wayland-protocols", "x11-compat"],
        BuildStage::PlasmaWaylandProtocols => &["formal-sysroot"],
        BuildStage::WaylandProtocols => &["formal-sysroot"],
        BuildStage::PolkitQt6 => &["formal-sysroot", "qtbase", "polkit", "glib", "dbus", "zlib"],
        BuildStage::YamlCpp => &["formal-sysroot"],
        // KPMCore's external-command helper links Polkit-Qt directly.  The
        // latter has ELF requirements on polkit, GLib, D-Bus and zlib that
        // must be present in KPMCore's target link environment; relying on
        // host linker search paths here would hide an incomplete closure.
        BuildStage::KPMCore => &["formal-sysroot", "qtbase", "kcoreaddons", "ki18n", "kwidgetsaddons", "polkit-qt-1", "polkit", "glib", "dbus", "zlib", "libffi", "util-linux", "installer"],
        BuildStage::PlasmaKWin => &["formal-sysroot", "qtbase", "qt5compat", "qtwayland", "qtdeclarative", "qttools", "kconfig", "kcoreaddons", "kdbusaddons", "kauth", "karchive", "kcolorscheme", "kcrash", "kglobalaccel", "kguiaddons", "ki18n", "kidletime", "kpackage", "kservice", "ksvg", "kwidgetsaddons", "kwindowsystem", "kdecoration", "kwayland", "knighttime", "kholidays", "qtpositioning", "lcms2", "x11-compat", "libffi", "libevdev", "plasma-wayland-protocols", "wayland-protocols", "wayland", "xkbcommon", "libdrm", "mesa", "libinput", "libdisplay-info", "seatd", "libepoxy", "libxcvt", "libcanberra", "systemd", "dbus", "xwayland"],
        BuildStage::PlasmaFramework => &["formal-sysroot", "qtbase", "qtdeclarative", "qttools", "kconfig", "kcoreaddons", "ki18n", "kguiaddons", "kwidgetsaddons", "kiconthemes", "kirigami", "ksvg", "kpackage", "kglobalaccel", "kwindowsystem", "kwayland", "kio", "kbookmarks", "kcompletion", "solid", "kservice", "kcodecs", "kitemmodels", "kitemviews", "kjobwidgets", "karchive", "kauth", "kcrash", "kdbusaddons", "knotifications", "kcolorscheme", "plasma-wayland-protocols", "plasma-activities", "gzip", "x11-compat"],
        BuildStage::PlasmaActivities => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "kdbusaddons", "dbus"],
        BuildStage::KActivityManagerd => &["formal-sysroot", "qtbase", "qtdeclarative", "qttools", "kconfig", "kconfigwidgets", "kcodecs", "kcoreaddons", "kdbusaddons", "ki18n", "kcrash", "kglobalaccel", "kxmlgui", "kio", "kwindowsystem", "kbookmarks", "kcompletion", "kitemviews", "kjobwidgets", "solid", "kwidgetsaddons", "kservice", "kguiaddons", "kiconthemes", "kcolorscheme", "util-linux"],
        BuildStage::KGlobalAccelD => &["formal-sysroot", "qtbase", "qtdeclarative", "qttools", "kconfig", "kcoreaddons", "kcrash", "kdbusaddons", "kwindowsystem", "kglobalaccel", "kservice", "kio", "karchive", "kauth", "kbookmarks", "kcolorscheme", "kcompletion", "kguiaddons", "ki18n", "kiconthemes", "kitemmodels", "kitemviews", "kjobwidgets", "knotifications", "kwidgetsaddons", "solid", "util-linux", "xkbcommon", "libcanberra"],
        BuildStage::PlasmaActivitiesStats => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "ki18n", "plasma-activities"],
        BuildStage::Plasma5Support => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kcoreaddons", "kguiaddons", "ki18n", "knotifications", "solid", "kservice", "kidletime", "kio", "kunitconversion", "kwindowsystem", "kbookmarks", "kcompletion", "kitemviews", "kjobwidgets", "kwidgetsaddons", "kholidays", "ksysguard", "plasma-activities", "xkbcommon"],
        BuildStage::LibKScreen => &["formal-sysroot", "qtbase", "qtdeclarative", "qtwayland", "kconfig", "kcoreaddons", "ki18n", "kwidgetsaddons", "kwayland", "plasma-wayland-protocols", "wayland", "libdrm"],
        BuildStage::LayerShellQt => &["formal-sysroot", "qtbase", "qtwayland", "qtdeclarative", "kwayland", "plasma-wayland-protocols", "wayland", "xkbcommon", "libffi"],
        BuildStage::KScreenLocker => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kconfigwidgets", "kcolorscheme", "kcoreaddons", "kcrash", "kdbusaddons", "ki18n", "kpackage", "kcmutils", "kio", "kbookmarks", "kwidgetsaddons", "kwindowsystem", "kglobalaccel", "kidletime", "knotifications", "solid", "kxmlgui", "ksvg", "kguiaddons", "kcodecs", "karchive", "kcompletion", "kitemviews", "kitemmodels", "kjobwidgets", "kservice", "plasma-framework", "kirigami", "layer-shell-qt", "libkscreen", "linux-pam"],
        BuildStage::KSysGuard => &["formal-sysroot", "qtbase", "qtdeclarative", "kcoreaddons", "kconfig", "ki18n", "kdbusaddons", "kio", "kpackage", "kconfigwidgets", "kglobalaccel", "kiconthemes", "kwidgetsaddons", "kxmlgui", "kservice", "kitemmodels", "kitemviews", "knotifications", "kjobwidgets", "kauth", "knewstuff", "solid", "attica", "zlib", "libdrm", "libcap", "libnl"],
        BuildStage::Icu => &["formal-sysroot"],
        BuildStage::PlasmaWorkspace => &["formal-sysroot", "qtbase", "qtdeclarative", "qtshadertools", "qtpositioning", "qtlocation", "qcoro", "kconfig", "kcoreaddons", "kdbusaddons", "kauth", "karchive", "kcrash", "kglobalaccel", "kguiaddons", "ki18n", "kholidays", "kidletime", "kpackage", "ksvg", "kcolorscheme", "kwidgetsaddons", "kdeclarative", "kirigami", "kirigami-addons", "kquickcharts", "kiconthemes", "kitemmodels", "kitemviews", "kcmutils", "kded", "kio", "kwindowsystem", "kwayland", "libkscreen", "layer-shell-qt", "knighttime", "plasma-wayland-protocols", "xkbcommon", "kbookmarks", "kcompletion", "solid", "kservice", "kcodecs", "knewstuff", "attica", "knotifications", "kparts", "kjobwidgets", "prison", "krunner", "kstatusnotifieritem", "ktextwidgets", "sonnet", "ktexteditor", "kwallet", "kconfigwidgets", "kxmlgui", "breeze-icons", "libffi", "libcanberra", "zlib", "icu", "polkit-qt-1", "plasma-framework", "plasma-activities", "plasma-activities-stats", "kwin", "systemd", "dbus", "networkmanager", "pipewire", "x11-compat"],
        BuildStage::PlasmaDesktop => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kconfigwidgets", "kwidgetsaddons", "kitemviews", "kitemmodels", "kcompletion", "kcoreaddons", "ki18n", "kcolorscheme", "kguiaddons", "kiconthemes", "kwindowsystem", "kirigami", "kirigami-addons", "kdecoration", "kcmutils", "kxmlgui", "kglobalaccel", "karchive", "breeze-icons", "kbookmarks", "kjobwidgets", "kservice", "kparts", "solid", "kded", "plasma-framework", "plasma-activities", "plasma-activities-stats", "plasma5support", "kwin", "ksysguard", "qtshadertools", "libffi", "plasma-workspace"],
        BuildStage::Breeze => &["formal-sysroot", "qtbase", "qtdeclarative", "kconfig", "kconfigwidgets", "kwidgetsaddons", "kitemviews", "kitemmodels", "kcompletion", "kcoreaddons", "ki18n", "kcolorscheme", "kguiaddons", "kiconthemes", "kwindowsystem", "kirigami", "kdecoration", "kcmutils", "kxmlgui", "kglobalaccel", "karchive", "breeze-icons", "libffi", "plasma-desktop"],
        BuildStage::Wayland => &["formal-sysroot", "libffi"],
        BuildStage::Xkbcommon => &["formal-sysroot", "x11-compat"],
        // Unprivileged graphical sessions acquire DRM/input devices through
        // logind. The builtin libseat backend is only sufficient for the
        // privileged live-installer compositor.
        BuildStage::Libseat => &["formal-sysroot", "systemd"],
        BuildStage::Libevdev | BuildStage::Pixman => &["formal-sysroot"],
        BuildStage::LibdisplayInfo => &["formal-sysroot"],
        BuildStage::Libinput => &["formal-sysroot", "libevdev", "systemd"],
        BuildStage::Libdrm => &["formal-sysroot", "systemd"],
        BuildStage::VulkanHeaders => &["formal-sysroot"],
        BuildStage::VulkanLoader => &["formal-sysroot", "cpython", "vulkan-headers", "wayland"],
        BuildStage::VulkanTools => &[
            "formal-sysroot",
            "cpython",
            "libffi",
            "mesa",
            "vulkan-headers",
            "vulkan-loader",
            "wayland",
        ],
        BuildStage::X11Compat => &["formal-sysroot", "cpython", "expat"],
        // These are the concrete native interfaces required to build the
        // Xwayland server. They are deliberately separate from COSMIC desktop
        // composition so unrelated desktop applications never inherit them as
        // compile-time inputs.
        BuildStage::Libepoxy => &["formal-sysroot", "x11-compat", "libglvnd"],
        BuildStage::Freetype => &["formal-sysroot", "zlib"],
        BuildStage::Fontconfig => &["formal-sysroot", "expat", "freetype", "zlib"],
        // libfontenc's configure probe includes zlib.h; keep the target-owned
        // zlib development output in its explicit native build environment.
        BuildStage::Libfontenc => &["formal-sysroot", "x11-compat", "zlib"],
        BuildStage::Libxfont => &[
            "formal-sysroot",
            "x11-compat",
            "freetype",
            "libfontenc",
            "zlib",
        ],
        BuildStage::Libxcvt => &["formal-sysroot", "x11-compat"],
        BuildStage::Lcms2 => &["formal-sysroot", "zlib"],
        BuildStage::Libxshmfence => &["formal-sysroot", "x11-compat"],
        BuildStage::Libxkbfile => &["formal-sysroot", "x11-compat"],
        // xkbcomp is an Xwayland runtime helper, not a build dependency of
        // the Xwayland server itself.  Package composition carries it with
        // the server so a keyboard-map update cannot rebuild Xwayland.
        BuildStage::Xkbcomp => &["formal-sysroot", "x11-compat", "libxkbfile"],
        BuildStage::Libglvnd => &["formal-sysroot", "x11-compat"],
        BuildStage::Mesa => &[
            "formal-sysroot",
            "libdrm",
            "libdisplay-info",
            "elfutils",
            "libffi",
            "libglvnd",
            "llvm",
            "rust",
            "systemd",
            "vulkan-headers",
            "wayland",
            "zlib",
            "zstd",
        ],
        BuildStage::Xwayland => &[
            "formal-sysroot",
            "x11-compat",
            "pixman",
            "wayland",
            "libffi",
            "xkbcommon",
            "libxkbfile",
            "libxfont",
            "libfontenc",
            "freetype",
            "zlib",
            "libxcvt",
            "libxshmfence",
            "libepoxy",
            "libdrm",
            "libglvnd",
            "libmd",
            "mesa",
        ],
        BuildStage::NvidiaDriver => &["linux", "libglvnd", "x11-compat", "wayland", "libdrm"],
        BuildStage::CosmicSession => &["formal-sysroot"],
        // These are the native outputs consumed by the compositor's
        // pkg-config probes and link step. Runtime desktop membership is
        // represented by cosmic-desktop, not by these build edges.
        BuildStage::CosmicComp => &[
            "formal-sysroot",
            "seatd",
            "libdisplay-info",
            "libinput",
            "pixman",
            "mesa",
            "libdrm",
            "wayland",
            "xkbcommon",
            "systemd",
        ],
        // cosmic-greeter's locked winit graph enables udev, whose
        // libudev-sys build requires systemd's target-owned libudev.pc,
        // header, and library.  Keep this narrow build edge explicit; the
        // runtime desktop graph remains separate. Greeter's Debian metadata
        // also requires libinput-dev for its target-owned -linput link.
        BuildStage::CosmicGreeter => &[
            "formal-sysroot",
            "xkbcommon",
            "linux-pam",
            "systemd",
            "libinput",
        ],
        BuildStage::CosmicPanel => &["formal-sysroot", "xkbcommon"],
        // The applet aggregate links its input/workspaces code against
        // libinput in addition to the shared xkbcommon metadata.
        BuildStage::CosmicApplets => {
            &["formal-sysroot", "dbus", "systemd", "xkbcommon", "libinput"]
        }
        BuildStage::CosmicAppLibrary => &["formal-sysroot", "xkbcommon"],
        BuildStage::CosmicLauncher => &["formal-sysroot", "xkbcommon"],
        // The display/input pages link libudev and libinput directly, so these
        // target-owned native providers must be exposed to Cargo's final link.
        BuildStage::CosmicSettings => &[
            "formal-sysroot",
            "dav1d",
            "xkbcommon",
            "systemd",
            "libinput",
        ],
        // smithay-client-toolkit's build script probes xkbcommon.pc directly.
        BuildStage::CosmicSettingsDaemon => &[
            "formal-sysroot",
            "pipewire",
            "systemd",
            "xkbcommon",
            "openssl",
            "libinput",
        ],
        BuildStage::CosmicNotifications => &["formal-sysroot", "xkbcommon"],
        // cosmic-osd's locked winit graph enables udev; libudev-sys must
        // resolve systemd's target-owned libudev.pc, and its input graph
        // links the target-owned libinput development output.
        BuildStage::CosmicOsd => &["formal-sysroot", "xkbcommon", "systemd", "libinput"],
        // cosmic-bg's locked libcosmic/winit graph builds
        // smithay-client-toolkit, whose build script resolves xkbcommon via
        // pkg-config.  The target-owned development metadata is therefore a
        // real compile dependency alongside dav1d.
        BuildStage::CosmicBg => &["formal-sysroot", "dav1d", "xkbcommon"],
        BuildStage::CosmicIdle => &["formal-sysroot"],
        // cosmic-workspaces declares libudev-dev and libinput-dev.  These
        // are build-time native inputs, not runtime desktop membership.
        BuildStage::CosmicWorkspaces => {
            &["formal-sysroot", "mesa", "xkbcommon", "systemd", "libinput"]
        }
        // COSMIC Files links through gio.  gio-2.0.pc itself requires zlib.pc,
        // so zlib is a real native build input rather than a desktop-runtime
        // co-member.
        BuildStage::CosmicFiles => &["formal-sysroot", "glib", "xkbcommon", "zlib"],
        BuildStage::CosmicTerm => &["formal-sysroot", "xkbcommon"],
        BuildStage::CosmicTweaks => &["formal-sysroot", "xkbcommon"],
        // Third-party COSMIC applications are built in this aggregate stage,
        // but still receive the same output-owned Rust/COSMIC and Wayland
        // inputs as the first-party applications.
        // cosmic-ext-storage links the target-owned libbtrfsutil development
        // output produced as part of the installer stage's btrfs-progs build.
        // Compatibility aggregate only: leaf utilities compile in their own
        // stages so a Calculator, Storage, Monitor, or launcher change does
        // not rebuild its siblings.
        BuildStage::CosmicUtilities => &[
            "cosmic-randr",
            "cosmic-screenshot",
            "pop-launcher",
            "cosmic-calculator",
            "cosmic-storage",
            "cosmic-monitor",
        ],
        BuildStage::CosmicRandr
        | BuildStage::CosmicScreenshot
        | BuildStage::PopLauncher
        | BuildStage::CosmicCalculator
        | BuildStage::CosmicMonitor => {
            &["formal-sysroot", "rust", "xkbcommon", "glib", "zlib"]
        }
        // cosmic-ext-storage links target-owned libbtrfsutil from installer.
        BuildStage::CosmicStorage => &[
            "formal-sysroot",
            "rust",
            "xkbcommon",
            "installer",
            "glib",
            "zlib",
        ],
        // Store alone consumes the Flatpak/OSTree native stack. Keeping it
        // separate prevents a Store or Flatpak update from invalidating the
        // unrelated COSMIC utility applications.
        BuildStage::CosmicStore => &[
            "formal-sysroot",
            "rust",
            "xkbcommon",
            "glib",
            "zlib",
            "flatpak",
            "polkit",
            "ostree",
            "xz",
            "libarchive",
            "gpgme",
            "json-glib",
            "curl",
            "systemd",
            "libxml2",
            "zstd",
            "libgpg-error",
            "libassuan",
            "openssl",
        ],
        BuildStage::Flatpak => &[
            "formal-sysroot",
            "glib",
            "libffi",
            "pcre2",
            "zlib",
            "xz",
            "curl",
            "openssl",
            "libcap",
            "libarchive",
            "bzip2",
            "lz4",
            "libxml2",
            "fuse3",
            "ostree",
            "libfyaml",
            "libxmlb",
            "systemd",
            "dbus",
            "gpgv",
            "zstd",
            "wayland",
            "xkbcommon",
            "libpng",
            "libbsd",
            "libmd",
            "libassuan",
            "libgcrypt",
            "libgpg-error",
            "libksba",
            // Meson discovers these through Flatpak's actual build graph.
            // They must be declared so its output cannot silently depend on
            // host metadata or wrap-built helpers.
            "appstream",
            "gdk-pixbuf",
            "gpgme",
            "polkit",
            "bubblewrap",
            "xdg-dbus-proxy",
        ],
        // Bubblewrap consumes Linux's seccomp syscall ABI directly but links
        // libcap for capability handling. Do not invent a libseccomp edge.
        BuildStage::Bubblewrap => &["formal-sysroot", "libcap"],
        // A standalone, target-owned D-Bus sandbox proxy.  Flatpak consumes
        // its published binary rather than a Meson wrap subproject.
        BuildStage::XdgDbusProxy => &["formal-sysroot", "glib", "libffi", "zlib"],
        BuildStage::Gstreamer => &[
            "formal-sysroot",
            "glib",
            "libffi",
            "zlib",
            "pcre2",
        ],
        // gio-2.0's declared pkg-config requirements include zlib, so the
        // plugins-base stage must receive it as a direct target input rather
        // than relying on any host pkg-config search path.
        BuildStage::GstreamerBase => &[
            "formal-sysroot",
            "glib",
            "libffi",
            "zlib",
            // The stage links helper binaries through GLib; carry GLib's
            // target PCRE2 ABI explicitly rather than consulting the host.
            "pcre2",
            "gstreamer",
        ],
        BuildStage::XdgDesktopPortal => &[
            "formal-sysroot",
            "glib",
            "libffi",
            "pcre2",
            "zlib",
            "json-glib",
            "fuse3",
            "gdk-pixbuf",
            // gdk-pixbuf-2.0 publishes libpng as a pkg-config requirement.
            "libpng",
            // gstreamer-pbutils-1.0 resolves its video/audio interfaces
            // through both the core and plugins-base pkg-config metadata.
            "gstreamer",
            "gstreamer-base",
            "pipewire",
            "systemd",
            "dbus",
            "flatpak",
            "polkit",
            "ostree",
            "xz",
            "curl",
            // flatpak.pc exposes libcurl's TLS backend; OpenSSL's target
            // pkg-config metadata must therefore be visible to Meson.
            "openssl",
            "gpgme",
            "libgpg-error",
            // gpgme.pc declares libassuan, which must resolve from MattOS's
            // staged target prefix rather than the host.
            "libassuan",
            // flatpak.pc explicitly publishes these private build
            // requirements; keep them target-owned and explicit.
            "libxml2",
            "zstd",
            "libarchive",
            "bubblewrap",
        ],
        BuildStage::Libarchive => &[
            "formal-sysroot",
            "zlib",
            "zstd",
            "bzip2",
            "xz",
            "lz4",
            "libcap",
        ],
        BuildStage::Libxml2 => &["formal-sysroot", "zlib", "expat"],
        BuildStage::Fuse3 => &["formal-sysroot"],
        BuildStage::Libfyaml => &["formal-sysroot"],
        BuildStage::Libxmlb => &["formal-sysroot", "glib", "libffi", "xz", "zlib"],
        BuildStage::JsonGlib => &["formal-sysroot", "glib", "libffi", "pcre2", "zlib"],
        BuildStage::Appstream => &[
            "formal-sysroot",
            "glib",
            "libffi",
            "libxml2",
            "zlib",
            "curl",
            "openssl",
            "libfyaml",
            "libxmlb",
            "xz",
            "zstd",
            "systemd",
            "wayland",
        ],
        BuildStage::GdkPixbuf => &[
            "formal-sysroot",
            "glib",
            "libffi",
            "zlib",
            "libpng",
            // gdk-pixbuf's helper executables link through GLib, whose
            // published target ABI needs libpcre2-8 at link time.
            "pcre2",
        ],
        BuildStage::Gpgme => &[
            "formal-sysroot",
            "libassuan",
            "libgcrypt",
            "libgpg-error",
            "libksba",
            "zlib",
        ],
        BuildStage::Ostree => &[
            "formal-sysroot",
            "glib",
            "libffi",
            "zlib",
            "bzip2",
            "lz4",
            "xz",
            "zstd",
            "curl",
            "openssl",
            "libarchive",
            "libxml2",
            "fuse3",
            // OSTree verifies Flatpak remote metadata through GPGME.  This
            // is a real native link/runtime requirement, not desktop
            // composition co-membership.
            "gpgme",
            // gpgme.pc has target-owned pkg-config requirements on these
            // interfaces while OSTree's configure probes GPGME.
            "libassuan",
            "libgpg-error",
            "gpgv",
            "libmd",
            "libbsd",
            "installer",
        ],
        BuildStage::CosmicPortal => &[
            "formal-sysroot",
            "mesa",
            "glib",
            "pipewire",
            "xkbcommon",
            "zlib",
        ],
        BuildStage::CosmicAssets => &[],
        BuildStage::CosmicDesktop => &[
            "cosmic-session",
            "cosmic-greeter",
            "cosmic-panel",
            "cosmic-applets",
            "cosmic-applibrary",
            "cosmic-launcher",
            "cosmic-settings",
            "cosmic-settings-daemon",
            "cosmic-notifications",
            "cosmic-osd",
            "cosmic-bg",
            "cosmic-idle",
            "cosmic-workspaces",
            "cosmic-files",
            "cosmic-term",
            "cosmic-tweaks",
            "cosmic-randr",
            "cosmic-screenshot",
            "pop-launcher",
            "cosmic-calculator",
            "cosmic-storage",
            "cosmic-monitor",
            "cosmic-store",
            "flatpak",
            "cosmic-portal",
            "cosmic-assets",
            "greetd",
            "cosmic-initial-setup",
            "networkmanager",
        ],
        // Initial Setup's locked libcosmic graph enables udev and libinput
        // through winit/libinput.  Those crates need systemd's target-owned
        // libudev metadata and libinput's target-owned link inputs.
        BuildStage::CosmicInitialSetup => &["formal-sysroot", "systemd", "xkbcommon", "libinput"],
        BuildStage::Polkit => &[
            "formal-sysroot",
            "glib",
            "zlib",
            "expat",
            "systemd",
            "dbus",
            "duktape",
            "linux-pam",
            "libffi",
        ],
        BuildStage::Duktape => &["formal-sysroot"],
        BuildStage::Libnl => &["formal-sysroot"],
        BuildStage::WpaSupplicant => &["formal-sysroot", "openssl", "libnl", "dbus"],
        BuildStage::Grub => &["formal-sysroot", "zlib", "xz", "freetype"],
        BuildStage::NetworkManager => &[
            "formal-sysroot",
            "glib",
            "systemd",
            "dbus",
            "polkit",
            "iproute2",
            "util-linux",
            "libndp",
            "zlib",
            "readline",
            "ncurses",
            "libffi",
        ],
        // gio-2.0.pc exposes GLib's zlib requirement; pkg-config resolves
        // that transitive metadata while compiling gio-sys.
        BuildStage::CosmicEdit => &["formal-sysroot", "glib", "zlib", "xkbcommon"],
        BuildStage::Greetd => &["formal-sysroot", "linux-pam"],
        BuildStage::Cozy => &["formal-sysroot", "glibc", "gcc-runtime"],
        BuildStage::Dav1d => &["formal-sysroot"],
        BuildStage::Glib => &["formal-sysroot", "libffi", "pcre2", "zlib"],
        BuildStage::Pipewire => &["formal-sysroot", "systemd", "dbus"],
        BuildStage::Python => &[
            "formal-sysroot",
            "libffi",
            "openssl",
            "zlib",
            "bzip2",
            "xz",
            "expat",
            "ncurses",
        ],
        BuildStage::Llvm => &["formal-sysroot", "zlib", "zstd"],
        BuildStage::Rust => &["formal-sysroot", "llvm", "openssl", "zlib"],
        BuildStage::Procps => &["formal-sysroot", "ncurses"],
        BuildStage::Kmod => &["formal-sysroot", "zstd"],
        BuildStage::Iproute2 => &[
            "formal-sysroot",
            "libcap",
            "zlib",
            "zstd",
            "elfutils",
            "pcre2",
            "selinux",
        ],
        BuildStage::Curl => &["formal-sysroot", "openssl", "zlib", "zstd"],
        BuildStage::Pam => &["formal-sysroot", "libxcrypt"],
        BuildStage::UtilLinux => &["formal-sysroot", "linux-pam", "selinux", "pcre2", "ncurses"],
        BuildStage::Shadow => &[
            "formal-sysroot",
            "linux-pam",
            "libbsd",
            "libmd",
            "libxcrypt",
        ],
        BuildStage::SudoRs => &["formal-sysroot", "linux-pam"],
        BuildStage::Systemd => &[
            "formal-sysroot",
            "dbus",
            "kmod",
            "util-linux",
            "linux-pam",
            "libcap",
            "openssl",
            "pcre2",
        ],
        BuildStage::Dbus => &["formal-sysroot", "expat"],
        BuildStage::DbusBroker => &["formal-sysroot", "systemd", "expat"],
        BuildStage::Dpkg => &[
            "formal-sysroot",
            "zlib",
            "bzip2",
            "xz",
            "zstd",
            "libmd",
            "selinux",
            "pcre2",
        ],
        BuildStage::LibgpgError | BuildStage::Npth => &["formal-sysroot"],
        BuildStage::Libgcrypt | BuildStage::Libassuan | BuildStage::Libksba => {
            &["formal-sysroot", "libgpg-error"]
        }
        BuildStage::Gpgv => &[
            "formal-sysroot",
            "libgpg-error",
            "libgcrypt",
            "libassuan",
            "libksba",
            "npth",
            "zlib",
        ],
        BuildStage::Apt => &[
            "formal-sysroot",
            "dpkg",
            "openssl",
            "zlib",
            "bzip2",
            "xz",
            "zstd",
            "systemd",
        ],
        BuildStage::Installer => &[
            "grub",
            "formal-sysroot",
            "util-linux",
            "zlib",
            "zstd",
            "linux",
            "wayland",
            "xkbcommon",
            "cosmic-comp",
        ],
        BuildStage::Rootfs => &[
            "apt",
            "dpkg",
            "systemd",
            "dbus-broker",
            "grep",
            "sed",
            "findutils",
            "diffutils",
            "init",
            "installer",
            "repository",
        ],
        BuildStage::LiveRoot => &["rootfs"],
        BuildStage::Initramfs => &["formal-sysroot", "linux"],
        BuildStage::Iso => &["linux", "live-root", "initramfs", "grub"],
        _ => &["formal-sysroot"],
    }
}

pub(crate) fn all_build_stages() -> &'static [BuildStage] {
    &[
        BuildStage::Kernel,
        BuildStage::Glibc,
        BuildStage::GccRuntime,
        BuildStage::Binutils,
        BuildStage::GccToolchain,
        BuildStage::Make,
        BuildStage::Brush,
        BuildStage::Coreutils,
        BuildStage::Grep,
        BuildStage::Sed,
        BuildStage::Findutils,
        BuildStage::Diffutils,
        BuildStage::Gzip,
        BuildStage::Patch,
        BuildStage::File,
        BuildStage::Less,
        BuildStage::Git,
        BuildStage::Openssh,
        BuildStage::Libffi,
        BuildStage::QtBase,
        BuildStage::QtSvg,
        BuildStage::QtWayland,
        BuildStage::QtDeclarative,
        BuildStage::QtPositioning,
        BuildStage::QtLocation,
        BuildStage::QtShaderTools,
        BuildStage::QtTools,
        BuildStage::QtMultimedia,
        BuildStage::QtSpeech,
        BuildStage::QtCore5Compat,
        BuildStage::Qca,
        BuildStage::KCoreAddons,
        BuildStage::KI18n,
        BuildStage::KWidgetsAddons,
        BuildStage::KConfig,
        BuildStage::KConfigWidgets,
        BuildStage::KDbusAddons,
        BuildStage::KAuth,
        BuildStage::KArchive,
        BuildStage::KDecoration,
        BuildStage::KWayland,
        BuildStage::KNightTime,
        BuildStage::KHolidays,
        BuildStage::Libcanberra,
        BuildStage::Libqrencode,
        BuildStage::KirigamiPlatform,
        BuildStage::KirigamiAddons,
        BuildStage::KQuickCharts,
        BuildStage::KColorScheme,
        BuildStage::KCrash,
        BuildStage::KGlobalAccel,
        BuildStage::KGuiAddons,
        BuildStage::KIdleTime,
        BuildStage::KPackage,
        BuildStage::KService,
        BuildStage::QCoro,
        BuildStage::KSvg,
        BuildStage::KDEDeclarative,
        BuildStage::KIconThemes,
        BuildStage::BreezeIcons,
        BuildStage::KItemModels,
        BuildStage::KItemViews,
        BuildStage::KJobWidgets,
        BuildStage::KCMUtils,
        BuildStage::KDED,
        BuildStage::KIO,
        BuildStage::KUnitConversion,
        BuildStage::KSolid,
        BuildStage::KBookmarks,
        BuildStage::KCompletion,
        BuildStage::KCodecs,
        BuildStage::KNewStuff,
        BuildStage::KAttica,
        BuildStage::KNotifications,
        BuildStage::KParts,
        BuildStage::KXmlGui,
        BuildStage::KPrison,
        BuildStage::KRunner,
        BuildStage::KStatusNotifierItem,
        BuildStage::KTextEditor,
        BuildStage::KSyntaxHighlighting,
        BuildStage::KTextWidgets,
        BuildStage::KSonnet,
        BuildStage::KWallet,
        BuildStage::KWindowSystem,
        BuildStage::PlasmaWaylandProtocols,
        BuildStage::WaylandProtocols,
        BuildStage::PolkitQt6,
        BuildStage::YamlCpp,
        BuildStage::KPMCore,
        BuildStage::PlasmaKWin,
        BuildStage::PlasmaFramework,
        BuildStage::PlasmaActivities,
        BuildStage::KActivityManagerd,
        BuildStage::KGlobalAccelD,
        BuildStage::PlasmaActivitiesStats,
        BuildStage::Plasma5Support,
        BuildStage::LibKScreen,
        BuildStage::LayerShellQt,
        BuildStage::KSysGuard,
        BuildStage::Icu,
        BuildStage::PlasmaWorkspace,
        BuildStage::PlasmaDesktop,
        BuildStage::Breeze,
        BuildStage::Wayland,
        BuildStage::Xkbcommon,
        BuildStage::Libseat,
        BuildStage::LibdisplayInfo,
        BuildStage::Libevdev,
        BuildStage::Libinput,
        BuildStage::Pixman,
        BuildStage::Libdrm,
        BuildStage::VulkanHeaders,
        BuildStage::VulkanLoader,
        BuildStage::X11Compat,
        BuildStage::Libepoxy,
        BuildStage::Freetype,
        BuildStage::Fontconfig,
        BuildStage::Libfontenc,
        BuildStage::Libxfont,
        BuildStage::Libxcvt,
        BuildStage::Lcms2,
        BuildStage::Libxshmfence,
        BuildStage::Libxkbfile,
        BuildStage::Xkbcomp,
        BuildStage::Libglvnd,
        BuildStage::Mesa,
        BuildStage::Xwayland,
        BuildStage::NvidiaDriver,
        BuildStage::VulkanTools,
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
        BuildStage::CosmicIdle,
        BuildStage::CosmicWorkspaces,
        BuildStage::CosmicFiles,
        BuildStage::CosmicEdit,
        BuildStage::CosmicInitialSetup,
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
        BuildStage::Flatpak,
        BuildStage::Bubblewrap,
        BuildStage::XdgDbusProxy,
        BuildStage::Gstreamer,
        BuildStage::GstreamerBase,
        BuildStage::XdgDesktopPortal,
        BuildStage::Libarchive,
        BuildStage::Libxml2,
        BuildStage::Libpng,
        BuildStage::Fuse3,
        BuildStage::Libfyaml,
        BuildStage::Libxmlb,
        BuildStage::JsonGlib,
        BuildStage::Appstream,
        BuildStage::GdkPixbuf,
        BuildStage::Gpgme,
        BuildStage::Ostree,
        BuildStage::CosmicPortal,
        BuildStage::CosmicAssets,
        BuildStage::Greetd,
        BuildStage::CosmicDesktop,
        BuildStage::Cozy,
        BuildStage::Python,
        BuildStage::Llvm,
        BuildStage::Rust,
        BuildStage::Expat,
        BuildStage::Libcap,
        BuildStage::Attr,
        BuildStage::Acl,
        BuildStage::Zlib,
        BuildStage::Bzip2,
        BuildStage::Lz4,
        BuildStage::Xz,
        BuildStage::Xxhash,
        BuildStage::Zstd,
        BuildStage::Dav1d,
        BuildStage::Glib,
        BuildStage::Pipewire,
        BuildStage::Openssl,
        BuildStage::Elfutils,
        BuildStage::Pcre2,
        BuildStage::Selinux,
        BuildStage::Libxcrypt,
        BuildStage::Libmd,
        BuildStage::Libbsd,
        BuildStage::Libndp,
        BuildStage::Readline,
        BuildStage::Tar,
        BuildStage::Ncurses,
        BuildStage::Procps,
        BuildStage::Iproute2,
        BuildStage::Iputils,
        BuildStage::Curl,
        BuildStage::Pam,
        BuildStage::UtilLinux,
        BuildStage::Kmod,
        BuildStage::Shadow,
        BuildStage::SudoRs,
        BuildStage::Systemd,
        BuildStage::Dbus,
        BuildStage::DbusBroker,
        BuildStage::Dpkg,
        BuildStage::LibgpgError,
        BuildStage::Libgcrypt,
        BuildStage::Libassuan,
        BuildStage::Libksba,
        BuildStage::Npth,
        BuildStage::Gpgv,
        BuildStage::Polkit,
        BuildStage::Duktape,
        BuildStage::NetworkManager,
        BuildStage::Libnl,
        BuildStage::WpaSupplicant,
        BuildStage::Grub,
        BuildStage::Apt,
        BuildStage::Init,
        BuildStage::Installer,
        BuildStage::Rootfs,
        BuildStage::LiveRoot,
        BuildStage::Initramfs,
        BuildStage::Iso,
    ]
}

pub(crate) fn build_plan(stage: BuildStage) -> Vec<BuildStage> {
    if stage == BuildStage::All {
        all_build_stages().to_vec()
    } else {
        vec![stage]
    }
}

#[cfg(test)]
pub(crate) fn dependency_map() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    let mut dependencies = all_build_stages()
        .iter()
        .map(|stage| {
            (
                stage_id(*stage),
                direct_dependencies(*stage).iter().copied().collect(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    dependencies.insert("linux-headers", ["glibc"].into_iter().collect());
    dependencies.insert(
        "formal-sysroot",
        ["linux-headers", "glibc", "gcc-runtime"]
            .into_iter()
            .collect(),
    );
    let package_producers = all_build_stages()
        .iter()
        .copied()
        .filter(|stage| {
            !matches!(
                stage,
                BuildStage::Kernel
                    | BuildStage::Rootfs
                    | BuildStage::LiveRoot
                    | BuildStage::Initramfs
                    | BuildStage::Iso
            )
        })
        .map(stage_id)
        .collect::<BTreeSet<_>>();
    dependencies.insert("packages", package_producers);
    dependencies.insert("repository", ["packages"].into_iter().collect());
    dependencies.entry("rootfs").or_default().insert("packages");
    dependencies
}

#[cfg(test)]
pub(crate) fn downstream_invalidation(changed_outputs: &[&'static str]) -> BTreeSet<&'static str> {
    let graph = dependency_map();
    let mut invalidated = changed_outputs.iter().copied().collect::<BTreeSet<_>>();
    let mut queue = changed_outputs.iter().copied().collect::<VecDeque<_>>();
    while let Some(changed) = queue.pop_front() {
        for (stage, dependencies) in &graph {
            if dependencies.contains(changed) && invalidated.insert(stage) {
                queue.push_back(stage);
            }
        }
    }
    invalidated
}

#[cfg(test)]
fn actual_rebuilds(
    direct_input_owners: &[&'static str],
    changed_outputs: &[&'static str],
) -> BTreeSet<&'static str> {
    let graph = dependency_map();
    let changed_outputs = changed_outputs.iter().copied().collect::<BTreeSet<_>>();
    let mut rebuilt = direct_input_owners.iter().copied().collect::<BTreeSet<_>>();
    let mut queue = direct_input_owners.iter().copied().collect::<VecDeque<_>>();
    while let Some(completed) = queue.pop_front() {
        if !changed_outputs.contains(completed) {
            continue;
        }
        for (stage, dependencies) in &graph {
            if dependencies.contains(completed) && rebuilt.insert(stage) {
                queue.push_back(stage);
            }
        }
    }
    rebuilt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_references_only_known_stages() {
        let graph = dependency_map();
        for (stage, dependencies) in &graph {
            for dependency in dependencies {
                assert!(
                    graph.contains_key(dependency),
                    "{stage} depends on unknown {dependency}"
                );
            }
        }
    }

    #[test]
    fn cosmic_leaf_edges_are_build_artifacts_not_runtime_membership() {
        let forbidden_for_leaves = ["cosmic-comp", "dbus-broker", "wayland"];
        for stage in [
            BuildStage::CosmicFiles,
            BuildStage::CosmicPanel,
            BuildStage::CosmicSettings,
            BuildStage::CosmicGreeter,
            BuildStage::CosmicPortal,
            BuildStage::CosmicInitialSetup,
            BuildStage::Greetd,
        ] {
            let edges = direct_dependencies(stage);
            for forbidden in forbidden_for_leaves {
                assert!(
                    !edges.contains(&forbidden),
                    "{} retained runtime-only edge {forbidden}",
                    stage_id(stage)
                );
            }
        }
        assert_eq!(
            direct_dependencies(BuildStage::Greetd),
            &["formal-sysroot", "linux-pam"]
        );
        assert_eq!(
            direct_dependencies(BuildStage::CosmicFiles),
            &["formal-sysroot", "glib", "xkbcommon", "zlib"]
        );
        assert_eq!(
            direct_dependencies(BuildStage::CosmicPanel),
            &["formal-sysroot", "xkbcommon"]
        );
        assert_eq!(
            direct_dependencies(BuildStage::CosmicSettings),
            &[
                "formal-sysroot",
                "dav1d",
                "xkbcommon",
                "systemd",
                "libinput",
            ]
        );
        assert_eq!(
            direct_dependencies(BuildStage::CosmicPortal),
            &[
                "formal-sysroot",
                "mesa",
                "glib",
                "pipewire",
                "xkbcommon",
                "zlib"
            ]
        );
        assert_eq!(
            direct_dependencies(BuildStage::CosmicInitialSetup),
            &["formal-sysroot", "systemd", "xkbcommon", "libinput"]
        );
    }

    #[test]
    fn flatpak_and_store_edges_cover_only_their_real_owned_inputs() {
        let flatpak = direct_dependencies(BuildStage::Flatpak);
        for required in [
            "appstream",
            "gdk-pixbuf",
            "gpgme",
            "bubblewrap",
            "xdg-dbus-proxy",
        ] {
            assert!(
                flatpak.contains(&required),
                "Flatpak's native build environment omits {required}"
            );
        }

        let store = direct_dependencies(BuildStage::CosmicStore);
        assert!(store.contains(&"flatpak"));
        assert!(store.contains(&"ostree"));
        assert!(!store.contains(&"mesa"));
        assert!(!store.contains(&"pipewire"));

        let utilities = direct_dependencies(BuildStage::CosmicUtilities);
        assert!(!utilities.contains(&"flatpak"));
        assert!(!utilities.contains(&"ostree"));

        let background = direct_dependencies(BuildStage::CosmicBg);
        assert!(background.contains(&"dav1d"));
        assert!(background.contains(&"xkbcommon"));
        assert!(!background.contains(&"pipewire"));

        let desktop = direct_dependencies(BuildStage::CosmicDesktop);
        assert!(desktop.contains(&"cosmic-store"));
    }

    #[test]
    fn composition_edges_are_confined_to_the_desktop_aggregate() {
        let leaves = [
            "cosmic-session",
            "cosmic-greeter",
            "cosmic-panel",
            "cosmic-files",
            "cosmic-settings",
            "cosmic-portal",
            "greetd",
        ];
        for leaf in leaves {
            let dependencies = dependency_map()[leaf].clone();
            assert!(!dependencies.contains("cosmic-comp"));
            assert!(!dependencies.contains("cosmic-desktop"));
        }
        let desktop = dependency_map()["cosmic-desktop"].clone();
        assert!(desktop.contains("cosmic-files"));
        assert!(desktop.contains("cosmic-settings"));
        assert!(desktop.contains("greetd"));
    }

    #[test]
    fn byte_identical_dependency_rebuild_does_not_invalidate_consumers() {
        assert!(downstream_invalidation(&[]).is_empty());
    }

    #[test]
    fn representative_output_cascades_are_exact() {
        assert_eq!(
            downstream_invalidation(&["brush"]),
            [
                "brush",
                "packages",
                "repository",
                "rootfs",
                "live-root",
                "iso"
            ]
            .into_iter()
            .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            downstream_invalidation(&["linux"]),
            [
                "linux",
                "initramfs",
                "installer",
                "nvidia-driver",
                "packages",
                "repository",
                "rootfs",
                "live-root",
                "iso",
                "cosmic-utilities",
                "cosmic-store",
                "cosmic-storage",
                "kpmcore",
                "ostree",
                "flatpak",
                "xdg-desktop-portal",
                "cosmic-desktop"
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(
            downstream_invalidation(&["repository"]),
            ["repository", "rootfs", "live-root", "iso"]
                .into_iter()
                .collect()
        );
        assert_eq!(
            downstream_invalidation(&["rootfs"]),
            ["rootfs", "live-root", "iso"].into_iter().collect()
        );
        assert_eq!(
            downstream_invalidation(&["initramfs"]),
            ["initramfs", "iso"].into_iter().collect()
        );
        assert_eq!(
            downstream_invalidation(&["git"]),
            [
                "git",
                "packages",
                "repository",
                "rootfs",
                "live-root",
                "iso"
            ]
            .into_iter()
            .collect()
        );
        let cpython_cascade = downstream_invalidation(&["cpython"]);
        assert!(cpython_cascade.is_superset(&
            [
                "cpython",
                "x11-compat",
                "vulkan-loader",
                "vulkan-tools",
                "libepoxy",
                "libfontenc",
                "libxfont",
                "libxcvt",
                "libxshmfence",
                "libxkbfile",
                "xkbcomp",
                "libglvnd",
                "mesa",
                "xwayland",
                "nvidia-driver",
                "qtbase",
                "qtsvg",
                "qtwayland",
                "qtdeclarative",
                "qtshadertools",
                "kcoreaddons",
                "kconfig",
                "kdbusaddons",
                "kauth",
                "ki18n",
                "kwidgetsaddons",
                "polkit-qt-1",
                "kpmcore",
                "cosmic-comp",
                "cosmic-portal",
                "cosmic-workspaces",
                "cosmic-utilities",
                "cosmic-store",
                "cosmic-storage",
                "ostree",
                "flatpak",
                "xdg-desktop-portal",
                "cosmic-desktop",
                "packages",
                "repository",
                "rootfs",
                "live-root",
                "iso",
                "installer"
            ]
            .into_iter()
            .collect::<BTreeSet<_>>()
        ));
        let llvm_cascade = downstream_invalidation(&["llvm"]);
        assert!(llvm_cascade.is_superset(&
            [
                "llvm",
                "rust",
                "mesa",
                "vulkan-tools",
                "qtbase",
                "qtsvg",
                "qtwayland",
                "qtdeclarative",
                "qtshadertools",
                "kcoreaddons",
                "kconfig",
                "kdbusaddons",
                "kauth",
                "ki18n",
                "kwidgetsaddons",
                "polkit-qt-1",
                "kpmcore",
                "cosmic-comp",
                "cosmic-portal",
                "cosmic-workspaces",
                "cosmic-desktop",
                "cosmic-utilities",
                "cosmic-randr",
                "cosmic-screenshot",
                "pop-launcher",
                "cosmic-calculator",
                "cosmic-storage",
                "cosmic-monitor",
                "cosmic-store",
                "ostree",
                "flatpak",
                "xwayland",
                "xdg-desktop-portal",
                "installer",
                "packages",
                "repository",
                "rootfs",
                "live-root",
                "iso"
            ]
            .into_iter()
            .collect::<BTreeSet<_>>()
        ));
        for component in [
            "cosmic-session",
            "cosmic-greeter",
            "cosmic-panel",
            "cosmic-applets",
            "cosmic-applibrary",
            "cosmic-launcher",
            "cosmic-settings",
            "cosmic-settings-daemon",
            "cosmic-notifications",
            "cosmic-osd",
            "cosmic-bg",
            "cosmic-workspaces",
            "cosmic-files",
            "cosmic-term",
            "cosmic-tweaks",
            "cosmic-store",
            "cosmic-portal",
            "cosmic-assets",
            "greetd",
        ] {
            assert_eq!(
                downstream_invalidation(&[component]),
                [
                    component,
                    "cosmic-desktop",
                    "packages",
                    "repository",
                    "rootfs",
                    "live-root",
                    "iso",
                ]
                .into_iter()
                .collect(),
                "{component} invalidated an unrelated native COSMIC stage"
            );
        }
        // The compatibility aggregate is composition-only: each leaf also
        // refreshes it, but never causes another utility leaf to compile.
        for component in [
            "cosmic-randr",
            "cosmic-screenshot",
            "pop-launcher",
            "cosmic-calculator",
            "cosmic-storage",
            "cosmic-monitor",
        ] {
            assert_eq!(
                downstream_invalidation(&[component]),
                [
                    component,
                    "cosmic-utilities",
                    "cosmic-desktop",
                    "packages",
                    "repository",
                    "rootfs",
                    "live-root",
                    "iso",
                ]
                .into_iter()
                .collect(),
                "{component} invalidated another utility leaf"
            );
        }
        for unrelated in [
            "linux",
            "glibc",
            "gcc-runtime",
            "gcc-compiler",
            "binutils",
            "curl",
            "openssl",
            "zlib",
        ] {
            assert!(
                !downstream_invalidation(&["git"]).contains(unrelated),
                "Git invalidation escaped into unrelated stage {unrelated}"
            );
        }
    }

    #[derive(Clone, Copy)]
    enum SyntheticChange {
        None,
        IrrelevantSource,
        RelevantInput(&'static str),
        DependencyInputWithIdenticalOutput(&'static str),
        DependencyOutput(&'static str),
        MissingOutput(&'static str),
        CorruptedOutput(&'static str),
        Recipe(&'static str),
    }

    fn expected_misses(change: SyntheticChange) -> BTreeSet<&'static str> {
        match change {
            SyntheticChange::None
            | SyntheticChange::IrrelevantSource
            | SyntheticChange::DependencyInputWithIdenticalOutput(_) => BTreeSet::new(),
            SyntheticChange::RelevantInput(stage)
            | SyntheticChange::DependencyOutput(stage)
            | SyntheticChange::MissingOutput(stage)
            | SyntheticChange::CorruptedOutput(stage)
            | SyntheticChange::Recipe(stage) => downstream_invalidation(&[stage]),
        }
    }

    #[test]
    fn incremental_cache_scenario_matrix_asserts_misses_and_hits() {
        let scenarios = [
            ("no change", SyntheticChange::None, BTreeSet::new()),
            (
                "irrelevant source",
                SyntheticChange::IrrelevantSource,
                BTreeSet::new(),
            ),
            (
                "relevant source",
                SyntheticChange::RelevantInput("brush"),
                downstream_invalidation(&["brush"]),
            ),
            (
                "configuration",
                SyntheticChange::RelevantInput("linux"),
                downstream_invalidation(&["linux"]),
            ),
            (
                "dependency output",
                SyntheticChange::DependencyOutput("zlib"),
                downstream_invalidation(&["zlib"]),
            ),
            (
                "dependency input, identical output",
                SyntheticChange::DependencyInputWithIdenticalOutput("zlib"),
                BTreeSet::new(),
            ),
            (
                "missing output",
                SyntheticChange::MissingOutput("binutils"),
                downstream_invalidation(&["binutils"]),
            ),
            (
                "corrupt output",
                SyntheticChange::CorruptedOutput("initramfs"),
                downstream_invalidation(&["initramfs"]),
            ),
            (
                "package only",
                SyntheticChange::RelevantInput("packages"),
                downstream_invalidation(&["packages"]),
            ),
            (
                "rootfs only",
                SyntheticChange::RelevantInput("rootfs"),
                downstream_invalidation(&["rootfs"]),
            ),
            (
                "Linux only",
                SyntheticChange::RelevantInput("linux"),
                downstream_invalidation(&["linux"]),
            ),
            (
                "build recipe",
                SyntheticChange::Recipe("gcc-compiler"),
                downstream_invalidation(&["gcc-compiler"]),
            ),
        ];
        let all = dependency_map().keys().copied().collect::<BTreeSet<_>>();
        for (name, change, expected) in scenarios {
            let misses = expected_misses(change);
            assert_eq!(misses, expected, "wrong misses for {name}");
            let hits = all.difference(&misses).copied().collect::<BTreeSet<_>>();
            assert_eq!(
                hits.len() + misses.len(),
                all.len(),
                "incomplete hit/miss partition for {name}"
            );
            assert!(
                hits.is_disjoint(&misses),
                "stage both hit and missed for {name}"
            );
        }
    }

    #[test]
    fn dependency_input_change_with_identical_bytes_keeps_consumers_hot() {
        let change = SyntheticChange::DependencyInputWithIdenticalOutput("glibc");
        assert!(expected_misses(change).is_empty());
        let SyntheticChange::DependencyInputWithIdenticalOutput(rebuilt) = change else {
            unreachable!()
        };
        assert_eq!(rebuilt, "glibc");
    }

    #[test]
    fn representative_cascade_report() {
        let scenarios: &[(&str, &[&str], usize, &[&str])] = &[
            ("Brush source", &["brush"], 6, &["zlib", "linux"]),
            // The first-class Flatpak desktop integration stages legitimately
            // consume glibc outputs. The explicit Flatpak proxy and isolated
            // Store stage and the six independent utility consumers extend
            // this closure from 141 to 149. Fontconfig is now a first-class
            // consumer of glibc through its source-owned runtime closure.
            ("glibc source", &["glibc"], 240, &["linux"]),
            ("Linux x86_64 config", &["linux"], 17, &["glibc", "brush"]),
            (
                "Linux x86_64 UAPI source",
                &["linux", "glibc", "linux-headers"],
                241,
                &[],
            ),
            (
                "GCC source",
                &["gcc-runtime", "gcc-compiler"],
                238,
                &["linux", "glibc", "linux-headers"],
            ),
            // Initial Setup now has its real systemd/libinput build edges, so
            // it is part of the legitimate native-library cascade.
            ("zlib shared library", &["zlib"], 170, &["brush", "linux"]),
            ("package metadata", &["packages"], 5, &["brush", "zlib"]),
            (
                "repository policy",
                &["repository"],
                4,
                &["packages", "brush"],
            ),
            (
                "rootfs configuration",
                &["rootfs"],
                3,
                &["repository", "packages"],
            ),
            (
                "initramfs configuration",
                &["initramfs"],
                2,
                &["rootfs", "packages"],
            ),
            (
                "live-root recipe",
                &["live-root"],
                2,
                &["rootfs", "packages"],
            ),
        ];
        for (name, changed, expected_count, unrelated_hits) in scenarios {
            let invalidated = downstream_invalidation(changed);
            println!("{name}: {} stage(s): {invalidated:?}", invalidated.len());
            assert!(changed.iter().all(|stage| invalidated.contains(stage)));
            assert_eq!(
                invalidated.len(),
                *expected_count,
                "closure changed for {name}"
            );
            assert!(
                unrelated_hits
                    .iter()
                    .all(|stage| !invalidated.contains(stage)),
                "unrelated stage invalidated for {name}"
            );
        }
    }

    #[test]
    fn representative_incremental_rebuilds_distinguish_candidates_from_changed_bytes() {
        struct Scenario {
            name: &'static str,
            owners: &'static [&'static str],
            all_rebuilt_outputs_change: bool,
            unrelated_hits: &'static [&'static str],
        }
        let scenarios = [
            Scenario {
                name: "Brush source, identical binary",
                owners: &["brush"],
                all_rebuilt_outputs_change: false,
                unrelated_hits: &["zlib", "linux", "packages", "rootfs"],
            },
            Scenario {
                name: "Brush source, changed binary",
                owners: &["brush"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["zlib", "linux", "glibc"],
            },
            Scenario {
                name: "glibc source, identical publication",
                owners: &["glibc"],
                all_rebuilt_outputs_change: false,
                unrelated_hits: &["linux", "gcc-runtime", "packages", "rootfs"],
            },
            Scenario {
                name: "glibc source, changed publication",
                owners: &["glibc"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["linux"],
            },
            Scenario {
                name: "Linux source, changed kernel",
                owners: &["linux"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["glibc", "linux-headers", "brush"],
            },
            Scenario {
                name: "Linux config, changed kernel",
                owners: &["linux"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["glibc", "linux-headers", "brush"],
            },
            Scenario {
                name: "Linux UAPI, changed kernel and headers",
                owners: &["linux", "glibc", "linux-headers"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &[],
            },
            Scenario {
                name: "GCC source, identical runtime and compiler",
                owners: &["gcc-runtime", "gcc-compiler"],
                all_rebuilt_outputs_change: false,
                unrelated_hits: &["linux", "glibc", "binutils", "packages", "rootfs"],
            },
            Scenario {
                name: "GCC source, changed runtime and compiler",
                owners: &["gcc-runtime", "gcc-compiler"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["linux", "glibc", "linux-headers"],
            },
            Scenario {
                name: "zlib source, changed library",
                owners: &["zlib"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["brush", "linux", "glibc"],
            },
            Scenario {
                name: "package metadata, changed package and inventory",
                owners: &["packages"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["brush", "zlib", "linux", "glibc"],
            },
            Scenario {
                name: "rootfs configuration, identical rootfs",
                owners: &["rootfs"],
                all_rebuilt_outputs_change: false,
                unrelated_hits: &["packages", "repository", "initramfs", "iso"],
            },
            Scenario {
                name: "rootfs configuration, changed rootfs",
                owners: &["rootfs"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["packages", "repository", "linux"],
            },
            Scenario {
                name: "initramfs recipe, changed archive",
                owners: &["initramfs"],
                all_rebuilt_outputs_change: true,
                unrelated_hits: &["rootfs", "packages", "repository", "linux"],
            },
        ];
        let all = dependency_map().keys().copied().collect::<BTreeSet<_>>();
        for scenario in scenarios {
            let candidates = scenario
                .owners
                .iter()
                .flat_map(|owner| downstream_invalidation(&[*owner]))
                .collect::<BTreeSet<_>>();
            let changed_outputs = if scenario.all_rebuilt_outputs_change {
                candidates.iter().copied().collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let required = actual_rebuilds(scenario.owners, &changed_outputs);
            let expected_required = if scenario.all_rebuilt_outputs_change {
                candidates.clone()
            } else {
                scenario.owners.iter().copied().collect()
            };
            assert_eq!(
                required, expected_required,
                "wrong rebuilds for {}",
                scenario.name
            );
            assert!(
                required.is_subset(&candidates),
                "rebuild escaped candidate closure for {}",
                scenario.name
            );
            let hits = all.difference(&required).copied().collect::<BTreeSet<_>>();
            assert!(
                scenario
                    .unrelated_hits
                    .iter()
                    .all(|stage| hits.contains(stage)),
                "unrelated stage rebuilt for {}",
                scenario.name
            );
        }
    }
}
