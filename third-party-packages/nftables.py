#!/usr/bin/env python3
"""Build nftables' selected release as a native MattOS .deb."""

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

class NftablesRecipe(SourceReleaseRecipe):
    name = "nftables"
    repository = "mattos"
    section = "net"
    description = "Netfilter packet filtering framework administration tool (nft)"
    depends = ("libc6",)
    # Debian ships the library separately as libnftables1.
    provides = ("libnftables1",)
    build_depends = ("libmnl0", "libnftnl11")
    git_url = "https://git.netfilter.org/nftables"
    tag_pattern = r"v([0-9]+\.[0-9]+\.[0-9]+)"
    source_url = "https://www.netfilter.org/projects/nftables/files/nftables-{version}.tar.xz"
    # JSON is what netavark (Podman's network stack) drives nft with; the
    # bundled mini-gmp replaces GMP, and the interactive shell (readline) is
    # left out.
    build_options = ("--disable-static", "--with-json", "--with-mini-gmp", "--without-cli",
                     "--disable-man-doc", "--sysconfdir=/etc")

    def post_install(self, staging: Path, source: Path) -> None:
        for archive in staging.rglob("*.la"):
            archive.unlink()
        install_license(staging, self.name, source, ["COPYING"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(NftablesRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
