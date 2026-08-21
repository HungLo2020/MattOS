# Source Layout

Shared Python implementation for the command-line tools.

| Path | Responsibility |
| --- | --- |
| `packages/` | Strict TOML loading, profile resolution, provider command planning, execution, and the Setup command implementation. |
| `storage_smb.py` | Linux/APT interactive setup and root-owned systemd helper for the persistent Tailscale CIFS mount. |
| `server/` | Modular Linux server-administration capabilities, beginning with Btrfs snapshots. |
| `containers/` | Shared Docker/Compose implementation for the public container workload launchers. |
| `konsave/` | Local Konsave profile import/apply and GitHub Release synchronization. |
| `scripts/` | Python dependencies invoked by package/profile hooks. |
| `host.py`, `system.py` | Host, distribution, platform, and package-manager detection. |
| `process.py` | Checked subprocess execution without shell parsing. |
| `github.py` | GitHub CLI and release-asset helpers. |
| `paths.py` | Repository and resource path discovery. |
| `toml_reader.py` | Python 3.10/3.11-compatible TOML loading. |

Keep source modules free of profile policy. Add package and platform policy under `resources/`, then cover behavior in `tests/`.