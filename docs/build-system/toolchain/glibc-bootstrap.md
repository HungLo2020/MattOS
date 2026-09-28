# MattOS glibc bootstrap

MattOS builds its runtime C library from imported GNU glibc source, compiled by MattOS-built tools:

```text
runtime libc and ELF loader: MattOS-built
compiler and Binutils used to build them: MattOS pass-1 GCC and cross Binutils (cross-toolchain stage)
build-time helper programs (BUILD_CC): host compiler, never installed
```

The pass-1 GCC has no C library, so glibc is configured as a cross build (`--build=x86_64-build-linux-gnu`). As a result, `make install` does not run the freshly built `ldconfig` to generate `etc/ld.so.cache`; that file was never packaged. See [MattOS native C/C++ toolchain](native-toolchain.md) for the complete toolchain order.

## Source and kernel ABI

The editable ordinary source tree is `src/system/libc/glibc/`. It has no nested Git repository. `upstream/sources.toml` and `upstream/state/glibc.toml` pin:

| Field | Value |
| --- | --- |
| Canonical project | `https://sourceware.org/glibc/` |
| Source repository used by the importer | `git://sourceware.org/git/glibc.git` |
| Stable release/tag | `glibc-2.43` |
| Exact commit | `f762ccf84f122d1354f103a151cba8bde797d521` |
| Primary development branch | `master` |
| Runtime license | LGPL-2.1-or-later, with per-file exceptions recorded by upstream |

Before configuring glibc, the build runs the kernel-supported UAPI export:

```text
make ARCH=x86 headers_install INSTALL_HDR_PATH=<repo>/out/sysroot/usr
```

The source is the imported Linux tree at revision `8ba098e6b6ff0db8edf28528d1552be261af30d4` (pinned in `upstream/sources.toml` and `upstream/state/linux.toml`). The `glibc` stage reads that revision from `upstream/state/linux.toml` and records it in its UAPI provenance files (`out/build/glibc/linux-headers-inventory.txt` and `kernel-headers-source.txt`), so they follow the pin when Linux is re-imported. Only exported UAPI headers under `out/sysroot/usr/include` are used; raw kernel-internal headers are neither copied into the sysroot nor packaged.

## Build and sysroot

glibc is built out of source in `out/build/glibc/build` and installed first into `out/build/glibc/install`. The deterministic environment is:

```text
SOURCE_DATE_EPOCH=1767225600
LC_ALL=C
TZ=UTC
libc_cv_slibdir=/usr/lib/x86_64-linux-gnu
libc_cv_rtlddir=/lib64
```

The recorded configure invocation is:

```text
src/system/libc/glibc/configure \
  --prefix=/usr \
  --libdir=/usr/lib/x86_64-linux-gnu \
  --libexecdir=/usr/libexec \
  --build=x86_64-build-linux-gnu \
  --host=x86_64-pc-linux-gnu \
  --enable-kernel=5.10.0 \
  --with-headers=<repo>/out/sysroot/usr/include \
  --without-selinux \
  --disable-werror \
  --disable-profile \
  --disable-build-nscd \
  --disable-nscd \
  --enable-stack-protector=strong \
  --enable-bind-now
```

`CC` and `CXX` are the MattOS pass-1 wrappers under `out/build/cross-toolchain/pass1-bin`, and `BUILD_CC=gcc`. The minimum supported kernel is 5.10.0. `config.make` is checked after configuration to ensure the selected system headers are the generated MattOS UAPI tree and the selected compiler is the MattOS pass-1 GCC.

The focused development sysroot is rebuilt at `out/sysroot` and contains:

```text
out/sysroot/
├── lib64/
├── usr/include/
├── usr/lib/
└── usr/lib/x86_64-linux-gnu/
```

It holds Linux UAPI headers, glibc headers, crt objects, linker scripts, runtime libraries, and the development files of source-built dependencies needed by later consumers. It contains no mutable rootfs state. The native-toolchain milestone reproduces its development surface through `linux-libc-dev`, `libc6-dev`, `mattos-libgcc-dev`, and `mattos-libstdc++-dev`; runtime DSOs remain in their existing unique owners. Every downstream native stage is cleared after a glibc build and receives explicit C, C++, linker, pkg-config, or Rust linker sysroot settings.

