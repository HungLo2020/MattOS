#!/usr/bin/env python3
"""Run a command and prove every imported source tree remains unchanged."""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import stat
import subprocess
import sys
import tomllib


def imported_paths(repository: Path) -> list[Path]:
    manifest = repository / "upstream" / "sources.toml"
    with manifest.open("rb") as stream:
        data = tomllib.load(stream)
    paths = [repository / component["path"] for component in data["component"]]
    missing = [path for path in paths if not path.is_dir()]
    if missing:
        rendered = ", ".join(str(path.relative_to(repository)) for path in missing)
        raise RuntimeError(f"imported source directories are missing: {rendered}")
    return sorted(paths)


def file_digest(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(1024 * 1024):
            digest.update(block)
    return digest.hexdigest()


def snapshot(repository: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for root in imported_paths(repository):
        for current, directory_names, file_names in os.walk(root, followlinks=False):
            directory_names.sort()
            file_names.sort()
            current_path = Path(current)
            entries = [current_path]
            entries.extend(current_path / name for name in directory_names)
            entries.extend(current_path / name for name in file_names)
            for path in entries:
                relative = path.relative_to(repository).as_posix()
                metadata = path.lstat()
                mode = stat.S_IMODE(metadata.st_mode)
                if path.is_symlink():
                    value = f"symlink:{mode:o}:{os.readlink(path)}"
                elif path.is_dir():
                    value = f"directory:{mode:o}"
                elif path.is_file():
                    value = f"file:{mode:o}:{metadata.st_size}:{file_digest(path)}"
                else:
                    value = f"other:{mode:o}:{metadata.st_mode}"
                result[relative] = value
    return result


def report_changes(before: dict[str, str], after: dict[str, str]) -> None:
    paths = sorted(set(before) | set(after))
    changes = []
    for path in paths:
        if path not in before:
            changes.append(f"created: {path}")
        elif path not in after:
            changes.append(f"removed: {path}")
        elif before[path] != after[path]:
            changes.append(f"changed: {path}")
    for change in changes[:200]:
        print(change, file=sys.stderr)
    if len(changes) > 200:
        print(f"... and {len(changes) - 200} more change(s)", file=sys.stderr)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Assert that a command leaves every imported source tree byte-for-byte unchanged."
    )
    parser.add_argument("command", nargs=argparse.REMAINDER)
    arguments = parser.parse_args()
    command = arguments.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        parser.error("a command is required after --")

    repository = Path(__file__).resolve().parents[1]
    print("snapshotting imported source trees before command...", flush=True)
    before = snapshot(repository)
    environment = os.environ.copy()
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    completed = subprocess.run(command, cwd=repository, env=environment, check=False)
    print("snapshotting imported source trees after command...", flush=True)
    after = snapshot(repository)
    if before != after:
        report_changes(before, after)
        print("imported source immutability check failed", file=sys.stderr)
        return 120 if completed.returncode == 0 else completed.returncode
    print(
        f"imported source immutability check passed: {len(after)} paths unchanged",
        flush=True,
    )
    return completed.returncode


if __name__ == "__main__":
    raise SystemExit(main())
