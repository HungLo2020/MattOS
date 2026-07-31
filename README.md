# MattOS

MattOS is a Linux-compatible OS project with upstream source imported directly as ordinary tracked files in one repository.

## Repository model

- `kernel/linux`: upstream Linux kernel source
- `userland/brush`: upstream Brush shell source
- `userland/coreutils`: upstream uutils/coreutils source
- `userland/init`: MattOS-owned Rust PID 1
- `tools/mattos-build`: MattOS-owned Rust orchestrator

No Git submodules are used.

## Native Linux quick start

1. Check prerequisites:

```
cargo run -p mattos-build -- doctor
```

2. Inspect imported upstream state:

```
cargo run -p mattos-build -- upstream status
```

3. Build all components and ISO:

```
cargo run -p mattos-build -- build
```

4. Run in QEMU:

```
cargo run -p mattos-build -- run
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
```

See `docs/UPSTREAM_SYNC.md` for conflict behavior and metadata.

## Build stages

```
cargo run -p mattos-build -- build kernel
cargo run -p mattos-build -- build brush
cargo run -p mattos-build -- build coreutils
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
