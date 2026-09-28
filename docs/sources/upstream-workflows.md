# Upstream Workflows

```
cargo run -p mattos-build -- upstream import --all
cargo run -p mattos-build -- upstream sync --all
cargo run -p mattos-build -- upstream sync linux
cargo run -p mattos-build -- upstream import systemd
cargo run -p mattos-build -- upstream sync systemd
```

See [Upstream synchronization model](upstream-sync.md) for conflict behavior
and metadata.

Use `python3 DevUtils/VendoredPackageStatus.py` for a read-only joined report of
each vendored component's upstream branch tip, MattOS source pin/provenance
state, locally built package versions, and hosted repository versions. Add
`--component NAME --verbose` for an individual package-family report.
