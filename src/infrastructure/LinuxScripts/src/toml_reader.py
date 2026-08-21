"""TOML loading compatible with Python 3.10 and newer."""

from __future__ import annotations

from pathlib import Path
from typing import Any


def load_toml(path: Path) -> dict[str, Any]:
    """Load a TOML document, using tomli only on Python versions before 3.11."""

    try:
        import tomllib
    except ModuleNotFoundError:
        try:
            import tomli as tomllib
        except ModuleNotFoundError as error:
            raise RuntimeError(
                "TOML support requires Python 3.11 or newer, or the 'tomli' package on Python 3.10."
            ) from error

    with path.open("rb") as file:
        return tomllib.load(file)