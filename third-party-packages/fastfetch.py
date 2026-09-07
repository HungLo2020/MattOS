


#!/usr/bin/env python3
"""Build and publish the current Fastfetch release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (
    BuildResult, PackageRecipe, RecipeError, cmake_build_install,
    download, extract_archive, finalize_package, github_latest_release,
    github_source_archive, run_recipe, sha256_file,
)


class FastfetchRecipe(PackageRecipe):
    name = "fastfetch"
    repository = "mattos"
    description = "System information tool"
    depends = ("libc6", "libgcc-s1")

    def discover_version(self) -> tuple[str, dict[str, str]]:
        version, provenance = github_latest_release("fastfetch-cli", "fastfetch")
        if not all(part.isdigit() for part in version.split(".")):
            raise RecipeError("Fastfetch release API returned an invalid stable tag")
        return version, provenance

    def build(self, workspace: Path, version: str, provenance: dict[str, str]) -> BuildResult:
        archive = workspace / f"fastfetch-{version}.tar.gz"
        url = github_source_archive("fastfetch-cli", "fastfetch", provenance["release_tag"])
        download(url, archive)
        source = extract_archive(archive, workspace / "source")
        build = workspace / "build"
        staging = workspace / "package"
        cmake_build_install(source, build, staging, options=["-DENABLE_TESTS=OFF"])
        provenance = {**provenance, "source_url": url, "source_sha256": sha256_file(archive)}
        return finalize_package(self, staging, workspace, version, provenance)


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(FastfetchRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
