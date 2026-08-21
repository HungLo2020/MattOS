"""Import and apply local Konsave profile exports."""

from __future__ import annotations

from pathlib import Path

from konsave.command import resolve_konsave_command
from paths import profile_directory
from process import run_command


DEFAULT_PROFILE_NAME = "HungLoStandard"


def available_profiles(repository_root: Path) -> list[str]:
    """Return local profile names with the default profile first when present."""

    profiles_dir = profile_directory(repository_root)
    profile_names = {profile_path.stem for profile_path in profiles_dir.glob("*.knsv")}
    other_profiles = sorted(profile_names - {DEFAULT_PROFILE_NAME}, key=str.casefold)
    return ([DEFAULT_PROFILE_NAME] if DEFAULT_PROFILE_NAME in profile_names else []) + other_profiles


def choose_profile(repository_root: Path) -> str | None:
    """Present the legacy profile-selection menu and return None to skip apply."""

    profiles = available_profiles(repository_root)
    default_selection = 2 if DEFAULT_PROFILE_NAME in profiles else 1
    print("Available KDE profiles:")
    print("  1. Do not apply any profile")
    for index, name in enumerate(profiles, start=2):
        print(f"  {index}. {name}")
    while True:
        try:
            entered = input(f"Select profile number [{default_selection}]: ").strip()
        except EOFError:
            return None
        selected = default_selection if not entered else int(entered) if entered.isdigit() else 0
        if 1 <= selected <= len(profiles) + 1:
            return None if selected == 1 else profiles[selected - 2]
        print(f"Please enter a valid number between 1 and {len(profiles) + 1}.")


def apply_profile(repository_root: Path, profile_name: str) -> bool:
    """Import a local profile when available, then apply it by name.

    Returns True when a local export was imported before applying. A missing
    local export is allowed because Konsave may already have that profile
    installed from another source.
    """

    cleaned_name = profile_name.strip()
    if not cleaned_name:
        raise ValueError("Profile name cannot be empty.")

    profiles_dir = profile_directory(repository_root)
    profile_file = profiles_dir / f"{cleaned_name}.knsv"
    konsave = resolve_konsave_command()
    imported = profile_file.is_file()

    if imported:
        print(f"Importing profile from {profile_file}")
        run_command([*konsave, "-i", str(profile_file)])
    else:
        print(f"No local export found for '{cleaned_name}'; applying the installed profile by name.")

    print(f"Applying profile: {cleaned_name}")
    run_command([*konsave, "-a", cleaned_name])
    print(f"Done applying profile '{cleaned_name}'.")
    return imported