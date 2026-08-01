# MattOS

MattOS is a Linux-compatible OS project with upstream source imported directly as ordinary tracked files in one repository.

## Repository model

- `src/kernel/linux`: upstream Linux kernel source
- `src/userland/brush`: upstream Brush shell source
- `src/userland/coreutils`: upstream uutils/coreutils source
- `src/system/systemd`: upstream systemd source
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

## Build stages

```
cargo run -p mattos-build -- build kernel
cargo run -p mattos-build -- build brush
cargo run -p mattos-build -- build coreutils
cargo run -p mattos-build -- build systemd
cargo run -p mattos-build -- build pam
cargo run -p mattos-build -- build util-linux
cargo run -p mattos-build -- build shadow
cargo run -p mattos-build -- build sudo-rs
cargo run -p mattos-build -- build init
cargo run -p mattos-build -- image
```

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
