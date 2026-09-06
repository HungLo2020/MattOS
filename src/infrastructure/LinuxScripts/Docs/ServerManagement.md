# Server Management

## MattOS and MattPackages Debian repositories

One `mattos-repository.service` manages two independent signed APT archives:

| Selection | Local root | R2 bucket | Public URL | Suite / component |
| --- | --- | --- | --- | --- |
| `--repo mattos` | `/srv/storage/Storage/MattOSPackageRepo` | `matt-apt-repo` | `https://packages.mattsherfey.com` | `trixie / main` |
| `--repo mattpackages` | `/srv/storage/Storage/MattPackagesRepo` | `mattpackages-apt-repo` | `https://mattpackages.mattsherfey.com` | `stable / main` |

Both support `amd64` and architecture-independent (`all`) packages. MattPackages
uses the existing MattOS signing key, but separate local state and R2 credentials.
There are no package ownership rules: the explicit selector determines where a
package goes. Creating MattPackages does not copy, move, or remove any packages,
and does not enable an APT source on any machine.

### Provisioning and server setup

Before setting up MattPackages:

1. Create an empty R2 bucket named `mattpackages-apt-repo` and connect the custom
   domain `mattpackages.mattsherfey.com` to it in Cloudflare. Keep the existing
   MattOS bucket and domain intact. The publisher uses an existing bucket; it
   does not create buckets or DNS records.
2. Create R2 object read/write credentials for the new bucket. In Bitwarden,
   create a login named `MattPackages R2 Repository Publisher`, with the access
   key ID as username and secret access key as password. Add custom fields
   `R2_ENDPOINT`, `R2_BUCKET_NAME=mattpackages-apt-repo`, and
   `R2_PUBLIC_URL=https://mattpackages.mattsherfey.com`.
3. Keep the existing `MattOS Repository Signing Key` item and local private key.
   MattPackages reuses that key and never generates a replacement of its own.
   Bitwarden must be available/unlocked during initial provisioning if the key
   or R2 credentials are not already cached.
4. Update LinuxScripts on the server, then run:

   ```bash
   sudo python3 Tools/ManageMattOSRepositoryServer.py --repo mattpackages setup
   ```

Alternatively, run `python3 Tools/ServerManager.py`, choose **Debian repository
management**, select **MattPackages**, and select **setup**. The menu also offers
status, listing, verification, and publication for either selected repository.

Setup installs dependencies, initializes only the selected repository, saves both
configurations to `/etc/mattos-repository/server.json`, and updates the shared
service. It starts MattPackages as a signed empty archive. If its local archive
is absent but its bucket already contains repository files, setup refuses to
import or overwrite those files. Existing initialized archives are retained on
repeated setup. Keep a backup of local repository state for disaster recovery.

The existing MattOS root, bucket, public URL, package contents, and key remain in
use. If your existing server uses non-default paths or R2 settings, supply those
settings on the first setup so they are captured in the new configuration file.
Do not copy the MattOS R2 credential cache into MattPackages.

### Configuration and access

`server.json` persists repository metadata, paths, bucket destinations, credential
cache paths, and Bitwarden item names. It contains no credential values or private
key material. The service explicitly loads this file on restart. Server commands
accept `--config PATH` before the subcommand for an alternate configuration.

Environment overrides use `MATTOS_` or `MATTPACKAGES_` prefixes:
`REPOSITORY_ROOT`, `REPOSITORY_SUITE`, `REPOSITORY_COMPONENT`,
`REPOSITORY_ARCHITECTURES`, `REPOSITORY_PUBLIC_URL`, `R2_BUCKET`, `R2_ENDPOINT`,
`R2_ITEM`, and `R2_CREDENTIALS_FILE`. For example,
`MATTPACKAGES_R2_BUCKET=mattpackages-apt-repo`. Supply overrides when running
setup to persist them. The signing key and API token are shared and selected by
`MATTOS_REPOSITORY_PRIVATE_KEY_FILE`, `MATTOS_GPG_ITEM`, and
`MATTOS_REPOSITORY_TOKEN_FILE`.

R2 credentials are cached in each root's `r2-credentials.json`. Cached or
Bitwarden destinations must match the selected repository configuration;
mismatches fail before R2 requests. Set `MATTPACKAGES_R2_REFRESH_CREDENTIALS=1`
(or the MattOS equivalent) when running setup to refresh that repository's cache.
Roots cannot overlap, buckets must differ, and credential caches must be separate.

The installed service retains Tailscale-based access: it binds to the Tailscale
IPv4 address when available (otherwise loopback), uses port 8790, and allows
clients that can reach it to operate without a separate token. Keep the
management service private. `MATTOS_REPOSITORY_BIND`, `MATTOS_REPOSITORY_PORT`,
and `MATTOS_REPOSITORY_SERVICE_USER` customize service installation.

### Explicit client selection

The standalone helper still defaults to `http://hunglosvr:8790`:

```bash
python3 GenericScripts/ManageMattOSRepository.py --repo mattos list
python3 GenericScripts/ManageMattOSRepository.py --repo mattpackages list
python3 GenericScripts/ManageMattOSRepository.py --repo mattpackages status
python3 GenericScripts/ManageMattOSRepository.py --repo mattpackages --dry-run upload package.deb
python3 GenericScripts/ManageMattOSRepository.py --repo mattpackages upload package.deb
```

