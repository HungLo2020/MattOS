# MattOS Userland Commands

This document tracks command provenance for the MattOS base userland.

## Build Snapshot

Sizes and counts below are from the build output of 2026-09-27; they change
with every build. `out/reports/artifacts.tsv` (from `mattos-build artifacts`)
is the authoritative size record for a given build.

- ISO: `out/images/mattos-x86_64.iso`
- ISO size: `3,773,081,600` bytes (about 3.5 GiB; the KDE Plasma live root
  and the offline package repository dominate it)

## Upstream Commits

- `uutils/coreutils`: `91f6543cad721aba0bf17806e803e84a116f8603`
- `uutils/grep`: `3e5552d8f78a94fb14149a7d3ba3f642725aafb9`
- GNU `sed`: `89b7a2224d4faa9d8baf76094b1232ad1477ef3e` (`v4.10`)
- `dash`: `037bbdfd330017c368caf6242f977974123239b5` (`v0.5.13.5`)
- `mawk`: `cbefe06ea693a204ebf73f5fe0e1259326339265` (`t20260302`)
- `rsync`: `04355d27b7386d7de0e6bd5e79c556223210f700` (`v3.5.1`)
- `uutils/findutils`: `6ef1fd6cd4885c2970ea99a6d259c9c911a18e04`
- `uutils/diffutils`: `4e8c5099485af4b15fa0b0221d51a5316ca43ad3`
- `util-linux`: `fd82c4043fab942b889f478800118c66edfbc39f`
- `kmod`: `5086df53090b2fe9fa1c31351c05a78a12a4ba71`
- `procps-ng`: `619562d36cbd48fb6958043577558cbc32a6ba79`
- `ncurses`: `c7556ecbc951326acab37c9cf1e7d690456959e0`
- `iproute2`: `5696fee4c69fe3cc12e8cc821630633f616db8e2`
- `iputils`: `75cd9d544baad45f81ed5c72bca332f577c3d81e`
- `curl`: `527573490eb2564b3d7c9dd51d8bff963b5d6303`
- `dbus-broker`: `2956b5d381deeea709c53d02f10e799e50e44f4b`
- `gzip`: `fbc4883eb9c304a04623ac506dd5cf5450d055f1` (`v1.14`)
- `patch`: `48ceda8200aaf30c3ce42c31cd70ff6087db2425` (`v2.8`)
- `file`: `eb754ace19fed5481d8142426543100a2d6bae4e` (`FILE5_48`)
- `less`: `7ea9586a9a1273eb9658d76af8986fdcf6738096` (`v704`)
- `Git`: `e9019fcafe0040228b8631c30f97ae1adb61bcdc` (`v2.55.0`)
- `OpenSSH portable`: `e8dd756725e8800fcd0b3fd71ee6b4382d1e8fab` (`V_10_4_P1`)

## Inventory Source

The build pipeline writes the machine-readable inventory to:

- `out/build/rootfs/usr/share/mattos/userland-commands.txt`

That file contains five sections:

- `implemented_upstream`
- `compiled`
- `installed`
- `intentionally_excluded`
- `failed_compatibility`

Entries use `provider:command` format.

Counts from the 2026-09-27 build:

- `implemented_upstream`: `236`
- `compiled`: `234`
- `installed`: `237`
- `intentionally_excluded`: `3`
- `failed_compatibility`: `2`

## Command Providers

### uutils/coreutils

- Built as a multicall binary at `/usr/bin/coreutils`.
- Applet links are generated dynamically from `coreutils --list`.
- Current provider label: `uutils/coreutils`.
- Applets reported by `coreutils --list`: `107`
- Applets exposed in MattOS: `106` (`uptime` is owned by procps-ng)

### uutils/grep

- Binary: `grep`, from the `grep` package (Essential, as in Debian)
- Installed path: `/usr/bin/grep`
- Provider label: `uutils/grep`

### GNU sed

- Binary: `sed`, from the `sed` package (Essential, as in Debian)
- Installed path: `/usr/bin/sed`
- Provider label: `sed`
- GNU sed replaced uutils sed: running an Autotools `config.status` with uutils
  sed produced an empty `Makefile`, so native builds could not use it. The
  package `Replaces: mattos-base-runtime`, which shipped the uutils binary.

### dash

- Binaries: `dash`, and `/usr/bin/sh` as a symlink to it (package `dash`,
  Essential)
- Provider label: `dash`
- dash is `/bin/sh`, as in Debian: every `#!/bin/sh` script and every dpkg
  maintainer script runs under it. Brush remained the login shell and `bash`
  (below); as `/bin/sh` it deadlocked in rsync's generated `configure`.
