# Homelab Setup

This homelab is split across three roles: a home-server Debian package
repository, a DigitalOcean monitoring node, and a home server that runs the
primary application containers.

```mermaid
flowchart LR
    Clients[Linux and MattOS clients] --> Repo[Home-server HTTPS Debian repository]
    Repo --> Builder[Local reprepro repository manager]
    Discord[Discord server] <-->|Alerts and notifications| Kuma[Uptime Kuma on DigitalOcean]
    Kuma --> Home[Home server]
    Home --> Apps[Home server containers]
```

## Home-server Debian Repository

The MattOS and MattPackages Debian APT repositories are built and signed on the home server and
served over HTTPS at the configured repository URL.

```text
https://packages.mattsherfey.com
```

The compatibility client is
[../GenericScripts/ManageMattOSRepository.py](../GenericScripts/ManageMattOSRepository.py).
The home-server manager is
[../Tools/ManageMattOSRepositoryServer.py](../Tools/ManageMattOSRepositoryServer.py).
The server publishes the `trixie` suite, `main` component, and `amd64`
packages. It keeps the private signing key on the server and exposes only the
public key to clients.

The server's persistent `reprepro` state is the package source of truth. Each
operation builds a new release directory from the active release and switches
the `current` symlink only after indexes and signatures are complete. A file
lock serializes concurrent operations. Clients upload only the package being
added; they never download and rebuild the repository.

## DigitalOcean Monitoring Node

Uptime Kuma runs on a DigitalOcean droplet as the independent monitoring service. Keeping monitoring off the home server allows it to report when the home server or its services are unreachable.

The Uptime Kuma container is explicitly configured as:

| Setting | Value |
| --- | --- |
| Container name | `uptime-kuma` |
| Image | `louislam/uptime-kuma:latest` |
| Persistent data | `~/.uptime-kuma/data` |
| Default host port | `3002` |
| Container port | `3001` |
| Restart policy | `unless-stopped` |

The service is managed from `Tools/ServerManager.py` or directly with:

```bash
python3 src/containers/run_uptime_kuma.py
```

It retains the normal install/start behavior plus `--on`, `--off`, and `-D` lifecycle actions. Uptime Kuma communicates monitoring alerts and notifications to the Discord server. The Discord webhook and monitor definitions are managed in Uptime Kuma itself rather than stored in this repository.

## Home Server

The home server hosts the regular application workloads. `Tools/ContainerManager.py` is the main interactive entry point for the first five workloads; each also has a direct Python launcher under `src/containers/`.

### Standalone Containers

| Service | Container name | Default host port | Persistent data |
| --- | --- | ---: | --- |
| Homepage dashboard | `homepage` | `3001` | `~/.homepage-dashboard` |
| Portainer CE | `portainer` | `9443` HTTPS, `8000` Edge | `~/.portainer/data` |
| Ollama | `ollama` | `11434` | `~/.ollama-stack/ollama` |
| Open WebUI | `open-webui` | `3000` | `~/.ollama-stack/open-webui` |
| AUTOMATIC1111 Stable Diffusion WebUI | `automatic1111` | `7861` | `~/.automatic1111` |

Homepage requires an active Tailscale connection and rewrites configured loopback URLs to the active Tailscale IPv4 address. Portainer provides Docker administration. Ollama and Open WebUI form the local AI stack. AUTOMATIC1111 runs the Stable Diffusion WebUI and uses NVIDIA GPU passthrough when Docker and the host support it.

### Jellyfin Media Stack

The Jellyfin workload is a Compose stack rooted at `~/.jellyfin-stack`. Its services are:

| Service | Container name | Default host port | Role |
| --- | --- | ---: | --- |
| Jellyfin | `jellyfin` | `8096` | Media server |
| Radarr | `radarr` | `7878` | Movie library automation |
| Sonarr | `sonarr` | `8989` | Television library automation |
| Seerr | `seerr` | `5055` | Media request management |
| Jackett | `jackett` | `9117` | Indexer aggregation |
| qBittorrent | `qbittorrent` | `8080`, `6881` TCP/UDP | Download client |
| Gluetun | `gluetun` | Exposes qBittorrent ports | ProtonVPN network gateway and kill switch |
| FlareSolverr | `flaresolverr` | Shares Jackett network namespace | Browser-based challenge solving for Jackett |

qBittorrent uses Gluetun's network namespace, keeping torrent traffic inside the ProtonVPN tunnel. The other media services remain on the `jellyfin_stack_net` Docker network. Media, music, and download locations are selected during initial stack setup and stored in the stack configuration.

## Operational Boundaries

- The home server builds and serves the Debian package repository.
- The DigitalOcean droplet runs Uptime Kuma and relays monitoring events to Discord.
- The home server runs Homepage, Portainer, the Ollama/Open WebUI stack, AUTOMATIC1111, and the Jellyfin media stack.
- Uptime Kuma is deliberately outside the normal home-server Container Manager queue because its monitoring value depends on being independent from the services it monitors.

Repository management now requires `--repo mattos` or `--repo mattpackages`.
Both archives share one service and signing key, with separate local roots and
R2 buckets. See [ServerManagement.md](ServerManagement.md) for setup and the
intentional rejection of older publishing clients.
