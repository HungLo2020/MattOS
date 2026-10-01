#!/usr/bin/env python3
"""Build catatonit's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    autotools_build_install,
    install_file,
    install_license,
    run_recipe,
)

class CatatonitRecipe(SourceReleaseRecipe):
    name = "catatonit"
    repository = "mattos"
    section = "admin"
    description = "Container init process (Podman's --init)"
    depends = ()
    github = ("openSUSE", "catatonit")
    source_url = "https://github.com/openSUSE/catatonit/archive/refs/tags/{tag}.tar.gz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        installed = workspace / "installed"
        autotools_build_install(source, workspace / "build", installed)
        # Podman's default init_path.
        install_file(installed / "usr/bin/catatonit", staging / "usr/libexec/podman/catatonit", 0o755)
        install_license(staging, self.name, source, ["COPYING"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(CatatonitRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
