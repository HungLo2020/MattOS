#!/usr/bin/env python3
"""Save the current KDE configuration as a repository Konsave profile."""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

SOURCE_DIRECTORY = Path(__file__).resolve().parents[1] / "src"
if str(SOURCE_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SOURCE_DIRECTORY))

from host import detect_host
from konsave.command import resolve_konsave_command
from konsave.releases import upload_profiles
from paths import find_repository_root, profile_directory
from process import run_command


DEFAULT_PROFILE_NAME = "HungLoStandard"
PROFILE_NAME_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")


def project_root() -> Path:
    """Resolve the project root from this module, independent of cwd."""

    return find_repository_root(Path(__file__).parent)


def validate_profile_name(profile_name: str) -> str:
    """Reject names that could escape the profile directory or be ambiguous."""

    cleaned = profile_name.strip()
    if not cleaned:
        raise ValueError("Profile name cannot be empty.")
    if not PROFILE_NAME_PATTERN.fullmatch(cleaned):
        raise ValueError("Profile name may contain only letters, numbers, '.', '_' and '-'.")
    return cleaned


def choose_profile_name(requested: str | None) -> str:
    if requested is not None:
        return validate_profile_name(requested)

    entered = input(f"Enter konsave profile name [{DEFAULT_PROFILE_NAME}]: ").strip()
    return validate_profile_name(entered or DEFAULT_PROFILE_NAME)


def newest_export(profile_directory_path: Path) -> Path | None:
    exports = list(profile_directory_path.glob("*.knsv"))
    return max(exports, key=lambda path: path.stat().st_mtime, default=None)


def save_profile(profile_name: str, profiles_dir: Path, konsave: list[str]) -> Path | None:
    print(f"Saving current KDE configuration as profile: {profile_name}")
    run_command([*konsave, "-s", profile_name])

    print(f"Exporting profile to {profiles_dir}")
    run_command([*konsave, "-e", profile_name], cwd=profiles_dir)

    expected_export = profiles_dir / f"{profile_name}.knsv"
    exported = expected_export if expected_export.is_file() else newest_export(profiles_dir)
    if exported is None:
        print("Profile exported, but no .knsv output file could be confirmed.")
    else:
        print(f"Saved profile export: {exported}")
    return exported


def ask_upload() -> bool:
    answer = input("Upload profiles to GitHub Releases now? [y/N]: ").strip().lower()
    return answer in {"y", "yes"}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--name", help="Profile name; prompts when omitted.")
    parser.add_argument(
        "--upload",
        action="store_true",
        help="Upload profiles without prompting after export.",
    )
    parser.add_argument(
        "--no-upload",
        action="store_true",
        help="Skip the upload prompt after export.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.upload and args.no_upload:
        raise ValueError("--upload and --no-upload cannot be used together.")

    repository_root = project_root()
    host = detect_host()
    print(f"Detected host: {host.system}/{host.architecture}")

    profiles_dir = profile_directory(repository_root)
    profiles_dir.mkdir(parents=True, exist_ok=True)
    if not os.access(profiles_dir, os.W_OK):
        raise RuntimeError(f"KDE profiles directory is not writable: {profiles_dir}")

    konsave = resolve_konsave_command()
    profile_name = choose_profile_name(args.name)
    save_profile(profile_name, profiles_dir, konsave)

    if args.upload or (not args.no_upload and ask_upload()):
        upload_profiles(repository_root, confirm=False)
    else:
        print("Skipping upload.")

    print("Done.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Error: {error}", file=sys.stderr)
        raise SystemExit(1) from error