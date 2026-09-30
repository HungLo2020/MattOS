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

The launcher and `mattos-build` use the repository-owned `out/tmp/` directory
for build temporary files. It is created and write-tested automatically, and
takes precedence over an inherited `TMPDIR` so a full host `/tmp` cannot break
the build. `--build-only` runs `doctor` and the same `cargo run -p mattos-build -- build all` command used directly, in separate child processes. Stage keys normalize the build locale/time policy and identify selected tools rather than hashing the caller's raw `PATH`, so unchanged direct and launcher builds share the same cache identity.

This milestone also requires the systemd, dbus-broker, Autotools, networking, packaging, glibc/GCC-runtime-bootstrap, and ELF-inspection tools declared by `DevUtils/setup.py`, including GCC/G++, GNU assembler and linker tools, Make, Bison, Meson/Ninja, CMake, Autoconf/Automake/libtool, `gnulib-tool`, GNU awk (`gawk`), `rsync`, `bindgen`, `dpkg-deb`, `dpkg-scanpackages`, `apt-ftparchive`, `fakeroot`, `zstd`, `xz`, `file`, `ldd`, and `readelf`. The host compiler only builds the stage-0 cross toolchain, the host-running compiler proper, and build-time helper programs. All target code is compiled by source-built GCC and Binutils (see [MattOS native C/C++ toolchain](toolchain/native-toolchain.md)). GCC's GMP, MPFR, and MPC prerequisites are built from checksum-pinned sources, not host `-dev` packages. Target runtime development files come from imported source builds and `out/sysroot`, not host distribution `-dev` packages.

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

By default the first failing stage stops the build: nothing new is
dispatched and running stages finish. With `--keep-going`, every stage that
does not depend on a failed stage still builds, and the run ends with a list
of each failure and the stages it kept from building:

```
cargo run -p mattos-build -- build all --keep-going
```

Stages that did build are cached, so the next run resumes after the fixes.

Check that the cache is stable (a no-op build after the unit tests must reuse
every stage and package; any miss names the input that moved):

```
python3 DevUtils/check_cache_stability.py            # build, test, build
python3 DevUtils/check_cache_stability.py --no-establish   # after a build
```

Run it after changing cache keys, stage inputs, or anything tests execute.

Check that the toolchain stages rebuild byte-for-byte from unchanged inputs.
Every target stage's key includes their outputs, so a nondeterministic
toolchain stage recompiles the whole system:

```
python3 DevUtils/check_stage_reproducibility.py               # cross-toolchain, glibc, gcc-runtime
python3 DevUtils/check_stage_reproducibility.py --if-changed  # skip stages verified for their current inputs
python3 DevUtils/check_stage_reproducibility.py zlib          # any stage
```

Each stage is rebuilt once and its per-file output inventory compared with
the previous build; differences are listed with small text diffs. Verified
input/output pairs are recorded under `out/state/reproducibility/`. The
rebuilt output is kept, so a stage that is not reproducible rebuilds its
downstream stages on the next build.

### Testing the build tool

`cargo test -p mattos-build` runs the build tool's unit tests. Test what the
code does rather than how it is written:

- `performance::command_recorder::record` intercepts every command a recipe,
  helper, or staging function runs, and records its program, arguments,
  working directory and environment. The test's effect closure can create the
  files the real command would produce, or run it for real with
  `RecordedCommand::run_for_real` (for example a source-tree `rsync`). Combined
  with a temporary fixture tree, this exercises real recipe code without
  building anything. It intercepts only commands run through the `run_cmd*`
  helpers on the test's own thread; a direct `Command::output()` or a worker
  thread still executes for real.
- Validate data such as the package staging tables directly; for example,
  `every_source_path_package_staging_reads_is_tracked` checks every `src/...`
  path packaging code reads against the Git index.
- A test that searches Rust source text pins how code is written, and passes
  or fails for layout reasons. `tests_that_read_rust_source_text_do_not_grow`
  caps how many such tests exist; lower its ceiling when converting one. When a
  pin is unavoidable, look the code up with `source_item(name)` or
  `packaging_source()` (in `build_system_tests.rs`), which find it wherever it
  lives, instead of slicing a file between neighbouring items.

