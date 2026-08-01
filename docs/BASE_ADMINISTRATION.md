# Base-System Administration Milestone

Date: 2026-08-01

MattOS imports kmod, procps-ng, and ncurses as editable source trees through the same copy-based upstream synchronization workflow as the existing kernel and userland components. They are ordinary repository files, not submodules.

## Upstream provenance

| Component | Repository | Branch | Imported commit | Destination |
| --- | --- | --- | --- | --- |
| kmod | `https://github.com/kmod-project/kmod.git` | `master` | `5086df53090b2fe9fa1c31351c05a78a12a4ba71` | `src/system/kmod/` |
| procps-ng | `https://gitlab.com/procps-ng/procps.git` | `master` | `619562d36cbd48fb6958043577558cbc32a6ba79` | `src/userland/procps-ng/` |
| ncurses snapshots maintained by Thomas E. Dickey | `https://github.com/ThomasDickey/ncurses-snapshots.git` | `master` | `c7556ecbc951326acab37c9cf1e7d690456959e0` | `src/system/terminal/ncurses/` |

Exact import timestamps and sync methods are recorded in `upstream/state/{kmod,procps-ng,ncurses}.toml`.

## Build configuration

- kmod uses Meson/Ninja in `out/build/kmod/build`. Tools and shared libkmod are enabled. Tests, manuals, documentation, module compression integrations, and signature-library integrations are disabled. Its deterministic install tree is `out/build/kmod/install`.
- ncurses uses its Autoconf/Make build in `out/build/ncurses/build`, with shared wide-character ncurses and a separate terminfo library. Static, debug, C++, Ada, tests, manuals, and stripping are disabled. Its install tree is `out/build/ncurses/install`.
- procps-ng uses Autoconf/Automake/Make in `out/build/procps-ng/build`, linked against the MattOS-built ncurses install tree. NLS, systemd/elogind, NUMA, `kill`, `pidwait`, examples, and static libraries are disabled. Its install tree is `out/build/procps-ng/install`.
- systemd now enables its kmod integration and resolves kmod version 34 from `out/build/kmod/install`; networking and the other previously excluded systemd subsystems remain disabled.

Configuration stamps preserve incremental object builds and trigger reconfiguration when options or dependency paths change. Install staging directories are recreated on each install pass.

Targeted stages are:

```text
cargo run -p mattos-build -- build kmod
cargo run -p mattos-build -- build ncurses
cargo run -p mattos-build -- build procps
```

`build all` orders ncurses before procps and kmod before systemd.

## Installed runtime

- kmod: `kmod`, `modprobe`, `insmod`, `rmmod`, `lsmod`, `modinfo`, `depmod`, plus `libkmod.so.2` and the configuration directories under `/etc` and `/usr/lib`.
- procps-ng: `ps`, `top`, `free`, `uptime`, `pgrep`, `pkill`, `pidof`, `watch`, `sysctl`, `vmstat`, `w`, `pmap`, `pwdx`, `tload`, `slabtop`, and `hugetop`, plus `libproc2.so.1` and `/etc/sysctl.conf`.
- ncurses: `clear`, `tput`, `tic`, `toe`, and `infocmp`, plus the required ncurses/terminfo shared libraries.

The rootfs assembler owns centralized component install manifests. Every manifest executable is inspected with `file -L`, `readelf -d`, and `ldd`; unresolved libraries fail assembly. Dependencies resolving inside component install trees are mapped back to their merged-`/usr` runtime paths rather than copied under their host build paths.

uutils also implements `uptime`; that applet is intentionally not linked into the image so procps-ng is the only installed provider.

## Terminal database

The image carries compiled entries from the real ncurses database for:

```text
linux
xterm
xterm-256color
screen
screen-256color
vt100
```

The live login environment defaults to `TERM=linux`. `clear` and `tput` use this database; no hard-coded clear escape sequence is used.

## Kernel module status

The MattOS kernel configuration currently contains `# CONFIG_MODULES is not set`. It is intentionally monolithic: there is no `/lib/modules/<release>` tree, no `modules.dep`, and no fabricated module metadata. The real `lsmod` therefore reports that `/proc/modules` does not exist, and module insertion is unavailable until `CONFIG_MODULES` is enabled and matching modules are built.

The userspace tooling and libkmod are present for future hardware work. systemd's module helpers can now execute the real `modprobe` command; the prior executable-not-found warning is not suppressed or masked. The remaining early-boot `Failed to find module 'autofs4'` message is the expected consequence of systemd probing for a module while MattOS has both `CONFIG_MODULES` and `CONFIG_AUTOFS_FS` disabled. The generated module services for `configfs` and `fuse` execute and finish successfully; there are no `modprobe` executable failures.

## In-guest validation

The current ISO was launched with the graphical QEMU path and validated directly on tty1:

- the live session logged in as `mattos`, `/proc/1/comm` returned `systemd`, and `sudo id` returned UID/GID 0;
- all kmod, procps-ng, and ncurses commands listed above resolved through the live user's merged-`/usr` PATH;
- `ps`, `ps aux`, and `free` reported live process and memory state, `uptime` reported the running system, `pgrep systemd` found PID 1 and systemd helpers, and `sysctl kernel.hostname` returned `MattOS`;
- `modprobe --version` reported kmod 34 and `lsmod` produced the documented monolithic-kernel result;
- `tput colors` returned `8`, `infocmp linux` read the compiled database, and `clear` visibly cleared tty1;
- `top` rendered its full-screen process display and exited normally with `q`;
- Brush completion expanded `tpu` to `tput`, autosuggestions remained visible, cursor editing produced the intended command text, and exiting Brush caused getty to create a fresh live session;
- the alternate GRUB entry booted `/usr/libexec/mattos/rescue-init`; `/proc/1/comm` returned `rescue-init` and the rescue root prompt remained usable.

## Scope and limitations

This milestone does not add networking, SSH, package management, an installer, persistent disks, firmware, a desktop, or changes to PAM, accounts, the live user, or sudo policy. The rootfs still relies on host-built glibc and selected pre-existing bootstrap libraries; the three new upstream libraries are staged from their MattOS component builds.
