# KDE Applications

The Plasma profile (`mattos-plasma`) installs these KDE applications, each
built from vendored source as its own stage and package:

| Application | Package | Executable |
| --- | --- | --- |
| Dolphin | dolphin | `dolphin` |
| Kate | kate | `kate` |
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
(`ldd`), and that it answers `--version` on the offscreen Qt platform.

## Supporting libraries

The applications' dependencies are vendored as MattOS packages, following
Debian's package names:

- **KCalc:** GNU MPC (`libmpc3`) beside GMP and MPFR.
- **Gwenview:** Exiv2 (`libexiv2-28`, the library only), libjpeg-turbo
  (`libjpeg62-turbo`, which also provides the TurboJPEG API and the
  development files), kColorPicker and kImageAnnotator
  (`libkcolorpicker-qt6-0`, `libkimageannotator-qt6-0`). TIFF, FITS, RAW
  (KDcraw) and Baloo integration are not built.
- **KDE Partition Manager:** pinned to the kpmcore release it matches
  (25.12.3).
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
  (`libjansson4`)). PackageKit's APT backend links GStreamer, which the
  `xdg-desktop-portal` package currently carries. Discover needs Kirigami
  Addons 1.10 or later (MattOS pins 1.15.0) and QCoro's network module. Its
  update notifier starts with the session (`/etc/xdg/autostart`), and its
  Updates page appears in System Settings.

libjpeg-turbo, libass and FFmpeg are built without their x86 assembly, which
needs NASM; MattOS does not build NASM.
