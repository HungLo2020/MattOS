#!/usr/bin/env python3
"""Build Fastfetch's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import RecipeError, SourceReleaseRecipe, run_recipe


class FastfetchRecipe(SourceReleaseRecipe):
    name = "fastfetch"
    repository = "mattos"
    description = "System information tool"
    depends = ("libc6", "libgcc-s1")
    github = ("fastfetch-cli", "fastfetch")
    source_url = "https://github.com/fastfetch-cli/fastfetch/archive/refs/tags/{tag}.tar.gz"
    build_system = "cmake"
    # GLX needs the X11 development headers, which MattOS (a Wayland system)
    # does not ship; OpenGL detection uses EGL.
    build_options = ("-DBUILD_TESTS=OFF", "-DENABLE_GLX=OFF")


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(FastfetchRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
