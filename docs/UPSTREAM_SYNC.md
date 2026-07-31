# Upstream synchronization model

MattOS imports upstream projects as normal tracked files (no submodules).

## Metadata

Source definitions are stored in `upstream/sources.toml`.
Synchronization state is written to `upstream/state/<component>.toml` and records:

- upstream repository
- upstream branch
- imported commit
- import timestamp
- synchronization method
- destination path

## Synchronize

For Linux kernel fidelity, run synchronization in a Linux filesystem path (for example `~/src/MattOS` in WSL), not from `/mnt/c`.

Run:

cargo run -p mattos-build -- import --all --update

Or update one component:

cargo run -p mattos-build -- import --component linux --update

## Safety and merge behavior

- Path safety: component paths are validated as repository-relative and cannot escape repo root.
- Dirty-tree protection: sync aborts if the destination path has uncommitted changes.
- Update strategy: updates use a three-way Git merge between:
	- prior imported upstream commit,
	- current MattOS destination tree,
	- latest upstream branch head.
- Conflict behavior: if both MattOS and upstream changed the same content, conflict markers are written and sync exits non-zero.
- Metadata behavior: sync state is only advanced to the new upstream commit when merge finishes without conflicts.
