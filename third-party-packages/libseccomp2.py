#!/usr/bin/env python3
"""Build libseccomp's selected release as a native MattOS .deb."""

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

class LibseccompRecipe(SourceReleaseRecipe):
    name = "libseccomp2"
    repository = "mattos"
    section = "libs"
    description = "High-level interface to Linux seccomp filters (library and development files)"
    depends = ("libc6",)
    # One package carries the library and its development files.
    provides = ("libseccomp-dev", "seccomp")
    github = ("seccomp", "libseccomp")
    source_url = "https://github.com/seccomp/libseccomp/releases/download/{tag}/libseccomp-{version}.tar.gz"
    # The release carries the generated syscall table; configure still asks
    # for gperf, which only regenerates it.
    build_options = ("--disable-static", "--disable-python", "GPERF=/usr/bin/true")

    def post_install(self, staging: Path, source: Path) -> None:
        for archive in staging.rglob("*.la"):
            archive.unlink()
        install_license(staging, self.name, source, ["LICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(LibseccompRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
