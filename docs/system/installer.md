# MattOS Installer

The installer is a MattOS-owned subsystem under `src/system/installer`. It is
not an adapter around a pristine upstream installer and it is not designed to
remain source-compatible with System76 distinst or the Pop!/elementary GUI.
Exact historical revisions, licenses, and attribution are recorded in
`src/system/installer/PROVENANCE.md`.

## Architecture

- `engine/` owns reusable destructive-operation mechanics: disk validation,
  partition naming, command execution, mount lifetime/cleanup, and the small
  installed-system initramfs.
- `policy/` defines a MattOS installation: plan schema, target constraints,
  UEFI/GPT/Btrfs layout, subvolumes, offline package composition, Brush
  account policy, profile markers, fstab, installed initramfs, and GRUB.
- `cli/` is the permanent `mattos-install` frontend. It supports guided,
  non-destructive plan display, and acknowledged noninteractive execution.
- `gui/model.rs` is the toolkit-neutral graphical wizard model. It creates the
  same versioned `InstallPlan` and consumes the same structured policy progress
  events as every other installer interface.
- `calamares/` is the graphical frontend: a pinned, unmodified Calamares 3.4
  import (`calamares/upstream/`) with MattOS policy and branding in
  `calamares/mattos/`. See `src/system/installer/calamares/README.md` for its
  Qt 6 / KDE Frameworks source closure.

System76 distinst informed the engine design. The Pop!/elementary Vala UI is the
historical interaction-design starting point. Ubuntu package policy, Pop
repositories and branding, elementary application identity, recovery/refresh
modes, systemd-boot/kernelstub, `update-initramfs`, OEM behavior, and arbitrary
distribution extension points are deliberately not retained.

The repository-wide classification rule is documented in
[MattOS source-closure policy](../sources/source-closure.md); future desktop imports must apply its
runtime-artifact/subsystem test before creating first-class source ownership.

## Graphical installer (Calamares)

In the live KDE Plasma session, the **Install MattOS (Calamares)** launcher
starts Calamares. Its pages are welcome, locale, keyboard, partitioning, users,
the MattOS profile chooser, summary, progress and completion. QML, Kirigami,
webview, package-manager and desktop-specific Calamares modules are
deliberately excluded.

Calamares owns the UI, partitioning and mounts; it does not decide the package
closure. Its profile chooser records `cli` or `plasma`. Two `shellprocess`
jobs then hand the Calamares-mounted target to the Rust installer through
`/usr/libexec/mattos/calamares-target-executor`, which execs
`mattos-install calamares --phase <phase> --profile <profile> --target <root>`:

- `mattos-executor` runs `--phase compose` after Calamares mounts the target,
  reusing the Rust profile resolver and offline repository transaction.
- `mattos-finalize` runs `--phase finalize` after Calamares' `users`, `locale`
  and `keyboard` jobs. It writes `/etc/mattos-storage.conf`, the
  subvolume-aware `/etc/fstab`, the installed initramfs and GRUB boot files,
  and the installed-profile marker. Calamares' generic `fstab` module is
  deliberately not in the exec sequence.

The Calamares partition module (`calamares/mattos/modules/partition.conf`)
defaults to erasing a disk with a GPT table, an ESP at `/boot/efi`, a Btrfs
root and no swap; its manual partitioning page remains available, and the
finalizer accepts a Btrfs or ext4 root. Calamares requires 15 GiB of storage
(`requiredStorage: 15GiB`).

## Plan and safety contract

Plans use TOML schema version 1. The packaged example is
`/usr/share/doc/mattos-installer/example-plan.toml`.

```text
mattos-install plan /path/to/plan.toml
mattos-install install /path/to/plan.toml --yes-really-erase
```

Planning is non-destructive. Execution additionally requires root, an explicit
whole block device, at least 8 GiB (the Rust policy's `MINIMUM_DISK_BYTES`; the
Calamares frontend separately asks for 15 GiB), no mounted target filesystems,
and proof that the target is not the disk backing the running root. The guided
frontend never chooses a disk automatically and requires the literal
confirmation `ERASE`. Unattended CLI plans accept an explicit crypt hash for the
account password.

