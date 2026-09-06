# MattOS Debian Packaging

MattOS uses Debian binary packages, `dpkg`, and APT. Its embedded repository supplies the complete protected base; signed Debian 13 repositories are configured as disabled supplemental scaffolding for controlled compatibility work. Editable source and package policy live in this monorepo. Generated `.deb` files and repository indexes live under `out/` and are ignored build artifacts.

This is a hybrid build-tool bootstrap, not a self-hosted distribution. Sixty-six packages own the initial base, package-manager runtime, MattOS-built glibc and GCC runtime libraries, native C/C++ development toolchain, selected source-built libraries and GNU tar, udev hardware database, administration/networking tools, D-Bus broker, and authentication stack. The final ISO has no host-derived executable or runtime-library payloads; host compilers and packaging tools remain build inputs.

## Imported package-manager sources

| Component | Official repository | Branch | Imported commit | Destination |
| --- | --- | --- | --- | --- |
| dpkg | `https://git.dpkg.org/git/dpkg/dpkg.git` | `main` | `ff7e9d8bf01379e8b022028a65afaa262e2c25cd` | `src/system/packages/dpkg/` |
| APT | `https://salsa.debian.org/apt-team/apt.git` | `main` | `5e6dcc8d0c8bdce61e9cc7f497abadb5349d509a` | `src/system/packages/apt/` |
| LinuxScripts | `https://github.com/HungLo2020/LinuxScripts.git` | `master` | `d1e85219c8f86ceaa1135312126d02fa4dbee623` | `src/infrastructure/LinuxScripts/` |

The imports are ordinary editable files without nested Git repositories. `upstream/sources.toml` is authoritative and `upstream/state/{dpkg,apt}.toml` records the exact imports.

## Commands and artifacts

```text
cargo run -p mattos-build -- package build --all
cargo run -p mattos-build -- package repo
cargo run -p mattos-build -- package inspect apt
cargo run -p mattos-build -- package audit
cargo run -p mattos-build -- package status
cargo run -p mattos-build -- package compatibility-audit
cargo run -p mattos-build -- package publish-plan out/packages/amd64/<package>.deb
```

Outputs are deterministic and written to:

```text
out/packages/staging/<package>/
out/packages/amd64/<package>_<version>_amd64.deb
out/packages/inventory.toml
out/repository/
```

Versions use `<upstream-version>-1mattos1`; unreleased snapshots use `0~git.<commit>-1mattos1`. Package modes and timestamps are normalized, directory walks are sorted, `dpkg-deb --root-owner-group` records root ownership, symlinks remain symlinks, and repository gzip headers and Release dates are fixed. No MattOS package in this milestone has a maintainer script.

`package inspect` reports Essential, Priority, Depends, Provides, Conflicts, Replaces, conffiles, installed size, detected ELF dependencies, package-owned shared libraries, and repository dependency resolution in deterministic order. Provenance is installed as `/usr/share/doc/<package>/mattos-build-info.toml`.

## Package ownership

