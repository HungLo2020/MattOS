#!/usr/bin/env python3
"""Build netavark's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    cargo_environment,
    command,
    download,
    install_file,
    install_license,
    run_recipe,
)

# netavark generates its DHCP-proxy gRPC code at build time with protoc: a
# pinned, checksum-verified upstream release, used only during the build.
PROTOC_VERSION = "36.2"
PROTOC_SHA256 = "121f6c7afe1d4d0e3ea6aab9432038599250134cbf4474cb1167d2c7decd4278"


def protoc(workspace: Path) -> Path:
    archive = workspace / "protoc.zip"
    download(f"https://github.com/protocolbuffers/protobuf/releases/download/v{PROTOC_VERSION}/"
             f"protoc-{PROTOC_VERSION}-linux-x86_64.zip", archive, sha256=PROTOC_SHA256)
    root = workspace / "protoc"
    with zipfile.ZipFile(archive) as bundle:
        bundle.extractall(root)
    binary = root / "bin/protoc"
    binary.chmod(0o755)
    return binary


class NetavarkRecipe(SourceReleaseRecipe):
    name = "netavark"
    repository = "mattos"
    section = "admin"
    description = "Container network stack for Podman"
    # Rootful container networking programs the firewall through nft.
    depends = ("libc6", "aardvark-dns", "nftables")
    github = ("containers", "netavark")
    source_url = "https://github.com/containers/netavark/archive/refs/tags/{tag}.tar.gz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        env = {**cargo_environment(workspace), "PROTOC": str(protoc(workspace))}
        command(["cargo", "build", "--release", "--locked"], cwd=source, env=env)
        for program in ("netavark", "netavark-dhcp-proxy-client"):
            install_file(workspace / "cargo-target/release" / program,
                         staging / "usr/libexec/podman" / program, 0o755)
        install_license(staging, self.name, source, ["LICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(NetavarkRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
