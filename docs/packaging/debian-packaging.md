# MattOS Debian Packaging

MattOS uses Debian binary packages, `dpkg`, and APT. Its local repository (carried on the installer medium and copied onto installed systems) supplies every MattOS package, and the signed hosted MattOS repository at `https://packages.mattsherfey.com` is an enabled source at equal priority. Signed Debian 13 (Trixie) sources are shipped but disabled. See [APT source and pin policy](#apt-source-and-pin-policy). Editable source and package policy live in this monorepo. Generated `.deb` files and repository indexes live under `out/` and are ignored build artifacts.

This is a hybrid build-tool bootstrap, not a self-hosted distribution. The authoritative package set is `PACKAGE_NAMES` in `src/tools/mattos-build/src/packaging/registry.rs` (332 packages at the time of writing; `out/packages/inventory.toml` and `src/system/packages/debian-compat/trixie.toml` carry one entry per package). It covers the base filesystem and runtime policy, the package-manager and signature-verification runtime, MattOS-built glibc and GCC runtime libraries, systemd, util-linux, the kernel modules and firmware, administration/networking tools, the D-Bus broker and authentication stack, the native C/C++/Rust/Python development toolchain, KDE Plasma and its Qt/KF6/graphics stack, the installer, and profile metapackages. The final ISO has no host-derived executable or runtime-library payloads; host compilers and packaging tools remain build inputs.

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

Versions use `<upstream-version>-1mattos1`; unreleased snapshots use `0~git.<12-hex commit>-1mattos1`. Where Debian's package carries an epoch, the `debian_epoch` field in `trixie.toml` adds it (for example `libxau6` is `1:1.0.12-1mattos1` and `libx11-6` is `2:1.8.12-1mattos1`). `dpkg` is the one exception to both forms: its version comes from the imported `debian/changelog` plus the short import commit, giving `1.23.8+git.ff7e9d8b-1mattos1`. `out/packages/inventory.toml` records the exact current version of every package. Package modes and timestamps are normalized, directory walks are sorted, `dpkg-deb --root-owner-group` records root ownership, symlinks remain symlinks, and repository gzip headers and Release dates are fixed. No package ships the aggregate Info index `/usr/share/info/dir`: `install-info` maintains it on the installed system (Debian Policy 12.2), so `stage_package` removes it from every payload, including indexes carried in by bundled component installs such as flatpak's gpgme.

Most packages have no maintainer scripts. The current exceptions are `mattos-plasma` (a `postinst` that enables the Plasma login manager and sets `graphical.target` as the default) and `linux-modules-nvidia-595-open-<kernel>` (a `postinst` and `postrm` that run `depmod`). All of them exit immediately when `DPKG_ROOT` is set, so offline root assembly does not run them against the build host.

`package inspect` reports Essential, Priority, Depends, Provides, Conflicts, Replaces, conffiles, installed size, detected ELF dependencies, package-owned shared libraries, and repository dependency resolution in deterministic order. Provenance is installed as `/usr/share/doc/<package>/mattos-build-info.toml`.

## Package ownership

The table below covers representative core packages only; it is not the complete set. `PACKAGE_NAMES` and the `PackageSpec` entries in `src/tools/mattos-build/src/packaging/registry.rs` define every package, and `src/system/packages/debian-compat/trixie.toml` records each one's representative owned paths.

| Package | Priority | Selected payload and role |
| --- | --- | --- |
| `mattos-filesystem` | required, Essential | merged-`/usr` structural directories and symlinks |
| `libc6` | required, Essential | source-built glibc loader, runtime, and NSS modules |
| `libc-bin` | required | selected glibc runtime utilities |
| `systemd` | required, Essential | source-built systemd service manager and runtime, including `udevadm` and `systemd-udevd`; `Provides: systemd-sysv` |
| `mattos-base-runtime` | required, Essential | MattOS base runtime policy and required userland command dependencies |
| `mattos-base` | required | base profile metapackage (filesystem, base files and runtime, systemd, locales, kernel modules) |
| `mattos-cli`, `mattos-plasma` | optional | installed-system profile metapackages built on `mattos-base` |
| `mattos-toolchain` | optional | native development toolchain metapackage installed on every installed system |
| `mattos-installer` | optional | permanent MattOS CLI installer and shared installation policy |
| `util-linux` | required | selected util-linux administration commands (for example `lsblk`, `fdisk`, `wipefs`, `findmnt`) |
| `gpgv` | required | source-built OpenPGP signature verifier used by APT |
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
| `libsystemd0`, `libudev1` | important | source-built public systemd libraries |
| `udev` | important | imported systemd vendor hwdb sources, stock update unit, and reproducibly prebuilt `/usr/lib/udev/hwdb.bin` (udev executables are in `systemd`) |
| `libexpat1`, `libcap2` | important | source-built Expat and libcap ABI libraries and SONAME links |
| `libacl1`, `zlib1g`, `libbz2-1.0` | important | source-built ACL, zlib, and bzip2 ABI libraries and SONAME links |
| `liblz4-1`, `liblzma5`, `libxxhash0` | important | source-built APT/dpkg compression ABI libraries and SONAME links |
| `tar` | required | source-built GNU tar `/usr/bin/tar`, license, and provenance |
| `dbus-broker` | important | broker/launcher, system and user units, bus policy, and sysusers definition |
| `libpam0g`, `mattos-libpam-misc0` | required | source-built public Linux-PAM runtime libraries |
| `libpam-modules`, `libpam-runtime` | required | selected PAM modules, helper, and MattOS PAM policy |
| `passwd` | required | selected account administration tools, `login.defs`, and `default/useradd` |
| `mattos-sudo-rs` | required | `sudo`, `visudo`, sudoers policy, and secure modes |
| `login` | required | source-built `agetty`, `login`, `su`, and `sulogin` |
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
| `agetty`, `login`, `su`, `sulogin` | `login` | no command copy |
| selected iproute2/iputils commands and iproute2 data | network command packages | validates package-installed commands |
| public `libsystemd.so.0`, `libudev.so.1`; the systemd executable and unit tree | `libsystemd0`, `libudev1`, `systemd` | the remaining systemd install-tree copy skips package-owned paths |
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

`apt` owns `apt`, `apt-get`, `apt-cache`, `apt-config`, and `apt-mark`; `/usr/lib/apt/apt-helper`; the `copy`, `file`, `gpgv`, `http`, `https`, and `store` methods; planners and solvers; `libapt-private.so.0.0`; the live-image `/etc/apt` policy files; `/usr/share/keyrings/mattos-archive-keyring.asc` and `/usr/share/keyrings/debian-archive-keyring.asc`; the installed-system policy templates under `/usr/share/mattos/apt/installed/`; the `mattos-apt-daily.{service,timer}` and `mattos-apt-bootstrap.{service,timer}` units; and empty writable state directory scaffolding. `APT_RUNTIME_PATHS` in `src/tools/mattos-build/src/packaging.rs` and `stage_apt` in `src/tools/mattos-build/src/packaging/staging.rs` define the payload.

`apt` depends exactly on the MattOS `gpgv` package, so signed remote sources always have a packaged verifier. Repository generation continues to use host `dpkg-scanpackages` and `apt-ftparchive`. The live and installed source policies are described in [APT source and pin policy](#apt-source-and-pin-policy).

Mutable lists, archives, logs, partial files, and locks are never package payload files. The package creates only directories such as `/var/lib/apt/lists/partial`, `/var/cache/apt/archives/partial`, and `/var/log/apt`; live commands create their ephemeral contents.

### Bootstrap runtime boundary

`libc6` is the foundational runtime package. It owns the loader, glibc runtime DSOs, compatibility DSOs, NSS modules, resolver, license, provenance, and a checksummed runtime manifest. `libc-bin` owns `getent`, `locale`, `ldd`, and `ldconfig`. Development headers, crt objects, static archives, and linker inputs are packaged separately (`libc6-dev`, `linux-libc-dev`, `mattos-libgcc-dev`, `mattos-libstdc++-dev`, and the other development packages in `MATTOS_TOOLCHAIN_PACKAGES` in `registry.rs`). Those toolchain packages and the `mattos-toolchain` metapackage are left out of the live root, but the installer adds `mattos-toolchain` to every installed profile, so every installed system receives them from the repository on the medium.

`mattos-brush` owns the source-built `brush` executable plus the `sh` and `bash` compatibility symlinks. Because MattOS uses a merged `/usr` layout, both `/usr/bin/{sh,bash}` and `/bin/{sh,bash}` resolve to Brush. This lets source-built upstream scripts retain either conventional shell interpreter without an unowned rootfs alias or per-script shebang rewriting.

`libgcc-s1` owns only `libgcc_s.so.1` plus license, ABI, and provenance metadata. `libstdc++6` owns only `libstdc++.so.6.0.34`, its SONAME link, license, ABI, and provenance metadata. The latter depends on the former; both depend on `libc6`. GCC headers and static link inputs are separately owned by the honest MattOS-specific `mattos-libgcc-dev` and `mattos-libstdc++-dev` packages because Trixie's corresponding development split is GCC 14.

The former `mattos-bootstrap-runtime` package is absent from the installed set and repository. Its audit interface remains and reports zero host-derived entries and bytes. See [Bootstrap runtime audit](../build-system/toolchain/bootstrap-runtime-audit.md), [MattOS glibc bootstrap](../build-system/toolchain/glibc-bootstrap.md), [MattOS GCC runtime bootstrap](../build-system/toolchain/gcc-runtime-bootstrap.md), and the generated audit.

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

Every MattOS-to-MattOS dependency is emitted with an exact `(= version)` constraint; `out/packages/inventory.toml` currently contains no unversioned or ranged dependency. Representative ABI-coupled relationships:

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

`Essential: yes` is set on exactly four packages (the `essential: true` entries in `registry.rs`): `mattos-filesystem`, because removing the merged-`/usr` structure makes all packages unsafe; `libc6`; `systemd`; and `mattos-base-runtime`. Other core packages such as `mattos-base-files`, `dpkg`, `util-linux`, and `login` are Priority `required` but deliberately non-Essential so the Essential set does not grow ahead of a mature recovery policy. Removal of core packages is not tested in the primary image.

Repository generation parses its finished `Packages` index and fails if a package is absent, an architecture is not `amd64`, an exact version does not resolve, a dependency or `Provides` target is missing, or a package/version/architecture key is duplicated. The builder also computes a deterministic topological install order, rejects cycles, and verifies every staged ELF SONAME is owned by itself or a declared dependency. This validation occurs before the repository is placed on the ISO.

## Conffile policy

APT owns and marks these as conffiles:

```text
/etc/apt/apt.conf.d/01mattos
/etc/apt/preferences.d/00mattos-priority
/etc/apt/sources.list.d/00-mattos-local.sources
/etc/apt/sources.list.d/mattos-hosted.sources
/etc/apt/sources.list.d/debian-trixie.sources
```

These carry the live policy. On an installed system the installer overwrites `01mattos`, `00mattos-priority`, `mattos-hosted.sources`, and `debian-trixie.sources` with the installed templates and adds `/etc/apt/sources.list.d/00-mattos-local.sources`, so dpkg sees those conffiles as locally modified on later `apt` upgrades.

dpkg owns and marks `/etc/dpkg/dpkg.cfg` as a conffile. `mattos-base-files` retains its identity and profile conffiles. No generated `/var` state is a conffile. Normal dpkg reinstall semantics therefore preserve an administrator-modified configuration or surface the standard conffile decision rather than silently replacing it.

The expanded packages also mark `/etc/sysctl.conf`, `/etc/dbus-1/system.conf`, every MattOS `/etc/pam.d/*` stack, `/etc/login.defs`, `/etc/default/useradd`, `/etc/sudoers`, and `/etc/sudoers.d/README` as conffiles. No package contains passwd/group/shadow/gshadow databases, machine-id, sockets, `/run/user`, locks, journals, leases, APT lists, or dpkg status.

## MattOS APT vendor and local repository

APT is compiled with `CURRENT_VENDOR=mattos`. The MattOS vendor metadata is added by `upstream/patches/apt/0001-mattos-vendor-and-optional-ftparchive.patch`, applied to the build's output-owned source mirror rather than to the vendored tree. Runtime policy lives in `/etc/apt/apt.conf.d/01mattos`. The image codename and repository suite are `trixie`, while Origin remains MattOS.

The local repository generated in `out/repository/` has this layout and publishes `Origin: MattOS`, `Label: MattOS Local`, `Suite: trixie`, and `Codename: trixie`:

```text
/usr/share/mattos/repository/
├── pool/main/*.deb
└── dists/trixie/
    ├── Release
    └── main/binary-amd64/
        ├── Packages
        └── Packages.gz
```

The ISO carries the repository beside the SquashFS at `/mattos/repository`. In the live system `/usr/share/mattos/repository` is a symlink to `/run/mattos/medium/mattos/repository` (`LIVE_REPOSITORY_LINK_TARGET` in `packaging.rs`), where the live medium stays mounted. The installer copies the directory that link names onto the target, so an installed system has its own local copy at `/usr/share/mattos/repository`.

## APT source and pin policy

The live policy files are `src/system/packages/config/apt/{00-mattos-local.sources,mattos-hosted.sources,debian-trixie.sources,00mattos-priority,01mattos}`. The `apt` package installs them under `/etc/apt`, and rootfs assembly re-applies and validates them (`apply_live_apt_policy` and `validate_live_apt_policy` in `staging.rs`). The installed-system templates are in `src/system/packages/config/apt/installed/`, shipped by `apt` as `/usr/share/mattos/apt/installed/*`, and copied into the target by the installer's `configure_installed_apt` (`src/system/installer/policy/mod.rs`). The live and installed local sources share the file name `00-mattos-local.sources`, so the installer replaces the live source instead of adding a second entry for the same repository; the QEMU install test checks that exactly one source names it.

| Source | Live image | Installed system |
| --- | --- | --- |
| local `file:/usr/share/mattos/repository` (`Label: MattOS Local`) | enabled, `Trusted: yes` (`00-mattos-local.sources`) | enabled, `Trusted: yes` (same file, replaced by the installed template) |
| hosted `https://packages.mattsherfey.com` (`Label: MattOS`) | enabled, `Signed-By: /usr/share/keyrings/mattos-archive-keyring.asc` | enabled, same `Signed-By` |
| Debian `trixie`, `trixie-updates`, `trixie-security` | present but `Enabled: no`, `Signed-By: /usr/share/keyrings/debian-archive-keyring.asc` | present but `Enabled: no` |

Pinning (`00mattos-priority`) is the same shape in both:

- local MattOS (`o=MattOS,l=MattOS Local,n=trixie`): `990`;
- hosted MattOS (`o=MattOS,l=MattOS,n=trixie`): `990`, so a newer hosted version is a normal upgrade candidate while the local repository can still satisfy a complete offline closure;
- Debian (`o=Debian,n=trixie`): `500`;
- protected MattOS names get `-1` from `o=Debian`. In both the live and installed files, the set of names pinned away from Debian must equal `protected.toml` exactly, each name listed once in a well-formed record (`validate_protected_pins` in `packaging.rs`); this covers the base system, the APT verification packages (`gpgv`, `libgcrypt20`, and related libraries), and `polkit`, `network-manager`, and `libduktape207`.

A local pin of `1001` is rejected: the build's policy validation and the installer's `configure_installed_apt` both fail if the installed preferences contain `Pin-Priority: 1001`. The installer also requires the installed Debian sources to stay disabled; Debian must not silently become part of installed APT state.

`Trusted: yes` on the local `file:` source is the only unauthenticated exception. Hosted and Debian sources always use `Signed-By` with the keyrings shipped by `apt`, and verification uses the packaged `gpgv`. The installed `01mattos` additionally sets `Acquire::https::Verify-Peer` and `Verify-Host` to `true` and `Acquire::AllowInsecureRepositories` and `AllowDowngradeToInsecureRepositories` to `false`.

Installed systems also enable `mattos-apt-bootstrap.timer` (a first index refresh shortly after boot) and `mattos-apt-daily.timer` (a daily `apt-get update`). The live root must not enable the daily timer. During installation the target's APT lists are seeded from the local source only, so installation does not depend on the hosted repository being reachable.

Both live and installed APT policy use the root sandbox identity (`APT::Sandbox::User "root"`) because `_apt` is not yet a MattOS system account, and set `Pager "false"` because no pager package is shipped. Both are explicit transitional policies.

## Live APT workflow

The live rootfs contains no pre-baked APT list or archive state. For example:

```text
sudo apt-get update
sudo apt-get install --reinstall -y mattos-brush
sudo apt-get install --reinstall -y libbsd0
sudo apt-get install --reinstall -y libzstd1
sudo apt-get install --reinstall -y iputils-ping procps ncurses-bin
cd /tmp
apt-get download mattos-brush
```

`apt-get update` reads both enabled sources: the local repository on the medium and the hosted repository. Without a network, the hosted fetch fails and is reported, while the local index is still refreshed, so the commands above keep working offline. When both repositories offer the same version, the local source is listed first in the preferences and is preferred. Reinstall invokes MattOS-built dpkg, preserves the database and unrelated files, and leaves Brush executable. Ordinary-user download produces a user-owned `.deb` with the same SHA-256 as `pool/main`.

## Hybrid assembly and remaining migration

Rootfs assembly builds all packages and the repository, initializes an empty dpkg database, installs packages in computed dependency order through real host `dpkg` under `fakeroot`, snapshots owned paths, layers only non-migrated components, initializes writable APT state, and links `/usr/share/mattos/repository` to the repository on the live medium. `fakeroot` permits normal archive modes and ownership semantics without making generated workspace files root-owned. There is no later legacy copy of APT, dpkg, the migrated ncurses/kmod/procps/auth/network/D-Bus payload, their selected libraries, or the CA bundle. Legacy integration functions validate authoritative package-installed configuration before creating only runtime aliases and enablement links.

Host `dpkg-deb` and `dpkg` still build and install archives. Host `dpkg-scanpackages`, `apt-ftparchive`, and deterministic `gzip` still create indexes. Host `file`, `readelf`, and `ldd` support closure inspection. This is a bootstrap boundary, not self-hosting.

Every package in `PACKAGE_NAMES` is installed through the same dpkg graph and repository; no direct toolchain-copy path exists. The live root installs all of them except the toolchain packages (`live_excluded_packages` in `registry.rs`), and installed systems receive those through `mattos-toolchain`. `systemd` owns the systemd executable and unit tree, including `udevadm` and `systemd-udevd`; `udev` owns only the imported hwdb source closure, the stock update unit, and the reproducibly generated vendor database. See [MattOS native C/C++ toolchain](../build-system/toolchain/native-toolchain.md) for the toolchain boundaries, [Debian Compatibility (Current State)](debian-compatibility.md) for the package map and known gaps, [MattOS remote repository integration](remote-repository.md) for the validation handoff, and [Publishing Packages](publishing.md) for uploads. Hosted repository signing, online publication through `DevUtils/PublishPackages.py`, installation, and the installed APT refresh timers exist today. A standalone libcurl package, further build systems and languages (for example Perl, Autotools, pkgconf, Meson, Ninja, and CMake), and the remaining Perl-based dpkg helpers are not yet packaged.
