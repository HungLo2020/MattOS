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

This milestone also requires the systemd, dbus-broker, Autotools, networking, packaging, glibc-bootstrap, and ELF-inspection tools declared by `DevUtils/setup.py`, including GCC/G++, GNU assembler and linker tools, Make, Bison, Meson/Ninja, CMake, Autoconf/Automake/libtool, `gnulib-tool`, GNU awk (`gawk`), `rsync`, `bindgen`, `dpkg-deb`, `dpkg-scanpackages`, `apt-ftparchive`, `fakeroot`, `zstd`, `xz`, `file`, `ldd`, and `readelf`. Target attr, Expat, PAM, libcrypt, libbsd, libmount, compression, and crypto development files come from imported source builds and `out/sysroot`, not host distribution `-dev` packages.

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
cargo run -p mattos-build -- upstream import glibc
cargo run -p mattos-build -- upstream import attr
cargo run -p mattos-build -- upstream import acl
cargo run -p mattos-build -- upstream import zlib
cargo run -p mattos-build -- upstream import bzip2
cargo run -p mattos-build -- upstream import lz4
cargo run -p mattos-build -- upstream import xz
cargo run -p mattos-build -- upstream import xxhash
cargo run -p mattos-build -- upstream import zstd
cargo run -p mattos-build -- upstream import openssl
cargo run -p mattos-build -- upstream import elfutils
cargo run -p mattos-build -- upstream import pcre2
cargo run -p mattos-build -- upstream import sljit
cargo run -p mattos-build -- upstream import selinux
cargo run -p mattos-build -- upstream import libxcrypt
cargo run -p mattos-build -- upstream import libmd
cargo run -p mattos-build -- upstream import libbsd
cargo run -p mattos-build -- upstream import paxutils
cargo run -p mattos-build -- upstream import tar
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
4. `glibc`: controlled Linux UAPI export, out-of-tree GNU libc build, and initial MattOS development sysroot
5. `attr`, `expat`, `libcap`, `acl`, `zlib`, `bzip2`, `lz4`, `xz`, `xxhash`, `zstd`: focused source-built development/runtime libraries
6. `openssl`: shared libcrypto/libssl build against the staged zlib and Zstandard ABIs
7. `elfutils`: focused libelf build against the staged zlib and Zstandard ABIs
8. `pcre2`, `selinux`, `libxcrypt`: focused PCRE2 8-bit, libselinux compatibility, and password-hashing runtimes; SELinux follows PCRE2
9. `libmd`, `libbsd`: source-built portability libraries; libmd precedes libbsd
10. `tar`: GNU tar built with the MattOS ACL ABI and without SELinux
11. `ncurses`: terminal libraries, tools, and compiled terminfo database
12. `procps`: process-management tools linked to the local ncurses build
13. `iproute2`: `ip`, `ss`, `bridge`, and `tc`, rebuilt against staged libelf, SELinux, and PCRE2
14. `iputils`: unprivileged `ping` and `tracepath`
15. `curl`: HTTP/HTTPS client using staged OpenSSL and the pinned MattOS CA file
16. `pam`, `shadow`, `sudo-rs`: authentication stack rebuilt against staged libxcrypt; Shadow also resolves libbsd and libmd from staged builds
17. `util-linux`: authentication tools plus source-built libblkid/libmount/libsmartcols and mount/umount, with staged SELinux compatibility enabled
18. `kmod`: module administration tools and libkmod
19. `systemd`: minimal Meson/Ninja build with staged kmod/libmount, networkd, resolved, timesyncd, timedated, logind, and `pam_systemd`
20. `dbus-broker`: upstream Meson/Ninja system-bus broker and launcher
21. `dpkg`: imported dpkg Autotools build against MattOS compression, libmd, SELinux, and PCRE2 libraries
22. `apt`: imported APT CMake/Ninja build against MattOS compression and libcrypto libraries
23. `init`: MattOS rescue init build
24. `rootfs`, `initramfs`, `iso`: hybrid package/legacy image assembly

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
cargo run -p mattos-build -- build acl
cargo run -p mattos-build -- build zlib
cargo run -p mattos-build -- build bzip2
cargo run -p mattos-build -- build lz4
cargo run -p mattos-build -- build xz
cargo run -p mattos-build -- build xxhash
cargo run -p mattos-build -- build zstd
cargo run -p mattos-build -- build openssl
cargo run -p mattos-build -- build elfutils
cargo run -p mattos-build -- build pcre2
cargo run -p mattos-build -- build selinux
cargo run -p mattos-build -- build libxcrypt
cargo run -p mattos-build -- build libmd
cargo run -p mattos-build -- build libbsd
cargo run -p mattos-build -- build tar
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

The complete prototype stack currently consists of 54 packages. `mattos-libc6` and `mattos-libc-bin` supply the MattOS-built glibc runtime, loader, NSS/resolver modules, and selected utilities. After glibc, all downstream native stages are cleared and rebuilt with the controlled sysroot. Repository creation validates the dependency graph, staged ELF ownership, exact interpreter, loader resolution, and glibc symbol versions before image embedding. The bootstrap package now contains only host-derived `libgcc_s.so.1` and `libstdc++.so.6`. See `docs/GLIBC_BOOTSTRAP.md`, `docs/PACKAGING.md`, and `docs/BOOTSTRAP_RUNTIME.md` for the exact boundary.

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

`--no-network` omits both the QEMU network backend and NIC. It is the supported negative-test path for confirming that boot and the local authentication/base-administration stack do not depend on connectivity. The embedded package repository is also expected to support `apt-get update` and safe reinstall of `mattos-brush`, `mattos-tar`, `mattos-libbsd0`, `mattos-libzstd1`, and selected leaf-library consumers in this mode. Critical PAM, login, sudo, D-Bus, and systemd-related packages are inspected/extracted in a separate validation root rather than reinstalled underneath the active session.

The default GRUB entry boots `rdinit=/usr/lib/systemd/systemd systemd.unit=mattos.target`.
A rescue GRUB entry is also provided and boots MattOS Rust rescue init from `/usr/libexec/mattos/rescue-init`.

## Cleanup

```
cargo run -p mattos-build -- clean artifacts
cargo run -p mattos-build -- clean logs
cargo run -p mattos-build -- clean cargo
cargo run -p mattos-build -- clean all
```

Cleanup never deletes imported upstream source trees.
