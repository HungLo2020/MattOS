# Debian Compatibility (Current State)

!!! warning "Not a guarantee"
    MattOS is not designed to be binary compatible with Debian 13 (Trixie).
    It is currently similar to Debian and partly compatible with it, but that
    is a consequence of shared tooling and conventions, not a promise: MattOS
    can and will diverge from Debian. Treat everything below as a description
    of the current state, which may change without a compatibility transition.

MattOS uses Debian's package formats and tooling (`.deb`, `dpkg`, APT) on
`amd64` with a MattOS-built and MattOS-controlled critical base. MattOS
repositories take precedence. Debian sources are shipped but disabled; if an
administrator enables them, Debian is only a supplemental source for optional
software and may not replace protected infrastructure.

The machine-readable mapping of the current state is
`src/system/packages/debian-compat/trixie.toml`. It maps every package in
`PACKAGE_NAMES` (`src/tools/mattos-build/src/packaging/registry.rs`; 332 at the
time of writing) to source, representative owned paths, ABI or command surface,
protection, deterministic version, Debian dependency role, classification, and
known gaps, and may record a Debian epoch. Classifications are
`debian-compatible`, `mattos-alternative`, `mattos-extension`, and
`mattos-specific`. `protected.toml` is the authoritative protected-name
inventory.
The build rejects an incomplete mapping, invalid classification or version,
missing protected pin, unsafe source configuration, changed LinuxScripts
publisher, or nested Git metadata.

## Current interfaces

| Interface | Current MattOS behavior |
| --- | --- |
| package identity | Real Trixie binary names are used only where the current payload is a credible replacement. MattOS-only packages keep `mattos-`. |
| versions | Debian syntax and `dpkg` comparison; releases use `<upstream>-1mattos<N>`, snapshots `0~git.<12hex>-1mattos<N>`, with a Debian epoch where `trixie.toml` records one; `dpkg` itself uses `<changelog version>+git.<8hex>-1mattos<N>`; never timestamps. |
| architecture | `Architecture: amd64`; no foreign-architecture or multiarch co-install support. |
| libraries | Runtime DSOs use `/usr/lib/x86_64-linux-gnu`, with Debian-relevant SONAME and symbol-version checks recorded by the ELF audit. |
| loader | Dynamic executables use `/lib64/ld-linux-x86-64.so.2`. |
| filesystem | merged `/usr`: `/bin`, `/sbin`, and `/lib` resolve into `/usr`; package paths and common commands remain conventional. |
| package state | `/var/lib/dpkg` is initialized as mutable state and populated through real `dpkg`; packages never ship its status, locks, or generated `info` data. |
| APT | deb822 sources, `/etc/apt/preferences.d`, conventional cache/list/log directories, the local `file:` repository, and the signed hosted MattOS repository are present. |
| maintainer scripts | Brush is available as both `/bin/sh` and `/bin/bash`; basic pre/post install/remove scripts are supported. Perl-based helpers are not. |
| systemd | The `systemd` package owns PID 1, `systemctl`, the udev executables, and the unit tree; it is `Essential: yes` and `Provides: systemd-sysv`. D-Bus, logind, and `pam_systemd` work. |
| metadata | conffiles are honored; the alternatives database and `update-alternatives` exist; full Debian trigger/helper coverage is not claimed. |
| dependencies | Build-time graph checks require every named dependency to resolve and ABI-coupled MattOS dependencies use exact versions. |

Brush remains `mattos-brush`; it does not claim the Debian `bash` package.
The package owns `/usr/bin/brush` and symlinks `/usr/bin/sh` and
`/usr/bin/bash` to it. With merged `/usr`, scripts using either `/bin/sh` or
`/bin/bash` execute Brush without shebang changes. No versioned `Provides:
bash` is emitted because Brush is not asserted to implement the complete Bash
package contract.

## Package naming result

The runtime and toolchain packages whose contracts match now use Debian binary
names. These include `libc6`, `libc-bin`, `libc6-dev`, `linux-libc-dev`,
`libgcc-s1`, `libstdc++6`, `binutils`, `cpp`, `gcc`, `g++`, `make`,
`ca-certificates`, `coreutils`, `curl`, `libmd0`, `libbsd0`, `libzstd1`,
`libssl3t64`, `libelf1t64`, `libpcre2-8-0`, `libselinux1`, `libcrypt1`,
`libblkid1`, `libmount1`, `libsmartcols1`, `mount`, `dpkg`,
`libapt-pkg7.0`, `apt`, `libncursesw6`, `ncurses-base`, `ncurses-bin`,
`libkmod2`, `kmod`, `procps`, `libsystemd0`, `libudev1`, `udev`, `libexpat1`,
`libcap2`, `libacl1`, `zlib1g`, `libbz2-1.0`, `liblz4-1`, `liblzma5`,
`libxxhash0`, `tar`, `dbus-broker`, `libpam0g`, `libpam-modules`,
`libpam-runtime`, `passwd`, `login`, `iproute2`, and `iputils-ping`.

The list above is representative; many more packages (for example `systemd`,
`util-linux`, `gpgv`, `python3`, `git`, `rustc`, `cargo`, and the Qt, KDE, Mesa,
and X11 libraries) also use Debian names. `trixie.toml` is the complete record.

Packages classified `mattos-specific` in `trixie.toml` currently are
`mattos-filesystem`, `mattos-compat`, `mattos-base-files`,
`mattos-base-runtime`, `mattos-base`, `mattos-cli`, `mattos-plasma`,
`mattos-plasma-live`, `mattos-plasma-theme`, `mattos-toolchain`,
`mattos-installer`, `mattos-cozy`, `mattos-brush`, `mattos-gcc-common`,
`mattos-libgcc-dev`, `mattos-libstdc++-dev`, `mattos-libcrypto3`,
`mattos-libtinfow6`, `mattos-libproc2`, `mattos-libpam-misc0`, and
`mattos-sudo-rs`. The GCC 15 development packages deliberately do not claim
Trixie's GCC 14 identities.

