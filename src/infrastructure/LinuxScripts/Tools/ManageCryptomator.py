#!/usr/bin/env python3
"""Standalone entry point for the Cryptomator vault manager."""

from __future__ import annotations

import sys
from pathlib import Path


SOURCE_DIRECTORY = Path(__file__).resolve().parents[1] / "src"
sys.path.insert(0, str(SOURCE_DIRECTORY))

from server.cryptomator import main


if __name__ == "__main__":
    raise SystemExit(main())
