#!/usr/bin/env python3
"""Build ripgrep's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    cargo_environment,
    command,
    install_file,
    install_license,
    run_recipe,
)

class RipgrepRecipe(SourceReleaseRecipe):
    name = "ripgrep"
    repository = "mattos"
    description = "Recursively searches directories for a regex pattern (rg)"
    depends = ("libc6",)
    github = ("BurntSushi", "ripgrep")
    tag_pattern = r"([0-9]+\.[0-9]+\.[0-9]+)"
    source_url = "https://github.com/BurntSushi/ripgrep/archive/refs/tags/{tag}.tar.gz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        command(["cargo", "build", "--release", "--locked"], cwd=source, env=cargo_environment(workspace))
        rg = workspace / "cargo-target/release/rg"
        install_file(rg, staging / "usr/bin/rg", 0o755)
        for flag, destination in (
            ("--generate=man", "usr/share/man/man1/rg.1"),
            ("--generate=complete-bash", "usr/share/bash-completion/completions/rg"),
            ("--generate=complete-zsh", "usr/share/zsh/vendor-completions/_rg"),
            ("--generate=complete-fish", "usr/share/fish/vendor_completions.d/rg.fish"),
        ):
            target = staging / destination
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(command([str(rg), flag]), encoding="utf-8")
        install_license(staging, self.name, source, ["LICENSE-MIT", "UNLICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(RipgrepRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
