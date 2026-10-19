# KDE Applications

The Plasma profile (`mattos-plasma`) installs these KDE applications, each
built from vendored source as its own stage and package:

[Slate](slate.md) is the default graphical editor. Kate and KWrite are optional
[third-party packages](../../packaging/third-party-packages.md); they are not
vendored or installed by the Plasma profile.

Font, palette, terminal, helper-command and Bluetooth integration are
described in [Desktop Defaults](desktop-defaults.md).

| Application | Package | Executable |
| --- | --- | --- |
| Dolphin | dolphin | `dolphin` |
| Discover | plasma-discover | `plasma-discover` |
| System Monitor | plasma-systemmonitor | `plasma-systemmonitor` |
| Konsole | konsole | `konsole` |
| System Settings | systemsettings | `systemsettings` |
| Haruna | haruna | `haruna` |
| Spectacle | spectacle | `spectacle` |
| KCalc | kcalc | `kcalc` |
| Gwenview | gwenview | `gwenview` |
| KDE Partition Manager | partitionmanager | `partitionmanager` |
| KWallet Manager | kwalletmanager | `kwalletmanager5` |
| Ark | ark | `ark` |
| Elisa | elisa | `elisa` |

The installed-system test (`DevUtils/run_qemu.py`, Plasma profile) checks that
each package is installed, that every library its executable links resolves
(`ldd`), and that it answers `--version` on the offscreen Qt platform. It also
compiles an installed-SDK probe that parses CSS through the Rust/C++ bridge,
loads Union's installed Breeze style, renders text with Qt and recognizes it
with the offline English OCR model, decodes a generated WAV file through Qt
Multimedia's installed backend, and writes/reads/deletes a disposable secret
through QtKeychain and KWallet. These checks run on fresh and upgraded
Plasma installations.

The Plasma session includes `/usr/qml` in its QML import path, matching the
native Qt Declarative install layout. Serial-driven application tests inherit
the session's Qt plugin and QML paths before launching the default editor.

The login check reads the default Secret Service collection's `Locked`
property to verify PAM unlocking, including after reboot. KWallet's legacy
`isOpen` reports open application handles rather than that collection's lock
state. `ksecretd` supplies the Secret Service backend; its object no longer
exports the legacy `org.kde.KWallet` interface.

## Stack version and update policy

The selected desktop stack is Plasma 6.8.0, KDE Frameworks 6.31.0, Qt 6.12.0
and KDE Gear 26.08.2. Spectacle follows the Plasma version. KPMCore and Partition
Manager advance together. Exact commits and source provenance are recorded in
`upstream/sources.toml` and `upstream/state/`; the OS release selector remains
independent of these upstream versions.

Qt's private CMake bridges and qsb tool namespace derive their version from
QtBase's `.cmake.conf`. Module builds reject mixed Qt source versions. XKB and
Wayland bridge metadata comes from the staged target pkg-config files rather
than host packages or invented versions. KDE uses pinned build-only ECM 6.31.0,
with process locking around its shared bootstrap. QtBase explicitly builds
its GLib event dispatcher from native GLib, PCRE2 and libffi stages.
PulseAudioQt requires that dispatcher for the desktop volume controls. The
installed Qt SDK normalizes GLib's multiarch include and library paths to
`/usr`, retaining no build checkout paths. The build-only Qt tool bridge
preserves Qt 6.12's complete export set; deployment tools for unsupported
platforms fail explicitly if invoked. Plasma's XML-only protocol package uses
GNUInstallDirs in its output mirror to preserve `/usr/share` installation
without requiring Qt path tools. Enabled Wayland protocol consumers declare
the canonical protocol stage, so ECM cannot select Mesa's private older
protocol metadata. Plasma Framework also declares its Qt Wayland and
Qt5Compat QML requirements.
ECM QML discovery sees every declared provider through a disposable target
view with real `qmldir` markers and linked payloads, including nested modules;
installed producer trees remain unchanged.
The KDE portal declares its required KPipeWire monitor library and target
DRM, GBM, VA-API and Epoxy link providers.
System Monitor declares QuickCharts and uses an output-only patch matching
its unversioned Qt 6 QML import, avoiding the obsolete qmlplugindump fallback.
Union's CSS parser uses pinned build-only Corrosion 0.6.1 and the MattOS Rust
toolchain, with its upstream Cargo lockfile enforced in an output-owned mirror. Cold
bootstraps materialize Corrosion's pinned checkout before configuring its
build-only CMake files.

