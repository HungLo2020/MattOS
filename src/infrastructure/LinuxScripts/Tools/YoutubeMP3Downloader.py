#!/usr/bin/env python3
"""Download audio from a YouTube URL and save it as an MP3.

On first run, the script creates a private virtual environment and installs
yt-dlp and a pip-managed ffmpeg binary. It does not use system/Apt packages.
"""

from __future__ import annotations

import subprocess
import sys
import venv
from argparse import ArgumentParser, Namespace
from pathlib import Path


# Change this path to use a different download location.
DOWNLOAD_DIRECTORY = Path.home() / "Downloads/Music"
RUNTIME_DIRECTORY = Path.home() / ".local" / "share" / "youtube-mp3-downloader"
RUNTIME_VENV = RUNTIME_DIRECTORY / "venv"


def runtime_python() -> Path:
    """Return the interpreter in the downloader's private virtual environment."""

    return RUNTIME_VENV / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")


def ensure_runtime() -> tuple[Path, str]:
    """Create a private runtime and return its Python and ffmpeg executable paths."""

    python = runtime_python()
    if not python.is_file():
        print(f"Setting up private downloader environment in {RUNTIME_DIRECTORY}...")
        RUNTIME_DIRECTORY.mkdir(parents=True, exist_ok=True)
        venv.create(RUNTIME_VENV, with_pip=True)

    dependencies_available = subprocess.run(
        [
            str(python),
            "-c",
            "import yt_dlp, imageio_ffmpeg",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    ).returncode == 0
    if not dependencies_available:
        subprocess.run(
            [
                str(python),
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "--upgrade",
                "yt-dlp",
                "imageio-ffmpeg",
            ],
            check=True,
        )
    ffmpeg = subprocess.check_output(
        [str(python), "-c", "import imageio_ffmpeg; print(imageio_ffmpeg.get_ffmpeg_exe())"],
        text=True,
    ).strip()
    return python, ffmpeg


def prompt_required(label: str) -> str:
    """Prompt until the user supplies a non-empty value."""

    while True:
        value = input(f"{label}: ").strip()
        if value:
            return value
        print(f"{label} cannot be empty.")


def safe_name(value: str, fallback: str) -> str:
    """Prevent names from escaping the configured download directory."""

    name = value.strip().replace("/", "-").replace("\\", "-")
    name = name.replace("..", ".")
    if name in {"", ".", ".."}:
        return fallback
    return name


def download_song(
    python: Path, ffmpeg: str, url: str, artist: str, song: str, allow_playlist: bool
) -> Path:
    """Download and convert a URL to the requested artist/song path."""

    artist_directory = DOWNLOAD_DIRECTORY / safe_name(artist, "Unknown Artist")
    song_name = safe_name(song, "Unknown Song")
    output_path = artist_directory / (
        f"{song_name} - %(title)s.%(ext)s" if allow_playlist else f"{song_name}.%(ext)s"
    )
    artist_directory.mkdir(parents=True, exist_ok=True)

    command = [
        str(python),
        "-m",
        "yt_dlp",
        "--extract-audio",
        "--audio-format",
        "mp3",
        "--audio-quality",
        "0",
        "--ffmpeg-location",
        ffmpeg,
        "--yes-playlist" if allow_playlist else "--no-playlist",
        "--no-overwrites",
        "--output",
        str(output_path),
        url,
    ]
    subprocess.run(command, check=True)
    return artist_directory / (
        f"{song_name}.mp3" if not allow_playlist else "(playlist files)"
    )


def parse_arguments() -> Namespace:
    """Parse command-line options controlling playlist behavior."""

    parser = ArgumentParser(description="Download YouTube audio as MP3 files.")
    parser.add_argument(
        "--playlist",
        action="store_true",
        help="explicitly allow downloading every item in a playlist or Mix",
    )
    return parser.parse_args()


def main() -> int:
    """Run the interactive downloader."""

    arguments = parse_arguments()

    print("YouTube MP3 Downloader")
    print(f"Download directory: {DOWNLOAD_DIRECTORY}")
    print()

    try:
        python, ffmpeg = ensure_runtime()
        url = prompt_required("URL")
        artist = prompt_required("Artist Name")
        song = prompt_required("Song Name")
        destination = download_song(python, ffmpeg, url, artist, song, arguments.playlist)
    except KeyboardInterrupt:
        print("\nDownload cancelled.")
        return 130
    except EOFError:
        print("\nInput cancelled.")
        return 1
    except subprocess.CalledProcessError as error:
        print(f"Downloader setup or download failed with exit code {error.returncode}.", file=sys.stderr)
        return error.returncode or 1
    except OSError as error:
        print(f"Unable to save the download: {error}", file=sys.stderr)
        return 1

    if arguments.playlist:
        print(f"Playlist saved under: {destination.parent}")
    else:
        print(f"Saved: {destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
