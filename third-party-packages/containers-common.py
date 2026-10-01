#!/usr/bin/env python3
"""Build the containers configuration files (containers-common) as a MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    install_file,
    install_license,
    run_recipe,
)

# Short image names ("alpine") resolve against Docker Hub, as on Debian.
UNQUALIFIED_SEARCH = """# MattOS: registries searched for short image names.
unqualified-search-registries = ["docker.io"]
"""


class ContainersCommonRecipe(SourceReleaseRecipe):
    name = "containers-common"
    repository = "mattos"
    section = "admin"
    description = "Configuration files for Podman and other containers tools"
    depends = ()
    github = ("containers", "container-libs")
    tag_pattern = r"common/v([0-9]+\.[0-9]+\.[0-9]+)"
    source_url = "https://github.com/containers/container-libs/archive/refs/tags/{tag}.tar.gz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        files = {
            "image/registries.conf": "etc/containers/registries.conf",
            "image/default-policy.json": "etc/containers/policy.json",
            "common/pkg/config/containers.conf": "usr/share/containers/containers.conf",
            "common/pkg/seccomp/seccomp.json": "usr/share/containers/seccomp.json",
        }
        for relative, destination in files.items():
            install_file(source / relative, staging / destination)
        search = staging / "etc/containers/registries.conf.d/00-mattos-unqualified-search.conf"
        search.parent.mkdir(parents=True, exist_ok=True)
        search.write_text(UNQUALIFIED_SEARCH, encoding="utf-8")
        conffiles = sorted("/" + path.relative_to(staging).as_posix()
                           for path in (staging / "etc").rglob("*") if path.is_file())
        (staging / "DEBIAN").mkdir(parents=True, exist_ok=True)
        (staging / "DEBIAN/conffiles").write_text("\n".join(conffiles) + "\n", encoding="utf-8")
        install_license(staging, self.name, source, ["common/LICENSE", "image/LICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(ContainersCommonRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