- `dash` replaces `mattos-brush`, so on an existing system installing it takes
  `/usr/bin/sh` over from the old brush package with no moment without
  `/bin/sh`; `mattos-brush` depends on `dash`, which is Essential, so APT
  installs and configures dash first.

### mawk

- Binaries: `mawk`, and `/usr/bin/awk` as a symlink to it (package `mawk`)
- Provider label: `mawk`
- Version `1.3.4.<snapshot date>`, as Debian numbers mawk snapshots.

### rsync

- Binary: `rsync` (package `rsync`, in the base system)
- Provider label: `rsync`
- Linked against the MattOS acl, popt, zlib, zstd, lz4, xxhash and OpenSSL
  libraries; IDN support is disabled because MattOS does not ship libidn2.

### uutils/findutils

- Binaries: `find`, `xargs`, `locate`, `updatedb`, from the `findutils`
  package (Essential, as in Debian)
- Installed path prefix: `/usr/bin/`
- Provider label: `uutils/findutils`

### uutils/diffutils

- Upstream binary currently built: `diffutils` (multicall style), from the
  `diffutils` package (Essential, as in Debian)
- Installed path: `/usr/bin/diffutils`
- Exposed aliases: `diff`, `cmp`
- Provider label: `uutils/diffutils`
- Dispatch behavior verified:
	- `diffutils` with no explicit subcommand prints multicall usage and available functions.
	- `diffutils diff ...` dispatches to `diff`.
	- Symlink argv0 dispatch works (`/tmp/mattos-diff` invokes `diff` mode).
- Compatibility gap (tracked): `diff3`, `sdiff` (not implemented in this revision)

The `grep`, `findutils` and `diffutils` packages were split out of
`mattos-base-runtime`, which used to ship these commands; each
`Replaces: mattos-base-runtime`, so installing them takes the files over on an
existing system. `mattos-base` depends on all three.

### util-linux (traditional C implementation)

- Authentication commands remain split into the existing `login` and `mount`
  package families: `agetty`, `login`, `su`, `mount`, and `umount`.
- The base `util-linux` package adds the deliberately selected administration
  set: `lsblk`, `dmesg`, `fdisk`, `cfdisk`, `sfdisk`, `wipefs`, `blkid`,
  `findmnt`, `losetup`, `mountpoint`, `blockdev`, `flock`, `lscpu`, `lslocks`,
  `lsns`, `nsenter`, `unshare`, `taskset`, `chrt`, `ionice`, `prlimit`, and
  `uuidgen`.
- Provider label: `util-linux`
- This remains intentionally separate from Rust/uutils command expansion and
  avoids installing every upstream helper into the live base image.

### Base compression and maintenance tools

- `gzip`: `gzip`, `gunzip`, `zcat`
- `bzip2`: `bzip2`, `bunzip2`, `bzcat`, `bzip2recover`
- `xz-utils`: `xz`, `unxz`, `xzcat`, `lzma`, `unlzma`, `lzcat`
- `zstd`: `zstd`, `unzstd`, `zstdcat`
- GNU Patch: `patch`
- libmagic: `file`, with the package-owned `/usr/share/misc/magic.mgc`
- less: `less`, `lesskey`, and `/usr/libexec/lessecho`, backed by MattOS
  ncurses/terminfo and PCRE2.

GNU gzip, GNU Patch, GNU sed, dash, rsync and less use checksum-verified official release archives
to supply generated release inputs missing from their exact Git revisions. The
archives are extracted only into `out/build/<component>/source`; authoritative
vendored source is never regenerated or modified.

### Git

- Git and Scalar are installed from the pinned Git source and use MattOS-built
  curl, OpenSSL, zlib, zstd, expat, and PCRE2.
- Normal local repository operations and the `git-remote-http(s)` helpers are
  included.
- Perl, Python, Tcl/Tk, gettext, and Rust-dependent optional Git features are
  deliberately omitted from the base Git package. Upstream's explicit
  unsupported-command stubs may remain in Git's private exec path; they do not
  imply those optional runtimes are available.

### OpenSSH

- Client commands: `ssh`, `scp`, `sftp`, `ssh-add`, `ssh-agent`, `ssh-keygen`,
  and `ssh-keyscan`.
- Server: `/usr/sbin/sshd`, package-owned secure configuration, PAM policy,
  sysusers entry, and `ssh.service` integration.
- Host keys are generated on the installed/runtime system; no mutable host key
  material is baked into the image.

### kmod

- Commands: `kmod`, `modprobe`, `insmod`, `rmmod`, `lsmod`, `modinfo`, `depmod`
- Paths: `/usr/bin/kmod` and `/usr/sbin/*`
- Provider label: `kmod`

