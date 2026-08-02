# Systemd Boot Milestone

Date: 2026-07-31

## Upstream Source

- Upstream repository: https://github.com/systemd/systemd.git
- Branch: `main`
- Imported commit: `91d2131e20ca304ee1d9dabf71b351d6b4cfcddc`
- Imported source location: `src/system/systemd/`
- Sync metadata file: `upstream/state/systemd.toml`

## Upstream Sync Commands

```bash
cargo run -p mattos-build -- upstream status
cargo run -p mattos-build -- upstream import systemd
cargo run -p mattos-build -- upstream sync systemd
cargo run -p mattos-build -- upstream sync --all
```

Sync uses the existing MattOS three-way merge workflow that preserves local edits and emits conflict markers when local and upstream touch the same lines.

## Build Integration

Systemd is integrated as a first-class build stage:

```bash
cargo run -p mattos-build -- build systemd
cargo run -p mattos-build -- build all
```

Build outputs:

- Meson/Ninja build directory: `out/build/systemd/build/`
- Meson option stamp: `out/build/systemd/meson-options.txt`
- Install staging root: `out/build/systemd/install/`

The build directory is kept for incremental Ninja rebuilds. Reconfigure is triggered only when the tracked Meson option set changes.

## Minimal Meson Configuration

The current integrated configuration is intentionally minimal. It enables the base services required by the wired/QEMU and system D-Bus milestones:

- Enabled services: `systemd-networkd`, `systemd-resolved`, `systemd-timesyncd`, `systemd-timedated`, `systemd-logind`
- Fixed ephemeral service IDs: `systemd-network` 192, `systemd-resolve` 193, `systemd-timesync` 194
- Disabled stacks: `homed`, `portabled`, `nspawn`, `oomd`, `remote`, `userdb`, `firstboot`, `bootloader`, `importd`, `vmspawn`, `coredump`, `pstore`, `machined`, `hostnamed`, `localed`, `nsresourced`
- Enabled base-system integration: locally built kmod 34 from `out/build/kmod/install`
- Enabled login integration: systemd's PAM support and locally built `pam_systemd.so`
- Disabled security/optional integrations: `seccomp`, `acl`, `audit`, `blkid`, `libcryptsetup`, `openssl`, `gnutls`, `libfido2`, `tpm2`, `qrencode`, `bpf-framework`
- Disabled extras: docs, man pages, html, translations, tests, kernel-install extras, analyze utility
- Journal default: volatile (`journal-storage-default=volatile`)

The full option list is defined by `systemd_meson_options()` in `src/tools/mattos-build/src/main.rs`.

## Rootfs and Boot Flow

Normal boot flow:

```text
GRUB -> Linux -> systemd (PID 1) -> getty -> login/PAM -> pam_systemd -> logind -> systemd --user -> Brush
```

Rescue flow:

```text
GRUB rescue entry -> Linux -> /usr/libexec/mattos/rescue-init (Rust init fallback)
```

MattOS-owned units are stored in:

- `src/system/units/mattos.target`
- `src/system/units/mattos-shell.service`

Units are installed into:

- `/usr/lib/systemd/system/`

Rootfs now sets a merged `/usr` style layout with symlinks:

- `/bin -> usr/bin`
- `/sbin -> usr/sbin`
- `/lib -> usr/lib`
- `/lib64 -> usr/lib64`

Minimum runtime paths/files created for this milestone include:

- `/etc/systemd/system/`
- `/usr/lib/systemd/system/`
- `/run/`
- `/var/`
- `/var/log/`
- `/var/tmp/`
- `/etc/machine-id` (empty for ephemeral live image initialization)
- `/etc/systemd/network/20-mattos-wired.network` (Ethernet IPv4 DHCP)
- `/etc/dbus-1/system.conf` and `/etc/dbus-1/system.d/`
- `/usr/share/dbus-1/system.d/` and `/usr/share/dbus-1/system-services/`
- `/run/dbus/system_bus_socket` (created by the system `dbus.socket` at runtime)
- `/run/user/$UID` and `/run/user/$UID/bus` (created only at runtime by systemd/logind and the user socket)
- `/etc/systemd/resolved.conf`, `/etc/systemd/timesyncd.conf`
- `/etc/nsswitch.conf`, `/etc/hosts`, `/etc/networks`
- `/etc/resolv.conf -> /run/systemd/resolve/stub-resolv.conf`
- `/etc/ssl/certs/ca-certificates.crt` (pinned Mozilla-derived bundle)

## Runtime Library Closure

Systemd runtime binaries are staged via Meson install and host dynamic library dependencies are copied into the rootfs using `ldd`-based scanning in the build orchestrator.

This is a bootstrap limitation: runtime libraries currently come from the build host and are copied into the image. They are not yet built from a dedicated MattOS sysroot.

Useful inspection commands:

```bash
ldd out/build/rootfs/usr/lib/systemd/systemd
readelf -d out/build/rootfs/usr/lib/systemd/systemd
```

## Setup Dependencies

`DevUtils/setup.py` (Debian/Ubuntu family) installs missing packages required by the current MattOS + minimal systemd build workflow, including:

- `meson`, `ninja-build`, `gperf`, `python3-jinja2`, `libmount-dev`

alongside existing kernel/ISO/QEMU prerequisites.

`cargo run -p mattos-build -- doctor` now checks:

- required tools (`meson`, `ninja`, `gperf`, etc.)
- Python Jinja import (`python3 -c "import jinja2"`)
- pkg-config module for mount (`pkg-config --exists mount`)

## Known Limitations

The current image intentionally does not provide:

- persistent installed-system users or sessions
- persistent journal
- Wi-Fi, SSH, a firewall policy, or physical Ethernet driver expansion beyond the QEMU virtio NIC
- Polkit or another interactive D-Bus authorization agent; privileged operations require root or sudo
- installed-disk boot/install support
- package management

The production system bus is the separately built dbus-broker described in `docs/DBUS.md`. `systemd-logind` owns `org.freedesktop.login1`; PAM-registered sessions start UID-generic per-user managers and a separate socket-activated user broker as described in `docs/SESSIONS.md`.
