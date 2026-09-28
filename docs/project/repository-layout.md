# Repository Layout

Project-managed source trees live under `src/`: `src/kernel/`,
`src/userland/`, `src/boot/`, `src/rootfs/`, `src/system/`, and `src/tools/`.
Upstream source is imported directly as ordinary tracked files; no Git
submodules are used.

Representative components:

- `src/kernel/linux`: upstream Linux kernel source
- `src/userland/brush`: upstream Brush shell source
- `src/userland/coreutils`: upstream uutils/coreutils source
- `src/system/systemd`: upstream systemd source
- `src/system/dbus/dbus-broker`: upstream dbus-broker source
- `src/system/packages/dpkg`: upstream dpkg source
- `src/system/packages/apt`: upstream APT source plus MattOS vendor policy
- `src/system/kmod`: upstream kmod source
- `src/system/terminal/ncurses`: upstream ncurses source
- `src/userland/procps-ng`: upstream procps-ng source
- `src/userland/iproute2`: upstream iproute2 source
- `src/userland/iputils`: upstream iputils source
- `src/userland/curl`: upstream curl source
- `src/system/network`: MattOS-owned network, resolver, time, NSS, and CA configuration
- `src/userland/init`: MattOS-owned Rust PID 1
- `src/tools/mattos-build`: MattOS-owned Rust build orchestrator

Other top-level directories: `upstream/` holds source pins, import state and
policies; `DevUtils/` holds developer scripts; `docs/` is this wiki; `out/` is
build output.
