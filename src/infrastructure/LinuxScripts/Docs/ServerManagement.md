# Server Management

## MattOS Debian repository

The home-server repository is managed by
`Tools/ManageMattOSRepositoryServer.py`. It stores the persistent signed
repository under `/srv/storage/Storage/MattOSPackageRepo/` by default and exposes a small
authenticated API for the compatibility client in
`GenericScripts/ManageMattOSRepository.py`.

Initialize it once on the home server from the Server Manager's
`MattOS repository setup` capability:

```bash
sudo python3 Tools/ServerManager.py
```

Choose `MattOS repository setup`. It installs the required Debian, GPG, and
R2 client packages, creates `/srv/storage/Storage/MattOSPackageRepo/` without
deleting existing packages, installs/updates the systemd service, and enables
it. Bitwarden is used only when the server needs credentials or the signing
key; cached credentials are reused afterward.

The API binds to the server's Tailscale IPv4 address when Tailscale is
available. The installed service uses Tailscale membership as its access
boundary, so clients do not need a copied token or Bitwarden authentication.
The compatibility client defaults to:

```text
http://hunglosvr:8790
```

This means existing projects need no repository URL, token, or per-project
setting. They only need the current compatible
`ManageMattOSRepository.py` file and access to the tailnet.

The built-in service serves the public repository locally as `/repository/`.
Cloudflare R2 remains the public APT publication target, so normal APT users
continue using `https://packages.mattsherfey.com`.

The server manager supports `init`, `status`, `token`, `add`, `remove`,
`list`, `verify`, and `serve`. Every mutation is serialized and publishes a
new release directory atomically. The server owns the private signing key;
clients use `export-key` to retrieve the public key.

If a manual unit is ever needed, it should be equivalent to:

```ini
[Unit]
Description=MattOS repository API
After=network-online.target

[Service]
User=mattos-repo
WorkingDirectory=/opt/LinuxScripts
Environment=MATTOS_REPOSITORY_ROOT=/srv/storage/Storage/MattOSPackageRepo
Environment=MATTOS_REPOSITORY_TOKEN_FILE=/srv/storage/Storage/MattOSPackageRepo/api-token
Environment=MATTOS_REPOSITORY_PUBLIC_URL=https://packages.mattsherfey.com
ExecStart=/usr/bin/python3 /opt/LinuxScripts/Tools/ManageMattOSRepositoryServer.py serve --bind 100.x.y.z --port 8790
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

The setup command owns the service configuration and updates only its own
unit. It performs an initial synchronization from the existing Cloudflare R2
repository. Later package operations update the persistent local repository
and publish only changed R2 objects.

### Tailscale name

The client is already configured for your existing Tailscale machine name,
`hunglosvr`. Verify from another tailnet-connected client with:

```bash
ping hunglosvr
curl http://hunglosvr:8790/v1/status
```

MagicDNS must be enabled in the tailnet DNS settings. If the short name does
not resolve, use the server's full MagicDNS name (`hunglosvr.<tailnet>.ts.net`)
in `/etc/mattos-repository/client.conf`:

```bash
sudo install -d -m 0755 /etc/mattos-repository
printf '%s\n' 'SERVER_URL=http://hunglosvr.<tailnet>.ts.net:8790' | sudo tee /etc/mattos-repository/client.conf
```

If a different name is preferred, set `SERVER_URL=http://that-name:8790` in
`/etc/mattos-repository/client.conf`; this is one machine-wide setting, not a
per-project setting.

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
