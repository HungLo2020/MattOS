"""Resolve the Konsave command installed through the package system."""

from __future__ import annotations

import os
from pathlib import Path

from process import find_command


def resolve_konsave_command() -> list[str]:
    """Return the Konsave executable installed by the package system."""

    local_bin = Path.home() / ".local" / "bin"
    os.environ["PATH"] = f"{local_bin}{os.pathsep}{os.environ.get('PATH', '')}"

    executable = find_command("konsave")
    if executable is not None:
        return [executable]

    raise RuntimeError("Konsave is not installed. Apply a Linux desktop profile through Tools/Setup.py first.")