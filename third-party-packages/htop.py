#!/usr/bin/env python3
"""Build htop's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import RecipeError, SourceReleaseRecipe, run_recipe


class HtopRecipe(SourceReleaseRecipe):
    name = "htop"
    repository = "mattos"
    description = "Interactive process viewer"
    depends = ("libc6", "libcap2", "libncursesw6", "mattos-libtinfow6", "libnl-3-200", "libnl-genl-3-200")
    github = ("htop-dev", "htop")
    # The release archive carries the generated configure script.
    source_url = "https://github.com/htop-dev/htop/releases/download/{version}/htop-{version}.tar.xz"
    build_options = ("--enable-unicode", "--enable-capabilities")


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(HtopRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
