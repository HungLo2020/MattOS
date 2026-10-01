#!/usr/bin/env python3
"""Build the GitHub CLI's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    command,
    go_environment,
    install_file,
    install_license,
    run_recipe,
)

class GhRecipe(SourceReleaseRecipe):
    name = "gh"
    repository = "mattos"
    toolchains = ("go",)
    section = "vcs"
    description = "GitHub's official command-line tool"
    depends = ("git",)
    github = ("cli", "cli")
    source_url = "https://github.com/cli/cli/archive/refs/tags/{tag}.tar.gz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        env = go_environment(workspace)
        binary = workspace / "gh"
        ldflags = f"-s -w -X github.com/cli/cli/v2/internal/build.Version={self.upstream_version}"
        command(["go", "build", "-ldflags", ldflags, "-o", str(binary), "./cmd/gh"], cwd=source, env=env)
        install_file(binary, staging / "usr/bin/gh", 0o755)
        manuals = workspace / "man"
        command(["go", "run", "./cmd/gen-docs", "--man-page", "--doc-path", str(manuals)], cwd=source, env=env)
        for page in sorted(manuals.glob("*.1")):
            install_file(page, staging / "usr/share/man/man1" / page.name)
        for shell, destination in (
            ("bash", "usr/share/bash-completion/completions/gh"),
            ("zsh", "usr/share/zsh/vendor-completions/_gh"),
            ("fish", "usr/share/fish/vendor_completions.d/gh.fish"),
        ):
            target = staging / destination
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(command([str(binary), "completion", "-s", shell], env={**env, "GH_CONFIG_DIR": str(workspace / "gh-config")}),
                              encoding="utf-8")
        install_license(staging, self.name, source, ["LICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(GhRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
