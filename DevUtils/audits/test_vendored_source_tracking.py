#!/usr/bin/env python3
"""Fast, network-free check that vendored source is fully tracked by Git.

Nested upstream ``.gitignore`` files remain active inside vendored trees, so a
file upstream tracks can exist locally yet be silently missing from a fresh
clone (and from build source mirrors that enumerate the Git index). Builds
never write into vendored trees, so any ignored path there is either an
untracked upstream file or residue; both must be resolved. Gitlinks in the
MattOS index likewise check out as empty directories.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = Path(__file__).with_name("test_imported_source_immutability.py")
SPEC = importlib.util.spec_from_file_location("imported_source_immutability", MODULE_PATH)
assert SPEC and SPEC.loader
IMMUTABILITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(IMMUTABILITY)

# Interpreter caches from running vendored helper scripts in place are local
# noise, not source; they are never copied into build mirrors.
TOLERATED_PARTS = frozenset({"__pycache__"})


def untracked_vendored_paths(repository: Path) -> list[str]:
    return sorted(
        path
        for path in IMMUTABILITY.ignored_untracked_paths(repository)
        if not TOLERATED_PARTS.intersection(Path(path).parts)
    )


def index_gitlinks(repository: Path) -> list[str]:
    raw = subprocess.run(
        ["git", "ls-files", "-s", "-z"],
        cwd=repository,
        stdout=subprocess.PIPE,
        check=True,
    ).stdout
    return sorted(
        record.split(b"\t", 1)[1].decode("utf-8", "surrogateescape")
        for record in raw.split(b"\0")
        if record.startswith(b"160000 ")
    )


class VendoredSourceTrackingTests(unittest.TestCase):
    def test_no_ignored_untracked_paths_in_vendored_trees(self) -> None:
        paths = untracked_vendored_paths(ROOT)
        self.assertEqual(
            paths,
            [],
            "vendored paths are present but ignored by Git; force-add upstream "
            "files (git add -f) or remove residue:\n" + "\n".join(paths[:50]),
        )

    def test_index_contains_no_gitlinks(self) -> None:
        self.assertEqual(index_gitlinks(ROOT), [])

    def test_root_gitignore_does_not_hide_license_files(self) -> None:
        completed = subprocess.run(
            ["git", "check-ignore", "-q", "--no-index", "src/example/LICENSE"],
            cwd=ROOT,
            check=False,
        )
        self.assertEqual(completed.returncode, 1, "root .gitignore ignores LICENSE files")


if __name__ == "__main__":
    unittest.main()
