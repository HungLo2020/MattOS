---
name: run-qemu
description: Use MattOS's DevUtils/run_qemu.py for controlled ISO builds, QEMU boots, headless validation, and persistent-disk testing.
---

# MattOS QEMU workflow

Use this skill whenever a task needs to build or boot MattOS through the canonical
`DevUtils/run_qemu.py` workflow. Read the current script before relying on details;
the script is authoritative for supported options.

## Modes

Run from the repository root, or use the equivalent absolute path:

```bash
python3 DevUtils/run_qemu.py --build-only
python3 DevUtils/run_qemu.py
python3 DevUtils/run_qemu.py --headless --no-install-disk
python3 DevUtils/run_qemu.py --no-build --install-disk PATH
```

- The default performs `mattos-build doctor`, then `cargo run -p mattos-build -- build all`, validates the ISO, and launches QEMU.
- `--build-only` performs the build and ISO validation, then exits without QEMU.
- `--no-build` reuses the existing ISO and must only be used when its freshness is established separately.
- `--headless` omits the graphical GPU and uses `-nographic -serial stdio`; use it for bounded CI/serial validation.
- `--serial-console` keeps the graphical device but routes QEMU through serial output.
- By default a persistent `out/qemu/mattos-dev.qcow2` is created if absent. Use `--no-install-disk` for an ephemeral live-image boot, or `--install-disk PATH` for an explicit disk.
- `--clean` passes the canonical artifact-clean operation before rebuilding; use only when explicitly authorized.
- `--no-kvm` forces TCG. Otherwise the script uses KVM only when `/dev/kvm` is readable and writable.
- `--dry-run` prints commands but does not build, create a disk, or launch QEMU.

## Safety and validation

Before starting a new run, inspect processes for existing MattOS `run_qemu.py`,
`mattos-build`, Cargo, rustc, and QEMU processes. Do not kill unrelated VMs.
During expensive builds, compare actual stage/compiler activity with the planned
scope and stop only the MattOS validation process tree if an unexpected cascade
appears.

The default graphical path intentionally requires QEMU's `virtio-vga-gl` and a
GL-capable display because the COSMIC desktop exercises VirGL/DRM/KMS. The
headless path intentionally omits that device. UEFI firmware is required and is
searched at `/usr/share/ovmf/OVMF.fd` and `/usr/share/qemu/OVMF.fd`.

`run_qemu.py` validates that the ISO contains `/live/rootfs.squashfs` before
launching. Runtime validation should record UEFI/GRUB progress, systemd target,
NetworkManager, and the intended live or installed environment. Do not claim
installed-system behavior from a live boot.

The build environment comes from `DevUtils/common/helpers.py`; do not replace it
with host binaries, ad-hoc Cargo invocations, or a second build system.
