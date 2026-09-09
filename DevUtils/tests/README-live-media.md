# Live media boot regression

Build with `python3 DevUtils/run_qemu.py --build-only`, then run
`python3 DevUtils/tests/validate_live_media.py`. It boots one VM at a time:
optical SCSI CD-ROM, xHCI USB mass storage, SATA/AHCI disk, and NVMe disk.
All use the same ISO without modifying it. USB/optical use read-only backends;
SATA/NVMe use QEMU's automatically discarded snapshot overlay because ATA
rejects read-only backends and OVMF's NVMe boot issues commands requiring a
writable backend. The test verifies the ISO SHA-256 is unchanged afterward.
Serial diagnostics and QMP screenshots are
saved under `out/logs/live-media-*`; the harness shuts down each VM and removes
its control sockets. Use `--media usb` to select one case. Unit coverage is
`python3 -m unittest DevUtils/tests/test_live_media.py`.

Interactive equivalent: `python3 DevUtils/run_qemu.py --no-build
--no-install-disk --live-media usb` (one command). The default remains optical;
the normal graphical path retains KVM/VirGL. `--test-control` selects the
existing capturable test display, not production GPU acceleration.

## Physical x86_64 UEFI check

Write the ISO as a raw image of the **whole USB device**, using a disk-image
writer, not by copying its files into an existing filesystem. Double-check the
destination model/serial/capacity first; raw writing destroys its existing data.
Compare the written image bytes with the ISO, safely eject, and select the
USB's UEFI entry with Secure Boot disabled (these are not Secure Boot-signed
release images). Choose Start MattOS Live. Test keyboard/mouse, the COSMIC
desktop, wired/Wi-Fi networking and installer disk discovery before installing.

The ISO9660 volume label is `MATTOS_LIVE`. Early userspace probes enumerated
block devices for that label and ISO signature **before** mounting, then
requires `/live/rootfs.squashfs`. It does not assume `/dev/sr0` or `/dev/sda`.
Enumeration is retried for 60 seconds with five-second progress diagnostics.
The selected device is recorded at `/run/mattos/live-medium`.

A raw-written hybrid image retains its backup GPT at the image boundary.
On a larger USB device a warning that the alternate GPT is not at the end of
the disk describes that geometry; it does not itself mean GRUB or the ISO is
malformed. Do not repartition/repair the boot medium as a discovery workaround.

For a hardware report capture the early console and, when a shell is available:

For a display stall, switch to another VT (Ctrl+Alt+F2) or use an existing
serial/SSH connection, then run `sudo mattos-graphics-report > /tmp/graphics.txt`.
Copy that file to a separate writable USB filesystem before rebooting. The
command only observes state; each external diagnostic has a 15-second deadline.
It records PCI IDs, firmware availability, DRM connectors, driver files, seats,
failed units, and greetd/compositor logs. Missing advertised firmware is not
necessarily required by the detected GPU. Also preserve `sudo journalctl -b`
and `sudo dmesg` in full, the ISO SHA-256, GPU model, cable/monitor arrangement,
and a photograph of the last console output. Do not add `nomodeset` or disable
Display Core for the initial retest.

The automated suite proves transport and generic DRM/session startup only;
VirtIO/VirGL cannot validate AMDGPU register programming. Current Mesa uses
`libgallium-*.so`; absence of legacy `dri/radeonsi_dri.so` alone is not a defect.

```
uname -r
cat /proc/cmdline /run/mattos/live-medium /proc/partitions /proc/modules
findmnt / /run/mattos/medium
sudo dmesg
systemctl --failed --no-pager
systemctl status systemd-udevd NetworkManager display-manager --no-pager
pgrep -a cosmic
ip address
journalctl -b --no-pager
```

Report firmware-load failures and the exact storage/GPU PCI IDs. Virtual USB,
SATA, and NVMe tests do not prove a particular physical controller, GPU,
wireless firmware, or firmware implementation works.

## Graphics-start recovery regression

After building the image, run `python3 DevUtils/tests/validate_graphics_recovery.py`.
It starts one disposable USB-topology VM, proves a responsive COSMIC output,
stops that guest's compositor, and exercises the actual 120-second watchdog.
It requires preserved failure logs, active tty1, and a QMP-typed shell command
acknowledged over serial, then shuts down. Evidence is in
`out/logs/graphics-recovery.log` and `graphics-recovery.ppm`. It does not alter
the ISO, installed-test disk, host services, or GPU settings.

The **MattOS AMD graphics diagnostics (CLI)** boot entry enables extra DRM and
AMDGPU logging without disabling acceleration. Live boots save root-readable
reports in `/run/mattos-graphics/`; copy that directory with sudo to writable
external storage before rebooting. See the packaged COSMIC integration README
for capture/recovery semantics. A successful output-management round trip does
not prove that a physical monitor is displaying frames correctly.
