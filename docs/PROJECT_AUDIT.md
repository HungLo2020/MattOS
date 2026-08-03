# MattOS Project Audit

Date: 2026-08-03

## 1. Executive Summary

MattOS is a coherent Linux-native bootstrap system with systemd PID 1, separate system and per-user dbus-broker buses, registered logind console sessions, session-bound per-user managers, non-root live autologin, PAM/Shadow/sudo-rs authentication and account tools, Brush, a rescue-init path, and a reproducible build pipeline. Fifty-five MattOS packages are installed through a real dpkg database from an embedded local repository. GNU glibc, the GCC shared runtimes, PCRE2, SELinux userspace compatibility, libxcrypt, and the util-linux mount closure are source-built and package-owned. MattOS-built dpkg and APT work from the embedded repository with or without the QEMU NIC. The final ISO contains no host-derived executable or runtime-library payloads. Persistent installation, an online repository, Polkit, SSH, Wi-Fi, firewall policy, firmware packaging, and a graphical desktop remain intentionally absent.

The previous GRUB source-of-truth ambiguity has been resolved by keeping only `src/boot/grub/grub.cfg` as tracked source and validating that path in `mattos-build`. Runtime libc, GCC runtimes, and native consumers use the controlled MattOS sysroot. Host compiler, assembler, linker, and package-construction tools remain explicit build-time bootstrap inputs; MattOS is not yet self-hosting.

The static Brush prompt source has been replaced. The interactive prompt now comes from MattOS-owned startup configuration using normal Brush/Bash-style prompt semantics.

## 2. Current Boot Architecture

Boot flow:

1. GRUB loads the Linux kernel and initramfs.
2. The normal entry starts PID 1 at `/usr/lib/systemd/systemd` with `systemd.unit=mattos.target`.
3. `mattos.target` pulls in `multi-user.target` and `getty@tty1.service`.
4. `getty@tty1.service` is overridden to autologin the non-root `mattos` live user through `/bin/login` and PAM.
5. The account's `/bin/brush` login shell reads the MattOS profile and starts with the merged-`/usr` command PATH.
6. The rescue entry starts `mattos-init`, which mounts the pseudo-filesystems and spawns Brush directly.

Prompt behavior:

- The previous fixed `MattOS # ` prompt came from the rescue init shell environment in `src/userland/init/src/main.rs`.
- The current dynamic prompt is set in `src/rootfs/skeleton/etc/profile` as `\u@\h:\w\$ `.
- The rescue init path was updated to use the same dynamic prompt semantics.

## 3. Current Source Components and Exact Upstream Commits