| Package | Priority | Selected payload and role |
| --- | --- | --- |
| `mattos-filesystem` | required, Essential | merged-`/usr` structural directories and symlinks |
| `libc6`, `libc-bin` | required | source-built glibc loader/runtime/NSS modules and selected runtime utilities |
| `libgcc-s1`, `libstdc++6` | required | source-built GCC unwinding and C++ runtime ABIs; no development files |
| `mattos-base-files` | required | MattOS identity, hostname default, profile, issue, and shells |
| `ca-certificates` | important | pinned Mozilla-derived CA bundle and update provenance |
| `mattos-brush` | required | `/usr/bin/brush` |
| `coreutils` | required | uutils multicall binary and non-conflicting applet symlinks |
| `curl` | optional | curl CLI and its source-built matching `libcurl.so.4` ABI |
| `libmd0`, `libbsd0` | important | source-built message-digest and BSD portability ABIs and SONAME links |
| `libzstd1` | important | source-built Zstandard runtime ABI and SONAME links |
| `mattos-libcrypto3`, `libssl3t64` | important | source-built OpenSSL crypto and TLS runtime ABIs and SONAME links |
| `libelf1t64` | important | source-built elfutils `libelf.so.1` runtime ABI and SONAME links |
| `libpcre2-8-0`, `libselinux1`, `libcrypt1` | important/required | source-built PCRE2, SELinux compatibility, and password-hashing runtime ABIs |
| `libblkid1`, `libmount1`, `libsmartcols1`, `mount` | important/required | source-built util-linux mount closure replacing the former host mount/library copy path |
| `dpkg` | required | the selected source-built dpkg runtime and support data |
| `libapt-pkg7.0` | important | source-built `libapt-pkg.so.7.0` runtime and SONAME links |
| `apt` | important | APT commands, private library, local methods, helpers, solvers, planners, and configuration |
| `mattos-libtinfow6`, `libncursesw6` | important | source-built ncurses ABI libraries and SONAME links |
| `ncurses-base`, `ncurses-bin` | important | six required terminal descriptions and selected ncurses commands |
| `libkmod2`, `kmod` | important | source-built libkmod and selected module administration commands |
| `mattos-libproc2`, `procps` | important | source-built libproc2, selected procps commands, and `/etc/sysctl.conf` |
| `libsystemd0`, `libudev1` | important | narrow source-built public systemd libraries; not a full systemd package migration |
| `udev` | important | imported systemd vendor hwdb sources, stock update unit, and reproducibly prebuilt `/usr/lib/udev/hwdb.bin` |
| `libexpat1`, `libcap2` | important | source-built Expat and libcap ABI libraries and SONAME links |
| `libacl1`, `zlib1g`, `libbz2-1.0` | important | source-built ACL, zlib, and bzip2 ABI libraries and SONAME links |
| `liblz4-1`, `liblzma5`, `libxxhash0` | important | source-built APT/dpkg compression ABI libraries and SONAME links |
| `tar` | required | source-built GNU tar `/usr/bin/tar`, license, and provenance |
| `dbus-broker` | important | broker/launcher, system and user units, bus policy, and sysusers definition |
| `libpam0g`, `mattos-libpam-misc0` | required | source-built public Linux-PAM runtime libraries |
| `libpam-modules`, `libpam-runtime` | required | selected PAM modules, helper, and MattOS PAM policy |
| `passwd` | required | selected account administration tools, `login.defs`, and `default/useradd` |
| `mattos-sudo-rs` | required | `sudo`, `visudo`, sudoers policy, and secure modes |
| `login` | required | source-built `agetty`, `login`, and `su` |
| `iproute2`, `iputils-ping` | important | selected routing and network diagnostic commands plus iproute2 data |

Directories may be shared. Regular files and symlinks may have only one package owner. The builder rejects package/package collisions before archive creation and rejects later legacy overwrites by snapshotting package-owned paths.

### Package path migration map

| Former rootfs path | New package owner | Legacy path after migration |
| --- | --- | --- |
| selected ncurses commands | `ncurses-bin` | validates package-installed commands |
| selected terminfo entries | `ncurses-base` | validates the installed database |
| `libtinfow.so.6`, `libncursesw.so.6` | dedicated ncurses library packages | no direct library copy |
| kmod commands and `libkmod.so.2` | `kmod`, `libkmod2` | no command/library copy |
| procps commands, `libproc2.so.1`, `sysctl.conf` | procps binary/library packages | configuration comparison only |
| dbus-broker binaries, policy, units, session policy | `dbus-broker` | validation plus aliases/wants only |
| PAM libraries, selected modules, helper, `/etc/pam.d` | four PAM packages | no auth-runtime/config copy |
| Shadow commands and static configuration | `passwd` | no command/config copy |
| sudo-rs commands and permanent sudoers policy | `mattos-sudo-rs` | live-profile overlay remains separate |
| `agetty`, `login`, `su` | `login` | no command copy |
| selected iproute2/iputils commands and iproute2 data | network command packages | validates package-installed commands |
| public `libsystemd.so.0`, `libudev.so.1` | narrow systemd library packages | full systemd tree copy skips owned paths |
| `libexpat.so.1`, `libcap.so.2` | `libexpat1`, `libcap2` | excluded from bootstrap closure and rejected if restored |
| `/usr/bin/tar`, `libacl.so.1`, `libz.so.1`, `libbz2.so.1.0` | `tar`, `libacl1`, `zlib1g`, `libbz2-1.0` | excluded from bootstrap closure and rejected if restored |
| `liblz4.so.1`, `liblzma.so.5`, `libxxhash.so.0` | `liblz4-1`, `liblzma5`, `libxxhash0` | excluded from bootstrap closure and rejected if restored |
| `libmd.so.0`, `libbsd.so.0` | `libmd0`, `libbsd0` | excluded from bootstrap closure and rejected if restored |
| `libzstd.so.1`, `libcrypto.so.3`, `libssl.so.3`, `libelf.so.1` | `libzstd1`, `mattos-libcrypto3`, `libssl3t64`, `libelf1t64` | excluded from bootstrap closure and rejected if restored |
| `libpcre2-8.so.0`, `libselinux.so.1`, `libcrypt.so.1` | `libpcre2-8-0`, `libselinux1`, `libcrypt1` | excluded from bootstrap closure and rejected if restored |
| `mount`, `umount`, `libblkid.so.1`, `libmount.so.1`, `libsmartcols.so.1` | four util-linux packages | former host-copy path removed; every file is dpkg-owned |
| `libgcc_s.so.1`, `libstdc++.so.6` | `libgcc-s1`, `libstdc++6` | final host-runtime copy path removed; selected GCC shared runtimes only |

