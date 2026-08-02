# Bootstrap runtime audit

`mattos-bootstrap-runtime` is a transitional closure of host-derived runtime files. It is not a libc package and does not make MattOS self-hosting. The complete pre-migration machine report is generated at `out/reports/bootstrap-runtime-audit.toml` by:

```text
cargo run -p mattos-build -- package audit
```

The report records every regular file and symlink with its path, type, size, mode, target, SHA-256, `file` description, ELF type and interpreter, SONAME, `readelf` and `objdump` dependencies, `ldd` resolution, host package found by `dpkg-query -S`, separately inferred upstream project, source availability, concrete package/ELF consumers, reason, proposed package, difficulty, confidence, classification, and boundary group.

## Baseline and classification

The original audited payload contained 23 files and 17,600,184 bytes. Expat/libcap reduced it to 21 files and 17,365,960 bytes, and GNU tar/ACL/zlib/bzip2 reduced it to 17 files and 16,669,816 bytes. The current APT/dpkg compression milestone removes LZ4, liblzma, and xxHash, leaving 14 files and 16,191,736 bytes. Every source-project attribution is explicitly `inferred`; host binary-package ownership is separately confirmed from this build host. Attribution confidence is high for all known entries.

| Class | Count | Meaning in this snapshot |
| --- | ---: | --- |
| A | 0 | every imported source represented by this audit has moved to package ownership |
| B | 0 | no imported-but-unbuilt sources |
| C | 9 | separable projects still in the bootstrap boundary |
| D | 5 | glibc, loader, GCC runtime, or C++ runtime boundary |
| E | 0 | no unidentified source projects |
| F | 0 | standalone GNU tar is now source-built and package-owned |

The current machine report contains only categories C and D. GNU tar, ACL, zlib, bzip2, LZ4, liblzma, and xxHash no longer appear in this host-derived boundary.

## Complete remaining inventory

| File | Class | Inferred project | Boundary | Direct consumer packages |
| --- | --- | --- | --- | --- |
| `libbsd.so.0` | C | libbsd | leaf | Shadow |
| `libc.so.6` | D | glibc | glibc | nearly every ELF package; full paths are in the report |
| `libcrypt.so.1` | C | libxcrypt | widely shared | PAM runtime/modules, Shadow |
| `libcrypto.so.3` | C | OpenSSL | crypto | curl, libapt-pkg |
| `libelf.so.1` | C | elfutils | leaf | iproute2 |
| `libgcc_s.so.1` | D | GCC runtime | compiler runtime | APT, bootstrap closure, Brush, coreutils, libapt-pkg, sudo-rs |
| `libm.so.6` | D | glibc | glibc | bootstrap closure, Brush, coreutils, iproute2, libapt-pkg |
| `libmd.so.0` | C | libmd | leaf | Shadow, dpkg |
| `libpcre2-8.so.0` | C | PCRE2 | leaf | SELinux closure |
| `libselinux.so.1` | C | SELinux userspace | widely shared | dpkg, iproute2 |
| `libssl.so.3` | C | OpenSSL | crypto | curl |
| `libstdc++.so.6` | D | GCC libstdc++ runtime | C++ runtime | APT, libapt-pkg |
| `libzstd.so.1` | C | Zstandard | compression | direct consumers include `libcrypto.so.3`, `libelf.so.1`, dpkg-deb, and libapt-pkg; full transitive paths are in the report |
| `ld-linux-x86-64.so.2` | D | glibc | glibc | all dynamically loaded executables; representative paths are in the report |

There are no data-file entries in this closure. “Widely shared” means more than two package consumers. The machine report retains every individual consumer path rather than collapsing it to the package summaries above.

## Completed migrations

The focused selection is Expat and libcap. Both were already present, actually consumed, small independent builds, and did not cross the libc or toolchain boundary. OpenSSL was deliberately excluded because the crypto boundary is substantially larger.

| Project | Official repository | Branch | Exact imported commit | Runtime package | Rebuilt consumer |
| --- | --- | --- | --- | --- | --- |
| Expat | `https://github.com/libexpat/libexpat.git` | `master` | `236c3f8f949209501b568032553c17577901c7ec` | `mattos-libexpat1` | `mattos-dbus-broker` |
| libcap | `https://git.kernel.org/pub/scm/libs/libcap/libcap.git` | `master` | `bd54ca54ff9fc963954f11ffd9acffbaf1447723` | `mattos-libcap2` | `mattos-iproute2` |

Both imports are editable ordinary files under `src/system/libraries/`, have exact state in `upstream/state/`, and contain no nested Git repository. Builds are out-of-source under `out/build/`. Consumer builds use MattOS headers, pkg-config files, and libraries; post-build `ldd` validation rejects resolution outside those build trees.

