# MattOS GCC runtime bootstrap

MattOS builds the target GCC shared runtimes from imported GCC source while retaining a host-provided compiler toolchain as a build input. This closes the executable/runtime-library source boundary in the ISO; it is not a claim that MattOS is self-hosting.

## Source and bootstrap boundary

| Field | Value |
| --- | --- |
| Official repository | `https://gcc.gnu.org/git/gcc.git` |
| Stable release tag | `releases/gcc-15.3.0` |
| Exact commit | `4db0e8df15bef836558857c291c323add11d035c` |
| Primary development branch | `master` |
| Imported tree | `src/toolchain/gcc/` |
| Runtime license | GPL-3.0-or-later with the GCC Runtime Library Exception |

The import is ordinary editable source with no nested `.git`. `upstream/sources.toml` and `upstream/state/gcc.toml` pin the repository, release, commit, destination, and copy synchronization policy.

Host GCC/G++, assembler, linker, Make, GMP, MPFR, MPC, and zlib are bootstrap build inputs. They remain outside the target image. GCC's top-level build necessarily creates internal compiler components to build the target libraries, but MattOS selects only `libgcc_s.so.1`, `libstdc++.so.6.0.34`, and the `libstdc++.so.6` SONAME link for runtime packaging. Compiler drivers, internal executables, headers, static archives, development links, libtool files, GDB helpers, and unrelated language runtimes are excluded.

## Runtime-only build

The out-of-source build is under `out/build/gcc-runtime/build`. It targets `x86_64-pc-linux-gnu` and uses the completed MattOS sysroot for both `--with-sysroot` and `--with-build-sysroot`. The configured runtime library directory is `/usr/lib/x86_64-linux-gnu`.

The important top-level options are:

```text
--enable-languages=c,c++
--disable-bootstrap
--disable-multilib
--disable-nls
--disable-werror
--disable-checking
--disable-analyzer
--enable-shared
--enable-threads=posix
--disable-libsanitizer
--disable-libssp
--disable-libquadmath
--disable-libgomp
--disable-libatomic
--disable-libvtv
--disable-libcc1
--disable-lto
--disable-plugin
--disable-libstdcxx-pch
--without-isl
--with-system-zlib
```

Only these make targets are requested:

```text
make -j 4 all-target-libgcc all-target-libstdc++-v3
make DESTDIR=<controlled-install> install-target-libgcc install-target-libstdc++-v3
```

The complete generated invocation is recorded in `out/build/gcc-runtime/configure-invocation.txt`. `SOURCE_DATE_EPOCH`, `LC_ALL=C`, `TZ=UTC`, fixed parallelism, prefix maps, and deterministic archive behavior constrain output variation. Although upstream install targets also produce development material, the package selector copies only the validated shared runtimes into `out/build/gcc-runtime/runtime/`.

## ABI and packages

`libgcc-s1` owns `libgcc_s.so.1`, the runtime exception/license text, ABI inventory, and provenance. It depends on the exact MattOS libc runtime.

`libstdc++6` owns `libstdc++.so.6.0.34`, its `libstdc++.so.6` SONAME link, the runtime exception/license text, ABI inventory, and provenance. It depends on MattOS libc and `libgcc-s1`.

The build requires the compatibility nodes already used by MattOS consumers, including `GCC_3.0`, `GCC_4.2.0`, `GCC_14.0.0`, `GLIBCXX_3.4.34`, and `CXXABI_1.3.15`. The complete exported sets are written to `out/build/gcc-runtime/runtime-abi.tsv`. Unexpected dynamic dependencies, including GMP/MPFR/MPC/zlib leakage into the target shared runtimes, fail the build.

## Consumers and unwinding

The pre-migration ELF inventory identifies APT commands, methods, planners, solvers, `libapt-pkg`, and `libapt-private` as the direct C++ consumers. Direct `libgcc_s` consumers also include Brush, uutils/coreutils applets, sudo-rs/visudo, and the Rust rescue init. `out/reports/gcc-runtime-consumers.tsv` is authoritative for individual final paths, package owners, direct SONAME edges, and required GCC/GLIBCXX/CXXABI nodes.

Rust's existing unwind behavior is preserved. MattOS does not globally switch Rust packages to `panic=abort`; the rescue init must retain its `DT_NEEDED` entry for `libgcc_s.so.1`. The build validates a temporary C++ program that throws and catches `std::runtime_error` while using `std::string`, then uses the MattOS loader and controlled library paths for representative APT, dpkg, curl, systemd, D-Bus, Brush, sudo, login, and rescue-init objects.

## Retirement and validation

`mattos-bootstrap-runtime` is removed from the package set and dependency graph. `out/reports/bootstrap-runtime-audit.toml` remains as a historical machine-readable interface but reports a retired package, zero entries, and zero host-derived payload bytes.

Final rootfs validation compares both GCC runtime files byte-for-byte with the selected GCC build output, rejects duplicate SONAME providers, checks every interpreter and `DT_NEEDED` provider, invokes the MattOS loader for every dynamic executable, rejects host resolution and host-style RPATH/RUNPATH entries, and records GLIBC, GLIBCXX, CXXABI, and GCC version nodes in `out/reports/elf-runtime-inventory.tsv`.

The runtime-only package boundary remains unchanged, but the subsequent native-toolchain milestone packages the GCC 15 headers/static link support separately as `mattos-libgcc-dev` and `mattos-libstdc++-dev`, then adds source-built Binutils, GCC C/C++, and GNU Make. These MattOS-specific names deliberately avoid claiming Trixie's GCC 14 development-package identities. Python, Perl, Rust/rustc/Cargo, Git, general build systems, compiler self-reproduction, and native image/package construction remain future work. See `NATIVE_TOOLCHAIN.md`.
