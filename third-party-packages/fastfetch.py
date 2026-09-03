


#!/usr/bin/env python3
"""Build and publish the current Fastfetch release as a native MattOS .deb."""

from __future__ import annotations

import json
import shutil
import sys
from pathlib import Path
from urllib.request import Request, urlopen

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import BuildResult, PackageRecipe, RecipeError, command, download, extract_archive, require_tools, run_recipe, sha256_file, write_control, write_provenance


class FastfetchRecipe(PackageRecipe):
    name = "fastfetch"

    def discover_version(self) -> tuple[str, dict[str, str]]:
        request = Request("https://api.github.com/repos/fastfetch-cli/fastfetch/releases/latest",
                          headers={"User-Agent": "MattOS-third-party-packages/1", "Accept": "application/vnd.github+json"})
        with urlopen(request, timeout=30) as response:
            release = json.load(response)
        tag = str(release.get("tag_name", ""))
        if tag.startswith("v"):
            tag = tag[1:]
        if not tag or not all(part.isdigit() for part in tag.split(".")):
            raise RecipeError("Fastfetch release API returned an invalid stable tag")
        return tag, {"upstream": "https://github.com/fastfetch-cli/fastfetch", "release_tag": release["tag_name"]}

    def build(self, workspace: Path, version: str, provenance: dict[str, str]) -> BuildResult:
        require_tools(["cmake", "make", "cc", "dpkg-deb"])
        archive = workspace / f"fastfetch-{version}.tar.gz"
        url = f"https://github.com/fastfetch-cli/fastfetch/archive/refs/tags/{version}.tar.gz"
        download(url, archive)
        source = extract_archive(archive, workspace / "source")
        build = workspace / "build"
        staging = workspace / "package"
        command(["cmake", "-S", str(source), "-B", str(build), "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_INSTALL_PREFIX=/usr", "-DENABLE_TESTS=OFF"])
        command(["cmake", "--build", str(build), "--parallel"])
        command(["cmake", "--install", str(build), f"--prefix={staging / 'usr'}"])
        write_control(staging, name=self.name, version=version, description="System information tool", depends=["libc6", "libgcc-s1"])
        provenance = {**provenance, "source_url": url, "source_sha256": sha256_file(archive)}
        write_provenance(staging, provenance)
        artifact = workspace / f"{self.name}_{version}_amd64.deb"
        command(["dpkg-deb", "--root-owner-group", "--build", str(staging), str(artifact)])
        return BuildResult(self.name, version, self.architecture, artifact, provenance)


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(FastfetchRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
