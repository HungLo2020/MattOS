# Generic Scripts

Standalone utilities that can be used independently of package profiles.

## Generic Backup

`GenericBackup.sh` creates a timestamped ZIP archive. Edit `DIR_TO_BACKUP` and `DIR_TO_BACKUP_TO` in the script before running it. It excludes `node_modules`, `*.tmp`, and `.git`, writes the archive to `/tmp`, then moves it to the configured destination.

## Debian Repository Manager

`ManageMattOSRepository.py` is the standalone client for the shared MattOS and
MattPackages server. It uploads existing `.deb` files; the server signs and
publishes each selected repository to its own R2 bucket.

Every command requires an explicit repository. Put global options before the
subcommand:

```bash
python3 GenericScripts/ManageMattOSRepository.py --repo mattos list
python3 GenericScripts/ManageMattOSRepository.py --repo mattpackages doctor
python3 GenericScripts/ManageMattOSRepository.py --repo mattpackages status
python3 GenericScripts/ManageMattOSRepository.py --repo mattpackages --dry-run upload package.deb
python3 GenericScripts/ManageMattOSRepository.py --repo mattpackages upload package.deb
python3 GenericScripts/ManageMattOSRepository.py --repo mattpackages upload --no-overwrites package.deb
```

Uploads replace an existing package with the same name, version, and
architecture by default. Pass `--no-overwrites` after `upload` or `add` to
reject that collision. Reusing a version does not cause APT to upgrade an
already installed package; increase the version for normal upgrades.

Selection also applies to `init`, `add`, `remove`, `publish`, `verify`, and both
key export commands. Omitting `--repo` fails clearly without performing an
operation. Old helpers using unqualified API requests are rejected by the server.
There is no package ownership filtering or automatic package migration.

The client defaults to `http://hunglosvr.tail30f889.ts.net:8790`. The server retains Tailscale-based
access, so clients need no Bitwarden or R2 configuration and no separate token.
A transport override can be placed in `/etc/mattos-repository/client.conf` as
`SERVER_URL=http://hostname:8790`.

Run `python3 Tools/ServerManager.py` on the server and choose **Debian repository
management** to select and manage either archive. MattPackages starts empty,
uses a separate R2 bucket, and shares the existing MattOS signing key.
See [server provisioning and configuration](../Docs/ServerManagement.md).

## Public GitHub backups

The GitHub backup manager lives in `src/server/github_backups.py` and runs on the
backup server. Edit the variables near the top of that module:

GitHub remains the repository-discovery and mirror orchestration layer, but it
delegates tar.zst creation, validation, checksums, and archive cleanup directly
to the generic Zip Backup Manager engine in `src/server/zip_backups.py`. Its
installer deploys both files together so the systemd job uses the same engine.

- `GITHUB_USER`: defaults to `HungLo2020`.
- `BACKUP_DESTINATION`: defaults to `/srv/storage/OneDrive/Apps/Programming`.
- `WORK_DIRECTORY`: local persistent mirrors/cache, separate from the archive destination.

It requires Linux, Python 3.10+, Git, GNU tar, and zstd. Installation additionally
requires systemd and sudo. It does not install dependencies or request GitHub
credentials. Run installation as the account that should own the backups:

```bash
python3 Tools/ServerManager.py
```

Choose **GitHub backup manager** to install or update the timer. This invokes the
manager with `--install` as the current user.

This installs a copy of the script and a system service/timer running as that user
at 00:00, 06:00, 12:00, and 18:00 in the server's timezone. A persistent timer can
catch up a missed activation. Installation enables the timer; it may therefore
start a catch-up job. For an exceptional manual one-pass run:

```bash
python3 src/server/github_backups.py --run
```

Rerun `--install` after editing settings or updating the source script. Setup stops
its existing timer/job before replacing its own installed copy and fixed unit
names, then reenables the timer. It never moves or removes archives at the old
destination. Subsequent runs only create/prune archives in the current destination.
Use one installation per server; the fixed service name is intentionally reused.