Inspect a stage without building it:

```text
cargo run -p mattos-build -- cache explain linux --details
cargo run -p mattos-build -- cache explain glibc --details
```

The detailed form compares schema, source/configuration/environment/tool/dependency/full digests and their exact stored/current fields. Schema 3 is a one-time migration from older manifests; after one successful establishing build, identical fresh direct and launcher processes must report foundation hits.

## Stage graph overview

`build all` runs a dependency graph of a few hundred stages; the
[architecture page](architecture.md) describes how it is scheduled and cached.
The groups below are a representative overview, not an exhaustive or strictly
ordered list. `cargo run -p mattos-build -- build --help` prints every stage
name, and `src/tools/mattos-build/src/stage_graph.rs` (`direct_dependencies`)
is the authoritative dependency list.

Stage names below are the CLI names accepted by `build <stage>`. A few differ
from the internal stage IDs used in cache manifests, logs, and
`cache explain`: `kernel` is `linux`, `gcc-toolchain` is `gcc-compiler`,
`python` is `cpython`, `pam` is `linux-pam`, and `procps` is `procps-ng`.

- **Foundational toolchain.** `cross-toolchain` (host-built stage-0 MattOS
  cross Binutils and libc-less pass-1 GCC) comes first. `kernel` (Linux built
  from `src/kernel/config/x86_64_mattos.config`) and `glibc` (controlled Linux
  UAPI export, out-of-tree GNU libc, and the initial development sysroot) are
  both compiled by the pass-1 GCC and can run in parallel. `gcc-runtime`
  follows glibc, selects `libgcc_s.so.1` and `libstdc++.so.6`, and installs
  the MattOS compiler used by every later target stage. `binutils`,
  `gcc-toolchain`, and `make` then produce the native toolchain, and the
  virtual `formal-sysroot` node marks the completed `out/sysroot` boundary.
- **Libraries and base userland.** Almost every other target stage depends on
  `formal-sysroot` plus the libraries it actually links: for example
  compression (`zlib`, `bzip2`, `lz4`, `xz`, `xxhash`, `zstd`), `openssl`,
  `elfutils`, `pcre2`, `selinux`, `libxcrypt`, `libmd`/`libbsd`, `ncurses`,
  `tar`, `sed`, `dash`, `mawk`, `rsync`, `procps`, `iproute2`, `iputils`, `curl`,
  the native build tools `pkgconf`, `cmake`, `perl`, `m4`, `autoconf`,
  `automake`, `libtool` and `ninja`, the authentication stack
  (`pam`, `shadow`, `util-linux`), `kmod`, `dbus`, `systemd`, `dpkg`, and
  `apt`.
- **Language toolchains.** `python` (CPython with `libffi`), `llvm` (LLVM
  shared runtime, selected tools, Clang, and LLD with the X86 and AMDGPU
  backends; AMDGPU is the userspace GPU compiler backend used by Mesa, not a
  CPU target), and `rust` (rustc, the native standard library, rustdoc, and
  Cargo, built from the checksummed official source release and depending on
  `llvm`). This MattOS rustc compiles all target Rust code, so the Cargo-built
  stages (`brush`, `coreutils`, `grep`, `findutils`, `diffutils`,
  `init`, `sudo-rs`, `greetd`, `cozy`, `installer`) and the Meson builds with
  Rust components (`dbus-broker`, `mesa`, `gstreamer`) depend on `rust` as
  well as `formal-sysroot`.
- **Graphics and desktop.** Wayland, Mesa, Vulkan, fonts, Qt (`qt-base`,
  `qt-declarative`, ...), KDE Frameworks (`k-core-addons`, `kio`, ...),
  Plasma (`plasma-k-win`, `plasma-workspace`, `plasma-desktop`, ...),
  applications, Flatpak, NetworkManager, PipeWire, and the Calamares
  installer.
- **Packages and repository.** After every package-producing stage, package
  staging and `.deb` publication and the offline Debian repository are built
  inside the `rootfs` stage's scheduled action.
