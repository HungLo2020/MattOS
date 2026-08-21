#!/usr/bin/env python3
"""Deploy the repository Variety configuration after Linux installation."""

from __future__ import annotations

import os
import pwd
import shutil
import sys
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
SOURCE_CONFIGURATION = REPOSITORY_ROOT / "resources" / "variety.conf"


def target_account() -> pwd.struct_passwd:
    """Resolve the interactive setup user, including when Setup was run with sudo."""

    username = os.environ.get("SUDO_USER") if os.geteuid() == 0 else None
    return pwd.getpwnam(username) if username else pwd.getpwuid(os.getuid())


def deploy_configuration(source: Path, account: pwd.struct_passwd) -> Path:
    """Copy the managed config to the target user's Variety config directory."""

    if not source.is_file():
        raise RuntimeError(f"Variety configuration template is missing: {source}")
    destination_directory = Path(account.pw_dir) / ".config" / "variety"
    destination_directory.mkdir(parents=True, exist_ok=True)
    destination = destination_directory / "variety.conf"
    shutil.copyfile(source, destination)
    if os.geteuid() == 0:
        os.chown(destination_directory, account.pw_uid, account.pw_gid)
        os.chown(destination, account.pw_uid, account.pw_gid)
    print(f"Installed Variety configuration at {destination}")
    return destination


def main() -> int:
    """Install the bundled configuration for the user who invoked setup."""

    deploy_configuration(SOURCE_CONFIGURATION, target_account())
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError) as error:
        print(f"Error: Variety configuration failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error