# Tools

User-facing Python entry points. Run these from the repository root with the project virtual environment available.

## Setup

`Setup.py` is the primary package-management entry point. With no arguments, it displays detected host details, presents a numbered profile menu, prints the selected plan, and asks for confirmation before applying it:

```bash
python3 Tools/Setup.py
```

On Linux and MattOS, the interactive flow starts with a guarded preflight. It offers to create or repair the `matt` operator account, grant its sudo membership, and copy the repository to `~/Documents/Repos/LinuxScripts` under that account. Every system change requires its own affirmative answer; an existing checkout at that path is left in place without a relocation prompt. After an approved copy, Setup restarts from the operator-owned checkout as `matt`.

After a successful interactive apply on Linux with APT, it also offers persistent Tailscale SMB storage-mount setup. The service retries indefinitely when Tailscale or the server share is unavailable; see [../Docs/PackageManagement.md](../Docs/PackageManagement.md) for its defaults and credential behavior.

On Linux, the next optional Setup step opens `ServerManager.py`. It can also be run directly for server-only administration:

```bash
python3 Tools/ServerManager.py
```

Server capabilities include Btrfs snapshot management for `/srv/storage`, plus legacy-compatible Restic and ZIP backup managers. Restic configurations, generated helpers, and password files remain under `~/.config/restic-mattmc/`; ZIP configurations and helpers remain under `~/.config/zip-backup-manager/`. Their systemd timers run as the user who configures them. The Server Manager also provides MattOS repository setup, which initializes the signed local Debian repository and creates its API token; the repository API should then be run as a dedicated systemd service.

`ContainerManager.py` is also available directly and preserves the legacy queue-then-run lifecycle flow:

```bash
python3 Tools/ContainerManager.py
```

For automation or one workload at a time, the direct source launchers accept the legacy no-argument install/update mode, `--on`, `--off`, and `-D` flags:

```bash
python3 src/containers/run_homepage.py --on
python3 src/containers/run_jellyfin.py -D
```

The available workloads are Homepage, the Jellyfin media stack, Ollama plus Open WebUI, Portainer, and AUTOMATIC1111 Stable Diffusion. They retain their legacy container names, ports, image names, host data directories, prompts, and cleanup targets.

It also forwards non-interactive commands to the implementation in `src/packages/cli.py`:

```bash
python3 Tools/Setup.py profiles
python3 Tools/Setup.py plan complete-desktop
python3 Tools/Setup.py apply complete-desktop --yes
```

See [../Docs/PackageManagement.md](../Docs/PackageManagement.md) for profile schema, providers, and platform behavior.

## KDE Profiles

`save_konsave_profile.py` exports the active KDE configuration to `resources/KDEProfiles/`. It can optionally synchronize exports to GitHub Releases:

```bash
python3 Tools/save_konsave_profile.py --name MyProfile --no-upload
python3 Tools/save_konsave_profile.py --name MyProfile --upload
```

On Linux, the declarative `konsave` package installs Konsave through `pipx`, then offers to download published `.knsv` profiles from GitHub Releases and select one to import/apply. The save tool uses that package-managed Konsave executable; it does not install or update Konsave itself.
