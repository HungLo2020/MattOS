#!/usr/bin/env python3
"""Build Tailscale's selected release as a native MattOS .deb."""

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

POSTINST = """#!/bin/sh
set -e
[ -n "${DPKG_ROOT:-}" ] && exit 0
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload >/dev/null || true
    systemctl enable tailscaled.service >/dev/null || true
    systemctl restart tailscaled.service >/dev/null || true
fi
"""

PRERM = """#!/bin/sh
set -e
[ -n "${DPKG_ROOT:-}" ] && exit 0
if [ "$1" = remove ] && [ -d /run/systemd/system ]; then
    systemctl disable --now tailscaled.service >/dev/null || true
fi
"""


class TailscaleRecipe(SourceReleaseRecipe):
    name = "tailscale"
    repository = "mattos"
    toolchains = ("go",)
    section = "net"
    description = "Tailscale WireGuard mesh VPN client and daemon"
    depends = ("iproute2", "ca-certificates")
    github = ("tailscale", "tailscale")
    # Even minor versions are Tailscale's stable releases.
    tag_pattern = r"v(1\.[0-9]*[02468]\.[0-9]+)"
    source_url = "https://github.com/tailscale/tailscale/archive/refs/tags/{tag}.tar.gz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        env = go_environment(workspace)
        version = self.upstream_version
        ldflags = (f"-s -w -X tailscale.com/version.longStamp={version} "
                   f"-X tailscale.com/version.shortStamp={version}")
        for program, destination in (("tailscale", "usr/bin/tailscale"), ("tailscaled", "usr/sbin/tailscaled")):
            binary = workspace / program
            command(["go", "build", "-ldflags", ldflags, "-o", str(binary), f"./cmd/{program}"], cwd=source, env=env)
            install_file(binary, staging / destination, 0o755)
        install_file(source / "cmd/tailscaled/tailscaled.service",
                     staging / "usr/lib/systemd/system/tailscaled.service")
        install_file(source / "cmd/tailscaled/tailscaled.defaults", staging / "etc/default/tailscaled")
        debian = staging / "DEBIAN"
        debian.mkdir(parents=True, exist_ok=True)
        (debian / "conffiles").write_text("/etc/default/tailscaled\n", encoding="utf-8")
        for script, text in (("postinst", POSTINST), ("prerm", PRERM)):
            (debian / script).write_text(text, encoding="utf-8")
            (debian / script).chmod(0o755)
        install_license(staging, self.name, source, ["LICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(TailscaleRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
