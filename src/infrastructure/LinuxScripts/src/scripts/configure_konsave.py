#!/usr/bin/env python3
"""Offer GitHub Release profile download and application after Konsave installs."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


SOURCE_DIRECTORY = Path(__file__).resolve().parents[1]
if str(SOURCE_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SOURCE_DIRECTORY))

from konsave.apply import apply_profile, choose_profile
from konsave.command import resolve_konsave_command
from konsave.releases import download_profiles
from paths import find_repository_root


def prompt_yes_no(question: str) -> bool:
    """Return True only for an explicit affirmative response."""

    try:
        return input(f"{question} [y/N]: ").strip().lower() in {"y", "yes"}
    except EOFError:
        return False


def main() -> int:
    """Retain the legacy optional download and profile-application workflow."""

    repository_root = find_repository_root(Path(__file__).parent)
    resolve_konsave_command()
    if prompt_yes_no("Download published KDE profiles from GitHub Releases now?"):
        download_profiles(repository_root)
    profile_name = choose_profile(repository_root)
    if profile_name is None:
        print("Skipping profile apply by user selection.")
        return 0
    apply_profile(repository_root, profile_name)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Error: Konsave setup failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error