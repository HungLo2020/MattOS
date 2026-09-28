# Publishing Packages

Build and upload every generated package through the vendored repository
manager:

```
python3 DevUtils/PublishPackages.py
```

Options (from the script's `argparse`):

- `--clean`: run `mattos-build clean artifacts` before the same `doctor` and
  `build all` path that `run_qemu.py` uses (it has no effect with `--no-build`).
- `--no-build`: skip the build and publish the existing artifacts recorded in
  `out/packages/inventory.toml`.
- `--dry-run`: still build (unless `--no-build` is also given) and still run
  the publisher, adding `--dry-run` to its `--non-interactive` invocation. The
  publisher validates the packages, prints
  `Dry run: upload N validated package(s)`, and uploads nothing.

The script lists the packages it discovered from the inventory before handing
them to the publisher. The build itself never publishes; see
[MattOS remote repository integration](remote-repository.md).
