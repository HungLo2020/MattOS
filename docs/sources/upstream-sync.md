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
- the imported commit's committer time (`upstream_committed_at_utc`), which
  orders snapshot package versions
- import timestamp
- synchronization method
- destination path
- exact upstream Git tree object
- canonical imported-tree SHA-256 (paths, blob identities, modes, and symlinks;
  documented gitlinks are excluded from this physical-tree digest)
- an optional source-selection policy path and SHA-256, and an optional
  standalone intentional-omission policy. Selected components use a
  selected-tree digest over the declared projection rather than the complete
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
When adding or changing a patch, record its SHA-256 in the component's
`manifest.toml`, then record the manifest's new SHA-256 as
`patch_manifest_sha256` in both `upstream/sources.toml` and
`upstream/state/<component>.toml`; the build refuses a mismatch.

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

## Safety and update behavior

- Vendored trees are never edited in place: MattOS changes live in
  `upstream/patches/` and are applied to build mirrors. An update therefore
  needs no upstream history. It clones only the new pinned commit (depth 1),
  verifies the vendored tree against the `imported_tree_digest` recorded at
  its last import, and replaces the tree with the new commit's files.
- Verification treats three kinds of extra file as harmless and removes
  them: untracked, Git-ignored build residue that is not an upstream file;
  leftovers at paths the current source-selection or omission policy
  excludes; and nothing else. A changed, missing or added file, including a
  new untracked file that is not ignored, is a local modification: sync
  refuses, names the paths (from the prior commit, fetched alone), and
  leaves both the tree and its state untouched. Nothing is merged, so no
  conflict markers can reach an authoritative source tree. Restore the tree
  (or move the change into a patch) and retry.
- Upstream files that the outer repository ignores are still verified: the
  prior commit's file list tells them apart from residue.
- Components nested inside another component's path (for example
  `ostree/libglnx`, `ostree/bsdiff`, `glib/subprojects/gvdb`) belong to their
  own component: a parent sync neither verifies nor clears them.
- Because no shared history is needed, `branch` may move to another tag and
  `repo` to another repository (the Linux kernel moved from the mainline to
  the stable tree this way). Sync announces a repository change and records
  the new repository in state.
- No dirty-tree check applies to the rest of the repository: uncommitted
  changes elsewhere do not block a sync.
- Path safety: component paths are validated as repository-relative and
  cannot escape the repository root.
- Metadata behavior: state is only written after the new tree is in place.
  Re-synchronizing an unchanged commit keeps its `imported_at_utc`.
- Projection behavior: `source_selection_policy` (architecture pruning of
  `arch/`) and standalone `intentional_omission_policy` files (a list of
  `retained_paths`, or one `upstream_subtree` whose prefix is stripped) are
  applied when materializing the tree and when computing its digest, exactly
  as the provenance audit applies them. A retained path that no longer exists
  upstream is an error, so an upstream rename cannot silently shrink an
  import.
- Import fidelity: the importer materializes files from Git blobs, so
  upstream-tracked files are written even when the component's own `.gitignore`
  matches them. Import and sync never touch the outer repository's index.
  The reconstructed source and state file are left unstaged.
  Commit upstream files that a component `.gitignore` matches with
  `git add -f`; the tracking audit
  (`DevUtils/audits/test_vendored_source_tracking.py`) lists any that are
  still untracked.
- Nested ignore rules do not change what a build reads. Source digests and
  build mirrors take untracked files through MattOS's own ignore rules only
  (the root `.gitignore` and `.git/info/exclude`), never a vendored
  component's `.gitignore`. A newly imported upstream file therefore has the
  same digest and reaches the same mirror whether or not it has been
  force-added yet, and committing it does not rebuild anything. Likewise a
  deleted tracked file is simply absent from the digest, exactly as it is
  once the deletion is committed.
