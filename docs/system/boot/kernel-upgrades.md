# Installed kernel packages and upgrades

Both `mattos-cli` and `mattos-plasma` inherit `linux-image-amd64` through
`mattos-base`. This tracking package depends on the exact current versioned
`linux-image-<release>` package, which depends on the matching
`linux-modules-<release>` package and the GRUB/hook runtime.

The image package owns `/boot/vmlinuz-<release>` and
`/boot/initrd.img-<release>`. The initramfs is built from MattOS's
`installed-init.c` and the same kernel's boot-module closure. The tracking
package owns `/usr/lib/mattos/kernel-release`, selecting the current release.
The live installer validates these files in the composed target; it does not
copy a second kernel/initramfs from its private asset directory.

On configuration, the image and tracking packages select the compatibility
links `/boot/vmlinuz` and `/boot/installed-initramfs.cpio.xz` when their release
matches the tracking package's marker. Reconfiguring an older retained image
therefore cannot switch the default back. They invoke executable regular-file
hooks in `/etc/kernel/postinst.d`, passing the release and image path. GRUB's
hook regenerates `/boot/grub/grub.cfg` only on an installed root with
`/etc/default/grub`. Image removal removes only aliases pointing to that image
and invokes `/etc/kernel/postrm.d` to remove stale menu entries.

Chrootless package composition uses `DPKG_ROOT`: links are selected inside that
root, and host kernel hooks are never executed. The installer writes the target
storage-specific GRUB defaults and installs/generates GRUB after composition.

An upgrade from older MattOS installations adopts the installer-created,
unowned versioned boot files into the image package. A new release installs
beside the old release; the old image/initramfs remain available as a GRUB
fallback. APT tracks future releases through `linux-image-amd64`; the live-only
`mattos-installer` package is not required on installed profiles.

Kernel upstream changes update `mattos_kernel_release!`, the MattOS kernel
configuration and the compatibility metadata together. Rename or remove obsolete
versioned module/image entries in `src/system/packages/revisions.toml` as well;
the revision ledger rejects names outside the current package set. Changes to an image or
initramfs at the same upstream version require packaging revision increases,
including the tracking/profile packages' exact dependency pins. Keep the
module package and image paired; do not update the independently pinned
userspace API header source merely to change the booted kernel.

The [installer upgrade test](../installer.md#upgrade-test) records the running
baseline release and the expected built release. After reboot,
`kernel-matches-build` requires that `uname -r` equals the built release,
that the selected image/modules/tracker are installed, that dpkg owns both
versioned boot files, that the compatibility links select that release, and
that both files' SHA-256 values match the current build. A release-changing
upgrade uses `--require-kernel-change` and must show `kernel_release_changed: true`; a same-release test
alone cannot prove that a newer kernel was selected.
