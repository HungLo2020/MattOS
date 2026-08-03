# MattOS Debian Packaging

MattOS uses Debian binary packages, `dpkg`, and APT without using Debian or Ubuntu as a binary source. Editable source and package policy live in this monorepo. Generated `.deb` files and repository indexes live under `out/` and are ignored build artifacts.

This is a hybrid bootstrap, not a self-hosted distribution. Forty-one packages own the initial base, package-manager runtime, selected source-built libraries and GNU tar, administration/networking tools, D-Bus broker, and authentication stack. Full systemd executables and several host runtime libraries still follow the proven legacy assembly path.

## Imported package-manager sources

| Component | Official repository | Branch | Imported commit | Destination |
| --- | --- | --- | --- | --- |
| dpkg | `https://git.dpkg.org/git/dpkg/dpkg.git` | `main` | `ff7e9d8bf01379e8b022028a65afaa262e2c25cd` | `src/system/packages/dpkg/` |
| APT | `https://salsa.debian.org/apt-team/apt.git` | `main` | `5e6dcc8d0c8bdce61e9cc7f497abadb5349d509a` | `src/system/packages/apt/` |

The imports are ordinary editable files without nested Git repositories. `upstream/sources.toml` is authoritative and `upstream/state/{dpkg,apt}.toml` records the exact imports.

## Commands and artifacts

```text
cargo run -p mattos-build -- package build --all
cargo run -p mattos-build -- package repo
cargo run -p mattos-build -- package inspect mattos-apt
cargo run -p mattos-build -- package audit
cargo run -p mattos-build -- package status
```

Outputs are deterministic and written to:

```text
out/packages/staging/<package>/
out/packages/amd64/<package>_<version>_amd64.deb
out/packages/inventory.toml
out/repository/
```

Versions use `<upstream-version>-1mattos1`. Package modes and timestamps are normalized, directory walks are sorted, `dpkg-deb --root-owner-group` records root ownership, symlinks remain symlinks, and repository gzip headers and Release dates are fixed. No package in this milestone has a maintainer script.

`package inspect` reports Essential, Priority, Depends, Provides, Conflicts, Replaces, conffiles, installed size, detected ELF dependencies, package-owned shared libraries, and repository dependency resolution in deterministic order. Provenance is installed as `/usr/share/doc/<package>/mattos-build-info.toml`.

## Package ownership

| Package | Priority | Selected payload and role |
| --- | --- | --- |
| `mattos-filesystem` | required, Essential | merged-`/usr` structural directories and symlinks |
| `mattos-bootstrap-runtime` | required | exact temporary host-derived ELF closure; GNU tar is excluded |
| `mattos-base-files` | required | MattOS identity, hostname default, profile, issue, and shells |
| `mattos-ca-certificates` | important | pinned Mozilla-derived CA bundle and update provenance |
| `mattos-brush` | required | `/usr/bin/brush` |
| `mattos-coreutils` | required | uutils multicall binary and non-conflicting applet symlinks |
| `mattos-curl` | optional | curl CLI and its source-built matching `libcurl.so.4` ABI |
| `mattos-libmd0`, `mattos-libbsd0` | important | source-built message-digest and BSD portability ABIs and SONAME links |
| `mattos-dpkg` | required | the selected source-built dpkg runtime and support data |
| `mattos-libapt-pkg` | important | source-built `libapt-pkg.so.7.0` runtime and SONAME links |
| `mattos-apt` | important | APT commands, private library, local methods, helpers, solvers, planners, and configuration |
| `mattos-libtinfow6`, `mattos-libncursesw6` | important | source-built ncurses ABI libraries and SONAME links |
| `mattos-terminfo`, `mattos-ncurses-bin` | important | six required terminal descriptions and selected ncurses commands |
| `mattos-libkmod2`, `mattos-kmod` | important | source-built libkmod and selected module administration commands |
| `mattos-libproc2`, `mattos-procps` | important | source-built libproc2, selected procps commands, and `/etc/sysctl.conf` |
| `mattos-libsystemd0`, `mattos-libudev1` | important | narrow source-built public systemd libraries; not a full systemd package migration |
| `mattos-libexpat1`, `mattos-libcap2` | important | source-built Expat and libcap ABI libraries and SONAME links |
| `mattos-libacl1`, `mattos-zlib1g`, `mattos-libbz2-1.0` | important | source-built ACL, zlib, and bzip2 ABI libraries and SONAME links |
| `mattos-liblz4-1`, `mattos-liblzma5`, `mattos-libxxhash0` | important | source-built APT/dpkg compression ABI libraries and SONAME links |
| `mattos-tar` | required | source-built GNU tar `/usr/bin/tar`, license, and provenance |
| `mattos-dbus-broker` | important | broker/launcher, system and user units, bus policy, and sysusers definition |
| `mattos-libpam0`, `mattos-libpam-misc0` | required | source-built public Linux-PAM runtime libraries |
| `mattos-pam-modules`, `mattos-pam-runtime` | required | selected PAM modules, helper, and MattOS PAM policy |
| `mattos-shadow` | required | selected account administration tools, `login.defs`, and `default/useradd` |
| `mattos-sudo-rs` | required | `sudo`, `visudo`, sudoers policy, and secure modes |
| `mattos-util-linux-auth` | required | source-built `agetty`, `login`, and `su` |
| `mattos-iproute2`, `mattos-iputils` | important | selected routing and network diagnostic commands plus iproute2 data |

