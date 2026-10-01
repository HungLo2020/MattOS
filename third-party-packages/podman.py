#!/usr/bin/env python3
"""Build Podman's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    command,
    go_environment,
    install_license,
    run_recipe,
)

# seccomp and systemd (journald) support link MattOS/third-party libraries;
# image signatures are verified in pure Go instead of through GPGME.
BUILD_TAGS = "grpcnotrace seccomp systemd containers_image_openpgp exclude_graphdriver_btrfs"


class PodmanRecipe(SourceReleaseRecipe):
    name = "podman"
    repository = "mattos"
    section = "admin"
    description = "Daemonless, rootless OCI container engine"
    depends = ("conmon", "crun", "netavark", "aardvark-dns", "passt", "catatonit",
               "containers-common", "uidmap")
    build_depends = ("libseccomp2",)
    github = ("containers", "podman")
    source_url = "https://github.com/containers/podman/archive/refs/tags/{tag}.tar.gz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        env = go_environment(workspace, cgo=True)
        options = [f"BUILDTAGS={BUILD_TAGS}", "GOCMD=go", "PREFIX=/usr", "ETCDIR=/etc",
                   f"RELEASE_VERSION={self.upstream_version}", "GIT_COMMIT="]
        command(["make", *options, "bin/podman", "bin/rootlessport", "bin/quadlet"], cwd=source, env=env)
        command(["make", *options, f"DESTDIR={staging}", "install.bin", "install.systemd"], cwd=source, env=env)
        install_license(staging, self.name, source, ["LICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(PodmanRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
