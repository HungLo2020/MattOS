# MattOS native C/C++ toolchain

This page covers the source-built GCC/Binutils/Make toolchain: the first
self-hosting capability layer, not a claim that MattOS can rebuild itself.  An
installed MattOS system receives enough source-built tools and development
files to compile, assemble, link, run, and package small C and C++ projects.

The toolchain packages are not part of the live image.  They are dependencies
of the `mattos-toolchain` meta package, which the installer adds to every
installed profile (`MATTOS_TOOLCHAIN_PACKAGES` and `live_excluded_packages` in
`src/tools/mattos-build/src/packaging/registry.rs`).  The same meta package
also pulls in the LLVM/Clang/LLD and Rust/Cargo packages described in
[Self-hosting development foundation](self-hosting.md).

## Pinned sources

The ordinary editable source trees contain no nested Git repositories.
`upstream/sources.toml` and the corresponding files under `upstream/state/`
record the canonical source and exact imported commit.

| Component | Canonical repository | MattOS source | Upstream selection | Imported commit |
| --- | --- | --- | --- | --- |
| Linux UAPI | `https://github.com/torvalds/linux.git` | `src/kernel/linux` | `master` (existing pinned Linux source) | `8ba098e6b6ff0db8edf28528d1552be261af30d4` |
| glibc | `git://sourceware.org/git/glibc.git` | `src/system/libc/glibc` | `glibc-2.43` | `f762ccf84f122d1354f103a151cba8bde797d521` |
| GCC | `https://gcc.gnu.org/git/gcc.git` | `src/toolchain/gcc` | `releases/gcc-15.3.0` | `4db0e8df15bef836558857c291c323add11d035c` |
| GNU Binutils | `https://sourceware.org/git/binutils-gdb.git` | `src/toolchain/binutils` | `binutils-2_46_1` | `5e56594815854de5eca35c7c04b11705d0f19c02` |
| GNU Make | `https://git.savannah.gnu.org/git/make.git` | `src/build-tools/make` | `4.4.1` | `d66a65ad5a0e31b287f53930b0f09e31801f1613` |
| Gnulib for Make bootstrap | `https://git.savannah.gnu.org/git/gnulib.git` | `src/build-support/gnulib` | `stable-202301` | `20932856a6a07f056918d58acd09cea4ba150a52` |

Binutils and GCC are GPL-3.0-or-later projects with separately licensed runtime
pieces; GNU Make is GPL-3.0-or-later.  Make's repository bootstrap is generated
from its declared `stable-202301` Gnulib revision with `--no-git`; it never
clones a moving Gnulib branch during a build and works from a warmed tree while
offline.  The packages carry the relevant license files from each imported
source tree.

## Bootstrap boundary and triples

All MattOS target code (glibc, the kernel, the GCC runtimes, and every
package) is compiled by GCC and Binutils built from the imported sources.  The
host compiler is a stage-0 input with exactly three uses, none of which reaches
the image:

1. building the `cross-toolchain` stage below;
2. building the host-running compiler proper inside the `gcc-runtime` stage;
3. compiling build-machine helpers that execute during a build
   (`CC_FOR_BUILD`, glibc `BUILD_CC`, Kbuild `HOSTCC`).

The verified primary host tools were GCC/G++ 15.2.0 and GNU Make 4.4.1.
GCC's build also uses setup-managed host utilities such as POSIX shell tools,
Perl, Bison, Flex, M4, and Texinfo.  No host executable or runtime-loaded
library is copied into the image.

The toolchain is built in this order.  The table uses stage ids (as used in
dependency lists, cache manifests, and timing data); two of them differ from
the `mattos-build` CLI stage names: stage id `linux` is selected on the command
line as `kernel`, and stage id `gcc-compiler` as `gcc-toolchain` (its build
output directory is `out/build/gcc-toolchain`).

| Stage | Output | Built by | Used for |
| --- | --- | --- | --- |
| `cross-toolchain` | `out/build/cross-toolchain/install` | host GCC | MattOS cross Binutils; libc-less pass-1 GCC (`--with-newlib --without-headers`) |
| `glibc` | `out/build/glibc/install`, `out/sysroot` | pass-1 GCC | runtime C library and development sysroot |
| `linux` | `out/build/linux` | pass-1 GCC (`CROSS_COMPILE`) | kernel and modules, plus out-of-tree NVIDIA modules |
| `gcc-runtime` | runtime libraries; `out/build/gcc-runtime/toolchain` | host GCC (compiler proper), new GCC (runtimes) | `libgcc_s`, `libstdc++`, `libgomp`, and the complete MattOS compiler |
| everything later | package outputs | MattOS GCC via `out/toolchain/bin` | all remaining target code |

The cross tools use these roles:

```text
pass-1 GCC build/host:  x86_64-build-linux-gnu
Binutils build/host:    x86_64-pc-linux-gnu (native-mode ld in a sysroot)
target:                 x86_64-pc-linux-gnu
tool names:             x86_64-pc-linux-gnu-<tool>
sysroot:                out/sysroot
linker search:          =/usr/lib/x86_64-linux-gnu =/lib/x86_64-linux-gnu =/usr/lib =/lib
```

