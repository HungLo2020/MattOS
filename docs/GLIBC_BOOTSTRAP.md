# MattOS glibc bootstrap

MattOS builds its runtime C library from imported GNU glibc source. This is a runtime transition, not a self-hosting toolchain milestone:

```text
runtime libc and ELF loader: MattOS-built
compiler, assembler, linker, and compiler runtimes used to build them: host-bootstrap
```

The remaining host-derived target files are `libgcc_s.so.1` and `libstdc++.so.6`. GCC, Binutils, Rust, Python, and other build tools are not built by MattOS yet.

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

The source is the imported Linux tree at revision `f17f39c917cd4aac09db1a6a083ef5ec09b4924d`. Only exported UAPI headers under `out/sysroot/usr/include` are used; raw kernel-internal headers are neither copied into the sysroot nor packaged.

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
  --build=x86_64-pc-linux-gnu \
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

The minimum supported kernel is 5.10.0. `config.make` is checked after configuration to ensure the selected system headers are the generated MattOS UAPI tree.

The focused development sysroot is rebuilt at `out/sysroot` and contains:

```text
out/sysroot/
├── lib64/
├── usr/include/
├── usr/lib/
└── usr/lib/x86_64-linux-gnu/
```

It holds Linux UAPI headers, glibc headers, crt objects, linker scripts, runtime libraries, and the development files of source-built dependencies needed by later consumers. It contains no mutable rootfs state and is not installed as a runtime development package. Every downstream native stage is cleared after a glibc build and receives explicit C, C++, linker, pkg-config, or Rust linker sysroot settings.

## Runtime packages

`mattos-libc6` is the foundational runtime package. It owns the MattOS loader at `/usr/lib64/ld-linux-x86-64.so.2` (reachable through the merged `/lib64` layout), `libc.so.6`, `libm.so.6`, `libmvec.so.1`, compatibility DSOs, resolver support, and the glibc NSS modules. Its complete shared-object inventory is recorded in `/usr/share/doc/mattos-libc6/runtime-files.tsv` with SHA-256 values.

The selected NSS/resolver inventory includes `libnss_files.so.2`, `libnss_dns.so.2`, `libnss_compat.so.2`, `libnss_db.so.2`, `libnss_hesiod.so.2`, and `libresolv.so.2`. systemd continues to provide `libnss_systemd.so.2` and `libnss_resolve.so.2`. This supports MattOS's `files systemd` account databases and `files resolve ... dns` host lookup policy.

`mattos-libc-bin` depends on `mattos-libc6` and owns `getent`, `locale`, `ldd`, and `ldconfig`. Locale data is not bulk-packaged. There is no `mattos-libc6-dev` in the runtime image; headers, crt objects, static archives, and unversioned linker inputs remain build/sysroot-only.

`mattos-bootstrap-runtime` now provides the `mattos-bootstrap-gcc-runtime` virtual boundary, depends on `mattos-libc6`, and owns only host-derived `libgcc_s.so.1` and `libstdc++.so.6`. `mattos-libc6` does not depend on it, so the graph is acyclic. Every other package receives a direct exact-version dependency on `mattos-libc6`; packages needing the compiler runtimes additionally retain the bootstrap-runtime dependency found by ELF analysis.

## Loader migration and validation

The assembled rootfs is switched only after the build has validated representative programs with the new loader and a controlled library search path. The required set is Brush, dpkg, APT, curl, systemd PID 1, dbus-broker, login, and sudo. The final rootfs validator then inventories every ELF executable and shared object in `out/reports/elf-runtime-inventory.tsv` and rejects:

- an executable whose `PT_INTERP` is not `/lib64/ld-linux-x86-64.so.2`;
- a `DT_NEEDED` SONAME missing from the assembled rootfs;
- loader resolution to a host path;
- an unsatisfied glibc symbol-version requirement;
- duplicate or host-derived libc, libm, or loader payloads.

The validator invokes the assembled MattOS loader with `--list`; `ldd` is not trusted as the final runtime authority. The kernel is not part of this consumer rebuild because it does not link to libc. The Rust rescue init and all dynamically linked Rust userland use the explicit MattOS linker/sysroot settings and are included in the same ELF inventory.

The completed inventory contains 258 ELF objects: 193 dynamic executables with the exact MattOS interpreter and 65 shared objects. Isolated `--list` checks pass for Brush, dpkg, APT, curl, systemd, dbus-broker, login, and sudo. Source and build-log checks reject direct downstream `-I/usr/include` and `-L/usr/lib` use; the only observed host library search during glibc itself is GCC's compiler-internal directory, which is part of the documented bootstrap compiler boundary.

Two clean full builds produced byte-identical glibc installation trees, all 54 packages, all 57 repository files, the ELF inventory, initramfs, and ISO. Deterministic image construction fixes file timestamps to `SOURCE_DATE_EPOCH`, uses reproducible sorted `cpio` plus headerless gzip output, fixes ISO metadata dates, and emits the supported BIOS GRUB image from the `i386-pc` modules.

## Remaining toolchain milestone

The next coordinated runtime step is to build and package GCC's `libgcc_s` and `libstdc++` ABIs, then rebuild their consumers. A later compiler/Binutils bootstrap can remove the host build-tool boundary. Until both are complete, MattOS accurately describes itself as using a MattOS-built libc runtime with a host-bootstrap compiler toolchain, not as self-hosting.
