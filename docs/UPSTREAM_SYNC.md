# Upstream synchronization model

MattOS imports upstream projects as normal tracked files (no submodules).

## Metadata

Source definitions are stored in `upstream/sources.toml`. Every component has a
40-hex `revision`; branch and tag names are descriptive upstream refs, never the
authority used for checkout.

Synchronization state uses schema version 2 in
`upstream/state/<component>.toml` and records:

- upstream repository
- upstream branch
- imported commit
- import timestamp
- synchronization method
- destination path
- exact upstream Git tree object
- canonical imported-tree SHA-256 (paths, blob identities, modes, and symlinks;
  documented gitlinks are excluded from this physical-tree digest)
- an optional source-selection policy path and SHA-256. Selected components use
  a selected-tree digest over the declared projection rather than the complete
  upstream tree.
- intentional omission and gitlink/submodule policy
- an output-mirror-only MattOS patch manifest and its SHA-256, or the explicit
  value `none` for both fields

Gitlink replacements and exclusions are recorded in
`upstream/policies/gitlinks.toml`. Official release archives used only to supply
generated bootstrap inputs are pinned in
`upstream/policies/release-archives.toml`.

Source-selection policies under `upstream/policies/` declare reproducible
component projections. The Linux policy is limited to `arch/`: it retains
shared `arch/` root files and the declared architecture directories, with an
explicit inventory of safely omitted x86 32-bit implementation units. No other
Linux subsystem is pruned. Exact `crypto/Kconfig` leaves from otherwise excluded
architecture trees remain when global Linux Kconfig parsing requires them; no
implementation source from those architectures is retained by that exception.

MattOS-specific changes do not live in authoritative imported source trees.
Checksummed patch files and manifests live under `upstream/patches/` and the
builder applies them only after copying source into `out/build/*/source`.

Run commands that may invoke component build systems through the imported-source
hygiene guard:

```
python3 DevUtils/test_imported_source_immutability.py -- <command> [arguments...]
```

The guard snapshots every configured component and separately inventories
ignored, untracked paths. It rejects source changes and newly generated ignored
artifacts in any authoritative vendored tree. Existing ignored paths form the
comparison baseline, while upstream files tracked by MattOS are never classified
as generated residue.

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

Synchronize after deliberately changing a component's exact `revision`:

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
- Projection behavior: synchronization reconstructs retained files from the
  pinned commit and reapplies source selection even when the commit is
  unchanged. Missing retained paths are restored, while stale paths excluded by
  policy are removed.
- Import fidelity: the importer force-records only the selected source path and
  state record so upstream-tracked files remain present even when the component's
  own `.gitignore` matches them. This intentionally updates the outer index when
  an import/sync command succeeds; ordinary builds never update the index.
  Repository maintenance that must leave the index untouched can set
  `MATTOS_IMPORT_NO_INDEX=1`; the reconstructed source and state remain unstaged.
- File-type fidelity: regular executable modes and symlink objects are copied as
  upstream records them. Upstream gitlinks are never initialized as nested Git
  repositories; explicit policy selects separately pinned ordinary-file
  replacements or exclusions.
- Generated residue is not provenance. Build mirrors enumerate outer-Git tracked
  files plus non-ignored local inputs, so ignored generated output cannot replace
  or hide a pinned source input.

## Full fidelity audit

Run the network-backed audit from the repository root:

```
python3 -B DevUtils/test_vendored_source_provenance.py
```

It fetches each immutable commit (using declared identity-preserving verification
mirrors only if an authoritative server cannot serve the object) and compares all
paths, blob contents, executable modes, symlink targets, and gitlinks. It also
validates tree digests, patch checksums/applicability, release-archive pins, nested
Git directories, Git LFS pointers, escaping symlinks, and the protected
LinuxScripts publisher checksum. For selected components, declared omissions are
accepted, but missing retained paths and stale excluded paths both fail the audit.

## Recovery notes

- If synchronization is interrupted, metadata is not advanced to a false success state.
- If a sync reports conflicts, resolve the files in the imported tree and commit normally.