| Component | Upstream URL | Branch | Imported commit | Destination |
| --- | --- | --- | --- | --- |
| Linux | https://github.com/torvalds/linux.git | `master` | `f17f39c917cd4aac09db1a6a083ef5ec09b4924d` | `src/kernel/linux/` |
| GNU glibc | `git://sourceware.org/git/glibc.git` | `master` / `glibc-2.43` | `f762ccf84f122d1354f103a151cba8bde797d521` | `src/system/libc/glibc/` |
| GCC | `https://gcc.gnu.org/git/gcc.git` | `releases/gcc-15.3.0` | `4db0e8df15bef836558857c291c323add11d035c` | `src/toolchain/gcc/` |
| Brush | https://github.com/reubeno/brush.git | `main` | `71afef7ce79ad2fd94833fa4f93fa5486c86c56b` | `src/userland/brush/` |
| uutils/coreutils | https://github.com/uutils/coreutils.git | `main` | `91f6543cad721aba0bf17806e803e84a116f8603` | `src/userland/coreutils/` |
| util-linux | https://github.com/util-linux/util-linux.git | `master` | `fd82c4043fab942b889f478800118c66edfbc39f` | `src/userland/util-linux/` |
| systemd | https://github.com/systemd/systemd.git | `main` | `91d2131e20ca304ee1d9dabf71b351d6b4cfcddc` | `src/system/systemd/` |
| dbus-broker | https://github.com/bus1/dbus-broker.git | `main` | `2956b5d381deeea709c53d02f10e799e50e44f4b` | `src/system/dbus/dbus-broker/` |
| kmod | https://github.com/kmod-project/kmod.git | `master` | `5086df53090b2fe9fa1c31351c05a78a12a4ba71` | `src/system/kmod/` |
| procps-ng | https://gitlab.com/procps-ng/procps.git | `master` | `619562d36cbd48fb6958043577558cbc32a6ba79` | `src/userland/procps-ng/` |
| ncurses | https://github.com/ThomasDickey/ncurses-snapshots.git | `master` | `c7556ecbc951326acab37c9cf1e7d690456959e0` | `src/system/terminal/ncurses/` |
| iproute2 | https://git.kernel.org/pub/scm/network/iproute2/iproute2.git | `main` | `5696fee4c69fe3cc12e8cc821630633f616db8e2` | `src/userland/iproute2/` |
| iputils | https://github.com/iputils/iputils.git | `master` | `75cd9d544baad45f81ed5c72bca332f577c3d81e` | `src/userland/iputils/` |
| curl | https://github.com/curl/curl.git | `master` | `527573490eb2564b3d7c9dd51d8bff963b5d6303` | `src/userland/curl/` |
| dpkg | https://git.dpkg.org/git/dpkg/dpkg.git | `main` | `ff7e9d8bf01379e8b022028a65afaa262e2c25cd` | `src/system/packages/dpkg/` |
| APT | https://salsa.debian.org/apt-team/apt.git | `main` | `5e6dcc8d0c8bdce61e9cc7f497abadb5349d509a` | `src/system/packages/apt/` |
| Expat | https://github.com/libexpat/libexpat.git | `master` | `236c3f8f949209501b568032553c17577901c7ec` | `src/system/libraries/expat/` |
| libcap | https://git.kernel.org/pub/scm/libs/libcap/libcap.git | `master` | `bd54ca54ff9fc963954f11ffd9acffbaf1447723` | `src/system/libraries/libcap/` |
| LZ4 | https://github.com/lz4/lz4.git | `v1.10.0` | `ebb370ca83af193212df4dcbadcc5d87bc0de2f0` | `src/system/libraries/lz4/` |
| XZ Utils | https://github.com/tukaani-project/xz.git | `v5.8.1` | `a522a226545730551f7e7c2685fab27cf567746c` | `src/system/libraries/xz/` |
| xxHash | https://github.com/Cyan4973/xxHash.git | `v0.8.3` | `e626a72bc2321cd320e953a0ccf1584cad60f363` | `src/system/libraries/xxhash/` |
| Zstandard | https://github.com/facebook/zstd.git | `v1.5.7` | `f8745da6ff1ad1e7bab384bd1f9d742439278e99` | `src/system/libraries/zstd/` |
| OpenSSL | https://github.com/openssl/openssl.git | `openssl-3.5.7` | `8cf17aaeb4599f8af87fefd810b5b5fee90fe69e` | `src/system/libraries/openssl/` |
| elfutils | https://sourceware.org/git/elfutils.git | `elfutils-0.195` | `302252356da5475670ac5b10dadd091c59689425` | `src/system/libraries/elfutils/` |
| PCRE2 | https://github.com/PCRE2Project/pcre2.git | `pcre2-10.47` | `f454e231fe5006dd7ff8f4693fd2b8eb94333429` | `src/system/libraries/pcre2/` |
| SLJIT build support | https://github.com/zherczeg/sljit.git | `master` | `45f910b78c6605ebf5b53d3ec7cb00f2312fe417` | `src/build-support/sljit/` |
| SELinux userspace | https://github.com/SELinuxProject/selinux.git | `3.10` | `ca10fc4204ed60540d41d2499127c18ad0643f9e` | `src/system/security/selinux/` |
| libxcrypt | https://github.com/besser82/libxcrypt.git | `develop` / `v4.4.38` | `55ea777e8d567e5e86ffac917c28815ac54cc341` | `src/system/libraries/libxcrypt/` |
| libmd | https://git.hadrons.org/git/libmd.git | `1.2.0` | `90c4f432134c608c7e2b4dd0a1d7ca5c40b92c7a` | `src/system/libraries/libmd/` |
| libbsd | https://gitlab.freedesktop.org/libbsd/libbsd.git | `0.12.2` | `04a24db27ad1572f766bad772cdd9c146e6d9cf0` | `src/system/libraries/libbsd/` |
| attr | https://git.savannah.nongnu.org/git/attr.git | `v2.6.0` | `c440855d6b33446edf4b5eb1a2d892281f15a99b` | `src/system/libraries/attr/` |

State tracking lives in `upstream/state/*.toml`, and component manifests live in `upstream/sources.toml`.

No Git submodules are used.

## 4. Repository Structure Assessment

The top-level structure is understandable: `src/` holds the active source tree, `DevUtils/` holds host wrappers, `docs/` holds project docs, and `upstream/` holds import metadata.

