#!/usr/bin/env python3
"""Build libwebp's selected release as a native MattOS .deb."""

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

class LibwebpRecipe(SourceReleaseRecipe):
    name = "libwebp7"
    repository = "mattos"
    section = "libs"
    description = "WebP image codec libraries (libwebp, mux, demux, sharpyuv; library and development files)"
    depends = ("libc6",)
    provides = ("libwebp-dev", "libwebpmux3", "libwebpdemux2", "libsharpyuv0", "libsharpyuv-dev")
    github = ("webmproject", "libwebp")
    source_url = "https://storage.googleapis.com/downloads.webmproject.org/releases/webp/libwebp-{version}.tar.gz"
    # Only the libraries: the example tools' image readers are disabled.
    build_options = ("--disable-static", "--enable-libwebpmux", "--enable-libwebpdemux",
                     "--disable-png", "--disable-jpeg", "--disable-tiff", "--disable-gif",
                     "--disable-gl", "--disable-sdl", "--disable-wic")

    def post_install(self, staging: Path, source: Path) -> None:
        shutil.rmtree(staging / "usr/bin", ignore_errors=True)
        shutil.rmtree(staging / "usr/share/man", ignore_errors=True)
        for archive in staging.rglob("*.la"):
            archive.unlink()
        install_license(staging, self.name, source, ["COPYING", "PATENTS"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(LibwebpRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
