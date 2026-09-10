# Source-owned Wi-Fi and GRUB

NetworkManager's default Wi-Fi backend is wpa_supplicant. Merely enabling Wi-Fi
in NetworkManager does not provide that backend. MattOS builds hostap 2.12 with
nl80211, libnl3, OpenSSL, and its D-Bus interface. The `wpasupplicant` package
provides upstream's D-Bus activation/policy files and a system service started
on demand as `fi.w1.wpa_supplicant1`. NetworkManager owns connection management;
the service does not start a competing per-interface configuration.

GRUB 2.14 and its declared Gnulib revision are independently pinned in
`upstream/sources.toml`. Bootstrap uses output-owned mirrors, a pinned
Autoconf Archive macro source, no Git fetching, and no translation downloads.
Both i386-pc and x86_64-efi modules are built to preserve the existing hybrid
ISO contract. Native utilities and EFI modules belong to `grub-efi-amd64`.
The 32-bit BIOS module set remains an image-build artifact only; it is not
included in the installed x86_64 UEFI package or exempted from ELF auditing.
The boot-label PF2 font is generated from MattOS's already pinned Open Sans
Regular using its owned FreeType stage, never a font from the build host.
Only disposable build-time font converters link FreeType; installed GRUB
utilities do not gain a desktop-library dependency for this image resource.
The configure patch makes `--without-unifont` explicitly disable host font
discovery and Unifont's special embedded ASCII/control-glyph generation.
GRUB retains its built-in ASCII fallback; its upstream font converter generates
the external PF2 resource separately. An ordinary proportional font cannot be
substituted for Unifont in the embedded 128-glyph table generator.

The checksummed output-mirror patch adds `pkglibdir` relocation, analogous to
upstream's existing `pkgdatadir` environment override. Image construction
sets both to MattOS stage outputs and executes the source-owned utilities
with the MattOS ELF loader. With `pkglibdir` unset, installed utilities keep
their ordinary `/usr/lib/grub` behavior. This avoids host GRUB module leakage
without embedding an absolute build workspace path in installed utilities.

The second patch addresses [Debian bug 787795](https://bugs.debian.org/787795):
upstream mkrescue takes its boot-search UUID from wall-clock time even if the
caller fixes the ISO filesystem date. MattOS honors `SOURCE_DATE_EPOCH` for
that UUID and rejects invalid/out-of-range values. The existing canonical ISO
command already fixes xorriso file dates; neither the compression policy nor
image layout changes. Both BIOS filesystem-UUID and EFI marker-file searches
must therefore agree with the reproducible image timestamp.

Installed systems use upstream `grub-install --target=x86_64-efi --removable
--no-nvram` and `grub-mkconfig`, not a handcrafted one-entry menu. The installer
pairs `/boot/vmlinuz-<release>` with `/boot/initrd.img-<release>` and writes
`/etc/default/grub` with its filesystem identity. `update-grub` and standard
kernel post-install/removal hooks regenerate menus from kernels actually
present; they do not invent older kernels. Upstream's firmware-settings entry
and the other grub.d scripts are dpkg conffiles, preserving administrator
edits such as `40_custom` on package upgrades. Generated grub.cfg and installed
`/etc/default/grub` are not overwritten by the GRUB package. The firmware entry
is conditional on UEFI firmware support. Other-OS probing is disabled because
MattOS does not ship os-prober. Secure Boot signing, LVM, ZFS, and TPM provisioning
are not added by this integration.

The recipe selects `-Ttext` instead of GRUB 2.14's automatic `--image-base`
choice. With Binutils 2.46 the latter moved the BIOS entry point to `0x9074`
instead of the required `0x9000`, and grub-mkrescue correctly rejected it.
This is the same defect documented in [LFS #5857](https://wiki.linuxfromscratch.org/lfs/ticket/5857).
The linker capability-cache constraint is explicit in the recipe; GRUB's
entry-address validation remains enabled and the artifact integration test
must successfully generate both boot images. The generated global Info `dir`
index is excluded from the GRUB package to avoid cross-package ownership.

Validation must distinguish QEMU D-Bus/service activation from physical Wi-Fi
association: a virtual Ethernet interface cannot prove that a physical radio
associates successfully. Collect NetworkManager and wpa_supplicant journals
on the real device when testing connectivity.
