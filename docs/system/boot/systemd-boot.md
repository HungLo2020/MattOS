# Systemd Boot Milestone

The systemd import began as the 2026-07-31 milestone; this page describes
the current integration.

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

Sync verifies the vendored tree against its last import and replaces it with the new pinned commit; it refuses a tree with local edits (MattOS changes belong in `upstream/patches/`). See [Upstream synchronization model](../../sources/upstream-sync.md).

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

## Meson Configuration

The integrated configuration enables the services MattOS uses and disables
the rest:

- Enabled services and tools: `systemd-networkd` (built but masked in images;
  see [MattOS Wired/QEMU Networking](../networking.md)), `systemd-resolved`,
  `systemd-timesyncd`, `systemd-timedated`, `systemd-localed`,
  `systemd-logind`, and `systemd-nspawn` (used by mattos-compat)
- Fixed service IDs: `systemd-network` 192, `systemd-resolve` 193,
  `systemd-timesync` 194
- Disabled stacks: `homed`, `portabled`, `oomd`, `remote`, `userdb`,
  `firstboot`, `bootloader`, `repart`, `sysupdate`, `importd`, `vmspawn`,
  `coredump`, `pstore`, `machined`, `hostnamed`, `nsresourced`
- Enabled base-system integration: MattOS-built kmod 34, util-linux
  `libmount`, and `blkid` (so udev creates the `/dev/disk/by-uuid` and
  `by-partuuid` links used by installed `fstab` entries)
- Enabled login integration: systemd's PAM support, `libcrypt`, and the
  MattOS-built `pam_systemd.so`
- Enabled: `selinux` library support (compatibility only; MattOS ships no SELinux policy)
- Disabled security/optional integrations: `seccomp`, `acl`, `audit`,
  `libcryptsetup`, `openssl`, `gnutls`, `libfido2`, `tpm2`, `qrencode`,
  `bpf-framework`
- Disabled extras: man pages, HTML, translations, tests, kernel-install,
  `systemd-analyze`
- Journal default: volatile (`journal-storage-default=volatile`)

The full option list is defined by `systemd_meson_options()` in
`src/tools/mattos-build/src/stages/system_runtime.rs`.

## Rootfs and Boot Flow

GRUB and the early `/init` are described in
[Live and Installed Root Architecture](live-root.md). Once systemd is PID 1,
the flows are:

```text
Live (default entry):  systemd -> mattos-live-graphical.target -> graphical.target
                       -> display-manager.service (= plasma-greeter.service, greetd
                          with /etc/greetd/plasma-live.toml) -> KDE Plasma (Wayland)
                          as the live user `mattos`
Installed Plasma:      systemd -> graphical.target -> plasmalogin.service
                       -> PAM -> pam_systemd -> logind -> KDE Plasma (Wayland)
Live CLI / Installed CLI:
                       systemd -> mattos.target or multi-user.target -> getty
                       -> login/PAM -> pam_systemd -> logind -> systemd --user -> Brush
Install (CLI):         systemd -> mattos-install-cli.target -> mattos-install-cli.service
                       (`mattos-install guided` on tty1)
```

`plasma-greeter.service` and greetd ship only in the live-only
`mattos-plasma-live` package; the packaged unit runs
`greetd --config /etc/greetd/plasma-live.toml`, the configuration shipped
beside it, and the live drop-in restates that explicitly. Installed systems use
Plasma Login Manager (`plasmalogin.service`) instead. The unit has
`Conflicts=getty@tty1.service`, so the tty1 getty only runs in the CLI modes. On the live image, the tty1 and ttyS0
gettys autologin the `mattos` user.

Rescue flow:

```text
GRUB "MattOS Rescue" (mattos.rescue=1) -> Linux -> early /init (mounts the live root)
  -> /usr/libexec/mattos/rescue-init (Rust fallback init, not systemd)
```

MattOS-owned units are stored in `src/system/units/`:

