# MattOS Wired/QEMU Networking

This page describes the MattOS network stack on the live image and on
installed systems. Despite the historical title, it covers wired, QEMU and
Wi-Fi networking; GRUB/Wi-Fi hardware bring-up is in
[Wi-Fi and GRUB](boot/wifi-and-grub.md).

## Scope

NetworkManager owns interface configuration:

```text
NIC driver (e.g. virtio_net, e1000e, r8169, iwlwifi) -> NetworkManager
                                -> wpa_supplicant (Wi-Fi, D-Bus activated)
                                -> systemd-resolved DNS/NSS
                                -> systemd-timesyncd clock synchronization
                                -> iproute2, iputils, curl HTTP/HTTPS
```

`systemd-networkd` is built (it is part of the systemd package) but masked:
the image build links `/etc/systemd/system/systemd-networkd.service` to
`/dev/null` and fails if that mask is missing while NetworkManager is enabled
(`install_network_configuration` and its validator in
`src/tools/mattos-build/src/stages/image.rs`). No `.network` file is
installed, so networkd cannot race NetworkManager.

The development launcher uses QEMU user-mode networking. There is no MattOS
firewall policy yet.

## Upstream Components

| Component | Repository | Branch | Imported commit |
| --- | --- | --- | --- |
| iproute2 | https://git.kernel.org/pub/scm/network/iproute2/iproute2.git | `main` | `5696fee4c69fe3cc12e8cc821630633f616db8e2` |
| iputils | https://github.com/iputils/iputils.git | `master` | `75cd9d544baad45f81ed5c72bca332f577c3d81e` |
| curl | https://github.com/curl/curl.git | `master` | `527573490eb2564b3d7c9dd51d8bff963b5d6303` |
| NetworkManager | https://github.com/NetworkManager/NetworkManager.git | `main` | `355bc902994f8f616bc80dcb451455f7aeb6b809` |
| wpa_supplicant (hostap) | https://git.w1.fi/hostap.git | `hostap_2_12` | `831364bf02710ad09c2f27d3efa92abeeb5634c0` |
| OpenSSH portable | https://github.com/openssh/openssh-portable.git | `V_10_4_P1` | `e8dd756725e8800fcd0b3fd71ee6b4382d1e8fab` |

iproute2, iputils and curl are imported under `src/userland/`;
NetworkManager, hostap and OpenSSH are imported under `src/system/network/`
beside the MattOS-owned configuration. Pins are recorded in
`upstream/state/*.toml`; no submodules or nested Git repositories are used.

## Kernel and Device Model

The MattOS kernel is modular (`CONFIG_MODULES=y`). Packet and IPv4/IPv6
sockets are built in; NIC and wireless drivers are modules, including
`virtio_net` (`CONFIG_VIRTIO_NET=m`), common Ethernet drivers (`e1000`,
`e1000e`, `igb`, `igc`, `r8169`) and the `cfg80211`/`mac80211` stack with
drivers such as `iwlwifi`, `ath11k` and `mt7921e`
(`src/kernel/config/x86_64_mattos.config`). The launcher supplies:

```text
-netdev user,id=net0 -device virtio-net-pci,netdev=net0
```

`python3 DevUtils/run_qemu.py --no-network` omits both arguments for a
deterministic disconnected boot.

## Runtime Configuration

The MattOS-owned files live in `src/system/network/` and are installed by
`install_network_configuration`:

- `NetworkManager.service`, `systemd-resolved.service` and
  `systemd-timesyncd.service` are enabled in `multi-user.target` on the live
  image. The installer enables the same three services, plus `ssh.service`,
  on every installed system (`configure_installed_profile_values` in
  `src/system/installer/policy/mod.rs`).
- `wpa_supplicant.service` is started on demand through D-Bus activation of
  `fi.w1.wpa_supplicant1` when NetworkManager manages Wi-Fi.
- NetworkManager runs with its upstream defaults; no MattOS
  `NetworkManager.conf` or `conf.d` snippet is shipped. Connection profiles
  are kept in `/etc/NetworkManager/system-connections/`.
- `/etc/resolv.conf` is a symbolic link to
  `/run/systemd/resolve/stub-resolv.conf`; no host resolver file is copied.
