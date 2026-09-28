#!/usr/bin/env python3
"""Serve, build, or set up the MattOS wiki (MkDocs Material, sources in docs/).

    python3 DevUtils/RunWiki.py          # live preview on http://127.0.0.1:8000
    python3 DevUtils/RunWiki.py build    # strict build into site/, as CI does
    python3 DevUtils/RunWiki.py setup    # (re)create the .venv-wiki environment
"""

from __future__ import annotations

import argparse
import os
import subprocess
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
VENV = REPO_ROOT / ".venv-wiki"
VENV_PYTHON = VENV / "bin" / "python"
REQUIREMENTS = REPO_ROOT / "requirements-docs.txt"


def run_checked(command: list[str]) -> None:
    subprocess.run(command, cwd=REPO_ROOT, check=True)


def install_wiki_environment() -> None:
    python = os.environ.get("PYTHON", "python3")
    print(f"Preparing wiki environment at {VENV}")
    run_checked([python, "-m", "venv", str(VENV)])
    run_checked([str(VENV_PYTHON), "-m", "pip", "install", "--upgrade", "pip"])
    run_checked([str(VENV_PYTHON), "-m", "pip", "install", "-r", str(REQUIREMENTS)])


def ensure_wiki_environment() -> None:
    if VENV_PYTHON.is_file():
        probe = subprocess.run(
            [str(VENV_PYTHON), "-m", "mkdocs", "--version"],
            cwd=REPO_ROOT,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if probe.returncode == 0:
            return
    install_wiki_environment()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "command",
        nargs="?",
        default="serve",
        choices=("serve", "build", "setup"),
        help="command to run; default: serve",
    )
    args = parser.parse_args()
    if args.command == "setup":
        install_wiki_environment()
        print("Wiki environment is ready.")
    elif args.command == "serve":
        ensure_wiki_environment()
        run_checked([str(VENV_PYTHON), "-m", "mkdocs", "serve"])
    else:
        run_checked([os.environ.get("PYTHON", "python3"), str(REPO_ROOT / "DevUtils" / "check_wiki_structure.py")])
        ensure_wiki_environment()
        run_checked([str(VENV_PYTHON), "-m", "mkdocs", "build", "--strict"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
