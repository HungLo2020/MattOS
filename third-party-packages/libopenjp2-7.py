#!/usr/bin/env python3
"""Build OpenJPEG's selected release as a native MattOS .deb."""

from __future__ import annotations

import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    install_license,
    run_recipe,
)

class OpenjpegRecipe(SourceReleaseRecipe):
    name = "libopenjp2-7"
    repository = "mattos"
    section = "libs"
    description = "OpenJPEG JPEG 2000 codec library (library and development files)"
    depends = ("libc6",)
    provides = ("libopenjp2-7-dev",)
    github = ("uclouvain", "openjpeg")
    source_url = "https://github.com/uclouvain/openjpeg/archive/refs/tags/{tag}.tar.gz"
    build_system = "cmake"
    # The library only: the codec tools would need PNG, TIFF and LCMS.
    build_options = ("-DBUILD_CODEC=OFF", "-DBUILD_STATIC_LIBS=OFF", "-DBUILD_SHARED_LIBS=ON")

    def post_install(self, staging: Path, source: Path) -> None:
        shutil.rmtree(staging / "usr/share/doc", ignore_errors=True)
        install_license(staging, self.name, source, ["LICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(OpenjpegRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