- `mattos.target` (live CLI target; requires `multi-user.target`, wants
  `getty@tty1.service`)
- `mattos-live-graphical.target` (requires `graphical.target`)
- `mattos-install-cli.target` and `mattos-install-cli.service`
- `mattos-smoke.service` (boot smoke-validation oneshot)
- `mattos-shell.service` (legacy root Brush shell on tty1; masked to
  `/dev/null` in the image)
- `getty@tty1.service.d/` and `serial-getty@ttyS0.service.d/` drop-in
  directories

The live autologin drop-ins themselves come from the live profile under
`src/system/profiles/live/etc/systemd/system/`; the installer removes them
from installed systems.

Units are installed into `/usr/lib/systemd/system/`. The live image's
`default.target` is `mattos.target`; the early `/init` overrides it with
`--unit=` on the systemd command line, chosen from `mattos.mode`. The installer points an installed
system's `default.target` at `graphical.target` (Plasma profile) or
`multi-user.target` (CLI profile).

The rootfs uses a merged `/usr` layout with symlinks:

- `/bin -> usr/bin`
- `/sbin -> usr/sbin`
- `/lib -> usr/lib`
- `/lib64 -> usr/lib64`

Runtime paths and files include:

- `/etc/systemd/system/`
- `/usr/lib/systemd/system/`
- `/run/`
- `/var/`
- `/var/log/`
- `/var/tmp/`
- `/etc/machine-id` (empty in the live image, initialized at boot)
- `/etc/dbus-1/system.conf` and `/etc/dbus-1/system.d/`
- `/usr/share/dbus-1/system.d/` and `/usr/share/dbus-1/system-services/`
- `/run/dbus/system_bus_socket` (created by the system `dbus.socket` at runtime)
- `/run/user/$UID` and `/run/user/$UID/bus` (created only at runtime by systemd/logind and the user socket)
- `/etc/systemd/resolved.conf.d/10-mattos.conf`,
  `/etc/systemd/timesyncd.conf.d/10-mattos.conf`
- `/etc/nsswitch.conf`, `/etc/hosts`, `/etc/networks`
- `/etc/resolv.conf -> /run/systemd/resolve/stub-resolv.conf`
- `/etc/ssl/certs/ca-certificates.crt` (pinned Mozilla-derived bundle)
- `/etc/systemd/system/systemd-networkd.service -> /dev/null` (networkd is
  masked; NetworkManager configures interfaces)

## Runtime Library Closure

systemd and its runtime libraries are built from source against the MattOS
sysroot with the MattOS toolchain; no host libraries are copied into the
image. The rootfs audit in `src/tools/mattos-build/src/stages/image.rs`
rejects any ELF file compiled by a non-MattOS compiler, requires every
executable to use the MattOS dynamic loader, and resolves each one's
libraries with that loader against the target library directories only.

Useful inspection commands:

```bash
readelf -d out/build/rootfs/usr/lib/systemd/systemd
readelf -l out/build/rootfs/usr/lib/systemd/systemd | grep interpreter
```

## Setup Dependencies

`DevUtils/setup.py` (Debian/Ubuntu family) installs the host build tools used
by the systemd stage, including `meson`, `ninja-build`, `gperf` and
`python3-jinja2`, alongside the kernel/ISO/QEMU prerequisites.
`cargo run -p mattos-build -- doctor` reports missing host tools and the
packages that provide them.

## Known Limitations

- The journal is volatile (`journal-storage-default=volatile`), including on
  installed systems; no `/var/log/journal` is created.
- No MattOS firewall policy is installed.
- `systemd-hostnamed`, `machined`, `homed` and the other disabled stacks
  above are unavailable.

The production system bus is the separately built dbus-broker described in [MattOS System D-Bus](../services/dbus.md). `systemd-logind` owns `org.freedesktop.login1`; PAM-registered sessions start UID-generic per-user managers and a separate socket-activated user broker as described in [Login Sessions and Per-User Services](../services/sessions.md).
