# LinuxScripts

Personal Linux automation scripts for:

- Linux workstation bootstrap (`SetupLinux.sh`)
- Container app lifecycle management (`miniscripts/server/ContainerManager.sh`)
- KDE profile workflows (konsave import/export + GitHub Releases sync)
- OneDrive/rclone sync and desktop utility setup

Most scripts are written for Ubuntu/Debian-style systems (`apt`, `dpkg`, `sudo`).

## Repository Layout

```text
.
├── SetupLinux.sh
├── miniscripts/
│   ├── setuplinux/      # Linux setup scripts (auto-discovered)
│   ├── containers/      # Container lifecycle scripts
│   ├── server/          # Server management scripts
│   └── notautorun/      # Helper scripts called by others
├── Tools/
│   └── TailscaleList.sh
├── resources/
│   ├── script-order.txt
│   ├── variety.conf
│   ├── jellyfin/        # compose + env templates
│   └── homepage/        # Homepage templates
├── DevUtils/
├── GenericScripts/
└── KDEProfiles/
```

## Quick Start

### 1) Linux setup flow

```bash
sudo bash SetupLinux.sh
```

What it does:

1. Runs validation (`sudo`, `apt`, internet).
2. Discovers all `*.sh` in `miniscripts/setuplinux/`.
3. Prompts for script selection.
4. Applies ordering rules from `resources/script-order.txt`.
5. Runs one global `sudo apt update && sudo apt upgrade -y`.
6. Executes selected scripts.

### 2) Container setup flow

```bash
bash miniscripts/server/ContainerManager.sh
```

What it does:

1. Discovers all `*.sh` in `miniscripts/containers/`.
2. Prompts for each script.
3. Executes selected scripts in sorted order.

## Container Script Flags

All container scripts follow the same pattern:

- no flag: install/pull/build if needed and start
- `--on`: start existing install only
- `--off`: stop without deleting data
- `-D`: full cleanup (stop/remove container(s), image(s), and local data)

## Linux Setup Scripts (`miniscripts/setuplinux`)

- `InstallDefaultPackages.sh`  
	Installs default apt tools (includes `rclone`, `flatpak`, `pipx`, `fastfetch`, etc).
- `InstallDevUtils.sh`  
	Ensures `~/Documents/Repos`, installs dev packages (`cura`, `virt-manager`, `gh`), installs VS Code repo/package, installs IntelliJ via snap.
- `InstallFlatpakPackages.sh`  
	Adds Flathub and installs flatpak apps (Bottles, Flatseal, MissionCenter, Discord).
- `InstallGamePackages.sh`  
	Installs gaming packages (`steam`, `kmines`) and Basalt.
- `InstallOfficePackages.sh`  
	Installs LibreOffice.
- `InstallVariety.sh`  
	Installs Variety and copies `resources/variety.conf` to user config.
- `KonsaveSetup.sh`  
	Wrapper that installs konsave, downloads profiles from GitHub releases, then lets you apply one.
- `NVIDIADrivers.sh`  
	Detects current/recommended/latest NVIDIA packages and offers install/update interactively.
- `OneDriveRcloneSetup.sh`  
	Interactive `rclone config`, creates systemd mount service, performs initial bisync, and installs fixed cron sync entries.
- `RDSetup.sh`  
	Installs Tailscale, RustDesk latest `.deb`, and OpenSSH server.
- `RemoveUnwantedPackages.sh`  
	Removes selected KDE/app packages when installed.
- `DownloadGitRepos.sh`  
	Fetches all repos for a GitHub user via API, prompts per repo, then clones selected repos.

## Container Scripts (`miniscripts/containers`)

### `RunStableDiffusionContainer.sh`

- Deploys AUTOMATIC1111 in Docker (`automatic1111`)
- Data path: `~/.automatic1111`
- Port: `7861` on host
- Downloads DreamShaper 8 model if missing
- Builds local image (`automatic1111-webui`) with version label logic
- Uses GPU automatically when NVIDIA runtime is available

### `RunOllamaContainer.sh`

- Deploys Ollama + Open WebUI containers
- Network: `ai-stack`
- Data path: `~/.ollama-stack`
- Ports:
	- Open WebUI: `3000`
	- Ollama API: `11434`
