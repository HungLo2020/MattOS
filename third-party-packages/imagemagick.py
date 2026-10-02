#!/usr/bin/env python3
"""Build ImageMagick's selected release as a native MattOS .deb."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (  # noqa: E402
    RecipeError,
    SourceReleaseRecipe,
    git_latest_tag,
    install_license,
    run_recipe,
)

class ImageMagickRecipe(SourceReleaseRecipe):
    name = "imagemagick"
    repository = "mattos"
    section = "graphics"
    description = "Image manipulation programs and libraries (ImageMagick 7: magick, convert, identify, ...)"
    depends = ("libc6",)
    # The image formats beyond ImageMagick's built-in coders.
    build_depends = ("libjpeg62-turbo", "libwebp7", "libtiff6", "libopenjp2-7")
    github = ("ImageMagick", "ImageMagick")
    source_url = "https://github.com/ImageMagick/ImageMagick/archive/refs/tags/{tag}.tar.gz"
    # Coders are built into libMagickCore rather than loaded as modules, and
    # only formats whose libraries MattOS or a third-party recipe provides are
    # enabled.  No X11 display, no Perl bindings.
    build_options = (
        "--disable-static", "--without-modules", "--disable-docs", "--without-x",
        "--without-perl", "--with-png", "--with-jpeg", "--with-tiff", "--with-webp",
        "--with-openjp2", "--with-lcms", "--with-freetype", "--with-fontconfig", "--with-xml",
        "--with-zlib", "--with-zstd", "--with-lzma", "--with-bzlib",
        "--without-heic", "--without-raw", "--without-gslib", "--without-rsvg", "--without-djvu",
        "--without-wmf", "--without-fftw", "--without-lqr", "--without-jxl", "--without-openexr",
        "--without-jbig", "--without-raqm", "--without-gvc", "--without-pango", "--without-uhdr",
        "--without-dps", "--without-fpx", "--without-flif", "--without-zip",
    )

    def discover_version(self) -> tuple[str, dict[str, str]]:
        # Release tags look like 7.1.2-8; the package version uses dots.
        tag_version, tag = git_latest_tag("https://github.com/ImageMagick/ImageMagick.git",
                                          r"(7\.[0-9]+\.[0-9]+-[0-9]+)")
        return tag_version.replace("-", "."), {"upstream": "https://github.com/ImageMagick/ImageMagick",
                                               "release_tag": tag}

    def post_install(self, staging: Path, source: Path) -> None:
        for archive in staging.rglob("*.la"):
            archive.unlink()
        configuration = sorted(path for path in (staging / "etc").rglob("*") if path.is_file())
        if configuration:
            debian = staging / "DEBIAN"
            debian.mkdir(parents=True, exist_ok=True)
            (debian / "conffiles").write_text(
                "".join(f"/{path.relative_to(staging).as_posix()}\n" for path in configuration), encoding="utf-8")
        install_license(staging, self.name, source, ["LICENSE", "NOTICE"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(ImageMagickRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
