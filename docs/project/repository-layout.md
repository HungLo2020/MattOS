# Repository Layout

Project-managed source trees live under `src/`:

- `src/boot/`: GRUB configuration and the early `/init`
- `src/build-support/`: build-time support sources such as gnulib and
  autoconf-archive
- `src/build-tools/`: build tools such as GNU Make and xcursorgen
- `src/desktop/`: desktop components (fonts, KDE, Qt, themes)
- `src/development/`: development runtimes such as Python
- `src/graphics/`: graphics stack components such as wayland-protocols
- `src/infrastructure/`: project infrastructure scripts (LinuxScripts)
- `src/kernel/`: the Linux kernel
- `src/rootfs/`: the root filesystem skeleton
- `src/system/`: system services, package management and networking
- `src/toolchain/`: compilers and toolchains (binutils, GCC, LLVM, Rust)
- `src/tools/`: MattOS-owned tools, including the build orchestrator
- `src/userland/`: core userland programs

Upstream source is imported directly as ordinary tracked files; no Git
submodules are used.

Representative components:

- `src/kernel/linux`: upstream Linux kernel source
- `src/userland/brush`: upstream Brush shell source
- `src/userland/coreutils`: upstream uutils/coreutils source
- `src/system/systemd`: upstream systemd source
- `src/system/dbus/dbus-broker`: upstream dbus-broker source
- `src/system/packages/dpkg`: upstream dpkg source
- `src/system/packages/apt`: upstream APT source; the MattOS vendor policy is
  applied as a patch from
  `upstream/patches/apt/0001-mattos-vendor-and-optional-ftparchive.patch`
- `src/system/packages/config/apt`: MattOS runtime APT configuration (sources,
  preferences and keys)
- `src/system/kmod`: upstream kmod source
- `src/system/terminal/ncurses`: upstream ncurses source
- `src/userland/procps-ng`: upstream procps-ng source
- `src/userland/iproute2`: upstream iproute2 source
- `src/userland/iputils`: upstream iputils source
- `src/userland/curl`: upstream curl source
- `src/system/network`: MattOS-owned network, resolver, time, NSS, and CA
  configuration, alongside upstream network sources (NetworkManager, hostap,
  libndp, libnl, OpenSSH and wpa_supplicant)
- `src/userland/init`: MattOS-owned Rust rescue init (crate `mattos-init`),
  installed as `/usr/libexec/mattos/rescue-init`; it runs as PID 1 only on the
  rescue boot path. Normal boots hand PID 1 to systemd from the early `/init`
  (`src/boot/live-init.c`, `src/boot/grub/grub.cfg`)
- `src/tools/mattos-build`: MattOS-owned Rust build orchestrator

Other top-level directories:

- `upstream/`: source pins, import state, policies and patches
- `DevUtils/`: developer scripts
- `docs/`: this wiki
- `third-party-packages/`: standalone recipes that build native `.deb`
  packages for software outside the ISO build (for example btop, fastfetch
  and htop) inside the MattOS builder container; they are not build stages
  (see [Third-Party Packages](../packaging/third-party-packages.md))
- `frnsrc/`: MattOS KDE Plasma theming files (global theme, launcher icons and
  KWin/panel defaults)
- `resources/`: project artwork such as the MattOS logo icons
- `out/`: build output
