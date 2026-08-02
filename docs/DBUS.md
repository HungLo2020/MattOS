# MattOS System D-Bus

Date: 2026-08-01

## Scope and source

MattOS uses `dbus-broker` as its production system message bus. The imported source is ordinary editable source, not a submodule or nested repository.

- Upstream: `https://github.com/bus1/dbus-broker.git`
- Primary branch: `main`
- Imported commit: `2956b5d381deeea709c53d02f10e799e50e44f4b`
- Destination: `src/system/dbus/dbus-broker/`
- State: `upstream/state/dbus-broker.toml`
- Import method: component-scoped copy with the existing three-way sync workflow

A Rust D-Bus broker can be reconsidered later if an implementation reaches the compatibility and maturity required for a system bus. The experimental Rust `busd` is not the default broker. MattOS does not import or ship `dbus-daemon`.

## Build

The targeted stage is:

```sh
cargo run -p mattos-build -- build dbus-broker
```

`build all` runs it after systemd and before image assembly. The stage copies source to `out/build/dbus-broker/source/`, builds in `out/build/dbus-broker/build/`, and installs into `out/build/dbus-broker/install/`. Imported source is never built in place.

The upstream Meson build uses a release `/usr` layout, enables the launcher, and disables upstream tests, documentation, audit, AppArmor, SELinux, reference tests, and unstable APIs. It compiles against the MattOS-built `libsystemd`; Expat is the launcher's XML parser. The stage stamp records source state, options, and build environment so unchanged builds remain incremental.

Installed runtime programs are:

- `/usr/bin/dbus-broker`
- `/usr/bin/dbus-broker-launch`
- `/usr/bin/busctl` from the existing systemd build

The optional upstream `dbus-broker-session` wrapper is not staged. The same broker binary serves the user scope through minimal MattOS-owned user units documented in `SESSIONS.md`.

## Runtime architecture

```text
systemd PID 1
  -> dbus.socket at /run/dbus/system_bus_socket
  -> dbus-broker.service
  -> dbus-broker-launch --scope system --config-file=/etc/dbus-1/system.conf
  -> one dbus-broker process running as messagebus
```

`dbus.socket` is enabled through `sockets.target.wants`. `dbus.service` aliases `dbus-broker.service`; no competing bus daemon is installed. The socket is created at runtime with mode `0666` so clients can connect, while message routing and name ownership remain controlled by D-Bus policy. `RemoveOnStop=yes` prevents an obsolete socket node from surviving a stopped socket unit, and image validation rejects any staged `/run/dbus/system_bus_socket`.

The dedicated `messagebus` account is created by sysusers with fixed UID/GID 195. Existing network service IDs remain 192 through 194.

## Configuration and policy

MattOS owns the launcher configuration and units under `src/system/dbus/config/` and `src/system/dbus/units/`. The image installs:

- `/etc/dbus-1/system.conf`
- `/etc/dbus-1/system.d/`
- `/usr/share/dbus-1/system.d/`
- `/usr/share/dbus-1/system-services/`
- `/usr/lib/systemd/system/dbus.socket`
- `/usr/lib/systemd/system/dbus-broker.service`

The base policy allows clients to connect, receive messages and replies, and use the standard bus interfaces. It denies arbitrary well-known-name ownership and method calls by default. Service-specific systemd policies then grant the intended interfaces for `systemd1`, `network1`, `resolve1`, `timesync1`, `timedate1`, and `login1`. Root retains bus administration and service-management access.

Activation aliases are installed only when their target units exist:

- `dbus.service -> dbus-broker.service`
- `dbus-org.freedesktop.network1.service -> systemd-networkd.service`
- `dbus-org.freedesktop.resolve1.service -> systemd-resolved.service`
- `dbus-org.freedesktop.timesync1.service -> systemd-timesyncd.service`
- `dbus-org.freedesktop.timedate1.service -> systemd-timedated.service`
- `dbus-org.freedesktop.login1.service -> systemd-logind.service`

PID 1 owns `org.freedesktop.systemd1` directly; it does not need a synthetic unit alias.

## logind and authorization

D-Bus removed the prior runtime blocker for `systemd-logind`. The service and its Varlink socket are enabled, and QEMU validation confirmed `systemd-logind.service` active with `org.freedesktop.login1` owned by that process. The later session milestone added the built `pam_systemd` module, so `loginctl` now reports real tty1/seat0 and ttyS0 sessions with their actual leaders.

MattOS still does not include Polkit. Non-root status and inspection calls work, but administrative operations remain restricted. Validation as `mattos` reached the bus and received `Access denied` (exit status 4) when attempting to restart `systemd-timesyncd.service`; the service was not accidentally made world-writable.

## Runtime closure

The ELF closure is inspected with `file`, `readelf`, and `ldd` during rootfs assembly. The broker needs `libc.so.6`; the launcher needs `libexpat.so.1`, MattOS's built `libsystemd.so.0`, and `libc.so.6`. Both use `/lib64/ld-linux-x86-64.so.2`. The required files are staged and rootfs validation resolves every `DT_NEEDED` entry inside the image.

As with other current bootstrap components, glibc, Expat, and the loader are copied from the build host. `libsystemd` comes from the MattOS systemd build. A fully MattOS-built sysroot remains future work.

## Validated behavior

In graphical QEMU, the live `mattos` user successfully used `busctl`, `systemctl status`, `networkctl`, `resolvectl status`, `timedatectl`, and `loginctl` without sudo merely to connect. `busctl status` resolved `systemd1`, `network1`, `resolve1`, `timesync1`, `timedate1`, and `login1`. Exactly one launcher and one broker process owned the single system bus.

The per-user bus is a separate broker scope. Its socket is `/run/user/$UID/bus`, its configuration is `/usr/share/dbus-1/session.conf`, and `busctl --user` connects through `DBUS_SESSION_BUS_ADDRESS`. It neither replaces nor relaxes the system bus.

The same image retained DHCP, DNS, NTP synchronization, ping, and certificate-verified HTTPS. A `--no-network` boot still started the system bus, registered tty sessions, started the user manager and user bus, and reached the live prompt with loopback only. The rescue GRUB entry continued to run `rescue-init` as PID 1 and intentionally had neither a system nor user bus socket.

## Known limits

- No Polkit; privileged D-Bus operations require root or sudo.
- No persistent installation or state.
- No SSH, Wi-Fi, NetworkManager, firewall, package manager, installer, or desktop environment.
- Runtime closure is not yet produced from a complete MattOS sysroot.