Directories may be shared. Regular files and symlinks may have only one package owner. The builder rejects package/package collisions before archive creation and rejects later legacy overwrites by snapshotting package-owned paths.

### Package path migration map

| Former rootfs path | New package owner | Legacy path after migration |
| --- | --- | --- |
| selected ncurses commands | `mattos-ncurses-bin` | validates package-installed commands |
| selected terminfo entries | `mattos-terminfo` | validates the installed database |
| `libtinfow.so.6`, `libncursesw.so.6` | dedicated ncurses library packages | no direct library copy |
| kmod commands and `libkmod.so.2` | `mattos-kmod`, `mattos-libkmod2` | no command/library copy |
| procps commands, `libproc2.so.1`, `sysctl.conf` | procps binary/library packages | configuration comparison only |
| dbus-broker binaries, policy, units, session policy | `mattos-dbus-broker` | validation plus aliases/wants only |
| PAM libraries, selected modules, helper, `/etc/pam.d` | four PAM packages | no auth-runtime/config copy |
| Shadow commands and static configuration | `mattos-shadow` | no command/config copy |
| sudo-rs commands and permanent sudoers policy | `mattos-sudo-rs` | live-profile overlay remains separate |
| `agetty`, `login`, `su` | `mattos-util-linux-auth` | no command copy |
| selected iproute2/iputils commands and iproute2 data | network command packages | validates package-installed commands |
| public `libsystemd.so.0`, `libudev.so.1` | narrow systemd library packages | full systemd tree copy skips owned paths |
| `libexpat.so.1`, `libcap.so.2` | `mattos-libexpat1`, `mattos-libcap2` | excluded from bootstrap closure and rejected if restored |
| `/usr/bin/tar`, `libacl.so.1`, `libz.so.1`, `libbz2.so.1.0` | `mattos-tar`, `mattos-libacl1`, `mattos-zlib1g`, `mattos-libbz2-1.0` | excluded from bootstrap closure and rejected if restored |
| `liblz4.so.1`, `liblzma.so.5`, `libxxhash.so.0` | `mattos-liblz4-1`, `mattos-liblzma5`, `mattos-libxxhash0` | excluded from bootstrap closure and rejected if restored |
| `libmd.so.0`, `libbsd.so.0` | `mattos-libmd0`, `mattos-libbsd0` | excluded from bootstrap closure and rejected if restored |

The dependency-aware order is computed from declared edges rather than this table or `PACKAGE_NAMES`. Independent packages retain a stable declaration-order tie break. A cycle or unknown MattOS dependency stops repository creation and rootfs installation.

### dpkg boundary

`mattos-dpkg` owns the built C/ELF commands `dpkg`, `dpkg-deb`, `dpkg-divert`, `dpkg-query`, `dpkg-realpath`, `dpkg-split`, `dpkg-statoverride`, `dpkg-trigger`, `update-alternatives`, and `start-stop-daemon`. It also owns `/usr/share/dpkg`, `/etc/dpkg/dpkg.cfg`, the configuration directory, and alternatives directory scaffolding.

The eight `dpkg*` ELF commands above directly need `libmd.so.0` and resolve it from `mattos-libmd0`. Shadow's `chage`, `newgrp`, `passwd`, `chpasswd`, `groupadd`, `groupdel`, `groupmod`, `useradd`, `userdel`, and `usermod` directly need `libbsd.so.0`; libbsd in turn needs `libmd.so.0`. Their builds receive explicit staged include, linker, pkg-config, and runtime-library paths, and post-build loader checks reject host fallback.

`dpkg-maintscript-helper` is deliberately excluded because the upstream output is a Perl program and MattOS does not yet provide a packaged Perl runtime. Existence in an upstream install tree is not treated as runtime support.

The package never ships `/var/lib/dpkg/status`, `available`, generated `info/`, `updates/`, locks, or other database state. Rootfs assembly initializes these and real host `dpkg` operations populate them.

### APT boundary