Binutils are configured with host equal to target so that `ld` behaves like a
native linker inside the sysroot.  Like the distribution linker MattOS was
previously built with, it then resolves `DT_NEEDED` dependencies through
`DT_RUNPATH`/`DT_RPATH`, `LD_LIBRARY_PATH`, and `$sysroot/etc/ld.so.conf`.
It also emits `DT_RUNPATH` rather than `DT_RPATH` (`--enable-new-dtags`), as
Debian and Ubuntu binutils do.

`out/toolchain/bin` holds wrappers named `gcc`, `cc`, `g++`, `c++`, `cpp`, and
their `x86_64-pc-linux-gnu-` forms, plus symlinks to the MattOS cross
Binutils.  `mattos-build` puts that directory first on `PATH` for every target
build, so build systems that invoke `cc`, `gcc`, `ld`, or `ar` get MattOS
tools.  The wrappers add the MattOS sysroot, its multiarch start-file and
library directory, and `mattos-hardening.specs`.

That specs file (`MATTOS_HARDENING_SPECS` in
`src/tools/mattos-build/src/stages/toolchain.rs`) reproduces the hardening
defaults of the Ubuntu host GCC that MattOS was previously built and validated
with, because upstream GCC enables none of them.  Compile defaults:
`-fstack-protector-strong`, `-fstack-clash-protection`, `-fcf-protection`
(except with `-m16`/`-m32`), `-Wformat -Wformat-security`,
`-fzero-init-padding-bits=all`, `-Wbidi-chars=any`, and `-D_FORTIFY_SOURCE=3`
when optimizing.  Link defaults: `--as-needed` (unless sanitizing), `-z relro`,
and `-z now` except for static, shared, relocatable, or `-no-pie` links.  Each
default yields to an explicit caller option such as `-fno-stack-protector` or
`-U_FORTIFY_SOURCE`.

Default PIE, CET, GNU build IDs, and the GNU hash style (`MATTOS_GCC_DEFAULTS`:
`--enable-default-pie --enable-cet --enable-linker-build-id
--with-linker-hash-style=gnu`) are configured directly into the two
build-machine compilers: pass-1 GCC (`cross-toolchain`) and the complete
compiler from `gcc-runtime`.  The kernel uses the raw pass-1 compiler, because
Kbuild selects all of its own code-generation flags.

The third GCC build, the native compiler shipped in the guest
(`gcc-compiler`), is configured differently: of those defaults it receives only
`--enable-default-pie`.  It is not configured with CET, linker build-id, or
hash-style defaults, and no hardening specs file is installed with it, so code
compiled natively on an installed MattOS system does not get the
`mattos-hardening.specs` defaults above.  This is a known gap between the
build-time and installed compilers.

The native compiler shipped in the guest (`gcc-compiler`) uses these roles:

```text
build:  x86_64-build-linux-gnu
host:   x86_64-pc-linux-gnu
target: x86_64-pc-linux-gnu
sysroot during bootstrap: out/sysroot
installed native sysroot: /
languages: c,c++
```

The build disables multilib, NLS, sanitizers, OpenMP, libquadmath, libssp,
libatomic, libvtv, libcc1, plugins, and target LTO.  GMP, MPFR, and MPC are
obtained through GCC's checksum-pinned `download_prerequisites` mechanism.
They are linked statically into the compilers: the cross-toolchain builds them
for the build machine, and the native compiler builds them with the MattOS
compiler.  Their verified cache is kept at `out/cache/gcc-prerequisites`; they
are not installed guest runtimes.  Exact configure commands and deterministic
environment settings are written to the component `configure-invocation.txt`
files under `out/build`.

## Development sysroot

`out/sysroot` is recreated from controlled source outputs.  Linux's supported
command generates the first header layer:

```text
make ARCH=x86 headers_install INSTALL_HDR_PATH=out/sysroot/usr
```

The unmodified generated UAPI inventory is retained separately under
`out/build/glibc/linux-headers` for package staging and provenance.  glibc then
installs its public headers, CRT objects, static link support, linker scripts,
development linker names, loader, and runtime libraries over that controlled
base.  The GCC runtime build adds its target headers, target metadata, CRT
support, `libgcc` link support, C++ standard-library headers, `libstdc++.so`,
`libstdc++.a`, and `libsupc++.a`.

No arbitrary host include or library directory is copied into this tree.  The
installed development packages reproduce the same target layout at the guest's
root, so native invocations do not require a manual `--sysroot` option.

## Debian package boundaries

The package graph adds these ten packages:

