# MattOS Wired/QEMU Networking

Date: 2026-08-01

## Scope

MattOS provides the first complete network path for QEMU and matching wired Ethernet devices:

```text
virtio-net-pci -> Linux virtio_net -> systemd-networkd IPv4 DHCP
                                -> systemd-resolved DNS/NSS
                                -> systemd-timesyncd clock synchronization
                                -> iproute2, iputils, curl HTTP/HTTPS
```

The default development launcher uses QEMU user-mode networking. This milestone deliberately excludes Wi-Fi, SSH, package management, an installer, persistence, firewall policy, GUI network management, and broader physical NIC driver support.

## Upstream Components

| Component | Repository | Branch | Imported commit |
| --- | --- | --- | --- |
| iproute2 | https://git.kernel.org/pub/scm/network/iproute2/iproute2.git | `main` | `5696fee4c69fe3cc12e8cc821630633f616db8e2` |
| iputils | https://github.com/iputils/iputils.git | `master` | `75cd9d544baad45f81ed5c72bca332f577c3d81e` |
| curl | https://github.com/curl/curl.git | `master` | `527573490eb2564b3d7c9dd51d8bff963b5d6303` |

They are ordinary imported files under `src/userland/`; no submodules or nested Git repositories are used.

## Kernel and Device Model

The monolithic kernel enables the packet and IPv4 socket families, inet/netlink diagnostics, PCI virtio networking, and `CONFIG_VIRTIO_NET=y`. The launcher supplies:

```text
-netdev user,id=net0 -device virtio-net-pci,netdev=net0
```

No physical Ethernet family or wireless stack was enabled as part of this milestone. `python3 DevUtils/run_qemu.py --no-network` omits both arguments for a deterministic disconnected boot.

## Runtime Configuration

- `/etc/systemd/network/20-mattos-wired.network` matches Ethernet links and requests IPv4 DHCP.
- `systemd-networkd.service`, `systemd-resolved.service`, and `systemd-timesyncd.service` are enabled in `multi-user.target`.
- `/etc/resolv.conf` points to resolved's local stub file under `/run/systemd/resolve/`.
- `/etc/nsswitch.conf` routes host lookups through `nss-resolve`, with conventional file and DNS fallbacks.
- `/etc/systemd/timesyncd.conf` selects `time.cloudflare.com` and pool fallbacks.
- The systemd sysusers definitions own the three service accounts with fixed non-colliding IDs 192 through 194.

## Commands and Privilege Model

Installed network commands are `ip`, `ss`, `bridge`, `tc`, `ping`, `tracepath`, `curl`, `networkctl`, `resolvectl`, and `timedatectl`.

MattOS permits ICMP datagram sockets for all groups with `net.ipv4.ping_group_range`. This lets the non-root live user run `ping` without setuid and without a file capability that the current `newc` initramfs cannot preserve.

curl is intentionally configured for HTTP and HTTPS only, with IPv4 and the blocking glibc resolver. HTTPS verification uses OpenSSL and the compiled default CA path `/etc/ssl/certs/ca-certificates.crt`.

## Pinned Trust Store

The trust store is the dated curl-hosted Mozilla CA extract from 2026-07-16:

- Source: `https://curl.se/ca/cacert-2026-07-16.pem`
- SHA-256: `3ff344e30b9b1ed2971044eabb438a08f2e2245ddb5f8ab1a3ad8b63ab4eaf91`
- Certificate count: 119
- License: Mozilla Public License 2.0
- Metadata: `src/system/network/ca-bundle.toml`

The dated URL and recorded digest make image assembly independent of an unpinned moving download. Updating the bundle is a deliberate source change: fetch a newer dated extract, verify its published checksum, replace the PEM, and update the metadata together.

## Validation Commands

Inside a normal graphical boot:

```sh
ip link
ip addr
ip route
networkctl
resolvectl status
sudo systemctl status systemd-timesyncd --no-pager
ls -l /run/systemd/timesync/synchronized
ping -c 3 10.0.2.2
ping -c 3 example.com
ss -lntu
curl -I http://example.com/
curl -I https://example.com/
```

The disconnected validation boots with `--no-network` and confirms that the normal getty, authentication, Brush, systemd, and base-administration paths remain usable without a NIC.

## Validated Behavior

The 2026-08-01 graphical QEMU validation observed:

- interface `ens3` up with DHCP address `10.0.2.15/24`;
- default route and gateway through `10.0.2.2`;
- resolved DNS server `10.0.2.3` and successful glibc `getent hosts example.com`;
- zero-loss pings to `10.0.2.2` and `example.com`;
- successful certificate-verified `curl -I https://example.com` and body download;
- active networkd, resolved, and timesyncd processes;
- timesyncd contacting `time.cloudflare.com`, logging initial clock synchronization, and creating `/run/systemd/timesync/synchronized`;
- a loopback-only, route-free but otherwise clean `--no-network` boot;
- unchanged non-root autologin, sudo, Brush, procps, ncurses, session restart, and rescue-init behavior.

The image now includes the source-built dbus-broker system bus. Non-root `networkctl`, `resolvectl status`, `timedatectl`, and `systemctl status` connect through `/run/dbus/system_bus_socket`, and the networkd, resolved, timesyncd, and timedated well-known names resolve through their installed policies and aliases. MattOS has no Polkit, so administrative changes can still be denied even though read-only bus access works.

## glibc resolver boundary

glibc and every native networking consumer are rebuilt against the MattOS sysroot. `mattos-libc6` owns `libresolv.so.2`, `libnss_dns.so.2`, and `libnss_files.so.2`; systemd packages continue to provide `libnss_resolve.so.2`. The existing `hosts: files resolve [!UNAVAIL=return] dns` and `networks: files dns` configuration is unchanged. Consequently `getent hosts`, `ping`, curl, APT, PAM account resolution, and ordinary application APIs all use the MattOS-built loader and resolver rather than a hidden host libc fallback. HTTPS certificate verification remains pinned to `/etc/ssl/certs/ca-certificates.crt` and is not disabled by this transition.
