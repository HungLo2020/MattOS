# MattOS releases and upgrades

MattOS release numbers use **YY.MM.revision**. The first release is **26.10.1**:
2026, October, revision 1. Revisions start at 1 each month and increase for each
published release that month (`26.10.2`); a new month starts at `26.11.1`.
Year and month have two digits; revision has no leading zero. Published release
identities and their package manifests are immutable.

## Release identity and build cost

`src/system/release/current.toml` selects the release. The `mattos-release`
package owns `/usr/lib/os-release`, the `/etc/os-release` symlink, and
`/usr/share/mattos/release.json`. `VERSION_ID`, `VERSION` and `PRETTY_NAME`
report the selected release. The manifest records exact native package versions
and the ordered migration scripts with their SHA-256 checksums.

The OS version is independent of component versions. Kernel, compiler, desktop
and library package versions continue to follow their pinned upstream versions
and packaging revisions. A release bump restages release metadata and affected
profile metapackages and installer branding; it does not change every package version or invalidate
upstream compilation caches. The upgrade binary's build inputs exclude the
release selector, manifests and migration scripts.

Freeze the package versions after component and packaging work is ready:

```text
cargo run -p mattos-build -- release-lock
python3 DevUtils/run_qemu.py --build-only
```

The lock is `src/system/release/manifests/<version>.json`. Building refuses a
lock that disagrees with current package versions. Regenerate an unpublished
lock after deliberately changing package revisions. For an already published
release, increment the OS revision and create a new lock instead of editing the
published release's manifest. Keep previous migration scripts unchanged.

For repository release preparation, use
`python3 DevUtils/PublishPackages.py --packages-only --no-upload`. Its canonical
Rust `prepare-packages` command checks/builds native producer stages, reuses
unchanged packages, and updates the local APT repository. Revision-preparation
rounds also stay in package-only mode. ISO assembly is a separate
`python3 DevUtils/run_qemu.py --build-only` step when preparing an installer
image. A version-only repository release therefore needs only changed metadata
packages and their exact dependents; existing compilation outputs remain usable.

## Upgrading an installed system

The upgrade tool uses the system's signed APT repositories. It downloads the
release package only on a MattOS installation (`ID=mattos`),
previews the complete transaction, and upgrades
installed native MattOS packages to the manifest's exact versions. Foreign and
optional third-party packages remain under their own package policy; profiles
and user data are preserved. New dependencies are resolved by APT. Automatic
removals and downgrades are refused, and existing APT automatic-package marks
are restored after installation.

```text
mattos-upgrade status
mattos-upgrade plan
sudo mattos-upgrade apply --yes
```

`plan` uses existing APT indexes and downloads metadata to a temporary directory;
it does not install packages or change system configuration. `apply` refreshes
APT indexes first. To select a particular published release, add
`--release 26.10.1` to either command. The release's versions must still be
available in the configured repository.

Unversioned installations first bootstrap the new tool from the hosted MattOS
repository:

```text
sudo apt-get update
sudo apt-get install mattos-upgrade
mattos-upgrade plan --release 26.10.1
sudo mattos-upgrade apply --release 26.10.1 --yes
```

The coordinator records the starting identity as `unversioned`. It updates the
existing installation; a reinstall is not required. Bootstrap commands become
usable after the new packages have been published. Building an ISO or pushing
Git does not publish them.

This is an APT upgrade, not an atomic filesystem switch. The journal at
`/var/lib/mattos/releases/upgrade.json` records packages, migrations and
completion; `state.json` records the original release and completed migration
checksums. After an interruption, resolve the reported APT/dpkg issue and rerun
the same `apply` command. Do not change release targets mid-upgrade. Existing
kernel packages remain available for boot fallback; whole-system rollback is
not provided by this workflow. Reboot after a completed upgrade.

## System-wide release migrations

Store root-owned, idempotent scripts under
`src/system/release/migrations/<release>/`. The release lock orders their IDs and
pins their bytes. The coordinator verifies every script before executing any,
runs pending scripts after APT succeeds, and records each success atomically.
Failures stop the upgrade; already completed scripts are skipped on retry.
Scripts must tolerate interruption between their last action and their success
record. Never change a completed migration ID's contents: add another migration.

Fresh installations run pending migrations through
`mattos-release-migrations.service` during boot. Direct APT users can run
`sudo mattos-upgrade migrate`; an interrupted coordinator package phase must be
resumed before migrations can run. Package-owned defaults and standard Debian
maintainer scripts remain preferable for ordinary package configuration.
Migrations are for coordinated changes that span packages or legacy installs,
and must preserve administrator and user configuration unless a change is
explicitly necessary and documented.

## Repository publication

Keep the existing `trixie` APT suite. It is the repository compatibility/layout
name, not the MattOS OS version; creating a new repository for every monthly
release is unnecessary. Existing immutable `.deb` files are reused.

The current repository offers the latest package versions. Older installations
upgrade directly to the latest tested release using its cumulative migrations;
intermediate releases do not need to remain downloadable. Selecting an older
target with `--release` requires retaining that target's complete package cohort
on the server. Historical target retention and snapshot rollback are outside
the initial repository policy.

[PublishPackages.py](../../DevUtils/PublishPackages.py) uploads new or changed
component packages first. It verifies local artifacts against the tested
inventory before uploading anything. Before uploading `mattos-release`, it checks that every
manifest package version is hosted with the tested inventory SHA-256. An upload
failure or missing package leaves the release undiscoverable. The release
package is uploaded last. An existing release manifest cannot be replaced by an
automatic packaging-revision bump; prepare a new OS revision instead.

Use `--no-upload` to prepare revision changes and a local build for validation,
and `--dry-run --no-build` to review publication. Real publication requires an
explicit request. Retain package versions needed by supported release manifests;
the current repository is not a snapshot rollback service. Separate testing and
stable channels can be added later without changing the release numbering or
package-version model.

## Full release workflow

Use the repository's [manage-release skill](../../.agents/skills/manage-release/SKILL.md)
for an end-to-end release, for example: "Use $manage-release to release all
local MattOS changes, push one commit, publish the packages, and give me the
existing-machine upgrade instructions."

The order is: choose or finish the next unpublished version, prepare package
revisions and the lock, validate the artifacts and upgrade path, commit all
local project changes together, push that commit, then publish the tested
artifacts. Required third-party replacements go before the native release;
`mattos-release` goes last. Final native publication uses
`python3 DevUtils/PublishPackages.py --packages-only --no-build --no-bump`
so it cannot silently change the committed packaging revisions. Verify the
public repository before giving upgrade commands for the published version.

A request to edit the skill, prepare a release, or leave changes uncommitted
does not run the full publication workflow. Updating installed machines remains
a separate action; the full release workflow supplies instructions for the user.

Validate both a fresh installation and an upgrade from an unversioned ISO before
calling the first release successful. See [installer upgrade tests](installer.md#upgrade-test),
[package publishing](../packaging/publishing.md), and the repository's
`.agents/skills/manage-release/SKILL.md` workflow.
