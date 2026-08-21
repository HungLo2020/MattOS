#!/usr/bin/env python3
"""Install the official Tailscale APT source for Debian and Ubuntu hosts."""

from __future__ import annotations

import os
import subprocess
import sys
import urllib.request


OS_RELEASE = "/etc/os-release"
KEY_PATH = "/usr/share/keyrings/tailscale-archive-keyring.gpg"
SOURCE_PATH = "/etc/apt/sources.list.d/tailscale.list"


def os_release() -> dict[str, str]:
    """Return the unquoted values needed to select Tailscale's APT source."""

    values: dict[str, str] = {}
    with open(OS_RELEASE, encoding="utf-8") as release_file:
        for line in release_file:
            key, separator, value = line.strip().partition("=")
            if separator:
                values[key] = value.strip('"')
    return values


def download(url: str) -> bytes:
    """Download one public repository file with a bounded timeout."""

    with urllib.request.urlopen(url, timeout=30) as response:
        return response.read()


def install_file(destination: str, contents: bytes) -> None:
    """Write repository data through sudo without a shell pipeline."""

    subprocess.run(("sudo", "install", "-d", "-m", "0755", os.path.dirname(destination)), check=True)
    subprocess.run(("sudo", "tee", destination), input=contents, stdout=subprocess.DEVNULL, check=True)


def main() -> int:
    values = os_release()
    distribution = values.get("ID", "")
    codename = values.get("VERSION_CODENAME", "")
    if distribution not in {"debian", "ubuntu"} or not codename:
        raise RuntimeError("The official Tailscale APT source requires a Debian or Ubuntu VERSION_CODENAME.")

    base_url = f"https://pkgs.tailscale.com/stable/{distribution}/{codename}"
    print(f"Configuring the official Tailscale APT source for {distribution} {codename}.")
    install_file(KEY_PATH, download(f"{base_url}.noarmor.gpg"))
    install_file(SOURCE_PATH, download(f"{base_url}.tailscale-keyring.list"))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError, urllib.error.URLError) as error:
        print(f"Error: {error}", file=sys.stderr)
        raise SystemExit(1) from error