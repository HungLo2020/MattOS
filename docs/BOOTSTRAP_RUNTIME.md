# Bootstrap runtime audit

The host-derived runtime closure is complete. `mattos-bootstrap-runtime` has been retired from the package set and dependency graph after glibc, `libgcc_s`, and `libstdc++` moved to source-built MattOS packages. MattOS still uses host build tools and does not claim self-hosting. The historical machine-report interface is generated at `out/reports/bootstrap-runtime-audit.toml` by:

```text
cargo run -p mattos-build -- package audit
```

The successful final report identifies the package as retired and records zero entries and zero payload bytes. The richer per-file schema remains documented by the earlier snapshots below.

## Baseline and classification

The original audited payload contained 23 files and 17,600,184 bytes. Expat/libcap reduced it to 21 files and 17,365,960 bytes, GNU tar/ACL/zlib/bzip2 reduced it to 17 files and 16,669,816 bytes, LZ4/liblzma/xxHash reduced it to 14 files and 16,191,736 bytes, libmd/libbsd reduced it to 12 files and 16,042,648 bytes, and the coordinated OpenSSL/elfutils/Zstandard migration reduced it to 8 files and 7,639,680 bytes. PCRE2, SELinux userspace, and libxcrypt leave 5 files and 6,518,032 bytes. Every source-project attribution is explicitly `inferred`; host binary-package ownership is separately confirmed from this build host. Attribution confidence is high for all known entries.

| Class | Count | Meaning in this snapshot |
| --- | ---: | --- |
| A | 0 | every imported source represented by this audit has moved to package ownership |
| B | 0 | no imported-but-unbuilt sources |
| C | 0 | all separable pre-toolchain libraries are package-owned |
| D | 0 | glibc, loader, GCC runtime, and C++ runtime are source-built |
| E | 0 | no unidentified source projects |
| F | 0 | standalone GNU tar is now source-built and package-owned |

The current machine report contains no entries. GNU tar, every separable library, glibc, the ELF loader, libgcc, and libstdc++ are package-owned source builds.

## Previous final-bootstrap inventory

| File | Class | Inferred project | Boundary | Direct consumer packages |
| --- | --- | --- | --- | --- |
| `libc.so.6` | D | glibc | glibc | moved to `libc6` |
| `libgcc_s.so.1` | D | GCC runtime | compiler runtime | moved to `libgcc-s1` |
| `libm.so.6` | D | glibc | glibc | moved to `libc6` |
| `libstdc++.so.6` | D | GCC libstdc++ runtime | C++ runtime | moved to `libstdc++6` |
| `ld-linux-x86-64.so.2` | D | glibc | glibc | moved to `libc6` |

There are no data-file entries in this closure. “Widely shared” means more than two package consumers. The machine report retains every individual consumer path rather than collapsing it to the package summaries above.

## Completed migrations

The focused selection is Expat and libcap. Both were already present, actually consumed, small independent builds, and did not cross the libc or toolchain boundary. OpenSSL was deliberately excluded because the crypto boundary is substantially larger.

| Project | Official repository | Branch | Exact imported commit | Runtime package | Rebuilt consumer |
| --- | --- | --- | --- | --- | --- |
| Expat | `https://github.com/libexpat/libexpat.git` | `master` | `236c3f8f949209501b568032553c17577901c7ec` | `libexpat1` | `dbus-broker` |
| libcap | `https://git.kernel.org/pub/scm/libs/libcap/libcap.git` | `master` | `bd54ca54ff9fc963954f11ffd9acffbaf1447723` | `libcap2` | `iproute2` |

Both imports are editable ordinary files under `src/system/libraries/`, have exact state in `upstream/state/`, and contain no nested Git repository. Builds are out-of-source under `out/build/`. Consumer builds use MattOS headers, pkg-config files, and libraries; post-build `ldd` validation rejects resolution outside those build trees.

The runtime packages contain only the versioned shared object, SONAME symlink, license, and provenance. `dbus-broker` has an exact dependency on `libexpat1`; `iproute2` has an exact dependency on `libcap2`.

