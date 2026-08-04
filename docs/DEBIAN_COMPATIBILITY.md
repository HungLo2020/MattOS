# Debian 13 compatibility contract

MattOS targets best-effort binary package compatibility with Debian 13
(Trixie) on `amd64` while retaining a MattOS-built and MattOS-controlled
critical base. MattOS repositories take precedence. Debian is a supplemental
source for optional software and may not replace protected infrastructure.
This is a compatibility target, not a promise that every Debian package works.

The machine-readable contract is
`src/system/packages/debian-compat/trixie.toml`. It maps all 65 installed
packages to source, representative owned paths, ABI or command surface,
protection, deterministic version, Debian dependency role, classification, and
known gaps. `protected.toml` is the authoritative protected-name inventory.
The build rejects an incomplete mapping, invalid classification or version,
missing protected pin, unsafe source configuration, changed LinuxScripts
publisher, or nested Git metadata.

## Contracted interfaces

| Interface | MattOS contract |
| --- | --- |
| package identity | Real Trixie binary names are used only where the current payload is a credible replacement. MattOS-only packages keep `mattos-`. |
| versions | Debian syntax and `dpkg` comparison; releases use `<upstream>-1mattos<N>`, snapshots `0~git.<12hex>-1mattos<N>`, never timestamps. |
| architecture | `Architecture: amd64`; no foreign-architecture or multiarch co-install contract yet. |
| libraries | Runtime DSOs use `/usr/lib/x86_64-linux-gnu`, with Debian-relevant SONAME and symbol-version checks recorded by the ELF audit. |
| loader | Dynamic executables use `/lib64/ld-linux-x86-64.so.2`. |
| filesystem | merged `/usr`: `/bin`, `/sbin`, and `/lib` resolve into `/usr`; package paths and common commands remain conventional. |
| package state | `/var/lib/dpkg` is initialized as mutable state and populated through real `dpkg`; packages never ship its status, locks, or generated `info` data. |
| APT | deb822 sources, `/etc/apt/preferences.d`, conventional cache/list/log directories, and the embedded `file:` repository are present. |
| maintainer scripts | `dpkg` supplies Brush through both `/bin/sh` and `/bin/bash`; basic pre/post install/remove scripts are supported. Perl-based helpers are not. |
| systemd | PID 1, system units, enablement links, D-Bus, logind, and `pam_systemd` work, but the systemd executable tree is not yet owned by a `systemd` package. |
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
`libkmod2`, `kmod`, `procps`, `libsystemd0`, `libudev1`, `libexpat1`,
`libcap2`, `libacl1`, `zlib1g`, `libbz2-1.0`, `liblz4-1`, `liblzma5`,
`libxxhash0`, `tar`, `dbus-broker`, `libpam0g`, `libpam-modules`,
`libpam-runtime`, `passwd`, `login`, `iproute2`, and `iputils-ping`.

MattOS-specific packages are `mattos-filesystem`, `mattos-base-files`,
`mattos-brush`, `mattos-gcc-common`, `mattos-libgcc-dev`,
`mattos-libstdc++-dev`, `mattos-libcrypto3`, `mattos-libtinfow6`,
`mattos-libproc2`, `mattos-libpam-misc0`, and `mattos-sudo-rs`. The GCC 15
development packages deliberately do not claim Trixie's GCC 14 identities.

## Versions and protected transactions

Release branches are converted to deterministic upstream versions. A moving
branch that cannot supply a release version sorts conservatively as a
`0~git...` snapshot and relies on repository policy rather than an artificially
high version. Tests exercise Debian 13 versions, epochs, `~` prereleases,
MattOS revisions, and downgrade ordering with `dpkg --compare-versions`.

APT priority is:

1. embedded local MattOS: `1001`, matching `o=MattOS,l=MattOS Local,n=trixie`;
2. hosted MattOS: `990`, matching `o=MattOS,l=MattOS,n=trixie`;
3. Debian Trixie: `500`, matching `o=Debian,n=trixie`.

Every name in `protected.toml` has an additional Debian-origin priority of
`-1`. Reserved gap names such as `systemd`, `util-linux`, Debian's GCC 14
development packages, and kernel metapackages are protected even before MattOS
ships a matching package. This prevents Debian from silently taking ownership
of files already supplied through another MattOS boundary.

The local repository publishes `Origin: MattOS`, `Label: MattOS Local`,
`Suite: trixie`, and `Codename: trixie`. Its unsigned `file:` source alone uses
the temporary `Trusted: yes` bootstrap exception. Hosted MattOS and Debian
deb822 sources are shipped disabled so offline live boot never contacts them;
they use `Signed-By` paths and never use `Trusted: yes`. Enabling them is a
future administrative action after their keyrings and APT remote methods are
installed.

## Controlled Debian test

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
embedded repository without attempting either remote.

## Known gaps

- The systemd executable/unit tree is assembled directly and is not yet owned
  by a `systemd` binary package. Debian `systemd` remains blocked.
- MattOS has no accurate `util-linux` aggregate package; `login` currently owns
  `login`, `su`, and `agetty`, while `mount` owns mount commands.
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
  complete triggers/alternatives helpers, foreign architectures, repository
  signing, and arbitrary maintainer-script behavior remain incomplete.

Future work—not part of this milestone—includes CPython, Perl, Autotools,
pkg-config, Meson/Ninja/CMake, Git, Rust/Cargo/rustup, complete native rebuild
and ISO generation, COSMIC, installer technology, and hosted publication.
