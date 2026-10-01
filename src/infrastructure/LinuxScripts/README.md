# LinuxScripts

Declarative workstation and server setup for Linux, MattOS, Windows, and macOS. Package profiles are defined in TOML; Python tooling plans the resulting changes before it applies them.

## Quick Start

Run the bootstrap for the current operating system to create `.venv` and install Python dependencies:

```bash
# Linux
./Bootstrap.sh

# macOS
./MacBootstrap.sh
```

```powershell
# Windows PowerShell
.\Bootstrap.ps1
```

Start the interactive setup interface to inspect the host, choose a profile, review its plan, and confirm application:

```bash
python3 Tools/Setup.py
```

On Linux APT hosts (including Debian, Kubuntu, and MattOS), interactive setup offers to enable
MattPackages before package selection. Press Enter to decline. On supported `amd64` machines,
accepting installs the bundled public signing key in `/etc/apt/keyrings/mattpackages-archive-keyring.asc`
and an enabled `/etc/apt/sources.list.d/mattpackages.sources` for
`https://mattpackages.mattsherfey.com`, suite `stable`, component `main`. It refreshes that repository
through APT, including signature verification, even if you subsequently skip package installation.
No Bitwarden or Tailscale access is needed. Other architectures are skipped.

Repeating setup preserves correct files and repairs its own missing or disabled configuration.
An existing MattPackages entry elsewhere, or a source file not managed by LinuxScripts at the
same path, produces an explanation instead of duplicate entries or overwriting that configuration.
If APT refresh fails, setup stops before package installation; the installed source and key remain
available for diagnosis and retry. This option does not change the MattOS repository or install
applications by itself. Command-line `profiles`, `plan`, and `apply` do not enroll the machine.

`Setup.py` also exposes non-interactive commands for automation:

```bash
python3 Tools/Setup.py profiles
python3 Tools/Setup.py plan desktop
python3 Tools/Setup.py apply desktop --yes
```

`plan` makes no system changes. The interactive flow and `apply` both require an explicit confirmation after displaying the plan; installation is not transactional, so a failed provider command may require manual recovery.

## Profiles

| Profile | Use |
| --- | --- |
| `base` | Cross-platform command-line essentials. |
| `utilities` | Shared command-line Linux utilities. |
| `gui-utilities` | Shared Linux graphical utilities and desktop cleanup. |
| `auth` | Bitwarden desktop application and CLI. |
| `desktop` | Base, utilities, graphical utilities, authentication, and desktop applications. |
| `coding` | Desktop development tools. |
| `gaming` | Desktop gaming software. |
| `office` | Office software where supported. |
| `complete-desktop` | Desktop, coding, gaming, and office profiles. |
| `server` | Base, command-line utilities, authentication, and server software without GUI utilities. |

## Layout

- [Tools/README.md](Tools/README.md): user-facing package, server, container, and KDE profile tools.
- [GenericScripts/README.md](GenericScripts/README.md): reusable backup and MattOS repository utilities.
- [DevUtils/README.md](DevUtils/README.md): repository-specific development utilities.
- [resources/README.md](resources/README.md): declarative package, profile, and KDE profile resources.
- [src/README.md](src/README.md): Python module layout.
- [Docs/README.md](Docs/README.md): detailed operational documentation.

## Development

Run the package-management tests after changing Python code or TOML resources:

```bash
python3 -m unittest discover -s tests -v
python3 -m compileall -q src Tools tests
```