| Package | Principal ownership |
| --- | --- |
| `linux-libc-dev` | exported Linux userspace UAPI headers |
| `libc6-dev` | glibc public headers, CRT objects, linker scripts, static link support |
| `mattos-libgcc-dev` | GCC 15 target headers, CRT objects, and libgcc link support |
| `mattos-libstdc++-dev` | GCC 15 C++ headers, development linker name, `libstdc++.a`, `libsupc++.a` |
| `binutils` | assembler, linker, archive, object-inspection, and binary-manipulation tools |
| `mattos-gcc-common` | installed GCC internal helpers and target-independent compiler support |
| `cpp` | C preprocessor driver |
| `gcc` | C compiler driver and `/usr/bin/cc` |
| `g++` | C++ compiler driver and `/usr/bin/c++` |
| `make` | `/usr/bin/make` |

Runtime libraries remain owned only by `libc6`, `libgcc-s1`, `libstdc++6`,
and `libgomp1` (the OpenMP runtime `libgomp.so.1`, built by the `gcc-runtime`
stage).  Development packages depend on those runtime owners and
do not duplicate their versioned shared objects.  Package staging, repository
auditing, and rootfs construction reject duplicate paths, unresolved ELF
dependencies, unowned helpers, host RPATH/RUNPATH, and embedded host paths.
Source-built Binutils `strip --strip-debug` normalizes staged ELF debug payloads;
runtime libraries compared byte-for-byte with their authoritative build outputs
instead receive deterministic compiler prefix maps at build time.

## Compiler defaults and validation

The installed GCC is configured with `/` as its native sysroot, standard
`/usr` target paths, and GCC's supported default-PIE mode so compile and link
defaults agree.  `-no-pie` remains available for an explicit traditional
executable.  Its internal helper location is `/usr/libexec/gcc`, target metadata
is under `/usr/lib/gcc` and `/usr/lib/x86_64-linux-gnu/gcc`, Binutils is under
`/usr/bin`, and the dynamic interpreter remains:

```text
/lib64/ld-linux-x86-64.so.2
```

Validation records and checks `gcc -v`, `g++ -v`, `-print-search-dirs`,
`-print-sysroot`, and `-dumpspecs`.  Installed outputs are rejected if they
contain the repository path or select host `/usr/include` or `/usr/lib` as a
bootstrap search root.

### Guest tests

The automated check is the QEMU install test: after installing a profile,
`installed_toolchain_probe` in `DevUtils/run_qemu.py` verifies that the
`mattos-toolchain` package is installed, that `gcc`, `g++`, `cpp`, `ld`,
`make`, `clang`, `lld`, `rustc`, and `cargo` are on `PATH`, and that GCC
compiles a small C program that then runs with the expected exit status on the
installed system.

The broader suite is `DevUtils/test_native_toolchain.sh`, run by hand inside an
installed guest from a MattOS checkout (it writes under `out/tmp`).  Nothing
invokes it automatically.  It covers direct C at `-O0` and `-O2`, PIE, pthreads, a shared
library and dynamic loading, C++ containers/strings/exceptions, a two-object
static archive using `as`, `ar`, `ranlib`, `nm`, `objdump`, `readelf`, and
`strip`, and a clean/rebuild cycle driven by GNU Make.  It also stages a tiny
ephemeral package, builds it with MattOS `dpkg-deb`, inspects it, installs it,
queries ownership, and removes it.  This test package is never added to the
embedded repository.  The suite inspects the loader with `readelf` and glibc's
source-built `ldd`.  Brush is package-owned as `brush`, `sh`, and `bash`, so
upstream scripts using either conventional interpreter run without per-script
shebang rewriting.

## Remaining self-hosting work

Since this milestone, the following have been built from pinned source and
packaged: CPython 3.14 (stage id `cpython`, CLI `python`; packages `python3`,
`libpython3.14`, `python3-venv`, `python3-dev`), Git (`git`), LLVM/Clang/LLD
(`llvm`, `llvm-dev`, `clang`, `lld`), and Rust 1.97.1 with Cargo (`rustc`,
`cargo`).  The LLVM and Rust packages and `python3-dev` are in
`mattos-toolchain`; `python3` and `git` are part of the base package set.  See
[Self-hosting development foundation](self-hosting.md).  The KDE Plasma desktop
([System](../../system/index.md)) and the Calamares graphical installer
([MattOS Installer](../../system/installer.md)) have also landed.

Still missing:

1. Perl, which many upstream build systems require.
2. Autoconf, Automake, Libtool, and pkg-config/pkgconf.
3. Meson, Ninja, and CMake.
4. Hardening defaults for the installed native GCC (see above).
5. Native rebuild of all MattOS packages.
6. Native ISO generation.

MattOS has not performed a compiler self-rebuild, a complete native package
rebuild, or native ISO generation.

### History

The follow-on order originally planned for this milestone was: additional
foundational build tools; a Python runtime evaluation that preferred
RustPython if it proved compatible with real workloads; Perl; Autotools and
pkg-config; Meson, Ninja, and CMake; Git; Rust, Cargo, the Rust standard
library, and rustup; native package rebuild; native ISO generation; the desktop
stack; and the graphical installer.  CPython was chosen over RustPython, and
rustup is not packaged; the Rust toolchain is built from the official source
release instead.
