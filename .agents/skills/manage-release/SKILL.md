---
name: manage-release
description: Cut a MattOS YY.MM.revision release from local changes, including versioning, validation, one commit and push, package publication, and existing-machine upgrade instructions. Also supports preparation-only work and release diagnosis; ordinary upstream updates do not imply publication.
---

# MattOS release management

Read `docs/system/releases.md`, `docs/packaging/publishing.md`, and the installer
upgrade-test notes before working. The first release is `26.10.1`. The selector
is `src/system/release/current.toml`; package versions are independent of it.

## Scope

For a full release request (for example, "use $manage-release to release all
local changes"), complete the whole sequence below: prepare the version and
artifacts, validate, make and push one commit containing all local project
changes, publish the affected package repositories, verify publication, and
give the user upgrade instructions. Do not stop at a plan or ask again for
commit/push/publication permission already supplied by that request.

Creating or editing this skill does not execute a release. A preparation-only,
audit, diagnosis, or "leave uncommitted" request retains those limits even when
this skill is selected. Do not infer permission to deploy to physical machines,
tag, create a GitHub release, upload an ISO, or change other repositories such
as the separate LinuxScripts checkout. Publishing uses MattOS's existing
repository client; it does not modify that client's imported source.

Inspect the branch, remote/upstream, existing staged and unstaged changes,
untracked files, and unpushed commits. Include pre-existing local project work
when the user requests all local changes; do not silently commit only this
turn's edits. Preserve the previous release ISO before a build overwrites it.

## Prepare and validate

1. Choose `YY.MM.revision`: two-digit year and month, revision starting at 1.
   Increment the revision within a month; start at 1 in a new month. Do not edit
   an already published release identity or manifest. Check the hosted package
   index as well as local manifests: finish an already prepared, unpublished
   version rather than incrementing it a second time. Never reuse a published
   release number for a different cohort.
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

For changed third-party recipes or optional replacements required by the native
upgrade, read `docs/packaging/third-party-packages.md`. Prepare any required
`third-party-packages/releases.json` revision changes before the final commit,
build the selected versions without uploading, and retain the validated `.deb`
files and hashes. Test required replacements with `--upgrade-extra-package`.
Do not update unrelated upstream selections or rebuild the entire third-party
catalog merely because the builder image changed.

Reuse valid existing test results only when their inputs, inventory hashes,
release lock and artifacts still match. If preparation changes the cohort,
regenerate the unpublished lock and rerun the affected build/install/upgrade
checks. Never stage artifacts or clean caches to make a release bump work.

## Commit and push the tested release

Run the final publication preview before staging:

```text
python3 DevUtils/PublishPackages.py --packages-only --no-build --no-bump --dry-run
```

If the preview requires another revision, finish preparation and validation
first. Review the complete source diff, documentation, selector, revision
ledgers and lock together. For vendored changes, follow the source audits in
`docs/sources/upstream-sync.md`: worktree mode before staging, then the normal
index-based audits after staging. Upstream-tracked files hidden by an imported
`.gitignore` need selective `git add -f` after provenance verification; a plain
`git add -A` can miss them. Never force-add generated build output or credentials.

Stage all reviewed local project changes, inspect the staged diff and whitespace
checks, and create one coherent release commit. Push it to the intended branch
and verify the remote branch contains that commit. Preserve any pre-existing
unpushed commits; do not amend them or force-push. If the remote has diverged
and cannot accept the push, resolve that before publication without discarding
local work or rewriting remote history. Capture the commit SHA and confirm the
release worktree is clean before uploading.

## Publish the committed artifacts

Publish any required third-party dependencies/replacements to their configured
recipe repositories first, using the validated saved artifacts and the existing
repository client/framework. Inspect current command behavior: recipe `publish`
can rebuild and mutate revisions, so do not let it silently change the committed
selection or upload untested bytes. Skip unchanged third-party packages.

Publish native packages from the tested inventory with revision changes disabled:

```text
python3 DevUtils/PublishPackages.py --packages-only --no-build --no-bump
```

The publisher sends component packages first, verifies all locked versions and
SHA-256 values in the hosted index, and sends `mattos-release` last. Never bypass
that gate or switch the repository's `trixie` suite merely to change an OS
version. Retain package versions needed by supported releases. A Git push or ISO
export does not publish APT packages. After upload, fetch the public repository
index and verify the selected `mattos-release` version and SHA-256, plus the
locked cohort's versions and hashes, against the tested inventory. Verify
affected third-party package versions in their own indexes too. Check that
publication left the committed source and revision ledgers unchanged.

If a final upload now requires revision changes, do not remove `--no-bump` or
silently amend the release: return to preparation, retest the changed artifacts,
and report the need for another commit. If an upload partially fails, retain
the commit, logs and exact artifacts and retry that same publication using the
client's bounded retries. A persistent failure ends with an accurate partial
publication report; do not claim success, overwrite published bytes, or invent
a new release number merely to retry. `mattos-release` must remain last.

The initial repository tracks the latest release. Older installed systems can
upgrade directly to it because migrations are cumulative. Explicit older
targets require server-side retention of their entire cohort; do not promise
that the current latest-version repository offers historical targets.

## Give existing-machine upgrade instructions

After publication is verified, provide copyable commands with the actual
published release number substituted for `YY.MM.revision`. For an existing
versioned MattOS installation:

```sh
sudo apt-get update
mattos-upgrade status
mattos-upgrade plan --release YY.MM.revision
```

Have the user review the plan before running the apply and reboot commands:

```sh
sudo mattos-upgrade apply --release YY.MM.revision --yes
sudo reboot
```

For an unversioned install or one missing the coordinator, first bootstrap with
`sudo apt-get update` and
`sudo apt-get install mattos-upgrade`, then use the same plan/apply sequence.
Explain that no reinstall is required. After reboot, verify:

```sh
cat /etc/os-release
mattos-upgrade status
sudo dpkg --audit
```

Explain that an interrupted upgrade resumes with the same target and apply
command after fixing its reported APT/dpkg problem. Do not clear release state,
force downgrades, remove unrelated packages, or promise atomic rollback. If
publication is incomplete, identify what is missing instead of presenting
upgrade commands as ready to run.

Finish in chat with the release number, commit SHA/branch and push outcome,
validation results and material limitations, package repositories published and
verified, any remaining local changes, and the upgrade commands. Distinguish
publication from deployment; do not SSH into or upgrade user machines unless
separately requested.
