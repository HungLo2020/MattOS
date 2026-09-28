# Upstream Workflows

```
cargo run -p mattos-build -- upstream status
cargo run -p mattos-build -- upstream sync --all
cargo run -p mattos-build -- upstream sync linux
cargo run -p mattos-build -- upstream sync systemd
cargo run -p mattos-build -- upstream import <new-component>
```

`import` is only for a newly added component whose destination is empty or
holds only scaffold placeholder files; it refuses an already-imported
destination such as `src/system/systemd`. `sync` updates an existing
component to the exact `revision` pinned in `upstream/sources.toml`.

See [Upstream synchronization model](upstream-sync.md) for conflict behavior
and metadata.

Use `python3 DevUtils/VendoredPackageStatus.py` for a read-only joined report of
each vendored component's upstream branch tip, MattOS source pin/provenance
state, locally built package versions, and hosted repository versions. Add
`--component NAME --verbose` for an individual package-family report.