- **Image.** `rootfs` assembles the package root; `live-root` compresses it
  into a deterministic zstd SquashFS; `initramfs` builds the small early
  archive from `formal-sysroot` and `kernel` (it does not consume the
  rootfs); `iso` combines the kernel, live root, early initramfs, GRUB, and
  repository into the bootable ISO.

The live root installs every package except the development toolchain
(`MATTOS_TOOLCHAIN_PACKAGES` in
`src/tools/mattos-build/src/packaging/registry.rs`: GCC, Binutils, Make,
Clang/LLD/LLVM, rustc/Cargo, and the `-dev` packages) and its
`mattos-toolchain` metapackage. Every installed system gets the toolchain:
the installer installs `mattos-toolchain` alongside the selected profile. The
offline package repository with all packages, toolchain included, sits on the
ISO at `/mattos/repository` rather than inside the SquashFS; the live root's
`/usr/share/mattos/repository` links to it on the mounted medium
(`/run/mattos/medium`), and the installer copies it onto installed systems so
they can install the toolchain offline.

Tar's explicit Gnulib replacement is copied into Tar's output-owned source mirror before bootstrap. A checksummed Tar patch adds the gitlink-era `FLEXNSIZEOF` compatibility macro to that mirror because the pinned stable-202301 replacement predates the macro used by Tar 1.35; the authoritative Tar and Gnulib imports remain unchanged.

Systemd configuration remains intentionally minimal. It enables networkd, resolved, timesyncd, timedated, logind, PAM integration, and `busctl` while continuing to disable homed, nspawn, bootloader tools, the remote journal stack, docs, tests, translations, TPM/FIDO, and BPF extras. The separate dbus-broker stage supplies both the system-scope binary and the binary used by MattOS-owned user units; systemd's Meson `dbus` option remains disabled because it controls the optional reference `libdbus` dependency, not sd-bus support.

## Incremental builds

Warm builds are guarded by content-addressed stage manifests rather than timestamps. Repository, rootfs, live-root, initramfs, and ISO layers participate in the same dependency model and retain full inventory/corruption validation. See [Build performance and cache model](performance.md) for keys, atomic replacement, package/ELF fact reuse, timing reports, quiet native logs, and scoped cache commands.

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

`image` runs the `rootfs`, `live-root`, `initramfs`, and `iso` stages in order, validating and reusing unchanged layers without forcing unrelated recompilation. A changed package cascades through the repository, rootfs, live root, and ISO; a GRUB-configuration-only change affects only the ISO.

The complete `build all` command already ends with a current ISO. The Python QEMU launcher therefore invokes `build all` once and does not call `image` afterward. For build-only automation:

```text
python3 DevUtils/run_qemu.py --build-only
```

Recent timing reports can be read without rebuilding:

```text
cargo run -p mattos-build -- timings
cargo run -p mattos-build -- cache status
cargo run -p mattos-build -- cache explain glibc
cargo run -p mattos-build -- cache explain repository
cargo run -p mattos-build -- cache explain rootfs-live
cargo run -p mattos-build -- cache explain elf-facts
```

`rootfs-live` (and `rootfs-base`) are diagnostic aliases that report the
`rootfs` stage; use `cache explain live-root` for the SquashFS stage.

Native-stage subprocess output is stored in `out/logs/<stage>.log`; failures show a useful tail. Set `MATTOS_VERBOSE_BUILD_OUTPUT=1` to stream full output for diagnosis.

Historical measurements (2026-08, when the stage graph was far smaller than it is now): for the first cache milestone, an unchanged complete `build all` measured 4:04.45 with 112 hits, zero misses, and seven intentionally non-cacheable stages, compared with the 53:00.44 audit baseline. The second layer/fact-cache milestone reduced the required second unchanged run to 3:50.94 with 116 hits, zero misses, and no non-cacheable timing entries. A scoped independent repository/rootfs/initramfs/ISO rebuild reproduced every recorded package, repository, rootfs inventory, ELF inventory, initramfs, and ISO digest exactly. These were warm-development measurements on the build graph of that time and are not current performance figures; release validation still uses independent rebuilds and byte comparisons as documented in [Build performance and cache model](performance.md).

Package and repository commands:

```
cargo run -p mattos-build -- package build --all
cargo run -p mattos-build -- package repo
cargo run -p mattos-build -- package inspect apt
cargo run -p mattos-build -- package audit
cargo run -p mattos-build -- package status
cargo run -p mattos-build -- package compatibility-audit
```

The package set is defined by `PACKAGE_NAMES` in `src/tools/mattos-build/src/packaging/registry.rs` (several hundred packages). `libc6` and `libc-bin` supply the MattOS-built glibc runtime, loader, NSS/resolver modules, and selected utilities; `libgcc-s1` and `libstdc++6` supply the final source-built compiler runtimes. The `udev` package owns systemd's selected vendor hwdb sources, the stock update unit, and a source-generated `/usr/lib/udev/hwdb.bin`. The development toolchain packages (`MATTOS_TOOLCHAIN_PACKAGES` in the same file, gathered by the `mattos-toolchain` metapackage) add Linux/glibc/GCC development files, source-built Binutils, GCC C/C++, GNU Make, Clang/LLD/LLVM, rustc/Cargo, and further `-dev` packages such as `python3-dev`, `libglvnd-dev`, and `libvulkan-dev`; they are excluded from the live root and installed on every installed system. After glibc and GCC runtime construction, downstream native stages are rebuilt with the controlled sysroot. Repository creation validates the dependency graph, staged ELF ownership, exact interpreter, loader resolution, and GLIBC/GLIBCXX/CXXABI/GCC symbol versions before image embedding. `mattos-bootstrap-runtime` is retired and the final host-derived target-runtime count is zero. The compatibility audit also validates all package classifications, versions, protected pins, source scaffolds, and the immutable LinuxScripts publisher. See [MattOS glibc bootstrap](toolchain/glibc-bootstrap.md), [MattOS GCC runtime bootstrap](toolchain/gcc-runtime-bootstrap.md), [MattOS native C/C++ toolchain](toolchain/native-toolchain.md), [MattOS Debian Packaging](../packaging/debian-packaging.md), [Debian Compatibility (Current State)](../packaging/debian-compatibility.md), [MattOS remote repository integration](../packaging/remote-repository.md), and [Bootstrap runtime audit](toolchain/bootstrap-runtime-audit.md).

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

Every GRUB entry in `src/boot/grub/grub.cfg` loads `initrd /boot/early-initramfs.cpio.xz` and boots `rdinit=/init`, the static early init built from `src/boot/live-init.c`; no entry passes `systemd.unit=`. The entries select a mode on the kernel command line, and the early init chooses the systemd target after switching to the live root:

| Entry | Kernel argument | Result |
| --- | --- | --- |
| Start MattOS Live (default) | `mattos.mode=live` | systemd with `mattos-live-graphical.target` |
| Start MattOS Live (CLI) | `mattos.mode=live-cli` | systemd with `mattos.target` |
| Install MattOS (CLI) | `mattos.mode=install-cli` | systemd with `mattos-install-cli.target` |
| MattOS Rescue | `mattos.rescue=1` | MattOS Rust rescue init at `/usr/libexec/mattos/rescue-init` |
| MattOS AMD graphics diagnostics (CLI) | `mattos.mode=live-cli` plus DRM/AMDGPU debug options | systemd with `mattos.target` |

Without a recognized mode the early init falls back to `mattos.target`.

Live media no longer unpacks the complete system into initramfs memory. GRUB
loads a small early archive whose static `/init` mounts the ISO SquashFS and a
tmpfs writable overlay before switching to systemd. The vendor hwdb remains
generated reproducibly in package staging, and the live overlay provides
ordinary writable runtime state without modifying the compressed lower root.
The archive also carries a dependency-ordered, zstd-compressed generic boot
module closure sourced from the matching MattOS kernel build. The installed
initramfs uses the same closure; the remaining modules are supplied by the
versioned `linux-modules-<release>` package under `/usr/lib/modules`.
See [Live and Installed Root Architecture](../system/boot/live-root.md).

## Cleanup

```
cargo run -p mattos-build -- clean artifacts
cargo run -p mattos-build -- clean logs
cargo run -p mattos-build -- clean cargo
cargo run -p mattos-build -- clean all
```

Cleanup never deletes imported upstream source trees.