- Ensures `dolphin-mistral:7b` model exists

### `RunHomepageContainer.sh`

- Deploys Homepage dashboard container
- Data path: `~/.homepage-dashboard`
- Port: `3001`
- Copies template configs from `resources/homepage/` on each run
- Requires active Tailscale for `run`/`--on`
- Rewrites `localhost/127.0.0.1/[::1]` URLs in service/widget/bookmark configs to active Tailscale IPv4
- Sets `HOMEPAGE_ALLOWED_HOSTS` dynamically and recreates container if allowlist changes

### `RunPortainerContainer.sh`

- Deploys Portainer CE
- Data path: `~/.portainer/data`
- Ports:
	- UI: `9443` (HTTPS)
	- Edge: `8000`

### `RunJellyfinStackContainer.sh`

- Deploys compose stack from `resources/jellyfin/docker-compose.yml`
- Stack root: `~/.jellyfin-stack`
- Prompts for:
	- media path (existing absolute path)
	- second library path (music)
	- downloads path
	- NordVPN service credentials + country
- Services:
	- Jellyfin (`8096`)
	- Radarr (`7878`)
	- Sonarr (`8989`)
	- Jackett (`9117`)
	- qBittorrent (`8080`, torrent port `6881`)
	- Gluetun (NordVPN tunnel + kill-switch)
	- FlareSolverr (runs in Jackett network namespace)
- Prints qBittorrent temporary password from logs when available

## KDE / Konsave Workflows

### Setup + apply profile

```bash
bash miniscripts/notautorun/ApplyKonsaveProfile.sh
```

- Installs konsave (via pipx if needed)
- Scans `KDEProfiles/*.knsv`
- Lets you choose `Do not apply`, default `HungLoStandard`, or other discovered profiles
- Imports profile file (if present) then applies profile by name

### Save current profile + optional upload to Releases

```bash
bash DevUtils/SaveKonsaveProfile.sh
```

- Saves active KDE config under chosen profile name
- Exports `.knsv` into `KDEProfiles/`
- Optionally syncs GitHub Releases via `miniscripts/notautorun/UploadKonsaveProfiles.sh`

### Download profiles from GitHub Releases

```bash
bash miniscripts/notautorun/DownloadKonsaveProfiles.sh
```

- Resolves repo owner/name from git origin
- Downloads all release assets to `KDEProfiles/`

## Resource Templates

- `resources/script-order.txt`  
	Optional execution priority rules for `SetupLinux.sh` (`[first]` and `[last]`).
- `resources/variety.conf`  
	Default Variety configuration copied by `InstallVariety.sh`.
- `resources/homepage/*.yaml`  
	Homepage config templates (`settings`, `services`, `widgets`, `bookmarks`, `docker`).
- `resources/jellyfin/docker-compose.yml` + `.env.example`  
	Media stack topology and variable template.

## Utility Scripts

- `DevUtils/BackupLinuxScripts.sh`  
	Zips repository and moves archive to a fixed OneDrive destination.
- `GenericScripts/GenericBackup.sh`  
	Generic zip backup template (requires you to set `DIR_TO_BACKUP` and `DIR_TO_BACKUP_TO`).

## Prerequisites & Notes

- Intended for Linux systems with `apt` and `systemd`.
- Most scripts expect `sudo` access.
- Container scripts auto-install Docker only during normal `run` flow.
- Some scripts are interactive by design (credentials, paths, yes/no prompts).
- For GitHub API-heavy scripts, set `GITHUB_TOKEN` to reduce rate-limit issues.

## Troubleshooting

- Docker daemon errors:
	- Ensure Docker is running.
	- Re-login after being added to `docker` group, or run with sudo.
- `--on` failing in container scripts:
	- The script expects prior install artifacts (image/data/config). Run without flags first.
- Homepage inaccessible from non-localhost hostnames:
	- Re-run `RunHomepageContainer.sh` so `HOMEPAGE_ALLOWED_HOSTS` is regenerated.
- qBittorrent internet/pathing confusion in media stack:
	- qBittorrent traffic is tunneled through Gluetun; other services stay on stack network.
- NordVPN login failures in Gluetun:
	- Use NordVPN **service credentials** (not account portal password).