- `resolved.conf` is installed as
  `/etc/systemd/resolved.conf.d/10-mattos.conf` (stub listener on; LLMNR,
  mDNS and DNSSEC off).
- `/etc/nsswitch.conf` routes host lookups through `nss-resolve`, with
  conventional file and DNS fallbacks.
- `timesyncd.conf` is installed as
  `/etc/systemd/timesyncd.conf.d/10-mattos.conf` and selects
  `time.cloudflare.com` with `0.pool.ntp.org` and `1.pool.ntp.org` as
  fallbacks.
- The systemd sysusers definitions own the `systemd-network`,
  `systemd-resolve` and `systemd-timesync` accounts with fixed non-colliding
  IDs 192 through 194.
- `99-mattos-network.conf` is installed in `/etc/sysctl.d/`.

## Commands and Privilege Model

Installed network commands include `nmcli`, `ip`, `ss`, `bridge`, `tc`,
`ping`, `tracepath`, `curl`, `ssh`, `wpa_cli`, `networkctl`, `resolvectl`,
`timedatectl`, `loginctl` and `busctl`. In the Plasma session the
`plasma-nm` applet provides the graphical front end to NetworkManager.

MattOS permits ICMP datagram sockets for all groups with
`net.ipv4.ping_group_range = 0 2147483647`. This lets non-root users run
`ping` without a setuid bit or file capability; the packaged `ping` has
neither.

curl is intentionally configured for HTTP and HTTPS only, with IPv4 and the
blocking glibc resolver. HTTPS verification uses OpenSSL and the compiled
default CA path `/etc/ssl/certs/ca-certificates.crt`.

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
nmcli general status
nmcli device status
ip addr
ip route
resolvectl status
systemctl status NetworkManager systemd-resolved systemd-timesyncd --no-pager
ls -l /run/systemd/timesync/synchronized
ping -c 3 10.0.2.2
ping -c 3 example.com
ss -lntu
curl -I https://example.com/
```

`networkctl` still runs, but it reports networkd's view and is not useful
while networkd is masked.

The disconnected validation boots with `--no-network` and confirms that the
normal login, systemd, and base-administration paths remain usable without a
NIC.

System-bus clients reach NetworkManager, resolved, timesyncd and timedated
through the dbus-broker system bus described in
[MattOS System D-Bus](services/dbus.md). Polkit
(`org.freedesktop.PolicyKit1`) and, in Plasma sessions, `polkit-kde-agent-1`
authorize administrative requests such as NetworkManager connection changes.

## glibc resolver boundary

glibc and every native networking consumer are rebuilt against the MattOS sysroot. `libc6` owns `libresolv.so.2`, `libnss_dns.so.2`, and `libnss_files.so.2`; systemd packages continue to provide `libnss_resolve.so.2`. The existing `hosts: files resolve [!UNAVAIL=return] dns` and `networks: files dns` configuration is unchanged. Consequently `getent hosts`, `ping`, curl, APT, PAM account resolution, and ordinary application APIs all use the MattOS-built loader and resolver rather than a hidden host libc fallback. HTTPS certificate verification remains pinned to `/etc/ssl/certs/ca-certificates.crt` and is not disabled by this transition.

## History (2026-08-01 milestone)

The first network milestone used `systemd-networkd` with a MattOS
`20-mattos-wired.network` file (Ethernet IPv4 DHCP) and a monolithic kernel
with `CONFIG_VIRTIO_NET=y`. Its 2026-08-01 graphical QEMU validation observed:

- interface `ens3` up with DHCP address `10.0.2.15/24`;
- default route and gateway through `10.0.2.2`;
- resolved DNS server `10.0.2.3` and successful glibc `getent hosts example.com`;
- zero-loss pings to `10.0.2.2` and `example.com`;
- successful certificate-verified `curl -I https://example.com` and body download;
- active networkd, resolved, and timesyncd processes;
- timesyncd contacting `time.cloudflare.com`, logging initial clock
  synchronization, and creating `/run/systemd/timesync/synchronized`;
- a loopback-only, route-free but otherwise clean `--no-network` boot;
- unchanged non-root autologin, sudo, Brush, procps, ncurses, session restart,
  and rescue-init behavior.

NetworkManager has since replaced networkd. The legacy
`src/system/network/network/20-mattos-wired.network` source file is kept but
is not installed.