Plasma's core and Quick libraries advance from ABI 7 to 8 within the existing
`plasma-framework` package; its virtual Provides name the actual ABI 8 libraries.
All native consumers are rebuilt together.

Plasma 6.8 moves global shortcut handling into KWin. The `kglobalacceld`
package now supplies the upstream engine library and SDK; KWin builds its
shortcut integration and owns the `org.kde.kglobalaccel` D-Bus service.
KWin also declares KRunner for its overview integration and the native Vulkan
loader/headers and libcap now required by upstream, with exact target SDK
paths. KDE native links derive private dependency `-rpath-link` directories
from canonical build-graph ancestors, without exposing extra headers or
pkg-config packages or host runtime libraries. It declares the complete
shortcut/overview library closure, Plasma
activities and QML providers, and PipeWire for the explicitly enabled
screencasting feature. Its output-only QML patch uses Qt 6 module discovery
against the pinned cohort without legacy qmlplugindump version probes. The retired
standalone daemon is no longer expected or advertised. Installed
VM checks verify that KWin owns the service and that the shipped Meta+E
shortcut opens Dolphin through virtual keyboard input.

QEMU validation uses standard `virtio-vga-gl` with VirGL acceleration.
Blob resources are left disabled: on QEMU 10.2.1 with virglrenderer 1.2.0,
opening a window can produce an illegal-resource error in the host renderer
and leave KWin blocked in a Mesa GPU render-time query. Standard VirGL passes
the same shortcut and compositor-response checks without that resource path.

KSysGuard's processcore ABI advances from 10 to 11. `libprocesscore11` owns
its runtime and SDK; `libprocesscore10` remains an empty transitional package
that depends on the replacement, allowing existing installations to upgrade
without an automatic package removal or a false claim to provide ABI 10.

Plasma 6.8 requires Union's libraries and CSS parser. Their source, plugins and
package dependencies are included; MattOS keeps its existing Breeze/Nord theme
selection. The workspace declares NetworkManagerQt for its updated Linux
location-based time zone build configuration. Plasma Desktop declares
its required PolkitQt integration and target GLib/D-Bus metadata. Plasma Network Management uses QtKeychain 0.17.0 with the existing
KWallet D-Bus backend and declares Kirigami Addons' FormCard module. The
optional libsecret backend is disabled. Its
output-only translation patch compiles all upstream TS catalogs with the
versioned Linguist bridge without refreshing those pinned authoring files.

KWin's output-only platform-theme patch retains its generic fallback when the
optional KDE theme cannot be created. The patch is rebased against the exact
6.8.0 commit. The former pager X11/QMimeData and KHolidays ECM-floor workarounds
are retired because upstream supplies the required behavior. Plasma Framework's legacy
Wayland-only X11 source edits and removed `WITHOUT_X11` option are retired;
the native KWindowSystem X11 compatibility ABI supports the upstream code.
Plasma Desktop's old activity-switcher X11 guards are retired for the same
reason; the retained upstream file no longer contains the old X11 drag path.
The optional NotifyConfig accessibility KCM and its X11-only `kaccess`
daemon remain omitted together; their shared generated settings must not
leave a partially enabled daemon.
Login Manager's
upstream BUILD_TESTING gate is preserved; its optional shader optimization is
still disabled for reproducibility.

## Supporting libraries

The applications' dependencies are vendored as MattOS packages, following
Debian's package names:

- **Spectacle OCR:** source-built Tesseract 5.5.3 and Leptonica 1.87.0, with
  the upstream fast English and orientation models pinned to the 4.1.0 data
  release. Models install under `/usr/share/tessdata`, so recognition works
  offline. Other language models are not part of the default profile.
  Leptonica builds PNG/JPEG and built-in BMP/PNM support; optional GIF, TIFF,
  WebP and OpenJPEG codecs and Tesseract's network/archive/training tools are
  disabled. Spectacle supplies screenshot pixels directly to the engine.
  Leptonica's output-only SDK patch makes its CMake include paths relocatable,
  uses the configured multiarch library directory in pkg-config, and keeps
  configure probes on discovered target headers instead of host `/usr/include`.
  Spectacle declares the PNG/JPEG/Zlib pkg-config closure needed by the OCR
  SDK, in addition to the engine libraries.
- **KCalc:** GNU MPC (`libmpc3`) beside GMP and MPFR.
- **Gwenview:** Exiv2 (`libexiv2-28`, the library only), libjpeg-turbo
  (`libjpeg62-turbo`, which also provides the TurboJPEG API and the
  development files), kColorPicker and kImageAnnotator
  (`libkcolorpicker-qt6-0`, `libkimageannotator-qt6-0`). TIFF, FITS, RAW
  (KDcraw) and Baloo integration are not built.
- **KDE Partition Manager:** pinned to the kpmcore release it matches
  (26.08.2).
- **Elisa, Gwenview's video and Haruna:** Qt Multimedia's FFmpeg backend,
  decoding with the MattOS FFmpeg (built-in decoders plus dav1d for AV1, no
  network protocols, no hardware decoding), and its PipeWire audio backend.
  Qt loads libpipewire at runtime, so Qt Multimedia adds no audio link
  dependency to its consumers. Its X11 screen capture (XWayland windows)
  links libX11, libXext and libXrandr (`libxrandr2`).
- **Haruna:** libmpv (`libmpv2`, without the `mpv` player) rendering through
  OpenGL/EGL on Wayland with PulseAudio output to PipeWire's server; MpvQt
  (`libmpvqt3`); KDSingleApplication (`libkdsingleapplication-qt6-1.2`);
  libplacebo (`libplacebo360`) without its Vulkan and OpenGL renderers, which
  libmpv's own OpenGL backend does not use; libass (`libass9`) with HarfBuzz
  (`libharfbuzz0b`) and FriBidi (`libfribidi0`). Lua scripting and yt-dlp
  streaming are not available.
- **Discover:** AppStreamQt (`libappstreamqt3`, built with AppStream), the
  Flatpak and KDE Store backends, and the PackageKit backend through
  PackageKit-Qt (`libpackagekitqt6-2`) and PackageKit with its APT backend
  (`packagekit`, which also needs SQLite (`libsqlite3-0`) and Jansson
  (`libjansson4`)). PackageKit's APT backend links GStreamer
  (`libgstreamer1.0-0`, `libgstreamer-plugins-base1.0-0`). As in Debian,
  `packagekit` depends on `libglib2.0-bin` (GLib's `gdbus`, `gio`,
  `gsettings`, `gapplication` and `gresource`): APT's `20packagekit` hook runs
  `gdbus` after every cache update so PackageKit, and with it Discover, sees
  new package lists. Discover needs Kirigami
  Addons 1.10 or later (MattOS pins 1.15.0) and QCoro's network module. Its
  update notifier starts with the session (`/etc/xdg/autostart`), and its
  Updates page appears in System Settings.

libjpeg-turbo, libass, dav1d and FFmpeg build their x86 SIMD code with the
MattOS-built NASM (the `nasm` stage, also shipped as the `nasm` package). It
runs on the build host as a declared stage output, like Qt's own tools, and
reaches Meson and Autotools builds through the staged dependency `PATH`.
NASM's man pages are not built, since they need AsciiDoc.
