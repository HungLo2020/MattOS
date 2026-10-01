#!/usr/bin/env python3
"""Build SDL2's selected release as a native MattOS .deb."""

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

class Sdl2Recipe(SourceReleaseRecipe):
    name = "libsdl2-2.0-0"
    repository = "mattos"
    section = "libs"
    description = "Simple DirectMedia Layer 2 for Wayland (library and development files)"
    depends = ("libc6",)
    provides = ("libsdl2-dev",)
    github = ("libsdl-org", "SDL")
    tag_pattern = r"release-(2\.[0-9]+\.[0-9]+)"
    source_url = "https://github.com/libsdl-org/SDL/releases/download/{tag}/SDL2-{version}.tar.gz"
    build_system = "cmake"
    # Video through Wayland and EGL (loaded at run time); MattOS has no X11
    # development stack, and the audio backends are left out.
    build_options = (
        "-DSDL_STATIC=OFF", "-DSDL_TEST=OFF", "-DSDL_TESTS=OFF",
        "-DSDL_WAYLAND=ON", "-DSDL_WAYLAND_LIBDECOR=OFF", "-DSDL_X11=OFF", "-DSDL_KMSDRM=OFF",
        "-DSDL_OPENGL=ON", "-DSDL_OPENGLES=ON", "-DSDL_VULKAN=OFF",
        "-DSDL_PIPEWIRE=OFF", "-DSDL_PULSEAUDIO=OFF", "-DSDL_ALSA=OFF", "-DSDL_JACK=OFF",
        "-DSDL_SNDIO=OFF", "-DSDL_DBUS=OFF", "-DSDL_IBUS=OFF", "-DSDL_HIDAPI=OFF",
    )

    def post_install(self, staging: Path, source: Path) -> None:
        install_license(staging, self.name, source, ["LICENSE.txt"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(Sdl2Recipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
