# Package Management

`Tools/Setup.py` is the package-management entry point for TOML profile resources. Without arguments, it presents an interactive terminal interface that shows detected host details, lets you select one profile, prints its plan, and requires confirmation before applying it.

## Commands

```bash
python3 Tools/Setup.py
python3 Tools/Setup.py profiles
python3 Tools/Setup.py plan complete-desktop
python3 Tools/Setup.py plan complete-desktop --platform windows
python3 Tools/Setup.py apply complete-desktop --yes
```

`plan` never changes the host. `apply` prints the same plan and requires `--yes` before it executes provider commands. The interactive interface also prints the plan and asks for an explicit confirmation before it runs provider commands.

On Linux with APT, interactive Setup always continues to offer the persistent Tailscale SMB storage mount after the package-profile step, even when package installation is skipped. It defaults to `//100.72.33.98/storage` at `/mnt/storage`, prompts for any changed values and the SMB password, and can retrieve the password from Bitwarden item `PCPassword` (override with `SMB_BITWARDEN_ITEM`). The generated root-owned systemd service waits for an online Tailscale node and retries every 20 seconds indefinitely when the share or network is unavailable. Setup retires the legacy shell helper at `/usr/local/sbin/storage-smb-mount.sh` and replaces the shared service unit without unmounting an active matching share. This option is not available through non-interactive `apply`, on MattOS, or on non-APT platforms.

On Linux, interactive Setup also offers the modular Server Manager after the storage step. It can be run directly with `python3 Tools/ServerManager.py`; it elevates with `sudo` when needed. The first server capability is the Btrfs Snapshot Manager, a replacement for the legacy `/srv/storage` workflow. It validates that `/srv/storage` is mounted as Btrfs, lists subvolumes, creates read-only snapshots by default below `/srv/storage/snapshots`, and requires an exact snapshot-name plus final confirmation before deletion.

`--platform` and `--package-manager` are available only on `plan` for safe cross-platform previews. `apply` always detects the local host so it cannot execute another platform's provider commands.

## Bootstrap

- Linux: `./Bootstrap.sh`
- macOS: `./MacBootstrap.sh`
- Windows PowerShell: `.\Bootstrap.ps1`

Each bootstrap script creates the project-local `.venv` and installs `requirements.txt`. `MacBootstrap.sh` asks you to install Homebrew from https://brew.sh when it is unavailable. `Bootstrap.ps1` uses Winget to install Python 3.12 when necessary, or tells you how to install Python manually if Winget is unavailable.

## Resources

- `resources/profiles/*.toml` defines named profiles.
- `resources/packages/*.toml` maps one logical package per file to platform and provider targets.

Profiles can depend on other profiles:

```toml
[profile]
name = "complete-desktop"
includes = ["desktop", "coding", "gaming", "office"]
required_packages = []
optional_packages = []

[platforms.linux]
required_packages = ["flatpak", "konsave"]
optional_packages = []
delete_packages = ["unwanted-desktop-package"]
```

Packages can have global dependencies, and each target can add dependencies that apply only to that platform/provider:

```toml
[package]
name = "discord"
depends_on = []

[targets.linux.flatpak]
id = "com.discordapp.Discord"
remote = "flathub"
depends_on = ["flatpak"]

[targets.windows.winget]
id = "Discord.Discord"
```

Each profile and platform table has `required_packages` and `optional_packages` arrays. A package cannot appear in both arrays within the same table. Platform tables can also declare `delete_packages`: native provider package identifiers that should be removed after all profile installations. The initial implementation supports guarded APT removals, skipping identifiers that are unavailable or not installed. The resolver expands profile includes, adds common packages and packages from the matching `[platforms.<os>]` table, selects a target for the requested platform, resolves package dependencies before dependents, removes duplicates, and rejects profile or package cycles. Platform-specific profile packages are not requested on other operating systems. Required packages with no compatible target fail the plan. Optional packages are listed as skipped; the resolver never substitutes an incompatible native Linux package manager.

Catalog and profile resources are strict: unknown top-level tables, fields, platforms, providers, and provider options fail during loading. This prevents a misspelled TOML field from silently changing an installation plan.

Profiles can run repository Python dependency scripts before their packages are installed:

```toml
[profile]
script_dependencies = ["hello_world.py"]
```

Platform sections can declare the same `script_dependencies` field when a setup
script should run only for that platform:

```toml
[platforms.mattos]
script_dependencies = ["configure_cosmic_wallpapers.py"]
```

