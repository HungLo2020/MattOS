# Building MattOS

All builds for this milestone are Linux-native and must run on a case-sensitive filesystem.

## Prerequisites

For first-time setup on a native Linux host:

```
python3 DevUtils/setup.py
```

Read-only checks:

```
python3 DevUtils/setup.py --check
python3 DevUtils/setup.py --dry-run
```

Run:

```
cargo run -p mattos-build -- doctor
```

Required tools are reported separately from optional tools. Missing-tool package hints are printed for common Linux distributions.
`DevUtils/run_qemu.py` also runs `doctor` first and will direct you to `python3 DevUtils/setup.py` if required prerequisites are missing.

This milestone also requires the systemd, dbus-broker, Autotools, networking, packaging, and ELF-inspection toolchain declared by `DevUtils/setup.py`, including Meson/Ninja, CMake, Autoconf/Automake/libtool, `rsync`, `bindgen`, Expat and OpenSSL development metadata, `dpkg-deb`, `dpkg-scanpackages`, `apt-ftparchive`, `fakeroot`, `zstd`, `xz`, `file`, `ldd`, and `readelf`.

## Upstream source status

```
cargo run -p mattos-build -- upstream status
```

Optional import/sync commands:

```
cargo run -p mattos-build -- upstream import --all
cargo run -p mattos-build -- upstream sync --all
cargo run -p mattos-build -- upstream import systemd
cargo run -p mattos-build -- upstream sync systemd
cargo run -p mattos-build -- upstream import kmod
cargo run -p mattos-build -- upstream import procps-ng
cargo run -p mattos-build -- upstream import ncurses
cargo run -p mattos-build -- upstream import iproute2
cargo run -p mattos-build -- upstream import iputils
cargo run -p mattos-build -- upstream import curl
cargo run -p mattos-build -- upstream import dbus-broker
cargo run -p mattos-build -- upstream import dpkg
cargo run -p mattos-build -- upstream import apt
```

## Full build

```
cargo run -p mattos-build -- build
```

The pipeline stages are:

1. `kernel`: Linux kernel build using `src/kernel/config/x86_64_mattos.config`
2. `brush`: Brush release build
3. `coreutils`: uutils/coreutils multicall build
4. `ncurses`: terminal libraries, tools, and compiled terminfo database
5. `procps`: process-management tools linked to the local ncurses build
6. `iproute2`: `ip`, `ss`, `bridge`, and `tc`
7. `iputils`: unprivileged `ping` and `tracepath`
8. `curl`: HTTP/HTTPS client using OpenSSL and the pinned MattOS CA path
9. `pam`, `util-linux`, `shadow`, `sudo-rs`: existing authentication stack
10. `kmod`: module administration tools and libkmod
11. `systemd`: minimal Meson/Ninja build with kmod, networkd, resolved, timesyncd, timedated, logind, and `pam_systemd`
12. `dbus-broker`: upstream Meson/Ninja system-bus broker and launcher
13. `dpkg`: imported dpkg Autotools build
14. `apt`: imported APT CMake/Ninja build
15. `init`: MattOS rescue init build
16. `rootfs`, `initramfs`, `iso`: hybrid package/legacy image assembly

Systemd configuration remains intentionally minimal. It enables networkd, resolved, timesyncd, timedated, logind, PAM integration, and `busctl` while continuing to disable homed, nspawn, bootloader tools, the remote journal stack, docs, tests, translations, TPM/FIDO, and BPF extras. The separate dbus-broker stage supplies both the system-scope binary and the binary used by MattOS-owned user units; systemd's Meson `dbus` option remains disabled because it controls the optional reference `libdbus` dependency, not sd-bus support.

## Incremental builds

```
cargo run -p mattos-build -- build kernel
cargo run -p mattos-build -- build brush
cargo run -p mattos-build -- build coreutils
cargo run -p mattos-build -- build kmod
cargo run -p mattos-build -- build ncurses
cargo run -p mattos-build -- build procps
cargo run -p mattos-build -- build iproute2
cargo run -p mattos-build -- build iputils
cargo run -p mattos-build -- build curl
cargo run -p mattos-build -- build expat
cargo run -p mattos-build -- build libcap
cargo run -p mattos-build -- build systemd
cargo run -p mattos-build -- build dbus-broker
cargo run -p mattos-build -- build dpkg
cargo run -p mattos-build -- build apt
cargo run -p mattos-build -- build init
cargo run -p mattos-build -- image
```

`image` reassembles rootfs, initramfs, and ISO without forcing unrelated recompilation.

Package and repository commands:

```
cargo run -p mattos-build -- package build --all
cargo run -p mattos-build -- package repo
cargo run -p mattos-build -- package inspect mattos-apt
cargo run -p mattos-build -- package audit
cargo run -p mattos-build -- package status
```

The complete prototype stack currently consists of 32 packages. In addition to filesystem, package-manager, Brush, coreutils, curl, and CA packages, it now splits ncurses/terminfo, kmod, procps, the public systemd libraries, Expat, libcap, dbus-broker, Linux-PAM, Shadow, sudo-rs, util-linux authentication tools, iproute2, and iputils into deliberate ownership boundaries. Repository creation validates the complete dependency graph, detects cycles, computes install order, checks staged ELF ownership, and rejects a migrated library in the bootstrap closure before image embedding. See `docs/PACKAGING.md` and `docs/BOOTSTRAP_RUNTIME.md` for the exact boundaries and migration map.

## QEMU boot

```
cargo run -p mattos-build -- run
```

Boot logs are written to `out/logs/qemu-boot.log`.

The Python launcher adds `virtio-net-pci` backed by QEMU user-mode networking by default:

```
python3 DevUtils/run_qemu.py
python3 DevUtils/run_qemu.py --no-network
```

`--no-network` omits both the QEMU network backend and NIC. It is the supported negative-test path for confirming that boot and the local authentication/base-administration stack do not depend on connectivity. The embedded package repository is also expected to support `apt-get update` and safe reinstall of `mattos-iputils`, `mattos-procps`, and `mattos-ncurses-bin` in this mode. Critical PAM, login, sudo, D-Bus, and systemd-related packages are inspected/extracted in a separate validation root rather than reinstalled underneath the active session.

The default GRUB entry boots `init=/usr/lib/systemd/systemd systemd.unit=mattos.target`.
A rescue GRUB entry is also provided and boots MattOS Rust rescue init from `/usr/libexec/mattos/rescue-init`.

## Cleanup

```
cargo run -p mattos-build -- clean artifacts
cargo run -p mattos-build -- clean logs
cargo run -p mattos-build -- clean cargo
cargo run -p mattos-build -- clean all
```

Cleanup never deletes imported upstream source trees.
