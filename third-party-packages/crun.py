#!/usr/bin/env python3
"""Build crun's selected release as a native MattOS .deb."""

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

class CrunRecipe(SourceReleaseRecipe):
    name = "crun"
    repository = "mattos"
    section = "admin"
    description = "Fast, lightweight OCI container runtime"
    depends = ("libc6",)
    provides = ("oci-runtime",)
    build_depends = ("libseccomp2",)
    github = ("containers", "crun")
    tag_pattern = r"([0-9]+\.[0-9]+(?:\.[0-9]+)?)"
    source_url = "https://github.com/containers/crun/releases/download/{tag}/crun-{version}.tar.gz"
    build_options = ("--disable-static", "--disable-criu", "--disable-libkrun", "--disable-wasm")

    def post_install(self, staging: Path, source: Path) -> None:
        # Only the runtime: libcrun's development files are not a supported ABI.
        for pattern in ("usr/include", "usr/lib/x86_64-linux-gnu/libcrun*", "usr/lib/x86_64-linux-gnu/pkgconfig"):
            for path in staging.glob(pattern):
                if path.is_dir():
                    shutil.rmtree(path)
                else:
                    path.unlink()
        install_license(staging, self.name, source, ["COPYING"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(CrunRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
