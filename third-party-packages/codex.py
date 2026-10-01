#!/usr/bin/env python3
"""Build the OpenAI Codex CLI's selected release as a native MattOS .deb."""

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

class CodexRecipe(SourceReleaseRecipe):
    name = "codex"
    repository = "mattos"
    section = "devel"
    description = "OpenAI Codex coding agent command-line interface"
    # Codex searches files with ripgrep.
    depends = ("ripgrep", "ca-certificates")
    github = ("openai", "codex")
    tag_pattern = r"rust-v([0-9]+\.[0-9]+\.[0-9]+)"
    source_url = "https://github.com/openai/codex/archive/refs/tags/{tag}.tar.gz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        # Upstream's release sets the workspace version in Cargo.toml but
        # leaves the workspace crates at 0.0.0 in Cargo.lock.  Only those
        # entries are 0.0.0; after recording the release version, --locked
        # still verifies every dependency is exactly as upstream pinned it.
        lock = source / "codex-rs/Cargo.lock"
        lock.write_text(lock.read_text(encoding="utf-8").replace(
            'version = "0.0.0"', f'version = "{self.upstream_version}"'), encoding="utf-8")
        # Upstream's release profile (thin LTO, debug line tables) needs about
        # 18 GiB for the final crate; without LTO and debug info the build
        # fits an ordinary workstation, and the binary is stripped anyway.
        env = {**cargo_environment(workspace), "CARGO_PROFILE_RELEASE_LTO": "false",
               "CARGO_PROFILE_RELEASE_DEBUG": "false", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "16",
               # Its largest crates (codex-core, the app server) each need
               # about 3 GiB to compile; two at a time fit the build cgroup.
               "CARGO_BUILD_JOBS": "2"}
        command(["cargo", "build", "--release", "--locked", "--bin", "codex"], cwd=source / "codex-rs", env=env)
        binary = staging / "usr/bin/codex"
        install_file(workspace / "cargo-target/release/codex", binary, 0o755)
        # Upstream keeps line tables for its own symbol archive; ship stripped.
        command(["strip", str(binary)])
        install_license(staging, self.name, source, ["LICENSE", "NOTICE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(CodexRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
