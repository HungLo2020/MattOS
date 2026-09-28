# MattOS network policy

This directory owns MattOS's resolver, time-sync, NSS, sysctl, and trust-store
configuration, plus the MattOS-owned wpa_supplicant and OpenSSH integration
files (`wpa-supplicant/`, `openssh/`). It also holds the pinned upstream
imports of NetworkManager, hostap (wpa_supplicant), OpenSSH portable, libnl
and libndp; see `upstream/state/` for their exact commits. Imported systemd,
iproute2, iputils, and curl source trees remain unmodified.

NetworkManager owns interface configuration. The image build
(`install_network_configuration` in
`src/tools/mattos-build/src/stages/image.rs`) enables `NetworkManager.service`,
`systemd-resolved.service` and `systemd-timesyncd.service`, and masks
`systemd-networkd.service`. `resolved.conf` and `timesyncd.conf` are installed
as `10-mattos.conf` drop-ins under `/etc/systemd/`. `/etc/resolv.conf` is
assembled as a symlink to `/run/systemd/resolve/stub-resolv.conf`; no host
resolver file is copied.

`network/20-mattos-wired.network` is the legacy systemd-networkd Ethernet
DHCP policy. It is kept for recovery use but is not installed into images.
