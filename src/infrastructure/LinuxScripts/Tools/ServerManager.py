#!/usr/bin/env python3
"""Interactive server administration entry point for LinuxScripts."""

from __future__ import annotations

import os
import subprocess
import sys
from collections.abc import Callable
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIRECTORY = REPOSITORY_ROOT / "src"


def use_project_interpreter() -> None:
    """Re-execute with the project virtual environment when it exists."""

    venv_python = REPOSITORY_ROOT / (".venv/Scripts/python.exe" if os.name == "nt" else ".venv/bin/python")
    if venv_python.is_file() and Path(sys.executable).resolve() != venv_python.resolve():
        os.execv(str(venv_python), (str(venv_python), *sys.argv))


use_project_interpreter()
if str(SOURCE_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SOURCE_DIRECTORY))

from host import detect_host
from server.btrfs_snapshots import main as btrfs_main
from server.restic_backups import main as restic_main
from server.zip_backups import main as zip_backup_main
from server.mattos_repository import main as mattos_repository_main
from containers.run_uptime_kuma import main as uptime_kuma_main


def prompt_yes_no(question: str) -> bool:
    """Return True only for an explicit affirmative answer."""

    try:
        return input(f"{question} [y/N]: ").strip().lower() in {"y", "yes"}
    except EOFError:
        return False


def btrfs_snapshot_action() -> int:
    """Run the legacy-compatible Btrfs snapshot manager capability."""

    return btrfs_main([])


def container_manager_action() -> int:
    """Run container administration as the invoking user, not as root."""

    return subprocess.run((sys.executable, str(REPOSITORY_ROOT / "Tools" / "ContainerManager.py")), check=False).returncode


def restic_backup_action() -> int:
    """Run the legacy-compatible user-owned Restic backup manager."""

    return restic_main([])


def zip_backup_action() -> int:
    """Run the legacy-compatible user-owned ZIP backup manager."""

    return zip_backup_main([])


def uptime_kuma_action() -> int:
    """Run the legacy-compatible Uptime Kuma droplet workload."""

    return uptime_kuma_main([])


def mattos_repository_action() -> int:
    """Initialize the local MattOS repository and provision its API token."""

    return mattos_repository_main(["setup"])


def capabilities() -> tuple[tuple[str, str, Callable[[], int]], ...]:
    """Return modular server capabilities for the interactive menu."""

    return (
        ("Btrfs snapshot manager", "Manage snapshots under /srv/storage/snapshots", btrfs_snapshot_action),
        ("Container manager", "Queue Docker workload install, start, stop, or deletion actions", container_manager_action),
        ("Restic backup manager", "Configure, run, restore, and schedule local Restic backup jobs", restic_backup_action),
        ("ZIP backup manager", "Configure, archive, retain, and schedule local ZIP backup jobs", zip_backup_action),
        ("Uptime Kuma", "Install, start, stop, or remove the Uptime Kuma monitoring container", uptime_kuma_action),
        ("MattOS repository setup", "Initialize the local signed Debian repository and display its API token", mattos_repository_action),
    )


def main() -> int:
    """Run the extensible server-management terminal interface."""

    host = detect_host()
    if host.system != "linux":
        print("Server Manager currently supports Linux only.", file=sys.stderr)
        return 1
    choices = capabilities()
    while True:
        print("LinuxScripts Server Manager")
        print("=" * 27)
        for index, (name, description, _) in enumerate(choices, start=1):
            print(f"  {index}. {name} - {description}")
        print("  0. Exit")
        try:
            selected = input("Choose an option: ").strip()
        except EOFError:
            return 0
        if selected in {"", "0"}:
            return 0
        if not selected.isdigit() or not 1 <= int(selected) <= len(choices):
            print("Enter one of the listed option numbers.\n")
            continue
        name, _, action = choices[int(selected) - 1]
        if prompt_yes_no(f"Run {name}?"):
            try:
                action()
            except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
                print(f"{name} failed: {error}", file=sys.stderr)
        print()


if __name__ == "__main__":
    raise SystemExit(main())