The next focused selection moved GNU tar, ACL, zlib, and bzip2. They were present in the 21-entry audit, have small official builds and identifiable consumers, and do not cross the libc/toolchain boundary.

| Project | Official repository | Release | Exact imported commit | Runtime package | Rebuilt direct consumers |
| --- | --- | --- | --- | --- | --- |
| GNU tar | `https://git.savannah.gnu.org/git/tar.git` | `v1.35` | `e545d446dfe6564265cdf4186641ee76f4acc7fa` | `tar` | dpkg extraction contract |
| ACL | `https://git.savannah.nongnu.org/git/acl.git` | `v2.3.2` | `214c7d146945c31a9dc04cb7094b85053f52a21e` | `libacl1` | GNU tar |
| zlib | `https://github.com/madler/zlib.git` | `v1.3.2` | `da607da739fa6047df13e66a2af6b8bec7c2a498` | `zlib1g` | dpkg and APT; curl/OpenSSL and iproute2/libelf closures declare it |
| bzip2 | `https://sourceware.org/git/bzip2.git` | `bzip2-1.0.8` | `6a8690fc8d26c815e798c588f796eabe9d684cf0` | `libbz2-1.0` | dpkg and APT |

GNU tar's release bootstrap references paxutils. MattOS imports that build support as ordinary files, not a submodule, pinned to commit `481bae11050fcbdca67a66eb57390267b280a312`. Tar is configured with POSIX ACL support and without SELinux, links only to the MattOS ACL build plus libc, and owns `/usr/bin/tar`. dpkg and APT are rebuilt against the MattOS zlib and bzip2 build trees; configure-cache and `ldd` checks reject host fallback.

The compression selection imported all four requested projects and built their real shared-library ABIs outside the source trees:

| Project | Official repository | Release | Exact imported commit | Runtime result | Rebuilt direct consumers |
| --- | --- | --- | --- | --- | --- |
| LZ4 | `https://github.com/lz4/lz4.git` | `v1.10.0` | `ebb370ca83af193212df4dcbadcc5d87bc0de2f0` | `liblz4-1` | libapt-pkg |
| XZ Utils | `https://github.com/tukaani-project/xz.git` | `v5.8.1` | `a522a226545730551f7e7c2685fab27cf567746c` | `liblzma5` | dpkg-deb and libapt-pkg |
| xxHash | `https://github.com/Cyan4973/xxHash.git` | `v0.8.3` | `e626a72bc2321cd320e953a0ccf1584cad60f363` | `libxxhash0` | libapt-pkg |
| Zstandard | `https://github.com/facebook/zstd.git` | `v1.5.7` | `f8745da6ff1ad1e7bab384bd1f9d742439278e99` | source-built `libzstd.so.1` is build-ready; runtime migration deferred | none in this milestone |

dpkg's Autotools build receives explicit MattOS XZ include, linker, pkg-config, and runtime-library paths. APT's CMake build receives exact `LZ4_*`, `LZMA_*`, and `XXHASH_*` cache paths plus the earlier zlib/bzip2 paths. Cache validation and post-build `ldd` checks reject host fallback. The three runtime packages ship only the versioned library, SONAME symlink, license, and provenance; headers, static archives, pkg-config files, tools, and linker-name symlinks stay in build-only install trees.

At that intermediate milestone, Zstandard was a legitimate layering blocker rather than a forced fourth package. The source-built library needed libc from `mattos-bootstrap-runtime`, while bootstrap-owned `libcrypto.so.3` and `libelf.so.1` directly needed `libzstd.so.1`. Making `mattos-bootstrap-runtime` depend on `libzstd1` while that package depended back on the bootstrap runtime would have created the dependency cycle the repository correctly rejects. The coordinated migration below resolves that cycle.

The libmd/libbsd milestone imports the stable releases as ordinary editable files and builds both with their upstream Autotools systems:

| Project | Official repository | Primary branch / release | Exact imported commit | License | ABI package |
| --- | --- | --- | --- | --- | --- |
| libmd | `https://git.hadrons.org/git/libmd.git` | `main` / `1.2.0` | `90c4f432134c608c7e2b4dd0a1d7ca5c40b92c7a` | BSD-3-Clause primary; mixed permissive notices in `COPYING` | `libmd0` (`libmd.so.0`) |
| libbsd | `https://gitlab.freedesktop.org/libbsd/libbsd.git` | `main` / `0.12.2` | `04a24db27ad1572f766bad772cdd9c146e6d9cf0` | BSD-3-Clause primary; mixed permissive notices in `COPYING` | `libbsd0` (`libbsd.so.0`) |

libmd is built first under `out/build/libmd/`. libbsd is then configured under `out/build/libbsd/` with explicit libmd `CPPFLAGS`, `LDFLAGS`, `LIBRARY_PATH`, `LD_LIBRARY_PATH`, and `PKG_CONFIG_PATH`. The build-only libtool archives are removed, and libbsd's build-only absolute linker script is replaced by a staged-tree symlink so later links cannot resolve `/usr/lib/x86_64-linux-gnu/libmd`. Post-build checks require libbsd's `DT_NEEDED` entry for `libmd.so.0` to resolve from `out/build/libmd/`.

The direct libmd consumers are `/usr/bin/dpkg`, `dpkg-deb`, `dpkg-divert`, `dpkg-query`, `dpkg-realpath`, `dpkg-split`, `dpkg-statoverride`, and `dpkg-trigger`. The direct libbsd consumers are `/usr/bin/chage`, `newgrp`, and `passwd`, plus `/usr/sbin/chpasswd`, `groupadd`, `groupdel`, `groupmod`, `useradd`, `userdel`, and `usermod`. dpkg receives explicit libmd header, linker, pkg-config, and runtime search paths. Shadow is configured with `--with-libbsd`, `LIBBSD_CFLAGS`, `LIBBSD_LIBS`, and explicit libbsd/libmd include and library paths. Build-time and assembled-rootfs loader checks reject a host resolution for every listed consumer.

The runtime packages contain only their versioned shared object, SONAME link, license, and provenance. `libbsd0` depends exactly on `libmd0`; `dpkg` depends on `libmd0`; and `passwd` declares both libraries because its validated runtime closure includes both. The bootstrap manifest rejects either SONAME's reappearance, and rootfs ownership is unique to the dedicated packages.

The coordinated crypto/ELF/compression migration imports OpenSSL and elfutils and completes the already imported Zstandard runtime split:

| Project | Official repository | Release | Exact imported commit | License | ABI package |
| --- | --- | --- | --- | --- | --- |
| Zstandard | `https://github.com/facebook/zstd.git` | `v1.5.7` | `f8745da6ff1ad1e7bab384bd1f9d742439278e99` | BSD-3-Clause / GPL-2.0-only dual choice | `libzstd1` (`libzstd.so.1`) |
| OpenSSL | `https://github.com/openssl/openssl.git` | `openssl-3.5.7` | `8cf17aaeb4599f8af87fefd810b5b5fee90fe69e` | Apache-2.0 | `mattos-libcrypto3` and `libssl3t64` |
| elfutils | `https://sourceware.org/git/elfutils.git` | `elfutils-0.195` | `302252356da5475670ac5b10dadd091c59689425` | GPL-2.0-or-later and LGPL-3.0-or-later library terms | `libelf1t64` (`libelf.so.1`) |

Zstandard is built first. OpenSSL is configured for `linux-x86_64`, shared libraries, zlib and Zstandard support, `/usr`, `/usr/lib/x86_64-linux-gnu`, and `OPENSSLDIR=/etc/ssl`; applications, tests, documentation, the legacy provider, and loadable modules are disabled. The default provider is therefore built into `libcrypto`, and no provider module or OpenSSL configuration file is needed in the runtime packages. curl is rebuilt with certificate verification pinned to `/etc/ssl/certs/ca-certificates.crt` and no default CA directory.