Every command requires `--repo mattos` or `--repo mattpackages`, including
`doctor`, `init`, `add`/`upload`, `remove`, `publish`, `list`, `status`, `verify`,
and key exports. Put global options before the subcommand. There is no default,
environment fallback, or automatic selection based on the host distro. Missing
or invalid selection exits with code 2 before configuration or network access.
Listing remains tab-separated `name`, `version`, `architecture` on stdout; the
selected repository is reported separately on stderr.

The transport uses `/v2/repos/mattos/...` or `/v2/repos/mattpackages/...`.
All old `/v1/...` requests are rejected with an instruction to update the helper
and pass the selector. Downloading or importing the updated helper alone is not
enough: downstream callers must pass the switch. This change deliberately does
not edit any downstream project or MattOS installer/source configuration.

A machine-wide transport override can be placed in
`/etc/mattos-repository/client.conf` as `SERVER_URL=http://hostname:8790`.
`MATTOS_REPOSITORY_SERVER_URL` overrides it for both repositories. Clients have
no R2, boto3, or Bitwarden dependency.

### Administration and publication

```bash
python3 Tools/ManageMattOSRepositoryServer.py --repo mattos status
python3 Tools/ManageMattOSRepositoryServer.py --repo mattpackages list
python3 Tools/ManageMattOSRepositoryServer.py --repo mattpackages verify
python3 Tools/ManageMattOSRepositoryServer.py --repo mattpackages publish
```

Server repository commands require the same explicit selector. The exception is
`serve`: it starts the shared service for both configurations and accepts no
repository selector. `setup` persists configuration; `init` initializes only the
selected archive without installing a service.

Uploads and removals publish immediately. `publish` verifies local state and
retries synchronization to R2; `verify` checks local reprepro consistency only.
A local commit can succeed before an R2 failure, so retry `publish` after fixing
a publication error. Each mutation holds its repository's local lock; R2 uses a
separate lock in each bucket. Public R2 uploads are incremental, not an atomic
multi-object transaction. Normal synchronization never imports remote packages
into an empty archive or restores the last package after explicit removal.

The local HTTP server exposes `/repositories/mattos/dists/...` and
`/repositories/mattpackages/dists/...` (and corresponding `pool/...` paths).
The old public `/repository/...` path remains a MattOS download alias; it is not
a management endpoint. Normal APT users download directly from the R2 domains.

Public key export is available through either selected helper:

```bash
python3 GenericScripts/ManageMattOSRepository.py --repo mattpackages export-key --output archive-key.asc
```

The new APT source will use `https://mattpackages.mattsherfey.com`, suite `stable`,
component `main`, and the shared key via `Signed-By`. Provisioning sources and
keys on clients or in MattOS is a separate task. The suite name does not promise
binary compatibility with every APT distribution.

Run the Linux-only server administration interface with:

```bash
python3 Tools/ServerManager.py
```

The Btrfs Snapshot Manager elevates independently because it manages Btrfs subvolumes. Container and Restic management intentionally run as the invoking user, preserving user-owned service data and configuration.

## Restic Backups

The Restic capability is a Python replacement for the legacy manager. It retains the legacy defaults, persisted configuration paths, command-line interface, helper names, and systemd unit names:

```text
Source:     /srv/storage/Storage/Sync/MattMC
Repository: /srv/storage/OneDrive/Apps/Games/Storage/MattMC/Restic
Config root: ~/.config/restic-mattmc
```

It supports multiple named configurations, setup/rerun, immediate backup, snapshot listing with approximate size, restore to `~/Downloads`, manual retention pruning, status display, and deletion of a config/service/timer. A configured job has a daily persistent systemd timer with up to a 30-minute random delay and retention of 7 daily, 4 weekly, 12 monthly, and 2 yearly snapshots.

Restic reads existing shell-style `.env` configuration files from the legacy manager and imports the older single `backup.env` configuration as `MattMC` when required. It operates on source and repository paths accessible from the machine running the manager; it does not remotely execute backups over SSH or Tailscale.

## ZIP Backups

The ZIP capability replaces the legacy ZIP backup manager and retains its configuration root, defaults, helper/unit names, menus, and command aliases:

```text
Source:      /srv/storage/Storage/Sync/MattMC
Destination: /srv/storage/OneDrive/Apps/Games/Storage/MattMC/AutoZipArchives
Config root: ~/.config/zip-backup-manager
```

Each run creates `prefix_YYYY-MM-DD_HH-MM-SS.zip`, validates it with `unzip -tqq`, and writes a `.sha256` sidecar when `sha256sum` is available. Retention preserves the newest archive across the newest 3 daily, 3 ISO-weekly, 3 monthly, and 2 yearly buckets. It removes only archives matching that exact managed naming pattern and removes their checksum sidecars alongside them.

The manager supports multiple configurations, setup/rerun, immediate archive creation, archive listing, manual pruning, unit triggering, status display, and safe configuration deletion. A job runs through a daily persistent systemd timer with up to 30 minutes of randomized delay as the user who created it. If a destination is shared by another configuration, deletion preserves the destination and its archives.

## Uptime Kuma

Server Manager can optionally run the legacy-compatible Uptime Kuma container workload. The direct launcher is:

```bash
python3 src/containers/run_uptime_kuma.py
```

It preserves the no-argument install/update/start behavior and `--on`, `--off`, and `-D` lifecycle flags. The container is named `uptime-kuma`, uses `louislam/uptime-kuma:latest`, persists data at `~/.uptime-kuma/data`, and serves the UI at `http://localhost:3002` by default. Set `UPTIME_KUMA_PORT` before running it to select another host port.
