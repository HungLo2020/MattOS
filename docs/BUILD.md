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

This milestone also requires the systemd, dbus-broker, Autotools, networking, packaging, glibc/GCC-runtime-bootstrap, and ELF-inspection tools declared by `DevUtils/setup.py`, including GCC/G++, GNU assembler and linker tools, Make, Bison, Meson/Ninja, CMake, Autoconf/Automake/libtool, `gnulib-tool`, GNU awk (`gawk`), `rsync`, `bindgen`, `dpkg-deb`, `dpkg-scanpackages`, `apt-ftparchive`, `fakeroot`, `zstd`, `xz`, `file`, `ldd`, and `readelf`. Host GMP, MPFR, and MPC development files are GCC-internal build prerequisites only. Target runtime development files come from imported source builds and `out/sysroot`, not host distribution `-dev` packages.

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
cargo run -p mattos-build -- upstream import gcc
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
cargo run -p mattos-build -- upstream import linuxscripts
```

## Full build

```
cargo run -p mattos-build -- build
```

The pipeline stages are:

1. `kernel`: Linux kernel build using `src/kernel/config/x86_64_mattos.config`
2. `glibc`: controlled Linux UAPI export, out-of-tree GNU libc build, and initial MattOS development sysroot
3. `gcc-runtime`: top-level GCC runtime-only build selecting `libgcc_s.so.1` and `libstdc++.so.6`
4. `brush`: Brush release build
5. `coreutils`: uutils/coreutils multicall build
6. `attr`, `expat`, `libcap`, `acl`, `zlib`, `bzip2`, `lz4`, `xz`, `xxhash`, `zstd`: focused source-built development/runtime libraries
7. `openssl`: shared libcrypto/libssl build against the staged zlib and Zstandard ABIs
8. `elfutils`: focused libelf build against the staged zlib and Zstandard ABIs
9. `pcre2`, `selinux`, `libxcrypt`: focused PCRE2 8-bit, libselinux compatibility, and password-hashing runtimes; SELinux follows PCRE2
10. `libmd`, `libbsd`: source-built portability libraries; libmd precedes libbsd
11. `tar`: GNU tar built with the MattOS ACL ABI and without SELinux
12. `ncurses`: terminal libraries, tools, and compiled terminfo database
13. `procps`: process-management tools linked to the local ncurses build
14. `iproute2`: `ip`, `ss`, `bridge`, and `tc`, rebuilt against staged libelf, SELinux, and PCRE2
15. `iputils`: unprivileged `ping` and `tracepath`
16. `curl`: HTTP/HTTPS client using staged OpenSSL and the pinned MattOS CA file
17. `pam`, `shadow`, `sudo-rs`: authentication stack rebuilt against staged libxcrypt; Shadow also resolves libbsd and libmd from staged builds
18. `util-linux`: authentication tools plus source-built libblkid/libmount/libsmartcols and mount/umount, with staged SELinux compatibility enabled
19. `kmod`: module administration tools and libkmod
20. `systemd`: minimal Meson/Ninja build with staged kmod/libmount, networkd, resolved, timesyncd, timedated, logind, and `pam_systemd`
21. `dbus-broker`: upstream Meson/Ninja system-bus broker and launcher
22. `dpkg`: imported dpkg Autotools build against MattOS compression, libmd, SELinux, and PCRE2 libraries
23. `apt`: imported APT CMake/Ninja build against MattOS compression and libcrypto libraries
24. `init`: MattOS rescue init build
25. `rootfs`, `initramfs`, `iso`: hybrid package/legacy image assembly

Systemd configuration remains intentionally minimal. It enables networkd, resolved, timesyncd, timedated, logind, PAM integration, and `busctl` while continuing to disable homed, nspawn, bootloader tools, the remote journal stack, docs, tests, translations, TPM/FIDO, and BPF extras. The separate dbus-broker stage supplies both the system-scope binary and the binary used by MattOS-owned user units; systemd's Meson `dbus` option remains disabled because it controls the optional reference `libdbus` dependency, not sd-bus support.

## Incremental builds

```
cargo run -p mattos-build -- build kernel
cargo run -p mattos-build -- build glibc
cargo run -p mattos-build -- build gcc-runtime
cargo run -p mattos-build -- build binutils
cargo run -p mattos-build -- build gcc-toolchain
cargo run -p mattos-build -- build make
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
cargo run -p mattos-build -- package inspect apt
cargo run -p mattos-build -- package audit
cargo run -p mattos-build -- package status
cargo run -p mattos-build -- package compatibility-audit
```

The complete prototype stack consists of 65 packages. `libc6` and `libc-bin` supply the MattOS-built glibc runtime, loader, NSS/resolver modules, and selected utilities; `libgcc-s1` and `libstdc++6` supply the final source-built compiler runtimes. Ten development packages add Linux/glibc/GCC development files, source-built Binutils, GCC C/C++, and GNU Make. After glibc and GCC runtime construction, downstream native stages are rebuilt with the controlled sysroot. Repository creation validates the dependency graph, staged ELF ownership, exact interpreter, loader resolution, and GLIBC/GLIBCXX/CXXABI/GCC symbol versions before image embedding. `mattos-bootstrap-runtime` is retired and the final host-derived target-runtime count is zero. The compatibility audit also validates all package classifications, versions, protected pins, source scaffolds, and the immutable LinuxScripts publisher. See `docs/GLIBC_BOOTSTRAP.md`, `docs/GCC_RUNTIME_BOOTSTRAP.md`, `docs/NATIVE_TOOLCHAIN.md`, `docs/PACKAGING.md`, `docs/DEBIAN_COMPATIBILITY.md`, `docs/REMOTE_REPOSITORY.md`, and `docs/BOOTSTRAP_RUNTIME.md`.

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

`--no-network` omits both the QEMU network backend and NIC. It is the supported negative-test path for confirming that boot and the local authentication/base-administration stack do not depend on connectivity. The embedded package repository is also expected to support `apt-get update` and safe reinstall of `mattos-brush`, `tar`, `libbsd0`, `libzstd1`, and selected leaf-library consumers in this mode. Critical PAM, login, sudo, D-Bus, and systemd-related packages are inspected/extracted in a separate validation root rather than reinstalled underneath the active session.

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
