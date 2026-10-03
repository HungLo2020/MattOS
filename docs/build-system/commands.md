# Build Commands

## Quick start

```
python3 DevUtils/setup.py                  # first-time machine setup
cargo run -p mattos-build -- doctor        # check prerequisites
cargo run -p mattos-build -- upstream status
cargo run -p mattos-build -- build         # build all components and the ISO
cargo run -p mattos-build -- run           # boot it in QEMU
```

The ISO is written to `out/images/mattos-x86_64.iso`. The development
launcher `python3 DevUtils/run_qemu.py` boots it with a QEMU user-mode
virtio-net interface by default (`--no-network` for an isolated boot).

## Individual stages

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

## Packages

```
cargo run -p mattos-build -- package build --all
cargo run -p mattos-build -- package repo
cargo run -p mattos-build -- package inspect mattos-brush
cargo run -p mattos-build -- package status
cargo run -p mattos-build -- package compatibility-audit
cargo run -p mattos-build -- package publish-plan out/packages/amd64/<package>.deb
```

## Cleanup

```
cargo run -p mattos-build -- clean artifacts
cargo run -p mattos-build -- clean logs
cargo run -p mattos-build -- clean cargo
cargo run -p mattos-build -- clean all
```

`clean` removes whole trees. To reclaim space without losing current build
outputs, use the `disk` commands:

```
cargo run -p mattos-build -- disk report
cargo run -p mattos-build -- disk prune --dry-run
cargo run -p mattos-build -- disk prune
```

`disk report` is read-only: it shows usage per `out/` area (and `target/`),
lists the ISO, upgrade baseline, QEMU disks and caches that prune keeps, and
sums what `disk prune` would reclaim. `disk prune` removes only leftovers that
no current recipe, stage or package uses:

- `out/build` directories whose name the build tool's source never mentions
  (outputs of removed or renamed recipes);
- `.<name>.building-<pid>` temporaries whose process no longer exists;
- pkg-config overlays (`out/build/.pkgconfig-overlays`), which builds
  regenerate on demand;
- `.deb` files `out/packages/inventory.toml` no longer references, and
  staging directories of packages that no longer exist;
- manifests of stages that no longer exist;
- `out/tmp` entries older than a day.

It refuses to run (except with `--dry-run`) while a `mattos-build` build,
package or image command, a third-party recipe build, a QEMU test or a
publication is running.
