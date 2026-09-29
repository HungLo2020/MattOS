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
them to the publisher.

## What gets uploaded

The publisher replaces a package already present with the same name, version
and architecture, which constant test builds rely on. Uploading everything
would re-send hundreds of unchanged packages, so before uploading the script
reads the hosted repository's `Packages` index (the URL comes from
`src/system/packages/config/apt/mattos-hosted.sources`) and compares each
inventory package with it:

- not published, or newer than every published version: uploaded;
- published at the same version with a different SHA-256 (a rebuild, for
  example after a toolchain or packaging change): uploaded, replacing it;
- published at the same version with the same SHA-256: skipped. Package builds
  are reproducible, so equal hashes mean the package really is unchanged;
- older than a published version: refused, and nothing is uploaded. That is
  usually a stale checkout, and publishing it would not reach clients.

A replaced package keeps its version, so a system that already installed that
version keeps its old bytes until it reinstalls the package (`apt install
--reinstall`) or installs fresh; fresh test installs always get the new bytes. The build itself never publishes; see
[MattOS remote repository integration](remote-repository.md).
