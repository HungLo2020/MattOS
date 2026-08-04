#!/usr/bin/env python3
"""Interactive YouTube-to-MP3 downloader organized by artist folders."""

from __future__ import annotations

import os
import re
import shutil
import site
import subprocess
import sys
import unicodedata
from pathlib import Path
from typing import Optional
from urllib.parse import urlparse


WINDOWS_RESERVED_NAMES = {
    "CON",
    "PRN",
    "AUX",
    "NUL",
    *(f"COM{i}" for i in range(1, 10)),
    *(f"LPT{i}" for i in range(1, 10)),
}

YOUTUBE_HOSTS = {
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtu.be",
    "www.youtu.be",
    "youtube-nocookie.com",
    "www.youtube-nocookie.com",
}


def managed_venv_dir() -> Path:
    return Path.home() / ".yt-music-downloader" / "venv"


def managed_venv_python() -> Path:
    if os.name == "nt":
        return managed_venv_dir() / "Scripts" / "python.exe"
    return managed_venv_dir() / "bin" / "python"


def print_header() -> None:
    print("YouTube Music Downloader")
    print("========================")
    print("Downloads a YouTube URL as an MP3 into an artist folder.")
    print()


def prompt_required(prompt: str) -> str:
    while True:
        value = input(prompt).strip()
        if value:
            return value
        print("Please enter a value.")


def prompt_yes_no(prompt: str, default: bool = False) -> bool:
    suffix = "Y/n" if default else "y/N"
    while True:
        value = input(f"{prompt} ({suffix}): ").strip().lower()
        if not value:
            return default
        if value in {"y", "yes"}:
            return True
        if value in {"n", "no"}:
            return False
        print("Please enter y or n.")


def normalize_key(value: str) -> str:
    normalized = unicodedata.normalize("NFC", value)
    return normalized.casefold()


def sanitize_path_component(value: str, fallback: str) -> str:
    """Return a single directory/file name that is valid on Windows and Linux."""
    cleaned = unicodedata.normalize("NFC", value).strip()
    cleaned = re.sub(r'[<>:"/\\|?*\x00-\x1f]', "_", cleaned)
    cleaned = re.sub(r"\s+", " ", cleaned).strip(" .")
    cleaned = re.sub(r"_+", "_", cleaned)

    if not cleaned:
        cleaned = fallback

    stem = cleaned.split(".", 1)[0].upper()
    if stem in WINDOWS_RESERVED_NAMES:
        cleaned = f"{cleaned}_"

    return cleaned


def validate_youtube_url(url: str) -> str:
    parsed = urlparse(url)
    if parsed.scheme not in {"http", "https"}:
        raise ValueError("URL must start with http:// or https://.")

    host = parsed.netloc.lower()
    if ":" in host:
        host = host.split(":", 1)[0]

    if host not in YOUTUBE_HOSTS:
        raise ValueError("URL must be from youtube.com, music.youtube.com, or youtu.be.")

    return url


def resolve_existing_dir(prompt: str) -> Path:
    while True:
        raw_path = prompt_required(prompt)
        expanded = os.path.expandvars(os.path.expanduser(raw_path))
        directory = Path(expanded).resolve()

        if directory.exists() and directory.is_dir():
            return directory

        if directory.exists():
            print(f"Path exists but is not a directory: {directory}")
            continue

        if prompt_yes_no(f"Directory does not exist: {directory}\nCreate it"):
            try:
                directory.mkdir(parents=True, exist_ok=True)
                return directory
            except OSError as exc:
                print(f"Could not create directory: {exc}")
        else:
            print("Choose an existing directory or allow the script to create it.")


def choose_from_matches(matches: list[Path], artist_dir_name: str) -> Optional[Path]:
    if len(matches) == 1:
        return matches[0]

    print()
    print("Multiple matching artist folders were found:")
    for index, path in enumerate(matches, start=1):
        print(f"  {index}. {path.name}")
    print(f"  {len(matches) + 1}. Create new folder: {artist_dir_name}")

    while True:
        choice = input("Choose a folder number: ").strip()
        if not choice.isdigit():
            print("Please enter a number.")
            continue

        index = int(choice)
        if 1 <= index <= len(matches):
            return matches[index - 1]
        if index == len(matches) + 1:
            return None
        print("Choice is out of range.")


