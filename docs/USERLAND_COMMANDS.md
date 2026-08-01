# MattOS Userland Commands

This document tracks command provenance for the MattOS base userland.

## Build Snapshot

- Date: 2026-07-31
- ISO: `out/images/mattos-x86_64.iso`
- ISO size: `57,403,392` bytes (about `55M`)

## Upstream Commits

- `uutils/coreutils`: `91f6543cad721aba0bf17806e803e84a116f8603`
- `uutils/grep`: `3e5552d8f78a94fb14149a7d3ba3f642725aafb9`
- `uutils/sed`: `7239fb0e08d7d3ba2742ecc3c28f0d0e3eb5a4dd`
- `uutils/findutils`: `6ef1fd6cd4885c2970ea99a6d259c9c911a18e04`
- `uutils/diffutils`: `4e8c5099485af4b15fa0b0221d51a5316ca43ad3`
- `util-linux`: `fd82c4043fab942b889f478800118c66edfbc39f`

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

- `implemented_upstream`: `118`
- `compiled`: `116`
- `installed`: `119`
- `intentionally_excluded`: `2`
- `failed_compatibility`: `2`

## Command Providers

### uutils/coreutils

- Built as a multicall binary at `/usr/bin/coreutils`.
- Applet links are generated dynamically from `coreutils --list`.
- Current provider label: `uutils/coreutils`.
- Applets reported by `coreutils --list`: `107`
- Applets exposed in MattOS: `107`

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

## Installed Commands (provider:command)

Exact installed entries from `out/build/rootfs/usr/share/mattos/userland-commands.txt`:

```text
brush:brush
brush:sh
util-linux:agetty
uutils/coreutils:[
uutils/coreutils:arch
uutils/coreutils:b2sum
uutils/coreutils:base32
uutils/coreutils:base64
uutils/coreutils:basename
uutils/coreutils:basenc
uutils/coreutils:cat
uutils/coreutils:chgrp
uutils/coreutils:chmod
uutils/coreutils:chown
uutils/coreutils:chroot
uutils/coreutils:cksum
uutils/coreutils:comm
uutils/coreutils:cp
uutils/coreutils:csplit
uutils/coreutils:cut
uutils/coreutils:date
uutils/coreutils:dd
uutils/coreutils:df
uutils/coreutils:dir
uutils/coreutils:dircolors
uutils/coreutils:dirname
uutils/coreutils:du
uutils/coreutils:echo
uutils/coreutils:env
uutils/coreutils:expand
uutils/coreutils:expr
uutils/coreutils:factor
uutils/coreutils:false
uutils/coreutils:fmt
uutils/coreutils:fold
uutils/coreutils:groups
uutils/coreutils:head
uutils/coreutils:hostid
uutils/coreutils:hostname
uutils/coreutils:id
uutils/coreutils:install
uutils/coreutils:join
uutils/coreutils:kill
uutils/coreutils:link
uutils/coreutils:ln
uutils/coreutils:logname
uutils/coreutils:ls
uutils/coreutils:md5sum
uutils/coreutils:mkdir
uutils/coreutils:mkfifo
uutils/coreutils:mknod
uutils/coreutils:mktemp
uutils/coreutils:more
uutils/coreutils:mv
uutils/coreutils:nice
uutils/coreutils:nl
uutils/coreutils:nohup
uutils/coreutils:nproc
uutils/coreutils:numfmt
uutils/coreutils:od
uutils/coreutils:paste
uutils/coreutils:pathchk
uutils/coreutils:pinky
uutils/coreutils:pr
uutils/coreutils:printenv
uutils/coreutils:printf
uutils/coreutils:ptx
uutils/coreutils:pwd
uutils/coreutils:readlink
uutils/coreutils:realpath
uutils/coreutils:rm
uutils/coreutils:rmdir
uutils/coreutils:seq
uutils/coreutils:sha1sum
uutils/coreutils:sha224sum
uutils/coreutils:sha256sum
uutils/coreutils:sha384sum
uutils/coreutils:sha512sum
uutils/coreutils:shred
uutils/coreutils:shuf
uutils/coreutils:sleep
uutils/coreutils:sort
uutils/coreutils:split
uutils/coreutils:stat
uutils/coreutils:stdbuf
uutils/coreutils:stty
uutils/coreutils:sum
uutils/coreutils:sync
uutils/coreutils:tac
uutils/coreutils:tail
uutils/coreutils:tee
uutils/coreutils:test
uutils/coreutils:timeout
uutils/coreutils:touch
uutils/coreutils:tr
uutils/coreutils:true
uutils/coreutils:truncate
uutils/coreutils:tsort
uutils/coreutils:tty
uutils/coreutils:uname
uutils/coreutils:unexpand
uutils/coreutils:uniq
uutils/coreutils:unlink
uutils/coreutils:uptime
uutils/coreutils:users
uutils/coreutils:vdir
uutils/coreutils:wc
uutils/coreutils:who
uutils/coreutils:whoami
uutils/coreutils:yes
uutils/diffutils:cmp
uutils/diffutils:diff
uutils/diffutils:diffutils
uutils/findutils:find
uutils/findutils:locate
uutils/findutils:updatedb
uutils/findutils:xargs
uutils/grep:grep
uutils/sed:sed
```
