# MattOS Milestone: First Bootable Baseline

Date: 2026-07-31

## Upstream component commits

- Linux: `f17f39c917cd4aac09db1a6a083ef5ec09b4924d` (torvalds/linux, branch `master`)
- Brush: `71afef7ce79ad2fd94833fa4f93fa5486c86c56b` (reubeno/brush, branch `main`)
- uutils coreutils: `91f6543cad721aba0bf17806e803e84a116f8603` (uutils/coreutils, branch `main`)

## Build and validation commands

```bash
cargo run -p mattos-build -- clean artifacts
cargo run -p mattos-build -- build all
cargo run -p mattos-build -- image

( sleep 12; printf 'pwd\nls /\necho MattOS\nuname -a\ncat /proc/version\nmkdir -p /tmp/test\ntouch /tmp/test/file\nls /tmp/test\n'; sleep 2 ) \
  | timeout 240s qemu-system-x86_64 \
      -m 1024 \
      -cdrom out/images/mattos-x86_64.iso \
      -nographic \
      -serial stdio \
      -monitor none \
      -no-reboot \
      -no-shutdown \
      > out/logs/qemu-milestone-final.log 2>&1 || true
```

## ISO artifact

- Path: `out/images/mattos-x86_64.iso`
- Size: `29319168` bytes (`28M`)

## In-guest validation results

From `out/logs/qemu-milestone-final.log`:

- Brush prompt reached: `MattOS #`
- `pwd` output: `/`
- `ls /` output includes: `README.md bin dev etc lib lib64 proc root sbin sys tmp usr var`
- `echo MattOS` output: `MattOS`
- `uname -a` output contains: `Linux (none) 7.2.0-rc5 ... x86_64 GNU/Linux`
- `cat /proc/version` output contains: `Linux version 7.2.0-rc5 ...`
- `mkdir -p /tmp/test && touch /tmp/test/file && ls /tmp/test` output: `file`

## Architecture snapshot

- Boot mode: BIOS GRUB ISO (`grub-mkrescue`)
- Kernel: Linux `arch/x86/boot/bzImage` with MattOS seed config
- Initramfs: generated cpio gzip archive from `out/build/rootfs`
- PID 1: `userland/init` (`/sbin/init`)
- Interactive shell: `brush` launched by init
- Core user commands: provided by multicall `coreutils` with links in `/bin` and `/usr/bin`

## Milestone limitations

- Minimal rootfs only; no package manager or installer flow.
- No graphics stack or desktop environment.
- No network configuration/userland networking setup in rootfs baseline.
- Boot validation currently targets serial-console QEMU path.

## Notes on boot blocker fixed in this milestone

- Brush crash at startup was resolved by enabling:
  - `CONFIG_NET=y`
  - `CONFIG_UNIX=y`
- Minimal identity files were added under `rootfs/skeleton/etc`:
  - `passwd`, `group`, `hostname`, `os-release`
