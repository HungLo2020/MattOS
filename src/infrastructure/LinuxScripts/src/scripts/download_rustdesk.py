#!/usr/bin/env python3
"""Download the latest official RustDesk AMD64 Debian package to /tmp."""

from __future__ import annotations

import json
import sys
import urllib.request


RELEASE_URL = "https://api.github.com/repos/rustdesk/rustdesk/releases/latest"
DESTINATION = "/tmp/linuxscripts-rustdesk-amd64.deb"


def latest_amd64_deb_url() -> str:
    """Return the latest RustDesk AMD64 Debian package download URL."""

    request = urllib.request.Request(RELEASE_URL, headers={"Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(request, timeout=30) as response:
        release = json.load(response)
    for asset in release.get("assets", []):
        name = asset.get("name", "")
        if name.endswith(".deb") and ("amd64" in name or "x86_64" in name):
            return asset["browser_download_url"]
    raise RuntimeError("The latest RustDesk release has no AMD64 Debian package.")


def main() -> int:
    url = latest_amd64_deb_url()
    print(f"Downloading RustDesk from {url}")
    request = urllib.request.Request(url, headers={"Accept": "application/octet-stream"})
    with urllib.request.urlopen(request, timeout=120) as response, open(DESTINATION, "wb") as package_file:
        while chunk := response.read(1024 * 1024):
            package_file.write(chunk)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, OSError, RuntimeError, urllib.error.URLError) as error:
        print(f"Error: {error}", file=sys.stderr)
        raise SystemExit(1) from error