def resolve_artist_dir(root_dir: Path, artist_name: str) -> Path:
    safe_artist_name = sanitize_path_component(artist_name, "Unknown Artist")

    exact_matches: list[Path] = []
    fuzzy_matches: list[Path] = []
    raw_key = normalize_key(artist_name)
    safe_key = normalize_key(safe_artist_name)

    for child in root_dir.iterdir():
        if not child.is_dir():
            continue
        if child.name == artist_name or child.name == safe_artist_name:
            exact_matches.append(child)
            continue
        child_key = normalize_key(child.name)
        if child_key in {raw_key, safe_key}:
            fuzzy_matches.append(child)

    if exact_matches:
        match = choose_from_matches(sorted(exact_matches), safe_artist_name)
        if match is not None:
            return match

    if fuzzy_matches:
        match = choose_from_matches(sorted(fuzzy_matches), safe_artist_name)
        if match is not None:
            return match

    artist_dir = root_dir / safe_artist_name
    artist_dir.mkdir(parents=True, exist_ok=True)
    return artist_dir


def resolve_yt_dlp_command() -> Optional[list[str]]:
    add_common_user_bin_paths()

    executable = shutil.which("yt-dlp")
    if executable:
        return [executable]

    venv_python = managed_venv_python()
    if venv_python.exists() and python_module_available("yt_dlp", python_executable=venv_python):
        return [str(venv_python), "-m", "yt_dlp"]

    try:
        subprocess.run(
            [sys.executable, "-m", "yt_dlp", "--version"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=True,
            timeout=15,
        )
    except (OSError, subprocess.SubprocessError):
        return None

    return [sys.executable, "-m", "yt_dlp"]


def check_dependencies() -> list[str]:
    missing: list[str] = []

    if resolve_yt_dlp_command() is None:
        missing.append("yt-dlp")

    if resolve_ffmpeg_path() is None:
        missing.append("ffmpeg")

    return missing


def refresh_windows_path_from_registry() -> None:
    if os.name != "nt":
        return

    try:
        import winreg
    except ImportError:
        return

    path_values = []
    registry_paths = [
        (winreg.HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"),
        (winreg.HKEY_CURRENT_USER, r"Environment"),
    ]

    for root, subkey in registry_paths:
        try:
            with winreg.OpenKey(root, subkey) as key:
                value, _ = winreg.QueryValueEx(key, "Path")
                path_values.append(os.path.expandvars(value))
        except OSError:
            continue

    current_paths = os.environ.get("PATH", "").split(os.pathsep)
    for value in path_values:
        for path in value.split(os.pathsep):
            if path and path not in current_paths:
                current_paths.append(path)

    os.environ["PATH"] = os.pathsep.join(current_paths)


def find_winget_ffmpeg() -> Optional[Path]:
    if os.name != "nt":
        return None

    local_app_data = os.environ.get("LOCALAPPDATA")
    if not local_app_data:
        return None

    packages_dir = Path(local_app_data) / "Microsoft" / "WinGet" / "Packages"
    if not packages_dir.exists():
        return None

    matches = sorted(packages_dir.glob("Gyan.FFmpeg_*/*/bin/ffmpeg.exe"), reverse=True)
    for match in matches:
        if match.is_file():
            return match

    return None


def resolve_ffmpeg_path() -> Optional[Path]:
    add_common_user_bin_paths()
    refresh_windows_path_from_registry()

    executable = shutil.which("ffmpeg")
    if executable:
        return Path(executable)

    winget_ffmpeg = find_winget_ffmpeg()
    if winget_ffmpeg:
        return winget_ffmpeg

    return None


def add_common_user_bin_paths() -> None:
    refresh_windows_path_from_registry()

    paths = os.environ.get("PATH", "").split(os.pathsep)
    candidates = []

    user_base = Path(site.getuserbase())
    if os.name == "nt":
        candidates.append(user_base / "Scripts")
        candidates.append(Path(sys.executable).resolve().parent / "Scripts")
        local_app_data = os.environ.get("LOCALAPPDATA")
        if local_app_data:
            candidates.append(Path(local_app_data) / "Microsoft" / "WindowsApps")
            candidates.append(Path(local_app_data) / "Microsoft" / "WinGet" / "Links")
            winget_ffmpeg = find_winget_ffmpeg()
            if winget_ffmpeg:
                candidates.append(winget_ffmpeg.parent)
    else:
        candidates.append(user_base / "bin")
        candidates.append(Path.home() / ".local" / "bin")

    for candidate in candidates:
        candidate_str = str(candidate)
        if candidate.exists() and candidate_str not in paths:
            paths.insert(0, candidate_str)

    os.environ["PATH"] = os.pathsep.join(paths)


def print_dependency_help(missing: list[str]) -> None:
    print("Missing required dependency/dependencies:")
    for name in missing:
        print(f"  - {name}")
    print()
    print("Install suggestions:")
    if "yt-dlp" in missing:
        print("  yt-dlp: python -m pip install --user yt-dlp")
        print("          or: pipx install yt-dlp")
    if "ffmpeg" in missing:
        print("  Linux ffmpeg: sudo apt install ffmpeg")
        print("  Windows ffmpeg: winget install Gyan.FFmpeg")
    print()
    print("After installing, make sure the commands are available in PATH and rerun this script.")


def run_install_command(command: list[str], label: str) -> bool:
    print()
    print(f"Running: {label}")
    try:
        subprocess.run(command, check=True)
    except (OSError, subprocess.CalledProcessError) as exc:
        print(f"Install step failed: {exc}")
        return False
    return True


def python_module_available(module_name: str, python_executable: Optional[Path] = None) -> bool:
    executable = str(python_executable or sys.executable)
    try:
        subprocess.run(
            [executable, "-m", module_name, "--version"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=True,
            timeout=15,
        )
    except (OSError, subprocess.SubprocessError):
        return False
    return True


def ensure_pip() -> bool:
    if python_module_available("pip"):
        return True

    print("pip is not available for this Python. Trying ensurepip...")
    if not run_install_command([sys.executable, "-m", "ensurepip", "--upgrade"], "bootstrap pip"):
        return False

    return python_module_available("pip")


def install_yt_dlp_to_managed_venv() -> bool:
    venv_dir = managed_venv_dir()
    venv_python = managed_venv_python()

    if not venv_python.exists():
        if not run_install_command(
            [sys.executable, "-m", "venv", str(venv_dir)],
            f"create managed Python venv at {venv_dir}",
        ):
            return False

    if not run_install_command(
        [str(venv_python), "-m", "pip", "install", "--upgrade", "pip", "yt-dlp"],
        "install yt-dlp in managed venv",
    ):
        return False

    return resolve_yt_dlp_command() is not None


def install_yt_dlp() -> bool:
    if ensure_pip():
        if run_install_command(
            [sys.executable, "-m", "pip", "install", "--user", "--upgrade", "yt-dlp"],
            "install yt-dlp with pip",
        ):
            add_common_user_bin_paths()
            if resolve_yt_dlp_command() is not None:
                return True

    if install_yt_dlp_to_managed_venv():
        return True

    if shutil.which("pipx"):
        if run_install_command(["pipx", "install", "yt-dlp"], "install yt-dlp with pipx"):
            add_common_user_bin_paths()
            if resolve_yt_dlp_command() is not None:
                return True
        if run_install_command(["pipx", "upgrade", "yt-dlp"], "upgrade yt-dlp with pipx"):
            add_common_user_bin_paths()
            if resolve_yt_dlp_command() is not None:
                return True

    return resolve_yt_dlp_command() is not None


def with_sudo(command: list[str]) -> list[str]:
    if os.name == "nt" or hasattr(os, "geteuid") and os.geteuid() == 0:
        return command
    sudo = shutil.which("sudo")
    if sudo:
        return [sudo, *command]
    return command


def install_ffmpeg_windows() -> bool:
    winget = shutil.which("winget")
    if winget and run_install_command(
        [
            winget,
            "install",
            "--id",
            "Gyan.FFmpeg",
            "-e",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ],
        "install ffmpeg with winget",
    ):
        add_common_user_bin_paths()
        return resolve_ffmpeg_path() is not None

    choco = shutil.which("choco")
    if choco and run_install_command([choco, "install", "ffmpeg", "-y"], "install ffmpeg with Chocolatey"):
        add_common_user_bin_paths()
        return resolve_ffmpeg_path() is not None

    scoop = shutil.which("scoop")
    if scoop and run_install_command([scoop, "install", "ffmpeg"], "install ffmpeg with Scoop"):
        add_common_user_bin_paths()
        return resolve_ffmpeg_path() is not None

    return False


def install_ffmpeg_linux() -> bool:
    package_commands = [
        ("apt-get", [["apt-get", "update"], ["apt-get", "install", "-y", "ffmpeg"]]),
        ("dnf", [["dnf", "install", "-y", "ffmpeg"]]),
        ("pacman", [["pacman", "-S", "--needed", "--noconfirm", "ffmpeg"]]),
        ("zypper", [["zypper", "install", "-y", "ffmpeg"]]),
        ("apk", [["apk", "add", "ffmpeg"]]),
    ]

    for package_manager, commands in package_commands:
        if not shutil.which(package_manager):
            continue

        for command in commands:
            if not run_install_command(with_sudo(command), " ".join(command)):
                return False
        return resolve_ffmpeg_path() is not None

    return False


def install_ffmpeg() -> bool:
    if os.name == "nt":
        return install_ffmpeg_windows()
    if sys.platform.startswith("linux"):
        return install_ffmpeg_linux()
    return False


def install_missing_dependencies(missing: list[str]) -> list[str]:
    if "yt-dlp" in missing:
        install_yt_dlp()
    if "ffmpeg" in missing:
        install_ffmpeg()

    add_common_user_bin_paths()
    return check_dependencies()


def ensure_dependencies() -> bool:
    missing = check_dependencies()
    if not missing:
        return True

    print_dependency_help(missing)
    if not prompt_yes_no("Install missing dependencies now", default=True):
        return False

    remaining = install_missing_dependencies(missing)
    if not remaining:
        print()
        print("Dependencies installed successfully.")
        return True

    print()
    print("Some dependencies are still missing after the install attempt.")
    print_dependency_help(remaining)
    return False


def build_output_template() -> str:
    return "%(title).180B.%(ext)s"


def run_download(url: str, artist_dir: Path) -> None:
    yt_dlp_command = resolve_yt_dlp_command()
    if yt_dlp_command is None:
        raise RuntimeError("yt-dlp was available earlier but is no longer resolvable.")

    ffmpeg_path = resolve_ffmpeg_path()
    if ffmpeg_path is None:
        raise RuntimeError("ffmpeg was available earlier but is no longer resolvable.")

    command = [
        *yt_dlp_command,
        "--extract-audio",
        "--audio-format",
        "mp3",
        "--audio-quality",
        "0",
        "--no-playlist",
        "--restrict-filenames",
        "--windows-filenames",
        "--no-overwrites",
        "--continue",
        "--paths",
        str(artist_dir),
        "--ffmpeg-location",
        str(ffmpeg_path.parent),
        "--output",
        build_output_template(),
        url,
    ]

    print()
    print(f"Saving MP3 to: {artist_dir}")
    subprocess.run(command, check=True)


def collect_inputs() -> tuple[str, str, Path]:
    while True:
        raw_url = prompt_required("YouTube URL: ")
        try:
            url = validate_youtube_url(raw_url)
            break
        except ValueError as exc:
            print(f"Invalid YouTube URL: {exc}")

    artist_name = prompt_required("Artist name: ")
    root_dir = resolve_existing_dir("Music library directory: ")
    return url, artist_name, root_dir


def main() -> int:
    print_header()

    if not ensure_dependencies():
        return 1

    try:
        url, artist_name, root_dir = collect_inputs()
        artist_dir = resolve_artist_dir(root_dir, artist_name)
        run_download(url, artist_dir)
    except KeyboardInterrupt:
        print()
        print("Canceled.")
        return 130
    except subprocess.CalledProcessError as exc:
        print()
        print(f"Download failed with exit code {exc.returncode}.")
        return exc.returncode or 1
    except OSError as exc:
        print()
        print(f"Filesystem or process error: {exc}")
        return 1
    except RuntimeError as exc:
        print()
        print(f"Error: {exc}")
        return 1

    print()
    print("Done.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