Platform scripts are included in the plan only when that platform is selected.
They run during apply, before package-provider operations, using the invoking
user's project Python interpreter. They are suitable for configuring an
already-installed desktop or service and do not imply package installation.

Packages can run dependency scripts immediately before or after that individual package's provider operation:

```toml
[script_dependencies]
before = ["hello_world.py"]
after = []
```

Script paths are relative to `src/scripts/`. They are shown by `plan`, then run only during `apply --yes` with the project Python interpreter. Package installs without hooks remain batched; a package with hooks gets its own provider operation so its declared order is preserved.

`utilities` is a shared command-line profile included by both `desktop` and `server`. On Linux it installs Tailscale through its official APT source and enables `tailscaled`. It skips enrollment when the device is already connected; otherwise it asks for explicit confirmation before opening interactive `tailscale up` authentication. `gui-utilities` is Linux-only and supplies desktop applications, RustDesk unattended/direct-IP configuration, and cleanup of unwanted KDE desktop applications; it is included by `desktop`, but never by `server`. RustDesk obtains its permanent password from the `PCPassword` Bitwarden item by default, or prompts when unavailable; set `BITWARDEN_RUSTDESK_ITEM` to use another item. `base` currently removes unwanted legacy games on Linux, so that cleanup applies to every Linux profile stack. `auth` is also included by `desktop` and `server`, and installs Bitwarden plus the `bw` command-line client on every supported platform.

On Linux, `base` also installs `openssh-server` and enables/starts the `ssh` service after installation. The Linux-only `variety` package installs the bundled `resources/variety.conf` into the invoking user's `~/.config/variety/variety.conf` after APT installs Variety.

The Linux-only `konsave` package is installed by the declarative `pipx` provider. Its post-install Python workflow optionally downloads `.knsv` assets published in this repository's GitHub Releases, then presents the legacy-compatible profile menu: skip applying a profile, apply `HungLoStandard` by default when available, or choose another local profile. `Tools/save_konsave_profile.py` only saves/exports and optionally publishes profiles; it reuses the package-managed `konsave` executable and never invokes a legacy installer or force-reinstalls Konsave.

## MattOS

MattOS is detected when its `/etc/os-release` declares `ID=mattos`. It is an APT-based platform, but it does not inherit generic Linux profile sections or package targets. Every MattOS-specific profile package and package target must be declared explicitly.

```toml
[platforms.mattos]
required_packages = ["mattos-control-center"]
optional_packages = []
```

```toml
[targets.mattos.apt]
id = "mattos-control-center"
```

Declare only the provider targets that actually distribute a package. For example, a MattOS APT-only package needs only `[targets.mattos.apt]`; it must not include placeholder DNF, Pacman, or generic Linux targets. Likewise, `[platforms.linux]` entries do not apply on MattOS. If a required package is requested with an incompatible native package manager, planning fails clearly, for example: `Package 'basalt' is not available for the dnf package manager on mattos.`

Use `python3 Tools/Setup.py plan complete-desktop --platform mattos` to preview a MattOS plan on another machine. A MattOS host selects this platform automatically.

## Providers

The initial providers are APT, DNF, Pacman, Zypper, APK, Flatpak, pipx, npm, Node.js/npm capability, shell installer, Snap, Winget, and Homebrew. Native Linux provider selection uses `src/system.py` to read the distro and available package manager.

On APT-based Linux systems, the logical `npm` package uses the Node.js/npm capability provider rather than always installing the distribution package named `npm`. It first reuses a working `node` and `npm` runtime. If installation is needed and APT selects a NodeSource `nodejs` candidate, it installs `nodejs`, which supplies its matching npm. Otherwise it installs the distribution `npm` package. This prevents mixing NodeSource Node.js with Ubuntu or Debian's incompatible standalone npm package.

The `shell_installer` provider supports packages published as a remote shell installer. Its target `id` is a direct HTTPS URL without embedded credentials. The plan displays the exact URL. During apply, Setup downloads it to a private temporary file and runs `sh` as the invoking user; it never pipes a download into a shell or adds `sudo`. An installer can request elevation itself when its own workflow requires it. Codex CLI uses this provider on Linux, MattOS, and macOS through its official installer; its Windows target remains npm-based.

Provider logic is responsible only for translating resolved targets into commands. `src/packages/executor.py` performs those commands after explicit CLI confirmation. Profiles never contain shell commands, installer URLs, or privilege logic.

## Python 3.10

Python 3.11 includes `tomllib`. On Python 3.10, Bootstrap installs the conditional `tomli` dependency from `requirements.txt` into the project virtual environment.
