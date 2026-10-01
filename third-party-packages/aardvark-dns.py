#!/usr/bin/env python3
"""Build aardvark-dns's selected release as a native MattOS .deb."""

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

class AardvarkDnsRecipe(SourceReleaseRecipe):
    name = "aardvark-dns"
    repository = "mattos"
    toolchains = ("rust",)
    section = "admin"
    description = "Authoritative DNS server for Podman container networks"
    depends = ("libc6",)
    github = ("containers", "aardvark-dns")
    source_url = "https://github.com/containers/aardvark-dns/archive/refs/tags/{tag}.tar.gz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        command(["cargo", "build", "--release", "--locked"], cwd=source, env=cargo_environment(workspace))
        install_file(workspace / "cargo-target/release/aardvark-dns",
                     staging / "usr/libexec/podman/aardvark-dns", 0o755)
        install_license(staging, self.name, source, ["LICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(AardvarkDnsRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
