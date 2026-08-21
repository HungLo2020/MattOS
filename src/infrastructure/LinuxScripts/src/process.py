"""Safe subprocess helpers for invoking existing platform tools."""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path
from typing import Sequence


def find_command(command: str) -> str | None:
    """Return an executable path when the command is available on PATH."""

    return shutil.which(command)


def command_is_available(command: str) -> bool:
    """Return whether a command is available on PATH."""

    return find_command(command) is not None


def require_command(command: str) -> str:
    """Return an executable path or raise an actionable error."""

    executable = find_command(command)
    if executable is None:
        raise RuntimeError(f"Required command was not found: {command}")
    return executable


def run_command(
    command: Sequence[str],
    *,
    cwd: Path | None = None,
    check: bool = True,
    capture_output: bool = False,
) -> subprocess.CompletedProcess[str]:
    """Run an external command without shell parsing or word splitting."""

    return subprocess.run(
        list(command),
        cwd=cwd,
        check=check,
        text=True,
        capture_output=capture_output,
    )