Confirmed structure concerns:

- GRUB source-of-truth has been consolidated to `src/boot/grub/grub.cfg`; legacy `boot/grub/grub.cfg` was removed.
- Generated build products live at the root in `out/` and `target/`, but imported trees also contain their own `target/` directories (`src/userland/brush/target/`, `src/userland/coreutils/target/`) and Linux build artifacts stay in-tree.
- The source ownership boundary is mostly clear, but imported upstream trees still carry their own build caches and outputs inside the imported directories.

## 5. Build-System Assessment

The build orchestrator is `src/tools/mattos-build/src/main.rs`. It currently owns:

- doctor checks
- upstream import/sync
- kernel build
- Brush build
- coreutils build
- util-linux build
- kmod build
- procps-ng build
- ncurses build and terminfo selection
- iproute2, iputils, and HTTP/HTTPS-only curl builds
- focused Zstandard, OpenSSL libcrypto/libssl, and elfutils libelf builds with explicit staged dependency paths
- focused PCRE2 8-bit, libselinux, and libxcrypt builds with explicit staged dependency paths and ABI checks
- source-built util-linux libblkid/libmount/libsmartcols and mount/umount closure
- systemd build
- dbus-broker build
- init build
- rootfs assembly
- initramfs generation
- ISO generation
- `.deb` staging, metadata, collision checks, inventory, and local APT indexing
- imported dpkg and APT source builds

This is functional but large and tightly coupled. The main risks are maintainability and accidental cross-stage coupling, not current correctness failure.

Positive properties:

- Stage ordering is explicit.
- Kernel path safety checks exist for WSL `/mnt/*` paths.
- Upstream sync uses dirty-tree protection.
- Build outputs are validated before copy/install steps.

Current limitations:

- Rootfs/initramfs/ISO are always regenerated.
- The two remaining host GCC runtime libraries are copied through a narrow, checksum-recorded bootstrap manifest; final ELF resolution uses the MattOS loader rather than `ldd`.
- Kernel is built in-tree.
- The orchestrator is large enough that stage-specific logic should eventually be split into modules.
- Host dpkg/APT utilities still bootstrap archive creation and indexing.
- The complete selected dpkg/APT runtime is packaged, but archive construction and repository indexing still use host package tools.
- APT currently supports the embedded `file:` source only; HTTP methods, signing helpers, and apt-utils are intentionally excluded.

## 6. Runtime and Rootfs Assessment

The assembled rootfs is a merged `/usr` layout with `/bin`, `/sbin`, `/lib`, and `/lib64` symlinked into the `/usr` tree. Fifty-five packages own the initial base and selected source-built payloads, including glibc, libgcc, libstdc++, PCRE2, libselinux, libxcrypt, and the util-linux mount closure. They are installed with real dpkg semantics, and `/var/lib/dpkg` contains normal status, conffile, md5sum, file-list, and ownership data. The local repository is embedded at `/usr/share/mattos/repository`, APT is configured only for that `file:` source, and no Debian or Ubuntu source is configured.

APT mutable lists and archive cache are initialized as writable live state rather than shipped package content. Its selected commands, private library, local methods, helpers, configuration, CA trust, source-built `libapt-pkg`, and exact ELF closure all have package ownership. The retired bootstrap-runtime audit records zero installed host-derived entries.

Core identity files currently present in the skeleton:

- `src/rootfs/skeleton/etc/passwd`: `root:x:0:0:root:/root:/bin/brush`
- `src/rootfs/skeleton/etc/group`: root, tty, utmp groups
- `src/rootfs/skeleton/etc/hostname`: `mattos`
- `src/rootfs/skeleton/etc/os-release`: MattOS identity
- `src/rootfs/skeleton/etc/shells`: includes `/bin/brush`
- `src/rootfs/skeleton/etc/profile`: exports login environment and dynamic prompt

Rootfs modes and directories:

- `/root` is created with mode `0700`.
- `/run`, `/var/log`, `/var/tmp`, `/etc/systemd/system`, and `/usr/libexec/mattos` are created during assembly.
- `/etc/machine-id` is empty, which matches the current ephemeral-image model.

Representative runtime libraries in the image currently include:

- `libblkid.so.1`
- `libc.so.6`
- `libgcc_s.so.1`
- `libm.so.6`
- `libmount.so.1`
- `libsmartcols.so.1`
- `libnss_myhostname.so.2`
- `libnss_systemd.so.2`
- `libpcre2-8.so.0`
- `libselinux.so.1`
- `libsystemd.so.0.44.0`
- `libexpat.so.1`
- `libudev.so.1.7.14`
- `libsystemd-core-262.so`
- `libsystemd-shared-262.so`

glibc, the GCC runtime pair, and the previously migrated libraries above are package-owned source builds. The host-derived executable/runtime-library boundary in the ISO is zero.

## 7. systemd and Login Assessment

The minimal systemd build is intentionally stripped down and produces the current bootable baseline.

Enabled systemd service areas are `networkd`, `resolve`, `timesyncd`, `timedated`, `logind`, and PAM-backed login sessions. Intentionally disabled systemd areas include:

- `homed`, `portabled`, `nspawn`, `bootloader`, `firstboot`, `repart`
- `oomd`, `userdb`, `remote`, `sysupdate`, `sysupdated`, `sysinstall`
- `importd`, `vmspawn`
- `coredump`, `pstore`, `machined`, `hostnamed`, `localed`, `nsresourced`
- `dbus`, `glib`, `seccomp`, `acl`, `audit`, `blkid`
- `libcryptsetup`, `openssl`, `gnutls`, `libfido2`, `tpm`, `tpm2`, `qrencode`, `bpf-framework`
- `kernel-install`, `analyze`, `create-log-dirs`

Consequences:

- Ethernet links use IPv4 DHCP through networkd; DNS goes through resolved and time sync through timesyncd.
- There is no Wi-Fi, SSH, firewall policy, persistent network configuration UI, or broadened physical NIC support.
- Linux-PAM is provided by the separate MattOS authentication build; systemd's PAM feature is enabled specifically to build its compatible `pam_systemd` session module.
- No `systemd-homed`, persistent user database, or home-directory management stack; the per-user service manager is enabled.
- No container/VM spawn tooling.
- No coredump or persistence-oriented service stack.
- `dbus-broker` provides the conventional `/run/dbus/system_bus_socket`; systemd's Meson `dbus` option remains off because it controls optional reference-libdbus integration rather than sd-bus.
- MattOS has no Polkit. Read-only non-root clients work, while privileged D-Bus actions are denied unless run as root or through sudo.

Current unit state:

- `mattos.target` wants `getty@tty1.service`.
- `getty@tty1.service` is overridden for non-root `mattos` live autologin through `/bin/login` and PAM.
- `mattos-shell.service` remains in the tree but is masked and no longer on the active path.
- `systemd-logind.service` and `systemd-logind-varlink.socket` are enabled and healthy; `org.freedesktop.login1` resolves to logind.
- `login`, `su-l`, and `systemd-user` use the optional `pam_systemd` session hook. Logind registers tty1 on `seat0` and ttyS0 without a seat, and starts the UID-scoped runtime directory and user manager.
- `dbus.socket` is enabled, `dbus.service` aliases `dbus-broker.service`, and exactly one launcher/broker pair owns the system bus.
- The user `dbus.socket` listens at `%t/bus`; a separate `--scope user` broker starts on demand for each logged-in UID.
- `mattos-smoke.service` is present as a boot-time diagnostics helper.

This is a conventional ephemeral console-session model, but it is not yet a persistent installed multi-user system.

## 8. Userland Command Inventory

Current commands present in the built image:

`/usr/bin`

```text
brush
busctl
cat
coreutils
echo
journalctl
loginctl
ls
mkdir
mount
pwd
run0
sh
storagectl
systemctl
systemd-ac-power
systemd-ask-password
systemd-cat
systemd-cgls
systemd-cgtop
systemd-confext
systemd-creds
systemd-delta
systemd-detect-virt
systemd-escape
systemd-hwdb
systemd-id128
systemd-inhibit
systemd-machine-id-setup
systemd-mount
systemd-mstack
systemd-mute-console
systemd-notify
systemd-path
systemd-pty-forward
systemd-run
systemd-socket-activate
systemd-stdio-bridge
systemd-sysext
systemd-sysusers
systemd-tmpfiles
systemd-tty-ask-password-agent
systemd-umount
systemd-vpick
touch
udevadm
ukify
uname
varlinkctl
```

`/usr/sbin`

```text
agetty
halt
init
ldconfig
mount.mstack
mount.storage
poweroff
reboot
shutdown
```

`/bin` and `/sbin` are merged symlinks into `/usr/bin` and `/usr/sbin`.

