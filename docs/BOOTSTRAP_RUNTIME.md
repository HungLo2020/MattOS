# Bootstrap runtime audit

`mattos-bootstrap-runtime` is a transitional closure of host-derived runtime files. It is not a libc package and does not make MattOS self-hosting. The complete pre-migration machine report is generated at `out/reports/bootstrap-runtime-audit.toml` by:

```text
cargo run -p mattos-build -- package audit
```

The report records every regular file and symlink with its path, type, size, mode, target, SHA-256, `file` description, ELF type and interpreter, SONAME, `readelf` and `objdump` dependencies, `ldd` resolution, host package found by `dpkg-query -S`, separately inferred upstream project, source availability, concrete package/ELF consumers, reason, proposed package, difficulty, confidence, classification, and boundary group.

## Baseline and classification

The audited pre-migration payload contained 23 files and 17,600,184 bytes. Every source-project attribution is explicitly `inferred`; host binary-package ownership is separately confirmed from this build host. Attribution confidence is high for all known entries.

| Class | Count | Meaning in this snapshot |
| --- | ---: | --- |
| A | 2 | Expat and libcap source was imported and buildable by MattOS but ownership still came from the host |
| B | 0 | no imported-but-unbuilt sources |
| C | 15 | small or separable projects not yet imported |
| D | 5 | glibc, loader, GCC runtime, or C++ runtime boundary |
| E | 0 | no unidentified source projects |
| F | 1 | host `/usr/bin/tar`, a standalone utility that belongs in a future `mattos-tar` package |

Both category A entries were ownership defects and were migrated here. The category F entry remains only because source-built dpkg needs `tar` to extract archives; it is recorded rather than disguised as a library.

## Complete pre-migration inventory

| File | Class | Inferred project | Boundary | Direct consumer packages |
| --- | --- | --- | --- | --- |
| `/usr/bin/tar` | F | GNU tar | standalone utility | none; invoked by dpkg |
| `libacl.so.1` | C | Linux ACL utilities | leaf | bootstrap `tar` |
| `libbsd.so.0` | C | libbsd | leaf | Shadow |
| `libbz2.so.1.0` | C | bzip2 | compression | dpkg, libapt-pkg |
| `libc.so.6` | D | glibc | glibc | nearly every ELF package; full paths are in the report |
| `libcap.so.2` | A | libcap | leaf | iproute2 (`ip`, `ss`, `bridge`, `tc`) |
| `libcrypt.so.1` | C | libxcrypt | widely shared | PAM runtime/modules, Shadow |
| `libcrypto.so.3` | C | OpenSSL | crypto | bootstrap `tar` closure, curl, libapt-pkg |
| `libelf.so.1` | C | elfutils | leaf | iproute2 |
| `libexpat.so.1` | A | Expat | leaf | dbus-broker launcher |
| `libgcc_s.so.1` | D | GCC runtime | compiler runtime | APT, bootstrap closure, Brush, coreutils, libapt-pkg, sudo-rs |
| `liblz4.so.1` | C | LZ4 | compression | libapt-pkg |
| `liblzma.so.5` | C | XZ Utils | compression | dpkg, libapt-pkg |
| `libm.so.6` | D | glibc | glibc | bootstrap closure, Brush, coreutils, iproute2, libapt-pkg |
| `libmd.so.0` | C | libmd | leaf | bootstrap `tar` closure, dpkg |
| `libpcre2-8.so.0` | C | PCRE2 | leaf | bootstrap `tar` closure |
| `libselinux.so.1` | C | SELinux userspace | widely shared | bootstrap `tar` closure, dpkg, iproute2 |
| `libssl.so.3` | C | OpenSSL | crypto | curl |
| `libstdc++.so.6` | D | GCC libstdc++ runtime | C++ runtime | APT, libapt-pkg |
| `libxxhash.so.0` | C | xxHash | compression | bootstrap `tar` closure, libapt-pkg |
| `libz.so.1` | C | zlib | compression | bootstrap `tar` closure, dpkg, libapt-pkg |
| `libzstd.so.1` | C | Zstandard | compression | bootstrap `tar` closure, dpkg, libapt-pkg |
| `ld-linux-x86-64.so.2` | D | glibc | glibc | all dynamically loaded executables; representative paths are in the report |

There are no data-file entries in this closure. “Widely shared” means more than two package consumers. The machine report retains every individual consumer path rather than collapsing it to the package summaries above.

## Selected migration

The focused selection is Expat and libcap. Both were already present, actually consumed, small independent builds, and did not cross the libc or toolchain boundary. OpenSSL was deliberately excluded because the crypto boundary is substantially larger.

| Project | Official repository | Branch | Exact imported commit | Runtime package | Rebuilt consumer |
| --- | --- | --- | --- | --- | --- |
| Expat | `https://github.com/libexpat/libexpat.git` | `master` | `236c3f8f949209501b568032553c17577901c7ec` | `mattos-libexpat1` | `mattos-dbus-broker` |
| libcap | `https://git.kernel.org/pub/scm/libs/libcap/libcap.git` | `master` | `bd54ca54ff9fc963954f11ffd9acffbaf1447723` | `mattos-libcap2` | `mattos-iproute2` |

Both imports are editable ordinary files under `src/system/libraries/`, have exact state in `upstream/state/`, and contain no nested Git repository. Builds are out-of-source under `out/build/`. Consumer builds use MattOS headers, pkg-config files, and libraries; post-build `ldd` validation rejects resolution outside those build trees.

The runtime packages contain only the versioned shared object, SONAME symlink, license, and provenance. `mattos-dbus-broker` has an exact dependency on `mattos-libexpat1`; `mattos-iproute2` has an exact dependency on `mattos-libcap2`.

## Result and remaining order

After migration the bootstrap manifest contains 21 files and 17,365,960 bytes. It no longer contains `libexpat.so.1` or `libcap.so.2`; their 234,224 bytes moved to dedicated packages. The builder rejects their reappearance, duplicate SONAME ownership, unowned `DT_NEEDED` entries, and dependency metadata that omits an owning package.

Recommended next order is: move standalone GNU tar; then low-risk leaf/compression projects (`libacl`, libmd/libbsd, bzip2, LZ4, XZ, zlib, Zstandard, xxHash); then PCRE2/libxcrypt/elfutils; then SELinux userspace; then the OpenSSL boundary. glibc, its ELF loader, libgcc, and libstdc++ remain a later toolchain milestone and are explicitly out of scope here.
