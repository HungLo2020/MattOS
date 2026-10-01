#!/usr/bin/env python3
"""Build QEMU's selected release (x86_64 system emulation) as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    command,
    install_license,
    run_recipe,
)

class QemuRecipe(SourceReleaseRecipe):
    name = "qemu-system-x86"
    repository = "mattos"
    section = "otherosfs"
    description = "QEMU x86_64 system emulator with KVM, SDL display and user-mode networking"
    depends = ("libc6",)
    # qemu-img and the other QEMU utilities are part of this package.
    provides = ("qemu-utils", "qemu-system-common")
    build_depends = ("libslirp0", "libsdl2-2.0-0")
    git_url = "https://gitlab.com/qemu-project/qemu.git"
    tag_pattern = r"v([0-9]+\.[0-9]+\.[0-9]+)"
    source_url = "https://download.qemu.org/qemu-{version}.tar.xz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        build = workspace / "build"
        build.mkdir(parents=True, exist_ok=True)
        # configure builds a Python venv and lets pip fetch its build-time
        # modules (setuptools, qemu.qmp); the source already bundles its
        # Meson subprojects.
        command([
            str(source / "configure"), "--prefix=/usr", "--libexecdir=/usr/libexec",
            "--sysconfdir=/etc", "--localstatedir=/var", "--target-list=x86_64-softmmu",
            "--enable-download", "--disable-docs", "--disable-werror",
            "--enable-kvm", "--enable-slirp", "--enable-sdl", "--disable-gtk", "--disable-opengl",
            "--disable-spice", "--disable-xen", "--disable-gnutls", "--disable-vnc-sasl",
        ], cwd=build)
        command(["make", "-j", "4"], cwd=build)
        command(["make", f"DESTDIR={staging}", "install"], cwd=build)
        install_license(staging, self.name, source, ["COPYING", "LICENSE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(QemuRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
