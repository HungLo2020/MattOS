#!/usr/bin/env python3
"""Build Cozy as an optional third-party terminal editor."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (RecipeError, SourceReleaseRecipe, cargo_environment, command,
                    install_file, install_license, run_recipe)


class CozyRecipe(SourceReleaseRecipe):
    # Preserve the old MattOS package identity; Debian's cozy is unrelated.
    name = "mattos-cozy"
    repository = "mattos"
    section = "editors"
    description = "Optional Cozy terminal text editor"
    toolchains = ("rust",)
    github = ("takakix2", "cozy")
    source_url = "https://codeload.github.com/takakix2/cozy/tar.gz/refs/tags/{tag}"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        command(["cargo", "build", "--locked", "--release", "--bin", "cozy"],
                cwd=source, env=cargo_environment(workspace))
        install_file(workspace / "cargo-target/release/cozy", staging / "usr/bin/cozy", 0o755)

    def post_install(self, staging: Path, source: Path) -> None:
        install_license(staging, self.name, source, ["LICENSE-MIT", "LICENSE-APACHE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(CozyRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
