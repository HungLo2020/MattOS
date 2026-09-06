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
```

Selection also applies to `init`, `add`, `remove`, `publish`, `verify`, and both
key export commands. Omitting `--repo` fails clearly without performing an
operation. Old helpers using unqualified API requests are rejected by the server.
There is no package ownership filtering or automatic package migration.

The client defaults to `http://hunglosvr:8790`. The server retains Tailscale-based
access, so clients need no Bitwarden or R2 configuration and no separate token.
A transport override can be placed in `/etc/mattos-repository/client.conf` as
`SERVER_URL=http://hostname:8790`.

Run `python3 Tools/ServerManager.py` on the server and choose **Debian repository
management** to select and manage either archive. MattPackages starts empty,
uses a separate R2 bucket, and shares the existing MattOS signing key.
See [server provisioning and configuration](../Docs/ServerManagement.md).
