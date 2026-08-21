"""Synchronize Konsave profile exports with GitHub Releases."""

from __future__ import annotations

import os
from pathlib import Path

from github import download_asset, ensure_gh_authenticated, release_assets, release_tags, repository_slug
from paths import profile_directory
from process import run_command


def local_profiles(profiles_dir: Path) -> dict[str, Path]:
    """Return profile names mapped to their exported .knsv files."""

    profiles = {path.stem: path for path in sorted(profiles_dir.glob("*.knsv"))}
    if not profiles:
        raise RuntimeError(f"No .knsv profiles found in {profiles_dir}")
    return profiles


def upload_profiles(repository_root: Path, *, confirm: bool = True) -> bool:
    """Make GitHub Releases match the local Konsave profile exports.

    Returns False when the user declines the destructive synchronization plan.
    """

    profiles = local_profiles(profile_directory(repository_root))
    slug = repository_slug(repository_root)
    gh = ensure_gh_authenticated()
    existing_tags = release_tags(gh, slug)
    stale_tags = sorted(set(existing_tags) - set(profiles))

    print(f"Sync target repository: {slug}")
    print("Release synchronization plan:")
    print(f"  Replace or create: {', '.join(sorted(profiles))}")
    print(f"  Delete stale releases: {', '.join(stale_tags) if stale_tags else 'none'}")
    if confirm and input("Apply this release synchronization plan? [y/N]: ").strip().lower() not in {"y", "yes"}:
        print("Release synchronization cancelled.")
        return False

    for tag in stale_tags:
        print(f"Deleting stale release: {tag}")
        run_command([gh, "release", "delete", tag, "--repo", slug, "--yes", "--cleanup-tag"])

    for name, profile_path in profiles.items():
        if name in existing_tags:
            print(f"Deleting existing release for overwrite: {name}")
            run_command([gh, "release", "delete", name, "--repo", slug, "--yes", "--cleanup-tag"])
        print(f"Creating release {name} with asset {profile_path.name}")
        run_command([gh, "release", "create", name, str(profile_path), "--repo", slug, "--title", name, "--notes", ""])

    print("Done. GitHub Releases now match local .knsv profiles.")
    return True


def is_safe_profile_asset(name: str) -> bool:
    """Allow only simple .knsv filenames from GitHub Release assets."""

    return Path(name).name == name and name.endswith(".knsv")


def download_profiles(repository_root: Path, *, overwrite: bool = False) -> tuple[int, int]:
    """Download release profile assets and return downloaded/skipped counts."""

    profiles_dir = profile_directory(repository_root)
    profiles_dir.mkdir(parents=True, exist_ok=True)
    slug = repository_slug(repository_root)
    token = os.environ.get("GITHUB_TOKEN", "").strip() or None
    assets = [asset for asset in release_assets(slug, token) if is_safe_profile_asset(asset.name)]

    if not assets:
        print(f"No .knsv release assets found in {slug}.")
        return 0, 0

    downloaded = 0
    skipped = 0
    for asset in assets:
        destination = profiles_dir / asset.name
        if destination.exists() and not overwrite:
            print(f"Skipping existing profile: {asset.name}")
            skipped += 1
            continue
        print(f"Downloading [{asset.tag}] {asset.name}")
        download_asset(asset, destination, token)
        downloaded += 1

    print(f"Done. Downloaded {downloaded} profile(s); skipped {skipped} existing profile(s).")
    return downloaded, skipped