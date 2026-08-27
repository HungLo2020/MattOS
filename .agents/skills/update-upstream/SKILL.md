---
name: update-upstream
description: Update a pinned MattOS upstream component safely, validate provenance and patches, then verify the result through the canonical run_qemu.py workflow.
---

# MattOS upstream component updates

Use this skill when updating a vendored upstream project or any first-class
MattOS component whose source is pinned in `upstream/sources.toml`. Do not use
the upstream importer for ordinary transitive Cargo crates, local MattOS-only
configuration, or package versions managed by another subsystem.

## Before synchronization

Work from the repository root. Inspect the current component and repository
state first:

```bash
cargo run -p mattos-build -- upstream status
git status --short --branch
```

Confirm the requested component name and inspect its entry in
`upstream/sources.toml`, its `upstream/state/<component>.toml`, any source
selection/omission/gitlink policies, and its output-only patch manifest. The
exact 40-hex `revision` is authoritative; branch and tag fields are descriptive.

The importer normally refuses a dirty tree. Never discard unrelated user
changes to satisfy that guard. If preserving the index is required, the
repository-supported `MATTOS_IMPORT_NO_INDEX=1` mode leaves synchronization
results unstaged, but it does not make an unsafe merge acceptable.

## Update one component

After deliberately selecting the new immutable upstream commit, synchronize
only the requested component:

```bash
cargo run -p mattos-build -- upstream sync <component>
```

Use `--all` only when the user explicitly requests a complete synchronization.
Synchronization performs a three-way merge between the previous imported
upstream commit, the current MattOS destination, and the new upstream head. It
reapplies declared projections and policies, updates the state record only on a
conflict-free result, and reports conflicts rather than silently overwriting
MattOS work.

Do not edit authoritative imported files to apply MattOS adaptations. If a
compatibility change is still needed, preserve or update the checksummed
`output-mirror-only` patch under `upstream/patches/<component>/` and its manifest
integration. If upstream now contains the adaptation, prove that before
removing the patch.

## Validation gate

After every successful source update, run all of these checks before declaring
the update complete:

```bash
python3 DevUtils/generate_source_overrides.py --check
python3 -m unittest discover -s DevUtils/audits -p 'test_*.py'
python3 -B DevUtils/audits/test_vendored_source_provenance.py
python3 DevUtils/run_qemu.py --build-only
```

`run_qemu.py --build-only` is mandatory: it exercises the real MattOS build
graph, source-ownership mirrors, packages, repository metadata, rootfs,
live-root, and ISO validation. Do not substitute a focused Cargo build or a
host-built artifact.

For updates affecting boot, services, native libraries, desktop components,
packaging, or runtime configuration, also boot the resulting ISO through the
canonical launcher:

```bash
python3 DevUtils/run_qemu.py --headless --no-build --no-install-disk
```

Use graphical `python3 DevUtils/run_qemu.py --no-build` as well when the change
affects COSMIC or other graphical behavior. Validate the specific changed
behavior in the guest; do not claim an installed-system result from a live
boot. Check for stale QEMU/build processes before launching and stop only this
checkout's validation process tree if an unexpected rebuild or runtime failure
appears.

## Completion report

Report the old and new revision, merge/conflict result, state/policy/patch
changes, provenance result, affected package/stages, exact `run_qemu.py`
commands and outcomes, ISO path, runtime results, remaining failures, and final
`git status`. Leave changes unstaged and uncommitted unless the user explicitly
requests otherwise.
