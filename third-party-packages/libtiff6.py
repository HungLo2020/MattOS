#!/usr/bin/env python3
"""Build libtiff's selected release as a native MattOS .deb."""

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

class LibtiffRecipe(SourceReleaseRecipe):
    name = "libtiff6"
    repository = "mattos"
    section = "libs"
    description = "Tag Image File Format (TIFF) library (library and development files)"
    depends = ("libc6",)
    provides = ("libtiff-dev",)
    # JPEG and WebP compression come from these third-party libraries.
    build_depends = ("libjpeg62-turbo", "libwebp7")
    git_url = "https://gitlab.com/libtiff/libtiff.git"
    tag_pattern = r"v(4\.[0-9]+\.[0-9]+)"
    source_url = "https://download.osgeo.org/libtiff/tiff-{version}.tar.gz"
    build_options = ("--disable-static", "--disable-tools", "--disable-tests", "--disable-contrib",
                     "--disable-docs", "--disable-cxx", "--enable-jpeg", "--enable-webp",
                     "--enable-zstd", "--enable-lzma", "--disable-jbig", "--disable-lerc",
                     "--disable-libdeflate", "--disable-sphinx")

    def post_install(self, staging: Path, source: Path) -> None:
        shutil.rmtree(staging / "usr/bin", ignore_errors=True)
        shutil.rmtree(staging / "usr/share/doc", ignore_errors=True)
        shutil.rmtree(staging / "usr/share/man", ignore_errors=True)
        for archive in staging.rglob("*.la"):
            archive.unlink()
        install_license(staging, self.name, source, ["LICENSE.md"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(LibtiffRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