The dependency-aware order is computed from declared edges rather than this table or `PACKAGE_NAMES`. Independent packages retain a stable declaration-order tie break. A cycle or unknown MattOS dependency stops repository creation and rootfs installation.

### dpkg boundary

`dpkg` owns the built C/ELF commands `dpkg`, `dpkg-deb`, `dpkg-divert`, `dpkg-query`, `dpkg-realpath`, `dpkg-split`, `dpkg-statoverride`, `dpkg-trigger`, `update-alternatives`, and `start-stop-daemon`. It also owns `/usr/share/dpkg`, `/etc/dpkg/dpkg.cfg`, the configuration directory, and alternatives directory scaffolding.

The eight `dpkg*` ELF commands above directly need `libmd.so.0` and resolve it from `libmd0`. Shadow's `chage`, `newgrp`, `passwd`, `chpasswd`, `groupadd`, `groupdel`, `groupmod`, `useradd`, `userdel`, and `usermod` directly need `libbsd.so.0`; libbsd in turn needs `libmd.so.0`. Their builds receive explicit staged include, linker, pkg-config, and runtime-library paths, and post-build loader checks reject host fallback.

`dpkg-maintscript-helper` is deliberately excluded because the upstream output is a Perl program and MattOS does not yet provide a packaged Perl runtime. Existence in an upstream install tree is not treated as runtime support.

The package never ships `/var/lib/dpkg/status`, `available`, generated `info/`, `updates/`, locks, or other database state. Rootfs assembly initializes these and real host `dpkg` operations populate them.

### APT boundary

`apt` owns `apt`, `apt-get`, `apt-cache`, `apt-config`, and `apt-mark`; `/usr/lib/apt/apt-helper`; the `copy`, `file`, and `store` methods; planners and solvers; `libapt-private.so.0.0`; `/etc/apt`; and empty writable state directory scaffolding.

The live image carries APT's `file`, HTTP, HTTPS, and `gpgv` methods, but only the embedded `file:/usr/share/mattos/repository` source is enabled there. The installer replaces that policy in the target: the frozen local source is disabled, the signed hosted MattOS source is enabled at priority 990, and signed Debian Trixie, updates, and security sources are enabled at priority 500. Repository generation continues to use host `apt-ftparchive`; target-side remote use also requires the packaged `gpgv` executable and keyrings, so missing verification runtime is a build-time defect rather than an insecure fallback.

Mutable lists, archives, logs, partial files, and locks are never package payload files. The package creates only directories such as `/var/lib/apt/lists/partial`, `/var/cache/apt/archives/partial`, and `/var/log/apt`; live commands create their ephemeral contents.

### Bootstrap runtime boundary

`libc6` is the foundational runtime package. It owns the loader, glibc runtime DSOs, compatibility DSOs, NSS modules, resolver, license, provenance, and a checksummed runtime manifest. `libc-bin` owns `getent`, `locale`, `ldd`, and `ldconfig`. Development headers, crt objects, static archives, and linker inputs stay in `out/sysroot` and are not installed on the runtime ISO.

`mattos-brush` owns the source-built `brush` executable plus the `sh` and `bash` compatibility symlinks. Because MattOS uses a merged `/usr` layout, both `/usr/bin/{sh,bash}` and `/bin/{sh,bash}` resolve to Brush. This lets source-built upstream scripts retain either conventional shell interpreter without an unowned rootfs alias or per-script shebang rewriting.

`libgcc-s1` owns only `libgcc_s.so.1` plus license, ABI, and provenance metadata. `libstdc++6` owns only `libstdc++.so.6.0.34`, its SONAME link, license, ABI, and provenance metadata. The latter depends on the former; both depend on `libc6`. GCC headers and static link inputs are separately owned by the honest MattOS-specific `mattos-libgcc-dev` and `mattos-libstdc++-dev` packages because Trixie's corresponding development split is GCC 14.

The former `mattos-bootstrap-runtime` package is absent from the installed set and repository. Its audit interface remains and reports zero host-derived entries and bytes. See `docs/BOOTSTRAP_RUNTIME.md`, `docs/GLIBC_BOOTSTRAP.md`, `docs/GCC_RUNTIME_BOOTSTRAP.md`, and the generated audit.

