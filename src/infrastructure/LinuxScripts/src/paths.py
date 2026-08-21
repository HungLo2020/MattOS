"""Repository and user-path helpers shared by Python entrypoints."""

from __future__ import annotations

from pathlib import Path


def find_repository_root(start: Path) -> Path:
    """Find the project root using the source/resources layout."""

    resolved_start = start.resolve()
    for candidate in (resolved_start, *resolved_start.parents):
        if (candidate / "src").is_dir() and (candidate / "resources").is_dir():
            return candidate
    raise RuntimeError(f"Could not find repository root from {resolved_start}")


def profile_directory(repository_root: Path) -> Path:
    """Return the resource directory containing exported Konsave profiles."""

    return repository_root / "resources" / "KDEProfiles"