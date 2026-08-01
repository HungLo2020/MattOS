# Building MattOS

All builds for this milestone are Linux-native and must run on a case-sensitive filesystem.

## Prerequisites

For first-time setup on a native Linux host:

```
python3 DevUtils/setup.py
```

Read-only checks:

```
python3 DevUtils/setup.py --check
python3 DevUtils/setup.py --dry-run
```

Run:

```
cargo run -p mattos-build -- doctor
```

Required tools are reported separately from optional tools. Missing-tool package hints are printed for common Linux distributions.
`DevUtils/run_qemu.py` also runs `doctor` first and will direct you to `python3 DevUtils/setup.py` if required prerequisites are missing.

This milestone also requires the systemd, Autotools, and ELF-inspection toolchain declared by `DevUtils/setup.py`, including Meson/Ninja, Autoconf/Automake/libtool, `file`, `ldd`, and `readelf`.

## Upstream source status

```
cargo run -p mattos-build -- upstream status
```

Optional import/sync commands:

```
cargo run -p mattos-build -- upstream import --all
cargo run -p mattos-build -- upstream sync --all
cargo run -p mattos-build -- upstream import systemd
cargo run -p mattos-build -- upstream sync systemd
cargo run -p mattos-build -- upstream import kmod
cargo run -p mattos-build -- upstream import procps-ng
cargo run -p mattos-build -- upstream import ncurses
```

## Full build

```
cargo run -p mattos-build -- build
```

The pipeline stages are:

1. `kernel`: Linux kernel build using `src/kernel/config/x86_64_mattos.config`
2. `brush`: Brush release build
3. `coreutils`: uutils/coreutils multicall build
4. `ncurses`: terminal libraries, tools, and compiled terminfo database
5. `procps`: process-management tools linked to the local ncurses build
6. `pam`, `util-linux`, `shadow`, `sudo-rs`: existing authentication stack
7. `kmod`: module administration tools and libkmod
8. `systemd`: minimal Meson/Ninja build with local kmod integration
9. `init`: MattOS rescue init build
10. `rootfs`, `initramfs`, `iso`: image assembly

Systemd configuration is intentionally minimal for this boot milestone and disables optional subsystems including networkd, resolved, timesyncd, homed, nspawn, bootloader tools, remote journal stack, docs, tests, translations, TPM/FIDO, and BPF extras.

## Incremental builds

```
cargo run -p mattos-build -- build kernel
cargo run -p mattos-build -- build brush
cargo run -p mattos-build -- build coreutils
cargo run -p mattos-build -- build kmod
cargo run -p mattos-build -- build ncurses
cargo run -p mattos-build -- build procps
cargo run -p mattos-build -- build systemd
cargo run -p mattos-build -- build init
cargo run -p mattos-build -- image
```

`image` reassembles rootfs, initramfs, and ISO without forcing unrelated recompilation.

## QEMU boot

```
cargo run -p mattos-build -- run
```

Boot logs are written to `out/logs/qemu-boot.log`.

The default GRUB entry boots `init=/usr/lib/systemd/systemd systemd.unit=mattos.target`.
A rescue GRUB entry is also provided and boots MattOS Rust rescue init from `/usr/libexec/mattos/rescue-init`.

## Cleanup

```
cargo run -p mattos-build -- clean artifacts
cargo run -p mattos-build -- clean logs
cargo run -p mattos-build -- clean cargo
cargo run -p mattos-build -- clean all
```

Cleanup never deletes imported upstream source trees.