The generated inventory at `/usr/share/mattos/userland-commands.txt` is authoritative. It includes grep, sed, findutils, the PAM/Shadow/sudo-rs administration commands, kmod tools, procps-ng tools, ncurses terminal tools, iproute2, iputils, curl, and systemd's network control commands. Package and installation tools remain absent by design.

## 9. Cache Assessment

Current caching behavior:

- `target/` caches workspace Rust builds.
- `src/userland/brush/target/` and `src/userland/coreutils/target/` cache imported Rust project builds.
- `out/build/systemd/build/` and `out/build/util-linux/build/` cache Meson/Ninja state.
- `out/build/kmod/build/`, `out/build/ncurses/build/`, and `out/build/procps-ng/build/` keep their native build-system caches outside imported source.
- `src/kernel/linux/` keeps in-tree kernel build outputs.
- `out/build/rootfs/`, `out/build/initramfs.cpio.gz`, and `out/images/mattos-x86_64.iso` are regenerated from upstream build artifacts.
- `upstream/.tmp/` style clones are ephemeral and intentionally not preserved.

This is correct for a bootstrap system, but it is not especially efficient. The highest cache risk is the split between host-level Rust targets and imported-tree Rust targets.

## 10. Confirmed Defects

### High

- No currently open high-severity confirmed defect in the audited scope.

### Medium

- No executable or runtime-library files remain host-copied into the image. Host compiler, linker, language toolchains, and image/package construction tools remain outside the ISO as the next self-hosting boundary.
- The build orchestrator is large enough to be a maintenance risk and should eventually be split into smaller modules.

### Low

- `src/rootfs/skeleton/etc/inittab` is still present even though systemd is the actual init system.
- The image contains placeholder `.gitkeep` files in some command directories.

## 10.1 Findings By Category and Severity

The audit classifies each item as one of:

- confirmed defect
- known bootstrap limitation
- architectural risk
- future enhancement

Summary counts:

- Critical: 0
- High: 0
- Medium: 2
- Low: 2
- Informational: 10

Detailed classification:

| Finding | Type | Severity |
| --- | --- | --- |
| Duplicate GRUB configuration paths were resolved by removing `boot/grub/grub.cfg` and validating only `src/boot/grub/grub.cfg` | resolved defect | Informational |
| Host-derived runtime closure via `ldd` copy strategy | known bootstrap limitation | Medium |
| Large monolithic build orchestrator (`src/tools/mattos-build/src/main.rs`) | architectural risk | Medium |
| Legacy `etc/inittab` present while systemd is active init | future enhancement | Low |
| Placeholder `.gitkeep` files included in command dirs in image | future enhancement | Low |
| Upstream sync uses ephemeral shallow clones | known bootstrap limitation | Informational |
| Rootfs/initramfs/ISO always regenerated | known bootstrap limitation | Informational |
| Kernel built in-tree | known bootstrap limitation | Informational |
| systemd feature set intentionally minimized | known bootstrap limitation | Informational |
| Ephemeral non-root live-user autologin policy | known bootstrap limitation | Informational |
| Missing logind session registration and per-user bus | resolved defect | Informational |
| No in-guest automation for graphical validation | architectural risk | Informational |
| Command inventory intentionally narrow | known bootstrap limitation | Informational |
| Remaining unit masking strategy (`ldconfig`/`mattos-shell`) | known bootstrap limitation | Informational |
| Prompt now centralized in MattOS-owned startup config | informational state | Informational |
| Graphical tty1 validation is manual rather than a committed automated harness | known bootstrap limitation | Informational |

## 11. Risks and Technical Debt

- Host-linked runtime libraries are the biggest long-term portability risk.
- The current login model is an ephemeral live-user policy; it is not a persistent installed-system account model.
- There is no persistent installation flow yet.
- There is no automated in-guest command runner for boot smoke validation.
- The system bus and per-user buses are separate and complete for the enabled console services, but MattOS has no Polkit.
- Session state is intentionally ephemeral and lingering is disabled; persistent installed-user policy remains future work.
- systemd is built with many intentional feature gaps, so a large amount of conventional distro functionality is still absent.
- Brush history/config persistence is not yet provisioned in the live image.

## 12. Missing Distro Functionality

### Needed soon

- broader filesystem administration coverage beyond the now source-built mount closure
- persistent account and home-directory policy for a future installed system
- basic mount and disk utility coverage
- persistent shell history/config plumbing

### Needed before persistent installation

