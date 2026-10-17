---
name: manage-release
description: Prepare, validate, publish, or diagnose a MattOS YY.MM.revision release and upgrades, including adoption of unversioned installations. Use for coordinated OS releases, not ordinary upstream component updates.
---

# MattOS release management

Read `docs/system/releases.md`, `docs/packaging/publishing.md`, and the installer
upgrade-test notes before working. The first release is `26.10.1`. The selector
is `src/system/release/current.toml`; package versions are independent of it.

Inspect the worktree and the requested scope. A request to prepare a release
allows local implementation and validation, not Git commits, tags, publication,
or changes to a physical installed machine unless the user also requested them.
Honor authorization already given; do not request it again unnecessarily.

## Prepare and validate

1. Choose `YY.MM.revision`: two-digit year and month, revision starting at 1.
   Increment the revision within a month; start at 1 in a new month. Do not edit
   an already published release identity or manifest.
2. Finish source/package changes and their packaging revisions. For system-wide
   migration scripts, use a new immutable ID under
   `src/system/release/migrations/<release>/`, preserve administrator settings,
   and make each action safe to repeat after an interrupted run. Do not modify
   prior migration scripts. Ordinary defaults belong to their owning packages.
3. Run canonical `cargo run -p mattos-build -- release-lock` to freeze exact
   package versions and migration hashes. The release lock is checked in under
   `src/system/release/manifests/`. Regenerate only an unpublished lock when
   package revisions change. Avoid tying the global release version to every
   upstream package or build-stage input.
4. Use the `run-qemu` workflow for the full build, fresh installed boots, and an
   upgrade from the previous ISO. For the first release, use a retained
   unversioned ISO. Verify release identity, exact package versions, user data
   and configuration preservation, migration retry behavior, and a reboot.
   Run the coordinator's tests and publisher regression tests as well.
5. Compare cache behavior before/after a metadata-only bump. Unchanged upstream
   stages and their `.deb` payloads must remain reusable. Do not clean outputs
   or force unrelated rebuilds to prepare a release.

For repository-only preparation, use
`python3 DevUtils/PublishPackages.py --packages-only --no-upload`. The canonical
Rust `prepare-packages` path checks native producers and prepares packages and
the local repository, including revision-preparation rounds. Build an ISO
separately when preparing an installer image; keep the full build and fresh
installation tests for the first release.

## Publish when requested

Use `DevUtils/PublishPackages.py` and its current options. Review the dry run;
prepare automatic packaging revisions with `--no-upload`, regenerate the
unpublished lock as required, and retest the resulting artifacts. Published
release manifests cannot be changed through an automatic revision bump.

The publisher sends component packages first, verifies all locked versions and
SHA-256 values in the hosted index, and sends `mattos-release` last. Never bypass
that gate or switch the repository's `trixie` suite merely to change an OS
version. Retain package versions needed by supported releases. A Git push or ISO
export does not publish APT packages.

The initial repository tracks the latest release. Older installed systems can
upgrade directly to it because migrations are cumulative. Explicit older
targets require server-side retention of their entire cohort; do not promise
that the current latest-version repository offers historical targets.

For legacy systems, bootstrap `mattos-upgrade` from the hosted MattOS repository,
review `mattos-upgrade plan --release <version>`, then apply with explicit
acknowledgement. Do not automatically update a physical machine while preparing
a release. Interrupted upgrades resume the same target and journal; do not
clear state, force downgrades, remove unrelated packages, or promise atomic
rollback. Report preparation, testing, publication, and deployment separately.