`mattos-apt` owns `apt`, `apt-get`, `apt-cache`, `apt-config`, and `apt-mark`; `/usr/lib/apt/apt-helper`; the `copy`, `file`, and `store` methods; planners and solvers; `libapt-private.so.0.0`; `/etc/apt`; and empty writable state directory scaffolding.

HTTP/HTTPS methods, `gpgv`, `apt-ftparchive`, and apt-utils are intentionally excluded from the guest runtime. The immediate contract is the embedded `file:` repository. Repository generation continues to use host `apt-ftparchive`.

Mutable lists, archives, logs, partial files, and locks are never package payload files. The package creates only directories such as `/var/lib/apt/lists/partial`, `/var/cache/apt/archives/partial`, and `/var/log/apt`; live commands create their ephemeral contents.

### Bootstrap runtime boundary

`mattos-bootstrap-runtime` is an explicit transitional package, not a glibc package. At build time the packager runs `ldd` over every selected packaged ELF root. It resolves against MattOS component install trees first, excludes every library now owned by a dedicated package, normalizes the remaining closure under `/usr/lib/x86_64-linux-gnu` and the loader under `/usr/lib64`, and records destination, source, reason, and SHA-256 for every file in `runtime-files.tsv`.

The package no longer owns GNU tar, ACL, zlib, bzip2, LZ4, liblzma, xxHash, libmd, libbsd, libudev, libsystemd, PAM, ncurses, kmod, procps, Expat, or libcap. The original audit found 23 payload files and 17,600,184 bytes; Expat/libcap reduced that to 21 files and 17,365,960 bytes, tar/ACL/zlib/bzip2 reduced it to 17 files and 16,669,816 bytes, compression reduced it to 14 files and 16,191,736 bytes, and libmd/libbsd reduce it to 12 files and 16,042,648 bytes. Zstandard remains bootstrap-owned because bootstrap-owned OpenSSL and libelf directly consume it; introducing `mattos-libzstd1` now would create a bootstrap dependency cycle. This remains a portability and trust limitation. See `docs/BOOTSTRAP_RUNTIME.md` and the generated `out/reports/bootstrap-runtime-audit.toml`.

`mattos-curl` continues to carry its matching source-built `libcurl.so.4` because splitting one small ABI pair would add churn without improving this milestone. It depends on the exact bootstrap runtime and on `mattos-ca-certificates`.

### CA certificates

`mattos-ca-certificates` owns `/etc/ssl/certs/ca-certificates.crt`. `src/system/network/ca-bundle.toml` records the pinned curl CA Extract URL/date, SHA-256, destination, MPL-2.0 license, and validated count of 119 certificates. Ordinary builds never download a mutable `latest` bundle. The installed `UPDATE.md` describes the explicit checksum-and-count update process.

## Dependency and Essential policy

ABI-coupled relationships use exact versions:

```text
mattos-dpkg -> mattos-bootstrap-runtime (= exact)
mattos-libapt-pkg -> mattos-bootstrap-runtime (= exact)
mattos-apt -> mattos-bootstrap-runtime (= exact), mattos-dpkg (= exact),
              mattos-libapt-pkg (= exact), mattos-ca-certificates
mattos-brush/coreutils -> mattos-bootstrap-runtime (= exact)
mattos-curl -> mattos-bootstrap-runtime (= exact), mattos-ca-certificates
mattos-tar -> mattos-bootstrap-runtime, mattos-libacl1 (= exact)
mattos-dpkg -> mattos-tar, mattos-zlib1g, mattos-libbz2-1.0 (= exact)
mattos-dpkg -> mattos-liblzma5 (= exact)
mattos-dpkg -> mattos-libmd0 (= exact)
mattos-libbsd0 -> mattos-libmd0 (= exact)
mattos-shadow -> mattos-libbsd0, mattos-libmd0 (= exact)
mattos-libapt-pkg/mattos-apt -> mattos-zlib1g, mattos-libbz2-1.0,
                                mattos-liblz4-1, mattos-liblzma5,
                                mattos-libxxhash0 (= exact)
mattos-curl/mattos-iproute2 -> mattos-zlib1g (= exact, required by their transitive ELF closures)
mattos-procps -> mattos-libproc2, mattos-libncursesw6, mattos-libtinfow6 (= exact)
mattos-dbus-broker -> mattos-libsystemd0, mattos-libexpat1 (= exact)
mattos-iproute2 -> mattos-libcap2 (= exact)
mattos-pam-runtime -> mattos-libpam0, mattos-pam-modules (= exact)
mattos-shadow/sudo-rs/util-linux-auth -> exact PAM packages
```

