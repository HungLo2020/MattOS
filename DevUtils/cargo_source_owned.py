#!/usr/bin/env python3
"""Dispatch Cargo with the MattOS source-ownership config for the current component."""
from __future__ import annotations

import json
import os
import pathlib
import sys


def repo_root() -> pathlib.Path:
    value = os.environ.get("MATTOS_REPO_ROOT")
    if value:
        return pathlib.Path(value).resolve()
    here = pathlib.Path(__file__).resolve()
    return here.parents[1]


def component_for_cwd(root: pathlib.Path, cwd: pathlib.Path, index: dict) -> str | None:
    matches = []
    for name, metadata in index.get("components", {}).items():
        source = root / metadata["source_path"]
        try:
            relative = cwd.resolve().relative_to(source.resolve())
        except ValueError:
            continue
        matches.append((len(relative.parts), name))
    if matches:
        return min(matches)[1]

    parts = cwd.resolve().parts
    # Stable COSMIC mirrors: out/build/cosmic-desktop/sources/<component>/...
    marker = ("out", "build", "cosmic-desktop", "sources")
    for i in range(len(parts) - len(marker)):
        if tuple(parts[i : i + len(marker)]) == marker and i + len(marker) < len(parts):
            candidate = parts[i + len(marker)]
            if candidate in index.get("components", {}):
                return candidate

    # Generic component builds normally live under out/build/<component>/...
    for i in range(len(parts) - 2):
        if parts[i : i + 2] == ("out", "build"):
            candidate = parts[i + 2]
            if candidate in index.get("components", {}):
                return candidate
    return None


def main() -> int:
    root = repo_root()
    real_cargo = os.environ.get("MATTOS_REAL_CARGO")
    if not real_cargo:
        raise SystemExit("MATTOS_REAL_CARGO is not set")

    index_path = root / "out" / "source-ownership" / "cargo" / "index.json"
    args = [real_cargo]
    if index_path.is_file():
        index = json.loads(index_path.read_text(encoding="utf-8"))
        component = component_for_cwd(root, pathlib.Path.cwd(), index)
        if component is not None:
            config = index["components"][component].get("config")
            if config:
                args += ["--config", str((root / config).resolve())]
    args += sys.argv[1:]
    os.execv(real_cargo, args)
    return 127


if __name__ == "__main__":
    raise SystemExit(main())
