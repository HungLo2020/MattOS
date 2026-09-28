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
- a Git LFS hydration policy (`lfs_policy`) and its SHA-256
  (`lfs_policy_sha256`), or `none` for both. The values come from the
  component's entry in `upstream/sources.toml`; a declared policy must match
  its pinned checksum and the component's exact `revision`.

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
python3 DevUtils/audits/test_imported_source_immutability.py -- <command> [arguments...]
```

A command after `--` is required. The guard snapshots every configured component and separately inventories
ignored, untracked paths. It rejects source changes and newly generated ignored
artifacts in any authoritative vendored tree. Existing ignored paths form the
comparison baseline, while upstream files tracked by MattOS are never classified
as generated residue.

## Commands

Show configured and imported state:

```
cargo run -p mattos-build -- upstream status
```

Initial import of a new component (empty/scaffold destination only; a
destination containing anything other than the allowed placeholder files is
refused, so use `sync` for components that are already imported):

```
cargo run -p mattos-build -- upstream import --all
cargo run -p mattos-build -- upstream import linux
```

`import --all` therefore succeeds only when every configured destination is
still empty or scaffold-only, as in a fresh tree; with any component already
imported it stops with an `initial import refused` error. `sync` on a
component that has no state file but an empty/scaffold destination performs
the initial import.

Synchronize after deliberately changing a component's exact `revision`:

```
cargo run -p mattos-build -- upstream sync --all
cargo run -p mattos-build -- upstream sync linux
```

## Synchronization expectations

For Linux kernel fidelity, run synchronization in a Linux filesystem path (for example `~/src/MattOS` in WSL), not from `/mnt/c`.

## Safety and merge behavior

- No dirty-tree check: import and sync do not inspect the outer repository's
  Git status, so uncommitted changes elsewhere (including in other components)
  do not block them. The only overwrite protection is that initial import
  refuses a destination containing non-placeholder files. Commit or otherwise
  preserve local edits in a component before syncing it.
- Path safety: component paths are validated as repository-relative and cannot escape repo root.
- Update strategy: updates use a three-way Git merge between:
	- prior imported upstream commit,
	- current MattOS destination tree,
	- the new pinned `revision` from `upstream/sources.toml` (the importer runs
	  `git checkout --detach <revision>`; the branch name is never followed).
- Conflict behavior: if both MattOS and upstream changed the same content, conflict markers are written and sync exits non-zero.
- Metadata behavior: sync state is only advanced to the new upstream commit when merge finishes without conflicts.
- Projection behavior: synchronization reconstructs retained files from the
  pinned commit and reapplies source selection even when the commit is
  unchanged. Missing retained paths are restored, while stale paths excluded by
  policy are removed.
- Import fidelity: the importer materializes files from Git blobs, so
  upstream-tracked files are written even when the component's own `.gitignore`
  matches them. Import and sync never touch the outer repository's index; the
  only `git add` runs inside the temporary merge repository under
  `upstream/.tmp`. The reconstructed source and state file are left unstaged.
  Because build mirrors copy
  `git ls-files --cached --others --exclude-standard` (see below), a new
  upstream file matched by a component `.gitignore` is not copied into build
  mirrors until it is tracked; commit such files with `git add -f`.
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
python3 -B DevUtils/audits/test_vendored_source_provenance.py
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