### procps-ng

- Commands: `ps`, `top`, `free`, `uptime`, `pgrep`, `pkill`, `pidof`, `watch`, `sysctl`, `vmstat`, `w`, `pmap`, `pwdx`, `tload`, `slabtop`, `hugetop`
- Provider label: `procps-ng`
- The uutils `uptime` link is intentionally excluded so ownership remains unique.

### ncurses

- Commands: `clear`, `tput`, `tic`, `toe`, `infocmp`
- Provider label: `ncurses`
- These are real ncurses executables backed by the selected compiled terminfo database.

### Networking

- `iproute2`: `ip`, `ss`, `bridge`, `tc`
- `iputils`: `ping`, `tracepath`
- `curl`: `curl`
- `systemd`: `busctl`, `loginctl`, `networkctl`, `resolvectl`, `timedatectl`
- `dbus-broker`: `dbus-broker`, `dbus-broker-launch`
- `ping` uses Linux ICMP datagram sockets allowed by `/etc/sysctl.d/99-mattos-network.conf` (`net.ipv4.ping_group_range`), so it needs neither a setuid bit nor a file capability.
- curl is intentionally limited to HTTP and HTTPS, uses OpenSSL, and defaults to `/etc/ssl/certs/ca-certificates.crt`.
- These systemd clients connect to the dbus-broker system bus as the non-root live user. Read-only inspection works; administrative calls are authorized by Polkit policy.
- NetworkManager's `nmcli` and wpa_supplicant's `wpa_cli` are also installed; they come from system packages outside this base-userland inventory (see [MattOS Wired/QEMU Networking](../networking.md)).
- `systemctl --user` and `busctl --user` instead connect to the current UID's per-user manager and broker through `/run/user/$UID`; they do not grant system-service privileges.

### Brush shell and built-ins

- Shell binary: `brush` at `/usr/bin/brush`
- Package-owned compatibility entry point: `/usr/bin/bash -> brush`; the merged
  `/bin` layout therefore also provides `/bin/bash`. Brush is the login shell;
  `/usr/bin/sh` belongs to dash (above).
- MattOS applies a checksummed output-mirror patch so Brush selects POSIX mode
  when invoked as `sh` (it no longer is by default); `bash` and `brush` retain
  Bash-compatible behavior.
- A second output-mirror patch fixes an upstream parser bug that read nested
  subshells written `( ( ... ) )` as an arithmetic command. As in Bash, only
  adjacent `((` opens an arithmetic command.
- Provider label in inventory for shell binary: `brush`
- Built-ins are internal to Brush and are not listed as standalone ELF binaries.

## Notes

- Command collision checks are enforced during rootfs assembly.
- Missing required userland executables fail the build early.
- The inventory file should be treated as the runtime truth for a specific build output.

## Compatibility Gaps

- `uutils/diffutils:diff3` intentionally excluded.
- `uutils/diffutils:sdiff` intentionally excluded.
- `failed_compatibility` reasons in inventory:
	- `uutils/diffutils:diff3 (not implemented upstream)`
	- `uutils/diffutils:sdiff (not implemented upstream)`
- util-linux programs outside the selected base set remain available for later
  package expansion; hardware/destructive and specialized helpers are not
  installed merely because upstream built them.
- Git's Perl/Python/Tcl/Tk/gettext optional tooling is not built into the
  base Git package.
- OpenSSH security-key middleware and optional platform integrations require
  their respective future MattOS packages; core client/server and PAM paths do
  not depend on them.

## Duplicate Ownership Check

- Result: no duplicate command/provider conflicts detected.

## Installed command snapshot

The generated inventory is the exact full list. The 2026-09-27 build records 237 installed provider/command pairs. The networking, system-bus and process portion is:

```text
curl: curl
dbus-broker: dbus-broker dbus-broker-launch
iproute2: bridge ip ss tc
iputils: ping tracepath
kmod: depmod insmod kmod lsmod modinfo modprobe rmmod
ncurses: clear infocmp tic toe tput
procps-ng: free hugetop pgrep pidof pkill pmap ps pwdx slabtop sysctl tload top uptime vmstat w watch
systemd: busctl loginctl networkctl resolvectl timedatectl
```

The Brush, Linux-PAM, Shadow, sudo-rs, util-linux, OpenSSH, Git, compression-tool, uutils/coreutils, grep, sed, findutils, and diffutils entries are in the machine-readable file. `uutils/coreutils:uptime` moved to `intentionally_excluded`; `procps-ng:uptime` is installed.
