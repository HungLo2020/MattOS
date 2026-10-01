"""The pinned Go toolchain Go recipes build with.

Go is not needed to build MattOS itself, so it is neither vendored nor
packaged: a checksum-verified official release is downloaded into the build.
This file is part of the build inputs of recipes that declare the "go"
toolchain only, so a Go update rebuilds just the Go packages.
"""

from __future__ import annotations

import os
import tarfile
from pathlib import Path

from .build import download


GO_VERSION = "1.27.1"


GO_SHA256 = "63d339f0da5ab53635a56f2490a7984dfe12dfcff22ad749f63edaf590168445"


def go_environment(workspace: Path, *, cgo: bool = False) -> dict[str, str]:
    """Environment for `go build` with the pinned Go toolchain.  Modules are
    fetched from the Go module proxy and verified against go.sum."""
    root = workspace / "go-toolchain"
    if not (root / "go/bin/go").is_file():
        archive = workspace / f"go{GO_VERSION}.linux-amd64.tar.gz"
        download(f"https://go.dev/dl/go{GO_VERSION}.linux-amd64.tar.gz", archive, sha256=GO_SHA256)
        root.mkdir(parents=True, exist_ok=True)
        with tarfile.open(archive, "r:gz") as tar:
            tar.extractall(root, filter="data")
        archive.unlink()
    cache = workspace / "go-cache"
    return {
        **os.environ,
        "PATH": f"{root / 'go/bin'}:{os.environ.get('PATH', '/usr/bin:/bin')}",
        "GOPATH": str(cache / "path"),
        "GOMODCACHE": str(cache / "mod"),
        "GOCACHE": str(cache / "build"),
        "GOTOOLCHAIN": "local",
        "GOFLAGS": "-trimpath -buildvcs=false -mod=readonly",
        "CGO_ENABLED": "1" if cgo else "0",
        "HOME": str(workspace),
    }