- persistent filesystem support and installer flow
- persistent installed-system package and upgrade policy
- persistent networking policy beyond the ephemeral wired/QEMU DHCP baseline
- glibc built from source
- firmware packaging strategy

### Needed before physical hardware

- broader hardware drivers and firmware coverage
- more complete udev/device-management behavior
- Wi-Fi and Ethernet configuration path
- real console/keymap/locale setup
- storage discovery and recovery tooling

### Needed much later

- SSH
- user-facing package repositories
- richer desktop/session support
- policy tooling for upgrades and rollback

## 13. Recommended Milestone Order

1. Keep the current systemd/dbus-broker/logind/per-user-manager/getty/Brush boot path stable and tested.
2. Keep the completed PAM, `pam_systemd`, Shadow, sudo-rs, `su`, and non-root live-login stack stable and tested.
3. Add persistent account and home/rootfs handling when an installed-system milestone begins.
4. Build persistent installation and package management.
5. Preserve the completed wired/QEMU networking, DNS, certificates, and time-sync baseline while later adding installed-system policy.
6. Move more runtime libraries and core utilities from host copies to owned source or a sysroot.
7. Expand hardware support for physical machines.

## 14. Do Not Reinvent

MattOS should integrate upstream projects rather than rewriting these:

- systemd
- util-linux
- shadow
- PAM
- procps-ng
- findutils
- grep
- sed
- gawk
- coreutils
- glibc
- ca-certificates
- tzdata
- iproute2
- e2fsprogs
- dosfstools
- parted or a comparable partitioning stack

## 15. Prioritized Next Actions

1. Add an automated boot smoke test that checks prompt behavior and a few core commands.
2. Reduce host-library dependency by planning a real sysroot/runtime closure.
3. Split `mattos-build` into smaller modules when the next feature round starts.
4. Preserve the validated authentication stack when future persistent-install work begins.
5. Expand in-guest validation coverage for getty/session checks without relying on serial prompt parsing.

## 16. Validation Summary

Verified successfully during this audit pass:

