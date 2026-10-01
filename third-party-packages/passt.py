#!/usr/bin/env python3
"""Build passt/pasta's selected snapshot release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    command,
    git_latest_tag,
    install_license,
    run_recipe,
)

class PasstRecipe(SourceReleaseRecipe):
    name = "passt"
    repository = "mattos"
    section = "net"
    description = "User-mode networking for virtual machines and namespaces (passt and pasta)"
    depends = ("libc6",)
    git_url = "https://passt.top/passt"
    source_url = "https://passt.top/passt/snapshot/passt-{tag}.tar.xz"

    def discover_version(self) -> tuple[str, dict[str, str]]:
        # Releases are tagged <date>.<commit>; Debian versions them
        # 0.0~git<date>.<commit>.
        _, tag = git_latest_tag(self.git_url, r"([0-9]{4}_[0-9]{2}_[0-9]{2}\.[0-9a-f]+)")
        date, commit = tag.split(".", 1)
        return f"0.0~git{date.replace('_', '')}.{commit}", {"upstream": self.git_url, "release_tag": tag}

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        tag = self.upstream_version.split("~git", 1)[-1]
        options = [f"VERSION={tag}", "prefix=/usr"]
        command(["make", "-j4", *options], cwd=source)
        command(["make", *options, f"DESTDIR={staging}", "install"], cwd=source)
        install_license(staging, self.name, source, ["LICENSES/GPL-2.0-or-later.txt"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(PasstRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
