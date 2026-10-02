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
  `Dry run: upload N validated package(s)`, and uploads nothing. When packages
  would need a new revision (below), it lists them and changes nothing.
- `--no-bump`: refuse, instead of raising revisions, when a package changed
  under a published version.

The script lists the packages it discovered from the inventory before handing
them to the publisher.

## What gets uploaded

Uploading everything would re-send hundreds of unchanged packages, so before
uploading the script reads the hosted repository's `Packages` index (the URL
comes from `src/system/packages/config/apt/mattos-hosted.sources`) and
compares each inventory package with it:

- not published, or newer than every published version: uploaded;
- published at the same version with the same SHA-256: skipped. Package builds
  are reproducible, so equal hashes mean the package really is unchanged;
- published at the same version with a different SHA-256 (a packaging fix, a
  toolchain change, a dependency's new version): never uploaded as it is. A
  system that installed that version would never upgrade to different bytes
  under the same version, so the package gets a new version instead (below);
- older than a published version: refused, and nothing is uploaded. That is
  usually a stale checkout, or an uncommitted revision ledger, and publishing
  it would not reach clients.

## Packaging revisions

A MattOS package's version is `<upstream>-1mattos<N>`. N, the packaging
revision, comes from `src/system/packages/revisions.toml`: an entry
`name = { upstream = "...", revision = N }` applies while `upstream` is the
package's current upstream version (with any epoch), and a package without an
applicable entry is revision 1. A new upstream version therefore starts again
at revision 1; the build warns about entries an upstream update has made
obsolete, and rejects entries for unknown packages or with a revision below 2.

When packages changed under their published versions, the script raises the
revision of each of them, and of every published package that depends on one
of them with an exact version (MattOS pins ABI-coupled dependencies with
`(= version)`, so such a package's control data changes with the new
version). It records the new revisions in the ledger, rebuilds the packages
(only the affected packages are restaged; the ISO and local repository follow),
checks again, and uploads the new versions. This happens even with
`--no-build`. Commit the ledger with the change that caused the rebuild: a
checkout without it builds the old revisions, which the next publish refuses
as older than published. If packages still differ after three rounds, the
script stops, because their packaging is then probably not reproducible.

The build itself never publishes; see
[MattOS remote repository integration](remote-repository.md).

## Transient repository failures

Reading the index and uploading are retried when they fail for a reason that
can pass: the server unreachable (refused, reset or timed-out connections),
HTTP 5xx, 408 or 429. Each is retried ten times, ten seconds apart, before the
script fails. A rejection (bad credentials, an invalid package, any other HTTP
4xx) fails at once. Re-uploading a package with the same bytes is harmless, so
a failed batch is simply repeated. Third-party recipes follow the same policy
([Third-party packages](third-party-packages.md)).

The publisher reaches the server by its Tailscale name,
`http://hunglosvr.tail30f889.ts.net:8790`, because the service listens only
on the server's Tailscale address. The bare name `hunglosvr` is ambiguous: the
home router's DNS also answers it (`hunglosvr.home.local`, an address where
nothing listens on 8790). On 2026-10-01, while the router's IPv6 route was
flapping, that answer won and uploads were refused for about an hour although
the service never stopped; the publisher then still defaulted to the bare
name. A `SERVER_URL` in `~/.config/mattos-repository/client.conf` or
`MATTOS_REPOSITORY_SERVER_URL` overrides the default; keep it a fully
qualified name.
