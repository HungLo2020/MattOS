# Login Sessions and Per-User Services

Date: 2026-08-01

MattOS normal console logins use the standard systemd session lifecycle:

```text
agetty -> login -> PAM -> pam_systemd -> systemd-logind
       -> /run/user/$UID -> user@$UID.service -> systemd --user
       -> dbus.socket -> per-user dbus-broker
```

The Rust rescue-init boot entry remains independent of this path.

## PAM and logind

The systemd build produces `/usr/lib/x86_64-linux-gnu/security/pam_systemd.so`; MattOS stages that built module and its runtime closure rather than copying a host PAM module. MattOS-owned `login`, `su-l`, and `systemd-user` policies invoke it as an optional session hook. Authentication failures in an unavailable session-registration service therefore do not replace the existing Unix authentication decision. Password-changing, `sudo`, plain `su`, and fallback PAM services do not load it.

`pam_systemd` registers each login with `systemd-logind`. A tty1 login is local, attached to `seat0`, and identified as `tty1`. A ttyS0 login is local and identified as `ttyS0`, but correctly has no seat. Session leaders and IDs come from logind; MattOS does not synthesize them.

## Runtime directory and user manager

For UID 1000, logind and `user-runtime-dir@1000.service` create `/run/user/1000` at runtime with mode `0700` and `mattos:mattos` ownership. The path is exposed as `XDG_RUNTIME_DIR`. No `/run/user` tree or bus socket is baked into the initramfs.

`user@.service` starts one `systemd --user` manager per logged-in UID. Imported systemd user targets, sockets, and environment generators are installed without enabling desktop services. The manager receives the login user's `HOME`, `USER`, `LOGNAME`, `SHELL`, `PATH`, and `XDG_RUNTIME_DIR`; the terminal supplies `TERM`. MattOS does not enable lingering, so this state is session-bound.

The design is UID-generic. A normally authenticated temporary user receives `/run/user/<uid>`, `user@<uid>.service`, its own manager, and its own bus; generic PAM policy and user units contain no UID 1000 paths.

## Per-user D-Bus

MattOS installs minimal user units under `/usr/lib/systemd/user`:

- `dbus.socket` listens on `%t/bus`, where `%t` is the user's runtime directory;
- `dbus-broker.service` launches the existing broker with `--scope user`;
- `dbus.service` aliases `dbus-broker.service`;
- `sockets.target.wants/dbus.socket` enables socket activation.

The socket unit exports `DBUS_SESSION_BUS_ADDRESS=unix:path=%t/bus` to the user-manager environment. `/usr/share/dbus-1/session.conf` is a separate session-bus policy: it permits users to own names on their private bus and loads only session service directories and policy fragments.

This bus is not the system bus. The system broker continues to listen at `/run/dbus/system_bus_socket` with the restrictive policy documented in `DBUS.md`. A user-bus connection does not grant control of PID 1; without Polkit, an unprivileged request such as restarting `systemd-timesyncd.service` on the system bus remains denied.

## Lifecycle

The live profile autologins `mattos` independently on tty1 and ttyS0, so both sessions may coexist while sharing one UID-scoped runtime directory, user manager, and user bus. When one session exits, logind removes that session but retains the shared per-user services while another session remains. When the last session exits, logind stops the manager, the bus, and the runtime-directory unit and removes `/run/user/<uid>`. Getty then creates a fresh registered autologin session.

Accounts and sessions remain ephemeral because the current root filesystem is an initramfs. Persistent users, lingering services, desktop-session components, and interactive authorization through Polkit remain outside this milestone.
