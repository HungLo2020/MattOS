#!/usr/bin/env python3
"""Build conmon's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    command,
    install_file,
    install_license,
    run_recipe,
)

class ConmonRecipe(SourceReleaseRecipe):
    name = "conmon"
    repository = "mattos"
    section = "admin"
    description = "OCI container runtime monitor"
    depends = ("libc6",)
    build_depends = ("libseccomp2",)
    github = ("containers", "conmon")
    source_url = "https://github.com/containers/conmon/archive/refs/tags/{tag}.tar.gz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        command(["make", f"VERSION={self.upstream_version}", "GIT_COMMIT=", "bin/conmon"], cwd=source)
        install_file(source / "bin/conmon", staging / "usr/bin/conmon", 0o755)
        install_license(staging, self.name, source, ["LICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(ConmonRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
