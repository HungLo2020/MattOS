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

## Commands

Show configured and imported state:

```
cargo run -p mattos-build -- upstream status
```

Initial import (empty/scaffold destination only):

```
cargo run -p mattos-build -- upstream import --all
cargo run -p mattos-build -- upstream import linux
```

Synchronize to latest upstream branch head:

```
cargo run -p mattos-build -- upstream sync --all
cargo run -p mattos-build -- upstream sync linux
```

## Synchronization expectations

For Linux kernel fidelity, run synchronization in a Linux filesystem path (for example `~/src/MattOS` in WSL), not from `/mnt/c`.

## Safety and merge behavior

- Dirty-tree protection: upstream import/sync aborts when the repository has uncommitted changes.
- Path safety: component paths are validated as repository-relative and cannot escape repo root.
- Update strategy: updates use a three-way Git merge between:
	- prior imported upstream commit,
	- current MattOS destination tree,
	- latest upstream branch head.
- Conflict behavior: if both MattOS and upstream changed the same content, conflict markers are written and sync exits non-zero.
- Metadata behavior: sync state is only advanced to the new upstream commit when merge finishes without conflicts.

## Recovery notes

- If synchronization is interrupted, metadata is not advanced to a false success state.
- If a sync reports conflicts, resolve the files in the imported tree and commit normally.
