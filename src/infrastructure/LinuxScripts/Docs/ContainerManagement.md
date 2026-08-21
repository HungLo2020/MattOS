# Container Management

`Tools/ContainerManager.py` replaces the deprecated interactive container manager. It shows the current Docker container table, queues one action for each workload, and runs the queued actions only after all prompts have been answered:

```bash
python3 Tools/ContainerManager.py
```

The choices deliberately match the legacy manager:

- `-I`: run the normal install/update/start behavior with no launcher flags.
- `--on`: start an existing installation only.
- `--off`: stop containers without removing data.
- `--delete`: run the launcher's destructive `-D` cleanup action.
- `--skip` and `--end`: skip one or all remaining workload prompts.

Each workload also has a direct Python launcher under `src/containers/`. These preserve the legacy no-argument, `--on`, `--off`, and `-D` command-line interface.

| Workload | Direct launcher | Persistent host path |
| --- | --- | --- |
| Homepage | `run_homepage.py` | `~/.homepage-dashboard` |
| Jellyfin media stack | `run_jellyfin.py` | `~/.jellyfin-stack` |
| Ollama and Open WebUI | `run_ollama.py` | `~/.ollama-stack` |
| Portainer | `run_portainer.py` | `~/.portainer` |
| AUTOMATIC1111 | `run_stable_diffusion.py` | `~/.automatic1111` |

`-D` removes the workload's containers, data directories, and images where the legacy launcher did so. The Jellyfin stack uses Compose `down --remove-orphans` and removes its stack directory; Compose images remain managed by Docker.

The manager is an option in `Tools/ServerManager.py`, but it intentionally runs as the invoking user. That preserves the legacy paths under that user's home directory and avoids accidentally creating root-owned workload data. The Btrfs capability retains its independent `sudo` elevation.

Homepage copies the templates in `resources/homepage/` on every `run` or `--on`, rewrites loopback HTTP URLs to the active Tailscale IPv4 address, and recreates its container when `HOMEPAGE_ALLOWED_HOSTS` changes. The Jellyfin workload copies `resources/jellyfin/` when creating a stack, remembers media paths, and supports the existing ProtonVPN Bitwarden lookup and NordVPN-to-ProtonVPN migration.

The Stable Diffusion launcher retains the legacy permissive WebUI flags. It should only be exposed on trusted networks and used with model files from trusted sources.

Uptime Kuma remains a Server Manager capability rather than a regular Container Manager workload, matching its legacy droplet-only role. Its direct launcher is `src/containers/run_uptime_kuma.py`, with persistent data under `~/.uptime-kuma/data` and host port `3002` by default.