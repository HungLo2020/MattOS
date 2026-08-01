# MattOS Userland Commands

This document tracks command provenance for the MattOS base userland.

## Build Snapshot

- Date: 2026-08-01
- ISO: `out/images/mattos-x86_64.iso`
- ISO size: `63,363,072` bytes (about `61M`)

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

- `implemented_upstream`: `168`
- `compiled`: `166`
- `installed`: `168`
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

The generated inventory is the exact full list. This build records 168 installed provider/command pairs. The newly added portion is:

```text
kmod: depmod insmod kmod lsmod modinfo modprobe rmmod
ncurses: clear infocmp tic toe tput
procps-ng: free hugetop pgrep pidof pkill pmap ps pwdx slabtop sysctl tload top uptime vmstat w watch
```

The existing Brush, Linux-PAM, Shadow, sudo-rs, util-linux, uutils/coreutils, grep, sed, findutils, and diffutils entries remain in the machine-readable file. `uutils/coreutils:uptime` moved to `intentionally_excluded`; `procps-ng:uptime` is installed.
