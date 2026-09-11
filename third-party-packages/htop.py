#!/usr/bin/env python3
"""Build and publish the current htop release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (
    BuildResult, PackageRecipe, RecipeError, autotools_build_install, download,
    extract_archive, finalize_package, github_latest_release,
    github_source_archive, run_recipe, sha256_file,
)


class HtopRecipe(PackageRecipe):
    name = "htop"
    repository = "mattos"
    description = "Interactive process viewer"
    depends = ("libc6", "libcap2", "libncursesw6", "libnl-3-200", "libnl-genl-3-200", "libsystemd0")

    def discover_version(self) -> tuple[str, dict[str, str]]:
        version, provenance = github_latest_release("htop-dev", "htop")
        if not all(part.isdigit() for part in version.split(".")):
            raise RecipeError("htop release API returned an invalid stable tag")
        return version, provenance

    def build(self, workspace: Path, version: str, provenance: dict[str, str]) -> BuildResult:
        archive = workspace / f"htop-{version}.tar.gz"
        url = github_source_archive("htop-dev", "htop", provenance["release_tag"])
        download(url, archive)
        source = extract_archive(archive, workspace / "source")
        staging = workspace / "package"
        autotools_build_install(source, workspace / "build", staging, options=["--enable-unicode"])
        provenance = {**provenance, "source_url": url, "source_sha256": sha256_file(archive)}
        return finalize_package(self, staging, workspace, version, provenance)


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(HtopRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
