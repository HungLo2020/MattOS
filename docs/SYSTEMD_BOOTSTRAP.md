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

The current integrated configuration is intentionally minimal and disables optional subsystems not required for this milestone:

- Disabled stacks: `networkd`, `resolved`, `timesyncd`, `homed`, `portabled`, `nspawn`, `oomd`, `remote`, `userdb`, `firstboot`, `bootloader`, `importd`, `vmspawn`, `coredump`, `pstore`, `machined`, `hostnamed`, `localed`, `timedated`, `nsresourced`
- Disabled security/optional integrations: `pam`, `seccomp`, `acl`, `audit`, `blkid`, `kmod`, `libcryptsetup`, `openssl`, `gnutls`, `libfido2`, `tpm2`, `qrencode`, `bpf-framework`
- Disabled extras: docs, man pages, html, translations, tests, kernel-install extras, analyze utility
- Journal default: volatile (`journal-storage-default=volatile`)

The full option list is defined by `systemd_meson_options()` in `src/tools/mattos-build/src/main.rs`.

## Rootfs and Boot Flow

Normal boot flow:

```text
GRUB -> Linux -> /usr/lib/systemd/systemd (PID 1) -> mattos.target -> mattos-shell.service -> Brush on tty1
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

This milestone intentionally does not provide:

- login authentication
- PAM-enabled login/session stack
- non-root user sessions
- persistent journal
- networking stack integration
- installed-disk boot/install support
- package management
