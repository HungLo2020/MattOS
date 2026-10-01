#!/usr/bin/env python3
"""Build GNU Wget's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    run_recipe,
)

class WgetRecipe(SourceReleaseRecipe):
    name = "wget"
    repository = "mattos"
    section = "web"
    description = "Retrieves files from the web over HTTP, HTTPS and FTP"
    depends = ("libc6",)
    git_url = "https://git.savannah.gnu.org/git/wget.git"
    tag_pattern = r"v(1\.[0-9]+(?:\.[0-9]+)*)"
    source_url = "https://ftp.gnu.org/gnu/wget/wget-{version}.tar.gz"
    # OpenSSL, zlib and PCRE2 come from MattOS; like curl and rsync, Wget is
    # built without IDN and public-suffix support (MattOS ships neither).
    build_options = (
        "--sysconfdir=/etc", "--with-ssl=openssl", "--without-libpsl", "--disable-iri",
        "--without-metalink", "--without-cares", "--disable-nls", "--without-libuuid",
    )


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(WgetRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