- Checkout completeness: `python3 DevUtils/audits/test_vendored_source_tracking.py`
  compares the Git index's per-component tree digest with each recorded import.
  This catches missing files even when no ignored copy remains on disk. Before
  committing a restoration, use `--worktree` to include unstaged and ignored
  upstream files without changing the index. The full provenance audit supports
  the same flag, while still verifying every file's upstream bytes. Its default
  requires all upstream paths to be tracked. Force-add upstream paths matched
  by nested ignores when preparing the eventual commit.
- Byte fidelity: upstream `.gitattributes` files are never imported. A
  nested one outranks the MattOS root `.gitattributes` and would let Git
  rewrite line endings (`text`, `eol`, `crlf`) or run filters on vendored
  files when they are committed or checked out, so a clone could hold bytes
  that differ from upstream's blobs. The provenance audit rejects attribute
  residue even when nested Git ignore rules hide it. The root `.gitattributes` sets
  `* -text`, so every file is stored and checked out byte for byte whatever
  `core.autocrlf` says. The importer's tree projection and the provenance
  audit both omit `.gitattributes`, so the recorded `imported_tree_digest`
  covers every other upstream file. Source digests detect modified files
  with no attributes at all (`git --attr-source=<empty tree>`), so no
  attribute can hide a byte change from the cache.
- File-type fidelity: regular executable modes and symlink objects are copied as
  upstream records them. Upstream gitlinks are never initialized as nested Git
  repositories; explicit policy selects separately pinned ordinary-file
  replacements or exclusions.
- Generated residue is not provenance. Build mirrors enumerate outer-Git tracked
  files plus non-ignored local inputs, so ignored generated output cannot replace
  or hide a pinned source input.

## Package versions from pins

A component whose `branch` is a release tag gives its packages that release
as their upstream version (`openssl-3.5.8` → `3.5.8`, `V_10_5_P1` → `10.5p1`,
`curl-8_22_0` → `8.22.0`, `2026d`, `master-2026-09-03` → `2026.09.03`); a
moving branch (`main`, `master`) gives a snapshot version,
`<declared>+git<YYYYMMDD>.<HHMMSS>.<commit>`
(`packaging/snapshot_version.rs`). `<declared>` is the version the tree itself
declares: meson.build's `project(version:)`, configure.ac's `AC_INIT` (with
simple `m4_define` macros resolved), or a component-specific file such as the
kernel Makefile, `gmp-h.in` or `NEWS`, with a pre-release marker such as
`-dev` or `-rc5` turned into `~dev` or `~rc5`. The date and time are the pinned
commit's committer time, which sync records in the component's state as
`upstream_committed_at_utc`. Snapshots therefore sort by upstream release
first and commit time within one, and every snapshot sorts after the earlier
`0~git.<commit>` form. Packaging refuses a snapshot whose state lacks the
commit time, naming the `upstream sync` that records it. Unit tests fail when a
release tag in `upstream/sources.toml` yields no version, or when a packaged
snapshot component's tree declares none this parser can read.

## Kernel and userland kernel headers

`linux` is the kernel MattOS boots (from the stable tree); `linux-uapi` is a
separately pinned, selected import of the same project that supplies only the
userland kernel headers (`linux-libc-dev`, glibc's `--with-headers`). As in
Debian, a kernel update rebuilds only the kernel, its modules, the initramfs
and the image; updating the userland headers is a deliberate, separate change
to `linux-uapi` that rebuilds the userland. `mattos_kernel_release!` in the
build tool names the kernel packages (`linux-modules-<release>`); a unit test
checks it against the vendored kernel Makefile and configuration and against
the package metadata files.

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

- If synchronization is interrupted, metadata is not advanced to a false success state;
  rerun the sync (an interrupted replacement leaves a tree that no longer
  matches its import only if files were already replaced, in which case
  restore the component directory from Git first).
- If a sync refuses a modified tree, restore the listed paths (or move the
  change into `upstream/patches/`) and rerun it.