Only `mattos-filesystem` is `Essential: yes`, because removing the merged-`/usr` structure makes all packages unsafe. `mattos-base-files` and `mattos-dpkg` are Priority `required` but deliberately non-Essential during the prototype so the Essential set does not grow ahead of a mature recovery policy. Removal of core packages is not tested in the primary image.

Repository generation parses its finished `Packages` index and fails if a package is absent, an architecture is not `amd64`, an exact version does not resolve, a dependency or `Provides` target is missing, or a package/version/architecture key is duplicated. The builder also computes a deterministic topological install order, rejects cycles, and verifies every staged ELF SONAME is owned by itself or a declared dependency. This validation occurs before the repository is embedded.

## Conffile policy

APT owns and marks these as conffiles:

```text
/etc/apt/apt.conf.d/01mattos
/etc/apt/sources.list.d/mattos.sources
```

dpkg owns and marks `/etc/dpkg/dpkg.cfg` as a conffile. `mattos-base-files` retains its identity and profile conffiles. No generated `/var` state is a conffile. Normal dpkg reinstall semantics therefore preserve an administrator-modified configuration or surface the standard conffile decision rather than silently replacing it.

The expanded packages also mark `/etc/sysctl.conf`, `/etc/dbus-1/system.conf`, every MattOS `/etc/pam.d/*` stack, `/etc/login.defs`, `/etc/default/useradd`, `/etc/sudoers`, and `/etc/sudoers.d/README` as conffiles. No package contains passwd/group/shadow/gshadow databases, machine-id, sockets, `/run/user`, locks, journals, leases, APT lists, or dpkg status.

## MattOS APT vendor and local repository

APT is compiled with `CURRENT_VENDOR=mattos`. Vendored build metadata lives in `src/system/packages/apt/vendor/mattos`; runtime policy lives in `/etc/apt/apt.conf.d/01mattos`. Future persistent releases must keep `/etc/os-release`, the repository Codename/Suite, and vendor metadata aligned on `mattos` or a documented release codename. Debian release aliases and default sources are not installed.

The repository layout is:

```text
/usr/share/mattos/repository/
├── pool/main/*.deb
└── dists/mattos/
    ├── Release
    └── main/binary-amd64/
        ├── Packages
        └── Packages.gz
```

`/etc/apt/sources.list.d/mattos.sources` contains only `file:/usr/share/mattos/repository`, suite `mattos`, component `main`, architecture `amd64`, and `Trusted: yes`. The trust flag is a narrowly scoped unsigned local-bootstrap exception. It does not enable unauthenticated remote repositories and must not be reused when an HTTP(S) repository is introduced.

The temporary live APT policy also uses the root sandbox identity because `_apt` is not yet a MattOS system account, and disables APT's pager because a pager package is outside this milestone. Both choices are explicit transitional policies.

## Offline workflow

The live rootfs contains no pre-baked APT list or archive state. With or without a QEMU NIC:

```text
sudo apt-get update
sudo apt-get install --reinstall -y mattos-brush
sudo apt-get install --reinstall -y mattos-libbsd0
sudo apt-get install --reinstall -y mattos-iputils mattos-procps mattos-ncurses-bin
cd /tmp
apt-get download mattos-brush
```

Update reads only the embedded `file:` source. Reinstall selects that artifact, invokes MattOS-built dpkg, preserves the database and unrelated files, and leaves Brush executable. Ordinary-user download produces a user-owned `.deb` with the same SHA-256 as `pool/main`.

## Hybrid assembly and remaining migration

Rootfs assembly builds all packages and the repository, initializes an empty dpkg database, installs packages in computed dependency order through real host `dpkg` under `fakeroot`, snapshots owned paths, layers only non-migrated components, initializes writable APT state, and embeds the repository. `fakeroot` permits normal archive modes and ownership semantics without making generated workspace files root-owned. There is no later legacy copy of APT, dpkg, the migrated ncurses/kmod/procps/auth/network/D-Bus payload, their selected libraries, or the CA bundle. Legacy integration functions validate authoritative package-installed configuration before creating only runtime aliases and enablement links.

Host `dpkg-deb` and `dpkg` still build and install archives. Host `dpkg-scanpackages`, `apt-ftparchive`, and deterministic `gzip` still create indexes. Host `file`, `readelf`, and `ldd` support closure inspection. This is a bootstrap boundary, not self-hosting.

The next safe migration order is a coordinated OpenSSL/elfutils split that releases the Zstandard dependency cycle, then PCRE2/libxcrypt/SELinux. A MattOS-built libc and dynamic loader and the GCC/libstdc++ runtimes remain later toolchain boundaries. A standalone libcurl package, Perl and remaining dpkg helpers, and full systemd packaging can follow independently. Repository signing, online publication, persistence, installation, and automatic upgrades are separate future milestones.
