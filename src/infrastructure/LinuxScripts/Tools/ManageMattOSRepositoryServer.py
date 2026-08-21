#!/usr/bin/env python3
"""Home-server entry point for the local MattOS repository manager."""

from __future__ import annotations

import sys
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIRECTORY = REPOSITORY_ROOT / "src"
if str(SOURCE_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SOURCE_DIRECTORY))

from server.mattos_repository import main


if __name__ == "__main__":
    raise SystemExit(main())
