# Generic Scripts

Standalone utilities that can be used independently of package profiles.

## Generic Backup

`GenericBackup.sh` creates a timestamped ZIP archive. Edit `DIR_TO_BACKUP` and `DIR_TO_BACKUP_TO` in the script before running it. It excludes `node_modules`, `*.tmp`, and `.git`, writes the archive to `/tmp`, then moves it to the configured destination.

## MattOS Repository Manager

`ManageMattOSRepository.py` is the compatibility client for the locally hosted,
signed MattOS Debian repository. It does not build `.deb` packages; it sends
packages to the home-server repository service, which runs `reprepro` and
publishes the repository.

```bash
python3 GenericScripts/ManageMattOSRepository.py doctor
python3 GenericScripts/ManageMattOSRepository.py status
python3 GenericScripts/ManageMattOSRepository.py upload /absolute/path/package.deb
```

The client defaults to the existing Tailscale MagicDNS name `hunglosvr` on port
8790, so projects do not need per-project configuration:

```bash
python3 GenericScripts/ManageMattOSRepository.py upload package.deb
```

No token or Bitwarden setup is needed on client machines: the installed server
uses Tailscale membership as the access boundary. A machine-wide URL override
can still be placed in `/etc/mattos-repository/client.conf`; the client has no
Cloudflare, R2, boto3, or Bitwarden dependency.

On the home server, initialize and run the separate server manager:

```bash
python3 Tools/ServerManager.py
```

Choose `MattOS repository setup`. It installs dependencies, ensures the
repository directory exists, updates the service safely, and enables it.
Keep the mutation API private behind Tailscale.

The server setup uses the existing Cloudflare R2 credentials and publishes
incremental changes to the existing R2 bucket; no tunnel is required.
