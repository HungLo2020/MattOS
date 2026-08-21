# Resources

Declarative data consumed by `Tools/Setup.py`.

## Profiles

`profiles/*.toml` compose named package sets. Common packages belong in `[profile]`; operating-system-specific policy belongs in `[platforms.<os>]`.

```toml
[profile]
name = "example"
includes = ["base"]
required_packages = ["git"]
optional_packages = []

[platforms.linux]
required_packages = ["fastfetch"]
optional_packages = []
delete_packages = ["unwanted-package"]
```

`delete_packages` contains native provider identifiers, not logical package resource names. It is currently supported for guarded APT removal and runs after profile installations. See [../Docs/PackageManagement.md](../Docs/PackageManagement.md) for the complete schema.

Packages can declare Python `before` and `after` hooks for source setup, authentication, and service configuration. Tailscale uses these hooks to configure its APT source and complete `tailscale up`; RustDesk uses them to download its official `.deb` and configure unattended direct-IP access.

## Packages

`packages/*.toml` defines one logical package per file and maps it to supported platform/provider targets. Profiles refer to the logical package name, not a provider-specific identifier.

## KDE Profiles

`KDEProfiles/*.knsv` contains Konsave exports. The Linux `konsave` package post-install workflow can download published exports and select one to apply. Use `Tools/save_konsave_profile.py` to create or synchronize exports to GitHub Releases.

`homepage/` contains templates copied into `~/.homepage-dashboard/config/` on each Homepage workload run. `jellyfin/` contains the compose and environment templates copied into `~/.jellyfin-stack/` when creating the Jellyfin media stack.