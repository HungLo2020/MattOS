#!/usr/bin/env python3
"""Configure COSMIC to rotate wallpapers from the shared storage directory."""

from __future__ import annotations

import json
import os
import pwd
import tempfile
from pathlib import Path


WALLPAPER_DIRECTORY = Path("/mnt/storage/OneDrive/Media/Wallpapers/Wide")
COSMIC_DIRECTORY = ".config/cosmic"
BACKGROUND_DIRECTORY = "com.system76.CosmicBackground/v1"
WALLPAPER_SETTINGS_DIRECTORY = "com.system76.CosmicSettings.Wallpaper/v1"


def target_account() -> pwd.struct_passwd:
    """Resolve the desktop user, including when Setup was invoked with sudo."""

    username = os.environ.get("SUDO_USER") if os.geteuid() == 0 else None
    return pwd.getpwnam(username) if username else pwd.getpwuid(os.getuid())


def background_configuration(wallpaper_directory: Path) -> str:
    """Return COSMIC Background's native persistent wallpaper configuration."""

    return """(
    output: "all",
    source: Path(%s),
    filter_by_theme: true,
    rotation_frequency: 300,
    filter_method: Lanczos,
    scaling_mode: Zoom,
    sampling_method: Alphanumeric,
)""" % json.dumps(str(wallpaper_directory))


def write_user_file(path: Path, contents: str, account: pwd.struct_passwd) -> None:
    """Atomically write one COSMIC setting file with desktop-user ownership."""

    path.parent.mkdir(parents=True, exist_ok=True)
    temporary_path: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            dir=path.parent,
            prefix=f".{path.name}.",
            delete=False,
        ) as temporary:
            temporary.write(contents)
            temporary_path = Path(temporary.name)
        temporary_path.chmod(0o664)
        if os.geteuid() == 0:
            os.chown(temporary_path, account.pw_uid, account.pw_gid)
        os.replace(temporary_path, path)
    finally:
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)


def configure(
    home: Path,
    wallpaper_directory: Path = WALLPAPER_DIRECTORY,
    account: pwd.struct_passwd | None = None,
) -> tuple[Path, ...]:
    """Write the COSMIC background and wallpaper-picker settings."""

    account = account or target_account()
    cosmic_directory = home / COSMIC_DIRECTORY
    background_directory = cosmic_directory / BACKGROUND_DIRECTORY
    wallpaper_settings_directory = cosmic_directory / WALLPAPER_SETTINGS_DIRECTORY

    files = {
        background_directory / "all": background_configuration(wallpaper_directory),
        background_directory / "same-on-all": "true",
        wallpaper_settings_directory / "current-folder": f"Some({json.dumps(str(wallpaper_directory))})",
        wallpaper_settings_directory / "recent-folders": json.dumps([str(wallpaper_directory)], indent=4) + "\n",
    }
    for path, contents in files.items():
        write_user_file(path, contents, account)
    return tuple(files)


def main() -> int:
    """Configure the invoking user's COSMIC wallpaper settings."""

    account = target_account()
    paths = configure(Path(account.pw_dir), account=account)
    if not WALLPAPER_DIRECTORY.is_dir():
        print(f"Configured COSMIC wallpaper directory, but it is not currently mounted: {WALLPAPER_DIRECTORY}")
    else:
        print(f"Configured COSMIC wallpaper rotation from {WALLPAPER_DIRECTORY}")
    for path in paths:
        print(f"  Wrote {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