The runtime packages contain only the versioned shared object, SONAME symlink, license, and provenance. `mattos-dbus-broker` has an exact dependency on `mattos-libexpat1`; `mattos-iproute2` has an exact dependency on `mattos-libcap2`.

The next focused selection moved GNU tar, ACL, zlib, and bzip2. They were present in the 21-entry audit, have small official builds and identifiable consumers, and do not cross the libc/toolchain boundary.

| Project | Official repository | Release | Exact imported commit | Runtime package | Rebuilt direct consumers |
| --- | --- | --- | --- | --- | --- |
| GNU tar | `https://git.savannah.gnu.org/git/tar.git` | `v1.35` | `e545d446dfe6564265cdf4186641ee76f4acc7fa` | `mattos-tar` | dpkg extraction contract |
| ACL | `https://git.savannah.nongnu.org/git/acl.git` | `v2.3.2` | `214c7d146945c31a9dc04cb7094b85053f52a21e` | `mattos-libacl1` | GNU tar |
| zlib | `https://github.com/madler/zlib.git` | `v1.3.2` | `da607da739fa6047df13e66a2af6b8bec7c2a498` | `mattos-zlib1g` | dpkg and APT; curl/OpenSSL and iproute2/libelf closures declare it |
| bzip2 | `https://sourceware.org/git/bzip2.git` | `bzip2-1.0.8` | `6a8690fc8d26c815e798c588f796eabe9d684cf0` | `mattos-libbz2-1.0` | dpkg and APT |

GNU tar's release bootstrap references paxutils. MattOS imports that build support as ordinary files, not a submodule, pinned to commit `481bae11050fcbdca67a66eb57390267b280a312`. Tar is configured with POSIX ACL support and without SELinux, links only to the MattOS ACL build plus libc, and owns `/usr/bin/tar`. dpkg and APT are rebuilt against the MattOS zlib and bzip2 build trees; configure-cache and `ldd` checks reject host fallback.

The current selection imported all four requested compression projects and built their real shared-library ABIs outside the source trees:

| Project | Official repository | Release | Exact imported commit | Runtime result | Rebuilt direct consumers |
| --- | --- | --- | --- | --- | --- |
| LZ4 | `https://github.com/lz4/lz4.git` | `v1.10.0` | `ebb370ca83af193212df4dcbadcc5d87bc0de2f0` | `mattos-liblz4-1` | libapt-pkg |
| XZ Utils | `https://github.com/tukaani-project/xz.git` | `v5.8.1` | `a522a226545730551f7e7c2685fab27cf567746c` | `mattos-liblzma5` | dpkg-deb and libapt-pkg |
| xxHash | `https://github.com/Cyan4973/xxHash.git` | `v0.8.3` | `e626a72bc2321cd320e953a0ccf1584cad60f363` | `mattos-libxxhash0` | libapt-pkg |
| Zstandard | `https://github.com/facebook/zstd.git` | `v1.5.7` | `f8745da6ff1ad1e7bab384bd1f9d742439278e99` | source-built `libzstd.so.1` is build-ready; runtime migration deferred | none in this milestone |

dpkg's Autotools build receives explicit MattOS XZ include, linker, pkg-config, and runtime-library paths. APT's CMake build receives exact `LZ4_*`, `LZMA_*`, and `XXHASH_*` cache paths plus the earlier zlib/bzip2 paths. Cache validation and post-build `ldd` checks reject host fallback. The three runtime packages ship only the versioned library, SONAME symlink, license, and provenance; headers, static archives, pkg-config files, tools, and linker-name symlinks stay in build-only install trees.

Zstandard is a legitimate layering blocker rather than a forced fourth package. The source-built library needs libc from `mattos-bootstrap-runtime`, while bootstrap-owned `libcrypto.so.3` and `libelf.so.1` directly need `libzstd.so.1`. Making `mattos-bootstrap-runtime` depend on `mattos-libzstd1` while that package depends back on the bootstrap runtime would create the dependency cycle the repository correctly rejects. Splitting OpenSSL and elfutils is outside this milestone, so the host-derived Zstandard ABI remains uniquely owned by `mattos-bootstrap-runtime`.

## Result and remaining order

After this migration the bootstrap manifest contains 14 files and 16,191,736 bytes, down by three entries and 478,080 bytes from the preceding 17-file boundary. It no longer contains `liblz4.so.1`, `liblzma.so.5`, or `libxxhash.so.0`. The builder rejects their reappearance, duplicate SONAME ownership, unowned `DT_NEEDED` entries, host resolution in rebuilt consumers, and dependency metadata that omits an owning package.

Recommended next order is libmd/libbsd, then a planned OpenSSL/elfutils split that can release the Zstandard cycle, then PCRE2/libxcrypt/SELinux userspace. glibc, its ELF loader, libgcc, and libstdc++ remain a later toolchain milestone. MattOS does not claim self-hosting.
