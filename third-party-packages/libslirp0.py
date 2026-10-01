#!/usr/bin/env python3
"""Build libslirp's selected release as a native MattOS .deb."""

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

class LibslirpRecipe(SourceReleaseRecipe):
    name = "libslirp0"
    repository = "mattos"
    section = "libs"
    description = "User-mode TCP/IP networking library (library and development files)"
    depends = ("libc6",)
    provides = ("libslirp-dev",)
    git_url = "https://gitlab.freedesktop.org/slirp/libslirp.git"
    tag_pattern = r"v([0-9]+\.[0-9]+\.[0-9]+)"
    source_url = "https://gitlab.freedesktop.org/slirp/libslirp/-/archive/{tag}/libslirp-{tag}.tar.gz"
    build_system = "meson"
    build_options = ("-Ddefault_library=shared",)

    def post_install(self, staging: Path, source: Path) -> None:
        install_license(staging, self.name, source, ["COPYRIGHT"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(LibslirpRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