## Versions and protected transactions

Release branches are converted to deterministic upstream versions. A moving
branch that cannot supply a release version sorts conservatively as a
`0~git...` snapshot and relies on repository policy rather than an artificially
high version. Tests exercise Debian 13 versions, epochs, `~` prereleases,
MattOS revisions, and downgrade ordering with `dpkg --compare-versions`.

APT priority (`src/system/packages/config/apt/00mattos-priority`, and the
installed-system copy under `config/apt/installed/`) is:

1. local MattOS: `990`, matching `o=MattOS,l=MattOS Local,n=trixie`;
2. hosted MattOS: `990`, matching `o=MattOS,l=MattOS,n=trixie`;
3. Debian Trixie: `500`, matching `o=Debian,n=trixie`.

Every name in `protected.toml` has an additional Debian-origin priority of
`-1`. Some protected names are reserved before MattOS ships a matching
package: of the current list, `libgcc-14-dev`, `libstdc++-14-dev`,
`linux-image-amd64`, and `linux-headers-amd64` are not MattOS packages. This
prevents Debian from silently taking ownership of files already supplied
through another MattOS boundary.

The local repository publishes `Origin: MattOS`, `Label: MattOS Local`,
`Suite: trixie`, and `Codename: trixie`. Its unsigned `file:` source alone uses
the `Trusted: yes` local exception. The hosted MattOS source is enabled on both
the live image and installed systems and uses `Signed-By:
/usr/share/keyrings/mattos-archive-keyring.asc`. Only the Debian sources are
shipped disabled; they use `Signed-By:
/usr/share/keyrings/debian-archive-keyring.asc` and never `Trusted: yes`, and
the installer refuses to produce an installed system with them enabled.
Enabling Debian is an explicit administrative action. See
[APT source and pin policy](debian-packaging.md#apt-source-and-pin-policy) for
the full live and installed policy.

## Controlled Debian test (historical)

This is a dated record of an earlier experiment, run when the package set was
much smaller and every remote source, including hosted MattOS, was shipped
disabled. Its results describe that earlier state, not the current policy.

The Trixie `amd64` Packages metadata and Debian archive signatures were checked
in an isolated APT root. The resolver selected MattOS candidates for protected
`libc6`, `libstdc++6`, `dpkg`, and `apt`; Debian's `systemd` candidate was made
ineligible. `hello` 2.10-5, `vtable-dumper` 1.2-1+b1, and `anacron` 2.3-43
resolved as supplemental packages while their protected dependencies remained
MattOS-selected. The archives were downloaded and unpacked outside the host
root; ELF dependencies and `anacron`'s pre/post install/remove scripts and
systemd service/timer were inspected.

In an ephemeral networked MattOS guest, Debian `hello` was downloaded over
certificate-verified HTTPS, checked against SHA-256
`4536aabbb75ec21ffe161099ee4b97274945770bdb0682e25ec322421211ca5e`,
installed through MattOS `dpkg` against MattOS `libc6`, executed successfully,
and removed. No protected package was installed, removed, downgraded, or
replaced. In the regular and disconnected guests, local `apt-get update`,
`apt-get -s upgrade`, and `apt-get -s full-upgrade` completed with zero
transactions. The disconnected guest also reinstalled `iputils-ping` from the
embedded repository without attempting either remote, which reflects the
all-remotes-disabled policy of that time.

## Known gaps

- Debian's `systemd` remains blocked by the `-1` pin even though MattOS now
  ships its own `systemd` package.
- MattOS `util-linux` ships only a subset of administration commands (for
  example `lsblk`, `fdisk`, `wipefs`, `findmnt`); nonessential and legacy
  commands are deliberately omitted. `login` owns `login`, `su`, `agetty`, and
  `sulogin`, some of which Debian places in `util-linux`, and `mount` owns the
  mount commands.
- `curl` still owns `libcurl.so.4`; a separate Trixie `libcurl4t64` package is
  not claimed.
- Debian `libssl3t64` also owns `libcrypto.so.3`; MattOS keeps crypto in
  `mattos-libcrypto3` and uses an exact dependency.
- MattOS `libpam0g` has a separate `mattos-libpam-misc0`; Debian includes that
  SONAME in `libpam0g`.
- `mattos-libtinfow6` is not Debian `libtinfo6`, and `mattos-libproc2` has a
  different SONAME from Trixie's `libproc2-0`; neither false-provides it.
- Trixie uses GCC 14 development splits. MattOS's GCC 15 development packages
  remain MattOS-specific, so packages with exact `libgcc-14-dev` or
  `libstdc++-14-dev` dependencies are unsupported.
- Locale breadth, documentation, optional plugins, Perl maintainer tooling,
  complete triggers/alternatives helpers, foreign architectures, and arbitrary
  maintainer-script behavior remain incomplete. The local `file:` repository
  is unsigned and relies on `Trusted: yes`; the hosted repository is signed
  and verified with the packaged `gpgv`, and installed systems set
  `Acquire::AllowInsecureRepositories "false"`.

CPython, Git, Rust (`rustc`, `cargo`), KDE Plasma, the installer, and hosted
publication (`DevUtils/PublishPackages.py`) now exist. Still absent are Perl,
Autotools, pkg-config/pkgconf, Meson, Ninja, and CMake as MattOS packages.
