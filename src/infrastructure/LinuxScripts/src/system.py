"""Read-only helpers for detecting Linux system capabilities and sessions."""

from __future__ import annotations

import os
import shlex
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Callable, Mapping

from host import HostPlatform, detect_host
from process import find_command


class PackageManager(str, Enum):
    """Package managers supported by the initial LinuxScripts architecture."""

    APT = "apt"
    DNF = "dnf"
    PACMAN = "pacman"
    ZYPPER = "zypper"
    APK = "apk"


@dataclass(frozen=True)
class LinuxDistro:
    """Normalized Linux distribution metadata from os-release."""

    identifier: str
    name: str
    version_id: str | None
    like: tuple[str, ...]


_PACKAGE_MANAGER_COMMANDS: dict[PackageManager, str] = {
    PackageManager.APT: "apt-get",
    PackageManager.DNF: "dnf",
    PackageManager.PACMAN: "pacman",
    PackageManager.ZYPPER: "zypper",
    PackageManager.APK: "apk",
}

_DISTRO_PACKAGE_MANAGERS: dict[str, PackageManager] = {
    "alpine": PackageManager.APK,
    "arch": PackageManager.PACMAN,
    "debian": PackageManager.APT,
    "fedora": PackageManager.DNF,
    "manjaro": PackageManager.PACMAN,
    "mattos": PackageManager.APT,
    "opensuse": PackageManager.ZYPPER,
    "rhel": PackageManager.DNF,
    "suse": PackageManager.ZYPPER,
    "ubuntu": PackageManager.APT,
}

_DESKTOP_ENVIRONMENT_COMMANDS: dict[str, tuple[str, ...]] = {
    "budgie": ("budgie-desktop",),
    "cinnamon": ("cinnamon-session",),
    "gnome": ("gnome-shell",),
    "kde": ("startplasma-x11", "startplasma-wayland", "plasmashell"),
    "lxde": ("lxsession",),
    "lxqt": ("lxqt-session",),
    "mate": ("mate-session",),
    "sway": ("sway",),
    "xfce": ("xfce4-session",),
    "hyprland": ("Hyprland",),
}

_DESKTOP_ALIASES: dict[str, str] = {
    "budgie": "budgie",
    "cinnamon": "cinnamon",
    "gnome": "gnome",
    "hyprland": "hyprland",
    "kde": "kde",
    "kde-plasma": "kde",
    "plasma": "kde",
    "lxde": "lxde",
    "lxqt": "lxqt",
    "mate": "mate",
    "sway": "sway",
    "ubuntu": "gnome",
    "xfce": "xfce",
    "xfce4": "xfce",
}


def _parse_os_release(contents: str) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in contents.splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        parsed = shlex.split(value, posix=True)
        values[key] = parsed[0] if parsed else ""
    return values


def detect_linux_distro(os_release_path: Path = Path("/etc/os-release")) -> LinuxDistro | None:
    """Return Linux distribution metadata, or None when the host is not Linux."""

    if detect_host().system != "linux":
        return None
    if not os_release_path.is_file():
        return None

    values = _parse_os_release(os_release_path.read_text(encoding="utf-8"))
    identifier = values.get("ID", "unknown").lower()
    name = values.get("PRETTY_NAME") or values.get("NAME") or identifier
    like = tuple(value.lower() for value in values.get("ID_LIKE", "").split() if value)
    return LinuxDistro(identifier, name, values.get("VERSION_ID") or None, like)


def detect_package_platform(
    host: HostPlatform | None = None,
    distro: LinuxDistro | None = None,
) -> str:
    """Return the package-resource platform that applies to the current host."""

    host = host if host is not None else detect_host()
    if host.system != "linux":
        return host.system
    distro = distro if distro is not None else detect_linux_distro()
    return "mattos" if distro is not None and distro.identifier == "mattos" else "linux"


def detect_package_manager(
    distro: LinuxDistro | None = None,
    command_lookup: Callable[[str], str | None] = find_command,
) -> PackageManager | None:
    """Return the installed package manager preferred by the detected distro."""

    distro = distro if distro is not None else detect_linux_distro()
    if distro is None:
        return None

    candidate_ids = (distro.identifier, *distro.like)
    for identifier in candidate_ids:
        manager = _DISTRO_PACKAGE_MANAGERS.get(identifier)
        if manager is not None and command_lookup(_PACKAGE_MANAGER_COMMANDS[manager]) is not None:
            return manager

    for manager, command in _PACKAGE_MANAGER_COMMANDS.items():
        if command_lookup(command) is not None:
            return manager
    return None


def detect_active_desktop_environment(environment: Mapping[str, str] | None = None) -> str | None:
    """Return the normalized desktop environment active in the current session."""

    environment = os.environ if environment is None else environment
    values = (
        environment.get("XDG_CURRENT_DESKTOP", ""),
        environment.get("XDG_SESSION_DESKTOP", ""),
        environment.get("DESKTOP_SESSION", ""),
    )
    for value in values:
        for token in value.lower().replace(":", ";").split(";"):
            desktop = _DESKTOP_ALIASES.get(token.strip())
            if desktop is not None:
                return desktop
    return None


def detect_installed_desktop_environments(
    command_lookup: Callable[[str], str | None] = find_command,
) -> tuple[str, ...]:
    """Return desktop environments whose session commands are installed."""

    installed = [
        desktop
        for desktop, commands in _DESKTOP_ENVIRONMENT_COMMANDS.items()
        if any(command_lookup(command) is not None for command in commands)
    ]
    return tuple(installed)