`curl` continues to carry its matching source-built `libcurl.so.4` because splitting one small ABI pair would add churn without improving this milestone. It depends on MattOS libc, the CA bundle, zlib, Zstandard, libcrypto, and libssl packages.

### OpenSSL runtime policy

OpenSSL is configured for shared `linux-x86_64` libraries under `/usr/lib/x86_64-linux-gnu`, with `OPENSSLDIR=/etc/ssl`, zlib and Zstandard enabled, and applications, tests, documentation, the legacy provider, and loadable modules disabled. With `no-module`, the default provider is compiled into `libcrypto`; no provider module tree or OpenSSL configuration file is runtime payload. `mattos-libcrypto3` owns `libcrypto.so.3`, and `libssl3t64` owns `libssl.so.3` and depends on the exact crypto package.

curl is rebuilt against those exact staged libraries. Its compiled CA file is `/etc/ssl/certs/ca-certificates.crt`, its default CA directory is disabled, and ordinary HTTPS verification remains enabled.

### CA certificates

`ca-certificates` owns `/etc/ssl/certs/ca-certificates.crt` and the relative
`/etc/ssl/cert.pem -> certs/ca-certificates.crt` compatibility link used by
OpenSSL's compiled default lookup. `src/system/network/ca-bundle.toml` records
the pinned curl CA Extract URL/date, SHA-256, destination, MPL-2.0 license, and
validated count of 119 certificates. Ordinary builds never download a mutable
`latest` bundle. The installed `UPDATE.md` describes the explicit
checksum-and-count update process.

## Dependency and Essential policy

ABI-coupled relationships use exact versions:

```text
libgcc-s1 -> libc6 (= exact)
libstdc++6 -> libc6, libgcc-s1 (= exact)
mattos-brush/coreutils/sudo-rs -> libgcc-s1 (= exact)
libapt-pkg7.0/apt -> libgcc-s1, libstdc++6 (= exact)
apt -> dpkg, libapt-pkg7.0 (= exact), ca-certificates
curl -> zlib1g, libzstd1, mattos-libcrypto3,
               libssl3t64 (= exact)
mattos-libcrypto3 -> zlib1g, libzstd1 (= exact)
libssl3t64 -> mattos-libcrypto3, zlib1g, libzstd1 (= exact)
libelf1t64 -> zlib1g, libzstd1 (= exact)
tar -> libacl1 (= exact)
dpkg -> tar, zlib1g, libbz2-1.0,
               libzstd1 (= exact)
dpkg -> liblzma5 (= exact)
dpkg -> libmd0 (= exact)
libbsd0 -> libmd0 (= exact)
passwd -> libbsd0, libmd0 (= exact)
libapt-pkg7.0/apt -> zlib1g, libbz2-1.0,
                                liblz4-1, liblzma5,
                                libxxhash0, libzstd1,
                                mattos-libcrypto3 (= exact)
libapt-pkg7.0 -> libudev1, libsystemd0 (= exact)
iproute2 -> zlib1g, libzstd1, libelf1t64 (= exact)
procps -> mattos-libproc2, libncursesw6, mattos-libtinfow6 (= exact)
dbus-broker -> libsystemd0, libexpat1 (= exact)
iproute2 -> libcap2 (= exact)
libpam-runtime -> libpam0g, libpam-modules (= exact)
passwd/sudo-rs/login -> exact PAM packages
libselinux1 -> libpcre2-8-0 (= exact)
dpkg/iproute2 -> libselinux1, libpcre2-8-0 (= exact)
libpam-modules/libpam-runtime/passwd -> libcrypt1 (= exact)
libmount1 -> libblkid1 (= exact)
mount -> libblkid1, libmount1,
                libsmartcols1, libselinux1 (= exact)
```

Only `mattos-filesystem` is `Essential: yes`, because removing the merged-`/usr` structure makes all packages unsafe. `mattos-base-files` and `dpkg` are Priority `required` but deliberately non-Essential during the prototype so the Essential set does not grow ahead of a mature recovery policy. Removal of core packages is not tested in the primary image.

Repository generation parses its finished `Packages` index and fails if a package is absent, an architecture is not `amd64`, an exact version does not resolve, a dependency or `Provides` target is missing, or a package/version/architecture key is duplicated. The builder also computes a deterministic topological install order, rejects cycles, and verifies every staged ELF SONAME is owned by itself or a declared dependency. This validation occurs before the repository is embedded.

## Conffile policy

APT owns and marks these as conffiles:

```text
/etc/apt/apt.conf.d/01mattos
/etc/apt/preferences.d/00mattos-priority
/etc/apt/sources.list.d/mattos.sources
/etc/apt/sources.list.d/mattos-hosted.sources
/etc/apt/sources.list.d/debian-trixie.sources
```

dpkg owns and marks `/etc/dpkg/dpkg.cfg` as a conffile. `mattos-base-files` retains its identity and profile conffiles. No generated `/var` state is a conffile. Normal dpkg reinstall semantics therefore preserve an administrator-modified configuration or surface the standard conffile decision rather than silently replacing it.

The expanded packages also mark `/etc/sysctl.conf`, `/etc/dbus-1/system.conf`, every MattOS `/etc/pam.d/*` stack, `/etc/login.defs`, `/etc/default/useradd`, `/etc/sudoers`, and `/etc/sudoers.d/README` as conffiles. No package contains passwd/group/shadow/gshadow databases, machine-id, sockets, `/run/user`, locks, journals, leases, APT lists, or dpkg status.

## MattOS APT vendor and local repository

APT is compiled with `CURRENT_VENDOR=mattos`. Vendored build metadata lives in `src/system/packages/apt/vendor/mattos`; runtime policy lives in `/etc/apt/apt.conf.d/01mattos`. The image codename and repository suite are `trixie`, while Origin remains MattOS. Debian and hosted MattOS source scaffolds are present but explicitly disabled.

The repository layout is:

```text
/usr/share/mattos/repository/
├── pool/main/*.deb
└── dists/trixie/
    ├── Release
    └── main/binary-amd64/
        ├── Packages
        └── Packages.gz
```

`/etc/apt/sources.list.d/mattos.sources` selects `file:/usr/share/mattos/repository`, suite `trixie`, component `main`, architecture `amd64`, and `Trusted: yes`. The trust flag is a narrowly scoped unsigned local-bootstrap exception. Hosted MattOS and official Debian Trixie deb822 files are separately identifiable, disabled, and use `Signed-By`; neither permits unauthenticated remote packages. Pinning is local `1001`, hosted MattOS `990`, Debian `500`, with Debian-origin protected names forced to `-1`.

The temporary live APT policy also uses the root sandbox identity because `_apt` is not yet a MattOS system account, and disables APT's pager because a pager package is outside this milestone. Both choices are explicit transitional policies.

## Offline workflow

The live rootfs contains no pre-baked APT list or archive state. With or without a QEMU NIC:

```text
sudo apt-get update
sudo apt-get install --reinstall -y mattos-brush
sudo apt-get install --reinstall -y libbsd0
sudo apt-get install --reinstall -y libzstd1
sudo apt-get install --reinstall -y iputils-ping procps ncurses-bin
cd /tmp
apt-get download mattos-brush
```

Update reads only the embedded `file:` source. Reinstall selects that artifact, invokes MattOS-built dpkg, preserves the database and unrelated files, and leaves Brush executable. Ordinary-user download produces a user-owned `.deb` with the same SHA-256 as `pool/main`.

## Hybrid assembly and remaining migration

Rootfs assembly builds all packages and the repository, initializes an empty dpkg database, installs packages in computed dependency order through real host `dpkg` under `fakeroot`, snapshots owned paths, layers only non-migrated components, initializes writable APT state, and embeds the repository. `fakeroot` permits normal archive modes and ownership semantics without making generated workspace files root-owned. There is no later legacy copy of APT, dpkg, the migrated ncurses/kmod/procps/auth/network/D-Bus payload, their selected libraries, or the CA bundle. Legacy integration functions validate authoritative package-installed configuration before creating only runtime aliases and enablement links.

Host `dpkg-deb` and `dpkg` still build and install archives. Host `dpkg-scanpackages`, `apt-ftparchive`, and deterministic `gzip` still create indexes. Host `file`, `readelf`, and `ldd` support closure inspection. This is a bootstrap boundary, not self-hosting.

Target runtime closure and the first native C/C++ development-tool milestone are complete. The graph has 66 packages. `udev` owns the selected imported-systemd hwdb source closure, the stock update unit, and the reproducibly generated vendor database; the wider udev executable tree remains part of the legacy systemd integration layer. These packages are installed through the same dpkg graph and embedded repository as runtime packages; no direct toolchain-copy path exists. See `NATIVE_TOOLCHAIN.md` for the exact boundaries, `DEBIAN_COMPATIBILITY.md` for the full package map and known gaps, and `REMOTE_REPOSITORY.md` for the non-publishing handoff. A standalone libcurl package, later build systems and languages, remaining dpkg helpers, and full systemd packaging can follow independently. Repository signing, online publication, persistence, installation, and automatic upgrades are separate future milestones.