- `cargo check`
- `cargo test -p mattos-build`
- `python3 DevUtils/setup.py --check`
- `cargo run -p mattos-build -- doctor`
- `cargo run -p mattos-build -- upstream status`
- `cargo run -p mattos-build -- build all`
- `cargo run -p mattos-build -- image`
- the pre-glibc baseline built all 52 `.deb` files and installed them in computed dependency order through real dpkg semantics
- GNU tar 1.35, ACL 2.3.2, zlib 1.3.2, and bzip2 1.0.8 were imported at exact commits, built outside their source trees, and split into four new runtime packages
- the assembled rootfs used package-owned GNU tar to create, list, and extract an archive and used source-built `dpkg-deb` to extract `mattos-brush`
- the bootstrap boundary shrank from 21 entries / 17,365,960 bytes to 17 entries / 16,669,816 bytes
- LZ4 1.10.0, XZ Utils 5.8.1, xxHash 0.8.3, and Zstandard 1.5.7 were imported at exact stable tags and built outside their source trees
- dpkg and APT were rebuilt with explicit MattOS compression include/link/runtime paths; LZ4, liblzma, and xxHash moved into three ABI packages
- the current bootstrap boundary shrank from 17 entries / 16,669,816 bytes to 14 entries / 16,191,736 bytes
- OpenSSL 3.5.7 and elfutils 0.195 were imported at exact release tags and commits as ordinary editable files, with no nested Git repositories
- Zstandard, OpenSSL libcrypto/libssl, and elfutils libelf were built in dependency order and split into `mattos-libzstd1`, `mattos-libcrypto3`, `mattos-libssl3`, and `mattos-libelf1`
- curl, APT, dpkg, and iproute2 were rebuilt against explicit staged include/link/runtime paths; assembled-rootfs loader checks resolved every migrated SONAME inside the rootfs
- the bootstrap boundary shrank from 12 entries / 16,042,648 bytes to 8 entries / 7,639,680 bytes
- libmd 1.2.0 and libbsd 0.12.2 were imported from their canonical upstream repositories at exact stable commits and built out of source with Autotools
- libbsd was forced to the MattOS libmd headers and library; eight dpkg-family commands and ten Shadow commands were rebuilt with explicit staged paths and validated against the assembled rootfs loader
- `mattos-libmd0` and `mattos-libbsd0` uniquely own their SONAMEs, and the bootstrap boundary shrank from 14 entries / 16,191,736 bytes to 12 entries / 16,042,648 bytes
- PCRE2 10.47, SELinux userspace 3.10, and libxcrypt 4.4.38 were imported at exact release commits, built outside their source trees, and split into three ABI packages
- dpkg, iproute2, PAM, and Shadow direct consumers were rebuilt with staged paths; the exact rootfs graph contains no unowned direct consumer of the migrated SONAMEs
- util-linux libblkid/libmount/libsmartcols and mount/umount were source-built and split into four packages, eliminating the former host mount/library copy path while retaining the SELinux compatibility loader
- libxcrypt's upstream tests passed yescrypt coverage and the installed ABI exports all required `GLIBC_2.2.5` and `XCRYPT_2.0`/`4.3`/`4.4` nodes
- the bootstrap boundary shrank from 8 entries / 7,639,680 bytes to 5 entries / 6,518,032 bytes
- two consecutive pre-glibc package/repository builds produced identical aggregate hashes: 52 packages `3aefddd9419391a04c7366337afc58a8025b0729c7caf5ba6fdb19dbd5c07882`; 55 repository files `8ba00fae4273017332afffb903a8b61cafe9387cecce02ff1083894ef8c56b82`
- normal serial QEMU boot reached a `running` systemd system with the live `mattos` session, passwordless live-profile sudo, both system and user D-Bus, routable `ens3`, DHCP/DNS/NTP, and certificate-verified HTTPS
- the embedded repository updated successfully and safely reinstalled `mattos-mount` and the libselinux-dependent `mattos-iproute2` during normal boot
- temporary interactive authentication checks covered incorrect and successful manual login, yescrypt password creation, self-service password change, `su`, prompted administrative sudo rejection/success, non-administrator sudo denial, and locked-root rejection
- no-network QEMU boot retained login, sudo, both D-Bus scopes, a `running` systemd state, and loopback-only network state; local APT updated and reinstalled `mattos-brush`, `mattos-mount`, and `mattos-iproute2`
- representative ownership queries resolved PAM, ncurses, kmod, procps, dbus-broker, sudo, passwd, login, ip, and ping paths to their dedicated packages
- the embedded `file:` repository safely reinstalled `mattos-iputils`, `mattos-procps`, and `mattos-ncurses-bin`
- ten session-critical packages were separately extracted and inspected without replacing active PAM/login/sudo/D-Bus/systemd files
- `python3 DevUtils/run_qemu.py` launched the current graphical image boot path
- tty1 live-user identity, systemd PID 1, sudo, Brush interaction, and getty session restart were directly observed
- `/run/dbus/system_bus_socket` existed and `dbus-broker.service`/`dbus.service` were active with one launcher and one broker process
- non-root `busctl`, `systemctl status`, `networkctl`, `resolvectl status`, `timedatectl`, and `loginctl` connected successfully
- `busctl status` resolved `systemd1`, `network1`, `resolve1`, `timesync1`, `timedate1`, and `login1`; logind and timedated were active
- `loginctl` reported both tty1/seat0 and ttyS0 sessions, `/run/user/1000` was mode `0700` and owned by `mattos`, and `user@1000.service` was active
- `systemctl --user` reported a running user manager with no failed units, and socket activation made both user `dbus.socket` and `dbus.service` active
- `busctl --user` connected to `/run/user/1000/bus` and enumerated the user bus independently of the system bus
- a harmless non-root service restart reached PID 1 through D-Bus and was denied with `Access denied`, confirming the no-Polkit policy boundary
- the rescue GRUB entry selected `rdinit=/usr/libexec/mattos/rescue-init`; the kernel captured the exact `Run /usr/libexec/mattos/rescue-init as init process` handoff, preserving the independently validated PID-1 root Brush path on tty1
- default QEMU virtio networking produced `ens3`, DHCP `10.0.2.15/24`, a default route via `10.0.2.2`, and DNS via `10.0.2.3`
- gateway and named-host pings completed without packet loss; `getent` resolved through glibc; HTTPS headers and body downloads passed certificate validation
- networkd, resolved, and timesyncd were active; timesyncd contacted `time.cloudflare.com`, logged initial synchronization, and created its synchronization marker
- `--no-network` omitted the NIC/backend and reached a loopback-only live prompt without hanging
- the disconnected boot still had an active system bus and successful non-root `busctl` access

The remaining module-related boot message is precisely scoped: systemd's real libkmod integration probes `autofs4`, while the intentionally monolithic kernel has `CONFIG_MODULES=n` and `CONFIG_AUTOFS_FS=n`. The `configfs` and `fuse` module service attempts finish successfully, and no module helper fails because an executable is absent.

