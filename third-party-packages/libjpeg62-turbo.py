#!/usr/bin/env python3
"""Build libjpeg-turbo's selected release as a native MattOS .deb."""

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

class LibjpegTurboRecipe(SourceReleaseRecipe):
    name = "libjpeg62-turbo"
    repository = "mattos"
    section = "libs"
    description = "libjpeg-turbo JPEG codec with the libjpeg 6.2 ABI (library and development files)"
    depends = ("libc6",)
    provides = ("libjpeg62-turbo-dev", "libjpeg-dev", "libturbojpeg0", "libturbojpeg0-dev")
    github = ("libjpeg-turbo", "libjpeg-turbo")
    tag_pattern = r"([0-9]+\.[0-9]+\.[0-9]+)"
    source_url = ("https://github.com/libjpeg-turbo/libjpeg-turbo/releases/download/"
                  "{version}/libjpeg-turbo-{version}.tar.gz")
    build_system = "cmake"
    # The SIMD extensions need NASM, which MattOS does not build.
    build_options = ("-DENABLE_STATIC=OFF", "-DWITH_SIMD=OFF", "-DWITH_TESTS=OFF")

    def post_install(self, staging: Path, source: Path) -> None:
        # The command-line tools (cjpeg, djpeg, ...) are not packaged.
        shutil.rmtree(staging / "usr/bin", ignore_errors=True)
        shutil.rmtree(staging / "usr/share/man", ignore_errors=True)
        shutil.rmtree(staging / "usr/share/doc/libjpeg-turbo", ignore_errors=True)
        install_license(staging, self.name, source, ["LICENSE.md", "README.ijg"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(LibjpegTurboRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