`mattos-install guided` offers two storage modes:

- **guided** (default): the whole disk with a Btrfs (default) or ext4 root,
  and either a newly created ESP or an existing EFI system partition reused
  with or without formatting.
- **manual**: one explicit operation per partition (`create`, `delete`,
  `preserve`, `reuse` or `format`), with Btrfs, ext4 or FAT32 filesystems and
  unique mount roles; `/` and `/boot/efi` are required, `/home` is optional.

Installed boot files are always UEFI/GPT. The CLI does not offer encryption;
BIOS installation, resize and recovery/refresh installation are not exposed.

## MattOS disk and boot policy

- 512 MiB FAT32 EFI System Partition
- one Btrfs system partition
- `@` mounted at `/`
- `@home` mounted at `/home`
- `@snapshots` mounted at `/.snapshots`
- `compress=zstd:3,noatime`

This is the default guided layout; an ext4 root uses a single filesystem
without subvolumes (manual mode can add a separate `/home` partition).

The target is composed offline from the package repository on the live medium,
not copied from the live root: the installer resolves the selected profile's
meta-package closure plus `mattos-toolchain` (every installed system carries
the native development toolchain) and installs them with `dpkg`, then copies
the repository onto the target as its local APT source. `btrfs-progs`,
`dosfstools` and `e2fsprogs` are ordinary packages that `mattos-base` depends
on, so every installed system has them.

The guided CLI also offers optional applications installed through Flatpak.
These need Internet access; a failure to install them does not stop the MattOS
installation.

The installed system has its own initramfs, built from
`src/system/installer/engine/installed-init.c`. GRUB passes
`mattos.root_uuid` and `mattos.root_fstype` (`btrfs` or `ext4`). Early
userspace loads the boot-critical kernel modules, then tries each partition
listed in `/sys/class/block`: Btrfs candidates are mounted with
`subvol=@,compress=zstd:3`, ext4 candidates directly. A candidate is accepted
only if it has `/usr/lib/systemd/systemd`, `/etc/mattos-installed-profile`,
and a `root_uuid=` line in `/etc/mattos-storage.conf` matching the command
line; otherwise it is unmounted and the scan continues (retrying for about ten
seconds). The initramfs then switches root and execs systemd. It mounts only
the root: the ESP, `@home` and `@snapshots` are mounted afterwards by systemd
from `/etc/fstab`. `/etc/fstab` and `/etc/mattos-storage.conf` retain
UUID/PARTUUID identities and never record `/dev/vda*`. UEFI GRUB is installed under the removable-media path
`EFI/BOOT/BOOTX64.EFI`.

## Boot entries and installed profiles

The hybrid BIOS/UEFI ISO offers:

1. Start MattOS Live (the KDE Plasma live session, with the graphical installer)
2. Start MattOS Live (CLI)
3. Install MattOS (CLI)
4. MattOS Rescue
5. MattOS AMD graphics diagnostics (CLI)
6. UEFI Firmware Settings (only when booted through UEFI and GRUB's
   `fwsetup --is-supported` succeeds)

Either frontend may select either installed profile:

- **Plasma** (`mattos-plasma`): the KDE Plasma Wayland desktop
- **CLI** (`mattos-cli`): the command-line base system

Both profiles also receive `mattos-toolchain`.

## Validation and initial package discovery

`DevUtils/run_qemu.py --install` requires the installer and independent disk
boot to exit successfully. Forced QEMU termination is a failure, even when
QEMU itself returns zero. Failed test disks are retained without a completion
marker for diagnosis; a subsequent explicit `--install` replaces the test disk.
The boot checks include compositor/panel processes and a toolchain
compile-and-run check; these are not a substitute for interactive GUI or
application acceptance tests.

Installed systems enable a bounded APT index bootstrap 15 seconds after boot,
independently of login. Only a fully successful refresh records completion.
Offline failures do not prevent installation or login, and retry on a later
boot; the randomized daily metadata refresh remains enabled. No native package
upgrade is performed by either service. Inspect `mattos-apt-bootstrap.service`
when package discovery is empty immediately after a fresh installation.
