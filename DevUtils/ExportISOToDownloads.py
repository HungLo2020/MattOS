#!/usr/bin/env python3

from pathlib import Path
import os
import shutil
import tempfile


PROJECT_ROOT = Path(__file__).resolve().parent.parent
ISO_PATH = PROJECT_ROOT / "out/images/mattos-x86_64.iso"
DOWNLOADS_PATH = Path.home() / "Downloads" / ISO_PATH.name


def main() -> None:
    if not ISO_PATH.is_file():
        raise SystemExit(f"ISO not found: {ISO_PATH}\nBuild it first, then run this script again.")

    DOWNLOADS_PATH.parent.mkdir(parents=True, exist_ok=True)
    temporary_path: Path | None = None
    try:
        # Copy beside the destination and replace it atomically.  This allows
        # replacing an old ISO whose file permissions are not writable by the
        # current user, provided the Downloads directory is writable.
        with tempfile.NamedTemporaryFile(
            dir=DOWNLOADS_PATH.parent,
            prefix=f".{DOWNLOADS_PATH.name}.",
            suffix=".tmp",
            delete=False,
        ) as temporary_file:
            temporary_path = Path(temporary_file.name)

        shutil.copy2(ISO_PATH, temporary_path)
        os.replace(temporary_path, DOWNLOADS_PATH)
        temporary_path = None
    except PermissionError as error:
        raise SystemExit(
            f"Cannot replace {DOWNLOADS_PATH}: {error}\n"
            f"Ensure {DOWNLOADS_PATH.parent} is writable by the current user."
        ) from error
    finally:
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)

    print(f"Exported {ISO_PATH} to {DOWNLOADS_PATH}")


if __name__ == "__main__":
    main()
