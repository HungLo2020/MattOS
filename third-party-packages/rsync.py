#!/usr/bin/env python3
"""Build and publish the current rsync release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (
    BuildResult, PackageRecipe, RecipeError, autotools_build_install, download,
    extract_archive, finalize_package, github_latest_release,
    github_source_archive, run_recipe, sha256_file,
)


class RsyncRecipe(PackageRecipe):
    name = "rsync"
    repository = "mattos"
    description = "Fast, versatile remote and local file-copying tool"
    depends = ("libacl1", "libattr1", "libc6")

    def discover_version(self) -> tuple[str, dict[str, str]]:
        version, provenance = github_latest_release("RsyncProject", "rsync")
        if not all(part.isdigit() for part in version.split(".")):
            raise RecipeError("rsync release API returned an invalid stable tag")
        return version, provenance

    def build(self, workspace: Path, version: str, provenance: dict[str, str]) -> BuildResult:
        archive = workspace / f"rsync-{version}.tar.gz"
        url = github_source_archive("RsyncProject", "rsync", provenance["release_tag"])
        download(url, archive)
        source = extract_archive(archive, workspace / "source")
        staging = workspace / "package"
        autotools_build_install(
            source, workspace / "build", staging,
            options=[
                "--with-included-popt", "--enable-acl-support", "--enable-xattr-support",
                # GitHub's source archive intentionally omits release manpages.
                "--disable-md2man",
                # Keep the native closure deliberately limited to MattOS
                # libraries declared above; these optional features require
                # dependencies not yet provided as stable third-party ABI.
                "--disable-openssl", "--disable-xxhash", "--disable-zstd", "--disable-lz4",
            ],
        )
        provenance = {**provenance, "source_url": url, "source_sha256": sha256_file(archive)}
        return finalize_package(self, staging, workspace, version, provenance)


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(RsyncRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