Example output:

```text
/srv/storage/OneDrive/Apps/Programming/HungLo2020/Markerup/backup_2026-09-07_12-00-00_UTC.tar.zst
/srv/storage/OneDrive/Apps/Programming/HungLo2020/Markerup/backup_2026-09-07_12-00-00_UTC.tar.zst.sha256
```

Each pass discovers all public repositories owned by the account, including forks
and archived repositories. Missing mirrors are cloned automatically; existing
mirrors fetch incrementally, including branch/tag deletions. No pushes occur.
Git configuration/credential helpers are disabled for these anonymous HTTPS fetches.
Repository IDs identify cached mirrors, so a rename does not require a fresh clone;
new archives use the current name, and archives in old named directories remain.
Repositories no longer publicly listed are left on disk and are not pruned.

Every successful repository pass creates a new, independently restorable archive,
including when unchanged. Contents are:

- `repository.git`: bare Git mirror, including history, branches, tags and tracked files.
- `metadata`: repository details, open/closed issues, issue comments, published release
  descriptions/asset metadata, and backup timestamp/scope in JSON.
- `release-assets`: uploaded release downloads, cached locally to avoid downloading
  unchanged assets on every pass. Previously cached assets are preserved as well.

LFS payloads and referenced submodule repositories are not fetched; their committed
pointers/references remain in Git. PR records returned by the issues API may appear,
but PR review threads, issue attachments hosted elsewhere, wikis, discussions,
Actions artifacts, and other GitHub web features are not exported. JSON is a readable
export, not a promise of automatic restoration of GitHub issues or release records.

Anonymous GitHub API limits are shared by the server's public IP. The job waits for
rate-limit resets and paginates all collections. Large accounts can take longer than
six hours; systemd and a file lock prevent overlapping jobs. Initial Git/asset downloads
can be large. Archives include full copies of release downloads, so compression may
not greatly reduce already compressed binaries. A failed repository does not prevent
others being attempted and does not trigger retention for the failed repository.

Retention uses UTC timestamps and keeps the newest successful archive unconditionally:

| Age | Retained representatives |
| --- | --- |
| First 24 hours | Every run |
| 1–7 days | 2 per day (12-hour slots) |
| 7 days–1 calendar month | 4 per ISO week (four equal slots) |
| 1 calendar month–1 year | 4 per calendar month (four equal slots) |
| 1–5 years | 4 per calendar year (quarters) |
| Older than 5 years | 2 per five-year period (30-month slots), indefinitely |

Archive filenames explicitly use UTC and readable date/time separators. Retention also
recognizes the earlier compact timestamp format.

Five-year periods are anchored at years divisible by five, such as 2020–2024.
The latest available archive in each applicable slot survives; incomplete periods
or missed runs can have fewer representatives. Archives age into coarser tiers and
are deleted in place, not moved or recompressed. Only exact managed archive names
and their checksum sidecars are pruned, after compression, integrity validation,
and destination checksum verification succeed. Archive creation stages locally,
then uses a `.tmp` destination file followed by a completed-file rename.

On HungLoSVR, `/srv/storage` is Samba-shared, but only `/srv/storage/OneDrive` is
inside the inspected OneDrive client's sync directory. The default destination
`/srv/storage/OneDrive/Apps/Programming` uses this local synced storage. The separate
`/home/matt/OneDrive` rclone mount is not necessary. Keep working mirrors off cloud
mounts. Remote cloud upload completion is managed by OneDrive, not verified by this
script.

To inspect the installed job:

```bash
systemctl status github-public-backups.timer
journalctl -u github-public-backups.service
```

To restore a Git working tree, extract an archive into a new empty directory and
clone its `repository.git` into another new directory. For example:

```bash
sha256sum -c backup_TIMESTAMP.tar.zst.sha256
tar --zstd -xf backup_TIMESTAMP.tar.zst -C /path/to/empty-restore-directory
git clone /path/to/empty-restore-directory/repository.git /path/to/restored-project
```
