#!/usr/bin/env python3
"""Build btop's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import RecipeError, SourceReleaseRecipe, run_recipe


class BtopRecipe(SourceReleaseRecipe):
    name = "btop"
    repository = "mattos"
    description = "Resource monitor with a modern terminal interface"
    depends = ("libc6", "libgcc-s1", "libstdc++6")
    github = ("aristocratos", "btop")
    source_url = "https://github.com/aristocratos/btop/archive/refs/tags/{tag}.tar.gz"
    # btop's own Makefile needs only a C++20 compiler.
    build_system = "make"
    install_options = ("PREFIX=/usr",)


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(BtopRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
