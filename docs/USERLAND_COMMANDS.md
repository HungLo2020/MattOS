# MattOS Userland Commands

This document tracks command provenance for the MattOS base userland.

## Build Snapshot

- Date: 2026-08-01
- ISO: `out/images/mattos-x86_64.iso`
- ISO size: `74,125,312` bytes (about `71M`)

## Upstream Commits

- `uutils/coreutils`: `91f6543cad721aba0bf17806e803e84a116f8603`
- `uutils/grep`: `3e5552d8f78a94fb14149a7d3ba3f642725aafb9`
- `uutils/sed`: `7239fb0e08d7d3ba2742ecc3c28f0d0e3eb5a4dd`
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

Measured counts from this build:

- `implemented_upstream`: `181`
- `compiled`: `179`
- `installed`: `181`
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

- Binary: `grep`
- Installed path: `/usr/bin/grep`
- Provider label: `uutils/grep`

### uutils/sed

- Binary: `sed`
- Installed path: `/usr/bin/sed`
- Provider label: `uutils/sed`

### uutils/findutils

- Binaries: `find`, `xargs`, `locate`, `updatedb`
- Installed path prefix: `/usr/bin/`
- Provider label: `uutils/findutils`

### uutils/diffutils

- Upstream binary currently built: `diffutils` (multicall style)
- Installed path: `/usr/bin/diffutils`
- Exposed aliases: `diff`, `cmp`
- Provider label: `uutils/diffutils`
- Dispatch behavior verified:
	- `diffutils` with no explicit subcommand prints multicall usage and available functions.
	- `diffutils diff ...` dispatches to `diff`.
	- Symlink argv0 dispatch works (`/tmp/mattos-diff` invokes `diff` mode).
- Compatibility gap (tracked): `diff3`, `sdiff` (not implemented in this revision)

### util-linux (traditional C implementation)

- Binary retained for tty login: `agetty`
- Installed path: `/usr/sbin/agetty`
- Provider label: `util-linux`
- This remains intentionally separate from Rust/uutils command expansion.

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
- `ping` uses Linux ICMP datagram sockets allowed by `/etc/sysctl.d/99-mattos-network.conf`; the initramfs format does not preserve file capabilities, so MattOS does not make `ping` setuid or depend on `setcap`.
- curl is intentionally limited to HTTP and HTTPS, uses OpenSSL, and defaults to `/etc/ssl/certs/ca-certificates.crt`.
- These systemd clients connect to the dbus-broker system bus as the non-root live user. Read-only inspection works; administrative calls remain policy-controlled and may require root because MattOS does not include Polkit.
- `systemctl --user` and `busctl --user` instead connect to the current UID's per-user manager and broker through `/run/user/$UID`; they do not grant system-service privileges.

### Brush shell and built-ins

- Shell binary: `brush` at `/usr/bin/brush`
- Login shell symlink: `/usr/bin/sh -> /bin/brush`
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

## Duplicate Ownership Check

- Result: no duplicate command/provider conflicts detected.

## Installed command snapshot

The generated inventory is the exact full list. This build records 181 installed provider/command pairs. The networking and system-bus portion is:

```text
curl: curl
dbus-broker: dbus-broker dbus-broker-launch
iproute2: bridge ip ss tc
iputils: ping tracepath
kmod: depmod insmod kmod lsmod modinfo modprobe rmmod
ncurses: clear infocmp tic toe tput
procps-ng: free hugetop pgrep pidof pkill pmap ps pwdx slabtop sysctl tload top uptime vmstat w watch
systemd: busctl networkctl resolvectl timedatectl
```

The existing Brush, Linux-PAM, Shadow, sudo-rs, util-linux, uutils/coreutils, grep, sed, findutils, and diffutils entries remain in the machine-readable file. `uutils/coreutils:uptime` moved to `intentionally_excluded`; `procps-ng:uptime` is installed.
