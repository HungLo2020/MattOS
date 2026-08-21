"""Load and validate declarative package and profile TOML resources."""

from __future__ import annotations

from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from packages.models import PackageDefinition, PackageTarget, ProfileDefinition, ProfilePackage, ScriptDependencies
from toml_reader import load_toml


_PLATFORMS = {"linux", "mattos", "windows", "macos", "all"}
_PROFILE_PLATFORMS = _PLATFORMS - {"all"}
_PROVIDERS = {"apt", "apt_deb", "dnf", "pacman", "zypper", "apk", "flatpak", "pipx", "npm", "nodejs", "shell_installer", "snap", "winget", "homebrew"}
_PROVIDER_OPTIONS = {
    "flatpak": {"remote"},
    "homebrew": {"kind"},
}


def _validate_keys(value: dict[str, Any], allowed: set[str], label: str) -> None:
    unexpected = sorted(set(value).difference(allowed))
    if unexpected:
        raise ValueError(f"{label} has unsupported fields: {', '.join(unexpected)}")


def _string_list(value: Any, label: str) -> tuple[str, ...]:
    if not isinstance(value, list) or not all(isinstance(item, str) and item for item in value):
        raise ValueError(f"{label} must be a list of non-empty strings.")
    return tuple(value)


def _script_list(value: Any, label: str) -> tuple[str, ...]:
    scripts = _string_list(value, label)
    for script in scripts:
        path = Path(script)
        if path.is_absolute() or ".." in path.parts or path.suffix != ".py":
            raise ValueError(f"{label} must contain Python paths relative to src/scripts.")
    return scripts


def _package_script_dependencies(value: Any, label: str) -> ScriptDependencies:
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be a table.")
    _validate_keys(value, {"before", "after"}, label)
    return ScriptDependencies(
        _script_list(value.get("before", []), f"{label} before"),
        _script_list(value.get("after", []), f"{label} after"),
    )


def _validate_shell_installer_url(url: str, label: str) -> None:
    parsed = urlparse(url)
    if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password:
        raise ValueError(f"{label} must be an HTTPS URL without embedded credentials.")


def load_package(path: Path) -> PackageDefinition:
    """Load one logical package and all of its provider targets."""

    document = load_toml(path)
    _validate_keys(document, {"package", "script_dependencies", "targets"}, str(path))
    entry = document.get("package")
    if not isinstance(entry, dict):
        raise ValueError(f"{path} must contain a [package] table.")
    name = entry.get("name")
    description = entry.get("description", "")
    if not isinstance(name, str) or not name:
        raise ValueError(f"{path} package name must be a non-empty string.")
    if not isinstance(description, str):
        raise ValueError(f"Package '{name}' description must be a string.")
    _validate_keys(entry, {"name", "description", "depends_on"}, f"Package '{name}'")
    package_dependencies = _string_list(entry.get("depends_on", []), f"Package '{name}' depends_on")
    script_dependencies = _package_script_dependencies(
        document.get("script_dependencies", {}),
        f"Package '{name}' script_dependencies",
    )
    target_entries = document.get("targets", {})
    if not isinstance(target_entries, dict):
        raise ValueError(f"Package '{name}' targets must be a table.")

    targets: list[PackageTarget] = []
    for platform_name, providers in target_entries.items():
        if platform_name not in _PLATFORMS:
            raise ValueError(f"Package '{name}' has unsupported platform '{platform_name}'.")
        if not isinstance(providers, dict):
            raise ValueError(f"Package '{name}' target platform '{platform_name}' must be a table.")
        for provider_name, target in providers.items():
            if provider_name not in _PROVIDERS:
                raise ValueError(f"Package '{name}' has unsupported provider '{provider_name}'.")
            if not isinstance(target, dict) or not isinstance(target.get("id"), str) or not target["id"]:
                raise ValueError(f"Package '{name}' target '{platform_name}.{provider_name}' needs an id.")
            if provider_name == "shell_installer":
                _validate_shell_installer_url(target["id"], f"Package '{name}' target '{platform_name}.{provider_name}' id")
            allowed_target_keys = {"id", "depends_on", *_PROVIDER_OPTIONS.get(provider_name, set())}
            _validate_keys(target, allowed_target_keys, f"Package '{name}' target '{platform_name}.{provider_name}'")
            for option in _PROVIDER_OPTIONS.get(provider_name, set()):
                if option in target and not isinstance(target[option], str):
                    raise ValueError(f"Package '{name}' target '{platform_name}.{provider_name}' option '{option}' must be a string.")
            target_dependencies = _string_list(
                target.get("depends_on", []),
                f"Package '{name}' target '{platform_name}.{provider_name}' depends_on",
            )
            options = {
                key: value
                for key, value in target.items()
                if key not in {"id", "depends_on"} and isinstance(value, str)
            }
            targets.append(PackageTarget(platform_name, provider_name, target["id"], target_dependencies, options))

    if not targets:
        raise ValueError(f"Package '{name}' must define at least one target.")
    return PackageDefinition(name, description, package_dependencies, script_dependencies, tuple(targets))


