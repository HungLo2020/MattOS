# MattOS network policy

This directory owns the live image's wired DHCP, resolver, time-sync, NSS,
and trust-store configuration. Imported systemd, iproute2, iputils, and curl
source trees remain unmodified.

`20-mattos-wired.network` matches Ethernet devices by type rather than by a
QEMU-specific interface name. `/etc/resolv.conf` is assembled as a symlink to
`/run/systemd/resolve/stub-resolv.conf`; no host resolver file is copied.
