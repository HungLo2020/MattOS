#!/usr/bin/env python3
"""Build libmnl's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    install_license,
    run_recipe,
)

class LibmnlRecipe(SourceReleaseRecipe):
    name = "libmnl0"
    repository = "mattos"
    section = "libs"
    description = "Minimalistic Netlink library (library and development files)"
    depends = ("libc6",)
    provides = ("libmnl-dev",)
    git_url = "https://git.netfilter.org/libmnl"
    tag_pattern = r"libmnl-([0-9]+\.[0-9]+\.[0-9]+)"
    source_url = "https://www.netfilter.org/projects/libmnl/files/libmnl-{version}.tar.bz2"
    build_options = ("--disable-static",)

    def post_install(self, staging: Path, source: Path) -> None:
        for archive in staging.rglob("*.la"):
            archive.unlink()
        install_license(staging, self.name, source, ["COPYING"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(LibmnlRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
