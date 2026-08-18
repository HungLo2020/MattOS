#!/usr/bin/env python3
"""Dispatch Cargo with the MattOS source-ownership config for the current component."""
from __future__ import annotations

import json
import os
import pathlib
import subprocess
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


def metadata_resolution_args(original: list[str]) -> list[str]:
    """Return Cargo metadata flags that can affect dependency/lock resolution."""
    selected: list[str] = []
    value_flags = {"--manifest-path", "--features", "-F", "--package", "-p"}
    switch_flags = {"--all-features", "--no-default-features"}
    i = 0
    while i < len(original):
        arg = original[i]
        if arg in value_flags and i + 1 < len(original):
            selected.extend([arg, original[i + 1]])
            i += 2
            continue
        if any(arg.startswith(prefix + "=") for prefix in ("--manifest-path", "--features", "--package")):
            selected.append(arg)
            i += 1
            continue
        if arg in switch_flags:
            selected.append(arg)
        i += 1
    return selected


def reconcile_lockfile(real_cargo: str, config: pathlib.Path, original: list[str]) -> None:
    """Reconcile a build-mirror lockfile under the scoped ownership config.

    MattOS keeps imported upstream Cargo.lock files pristine in src/. Build
    mirrors copy those lockfiles, then source ownership changes Git package
    identities to MattOS-owned paths. Cargo must update the mirror lockfile once
    before the real --locked command can verify and use it.
    """
    if "--locked" not in original:
        return
    cwd = pathlib.Path.cwd()
    manifest = cwd / "Cargo.toml"
    lockfile = cwd / "Cargo.lock"
    if not manifest.is_file() or not lockfile.is_file():
        return

    command = [
        real_cargo,
        "--config",
        str(config),
        "metadata",
        "--format-version",
        "1",
        *metadata_resolution_args(original),
    ]
    completed = subprocess.run(
        command,
        cwd=str(cwd),
        stdout=subprocess.DEVNULL,
        check=False,
    )
    if completed.returncode != 0:
        raise SystemExit(completed.returncode)


def main() -> int:
    root = repo_root()
    real_cargo = os.environ.get("MATTOS_REAL_CARGO")
    if not real_cargo:
        raise SystemExit("MATTOS_REAL_CARGO is not set")

    index_path = root / "out" / "source-ownership" / "cargo" / "index.json"
    args = [real_cargo]
    scoped_config: pathlib.Path | None = None
    if index_path.is_file():
        index = json.loads(index_path.read_text(encoding="utf-8"))
        component = component_for_cwd(root, pathlib.Path.cwd(), index)
        if component is not None:
            config = index["components"][component].get("config")
            if config:
                scoped_config = (root / config).resolve()
                args += ["--config", str(scoped_config)]

    if scoped_config is not None:
        reconcile_lockfile(real_cargo, scoped_config, sys.argv[1:])

    args += sys.argv[1:]
    os.execv(real_cargo, args)
    return 127


if __name__ == "__main__":
    raise SystemExit(main())
