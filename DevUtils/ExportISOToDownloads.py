#!/usr/bin/env python3

from pathlib import Path
import shutil


PROJECT_ROOT = Path(__file__).resolve().parent.parent
ISO_PATH = PROJECT_ROOT / "out/images/mattos-x86_64.iso"
DOWNLOADS_PATH = Path.home() / "Downloads" / ISO_PATH.name


def main() -> None:
    if not ISO_PATH.is_file():
        raise SystemExit(f"ISO not found: {ISO_PATH}\nBuild it first, then run this script again.")

    DOWNLOADS_PATH.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ISO_PATH, DOWNLOADS_PATH)
    print(f"Copied {ISO_PATH} to {DOWNLOADS_PATH}")


if __name__ == "__main__":
    main()
