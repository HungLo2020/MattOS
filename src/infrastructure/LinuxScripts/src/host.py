"""Small, dependency-free helpers for identifying the host platform."""

from __future__ import annotations

import platform
import sys
from dataclasses import dataclass


@dataclass(frozen=True)
class HostPlatform:
    """Normalized operating-system and CPU information for the current host."""

    system: str
    architecture: str
    machine: str


def detect_host() -> HostPlatform:
    """Return stable names suitable for profile and installer selection."""

    system_names = {
        "linux": "linux",
        "darwin": "macos",
        "windows": "windows",
    }
    architecture_names = {
        "amd64": "x86_64",
        "x86_64": "x86_64",
        "x86-64": "x86_64",
        "aarch64": "arm64",
        "arm64": "arm64",
        "armv8l": "arm64",
        "i386": "x86",
        "i686": "x86",
    }

    raw_system = sys.platform.lower()
    system = system_names.get(raw_system, raw_system)
    machine = platform.machine().lower()
    architecture = architecture_names.get(machine, machine or "unknown")
    return HostPlatform(system=system, architecture=architecture, machine=machine)