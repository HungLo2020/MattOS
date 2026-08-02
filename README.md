# MattOS

MattOS is a Linux-compatible OS project with upstream source imported directly as ordinary tracked files in one repository.

## Project Rules / Vision / Goal

- Every executable, script, and runtime-loaded library installed in MattOS must be built from source as part of the MattOS build process.
- Build-only dependencies that are statically linked into a final artifact, used only during compilation, or fetched through a project’s normal dependency system do not need to become separate installed MattOS components or first-class MattOS packages.
- Every installed file must have a clear source, build path, and package owner. Host binaries and runtime libraries may be used only as explicitly documented temporary bootstrap dependencies.
- MattOS should eventually be fully self-hosting: a running MattOS system must contain the compilers, linkers, interpreters, package tools, and other development utilities required to rebuild MattOS and generate its packages, repository, and bootable ISO.
- Self-hosting does not require a completely offline build. MattOS may download pinned source and build dependencies through normal systems such as Cargo or project build tools.
- Builds should also be possible from an already populated local dependency cache when network access is unavailable.
- Downloaded build dependencies that do not become separate runtime artifacts do not need to be individually installed or managed through APT.

## Repository model

- `src/kernel/linux`: upstream Linux kernel source
- `src/userland/brush`: upstream Brush shell source
- `src/userland/coreutils`: upstream uutils/coreutils source
- `src/system/systemd`: upstream systemd source
- `src/system/dbus/dbus-broker`: upstream dbus-broker source
- `src/system/packages/dpkg`: upstream dpkg source
- `src/system/packages/apt`: upstream APT source plus MattOS vendor policy
- `src/system/kmod`: upstream kmod source
- `src/system/terminal/ncurses`: upstream ncurses source
- `src/userland/procps-ng`: upstream procps-ng source
- `src/userland/iproute2`: upstream iproute2 source
- `src/userland/iputils`: upstream iputils source
- `src/userland/curl`: upstream curl source
- `src/system/network`: MattOS-owned network, resolver, time, NSS, and CA configuration
- `src/userland/init`: MattOS-owned Rust PID 1
- `src/tools/mattos-build`: MattOS-owned Rust orchestrator

No Git submodules are used.

## Native Linux quick start

1. First-time machine setup:

```
python3 DevUtils/setup.py
```

2. Check prerequisites:

```
cargo run -p mattos-build -- doctor
```

3. Inspect imported upstream state:

```
cargo run -p mattos-build -- upstream status
```

4. Build all components and ISO:

```
cargo run -p mattos-build -- build
```

This includes a minimal systemd build in `out/build/systemd/`.

5. Run in QEMU:

```
cargo run -p mattos-build -- run
```

Or use the development launcher:

```
python3 DevUtils/setup.py --check
python3 DevUtils/run_qemu.py
```

Expected ISO artifact:

```
out/images/mattos-x86_64.iso
```

## Upstream workflows

```
cargo run -p mattos-build -- upstream import --all
cargo run -p mattos-build -- upstream sync --all
cargo run -p mattos-build -- upstream sync linux
cargo run -p mattos-build -- upstream import systemd
cargo run -p mattos-build -- upstream sync systemd
```

See `docs/UPSTREAM_SYNC.md` for conflict behavior and metadata.

See `docs/AUTHENTICATION.md` for the PAM, account, login, su, and sudo-rs architecture.

See `docs/BASE_ADMINISTRATION.md` for kmod, procps-ng, ncurses, terminfo, and kernel-module status.

See `docs/NETWORKING.md` for the wired/QEMU IPv4, DNS, time-sync, HTTPS, and CA-certificate architecture.

See `docs/DBUS.md` for the dbus-broker system bus, service policy, activation aliases, and non-root client behavior.

See `docs/SESSIONS.md` for pam_systemd, logind sessions, runtime directories, per-user systemd managers, and user D-Bus.

See `docs/PACKAGING.md` for `.deb` construction, the local MattOS APT repository, imported dpkg/APT builds, and hybrid rootfs assembly.

## Build stages

```
cargo run -p mattos-build -- build kernel
cargo run -p mattos-build -- build brush
cargo run -p mattos-build -- build coreutils
cargo run -p mattos-build -- build kmod
cargo run -p mattos-build -- build ncurses
cargo run -p mattos-build -- build procps
cargo run -p mattos-build -- build iproute2
cargo run -p mattos-build -- build iputils
cargo run -p mattos-build -- build curl
cargo run -p mattos-build -- build systemd
cargo run -p mattos-build -- build dbus-broker
cargo run -p mattos-build -- build dpkg
cargo run -p mattos-build -- build apt
cargo run -p mattos-build -- build pam
cargo run -p mattos-build -- build util-linux
cargo run -p mattos-build -- build shadow
cargo run -p mattos-build -- build sudo-rs
cargo run -p mattos-build -- build init
cargo run -p mattos-build -- image
```

## Package prototype

```
cargo run -p mattos-build -- package build --all
cargo run -p mattos-build -- package repo
cargo run -p mattos-build -- package inspect mattos-brush
cargo run -p mattos-build -- package status
```

The development launcher gives the guest a QEMU user-mode virtio-net interface by default. Use `python3 DevUtils/run_qemu.py --no-network` for an isolated boot.

## Cleanup

```
cargo run -p mattos-build -- clean artifacts
cargo run -p mattos-build -- clean logs
cargo run -p mattos-build -- clean cargo
cargo run -p mattos-build -- clean all
```

## Source layout

Project-managed source trees live under `src/`:

- `src/kernel/`
- `src/userland/`
- `src/boot/`
- `src/rootfs/`
- `src/tools/`
