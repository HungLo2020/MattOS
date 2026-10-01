#!/usr/bin/env python3
"""Build Jansson's selected release as a native MattOS .deb."""

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

class JanssonRecipe(SourceReleaseRecipe):
    name = "libjansson4"
    repository = "mattos"
    section = "libs"
    description = "C library for JSON data (library and development files)"
    depends = ("libc6",)
    provides = ("libjansson-dev",)
    github = ("akheron", "jansson")
    source_url = "https://github.com/akheron/jansson/releases/download/{tag}/jansson-{version}.tar.gz"
    build_options = ("--disable-static",)

    def post_install(self, staging: Path, source: Path) -> None:
        for archive in staging.rglob("*.la"):
            archive.unlink()
        install_license(staging, self.name, source, ["LICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(JanssonRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
