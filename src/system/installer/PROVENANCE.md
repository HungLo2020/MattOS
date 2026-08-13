# Installer historical provenance

`src/system/installer/**` is MattOS-owned code, independently maintained as of
2026-08-11. The repositories below are historical starting points, not current
upstream synchronization targets and not authoritative imported source trees.

## System76 distinst

- Repository: `https://github.com/pop-os/distinst.git`
- Starting revision: `16037d2aa56b5d81d458ed1a892e626eeb0c0ce8`
- Upstream tree: `9592f3e26b7a72c102478cbb465447810181b0e7`
- License: GNU Lesser General Public License 3.0 or later (`LGPL-3.0-or-later`)

Distinst informed the disk, partition, filesystem, and installation-mechanics
design. MattOS retained the useful Rust design ideas but did not retain its
Ubuntu/Pop package policy, systemd-boot/kernelstub path, `update-initramfs`
assumptions, recovery/refresh modes, or general multi-distribution abstraction.

## Pop!/elementary graphical installer

- Repository: `https://github.com/pop-os/installer.git`
- Starting revision: `5fb8c92ad1c2dfce4f0398fcd9c38de764f5648b`
- Upstream tree: `7bf5f6d54175553f4e357803411cc62e52f44821`
- License: GNU General Public License 3.0 or later (`GPL-3.0-or-later`)

Its Vala/GTK flow and presentation are the historical starting point for the
MattOS-owned GUI. Pop!, elementary, Ubuntu, OEM, recovery, refresh-install, and
systemd-boot-specific behavior is intentionally not part of MattOS policy.

Copyright and attribution remain with their respective upstream contributors.
MattOS modifications are not represented as upstream System76/elementary work.

## COSMIC boundary

`cosmic-initial-setup` is not installer source and is not covered by this
ownership transition. It remains a separately pinned normal upstream component
for future first-login Desktop setup.
