"""GitHub repository and release helpers shared by project tools."""

from __future__ import annotations

import json
import re
import urllib.request
from dataclasses import dataclass
from pathlib import Path

from process import require_command, run_command


GITHUB_REMOTE_PATTERN = re.compile(r"github\.com[:/]([^/]+)/([^/.]+)(?:\.git)?$")


@dataclass(frozen=True)
class ReleaseAsset:
    """A downloadable asset published on a GitHub Release."""

    tag: str
    name: str
    download_url: str


def repository_slug(repository_root: Path) -> str:
    """Resolve owner/repository from the repository's origin remote."""

    result = run_command(
        ["git", "-C", str(repository_root), "remote", "get-url", "origin"],
        capture_output=True,
    )
    origin_url = result.stdout.strip()
    match = GITHUB_REMOTE_PATTERN.search(origin_url)
    if match is None:
        raise RuntimeError(f"Origin is not a recognizable GitHub URL: {origin_url or 'missing'}")
    return f"{match.group(1)}/{match.group(2)}"


def ensure_gh_authenticated() -> str:
    """Return the GitHub CLI path, prompting for login only when needed."""

    gh = require_command("gh")
    if run_command([gh, "auth", "status"], check=False).returncode != 0:
        print("GitHub CLI is not authenticated. Starting interactive login...")
        run_command([gh, "auth", "login"])
        if run_command([gh, "auth", "status"], check=False).returncode != 0:
            raise RuntimeError("GitHub CLI authentication failed.")
    return gh


def release_tags(gh: str, slug: str) -> list[str]:
    """Return all release tag names for a GitHub repository."""

    result = run_command(
        [gh, "api", "--paginate", f"/repos/{slug}/releases?per_page=100", "--jq", ".[].tag_name"],
        capture_output=True,
    )
    return [tag for tag in result.stdout.splitlines() if tag]


def release_assets(slug: str, token: str | None = None) -> list[ReleaseAsset]:
    """List GitHub Release assets using the public API or GITHUB_TOKEN."""

    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "LinuxScripts-KonsaveDownloader",
    }
    if token:
        headers["Authorization"] = f"Bearer {token}"

    assets: list[ReleaseAsset] = []
    page = 1
    while True:
        request = urllib.request.Request(
            f"https://api.github.com/repos/{slug}/releases?per_page=100&page={page}",
            headers=headers,
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            releases = json.loads(response.read().decode("utf-8"))
        if not releases:
            return assets

        for release in releases:
            tag = release.get("tag_name", "")
            for asset in release.get("assets", []):
                name = asset.get("name", "")
                download_url = asset.get("browser_download_url", "")
                if tag and name and download_url:
                    assets.append(ReleaseAsset(tag, name, download_url))

        if len(releases) < 100:
            return assets
        page += 1


def download_asset(asset: ReleaseAsset, destination: Path, token: str | None = None) -> None:
    """Download one release asset to an already validated destination path."""

    headers = {"User-Agent": "LinuxScripts-KonsaveDownloader"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(asset.download_url, headers=headers)
    with urllib.request.urlopen(request, timeout=60) as response:
        destination.write_bytes(response.read())