# MattOS System D-Bus

The system bus was introduced by the 2026-08-01 milestone; this page
describes the current integration.

## Scope and source

MattOS uses `dbus-broker` as its production system message bus. The imported source is ordinary editable source, not a submodule or nested repository.

- Upstream: `https://github.com/bus1/dbus-broker.git`
- Primary branch: `main`
- Imported commit: `2956b5d381deeea709c53d02f10e799e50e44f4b`
- Destination: `src/system/dbus/dbus-broker/`
- State: `upstream/state/dbus-broker.toml`
- Import method: component-scoped copy with the standard verify-and-replace sync workflow ([Upstream synchronization model](../../sources/upstream-sync.md))

A Rust D-Bus broker can be reconsidered later if an implementation reaches the compatibility and maturity required for a system bus. The experimental Rust `busd` is not the default broker.

The reference implementation is also imported: `dbus` 1.16.2
(`https://gitlab.freedesktop.org/dbus/dbus.git`, branch `dbus-1.16.2`, commit
`958bf9db2100553bcd2fe2a854e1ebb42e886054`, destination `src/system/dbus/dbus/`).
It is built with `-Dmessage_bus=true -Dtools=true` (`build_dbus` in
`src/tools/mattos-build/src/stages/system_runtime.rs`) mainly for `libdbus-1`,
which Qt, KDE and other desktop components link against. The `libdbus-1-3`
package also installs `/usr/bin/dbus-daemon`, `dbus-run-session` and
`dbus-update-activation-environment`, but `dbus-daemon` is not the system or
session bus: `dbus.service` is an alias of `dbus-broker.service`.

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

The optional upstream `dbus-broker-session` wrapper is not staged. The same broker binary serves the user scope through minimal MattOS-owned user units documented in [Login Sessions and Per-User Services](sessions.md).

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

The base policy allows clients to connect, receive messages and replies, and use the standard bus interfaces. It denies arbitrary well-known-name ownership and method calls by default. Service-specific policies in `/usr/share/dbus-1/system.d/` then grant the intended interfaces. They are shipped by the packages that own each service, including:

- systemd: `systemd1`, `network1`, `resolve1`, `timesync1`, `timedate1`, `locale1` and `login1`;
- NetworkManager (`org.freedesktop.NetworkManager`, `nm-dispatcher`, `nm-priv-helper`) and wpa_supplicant (`fi.w1.wpa_supplicant1`);
- Polkit (`org.freedesktop.PolicyKit1`), UDisks2, UPower, power-profiles-daemon and BlueZ;
- Flatpak's system helper and the KDE/Plasma helpers (KAuth helpers, Plasma Login Manager's display-manager and settings helpers, PowerDevil helpers, and others).

Services that are started on demand have matching activation files in `/usr/share/dbus-1/system-services/`. Root retains bus administration and service-management access.

Activation aliases in `/usr/lib/systemd/system/`:

- `dbus.service -> dbus-broker.service`
- `dbus-org.freedesktop.network1.service -> systemd-networkd.service` (the
  target unit is masked, because NetworkManager configures interfaces;
  activating `network1` therefore fails)
- `dbus-org.freedesktop.resolve1.service -> systemd-resolved.service`
- `dbus-org.freedesktop.timesync1.service -> systemd-timesyncd.service`
- `dbus-org.freedesktop.timedate1.service -> systemd-timedated.service`
- `dbus-org.freedesktop.login1.service -> systemd-logind.service`
- `dbus-org.freedesktop.locale1.service -> systemd-localed.service`

`wpa_supplicant.service` carries its own `dbus-fi.w1.wpa_supplicant1.service`
alias.

PID 1 owns `org.freedesktop.systemd1` directly; it does not need a synthetic unit alias.

## logind and authorization

D-Bus removed the prior runtime blocker for `systemd-logind`. The service and its Varlink socket are enabled, and QEMU validation confirmed `systemd-logind.service` active with `org.freedesktop.login1` owned by that process. The later session milestone added the built `pam_systemd` module, so `loginctl` reports real sessions (tty1/seat0, ttyS0, graphical and SSH logins) with their actual leaders.

Polkit (`polkit.service`, `org.freedesktop.PolicyKit1`) is installed, with
`polkit-kde-agent-1` as the interactive agent in Plasma sessions; see
[Authentication](authentication.md). Non-root status and inspection calls work
directly; administrative operations such as restarting a system service are
authorized by Polkit policy (administrators are members of the `sudo` group)
rather than by relaxing bus policy.

## Runtime closure

The broker needs `libc.so.6`; the launcher needs `libexpat.so.1`, `libsystemd.so.0`, and `libc.so.6`. Both use `/lib64/ld-linux-x86-64.so.2`. glibc, the loader, Expat and `libsystemd` are all built from source against the MattOS sysroot and shipped as MattOS packages; no host libraries are copied. Rootfs validation resolves every executable's libraries with the MattOS loader inside the image and rejects code built by a non-MattOS compiler.

## Per-user bus

The per-user bus is a separate broker scope. Its socket is `/run/user/$UID/bus`, its configuration is `/usr/share/dbus-1/session.conf`, and `busctl --user` connects through `DBUS_SESSION_BUS_ADDRESS`. It neither replaces nor relaxes the system bus.

## Known limits

- No MattOS firewall policy is installed.
- The `network1` activation alias points at the masked `systemd-networkd.service`.

## History (2026-08-01 milestone)

At the time of the milestone the image had no Polkit, networkd still configured
interfaces, and glibc and Expat were copied from the build host. In graphical
QEMU, the live `mattos` user successfully used `busctl`, `systemctl status`, `networkctl`, `resolvectl status`, `timedatectl`, and `loginctl` without sudo merely to connect. `busctl status` resolved `systemd1`, `network1`, `resolve1`, `timesync1`, `timedate1`, and `login1`. Exactly one launcher and one broker process owned the single system bus. As `mattos`, restarting `systemd-timesyncd.service` reached the bus and was refused with `Access denied` (exit status 4).

The same image retained DHCP, DNS, NTP synchronization, ping, and certificate-verified HTTPS. A `--no-network` boot still started the system bus, registered tty sessions, started the user manager and user bus, and reached the live prompt with loopback only. The rescue GRUB entry continued to run `rescue-init` as PID 1 and intentionally had neither a system nor user bus socket.