elfutils builds only its libraries and `libelf`, with the MattOS zlib and Zstandard builds enabled and unused bzip2/liblzma support disabled. The direct migrated consumers are `libssl`/libapt-pkg/libcurl for `libcrypto`, libcurl for `libssl`, `ip`/`tc` for `libelf`, and libcrypto/libelf/libapt-pkg/dpkg-deb for Zstandard. Explicit include, pkg-config, linker, and runtime paths force each build to the staged MattOS dependencies. Post-build and assembled-rootfs loader checks reject host fallback.

The four ABI packages contain only runtime shared objects, SONAME links, licenses, and provenance. Their dependency order is acyclic: Zstandard depends on the bootstrap C runtime; libcrypto depends on Zstandard and zlib; libssl depends on the exact libcrypto ABI; and libelf depends on Zstandard and zlib. curl, APT, dpkg, and iproute2 declare their exact owning packages. This releases the prior cycle because OpenSSL and elfutils no longer remain inside the bootstrap package while consuming package-owned Zstandard.

## Result and remaining order

The final pre-toolchain migration uses these ordinary editable imports:

| Project | Official repository | Release / branch | Exact imported commit | License | Build system | ABI package |
| --- | --- | --- | --- | --- | --- | --- |
| PCRE2 | `https://github.com/PCRE2Project/pcre2.git` | `pcre2-10.47` | `f454e231fe5006dd7ff8f4693fd2b8eb94333429` | BSD-3-Clause with PCRE2 exception | CMake/Ninja | `libpcre2-8-0` |
| SLJIT build support | `https://github.com/zherczeg/sljit.git` | `master` | `45f910b78c6605ebf5b53d3ec7cb00f2312fe417` | BSD-2-Clause | compiled as PCRE2 JIT source | build-only, no package |
| SELinux userspace | `https://github.com/SELinuxProject/selinux.git` | `3.10` | `ca10fc4204ed60540d41d2499127c18ad0643f9e` | libselinux public-domain terms | upstream Makefiles | `libselinux1` |
| libxcrypt | `https://github.com/besser82/libxcrypt.git` | `v4.4.38` | `55ea777e8d567e5e86ffac917c28815ac54cc341` | LGPL-2.1-or-later overall; per-file exceptions | Autotools | `libcrypt1` |

SLJIT is an ordinary, non-submodule import. No imported tree contains a nested Git repository.

The exact direct graph is PCRE2 to `libselinux.so.1`; SELinux to dpkg, dpkg-statoverride, ip, and ss; and libcrypt to pam_unix, unix_chkpwd, newgrp, passwd, and chpasswd. All are package-owned. util-linux mount was also rebuilt from source with its SELinux compatibility loader enabled; its source-built libblkid, libmount, libsmartcols, mount, and umount closure has dedicated package ownership instead of the former host-copy path. SELinux policy, enforcement, relabeling, and policy tools remain absent.

libxcrypt is built with all hash algorithms and glibc-compatible obsolete APIs. Its test suite covers yescrypt, and the installed ABI exports `GLIBC_2.2.5`, `XCRYPT_2.0`, `XCRYPT_4.3`, and `XCRYPT_4.4`, satisfying the exact PAM and Shadow references. The three target runtime packages contain only runtime shared objects/SONAME links, license, and provenance.

That pre-glibc bootstrap manifest contained 5 files and 6,518,032 bytes. The glibc milestone moved three files to `libc6`, leaving 2 files and 2,832,624 bytes. The GCC runtime milestone moves those last files to `libgcc-s1` and `libstdc++6`, removes the transitional package, and produces a zero-entry audit.

All dynamically linked target userspace now uses MattOS-built glibc and GCC runtime libraries. Host compiler, assembler, linker, and packaging tools remain bootstrap inputs outside the ISO, so MattOS still does not claim self-hosting. See `GLIBC_BOOTSTRAP.md` and `GCC_RUNTIME_BOOTSTRAP.md` for the exact boundaries.
