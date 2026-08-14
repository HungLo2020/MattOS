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
  UEFI/GPT/Btrfs layout, subvolumes, live-source selection and cleanup, Brush
  account policy, profile markers, fstab, installed initramfs, and GRUB.
- `cli/` is the permanent `mattos-install` frontend. It supports guided,
  non-destructive plan display, and acknowledged noninteractive execution.
- `gui/model.rs` is the toolkit-neutral graphical wizard model. It creates the
  same versioned `InstallPlan` and consumes the same structured policy progress
  events as every other installer interface.
- `gui/cosmic/` is the one permanent graphical frontend: Rust + libcosmic. It
  is a presentation/controller over the shared model, policy, and engine while
  COSMIC remains pinned upstream source under `src/desktop/cosmic/`.

The MattOS graphical installer does not use GTK, Qt, Vala, or a Vala
toolchain. Those ecosystems may have unrelated future package uses, but they
are not installer architecture.

System76 distinst informed the engine design. The Pop!/elementary Vala UI is the
historical interaction-design starting point. Ubuntu package policy, Pop
repositories and branding, elementary application identity, recovery/refresh
modes, systemd-boot/kernelstub, `update-initramfs`, OEM behavior, and arbitrary
distribution extension points are deliberately not retained.

`cosmic-initial-setup` is separate upstream COSMIC source under
`src/desktop/cosmic/cosmic-initial-setup`. It belongs to the
future first-login Desktop flow and is not part of disk installation.

The native COSMIC installer is built as an output-owned artifact and packaged
as `/usr/bin/mattos-install-cosmic`. The graphical boot entry starts this
native frontend; the CLI entry remains independently usable without COSMIC.

The first-class native frontend source closure pinned in `upstream/sources.toml`
contains libcosmic, its coordinated exact iced gitlink revision, and COSMIC
protocols. The installer application itself is MattOS-owned source. Ordinary
implementation crates—including settings D-Bus bindings, freedesktop icons,
winit, window clipboard, softbuffer, smithay clipboard, AccessKit, cryoglyph,
and atomicwrites—remain normal Cargo dependencies rather than authoritative
MattOS components.

The repository-wide classification rule is documented in
[`SOURCE_CLOSURE.md`](SOURCE_CLOSURE.md); future desktop imports must apply its
runtime-artifact/subsystem test before creating first-class source ownership.

The committed frontend `Cargo.lock` pins every Git dependency to an exact
40-hex commit and every registry dependency to its Cargo checksum. The builder
validates the reviewed Git source set before invoking `cargo build --locked`.
An online build may populate Cargo's normal cache; after population, the same
locked build works with `cargo build --locked --offline`. Cargo cache content
is generated/fetched state and never belongs under authoritative `src/`.

The installer artifact is
`out/build/installer/cosmic-target/release/mattos-install-cosmic`. Its
`--contract-proof` mode exercises its shared discovery/model contract without a
display server. The real window is the authoritative GUI and requires the
normal COSMIC graphical runtime, while the separate CLI boot entry is the
supported no-graphics installation path.

## Plan and safety contract

Plans use TOML schema version 1. The packaged example is
`/usr/share/doc/mattos-installer/example-plan.toml`.

```text
mattos-install plan /path/to/plan.toml
mattos-install install /path/to/plan.toml --yes-really-erase
```

Planning is non-destructive. Execution additionally requires root, an explicit
whole block device, at least 8 GiB, no mounted target filesystems, and proof that
the target is not the disk backing the running root. The guided frontend never
chooses a disk automatically and requires the literal confirmation `ERASE`.

The supported first installation mode is GPT whole-disk with UEFI installed
boot files. Encryption,
dual-boot, resize, BIOS installation, and recovery/refresh installation are not
currently exposed.

## MattOS disk and boot policy

- 512 MiB FAT32 EFI System Partition
- one Btrfs system partition
- `@` mounted at `/`
- `@home` mounted at `/home`
- `@snapshots` mounted at `/.snapshots`
- `compress=zstd:3,noatime`

The immutable SquashFS lower tree at `/run/mattos/lower` is copied into the
target, not the mutable live OverlayFS. Live account/autologin/state and the
live-only installer package are removed. `btrfs-progs` and `dosfstools` remain
separate normal administration packages.

The installed system has its own initramfs. GRUB passes the filesystem UUID;
early userspace probes sysfs partitions, mounts the Btrfs `@` subvolume, and
requires the root's recorded UUID to match. It then locates the sibling ESP by
sysfs parent/partition number (covering sd, vd, NVMe, and loop naming), mounts
`@home`, `@snapshots`, and the ESP, and switches to the normal writable root.
`/etc/fstab` and `/etc/mattos-storage.conf` retain UUID/PARTUUID identities and
never record `/dev/vda*`. UEFI GRUB is installed under the removable-media path
`EFI/BOOT/BOOTX64.EFI`.

## Frontend/profile independence

The same hybrid BIOS/UEFI ISO exposes five intended entry modes:

1. Start MattOS Live
2. Start MattOS Live (CLI)
3. Install MattOS
4. Install MattOS (CLI)
5. MattOS Rescue

GUI versus CLI selects the live presentation only. Either frontend may select
either installed profile:

- MattOS Desktop
- MattOS CLI

The CLI profile is the currently complete base installation. Desktop plans are
recorded explicitly, but the target receives `mattos-desktop-pending` until the
COSMIC desktop, cosmic-greeter, and separately pinned cosmic-initial-setup are
integrated.

## Graphical frontend and credential handling

The native Rust/libcosmic installer is a multi-page wizard: welcome;
currently-supported English (US)/US keyboard disclosure; installed profile;
explicit target disk; the real GPT/EFI/Btrfs layout; account setup; review; and
shared-engine execution. It never selects a disk automatically. Optical and
read-only media are not candidates. The review page makes the destructive
target, installed profile, hostname/user, UEFI boot mode, partition layout, and
Btrfs subvolumes explicit before the user can request installation.

Password plaintext stays only in the GUI input buffer. Immediately before the
shared `InstallPlan → policy validation → engine` execution path begins, it is
hashed with MattOS libxcrypt SHA-512, the plaintext buffers are cleared, and
only the crypt hash reaches the in-memory plan. Plaintext is never put in argv,
logs, or a persistent plan. Unattended CLI plans accept an explicit crypt hash.