## Runtime packages

`libc6` is the foundational runtime package. It owns the MattOS loader at `/usr/lib64/ld-linux-x86-64.so.2` (reachable through the merged `/lib64` layout), `libc.so.6`, `libm.so.6`, `libmvec.so.1`, compatibility DSOs, resolver support, and the glibc NSS modules. Its complete shared-object inventory is recorded in `/usr/share/doc/libc6/runtime-files.tsv` with SHA-256 values.

The selected NSS/resolver inventory includes `libnss_files.so.2`, `libnss_dns.so.2`, `libnss_compat.so.2`, `libnss_db.so.2`, `libnss_hesiod.so.2`, and `libresolv.so.2`. systemd continues to provide `libnss_systemd.so.2` and `libnss_resolve.so.2`. This supports MattOS's `files systemd` account databases and `files resolve ... dns` host lookup policy.

`libc-bin` depends on `libc6` and owns `getent`, `locale`, `ldd`, and `ldconfig`. The `locales` package ships glibc's `localedef` and the locale source data under `/usr/share/i18n`; compiled locales are not packaged. Image construction generates `en_US.UTF-8` in the rootfs, and the installer generates the locale selected for the installed system. `libc6-dev` now owns the glibc headers, crt objects, static archives, and unversioned linker inputs required for native compilation, while `linux-libc-dev` uniquely owns the generated kernel UAPI layer.

`libgcc-s1` depends on `libc6`; `libstdc++6` depends on both. `mattos-bootstrap-runtime` is retired. Every other package receives a direct exact-version dependency on `libc6`, and direct compiler-runtime consumers declare the appropriate GCC runtime package. See [MattOS GCC runtime bootstrap](gcc-runtime-bootstrap.md).

## Loader migration and validation

The assembled rootfs is switched only after the build has validated representative programs with the new loader and a controlled library search path. The required set is Brush, dpkg, APT, curl, systemd PID 1, dbus-broker, login, and sudo. The final rootfs validator then inventories every ELF executable and shared object in `out/reports/elf-runtime-inventory.tsv` and rejects:

- an executable whose `PT_INTERP` is not `/lib64/ld-linux-x86-64.so.2`;
- a `DT_NEEDED` SONAME missing from the assembled rootfs;
- loader resolution to a host path;
- an unsatisfied glibc symbol-version requirement;
- duplicate or host-derived libc, libm, loader, libgcc, or libstdc++ payloads.

The validator invokes the assembled MattOS loader with `--list`; `ldd` is not trusted as the final runtime authority. The kernel is not part of this consumer rebuild because it does not link to libc. The Rust rescue init and all dynamically linked Rust userland use the explicit MattOS linker/sysroot settings and are included in the same ELF inventory.

### History: glibc migration measurements

The following figures were recorded when the glibc migration landed and are a dated snapshot, not the current image. At that time the completed inventory contained 258 ELF objects: 193 dynamic executables with the exact MattOS interpreter and 65 shared objects. Isolated `--list` checks pass for Brush, dpkg, APT, curl, systemd, dbus-broker, login, and sudo. Source and build-log checks reject direct downstream `-I/usr/include` and `-L/usr/lib` use; the only observed host library search during glibc itself is GCC's compiler-internal directory, which is part of the documented bootstrap compiler boundary.

At that time, two clean full builds produced byte-identical glibc installation trees, all 54 packages then in the set, all 57 repository files, the ELF inventory, initramfs, and ISO, and the image used a BIOS GRUB image built from the `i386-pc` modules.

Deterministic image construction still fixes file timestamps to `SOURCE_DATE_EPOCH`, uses reproducible sorted `cpio` plus headerless gzip output, and fixes ISO metadata dates. The current image's GRUB boot image is built with `grub-mkimage -O x86_64-efi` (`BOOTX64.EFI`); see `stages/image.rs`.

## Native-toolchain continuation

The GCC runtime step remains the source of the target runtime and development artifacts. The native-toolchain milestone adds source-built Binutils, GCC C/C++, and Make to the guest while retaining a documented host-bootstrap boundary. MattOS does not yet claim compiler self-reproduction or a native full-system rebuild; see [MattOS native C/C++ toolchain](native-toolchain.md).