## glibc runtime transition

- GNU glibc 2.43 is imported as ordinary editable source at commit `f762ccf84f122d1354f103a151cba8bde797d521`; no nested Git repository is present.
- Linux UAPI headers are regenerated with `make ARCH=x86 headers_install` from imported Linux revision `f17f39c917cd4aac09db1a6a083ef5ec09b4924d`.
- glibc is built out of source with a 5.10.0 minimum kernel and installed into the controlled `out/sysroot` before any native consumer rebuild.
- every post-glibc native build stage is invalidated and rebuilt with explicit sysroot/include/link/pkg-config settings; the kernel is correctly excluded from the libc consumer set.
- `mattos-libc6` and `mattos-libc-bin` bring the package total to 54. libc is foundational and acyclic; all other packages depend directly on it.
- the runtime package owns 17 glibc runtime/compatibility/NSS DSOs plus the MattOS ELF loader. The utility package owns `getent`, `locale`, `ldd`, and `ldconfig`; development inputs remain build-only.
- the host-derived bootstrap boundary shrinks from 5 files / 6,518,032 bytes to 2 files / 2,832,624 bytes: `libgcc_s.so.1` and `libstdc++.so.6`.
- assembled-rootfs validation requires `/lib64/ld-linux-x86-64.so.2` on every dynamically linked executable, resolves every `DT_NEEDED` entry through that loader inside the image, checks glibc symbol versions, and emits `out/reports/elf-runtime-inventory.tsv`.
- this glibc transition was not a self-hosted toolchain; the subsequent GCC-runtime milestone source-builds the target libgcc/libstdc++ payloads while the compiler, assembler, and linker remain host-bootstrap inputs.
- the final ELF inventory contains 258 objects, including 193 dynamic executables; every dynamic executable requests `/lib64/ld-linux-x86-64.so.2`, and every dependency and required glibc symbol version resolves within the assembled rootfs.
- isolated loader checks passed for Brush, dpkg, APT, curl, systemd, dbus-broker, login, and sudo before the final rootfs transition.
- two clean full builds produced byte-identical glibc installs, all 54 packages, all 57 repository files, the ELF inventory, initramfs, and ISO.
- normal QEMU boot retained systemd, both D-Bus scopes, logind sessions, yescrypt/PAM/Shadow/sudo-rs authentication, DNS, NTP, ping, and certificate-verified HTTPS; MattOS-built APT/dpkg updated the local repository and reinstalled Brush.
- `--no-network` boot retained systemd, login, sudo, both D-Bus scopes, local APT update, and offline Brush reinstall with loopback only.
- the rescue entry passed `rdinit=/usr/libexec/mattos/rescue-init`; the kernel executed that dynamically linked Rust binary as PID 1 using the MattOS loader, with its rescue shell on tty1.

## GCC runtime source closure

- GCC 15.3.0 is imported from `https://gcc.gnu.org/git/gcc.git` as ordinary editable source at exact commit `4db0e8df15bef836558857c291c323add11d035c`; no nested Git repository is present.
- the top-level GCC build uses the MattOS glibc/UAPI sysroot and requests only `all-target-libgcc` and `all-target-libstdc++-v3`; the selected runtime tree excludes compiler drivers, headers, static archives, development links, helper executables, and unrelated runtimes.
- `mattos-libgcc-s1` and `mattos-libstdc++6` bring the package total to 55. The graph is `libc -> libgcc -> libstdc++`, with direct Rust and C++ consumers declaring the matching runtime package.
- ABI validation requires the consumer-compatible GCC, GLIBCXX, and CXXABI version nodes and records every exported node in `out/build/gcc-runtime/runtime-abi.tsv`.
- Rust panic/unwind behavior is preserved; rescue-init retains its direct `libgcc_s.so.1` dependency. A temporary C++ throw/catch program validates exception interoperability through the MattOS loader.
- `mattos-bootstrap-runtime` is removed from the package set and repository. Its generated audit records zero entries and zero host-derived payload bytes.
- final ELF validation compares GCC runtime bytes to the source build, rejects duplicate SONAMEs and host resolution, and records GLIBC, GLIBCXX, CXXABI, and GCC nodes for every ELF object.
- runtime source closure does not make MattOS self-hosting. Host GCC/G++, Binutils, Make, package/image construction tools, and language build toolchains remain future work.