def load_catalog(directory: Path) -> dict[str, PackageDefinition]:
    """Load package files recursively and reject duplicate logical names."""

    catalog: dict[str, PackageDefinition] = {}
    for path in sorted(directory.rglob("*.toml")):
        package = load_package(path)
        if package.name in catalog:
            raise ValueError(f"Duplicate package name: {package.name}")
        catalog[package.name] = package
    return catalog


def _profile_packages(value: Any, label: str) -> tuple[ProfilePackage, ...]:
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be a table.")
    _validate_keys(value, {"required_packages", "optional_packages"}, label)
    required = _string_list(value.get("required_packages", []), f"{label} required_packages")
    optional = _string_list(value.get("optional_packages", []), f"{label} optional_packages")
    duplicated = set(required).intersection(optional)
    if duplicated:
        names = ", ".join(sorted(duplicated))
        raise ValueError(f"{label} lists packages as both required and optional: {names}")
    return tuple(ProfilePackage(name, True) for name in required) + tuple(ProfilePackage(name, False) for name in optional)


def _platform_profile_data(
    value: Any,
    profile_name: str,
) -> tuple[dict[str, tuple[ProfilePackage, ...]], dict[str, tuple[str, ...]]]:
    if not isinstance(value, dict):
        raise ValueError(f"Profile '{profile_name}' platforms must be a table.")
    packages_by_platform: dict[str, tuple[ProfilePackage, ...]] = {}
    delete_packages_by_platform: dict[str, tuple[str, ...]] = {}
    for platform_name, platform_entry in value.items():
        if platform_name not in _PROFILE_PLATFORMS:
            raise ValueError(f"Profile '{profile_name}' has unsupported platform '{platform_name}'.")
        if not isinstance(platform_entry, dict):
            raise ValueError(f"Profile '{profile_name}' platform '{platform_name}' must be a table.")
        _validate_keys(
            platform_entry,
            {"required_packages", "optional_packages", "delete_packages"},
            f"Profile '{profile_name}' platform '{platform_name}'",
        )
        packages_by_platform[platform_name] = _profile_packages(
            {key: value for key, value in platform_entry.items() if key in {"required_packages", "optional_packages"}},
            f"Profile '{profile_name}' platform '{platform_name}'",
        )
        delete_packages_by_platform[platform_name] = _string_list(
            platform_entry.get("delete_packages", []),
            f"Profile '{profile_name}' platform '{platform_name}' delete_packages",
        )
    return packages_by_platform, delete_packages_by_platform


def load_profile(path: Path) -> ProfileDefinition:
    """Load one composable package profile."""

    document = load_toml(path)
    _validate_keys(document, {"profile", "platforms"}, str(path))
    profile = document.get("profile")
    if not isinstance(profile, dict):
        raise ValueError(f"{path} must contain a [profile] table.")
    name = profile.get("name")
    description = profile.get("description", "")
    if not isinstance(name, str) or not name:
        raise ValueError(f"{path} profile name must be a non-empty string.")
    if not isinstance(description, str):
        raise ValueError(f"{path} profile description must be a string.")
    _validate_keys(
        profile,
        {"name", "description", "includes", "required_packages", "optional_packages", "script_dependencies"},
        f"Profile '{name}'",
    )
    platform_packages, platform_delete_packages = _platform_profile_data(document.get("platforms", {}), name)
    return ProfileDefinition(
        name,
        description,
        _string_list(profile.get("includes", []), f"Profile '{name}' includes"),
        _profile_packages(
            {key: value for key, value in profile.items() if key in {"required_packages", "optional_packages"}},
            f"Profile '{name}'",
        ),
        platform_packages,
        _script_list(profile.get("script_dependencies", []), f"Profile '{name}' script_dependencies"),
        platform_delete_packages,
    )


def load_profiles(directory: Path) -> dict[str, ProfileDefinition]:
    """Load every TOML profile in a directory and reject duplicate names."""

    profiles: dict[str, ProfileDefinition] = {}
    for path in sorted(directory.glob("*.toml")):
        profile = load_profile(path)
        if profile.name in profiles:
            raise ValueError(f"Duplicate profile name: {profile.name}")
        profiles[profile.name] = profile
    return profiles